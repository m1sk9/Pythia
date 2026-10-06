//! OpenRouter chat-completions client.
//!
//! Network calls are thin wrappers; request building, response parsing, and the
//! retry policy are pure functions so they can be tested without a server.

use super::{
    ChatRequest, ChatResponse, Citation, Content, Finish, LlmError, ModelCapabilities, Part, Role,
    ServerTool, Usage, WebFetchEngine, WebSearchEngine,
};
use anyhow::Context as _;
use bytes::Bytes;
use reqwest::{
    StatusCode,
    header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, RETRY_AFTER},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, time::Duration};

const CHAT_COMPLETIONS_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const MODELS_URL: &str = "https://openrouter.ai/api/v1/models";
const REFERER: &str = "https://github.com/m1sk9/Pythia";
const TITLE: &str = "Pythia";

/// Model id suffixes that OpenRouter accepts but does not list in `/models`.
const ROUTING_VARIANTS: [&str; 4] = [":nitro", ":floor", ":exacto", ":online"];

/// Delays before the first and subsequent retries.
const RETRY_DELAYS: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(3)];
/// `Retry-After` values above this fall back to [`RETRY_DELAYS`].
const MAX_RETRY_AFTER: Duration = Duration::from_secs(30);

/// Client for OpenRouter's chat-completions endpoint.
// Not `Debug`: it holds the API key.
pub struct OpenRouterClient {
    http: reqwest::Client,
    api_key: String,
    model: String,
    system_prompt: String,
    max_retries: u32,
}

impl OpenRouterClient {
    /// Creates a client. `http` is expected to carry the request timeout
    /// (`llm.timeout_secs`), so it is shared with [`fetch_capabilities`].
    pub fn new(
        http: reqwest::Client,
        api_key: String,
        model: String,
        system_prompt: String,
        max_retries: u32,
    ) -> Self {
        Self {
            http,
            api_key,
            model,
            system_prompt,
            max_retries,
        }
    }

    /// Sends one non-streaming chat request, retrying transient failures.
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError> {
        // `Bytes` so that a retry shares the body instead of copying it;
        // base64 images can make it tens of megabytes.
        let body = Bytes::from(request_body(&self.model, &self.system_prompt, request));
        let mut retries = 0;
        loop {
            match self.send_once(body.clone(), retries).await {
                Ok(response) => return Ok(response),
                Err((outcome, error)) => match should_retry(&outcome, retries, self.max_retries) {
                    Some(delay) => {
                        tracing::warn!(%error, ?delay, "retrying chat request");
                        tokio::time::sleep(delay).await;
                        retries += 1;
                    }
                    None => return Err(error),
                },
            }
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn send_once(
        &self,
        body: Bytes,
        retries: u32,
    ) -> Result<ChatResponse, (Outcome, LlmError)> {
        let transport = |source: reqwest::Error| {
            let outcome = transport_outcome(&source);
            let error = if source.is_timeout() {
                LlmError::Timeout {
                    model: self.model.clone(),
                    retries,
                    status: None,
                }
            } else {
                LlmError::Transport {
                    model: self.model.clone(),
                    retries,
                    source,
                }
            };
            (outcome, error)
        };

        let response = self
            .http
            .post(CHAT_COMPLETIONS_URL)
            .header(AUTHORIZATION, format!("Bearer {}", self.api_key))
            .header("HTTP-Referer", REFERER)
            .header("X-Title", TITLE)
            .header(CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .map_err(transport)?;

        let status = response.status();
        let retry_after = retry_after(response.headers());
        let bytes = response.bytes().await.map_err(transport)?;

        if status.is_success() {
            parse_completion(&bytes, &self.model, retries).map_err(|e| (Outcome::Other, e))
        } else {
            Err((
                Outcome::Status {
                    status: status.as_u16(),
                    retry_after,
                },
                error_from_status(status, &bytes, &self.model, retries),
            ))
        }
    }
}

/// Looks up whether `model` accepts images via OpenRouter's public model list.
#[cfg_attr(coverage_nightly, coverage(off))]
pub async fn fetch_capabilities(
    http: &reqwest::Client,
    model: &str,
) -> anyhow::Result<ModelCapabilities> {
    let body = http
        .get(MODELS_URL)
        .header("HTTP-Referer", REFERER)
        .header("X-Title", TITLE)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .context("failed to fetch the OpenRouter model list")?
        .bytes()
        .await
        .context("failed to read the OpenRouter model list")?;
    parse_capabilities(&body, model)
}

/// The result of one attempt, as far as the retry policy is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Status {
        status: u16,
        retry_after: Option<Duration>,
    },
    Connect,
    Timeout,
    Other,
}

/// Classifies a transport failure for the retry policy.
fn transport_outcome(error: &reqwest::Error) -> Outcome {
    if error.is_timeout() {
        Outcome::Timeout
    // Only a failed connect guarantees the request never reached the server;
    // other send errors may follow a processed (and billed) request.
    } else if error.is_connect() {
        Outcome::Connect
    } else {
        Outcome::Other
    }
}

/// Returns the delay before the next attempt, or `None` if the failure is final.
///
/// `retries` is the number of retries already made.
fn should_retry(outcome: &Outcome, retries: u32, max_retries: u32) -> Option<Duration> {
    if retries >= max_retries {
        return None;
    }
    let fallback = RETRY_DELAYS[(retries as usize).min(RETRY_DELAYS.len() - 1)];
    match *outcome {
        Outcome::Status {
            status,
            retry_after,
        } if status == 429 || (500..600).contains(&status) => Some(
            retry_after
                .filter(|delay| *delay <= MAX_RETRY_AFTER)
                .unwrap_or(fallback),
        ),
        Outcome::Connect => Some(fallback),
        Outcome::Status { .. } | Outcome::Timeout | Outcome::Other => None,
    }
}

/// Parses `Retry-After` given in seconds; HTTP-date values are ignored.
fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    let seconds = headers
        .get(RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()?;
    Some(Duration::from_secs(seconds))
}

#[derive(Serialize)]
struct WireRequest<'a> {
    model: &'a str,
    messages: Vec<WireMessage<'a>>,
    max_tokens: u32,
    stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<WireTool<'a>>,
}

