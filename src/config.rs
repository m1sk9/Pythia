//! Configuration module for Pythia.
//!
//! Secrets come from environment variables ([`EnvConfig`]); everything else
//! comes from the TOML file pointed to by `CONFIG_FILE_PATH` ([`PythiaConfig`]).

use crate::llm::{ServerTool, WebFetchEngine, WebSearchEngine};
use serde::Deserialize;
use std::{path::PathBuf, str::FromStr, sync::OnceLock};

/// Global environment configuration instance.
static ENV_CONFIG: OnceLock<EnvConfig> = OnceLock::new();

/// Global configuration instance.
static CONFIG: OnceLock<PythiaConfig> = OnceLock::new();

/// System prompt used when neither `llm.system_prompt` nor `llm.system_prompt_file` is set.
pub const DEFAULT_SYSTEM_PROMPT: &str = "You are Pythia, a helpful assistant on Discord.";

/// Environment variable configuration.
// Not `Debug`: both tokens would end up in the debug log of the resolved configuration.
#[derive(Deserialize)]
pub struct EnvConfig {
    /// Discord API token for bot authentication.
    pub discord_api_token: String,
    /// OpenRouter API key.
    pub openrouter_api_key: String,
    /// Path to the TOML configuration file.
    pub config_file_path: String,
}

impl EnvConfig {
    /// Loads the environment configuration and stores it globally.
    pub fn init() -> Result<&'static EnvConfig, PythiaConfigError> {
        let config = Self::from_vars(std::env::vars())?;
        ENV_CONFIG.set(config).map_err(|_| PythiaConfigError::Set)?;
        Ok(Self::get())
    }

    /// Returns a reference to the environment configuration.
    ///
    /// # Panics
    ///
    /// Panics if [`EnvConfig::init`] has not been called.
    pub fn get() -> &'static EnvConfig {
        ENV_CONFIG
            .get()
            .expect("environment configuration is not initialised")
    }

    /// Builds the configuration from `(name, value)` pairs, rejecting empty values.
    fn from_vars(
        vars: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self, PythiaConfigError> {
        let config: EnvConfig = envy::from_iter(vars)
            .map_err(|e| PythiaConfigError::Validate(format!("environment: {e}")))?;
        for (name, value) in [
            ("DISCORD_API_TOKEN", &config.discord_api_token),
            ("OPENROUTER_API_KEY", &config.openrouter_api_key),
            ("CONFIG_FILE_PATH", &config.config_file_path),
        ] {
            if value.is_empty() {
                return Err(PythiaConfigError::Validate(format!(
                    "environment variable {name} must not be empty"
                )));
            }
        }
        Ok(config)
    }
}

/// Pythia configuration, validated and with every default resolved.
#[derive(Debug)]
pub struct PythiaConfig {
    pub discord: DiscordConfig,
    pub llm: LlmConfig,
    pub context: ContextConfig,
    pub response: ResponseConfig,
    pub attachments: AttachmentsConfig,
    pub tools: ToolsConfig,
    pub thread: ThreadConfig,
    pub limits: LimitsConfig,
    pub log: LogConfig,
}

/// The configuration file as written, before validation.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawConfig {
    discord: DiscordConfig,
    llm: RawLlmConfig,
    context: ContextConfig,
    response: ResponseConfig,
    attachments: AttachmentsConfig,
    tools: ToolsConfig,
    thread: ThreadConfig,
    limits: LimitsConfig,
    log: LogConfig,
}

/// Discord's limit on a custom status.
const MAX_ACTIVITY_CHARS: usize = 128;

/// What Pythia shows as its custom status.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ActivityDisplay {
    /// `llm.model`.
    #[default]
    Model,
    /// `Running v{version}`.
    Version,
    /// `discord.activity_custom`.
    Custom,
    /// No activity.
    Disabled,
}

