//! Gateway event loop: receives events from the shard, keeps the cache
//! up to date, and shuts the connection down on Ctrl-C / SIGTERM.

use std::sync::OnceLock;
use twilight_cache_inmemory::{DefaultInMemoryCache, ResourceType};
use twilight_gateway::{CloseFrame, Event, EventTypeFlags, Shard, StreamExt};
use twilight_model::id::{Id, marker::UserMarker};

/// The bot's own user id, known once the first Ready event arrives.
static BOT_USER_ID: OnceLock<Id<UserMarker>> = OnceLock::new();

/// Returns the bot's user id, or `None` before the first Ready event.
#[expect(dead_code, reason = "used by the trigger decision (#294)")]
pub fn bot_user_id() -> Option<Id<UserMarker>> {
    BOT_USER_ID.get().copied()
}

/// Runs the shard until a shutdown signal is received or the connection fatally closes.
pub async fn run(mut shard: Shard) -> anyhow::Result<()> {
    let cache = DefaultInMemoryCache::builder()
        .resource_types(ResourceType::CHANNEL)
        .build();
    let mut shutdown = std::pin::pin!(shutdown_signal());
    let mut closing = false;

    loop {
        tokio::select! {
            _ = &mut shutdown, if !closing => {
                tracing::info!("shutdown signal received; closing gateway connection");
                shard.close(CloseFrame::NORMAL);
                closing = true;
            }
            item = shard.next_event(EventTypeFlags::all()) => {
                // twilight 0.17 has no `ReceiveMessageError::is_fatal`; the stream
                // yields `None` once the shard is fatally closed, so errors are only logged.
                let event = match item {
                    None => break,
                    Some(Err(source)) => {
                        tracing::warn!(?source, "error receiving gateway event");
                        continue;
                    }
                    Some(Ok(event)) => event,
                };
                cache.update(&event);

                match event {
                    Event::Ready(ready) => {
                        tracing::info!(
                            version = ready.version,
                            user = %ready.user.name,
                            user_id = %ready.user.id,
                            "connected to Discord gateway"
                        );
                        let _ = BOT_USER_ID.set(ready.user.id);
                    }
                    // `close` does not end the stream by itself; the shard reports
                    // the close it initiated and would otherwise keep running.
                    Event::GatewayClose(_) if closing => break,
                    _ => {}
                }
            }
        }
    }

    if closing {
        tracing::info!("gateway connection closed");
        Ok(())
    } else {
        anyhow::bail!(
            "gateway connection fatally closed (state: {:?})",
            shard.state()
        )
    }
}

/// Resolves on Ctrl-C, or on SIGTERM where available (`docker stop`).
async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();

    // `tokio::signal::unix` does not exist on Windows, which CI also builds for.
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm =
            signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {}
            _ = sigterm.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        let _ = ctrl_c.await;
    }
}
