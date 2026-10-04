//! Provider-neutral chat-completions types.
//!
//! The shape follows the OpenAI chat-completions API (messages with optional
//! content parts) so that other backends can sit behind the same types.

pub mod openrouter;

/// Upper bound on how much of a provider message is shown in [`LlmError`]'s `Display`.
const MAX_DISPLAYED_PROVIDER_MESSAGE_CHARS: usize = 1024;

/// Author of a chat message.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "sent by the orchestrator (#294)")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
}

/// One message of a conversation.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessage {
    pub role: Role,
    pub content: Content,
}

/// Message content: plain text, or a list of parts when images are attached.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "sent by the orchestrator (#294)")
)]
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Text(String),
    Parts(Vec<Part>),
}

/// One part of a multi-part message.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "built from image attachments (#295)"
    )
)]
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Text(String),
    ImageDataUrl { mime: String, base64: String },
}

/// A chat request. The model and system prompt are supplied by the client.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub max_output_tokens: u32,
}

/// A successful chat completion.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatResponse {
    pub text: String,
    pub finish: Finish,
    /// Model that actually served the request.
    pub model: String,
    pub request_id: Option<String>,
    pub usage: Option<Usage>,
}

/// Why generation stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finish {
    Stop,
    /// Cut off by `max_output_tokens`; the text is still usable.
    Length,
    Other(String),
}

/// Token usage reported by the provider.
#[derive(Debug, Clone, PartialEq)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    /// Cost in credits, when the provider reports it.
    pub cost: Option<f64>,
}

/// What the configured model accepts as input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelCapabilities {
    pub accepts_images: bool,
}

/// A failed chat request. `retries` is the number of retries made before giving up.
#[derive(thiserror::Error, Debug)]
pub enum LlmError {
    #[error("request to `{model}` timed out after {retries} retries")]
    Timeout {
        model: String,
        retries: u32,
        status: Option<u16>,
    },
    #[error(
        "`{model}` is rate limited (gave up after {retries} retries){}",
        provider_suffix(.provider_message)
    )]
    RateLimited {
        model: String,
        retries: u32,
        status: Option<u16>,
        provider_message: Option<String>,
        request_id: Option<String>,
    },
    #[error(
        "provider returned HTTP {status} for `{model}` after {retries} retries{}",
        provider_suffix(.provider_message)
    )]
    Provider {
        model: String,
        retries: u32,
        status: u16,
        provider_message: Option<String>,
        request_id: Option<String>,
    },
    #[error("failed to reach the provider for `{model}` after {retries} retries")]
    Transport {
        model: String,
        retries: u32,
        #[source]
        source: reqwest::Error,
    },
    #[error("`{model}` refused to answer (content filter)")]
    ContentFilter {
        model: String,
        retries: u32,
        request_id: Option<String>,
    },
    /// `finish_reason` is `length` and no text was produced, e.g. a reasoning
    /// model spent the whole `max_output_tokens` budget on reasoning.
    #[error("`{model}` hit the output token limit before producing any text")]
    OutputTokenLimit {
        model: String,
        retries: u32,
        request_id: Option<String>,
    },
    #[error("`{model}` returned an empty response")]
    EmptyResponse {
        model: String,
        retries: u32,
        request_id: Option<String>,
    },
    #[error("`{model}` failed while generating{}", provider_suffix(.provider_message))]
    InResponse {
        model: String,
        retries: u32,
        status: Option<u16>,
        provider_message: Option<String>,
        request_id: Option<String>,
    },
    #[error("unexpected response body from `{model}`")]
    Decode {
        model: String,
        retries: u32,
        status: Option<u16>,
        request_id: Option<String>,
        #[source]
        source: serde_json::Error,
    },
}

fn provider_suffix(message: &Option<String>) -> String {
    match message {
        None => String::new(),
        Some(message) => {
            let mut chars = message.chars();
            let head: String = chars
                .by_ref()
                .take(MAX_DISPLAYED_PROVIDER_MESSAGE_CHARS)
                .collect();
            let ellipsis = if chars.next().is_some() { "…" } else { "" };
            format!(": {head}{ellipsis}")
        }
    }
}