/// Discord-related configuration.
#[derive(Deserialize, Debug, Default)]
#[serde(default)]
pub struct DiscordConfig {
    /// Guilds where Pythia responds. Empty means nowhere.
    pub allowed_guilds: Vec<u64>,
    pub activity_display: ActivityDisplay,
    /// Required when `activity_display` is `custom`.
    pub activity_custom: Option<String>,
}

/// LLM provider.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    #[default]
    OpenRouter,
}

/// `[llm]` as written; `model` is optional here so that its absence can be
/// reported with the full key path.
#[derive(Deserialize)]
#[serde(default)]
struct RawLlmConfig {
    provider: LlmProvider,
    model: Option<String>,
    system_prompt: Option<String>,
    system_prompt_file: Option<PathBuf>,
    max_output_tokens: u32,
    timeout_secs: u64,
    max_retries: u32,
}

impl Default for RawLlmConfig {
    fn default() -> Self {
        Self {
            provider: LlmProvider::default(),
            model: None,
            system_prompt: None,
            system_prompt_file: None,
            max_output_tokens: 4096,
            timeout_secs: 120,
            max_retries: 2,
        }
    }
}

/// LLM configuration.
#[derive(Debug)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "`provider` has a single variant, so nothing branches on it yet"
    )
)]
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub model: String,
    /// Resolved from `system_prompt`, `system_prompt_file`, or the default.
    pub system_prompt: String,
    pub max_output_tokens: u32,
    pub timeout_secs: u64,
    /// Retries apply to 429 / 5xx / connection errors only.
    pub max_retries: u32,
}

/// Conversation context configuration.
#[derive(Deserialize, Debug)]
#[serde(default)]
pub struct ContextConfig {
    /// Thread messages fetched per turn (Discord allows at most 100 per request).
    pub max_messages: u8,
    /// Character budget for the whole context.
    pub max_chars: usize,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_messages: 100,
            max_chars: 32000,
        }
    }
}

/// Response configuration.
#[derive(Deserialize, Debug)]
#[serde(default)]
pub struct ResponseConfig {
    /// Maximum number of 2000-char messages per answer.
    pub max_parts: usize,
}

impl Default for ResponseConfig {
    fn default() -> Self {
        Self { max_parts: 5 }
    }
}

/// Image attachment configuration.
#[derive(Deserialize, Debug)]
#[serde(default)]
pub struct AttachmentsConfig {
    pub images: bool,
    pub max_images: usize,
    pub recent_messages: usize,
    pub max_image_bytes: usize,
}

impl Default for AttachmentsConfig {
    fn default() -> Self {
        Self {
            images: true,
            max_images: 4,
            recent_messages: 5,
            max_image_bytes: 5_242_880,
        }
    }
}

/// OpenRouter server tools. All are off by default because searches and
/// fetches are billed per request on top of tokens.
#[derive(Deserialize, Debug, Default)]
#[serde(default)]
pub struct ToolsConfig {
    pub web_search: bool,
    /// Left to OpenRouter (`auto`) when unset.
    pub web_search_engine: Option<WebSearchEngine>,
    /// Engine-specific and passed through as written; left to OpenRouter when unset.
    pub web_search_mode: Option<String>,
    pub web_fetch: bool,
    /// Left to OpenRouter (`auto`) when unset.
    pub web_fetch_engine: Option<WebFetchEngine>,
    pub datetime: bool,
}