#[derive(Serialize)]
struct WireTool<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(skip_serializing_if = "WireToolParameters::is_empty")]
    parameters: WireToolParameters<'a>,
}

#[derive(Serialize, Default)]
struct WireToolParameters<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    engine: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timezone: Option<&'a str>,
}

impl WireToolParameters<'_> {
    fn is_empty(&self) -> bool {
        self.engine.is_none() && self.mode.is_none() && self.timezone.is_none()
    }
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: &'static str,
    content: WireContent<'a>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum WireContent<'a> {
    Text(&'a str),
    Parts(Vec<WirePart<'a>>),
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WirePart<'a> {
    Text { text: &'a str },
    ImageUrl { image_url: WireImageUrl },
}

#[derive(Serialize)]
struct WireImageUrl {
    url: String,
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

fn wire_content(content: &Content) -> WireContent<'_> {
    match content {
        Content::Text(text) => WireContent::Text(text),
        Content::Parts(parts) => {
            let texts = parts.iter().filter_map(|part| match part {
                Part::Text(text) => Some(WirePart::Text { text }),
                Part::ImageDataUrl { .. } => None,
            });
            let images = parts.iter().filter_map(|part| match part {
                Part::Text(_) => None,
                Part::ImageDataUrl { mime, base64 } => Some(WirePart::ImageUrl {
                    image_url: WireImageUrl {
                        url: format!("data:{mime};base64,{base64}"),
                    },
                }),
            });
            WireContent::Parts(texts.chain(images).collect())
        }
    }
}

fn wire_tool(tool: &ServerTool) -> WireTool<'_> {
    match tool {
        ServerTool::WebSearch { engine, mode } => WireTool {
            kind: "openrouter:web_search",
            parameters: WireToolParameters {
                engine: engine.map(search_engine_name),
                mode: mode.as_deref(),
                ..Default::default()
            },
        },
        ServerTool::WebFetch { engine } => WireTool {
            kind: "openrouter:web_fetch",
            parameters: WireToolParameters {
                engine: engine.map(fetch_engine_name),
                ..Default::default()
            },
        },
        ServerTool::Datetime { timezone } => WireTool {
            kind: "openrouter:datetime",
            parameters: WireToolParameters {
                timezone: Some(timezone),
                ..Default::default()
            },
        },
    }
}

fn fetch_engine_name(engine: WebFetchEngine) -> &'static str {
    match engine {
        WebFetchEngine::Auto => "auto",
        WebFetchEngine::Native => "native",
        WebFetchEngine::Exa => "exa",
        WebFetchEngine::OpenRouter => "openrouter",
        WebFetchEngine::Firecrawl => "firecrawl",
        WebFetchEngine::Parallel => "parallel",
    }
}

fn search_engine_name(engine: WebSearchEngine) -> &'static str {
    match engine {
        WebSearchEngine::Auto => "auto",
        WebSearchEngine::Native => "native",
        WebSearchEngine::Exa => "exa",
        WebSearchEngine::Firecrawl => "firecrawl",
        WebSearchEngine::Parallel => "parallel",
        WebSearchEngine::Perplexity => "perplexity",
    }
}

/// Serialises the request body: the system prompt first, then the request messages.
fn request_body(model: &str, system_prompt: &str, request: &ChatRequest) -> Vec<u8> {
    let system = WireMessage {
        role: role_name(Role::System),
        content: WireContent::Text(system_prompt),
    };
    let messages = std::iter::once(system)
        .chain(request.messages.iter().map(|message| WireMessage {
            role: role_name(message.role),
            content: wire_content(&message.content),
        }))
        .collect();
    let body = WireRequest {
        model,
        messages,
        max_tokens: request.max_output_tokens,
        stream: false,
        tools: request.tools.iter().map(wire_tool).collect(),
    };
    serde_json::to_vec(&body).expect("request body contains only strings and integers")
}

#[derive(Deserialize)]
struct WireResponse {
    id: Option<String>,
    model: Option<String>,
    #[serde(default)]
    choices: Vec<WireChoice>,
    usage: Option<WireUsage>,
    /// Set instead of `choices` when the provider fails after a 200 was sent.
    error: Option<WireError>,
}

