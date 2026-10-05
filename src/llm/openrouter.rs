//! OpenRouter chat-completions client.
//!
//! Network calls are thin wrappers; request building, response parsing, and the
//! retry policy are pure functions so they can be tested without a server.

use super::{
    ChatRequest, ChatResponse, Content, Finish, LlmError, ModelCapabilities, Part, Role, Usage,
};
use anyhow::Context as _;
use bytes::Bytes;
use reqwest::{
    StatusCode,
    header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, RETRY_AFTER},
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const CHAT_COMPLETIONS_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const MODELS_URL: &str = "https://openrouter.ai/api/v1/models";
const REFERER: &str = "https://github.com/m1sk9/Pythia";
const TITLE: &str = "Pythia";

/// Model id suffixes that OpenRouter accepts but does not list in `/models`.
const ROUTING_VARIANTS: [&str; 4] = [":nitro", ":floor", ":exacto", ":online"];

/// Delays before the first and subsequent retries.
const RETRY_DELAYS: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(3)];
/// A `Retry-After` above this makes the failure final.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(30);

/// Client for OpenRouter's chat-completions endpoint.
// Not `Debug`: it holds the API key.
pub struct OpenRouterClient {
    http: reqwest::Client,
    api_key: String,
    model: String,
    /// Tried in order by OpenRouter when `model` fails.
    fallback_models: Vec<String>,
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
        fallback_models: Vec<String>,
        system_prompt: String,
        max_retries: u32,
    ) -> Self {
        Self {
            http,
            api_key,
            model,
            fallback_models,
            system_prompt,
            max_retries,
        }
    }

    /// Sends one non-streaming chat request, retrying transient failures.
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError> {
        // `Bytes` so that a retry shares the body instead of copying it;
        // base64 images can make it tens of megabytes.
        let body = Bytes::from(request_body(
            &self.model,
            &self.fallback_models,
            &self.system_prompt,
            request,
        ));
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

/// Looks up what every one of `models` accepts via OpenRouter's public model list.
#[cfg_attr(coverage_nightly, coverage(off))]
pub async fn fetch_capabilities(
    http: &reqwest::Client,
    models: &[&str],
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
    parse_capabilities(&body, models)
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
        } if status == 429 || (500..600).contains(&status) => match retry_after {
            // Retrying sooner than asked would only collect another 429.
            Some(delay) if delay > MAX_RETRY_AFTER => None,
            Some(delay) => Some(delay),
            None => Some(fallback),
        },
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
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    models: Option<Vec<&'a str>>,
    messages: Vec<WireMessage<'a>>,
    max_tokens: u32,
    stream: bool,
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

/// Serialises the request body: the system prompt first, then the request messages.
///
/// With fallbacks, every model goes into `models` in order, as OpenRouter's
/// model fallbacks expect; otherwise the model goes into `model`.
fn request_body(
    model: &str,
    fallback_models: &[String],
    system_prompt: &str,
    request: &ChatRequest,
) -> Vec<u8> {
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
    let (model, models) = if fallback_models.is_empty() {
        (Some(model), None)
    } else {
        let models = std::iter::once(model)
            .chain(fallback_models.iter().map(String::as_str))
            .collect();
        (None, Some(models))
    };
    let body = WireRequest {
        model,
        models,
        messages,
        max_tokens: request.max_output_tokens,
        stream: false,
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
}

#[derive(Deserialize)]
struct WireUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
    cost: Option<f64>,
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
    let text = match choice.message.and_then(|message| message.content) {
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
        }),
    })
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
}

#[derive(Deserialize)]
struct WireArchitecture {
    #[serde(default)]
    input_modalities: Vec<String>,
}