impl ToolsConfig {
    /// The enabled tools; `datetime` reports the time in `timezone`.
    pub fn server_tools(&self, timezone: &str) -> Vec<ServerTool> {
        [
            self.web_search.then(|| ServerTool::WebSearch {
                engine: self.web_search_engine,
                mode: self.web_search_mode.clone(),
            }),
            self.web_fetch.then_some(ServerTool::WebFetch {
                engine: self.web_fetch_engine,
            }),
            self.datetime.then(|| ServerTool::Datetime {
                timezone: timezone.to_string(),
            }),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

/// Thread configuration.
#[derive(Deserialize, Debug)]
#[serde(default)]
pub struct ThreadConfig {
    /// IANA time zone used in thread names.
    pub timezone: String,
}

impl Default for ThreadConfig {
    fn default() -> Self {
        Self {
            timezone: "Asia/Tokyo".to_string(),
        }
    }
}

/// Concurrency limits.
#[derive(Deserialize, Debug)]
#[serde(default)]
pub struct LimitsConfig {
    /// Concurrent generations across all threads.
    pub max_concurrent: usize,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self { max_concurrent: 4 }
    }
}

/// Logging configuration.
#[derive(Deserialize, Debug)]
#[serde(default)]
pub struct LogConfig {
    /// Tracing filter directive used when `RUST_LOG` is not set.
    pub level: String,
    pub format: LogFormat,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "pythia=info".to_string(),
            format: LogFormat::default(),
        }
    }
}

/// Log output format.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Human-readable compact format.
    #[default]
    Compact,
    /// Structured JSON format.
    Json,
}

/// Errors that can occur when loading configuration.
#[derive(thiserror::Error, Debug)]
pub enum PythiaConfigError {
    #[error("failed to read configuration file `{path}`")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse configuration file")]
    Parse(#[from] toml::de::Error),
    #[error("invalid configuration: {0}")]
    Validate(String),
    #[error("configuration is already initialised")]
    Set,
}

impl RawConfig {
    fn validate(self) -> Result<PythiaConfig, PythiaConfigError> {
        let invalid = |msg: String| Err(PythiaConfigError::Validate(msg));
        let llm = self.llm;

        let Some(model) = llm.model.filter(|m| !m.is_empty()) else {
            return invalid(
                "`llm.model` is required; set it in [llm] (models differ widely in price)"
                    .to_string(),
            );
        };

        let system_prompt = match (llm.system_prompt, llm.system_prompt_file) {
            (Some(_), Some(_)) => {
                return invalid(
                    "`llm.system_prompt` and `llm.system_prompt_file` are mutually exclusive"
                        .to_string(),
                );
            }
            (Some(prompt), None) => prompt,
            (None, Some(path)) => match std::fs::read_to_string(&path) {
                Ok(prompt) => prompt,
                Err(e) => {
                    return invalid(format!(
                        "failed to read `llm.system_prompt_file` `{}`: {e}",
                        path.display()
                    ));
                }
            },
            (None, None) => DEFAULT_SYSTEM_PROMPT.to_string(),
        };

        if jiff::tz::TimeZone::get(&self.thread.timezone).is_err() {
            return invalid(format!(
                "`thread.timezone` `{}` is not a known IANA time zone",
                self.thread.timezone
            ));
        }

        if llm.timeout_secs == 0 {
            return invalid("`llm.timeout_secs` must be at least 1".to_string());
        }

        if self.limits.max_concurrent == 0 {
            return invalid("`limits.max_concurrent` must be at least 1".to_string());
        }

        if !(1..=100).contains(&self.context.max_messages) {
            return invalid(
                "`context.max_messages` must be between 1 and 100 (Discord's limit per request)"
                    .to_string(),
            );
        }

        if self.response.max_parts == 0 {
            return invalid("`response.max_parts` must be at least 1".to_string());
        }

        if self.discord.activity_display == ActivityDisplay::Custom {
            match self.discord.activity_custom.as_deref() {
                None | Some("") => {
                    return invalid(
                        "`discord.activity_custom` is required when `discord.activity_display` is \"custom\""
                            .to_string(),
                    );
                }
                Some(text) if text.chars().count() > MAX_ACTIVITY_CHARS => {
                    return invalid(format!(
                        "`discord.activity_custom` must be at most {MAX_ACTIVITY_CHARS} characters"
                    ));
                }
                Some(_) => {}
            }
        }

        Ok(PythiaConfig {
            discord: self.discord,
            llm: LlmConfig {
                provider: llm.provider,
                model,
                system_prompt,
                max_output_tokens: llm.max_output_tokens,
                timeout_secs: llm.timeout_secs,
                max_retries: llm.max_retries,
            },
            context: self.context,
            response: self.response,
            attachments: self.attachments,
            tools: self.tools,
            thread: self.thread,
            limits: self.limits,
            log: self.log,
        })
    }
}

impl FromStr for PythiaConfig {
    type Err = PythiaConfigError;

