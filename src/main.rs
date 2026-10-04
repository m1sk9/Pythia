//! Pythia - A Discord bot that bridges your server and LLM APIs.

#![deny(clippy::all)]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

mod config;
mod gateway;
mod llm;

use crate::config::{EnvConfig, LogFormat, PythiaConfig};
use crate::llm::openrouter::{self, OpenRouterClient};
use std::time::Duration;
use tracing_subscriber::EnvFilter;
use twilight_gateway::{Intents, Shard, ShardId};

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
    let accepts_images = match openrouter::fetch_capabilities(&http, &config.llm.model).await {
        Ok(capabilities) => capabilities.accepts_images,
        Err(error) => {
            tracing::warn!(
                error = format!("{error:#}"),
                "failed to look up model capabilities; disabling images"
            );
            false
        }
    };
    let images_enabled = config.attachments.images && accepts_images;
    tracing::info!(model = %config.llm.model, images = images_enabled, "LLM client ready");
    let _llm = OpenRouterClient::new(
        http,
        envs.openrouter_api_key.clone(),
        config.llm.model.clone(),
        config.llm.system_prompt.clone(),
        config.llm.max_retries,
    );

    let _discord = twilight_http::Client::new(envs.discord_api_token.clone());
    let shard = Shard::new(
        ShardId::ONE,
        envs.discord_api_token.clone(),
        Intents::GUILDS | Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT,
    );

    gateway::run(shard).await
}