/// Finds each of `models` in a `/models` body and reads their input modalities.
///
/// A capability holds only if every model has it, because any of them may end
/// up answering. Routing variants (`:nitro` etc.) are accepted on any model id
/// but are not listed, so they are looked up by their base id.
fn parse_capabilities(body: &[u8], models: &[&str]) -> anyhow::Result<ModelCapabilities> {
    let list: WireModels =
        serde_json::from_slice(body).context("unexpected OpenRouter model list body")?;
    let mut accepts_images = true;
    for &model in models {
        let base = ROUTING_VARIANTS
            .iter()
            .find_map(|variant| model.strip_suffix(variant))
            .unwrap_or(model);
        let entry = list
            .data
            .iter()
            .find(|entry| entry.id == base)
            .with_context(|| format!("model `{model}` is not in the OpenRouter model list"))?;
        accepts_images &= entry
            .architecture
            .as_ref()
            .is_some_and(|arch| arch.input_modalities.iter().any(|m| m == "image"));
    }
    Ok(ModelCapabilities { accepts_images })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::ChatMessage;
    use serde_json::{Value, json};

    const MODEL: &str = "openai/gpt-4o-mini";

    fn body_json(request: &ChatRequest) -> Value {
        serde_json::from_slice(&request_body(MODEL, &[], "be helpful", request)).unwrap()
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
    fn request_body_with_fallbacks_lists_every_model_in_order_under_models_only() {
        let request = ChatRequest {
            messages: vec![user(Content::Text("hello".to_string()))],
            max_output_tokens: 256,
        };
        let fallbacks = ["backup/a".to_string(), "backup/b".to_string()];
        let body: Value =
            serde_json::from_slice(&request_body(MODEL, &fallbacks, "be helpful", &request))
                .unwrap();

        assert_eq!(body["models"], json!([MODEL, "backup/a", "backup/b"]));
        assert!(body.get("model").is_none(), "{body}");
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
                }),
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
    fn retry_after_is_honoured_up_to_30s_and_beyond_that_the_failure_is_final() {
        let with_retry_after = |seconds| Outcome::Status {
            status: 429,
            retry_after: Some(Duration::from_secs(seconds)),
        };
        assert_eq!(
            should_retry(&with_retry_after(10), 0, 2),
            Some(Duration::from_secs(10))
        );
        assert_eq!(
            should_retry(&with_retry_after(30), 0, 2),
            Some(Duration::from_secs(30))
        );
        assert_eq!(should_retry(&with_retry_after(120), 0, 2), None);
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
            Vec::new(),
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
            parse_capabilities(body, &["vision/model"])
                .unwrap()
                .accepts_images
        );
        assert!(
            !parse_capabilities(body, &["text/model"])
                .unwrap()
                .accepts_images
        );
        let error = parse_capabilities(body, &["missing/model"]).unwrap_err();
        assert!(error.to_string().contains("missing/model"), "{error}");
    }

    #[test]
    fn images_are_accepted_only_when_every_model_accepts_them() {
        let body = br#"{"data": [
            {"id": "vision/a", "architecture": {"input_modalities": ["text", "image"]}},
            {"id": "vision/b", "architecture": {"input_modalities": ["text", "image"]}},
            {"id": "text/model", "architecture": {"input_modalities": ["text"]}}
        ]}"#;

        assert!(
            parse_capabilities(body, &["vision/a", "vision/b"])
                .unwrap()
                .accepts_images
        );
        assert!(
            !parse_capabilities(body, &["vision/a", "text/model"])
                .unwrap()
                .accepts_images
        );
        let error = parse_capabilities(body, &["vision/a", "missing/model"]).unwrap_err();
        assert!(error.to_string().contains("missing/model"), "{error}");
    }

    #[test]
    fn routing_variant_uses_the_capabilities_of_its_base_model() {
        let body = br#"{"data": [
            {"id": "vision/model", "architecture": {"input_modalities": ["text", "image"]}},
            {"id": "vision/model:free", "architecture": {"input_modalities": ["text"]}}
        ]}"#;

        assert!(
            parse_capabilities(body, &["vision/model:nitro"])
                .unwrap()
                .accepts_images
        );
        assert!(
            !parse_capabilities(body, &["vision/model:free"])
                .unwrap()
                .accepts_images
        );
    }

    /// Manual check against the live API: `cargo test live_ping -- --ignored --nocapture`.
    /// Reads `OPENROUTER_API_KEY` from the environment or `.env`; the model is
    /// `OPENROUTER_MODEL` if set, otherwise `llm.model` from `CONFIG_FILE_PATH`.
    #[tokio::test]
    #[ignore = "calls the live OpenRouter API"]
    async fn live_ping() {
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

        println!("{:?}", fetch_capabilities(&http, &[&model]).await);
        let client = OpenRouterClient::new(
            http,
            api_key,
            model,
            Vec::new(),
            "Reply with one word.".to_string(),
            2,
        );
        let request = ChatRequest {
            messages: vec![user(Content::Text("ping".to_string()))],
            max_output_tokens: 32,
        };
        println!("{:#?}", client.chat(&request).await.unwrap());
    }
}
