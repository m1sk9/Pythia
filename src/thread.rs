//! Thread naming, the registry of conversation threads, and per-thread turn
//! scheduling.

use jiff::tz::TimeZone;
use std::{collections::HashMap, sync::Mutex};
use twilight_model::{
    id::{Id, marker::ChannelMarker},
    util::Timestamp,
};

/// Discord's limit on channel names.
const MAX_THREAD_NAME_CHARS: usize = 100;

/// `"{display_name} · {YYYY-MM-DD HH:mm}"` in `tz`, capped at Discord's limit.
pub fn thread_name(display_name: &str, timestamp: Timestamp, tz: &TimeZone) -> String {
    let time = jiff::Timestamp::from_microsecond(timestamp.as_micros())
        .map(|t| {
            t.to_zoned(tz.clone())
                .strftime("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default();
    format!("{display_name} · {time}")
        .chars()
        .take(MAX_THREAD_NAME_CHARS)
        .collect()
}

/// Which threads are conversations with Pythia.
///
/// Only an optimisation: after a restart it is rebuilt from thread history.
#[derive(Default)]
pub struct ConversationRegistry {
    threads: Mutex<HashMap<Id<ChannelMarker>, bool>>,
}

impl ConversationRegistry {
    /// `Some(true)` for a conversation, `Some(false)` for a thread known not
    /// to be one, `None` when the thread has not been checked.
    pub fn get(&self, thread: Id<ChannelMarker>) -> Option<bool> {
        self.lock().get(&thread).copied()
    }

    pub fn set(&self, thread: Id<ChannelMarker>, is_conversation: bool) {
        self.lock().insert(thread, is_conversation);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Id<ChannelMarker>, bool>> {
        self.threads.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Runs at most one turn per thread at a time.
///
/// A thread that is absent from the map is idle; a present one is busy, and its
/// value says whether another trigger arrived during the running turn.
#[derive(Default)]
pub struct TurnScheduler {
    busy: Mutex<HashMap<Id<ChannelMarker>, bool>>,
}

impl TurnScheduler {
    /// Returns `true` when the caller should start a turn. When a turn is
    /// already running, records that another one is needed and returns `false`.
    pub fn try_start(&self, thread: Id<ChannelMarker>) -> bool {
        let mut busy = self.lock();
        match busy.get_mut(&thread) {
            Some(pending) => {
                *pending = true;
                false
            }
            None => {
                busy.insert(thread, false);
                true
            }
        }
    }

    /// Called when a turn ends. Returns `true` when another turn must run
    /// because triggers arrived meanwhile; otherwise the thread becomes idle.
    pub fn finish(&self, thread: Id<ChannelMarker>) -> bool {
        let mut busy = self.lock();
        match busy.get_mut(&thread) {
            Some(pending) if *pending => {
                *pending = false;
                true
            }
            _ => {
                busy.remove(&thread);
                false
            }
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Id<ChannelMarker>, bool>> {
        self.busy.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timestamp() -> Timestamp {
        Timestamp::parse("2026-10-02T14:40:00.000000+00:00").unwrap()
    }

    #[test]
    fn thread_name_uses_the_configured_time_zone() {
        assert_eq!(
            thread_name("m1sk9", timestamp(), &TimeZone::get("Asia/Tokyo").unwrap()),
            "m1sk9 · 2026-10-02 23:40"
        );
        assert_eq!(
            thread_name("m1sk9", timestamp(), &TimeZone::UTC),
            "m1sk9 · 2026-10-02 14:40"
        );
    }

    #[test]
    fn thread_name_is_capped_at_100_chars() {
        let name = thread_name(&"あ".repeat(120), timestamp(), &TimeZone::UTC);
        assert_eq!(name.chars().count(), 100);
    }

    #[test]
    fn registry_remembers_the_latest_state_per_thread() {
        let registry = ConversationRegistry::default();
        let thread = Id::new(1);

        assert_eq!(registry.get(thread), None);
        registry.set(thread, false);
        assert_eq!(registry.get(thread), Some(false));
        registry.set(thread, true);
        assert_eq!(registry.get(thread), Some(true));
        assert_eq!(registry.get(Id::new(2)), None);
    }

    #[test]
    fn trigger_on_an_idle_thread_runs_exactly_one_turn() {
        let scheduler = TurnScheduler::default();
        let thread = Id::new(1);

        assert!(scheduler.try_start(thread));
        assert!(!scheduler.finish(thread));
        assert!(scheduler.try_start(thread), "thread must be idle again");
    }

    #[test]
    fn triggers_during_a_turn_coalesce_into_one_extra_turn() {
        let scheduler = TurnScheduler::default();
        let thread = Id::new(1);

        assert!(scheduler.try_start(thread));
        assert!(!scheduler.try_start(thread));
        assert!(!scheduler.try_start(thread));
        assert!(scheduler.finish(thread), "one extra turn");
        assert!(
            !scheduler.try_start(thread),
            "trigger during the extra turn"
        );
        assert!(scheduler.finish(thread), "another turn");
        assert!(!scheduler.finish(thread), "idle afterwards");
        assert!(scheduler.try_start(thread));
    }

    #[test]
    fn threads_are_scheduled_independently() {
        let scheduler = TurnScheduler::default();

        assert!(scheduler.try_start(Id::new(1)));
        assert!(scheduler.try_start(Id::new(2)));
    }
}
