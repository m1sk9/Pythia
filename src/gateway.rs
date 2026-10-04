//! Gateway event loop: receives events from the shard, keeps the cache up to
//! date, decides which messages start or continue a conversation, and shuts
//! the connection down on Ctrl-C / SIGTERM.

use crate::{
    context::display_name,
    orchestrator::{self, AppState},
    reply,
    thread::thread_name,
};
use std::sync::{Arc, OnceLock};
use twilight_gateway::{CloseFrame, Event, EventTypeFlags, Shard, StreamExt};
use twilight_model::{
    channel::{Channel, ChannelType, Message, message::MessageType},
    id::{
        Id,
        marker::{ChannelMarker, UserMarker},
    },
};

/// The bot's own user id, known once the first Ready event arrives.
static BOT_USER_ID: OnceLock<Id<UserMarker>> = OnceLock::new();

/// Returns the bot's user id, or `None` before the first Ready event.
pub fn bot_user_id() -> Option<Id<UserMarker>> {
    BOT_USER_ID.get().copied()
}

/// What the trigger decision needs to know about a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelInfo {
    pub kind: ChannelType,
    pub owner_id: Option<Id<UserMarker>>,
    pub parent_id: Option<Id<ChannelMarker>>,
    pub locked: bool,
}

impl From<&Channel> for ChannelInfo {
    fn from(channel: &Channel) -> Self {
        Self {
            kind: channel.kind,
            owner_id: channel.owner_id,
            parent_id: channel.parent_id,
            locked: channel
                .thread_metadata
                .as_ref()
                .is_some_and(|metadata| metadata.locked),
        }
    }
}

/// What to do with a message that passed [`is_candidate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Ignore,
    /// Answer in the message's thread; `register` marks the thread as a
    /// conversation first.
    Continue {
        register: bool,
    },
    /// The thread has not been checked yet: look for the bot in its history.
    ProbeHistory,
    /// Create a thread from the message and answer there.
    StartThread,
}

/// Whether a message can trigger a turn at all, before looking at its channel.
pub fn is_candidate(message: &Message, bot_id: Id<UserMarker>, allowed_guilds: &[u64]) -> bool {
    !message.author.bot
        && message.author.system != Some(true)
        && message.author.id != bot_id
        && message
            .guild_id
            .is_some_and(|guild| allowed_guilds.contains(&guild.get()))
        && matches!(message.kind, MessageType::Regular | MessageType::Reply)
}

/// Decides how to respond to a candidate message posted in `channel`.
/// `registry` is the conversation registry's entry for the channel.
pub fn decide(
    message: &Message,
    bot_id: Id<UserMarker>,
    channel: &ChannelInfo,
    registry: Option<bool>,
) -> Trigger {
    let mentioned = message.mentions.iter().any(|mention| mention.id == bot_id);

    if !channel.kind.is_thread() {
        return if mentioned {
            Trigger::StartThread
        } else {
            Trigger::Ignore
        };
    }

    if channel.locked {
        Trigger::Ignore
    } else if channel.owner_id == Some(bot_id) || registry == Some(true) {
        Trigger::Continue { register: false }
    } else if mentioned {
        Trigger::Continue { register: true }
    } else if registry.is_none() {
        Trigger::ProbeHistory
    } else {
        Trigger::Ignore
    }
}

