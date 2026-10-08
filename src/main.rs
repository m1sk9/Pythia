//! Pythia - A Discord bot that bridges your server and LLM APIs.

#![deny(clippy::all)]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

mod attachments;
mod config;
mod context;
mod gateway;
mod llm;
mod orchestrator;
mod reply;
mod thread;

use crate::attachments::ImagePolicy;
use crate::config::{EnvConfig, LogFormat, PythiaConfig};
use crate::llm::{
    ReasoningEffort,
    openrouter::{self, OpenRouterClient},
};
use crate::orchestrator::AppState;
use std::{sync::Arc, time::Duration};
use tracing_subscriber::EnvFilter;
use twilight_cache_inmemory::{DefaultInMemoryCache, ResourceType};
use twilight_gateway::{ConfigBuilder, Intents, Shard, ShardId};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let envs = EnvConfig::init()?;
    PythiaConfig::init()?;
    let config = PythiaConfig::get();

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log.level));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    match config.log.format {
        LogFormat::Json => builder.json().init(),
        LogFormat::Compact => builder.compact().init(),
    }

    tracing::debug!(?config, config_file_path = %envs.config_file_path, "resolved configuration");
    if config.discord.allowed_guilds.is_empty() {
        anyhow::bail!("discord.allowed_guilds is empty");
    }

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.llm.timeout_secs))
        .build()?;
    // The model list is off the hot path: an outage must not keep the bot down,
    // and a wrong model id still surfaces on the first chat request.
    let capabilities = match openrouter::fetch_capabilities(&http, &config.llm.model).await {
        Ok(capabilities) => Some(capabilities),
        Err(error) => {
            tracing::warn!(
                error = format!("{error:#}"),
                "failed to look up model capabilities; disabling images, tools, and reasoning effort"
            );
            None
        }
    };
    let images_enabled = config.attachments.images
        && capabilities
            .as_ref()
            .is_some_and(|capabilities| capabilities.accepts_images);
    let mut tools = config.tools.server_tools(&config.thread.timezone);
    match &capabilities {
        None => tools.clear(),
        Some(capabilities) if !tools.is_empty() && !capabilities.accepts_tools => {
            tracing::warn!(model = %config.llm.model, "model does not support tools; disabling them");
            tools.clear();
        }
        Some(_) => {}
    }
    let reasoning_effort = match capabilities
        .as_ref()
        .map(|capabilities| capabilities.reasoning_effort_to_send(config.llm.reasoning_effort))
    {
        None => None,
        Some(Ok(effort)) => effort,
        Some(Err(reason)) => {
            tracing::warn!(
                model = %config.llm.model,
                effort = config.llm.reasoning_effort.map(ReasoningEffort::as_str),
                reason,
                "leaving reasoning to the model's default"
            );
            None
        }
    };
    tracing::info!(
        model = %config.llm.model,
        images = images_enabled,
        ?tools,
        reasoning_effort = reasoning_effort.map(ReasoningEffort::as_str),
        "LLM client ready"
    );
    let llm = OpenRouterClient::new(
        http.clone(),
        envs.openrouter_api_key.clone(),
        config.llm.model.clone(),
        config.llm.system_prompt.clone(),
        config.llm.max_retries,
    );

    let state = Arc::new(AppState {
        http: twilight_http::Client::new(envs.discord_api_token.clone()),
        web: http,
        images: ImagePolicy {
            enabled: images_enabled,
            max_images: config.attachments.max_images,
            recent_messages: config.attachments.recent_messages,
            max_image_bytes: config.attachments.max_image_bytes,
        },
        tools,
        reasoning_effort,
        cache: DefaultInMemoryCache::builder()
            .resource_types(ResourceType::CHANNEL)
            .build(),
        llm,
        config,
        timezone: jiff::tz::TimeZone::get(&config.thread.timezone)?,
        registry: Default::default(),
        scheduler: Default::default(),
        semaphore: tokio::sync::Semaphore::new(config.limits.max_concurrent),
    });
    let mut shard_config = ConfigBuilder::new(
        envs.discord_api_token.clone(),
        Intents::GUILDS | Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT,
    );
    if let Some(text) = config.activity_text() {
        shard_config = shard_config.presence(gateway::presence(text));
    }
    let shard = Shard::with_config(ShardId::ONE, shard_config.build());

    gateway::run(shard, state).await
}
