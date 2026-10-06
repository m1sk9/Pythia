//! One conversation turn: history → context → LLM → reply.

use crate::{
    attachments::{self, ImagePolicy},
    config::PythiaConfig,
    context::{self, ContextError, ContextInput},
    llm::{ChatRequest, ServerTool, openrouter::OpenRouterClient},
    reply,
    thread::{ConversationRegistry, TurnScheduler},
};
use jiff::tz::TimeZone;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::Semaphore, task::JoinHandle};
use tracing::Instrument as _;
use twilight_cache_inmemory::DefaultInMemoryCache;
use twilight_model::{
    channel::Message,
    id::{
        Id,
        marker::{ChannelMarker, UserMarker},
    },
};

/// The typing indicator lasts 10 s; refresh it a little earlier.
const TYPING_INTERVAL: Duration = Duration::from_secs(8);

/// Everything a turn needs, shared by every task.
pub struct AppState {
    pub http: twilight_http::Client,
    /// Shared with the LLM client; used here to download images.
    pub web: reqwest::Client,
    pub images: ImagePolicy,
    /// Server tools sent with every request; empty when disabled.
    pub tools: Vec<ServerTool>,
    pub cache: DefaultInMemoryCache,
    pub llm: OpenRouterClient,
    pub config: &'static PythiaConfig,
    pub timezone: TimeZone,
    pub registry: ConversationRegistry,
    pub scheduler: TurnScheduler,
    /// Bounds concurrent turns across all threads (`limits.max_concurrent`).
    pub semaphore: Semaphore,
}

/// Starts answering in `thread` unless a turn is already running there, in
/// which case that turn is followed by one more covering the new messages.
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn schedule_turns(
    state: Arc<AppState>,
    bot_id: Id<UserMarker>,
    thread: Id<ChannelMarker>,
    parent: Option<Id<ChannelMarker>>,
) {
    if !state.scheduler.try_start(thread) {
        return;
    }
    tokio::spawn(async move {
        loop {
            // A separate task so that a panicking turn cannot leave the thread
            // marked busy forever.
            let turn = tokio::spawn(
                run_turn(Arc::clone(&state), bot_id, thread, parent)
                    .instrument(tracing::info_span!("turn", thread_id = %thread)),
            );
            if let Err(error) = turn.await {
                tracing::error!(%error, thread_id = %thread, "turn task failed");
            }
            if !state.scheduler.finish(thread) {
                break;
            }
        }
    });
}

/// Aborts the wrapped task when dropped.
struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn run_turn(
    state: Arc<AppState>,
    bot_id: Id<UserMarker>,
    thread: Id<ChannelMarker>,
    parent: Option<Id<ChannelMarker>>,
) {
    let Ok(_permit) = state.semaphore.acquire().await else {
        return;
    };
    let started = Instant::now();
    let _typing = AbortOnDrop(tokio::spawn(keep_typing(Arc::clone(&state), thread)));

    let history = match fetch_history(&state, thread).await {
        Ok(history) => history,
        Err(error) => {
            tracing::error!(
                error = format!("{error:#}"),
                "failed to fetch thread history"
            );
            return;
        }
    };
    let starter = match parent {
        Some(parent) => fetch_starter(&state, parent, thread).await,
        None => None,
    };

    let user_messages = context::user_messages_newest_first(&history, starter.as_ref(), bot_id);
    let plan = attachments::select_images(&user_messages, &state.images);
    let images = attachments::fetch_images(&state.web, &plan, state.images.max_image_bytes).await;
    tracing::debug!(
        selected = plan.fetch.len(),
        too_large = plan.too_large.len(),
        fetched = images
            .values()
            .filter(|outcome| matches!(outcome, attachments::ImageOutcome::Fetched { .. }))
            .count(),
        "images"
    );

    let built = match context::build_context(ContextInput {
        history_newest_first: &history,
        starter: starter.as_ref(),
        bot_id,
        max_chars: state.config.context.max_chars,
        images: &images,
    }) {
        Ok(built) => built,
        Err(ContextError::NoUserMessage) => {
            tracing::debug!("no user message to answer");
            return;
        }
    };

    // The starter message lives in the parent channel, and a reply cannot
    // point to another channel.
    let reply_to = Some(built.reply_to).filter(|&id| id != thread.cast());
    let request = ChatRequest {
        messages: built.messages,
        max_output_tokens: state.config.llm.max_output_tokens,
        tools: state.tools.clone(),
    };
    let posted = match state.llm.chat(&request).await {
        Ok(response) => {
            let usage = response.usage.as_ref();
            tracing::info!(
                model = %response.model,
                reply_to = %built.reply_to,
                prompt_tokens = usage.map(|u| u.prompt_tokens),
                completion_tokens = usage.map(|u| u.completion_tokens),
                cost = usage.and_then(|u| u.cost),
                web_search_requests = usage.and_then(|u| u.web_search_requests),
                server_tool_calls = usage.and_then(|u| u.server_tool_calls),
                citations = response.citations.len(),
                finish = ?response.finish,
                elapsed_ms = started.elapsed().as_millis(),
                "answered"
            );
            let parts = reply::answer_parts(&response, state.config.response.max_parts);
            reply::post_answer(&state.http, thread, reply_to, &parts).await
        }
        Err(error) => {
            let details = error.details();
            tracing::warn!(
                %error,
                model = details.model,
                status = details.status,
                retries = details.retries,
                request_id = details.request_id,
                reply_to = %built.reply_to,
                elapsed_ms = started.elapsed().as_millis(),
                "turn failed"
            );
            let embed = reply::error_embed(&error);
            reply::post_embed(&state.http, thread, reply_to, embed).await
        }
    };
    if let Err(error) = posted {
        tracing::error!(%error, "failed to post to Discord");
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn keep_typing(state: Arc<AppState>, thread: Id<ChannelMarker>) {
    let mut interval = tokio::time::interval(TYPING_INTERVAL);
    loop {
        interval.tick().await;
        if let Err(error) = state.http.create_typing_trigger(thread).await {
            tracing::debug!(%error, "failed to trigger typing indicator");
        }
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn fetch_history(
    state: &AppState,
    thread: Id<ChannelMarker>,
) -> anyhow::Result<Vec<Message>> {
    Ok(state
        .http
        .channel_messages(thread)
        .limit(u16::from(state.config.context.max_messages))
        .await?
        .models()
        .await?)
}

/// The message a thread was started from shares the thread's id. Threads not
/// started from a message, and starters that were deleted or are not
/// readable, yield `None`.
#[cfg_attr(coverage_nightly, coverage(off))]
async fn fetch_starter(
    state: &AppState,
    parent: Id<ChannelMarker>,
    thread: Id<ChannelMarker>,
) -> Option<Message> {
    let fetched = async {
        anyhow::Ok(
            state
                .http
                .message(parent, thread.cast())
                .await?
                .model()
                .await?,
        )
    }
    .await;
    match fetched {
        Ok(message) => Some(message),
        Err(error) => {
            tracing::debug!(error = format!("{error:#}"), "no starter message");
            None
        }
    }
}