/// Runs the shard until a shutdown signal is received or the connection fatally closes.
#[cfg_attr(coverage_nightly, coverage(off))]
pub async fn run(mut shard: Shard, state: Arc<AppState>) -> anyhow::Result<()> {
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
                state.cache.update(&event);

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
                    Event::MessageCreate(message) if !closing => {
                        if let Some(bot_id) = bot_user_id() {
                            tokio::spawn(handle_message(Arc::clone(&state), bot_id, message.0));
                        }
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

#[cfg_attr(coverage_nightly, coverage(off))]
async fn handle_message(state: Arc<AppState>, bot_id: Id<UserMarker>, message: Message) {
    if !is_candidate(&message, bot_id, &state.config.discord.allowed_guilds) {
        return;
    }
    let channel = match resolve_channel(&state, message.channel_id).await {
        Ok(channel) => channel,
        Err(error) => {
            tracing::warn!(error = format!("{error:#}"), channel_id = %message.channel_id, "failed to look up channel");
            return;
        }
    };
    let thread = message.channel_id;
    let registry = state.registry.get(thread);

    match decide(&message, bot_id, &channel, registry) {
        Trigger::Ignore => {}
        Trigger::Continue { register } => {
            if register {
                state.registry.set(thread, true);
            }
            orchestrator::schedule_turns(state, bot_id, thread, channel.parent_id);
        }
        Trigger::ProbeHistory => match has_bot_message(&state, bot_id, thread).await {
            Ok(is_conversation) => {
                state.registry.set(thread, is_conversation);
                if is_conversation {
                    orchestrator::schedule_turns(state, bot_id, thread, channel.parent_id);
                }
            }
            Err(error) => {
                tracing::warn!(error = format!("{error:#}"), thread_id = %thread, "failed to read thread history");
            }
        },
        Trigger::StartThread => start_thread(state, bot_id, &message).await,
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn resolve_channel(
    state: &AppState,
    channel_id: Id<ChannelMarker>,
) -> anyhow::Result<ChannelInfo> {
    if let Some(channel) = state.cache.channel(channel_id) {
        return Ok(ChannelInfo::from(channel.value()));
    }
    let channel = state.http.channel(channel_id).await?.model().await?;
    Ok(ChannelInfo::from(&channel))
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn has_bot_message(
    state: &AppState,
    bot_id: Id<UserMarker>,
    thread: Id<ChannelMarker>,
) -> anyhow::Result<bool> {
    let history = state
        .http
        .channel_messages(thread)
        .limit(u16::from(state.config.context.max_messages))
        .await?
        .models()
        .await?;
    Ok(history.iter().any(|message| message.author.id == bot_id))
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn start_thread(state: Arc<AppState>, bot_id: Id<UserMarker>, message: &Message) {
    let name = thread_name(display_name(message), message.timestamp, &state.timezone);
    let created = async {
        state
            .http
            .create_thread_from_message(message.channel_id, message.id, &name)
            .await?
            .model()
            .await
            .map_err(anyhow::Error::from)
    }
    .await;

    match created {
        Ok(thread) => {
            state.registry.set(thread.id, true);
            orchestrator::schedule_turns(state, bot_id, thread.id, Some(message.channel_id));
        }
        Err(error) => {
            tracing::warn!(error = format!("{error:#}"), channel_id = %message.channel_id, "failed to create thread");
            let embed = reply::notice_embed(
                "Could not start a thread",
                "Pythia answers in threads. Check that it can create public threads in this channel.",
            );
            if let Err(error) =
                reply::post_embed(&state.http, message.channel_id, Some(message.id), embed).await
            {
                tracing::error!(%error, "failed to post thread creation error");
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const BOT: u64 = 1000;
    const ALICE: u64 = 2000;
    const GUILD: u64 = 500;

    fn message(content: &str, mentions_bot: bool) -> Message {
        let mentions = if mentions_bot {
            json!([{"id": BOT.to_string(), "username": "pythia", "discriminator": "0",
                    "avatar": null, "bot": true, "public_flags": 0}])
        } else {
            json!([])
        };
        serde_json::from_value(json!({
            "id": "10",
            "channel_id": "20",
            "guild_id": GUILD.to_string(),
            "author": {"id": ALICE.to_string(), "username": "alice", "discriminator": "0", "avatar": null},
            "content": content,
            "timestamp": "2026-10-02T14:40:00.000000+00:00",
            "edited_timestamp": null,
            "tts": false,
            "mention_everyone": false,
            "mentions": mentions,
            "mention_roles": [],
            "attachments": [],
            "embeds": [],
            "pinned": false,
            "type": 0,
        }))
        .unwrap()
    }

    fn channel(kind: ChannelType) -> ChannelInfo {
        ChannelInfo {
            kind,
            owner_id: Some(Id::new(ALICE)),
            parent_id: kind.is_thread().then(|| Id::new(30)),
            locked: false,
        }
    }

    fn bot() -> Id<UserMarker> {
        Id::new(BOT)
    }

    #[test]
    fn bot_and_system_authors_are_not_candidates() {
        let mut from_bot = message("hi", true);
        from_bot.author.bot = true;
        let mut from_system = message("hi", true);
        from_system.author.system = Some(true);

        assert!(is_candidate(&message("hi", true), bot(), &[GUILD]));
        assert!(!is_candidate(&from_bot, bot(), &[GUILD]));
        assert!(!is_candidate(&from_system, bot(), &[GUILD]));
    }

    #[test]
    fn messages_outside_allowed_guilds_are_not_candidates() {
        let mut direct = message("hi", true);
        direct.guild_id = None;

        assert!(!is_candidate(&message("hi", true), bot(), &[GUILD + 1]));
        assert!(!is_candidate(&message("hi", true), bot(), &[]));
        assert!(!is_candidate(&direct, bot(), &[GUILD]));
    }

    #[test]
    fn non_regular_messages_are_not_candidates() {
        let mut pinned = message("", false);
        pinned.kind = MessageType::ChannelMessagePinned;
        let mut reply = message("hi", true);
        reply.kind = MessageType::Reply;

        assert!(!is_candidate(&pinned, bot(), &[GUILD]));
        assert!(is_candidate(&reply, bot(), &[GUILD]));
    }

    #[test]
    fn mention_in_a_plain_channel_starts_a_thread() {
        let text = channel(ChannelType::GuildText);

        assert_eq!(
            decide(&message("hi", true), bot(), &text, None),
            Trigger::StartThread
        );
        assert_eq!(
            decide(&message("hi", false), bot(), &text, None),
            Trigger::Ignore
        );
    }

    #[test]
    fn mention_everyone_alone_does_not_count_as_a_mention() {
        let mut everyone = message("@everyone hi", false);
        everyone.mention_everyone = true;

        assert_eq!(
            decide(&everyone, bot(), &channel(ChannelType::GuildText), None),
            Trigger::Ignore
        );
    }

    #[test]
    fn locked_threads_are_ignored_even_when_mentioned() {
        let mut locked = channel(ChannelType::PublicThread);
        locked.locked = true;
        locked.owner_id = Some(bot());

        assert_eq!(
            decide(&message("hi", true), bot(), &locked, Some(true)),
            Trigger::Ignore
        );
    }

    #[test]
    fn threads_owned_by_the_bot_or_registered_continue() {
        let mut owned = channel(ChannelType::PublicThread);
        owned.owner_id = Some(bot());
        let other = channel(ChannelType::PrivateThread);

        assert_eq!(
            decide(&message("hi", false), bot(), &owned, None),
            Trigger::Continue { register: false }
        );
        assert_eq!(
            decide(&message("hi", false), bot(), &other, Some(true)),
            Trigger::Continue { register: false }
        );
    }

    #[test]
    fn mention_in_an_unregistered_thread_continues_and_registers() {
        for registry in [None, Some(false)] {
            assert_eq!(
                decide(
                    &message("hi", true),
                    bot(),
                    &channel(ChannelType::AnnouncementThread),
                    registry
                ),
                Trigger::Continue { register: true }
            );
        }
    }

    #[test]
    fn unchecked_thread_without_mention_needs_a_history_probe() {
        let thread = channel(ChannelType::PublicThread);

        assert_eq!(
            decide(&message("hi", false), bot(), &thread, None),
            Trigger::ProbeHistory
        );
        assert_eq!(
            decide(&message("hi", false), bot(), &thread, Some(false)),
            Trigger::Ignore
        );
    }
}
