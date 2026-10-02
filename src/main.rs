//! Pythia - A Discord bot that bridges your server and LLM APIs.

#![deny(clippy::all)]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

mod config;
mod gateway;

use crate::config::{EnvConfig, LogFormat, PythiaConfig};
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
        tracing::warn!("discord.allowed_guilds is empty; Pythia will not respond anywhere");
    }

    let _http = twilight_http::Client::new(envs.discord_api_token.clone());
    let shard = Shard::new(
        ShardId::ONE,
        envs.discord_api_token.clone(),
        Intents::GUILDS | Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT,
    );

    gateway::run(shard).await
}