    /// Parses and validates a configuration file's contents.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        toml::from_str::<RawConfig>(s)?.validate()
    }
}

impl PythiaConfig {
    /// Reads the file at `CONFIG_FILE_PATH`, validates it, and stores it globally.
    ///
    /// Requires [`EnvConfig::init`] to have been called.
    pub fn init() -> Result<(), PythiaConfigError> {
        let path = &EnvConfig::get().config_file_path;
        let buffer = std::fs::read_to_string(path).map_err(|source| PythiaConfigError::Read {
            path: path.clone(),
            source,
        })?;
        CONFIG
            .set(buffer.parse()?)
            .map_err(|_| PythiaConfigError::Set)
    }

    /// The text shown as Pythia's activity, or `None` when it is disabled.
    pub fn activity_text(&self) -> Option<String> {
        match self.discord.activity_display {
            ActivityDisplay::Model => Some(self.llm.model.clone()),
            ActivityDisplay::Version => Some(format!("Running v{}", env!("CARGO_PKG_VERSION"))),
            ActivityDisplay::Custom => self.discord.activity_custom.clone(),
            ActivityDisplay::Disabled => None,
        }
    }

    /// Returns a reference to the global configuration.
    ///
    /// # Panics
    ///
    /// Panics if [`PythiaConfig::init`] has not been called.
    pub fn get() -> &'static PythiaConfig {
        CONFIG.get().expect("configuration is not initialised")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validation_message(result: Result<PythiaConfig, PythiaConfigError>) -> String {
        match result {
            Err(PythiaConfigError::Validate(msg)) => msg,
            other => panic!("expected a validation error, got {other:?}"),
        }
    }

    #[test]
    fn reference_config_parses_to_documented_values() {
        let config: PythiaConfig = include_str!("../config/config.toml").parse().unwrap();

        assert_eq!(config.discord.allowed_guilds, vec![123456789012345678]);
        assert_eq!(config.discord.activity_display, ActivityDisplay::Model);
        assert_eq!(config.discord.activity_custom, None);
        assert_eq!(config.llm.provider, LlmProvider::OpenRouter);
        assert_eq!(config.llm.model, "<openrouter model id>");
        assert_eq!(config.llm.system_prompt, DEFAULT_SYSTEM_PROMPT);
        assert_eq!(config.llm.max_output_tokens, 4096);
        assert_eq!(config.llm.timeout_secs, 120);
        assert_eq!(config.llm.max_retries, 2);
        assert_eq!(config.context.max_messages, 100);
        assert_eq!(config.context.max_chars, 32000);
        assert_eq!(config.response.max_parts, 5);
        assert!(config.attachments.images);
        assert_eq!(config.attachments.max_images, 4);
        assert_eq!(config.attachments.recent_messages, 5);
        assert_eq!(config.attachments.max_image_bytes, 5242880);
        assert!(!config.tools.web_search);
        assert_eq!(config.tools.web_search_engine, None);
        assert_eq!(config.tools.web_search_mode, None);
        assert_eq!(config.tools.web_fetch_engine, None);
        assert!(!config.tools.web_fetch);
        assert!(!config.tools.datetime);
        assert_eq!(config.thread.timezone, "Asia/Tokyo");
        assert_eq!(config.limits.max_concurrent, 4);
        assert_eq!(config.log.level, "pythia=info");
        assert_eq!(config.log.format, LogFormat::Compact);
    }