#[derive(Deserialize)]
struct WireChoice {
    message: Option<WireResponseMessage>,
    finish_reason: Option<String>,
    error: Option<WireError>,
}

#[derive(Deserialize)]
struct WireResponseMessage {
    content: Option<String>,
    annotations: Option<Vec<WireAnnotation>>,
}

// Every field is optional so that an annotation of another shape cannot turn
// an answer into a decode failure.
#[derive(Deserialize)]
struct WireAnnotation {
    #[serde(rename = "type")]
    kind: Option<String>,
    url_citation: Option<WireUrlCitation>,
}

#[derive(Deserialize)]
struct WireUrlCitation {
    url: Option<String>,
    title: Option<String>,
}

#[derive(Deserialize)]
struct WireUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
    cost: Option<f64>,
    // Chat completions report `server_tool_use_details`; the docs only show
    // the Responses API's `server_tool_use`.
    #[serde(alias = "server_tool_use_details")]
    server_tool_use: Option<WireServerToolUse>,
}

#[derive(Deserialize)]
struct WireServerToolUse {
    web_search_requests: Option<u64>,
    tool_calls_executed: Option<u64>,
}

#[derive(Deserialize)]
struct WireErrorBody {
    error: WireError,
}

#[derive(Deserialize)]
struct WireError {
    // A number per the OpenRouter docs; kept loose so an upstream string code
    // does not turn a readable error into a decode failure.
    code: Option<serde_json::Value>,
    message: Option<String>,
}

impl WireError {
    fn status(&self) -> Option<u16> {
        self.code
            .as_ref()?
            .as_u64()
            .and_then(|code| u16::try_from(code).ok())
    }
}

/// Turns a 200 body into a response, or into the error it describes.
fn parse_completion(body: &[u8], model: &str, retries: u32) -> Result<ChatResponse, LlmError> {
    let response: WireResponse =
        serde_json::from_slice(body).map_err(|source| LlmError::Decode {
            model: model.to_string(),
            retries,
            status: Some(200),
            request_id: None,
            source,
        })?;
    let request_id = response.id;
    let model = response.model.unwrap_or_else(|| model.to_string());
    let empty = |request_id, model| LlmError::EmptyResponse {
        model,
        retries,
        request_id,
    };

    let Some(choice) = response.choices.into_iter().next() else {
        return Err(match response.error {
            Some(error) => LlmError::InResponse {
                status: error.status(),
                provider_message: error.message,
                model,
                retries,
                request_id,
            },
            None => empty(request_id, model),
        });
    };
    if let Some(error) = choice.error {
        return Err(LlmError::InResponse {
            status: error.status(),
            provider_message: error.message,
            model,
            retries,
            request_id,
        });
    }
    let finish = match choice.finish_reason.as_deref() {
        Some("content_filter") => {
            return Err(LlmError::ContentFilter {
                model,
                retries,
                request_id,
            });
        }
        // Some providers omit the reason on a normal stop; reporting `Other`
        // would mark every such answer as abnormal.
        None | Some("stop") => Finish::Stop,
        Some("length") => Finish::Length,
        Some(other) => Finish::Other(other.to_string()),
    };
    let (content, annotations) = match choice.message {
        Some(message) => (message.content, message.annotations.unwrap_or_default()),
        None => (None, Vec::new()),
    };
    let text = match content {
        Some(text) if !text.trim().is_empty() => text,
        _ if finish == Finish::Length => {
            return Err(LlmError::OutputTokenLimit {
                model,
                retries,
                request_id,
            });
        }
        _ => return Err(empty(request_id, model)),
    };

    Ok(ChatResponse {
        text,
        finish,
        model,
        request_id,
        usage: response.usage.map(|usage| Usage {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            cost: usage.cost,
            web_search_requests: usage
                .server_tool_use
                .as_ref()
                .and_then(|tools| tools.web_search_requests),
            server_tool_calls: usage
                .server_tool_use
                .and_then(|tools| tools.tool_calls_executed),
        }),
        citations: citations(annotations),
    })
}

/// The `url_citation` annotations as citations, keeping the first of each URL.
fn citations(annotations: Vec<WireAnnotation>) -> Vec<Citation> {
    let mut seen = HashSet::new();
    annotations
        .into_iter()
        .filter(|annotation| annotation.kind.as_deref() == Some("url_citation"))
        .filter_map(|annotation| {
            let citation = annotation.url_citation?;
            let url = citation.url.filter(|url| !url.is_empty())?;
            Some(Citation {
                url,
                title: citation.title.filter(|title| !title.trim().is_empty()),
            })
        })
        .filter(|citation| seen.insert(citation.url.clone()))
        .collect()
}

/// Parses an OpenRouter error body (`{"error": {"code", "message", "metadata"}}`).
fn parse_error_body(body: &[u8]) -> Option<WireError> {
    serde_json::from_slice::<WireErrorBody>(body)
        .ok()
        .map(|body| body.error)
}