    #[test]
    fn minimal_config_yields_every_default() {
        let config: PythiaConfig = "[llm]\nmodel = \"x\"".parse().unwrap();

        assert!(config.discord.allowed_guilds.is_empty());
        assert_eq!(config.llm.provider, LlmProvider::OpenRouter);
        assert_eq!(config.llm.model, "x");
        assert_eq!(config.llm.system_prompt, DEFAULT_SYSTEM_PROMPT);
        assert_eq!(config.llm.max_output_tokens, 4096);
        assert_eq!(config.llm.timeout_secs, 120);
        assert_eq!(config.llm.max_retries, 2);
        assert_eq!(config.context.max_messages, 100);
        assert_eq!(config.context.max_chars, 32000);
        assert_eq!(config.response.max_parts, 5);
        assert!(config.attachments.images);
        assert_eq!(config.attachments.max_images, 4);
        assert_eq!(config.attachments.recent_messages, 5);
        assert_eq!(config.attachments.max_image_bytes, 5242880);
        assert!(!config.tools.web_search);
        assert_eq!(config.tools.web_search_engine, None);
        assert_eq!(config.tools.web_search_mode, None);
        assert_eq!(config.tools.web_fetch_engine, None);
        assert!(!config.tools.web_fetch);
        assert!(!config.tools.datetime);
        assert_eq!(config.thread.timezone, "Asia/Tokyo");
        assert_eq!(config.limits.max_concurrent, 4);
        assert_eq!(config.log.level, "pythia=info");
        assert_eq!(config.log.format, LogFormat::Compact);
    }

    #[test]
    fn missing_llm_model_error_names_the_key() {
        let msg = validation_message("".parse());
        assert!(msg.contains("llm.model"), "{msg}");
    }

    #[test]
    fn system_prompt_and_file_together_is_rejected() {
        let toml = r#"
            [llm]
            model = "x"
            system_prompt = "inline"
            system_prompt_file = "/config/system_prompt.md"
        "#;
        let msg = validation_message(toml.parse());
        assert!(msg.contains("mutually exclusive"), "{msg}");
    }

    #[test]
    fn neither_system_prompt_key_uses_default_prompt() {
        let config: PythiaConfig = "[llm]\nmodel = \"x\"".parse().unwrap();
        assert_eq!(config.llm.system_prompt, DEFAULT_SYSTEM_PROMPT);
    }

    #[test]
    fn unknown_provider_fails_to_parse() {
        let result: Result<PythiaConfig, _> = "[llm]\nprovider = \"openai\"\nmodel = \"x\"".parse();
        assert!(
            matches!(result, Err(PythiaConfigError::Parse(_))),
            "{result:?}"
        );
    }

    #[test]
    fn unknown_timezone_is_rejected() {
        let toml = "[llm]\nmodel = \"x\"\n[thread]\ntimezone = \"Mars/Olympus\"";
        let msg = validation_message(toml.parse());
        assert!(msg.contains("thread.timezone"), "{msg}");
    }

    #[test]
    fn zero_max_concurrent_is_rejected() {
        let toml = "[llm]\nmodel = \"x\"\n[limits]\nmax_concurrent = 0";
        let msg = validation_message(toml.parse());
        assert!(msg.contains("limits.max_concurrent"), "{msg}");
    }

    #[test]
    fn max_messages_outside_discords_range_is_rejected() {
        for value in [0, 101] {
            let toml = format!("[llm]\nmodel = \"x\"\n[context]\nmax_messages = {value}");
            let msg = validation_message(toml.parse());
            assert!(msg.contains("context.max_messages"), "{msg}");
        }
    }

    #[test]
    fn zero_max_parts_is_rejected() {
        let toml = "[llm]\nmodel = \"x\"\n[response]\nmax_parts = 0";
        let msg = validation_message(toml.parse());
        assert!(msg.contains("response.max_parts"), "{msg}");
    }