/// Maps a non-2xx response to an error. Whether to retry is decided separately.
fn error_from_status(status: StatusCode, body: &[u8], model: &str, retries: u32) -> LlmError {
    let provider_message = match parse_error_body(body) {
        Some(error) => error.message,
        None => Some(String::from_utf8_lossy(body).trim().to_string()).filter(|m| !m.is_empty()),
    };
    let model = model.to_string();
    match status.as_u16() {
        408 => LlmError::Timeout {
            model,
            retries,
            status: Some(408),
        },
        429 => LlmError::RateLimited {
            model,
            retries,
            status: Some(429),
            provider_message,
            request_id: None,
        },
        status => LlmError::Provider {
            model,
            retries,
            status,
            provider_message,
            request_id: None,
        },
    }
}

#[derive(Deserialize)]
struct WireModels {
    data: Vec<WireModel>,
}

#[derive(Deserialize)]
struct WireModel {
    id: String,
    architecture: Option<WireArchitecture>,
    #[serde(default)]
    supported_parameters: Vec<String>,
}

#[derive(Deserialize)]
struct WireArchitecture {
    #[serde(default)]
    input_modalities: Vec<String>,
}

/// Finds `model` in a `/models` body and reads its input modalities and
/// whether it takes `tools`.
///
/// Routing variants (`:nitro` etc.) are accepted on any model id but are not
/// listed, so they are looked up by their base id.
fn parse_capabilities(body: &[u8], model: &str) -> anyhow::Result<ModelCapabilities> {
    let models: WireModels =
        serde_json::from_slice(body).context("unexpected OpenRouter model list body")?;
    let base = ROUTING_VARIANTS
        .iter()
        .find_map(|variant| model.strip_suffix(variant))
        .unwrap_or(model);
    let entry = models
        .data
        .into_iter()
        .find(|entry| entry.id == base)
        .with_context(|| format!("model `{model}` is not in the OpenRouter model list"))?;
    let accepts_images = entry
        .architecture
        .is_some_and(|arch| arch.input_modalities.iter().any(|m| m == "image"));
    let accepts_tools = entry.supported_parameters.iter().any(|p| p == "tools");
    Ok(ModelCapabilities {
        accepts_images,
        accepts_tools,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::ChatMessage;
    use serde_json::{Value, json};

    const MODEL: &str = "openai/gpt-4o-mini";

    fn body_json(request: &ChatRequest) -> Value {
        serde_json::from_slice(&request_body(MODEL, "be helpful", request)).unwrap()
    }

    fn user(content: Content) -> ChatMessage {
        ChatMessage {
            role: Role::User,
            content,
        }
    }

    fn completion(choice: Value) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "id": "gen-123",
            "model": MODEL,
            "choices": [choice],
        }))
        .unwrap()
    }

    fn status(status: u16) -> Outcome {
        Outcome::Status {
            status,
            retry_after: None,
        }
    }

    #[test]
    fn request_body_puts_system_prompt_first_and_text_content_as_string() {
        let request = ChatRequest {
            messages: vec![
                user(Content::Text("hello".to_string())),
                ChatMessage {
                    role: Role::Assistant,
                    content: Content::Text("hi".to_string()),
                },
            ],
            max_output_tokens: 256,
            tools: Vec::new(),
        };

        assert_eq!(
            body_json(&request),
            json!({
                "model": MODEL,
                "messages": [
                    {"role": "system", "content": "be helpful"},
                    {"role": "user", "content": "hello"},
                    {"role": "assistant", "content": "hi"},
                ],
                "max_tokens": 256,
                "stream": false,
            })
        );
    }

    #[test]
    fn request_body_orders_text_parts_before_image_parts_as_data_urls() {
        let request = ChatRequest {
            messages: vec![user(Content::Parts(vec![
                Part::ImageDataUrl {
                    mime: "image/png".to_string(),
                    base64: "AAAA".to_string(),
                },
                Part::Text("what is this?".to_string()),
                Part::ImageDataUrl {
                    mime: "image/jpeg".to_string(),
                    base64: "BBBB".to_string(),
                },
            ]))],
            max_output_tokens: 256,
            tools: Vec::new(),
        };

        assert_eq!(
            body_json(&request)["messages"][1]["content"],
            json!([
                {"type": "text", "text": "what is this?"},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}},
                {"type": "image_url", "image_url": {"url": "data:image/jpeg;base64,BBBB"}},
            ])
        );
    }

    #[test]
    fn request_body_omits_tools_when_none_are_enabled() {
        let request = ChatRequest {
            messages: vec![user(Content::Text("hello".to_string()))],
            max_output_tokens: 256,
            tools: Vec::new(),
        };
        assert_eq!(body_json(&request).get("tools"), None);
    }

    #[test]
    fn request_body_declares_enabled_server_tools() {
        let request = ChatRequest {
            messages: vec![user(Content::Text("hello".to_string()))],
            max_output_tokens: 256,
            tools: vec![
                ServerTool::WebSearch {
                    engine: None,
                    mode: None,
                },
                ServerTool::WebSearch {
                    engine: Some(WebSearchEngine::Parallel),
                    mode: Some("fast".to_string()),
                },
                ServerTool::WebFetch { engine: None },
                ServerTool::WebFetch {
                    engine: Some(WebFetchEngine::OpenRouter),
                },
                ServerTool::Datetime {
                    timezone: "Asia/Tokyo".to_string(),
                },
            ],
        };

        assert_eq!(
            body_json(&request)["tools"],
            json!([
                {"type": "openrouter:web_search"},
                {"type": "openrouter:web_search", "parameters": {"engine": "parallel", "mode": "fast"}},
                {"type": "openrouter:web_fetch"},
                {"type": "openrouter:web_fetch", "parameters": {"engine": "openrouter"}},
                {"type": "openrouter:datetime", "parameters": {"timezone": "Asia/Tokyo"}},
            ])
        );
    }

    #[test]
    fn server_tool_engines_are_sent_by_their_config_names() {
        let search = [
            "auto",
            "native",
            "exa",
            "firecrawl",
            "parallel",
            "perplexity",
        ];
        let fetch = [
            "auto",
            "native",
            "exa",
            "openrouter",
            "firecrawl",
            "parallel",
        ];
        let tools = search
            .iter()
            .map(|name| ServerTool::WebSearch {
                engine: Some(serde_json::from_value(json!(name)).unwrap()),
                mode: None,
            })
            .chain(fetch.iter().map(|name| ServerTool::WebFetch {
                engine: Some(serde_json::from_value(json!(name)).unwrap()),
            }))
            .collect();
        let request = ChatRequest {
            messages: vec![user(Content::Text("hello".to_string()))],
            max_output_tokens: 256,
            tools,
        };

        let sent: Vec<Value> = body_json(&request)["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["parameters"]["engine"].clone())
            .collect();
        let expected: Vec<Value> = search.iter().chain(&fetch).map(|n| json!(n)).collect();
        assert_eq!(sent, expected);
    }

    #[test]
    fn annotations_without_the_url_citation_type_or_a_url_are_ignored() {
        let body = completion(json!({
            "finish_reason": "stop",
            "message": {
                "content": "answer",
                "annotations": [
                    {"url_citation": {"url": "https://untyped.example/"}},
                    {"type": "url_citation", "url_citation": {"url": "", "title": "empty"}},
                    {"type": "url_citation"},
                ],
            },
        }));

        assert!(
            parse_completion(&body, MODEL, 0)
                .unwrap()
                .citations
                .is_empty()
        );
    }

    #[test]
    fn url_citations_become_citations_without_duplicate_urls() {
        let body = completion(json!({
            "finish_reason": "stop",
            "message": {
                "content": "answer",
                "annotations": [
                    {"type": "url_citation", "url_citation": {"url": "https://a.example/", "title": "A", "content": "excerpt", "start_index": 0, "end_index": 6}},
                    {"type": "file", "file": {"name": "x"}},
                    {"type": "url_citation", "url_citation": {"url": "https://b.example/", "title": " "}},
                    {"type": "url_citation", "url_citation": {"url": "https://a.example/", "title": "A again"}},
                    {"type": "url_citation", "url_citation": {"title": "no url"}},
                ],
            },
        }));

        let response = parse_completion(&body, MODEL, 0).unwrap();

        assert_eq!(
            response.citations,
            [
                Citation {
                    url: "https://a.example/".to_string(),
                    title: Some("A".to_string()),
                },
                Citation {
                    url: "https://b.example/".to_string(),
                    title: None,
                },
            ]
        );
    }

    #[test]
    fn missing_or_null_annotations_yield_no_citations() {
        for message in [
            json!({"content": "answer"}),
            json!({"content": "answer", "annotations": null}),
        ] {
            let body = completion(json!({"finish_reason": "stop", "message": message}));
            assert!(
                parse_completion(&body, MODEL, 0)
                    .unwrap()
                    .citations
                    .is_empty()
            );
        }
    }

    #[test]
    fn server_tool_use_details_report_searches_and_executed_tool_calls() {
        for key in ["server_tool_use_details", "server_tool_use"] {
            let body = serde_json::to_vec(&json!({
                "id": "gen-123",
                "choices": [{"finish_reason": "stop", "message": {"content": "answer"}}],
                "usage": {
                    "prompt_tokens": 1768,
                    "completion_tokens": 208,
                    "total_tokens": 1976,
                    "cost": 0.007,
                    key: {"web_search_requests": 1, "tool_calls_requested": 3, "tool_calls_executed": 2},
                },
            }))
            .unwrap();

            let usage = parse_completion(&body, MODEL, 0).unwrap().usage.unwrap();
            assert_eq!(usage.web_search_requests, Some(1), "{key}");
            assert_eq!(usage.server_tool_calls, Some(2), "{key}");
        }
    }

    #[test]
    fn success_body_with_unknown_fields_yields_text_finish_id_and_usage() {
        let body = br#"{
            "id": "gen-1759420000-abc",
            "provider": "OpenAI",
            "model": "openai/gpt-4o-mini",
            "object": "chat.completion",
            "created": 1759420000,
            "choices": [{
                "logprobs": null,
                "finish_reason": "stop",
                "native_finish_reason": "stop",
                "index": 0,
                "message": {"role": "assistant", "content": "pong", "refusal": null, "reasoning": null}
            }],
            "system_fingerprint": "fp_1",
            "usage": {
                "prompt_tokens": 12,
                "completion_tokens": 2,
                "total_tokens": 14,
                "cost": 0.0000027,
                "prompt_tokens_details": {"cached_tokens": 0}
            }
        }"#;

        let response = parse_completion(body, "configured/model", 0).unwrap();

        assert_eq!(
            response,
            ChatResponse {
                text: "pong".to_string(),
                finish: Finish::Stop,
                model: "openai/gpt-4o-mini".to_string(),
                request_id: Some("gen-1759420000-abc".to_string()),
                usage: Some(Usage {
                    prompt_tokens: 12,
                    completion_tokens: 2,
                    total_tokens: 14,
                    cost: Some(0.0000027),
                    web_search_requests: None,
                    server_tool_calls: None,
                }),
                citations: Vec::new(),
            }
        );
    }

    #[test]
    fn length_finish_reason_is_a_successful_truncated_response() {
        let body =
            completion(json!({"finish_reason": "length", "message": {"content": "partial"}}));
        let response = parse_completion(&body, MODEL, 0).unwrap();
        assert_eq!(response.finish, Finish::Length);
        assert_eq!(response.text, "partial");
    }

    #[test]
    fn content_filter_finish_reason_is_a_content_filter_error() {
        let body =
            completion(json!({"finish_reason": "content_filter", "message": {"content": ""}}));
        let error = parse_completion(&body, MODEL, 0).unwrap_err();
        assert!(matches!(error, LlmError::ContentFilter { .. }), "{error:?}");
    }

    #[test]
    fn null_or_blank_content_is_an_empty_response_error() {
        for content in [Value::Null, json!("  \n ")] {
            let body =
                completion(json!({"finish_reason": "stop", "message": {"content": content}}));
            let error = parse_completion(&body, MODEL, 0).unwrap_err();
            assert!(matches!(error, LlmError::EmptyResponse { .. }), "{error:?}");
        }
    }

    #[test]
    fn length_finish_without_text_is_an_output_token_limit_error() {
        for message in [
            json!({"content": null}),
            json!({"content": " \n"}),
            json!({"content": null, "reasoning": "long chain of thought"}),
        ] {
            let body = completion(json!({"finish_reason": "length", "message": message}));
            let error = parse_completion(&body, MODEL, 0).unwrap_err();
            assert!(
                matches!(error, LlmError::OutputTokenLimit { .. }),
                "{error:?}"
            );
        }
        let body = completion(json!({"finish_reason": "length"}));
        let error = parse_completion(&body, MODEL, 0).unwrap_err();
        assert!(
            matches!(error, LlmError::OutputTokenLimit { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn empty_text_with_a_finish_reason_other_than_length_is_never_an_output_token_limit_error() {
        for finish_reason in [json!("stop"), Value::Null, json!("tool_calls")] {
            let body =
                completion(json!({"finish_reason": finish_reason, "message": {"content": null}}));
            let error = parse_completion(&body, MODEL, 0).unwrap_err();
            assert!(matches!(error, LlmError::EmptyResponse { .. }), "{error:?}");
        }
    }

    #[test]
    fn choice_error_is_an_in_response_error_with_code_and_message() {
        let body = completion(json!({
            "finish_reason": "error",
            "message": {"content": ""},
            "error": {"code": 502, "message": "upstream died"},
        }));

        match parse_completion(&body, MODEL, 1).unwrap_err() {
            LlmError::InResponse {
                status,
                provider_message,
                request_id,
                retries,
                ..
            } => {
                assert_eq!(status, Some(502));
                assert_eq!(provider_message.as_deref(), Some("upstream died"));
                assert_eq!(request_id.as_deref(), Some("gen-123"));
                assert_eq!(retries, 1);
            }
            other => panic!("expected InResponse, got {other:?}"),
        }
    }

    #[test]
    fn success_status_with_only_a_top_level_error_is_an_in_response_error() {
        let body = br#"{"error": {"code": 502, "message": "provider failed after headers"}}"#;

        match parse_completion(body, MODEL, 0).unwrap_err() {
            LlmError::InResponse {
                status,
                provider_message,
                ..
            } => {
                assert_eq!(status, Some(502));
                assert_eq!(
                    provider_message.as_deref(),
                    Some("provider failed after headers")
                );
            }
            other => panic!("expected InResponse, got {other:?}"),
        }
    }

    #[test]
    fn unexpected_success_body_is_a_decode_error() {
        let error = parse_completion(b"<html>bad gateway</html>", MODEL, 0).unwrap_err();
        assert!(matches!(error, LlmError::Decode { .. }), "{error:?}");
    }

    #[test]
    fn error_bodies_for_402_and_429_keep_code_and_message() {
        let body_402 = br#"{"error": {"code": 402, "message": "Insufficient credits"}}"#;
        let body_429 = br#"{"error": {"code": 429, "message": "Rate limit exceeded", "metadata": {"provider_name": "x"}}}"#;

        let error = parse_error_body(body_402).unwrap();
        assert_eq!(error.status(), Some(402));
        assert_eq!(error.message.as_deref(), Some("Insufficient credits"));
        let error = parse_error_body(body_429).unwrap();
        assert_eq!(error.status(), Some(429));
        assert_eq!(error.message.as_deref(), Some("Rate limit exceeded"));
    }

    #[test]
    fn http_status_maps_to_error_variant() {
        let body = br#"{"error": {"code": 0, "message": "m"}}"#;
        let variant = |code| error_from_status(StatusCode::from_u16(code).unwrap(), body, MODEL, 0);

        assert!(matches!(
            variant(408),
            LlmError::Timeout {
                status: Some(408),
                ..
            }
        ));
        assert!(matches!(variant(429), LlmError::RateLimited { .. }));
        for code in [400, 401, 402, 403, 404, 500, 503] {
            assert!(
                matches!(variant(code), LlmError::Provider { status, .. } if status == code),
                "{code}"
            );
        }
    }

    #[test]
    fn non_json_error_body_is_kept_as_provider_message() {
        let error = error_from_status(StatusCode::BAD_GATEWAY, b"Bad Gateway\n", MODEL, 0);
        assert!(
            matches!(&error, LlmError::Provider { provider_message: Some(m), .. } if m == "Bad Gateway"),
            "{error:?}"
        );
    }

    #[test]
    fn rate_limit_retries_with_1s_then_3s_then_stops_at_the_limit() {
        assert_eq!(
            should_retry(&status(429), 0, 2),
            Some(Duration::from_secs(1))
        );
        assert_eq!(
            should_retry(&status(429), 1, 2),
            Some(Duration::from_secs(3))
        );
        assert_eq!(should_retry(&status(429), 2, 2), None);
    }

    #[test]
    fn server_errors_and_connection_errors_are_retried() {
        assert!(should_retry(&status(503), 0, 2).is_some());
        assert!(should_retry(&Outcome::Connect, 0, 2).is_some());
    }

    #[test]
    fn client_errors_timeouts_and_other_failures_are_not_retried() {
        for outcome in [
            status(400),
            status(402),
            status(408),
            Outcome::Timeout,
            Outcome::Other,
        ] {
            assert_eq!(should_retry(&outcome, 0, 2), None, "{outcome:?}");
        }
    }

    #[test]
    fn retry_after_is_honoured_up_to_30s_and_falls_back_beyond() {
        let with_retry_after = |seconds| Outcome::Status {
            status: 429,
            retry_after: Some(Duration::from_secs(seconds)),
        };
        assert_eq!(
            should_retry(&with_retry_after(10), 0, 2),
            Some(Duration::from_secs(10))
        );
        assert_eq!(
            should_retry(&with_retry_after(120), 0, 2),
            Some(Duration::from_secs(1))
        );
    }

    #[tokio::test]
    async fn refused_connection_is_retried_but_a_request_that_was_not_sent_is_not() {
        let http = reqwest::Client::new();
        let refused = http.get("http://127.0.0.1:1").send().await.unwrap_err();
        let unbuildable = http.get("not a url").build().unwrap_err();

        assert_eq!(transport_outcome(&refused), Outcome::Connect);
        assert_eq!(transport_outcome(&unbuildable), Outcome::Other);
    }

    #[test]
    fn retry_after_header_is_read_as_seconds() {
        let mut headers = HeaderMap::new();
        assert_eq!(retry_after(&headers), None);
        headers.insert(RETRY_AFTER, "10".parse().unwrap());
        assert_eq!(retry_after(&headers), Some(Duration::from_secs(10)));
        headers.insert(
            RETRY_AFTER,
            "Wed, 21 Oct 2015 07:28:00 GMT".parse().unwrap(),
        );
        assert_eq!(retry_after(&headers), None);
    }

    #[test]
    fn error_display_never_contains_the_api_key() {
        const API_KEY: &str = "sk-or-v1-secret-key";
        let client = OpenRouterClient::new(
            reqwest::Client::new(),
            API_KEY.to_string(),
            MODEL.to_string(),
            "be helpful".to_string(),
            2,
        );
        let model = || client.model.clone();
        let message = || Some("provider said no".to_string());
        let errors = [
            LlmError::Timeout {
                model: model(),
                retries: 0,
                status: None,
            },
            LlmError::RateLimited {
                model: model(),
                retries: 2,
                status: Some(429),
                provider_message: message(),
                request_id: None,
            },
            LlmError::Provider {
                model: model(),
                retries: 0,
                status: 401,
                provider_message: message(),
                request_id: None,
            },
            LlmError::Transport {
                model: model(),
                retries: 2,
                source: client.http.get("not a url").build().unwrap_err(),
            },
            LlmError::ContentFilter {
                model: model(),
                retries: 0,
                request_id: Some("gen-1".to_string()),
            },
            LlmError::OutputTokenLimit {
                model: model(),
                retries: 0,
                request_id: Some("gen-1".to_string()),
            },
            LlmError::EmptyResponse {
                model: model(),
                retries: 0,
                request_id: Some("gen-1".to_string()),
            },
            LlmError::InResponse {
                model: model(),
                retries: 0,
                status: Some(502),
                provider_message: message(),
                request_id: Some("gen-1".to_string()),
            },
            LlmError::Decode {
                model: model(),
                retries: 0,
                status: Some(200),
                request_id: None,
                source: serde_json::from_str::<Value>("{").unwrap_err(),
            },
        ];

        for error in errors {
            let shown = format!("{:#}", anyhow::Error::from(error));
            assert!(!shown.contains(API_KEY), "{shown}");
        }
    }

    #[test]
    fn long_provider_message_is_truncated_to_1024_chars_in_display() {
        let error = LlmError::Provider {
            model: MODEL.to_string(),
            retries: 0,
            status: 500,
            provider_message: Some("x".repeat(5000)),
            request_id: None,
        };
        let shown = error.to_string();
        assert!(shown.contains(&format!("{}…", "x".repeat(1024))), "{shown}");
        assert!(!shown.contains(&"x".repeat(1025)), "{shown}");
    }

    #[test]
    fn capabilities_follow_the_models_input_modalities() {
        let body = br#"{"data": [
            {"id": "vision/model", "architecture": {"input_modalities": ["text", "image", "file"], "output_modalities": ["text"]}},
            {"id": "text/model", "architecture": {"input_modalities": ["text"]}}
        ]}"#;

        assert!(
            parse_capabilities(body, "vision/model")
                .unwrap()
                .accepts_images
        );
        assert!(
            !parse_capabilities(body, "text/model")
                .unwrap()
                .accepts_images
        );
        let error = parse_capabilities(body, "missing/model").unwrap_err();
        assert!(error.to_string().contains("missing/model"), "{error}");
    }

    #[test]
    fn tool_support_follows_the_models_supported_parameters() {
        let body = br#"{"data": [
            {"id": "tool/model", "supported_parameters": ["max_tokens", "tools", "tool_choice"]},
            {"id": "plain/model", "supported_parameters": ["max_tokens"]},
            {"id": "unknown/model"}
        ]}"#;

        assert!(
            parse_capabilities(body, "tool/model")
                .unwrap()
                .accepts_tools
        );
        assert!(
            !parse_capabilities(body, "plain/model")
                .unwrap()
                .accepts_tools
        );
        assert!(
            !parse_capabilities(body, "unknown/model")
                .unwrap()
                .accepts_tools
        );
    }

    #[test]
    fn routing_variant_uses_the_capabilities_of_its_base_model() {
        let body = br#"{"data": [
            {"id": "vision/model", "architecture": {"input_modalities": ["text", "image"]}},
            {"id": "vision/model:free", "architecture": {"input_modalities": ["text"]}}
        ]}"#;

        assert!(
            parse_capabilities(body, "vision/model:nitro")
                .unwrap()
                .accepts_images
        );
        assert!(
            !parse_capabilities(body, "vision/model:free")
                .unwrap()
                .accepts_images
        );
    }

    /// Manual check against the live API: `cargo test live_ping -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "calls the live OpenRouter API"]
    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn live_ping() {
        let (http, api_key, model) = live_setup();

        println!("{:?}", fetch_capabilities(&http, &model).await);
        let client =
            OpenRouterClient::new(http, api_key, model, "Reply with one word.".to_string(), 2);
        let request = ChatRequest {
            messages: vec![user(Content::Text("ping".to_string()))],
            max_output_tokens: 32,
            tools: Vec::new(),
        };
        println!("{:#?}", client.chat(&request).await.unwrap());
    }

    /// Manual check of server tools: `cargo test live_web_search -- --ignored --nocapture`.
    /// Prints the citations and the usage, including cost and search count.
    #[tokio::test]
    #[ignore = "calls the live OpenRouter API and is billed per search"]
    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn live_web_search() {
        let (http, api_key, model) = live_setup();

        println!("{:?}", fetch_capabilities(&http, &model).await);
        let client = OpenRouterClient::new(
            http,
            api_key,
            model,
            "Answer briefly and cite your sources.".to_string(),
            2,
        );
        let request = ChatRequest {
            messages: vec![user(Content::Text(
                "What is today's date, and what is the latest stable Rust release?".to_string(),
            ))],
            max_output_tokens: 1024,
            tools: vec![
                ServerTool::WebSearch {
                    engine: None,
                    mode: None,
                },
                ServerTool::Datetime {
                    timezone: "Asia/Tokyo".to_string(),
                },
            ],
        };
        println!("{:#?}", client.chat(&request).await.unwrap());
    }

    /// Reads `OPENROUTER_API_KEY` from the environment or `.env`; the model is
    /// `OPENROUTER_MODEL` if set, otherwise `llm.model` from `CONFIG_FILE_PATH`.
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn live_setup() -> (reqwest::Client, String, String) {
        dotenvy::dotenv().ok();
        let api_key = std::env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY");
        let model = std::env::var("OPENROUTER_MODEL").unwrap_or_else(|_| {
            let path =
                std::env::var("CONFIG_FILE_PATH").expect("OPENROUTER_MODEL or CONFIG_FILE_PATH");
            let config: crate::config::PythiaConfig =
                std::fs::read_to_string(path).unwrap().parse().unwrap();
            config.llm.model
        });
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap();
        (http, api_key, model)
    }
}