    #[test]
    fn zero_timeout_secs_is_rejected() {
        let toml = "[llm]\nmodel = \"x\"\ntimeout_secs = 0";
        let msg = validation_message(toml.parse());
        assert!(msg.contains("llm.timeout_secs"), "{msg}");
    }

    #[test]
    fn enabled_tools_are_listed_with_datetime_in_the_given_timezone() {
        let toml = r#"
            [llm]
            model = "x"
            [tools]
            web_search = true
            web_search_engine = "parallel"
            web_search_mode = "fast"
            web_fetch = true
            web_fetch_engine = "openrouter"
            datetime = true
        "#;
        let config: PythiaConfig = toml.parse().unwrap();

        assert_eq!(
            config.tools.server_tools("Asia/Tokyo"),
            [
                ServerTool::WebSearch {
                    engine: Some(WebSearchEngine::Parallel),
                    mode: Some("fast".to_string()),
                },
                ServerTool::WebFetch {
                    engine: Some(WebFetchEngine::OpenRouter)
                },
                ServerTool::Datetime {
                    timezone: "Asia/Tokyo".to_string()
                },
            ]
        );
        assert!(ToolsConfig::default().server_tools("UTC").is_empty());
    }

    #[test]
    fn unknown_web_search_or_fetch_engine_fails_to_parse() {
        for key in ["web_search_engine", "web_fetch_engine"] {
            let toml = format!("[llm]\nmodel = \"x\"\n[tools]\n{key} = \"google\"");
            let result: Result<PythiaConfig, _> = toml.parse();
            assert!(
                matches!(result, Err(PythiaConfigError::Parse(_))),
                "{key}: {result:?}"
            );
        }
    }

    #[test]
    fn activity_text_follows_activity_display() {
        let text = |discord: &str| {
            format!("[discord]\n{discord}\n[llm]\nmodel = \"vendor/model\"")
                .parse::<PythiaConfig>()
                .unwrap()
                .activity_text()
        };

        assert_eq!(text(""), Some("vendor/model".to_string()));
        assert_eq!(
            text("activity_display = \"version\""),
            Some(format!("Running v{}", env!("CARGO_PKG_VERSION")))
        );
        assert_eq!(
            text("activity_display = \"custom\"\nactivity_custom = \"Running!\""),
            Some("Running!".to_string())
        );
        assert_eq!(text("activity_display = \"disabled\""), None);
    }

    #[test]
    fn custom_activity_without_text_is_rejected() {
        for custom in ["", "activity_custom = \"\""] {
            let toml =
                format!("[discord]\nactivity_display = \"custom\"\n{custom}\n[llm]\nmodel = \"x\"");
            let msg = validation_message(toml.parse());
            assert!(msg.contains("discord.activity_custom"), "{msg}");
        }
    }

    #[test]
    fn custom_activity_over_discords_limit_is_rejected() {
        let toml = format!(
            "[discord]\nactivity_display = \"custom\"\nactivity_custom = \"{}\"\n[llm]\nmodel = \"x\"",
            "a".repeat(MAX_ACTIVITY_CHARS + 1)
        );
        let msg = validation_message(toml.parse());
        assert!(msg.contains("discord.activity_custom"), "{msg}");
    }

    #[test]
    fn empty_env_value_is_rejected() {
        let vars = [
            ("DISCORD_API_TOKEN", "token"),
            ("OPENROUTER_API_KEY", ""),
            ("CONFIG_FILE_PATH", "config/config.toml"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()));

        match EnvConfig::from_vars(vars) {
            Err(PythiaConfigError::Validate(msg)) => {
                assert!(msg.contains("OPENROUTER_API_KEY"), "{msg}")
            }
            Err(e) => panic!("expected a validation error, got {e:?}"),
            Ok(_) => panic!("expected an empty OPENROUTER_API_KEY to be rejected"),
        }
    }
}
