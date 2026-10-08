//! Turns thread history into the chat messages sent to the model.
//!
//! Everything here is pure: the orchestrator fetches the messages and passes
//! them in, so the rules can be tested without Discord.

use crate::{
    attachments::ImageOutcome,
    llm::{ChatMessage, Content, Part, Role},
    reply::{append_part, without_cut_note, without_footer},
};
use std::collections::HashMap;
use twilight_model::{
    channel::{
        Attachment, Message,
        message::{Mention, MessageType},
    },
    id::{
        Id,
        marker::{AttachmentMarker, MessageMarker, UserMarker},
    },
};

const TRUNCATION_MARKER: &str = " …(truncated)";
const IMAGE_MIME_TYPES: [&str; 4] = ["image/png", "image/jpeg", "image/webp", "image/gif"];
const IMAGE_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "webp", "gif"];

/// What [`build_context`] reads.
pub struct ContextInput<'a> {
    /// Thread messages as returned by Discord, newest first.
    pub history_newest_first: &'a [Message],
    /// The parent-channel message the thread was started from, if any.
    pub starter: Option<&'a Message>,
    pub bot_id: Id<UserMarker>,
    /// Character budget for all messages; the system prompt and images do not count.
    pub max_chars: usize,
    /// Downloaded or rejected images; image attachments without an entry
    /// are rendered as plain placeholders.
    pub images: &'a HashMap<Id<AttachmentMarker>, ImageOutcome>,
}

/// The conversation to send, oldest first, and the message to reply to.
#[derive(Debug, Clone, PartialEq)]
pub struct BuiltContext {
    pub messages: Vec<ChatMessage>,
    /// The newest user message; it is always the last of `messages`.
    pub reply_to: Id<MessageMarker>,
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum ContextError {
    #[error("the thread has no user message to answer")]
    NoUserMessage,
}

/// A message after filtering, before trimming.
struct Entry {
    role: Role,
    /// `"{display_name}: "` on user messages; the bot's own answers have none.
    label: String,
    content: String,
    /// `[image: …]` / `[attachment: …]` lines, rendered after the content.
    attachments: Vec<String>,
    images: Vec<Part>,
    id: Id<MessageMarker>,
}

impl Entry {
    /// The label and the content, followed by one line per attachment.
    fn text(&self) -> String {
        let body: Vec<&str> = (!self.content.is_empty())
            .then_some(self.content.as_str())
            .into_iter()
            .chain(self.attachments.iter().map(String::as_str))
            .collect();
        format!("{}{}", self.label, body.join("\n"))
    }

    /// The length of [`Entry::text`], without building it.
    fn chars(&self) -> usize {
        let lines = usize::from(!self.content.is_empty()) + self.attachments.len();
        self.label.chars().count()
            + self.content.chars().count()
            + self
                .attachments
                .iter()
                .map(|line| line.chars().count())
                .sum::<usize>()
            + lines.saturating_sub(1)
    }
}

/// The starter (unless the history already has it) followed by the history, oldest first.
fn chronological<'a>(
    history_newest_first: &'a [Message],
    starter: Option<&'a Message>,
) -> impl Iterator<Item = &'a Message> + Clone {
    let starter = starter.filter(|starter| history_newest_first.iter().all(|m| m.id != starter.id));
    starter.into_iter().chain(history_newest_first.iter().rev())
}

/// The user messages that [`build_context`] keeps, newest first, before
/// trimming. Images are selected from these.
pub fn user_messages_newest_first<'a>(
    history_newest_first: &'a [Message],
    starter: Option<&'a Message>,
    bot_id: Id<UserMarker>,
) -> Vec<&'a Message> {
    let no_names = HashMap::new();
    let mut messages: Vec<_> = chronological(history_newest_first, starter)
        .filter(|message| {
            to_entry(message, bot_id, &no_names, &HashMap::new())
                .is_some_and(|entry| entry.role == Role::User)
        })
        .collect();
    messages.reverse();
    messages
}

/// Builds the chat messages for one turn.
pub fn build_context(input: ContextInput<'_>) -> Result<BuiltContext, ContextError> {
    let chronological = chronological(input.history_newest_first, input.starter);
    let speakers: HashMap<_, _> = chronological
        .clone()
        .map(|message| (message.author.id, display_name(message)))
        .collect();

    let mut entries: Vec<Entry> = Vec::new();
    for message in chronological {
        let Some(entry) = to_entry(message, input.bot_id, &speakers, input.images) else {
            continue;
        };
        match entries.last_mut() {
            Some(last) if last.role == Role::Assistant && entry.role == Role::Assistant => {
                append_part(&mut last.content, &entry.content);
            }
            _ => entries.push(entry),
        }
    }

    let newest_user = entries
        .iter()
        .rposition(|entry| entry.role == Role::User)
        .ok_or(ContextError::NoUserMessage)?;
    let reply_to = entries[newest_user].id;
    entries.truncate(newest_user + 1);

    Ok(BuiltContext {
        messages: trim_to_budget(entries, input.max_chars)
            .into_iter()
            .map(|entry| ChatMessage {
                role: entry.role,
                content: if entry.images.is_empty() {
                    Content::Text(entry.text())
                } else {
                    Content::Parts(
                        std::iter::once(Part::Text(entry.text()))
                            .chain(entry.images)
                            .collect(),
                    )
                },
            })
            .collect(),
        reply_to,
    })
}

fn to_entry(
    message: &Message,
    bot_id: Id<UserMarker>,
    speakers: &HashMap<Id<UserMarker>, &str>,
    images: &HashMap<Id<AttachmentMarker>, ImageOutcome>,
) -> Option<Entry> {
    let author = &message.author;
    if author.system == Some(true)
        || (author.bot && author.id != bot_id)
        || !matches!(message.kind, MessageType::Regular | MessageType::Reply)
    {
        return None;
    }

    if author.id == bot_id {
        let text = without_footer(without_cut_note(&message.content));
        return (!text.is_empty()).then(|| Entry {
            role: Role::Assistant,
            label: String::new(),
            content: text.to_string(),
            attachments: Vec::new(),
            images: Vec::new(),
            id: message.id,
        });
    }

    let content = strip_mentions(&message.content, bot_id, &message.mentions, speakers);
    if content.is_empty() && message.attachments.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    let attachments = message
        .attachments
        .iter()
        .map(|attachment| match images.get(&attachment.id) {
            Some(ImageOutcome::Fetched { mime, base64 }) => {
                parts.push(Part::ImageDataUrl {
                    mime: (*mime).to_string(),
                    base64: base64.clone(),
                });
                attachment_placeholder(attachment)
            }
            Some(ImageOutcome::TooLarge) => {
                format!("[image: {} (omitted: too large)]", attachment.filename)
            }
            Some(ImageOutcome::Unreadable) => {
                format!("[image: {} (omitted: unreadable)]", attachment.filename)
            }
            Some(ImageOutcome::DownloadFailed) => {
                format!(
                    "[image: {} (omitted: download failed)]",
                    attachment.filename
                )
            }
            None => attachment_placeholder(attachment),
        })
        .collect();
    Some(Entry {
        role: Role::User,
        label: format!("{}: ", display_name(message)),
        content,
        attachments,
        images: parts,
        id: message.id,
    })
}

/// Keeps the newest entries that fit in `max_chars`, oldest first and
/// starting with a user message, since some providers reject a conversation
/// that starts with the assistant. Only the content of the newest entry is
/// cut; its label and attachment lines are always kept.
fn trim_to_budget(mut entries: Vec<Entry>, max_chars: usize) -> Vec<Entry> {
    let Some(mut newest) = entries.pop() else {
        return entries;
    };
    let newest_chars = newest.chars();
    let content_chars = newest.content.chars().count();
    // The label and attachment lines are kept even when they alone go over
    // the budget: Discord caps both, so the excess is bounded, and without
    // them the model loses who spoke and which images are whose.
    let room = max_chars.saturating_sub(newest_chars - content_chars);
    if content_chars > room {
        let kept: String = newest.content.chars().take(room).collect();
        newest.content = format!("{kept}{TRUNCATION_MARKER}")
            .trim_start()
            .to_string();
    }

    let mut used = newest_chars.min(max_chars);
    let mut kept = vec![newest];
    while let Some(entry) = entries.pop() {
        let chars = entry.chars();
        if used + chars > max_chars {
            break;
        }
        used += chars;
        kept.push(entry);
    }
    kept.reverse();
    let first_user = kept
        .iter()
        .position(|entry| entry.role == Role::User)
        .unwrap_or(0);
    kept.drain(..first_user);
    kept
}

/// The name shown for the author: server nickname, then global name, then username.
pub fn display_name(message: &Message) -> &str {
    message
        .member
        .as_ref()
        .and_then(|member| member.nick.as_deref())
        .or(message.author.global_name.as_deref())
        .unwrap_or(&message.author.name)
}

/// Removes mentions of the bot and replaces other user mentions with `@name`,
/// using the name from `speakers` so that a user is called the same in their
/// own messages and in mentions.
/// Mentions of users missing from `mentions`, channels, and roles are kept as is.
pub fn strip_mentions(
    content: &str,
    bot_id: Id<UserMarker>,
    mentions: &[Mention],
    speakers: &HashMap<Id<UserMarker>, &str>,
) -> String {
    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(start) = rest.find("<@") {
        out.push_str(&rest[..start]);
        let candidate = &rest[start..];
        match parse_user_mention(candidate) {
            Some((id, len)) if id == bot_id => {
                rest = &candidate[len..];
                if out.is_empty() || out.ends_with(' ') {
                    rest = rest.trim_start_matches(' ');
                }
            }
            Some((id, len)) => {
                match mentions.iter().find(|mention| mention.id == id) {
                    Some(mention) => {
                        out.push('@');
                        out.push_str(speakers.get(&id).copied().unwrap_or(mention_name(mention)));
                    }
                    None => out.push_str(&candidate[..len]),
                }
                rest = &candidate[len..];
            }
            None => {
                out.push_str("<@");
                rest = &candidate[2..];
            }
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

/// Parses a leading `<@ID>` or `<@!ID>`, returning the id and the length consumed.
fn parse_user_mention(s: &str) -> Option<(Id<UserMarker>, usize)> {
    let after = s.strip_prefix("<@")?;
    let (digits, prefix_len) = match after.strip_prefix('!') {
        Some(digits) => (digits, 3),
        None => (after, 2),
    };
    let end = digits.find('>')?;
    let id = Id::new_checked(digits[..end].parse().ok()?)?;
    Some((id, prefix_len + end + 1))
}

// twilight's `Mention` drops `global_name`, so a user who has not spoken in
// the thread cannot be named the way `display_name` would.
fn mention_name(mention: &Mention) -> &str {
    mention
        .member
        .as_ref()
        .and_then(|member| member.nick.as_deref())
        .unwrap_or(&mention.name)
}

/// The text line that stands in for an attachment.
pub fn attachment_placeholder(attachment: &Attachment) -> String {
    let kind = if is_image_attachment(attachment) {
        "image"
    } else {
        "attachment"
    };
    format!("[{kind}: {}]", attachment.filename)
}

/// Whether the attachment is a PNG, JPEG, WebP, or GIF image, judged by its
/// content type, or by its extension when Discord reports none.
pub fn is_image_attachment(attachment: &Attachment) -> bool {
    match &attachment.content_type {
        Some(content_type) => {
            let mime = content_type.split(';').next().unwrap_or_default().trim();
            IMAGE_MIME_TYPES
                .iter()
                .any(|image| image.eq_ignore_ascii_case(mime))
        }
        None => attachment
            .filename
            .rsplit_once('.')
            .is_some_and(|(_, extension)| {
                IMAGE_EXTENSIONS
                    .iter()
                    .any(|image| image.eq_ignore_ascii_case(extension))
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const BOT: u64 = 1000;
    const ALICE: u64 = 2000;
    const BOB: u64 = 3000;

    fn user_json(id: u64, name: &str) -> Value {
        json!({"id": id.to_string(), "username": name, "discriminator": "0", "avatar": null})
    }

    /// A regular message with the fields `build_context` reads; adjust the rest per test.
    fn message(id: u64, author_id: u64, content: &str) -> Message {
        let name = match author_id {
            BOT => "pythia",
            ALICE => "alice",
            BOB => "bob",
            _ => "someone",
        };
        let mut author = user_json(author_id, name);
        if author_id == BOT {
            author["bot"] = json!(true);
        }
        serde_json::from_value(json!({
            "id": id.to_string(),
            "channel_id": "1",
            "author": author,
            "content": content,
            "timestamp": "2026-10-02T14:40:00.000000+00:00",
            "edited_timestamp": null,
            "tts": false,
            "mention_everyone": false,
            "mentions": [],
            "mention_roles": [],
            "attachments": [],
            "embeds": [],
            "pinned": false,
            "type": 0,
        }))
        .unwrap()
    }

    fn mention(id: u64, name: &str, nick: Option<&str>) -> Mention {
        let mut value = user_json(id, name);
        value["public_flags"] = json!(0);
        if let Some(nick) = nick {
            value["member"] = json!({
                "deaf": false, "mute": false, "flags": 0, "joined_at": null,
                "communication_disabled_until": null, "roles": [], "nick": nick,
            });
        }
        serde_json::from_value(value).unwrap()
    }

    fn attachment(filename: &str, content_type: Option<&str>) -> Attachment {
        serde_json::from_value(json!({
            "id": "1",
            "filename": filename,
            "content_type": content_type,
            "size": 1024,
            "url": "https://cdn.discordapp.com/attachments/1/1/file",
            "proxy_url": "https://media.discordapp.net/attachments/1/1/file",
        }))
        .unwrap()
    }

    fn build(history_newest_first: &[Message], starter: Option<&Message>) -> BuiltContext {
        build_with_budget(history_newest_first, starter, 32_000).unwrap()
    }

    fn build_with_budget(
        history_newest_first: &[Message],
        starter: Option<&Message>,
        max_chars: usize,
    ) -> Result<BuiltContext, ContextError> {
        build_context(ContextInput {
            history_newest_first,
            starter,
            bot_id: Id::new(BOT),
            max_chars,
            images: &HashMap::new(),
        })
    }

    fn build_with_images(
        history_newest_first: &[Message],
        images: &HashMap<Id<AttachmentMarker>, ImageOutcome>,
    ) -> BuiltContext {
        build_context(ContextInput {
            history_newest_first,
            starter: None,
            bot_id: Id::new(BOT),
            max_chars: 32_000,
            images,
        })
        .unwrap()
    }

    fn image_attachment(id: u64, filename: &str) -> Attachment {
        let mut attachment = attachment(filename, Some("image/png"));
        attachment.id = Id::new(id);
        attachment
    }

    #[test]
    fn fetched_image_becomes_an_image_part_after_the_text() {
        let mut with_image = message(1, ALICE, "what is this?");
        with_image.attachments = vec![image_attachment(7, "shot.png")];
        let images = HashMap::from([(
            Id::new(7),
            ImageOutcome::Fetched {
                mime: "image/png",
                base64: "AAAA".to_string(),
            },
        )]);

        let context = build_with_images(&[with_image], &images);

        assert_eq!(
            context.messages,
            [ChatMessage {
                role: Role::User,
                content: Content::Parts(vec![
                    Part::Text("alice: what is this?\n[image: shot.png]".to_string()),
                    Part::ImageDataUrl {
                        mime: "image/png".to_string(),
                        base64: "AAAA".to_string(),
                    },
                ]),
            }]
        );
    }

    #[test]
    fn rejected_images_render_why_they_were_omitted() {
        let mut with_images = message(1, ALICE, "look");
        with_images.attachments = vec![
            image_attachment(7, "big.png"),
            image_attachment(8, "broken.png"),
            image_attachment(10, "corrupt.png"),
            image_attachment(9, "old.png"),
        ];
        let images = HashMap::from([
            (Id::new(7), ImageOutcome::TooLarge),
            (Id::new(8), ImageOutcome::DownloadFailed),
            (Id::new(10), ImageOutcome::Unreadable),
        ]);

        assert_eq!(
            texts(&build_with_images(&[with_images], &images)),
            [(
                Role::User,
                "alice: look\n[image: big.png (omitted: too large)]\n[image: broken.png (omitted: download failed)]\n[image: corrupt.png (omitted: unreadable)]\n[image: old.png]"
            )]
        );
    }

    #[test]
    fn image_candidates_are_the_kept_user_messages_newest_first() {
        let mut other_bot = message(5, BOB, "beep");
        other_bot.author.bot = true;
        let mention_only = message(4, ALICE, "<@1000>");
        let history = [
            other_bot,
            mention_only,
            message(3, BOT, "answer"),
            message(2, ALICE, "second"),
        ];
        let starter = message(1, BOB, "first");

        let ids: Vec<_> = user_messages_newest_first(&history, Some(&starter), Id::new(BOT))
            .iter()
            .map(|m| m.id.get())
            .collect();

        assert_eq!(ids, [2, 1]);
    }

    fn texts(context: &BuiltContext) -> Vec<(Role, &str)> {
        context
            .messages
            .iter()
            .map(|message| match &message.content {
                Content::Text(text) => (message.role, text.as_str()),
                Content::Parts(_) => panic!("unexpected parts: {message:?}"),
            })
            .collect()
    }

    #[test]
    fn history_is_returned_oldest_first_with_bot_messages_as_assistant() {
        let history = [
            message(5, ALICE, "question 3"),
            message(4, BOT, "answer 2"),
            message(3, ALICE, "question 2"),
            message(2, BOT, "answer 1"),
            message(1, ALICE, "question 1"),
        ];

        assert_eq!(
            texts(&build(&history, None)),
            [
                (Role::User, "alice: question 1"),
                (Role::Assistant, "answer 1"),
                (Role::User, "alice: question 2"),
                (Role::Assistant, "answer 2"),
                (Role::User, "alice: question 3"),
            ]
        );
    }

    #[test]
    fn speaker_label_prefers_nick_then_global_name_then_username() {
        let mut nick = message(3, ALICE, "a");
        nick.author.global_name = Some("Alice Global".to_string());
        nick.member = mention(ALICE, "alice", Some("Ali")).member;
        let mut global = message(2, ALICE, "b");
        global.author.global_name = Some("Alice Global".to_string());
        let username = message(1, ALICE, "c");

        assert_eq!(
            texts(&build(&[nick, global, username], None)),
            [
                (Role::User, "alice: c"),
                (Role::User, "Alice Global: b"),
                (Role::User, "Ali: a"),
            ]
        );
    }

    #[test]
    fn bot_mentions_are_removed_and_user_mentions_become_names() {
        let mentions = [
            mention(BOT, "pythia", None),
            mention(BOB, "bob", Some("Bobby")),
        ];

        assert_eq!(
            strip_mentions(
                "  <@1000> ask <@!3000> about <@!1000>it  ",
                Id::new(BOT),
                &mentions,
                &HashMap::new()
            ),
            "ask @Bobby about it"
        );
        assert_eq!(
            strip_mentions(
                "<@3000>",
                Id::new(BOT),
                &[mention(BOB, "bob", None)],
                &HashMap::new()
            ),
            "@bob"
        );
    }

    #[test]
    fn mentioned_user_who_spoke_in_the_thread_keeps_their_speaker_label() {
        let mut from_alice = message(1, ALICE, "hi");
        from_alice.author.global_name = Some("Alice Global".to_string());
        let mut from_bob = message(2, BOB, "<@2000> hello");
        from_bob.mentions = vec![mention(ALICE, "alice", None)];

        assert_eq!(
            texts(&build(&[from_bob, from_alice], None)),
            [
                (Role::User, "Alice Global: hi"),
                (Role::User, "bob: @Alice Global hello"),
            ]
        );
    }

    #[test]
    fn unknown_user_channel_and_role_mentions_are_kept() {
        assert_eq!(
            strip_mentions(
                "<@4000> in <#5> for <@&6> or <@> <@x>",
                Id::new(BOT),
                &[],
                &HashMap::new()
            ),
            "<@4000> in <#5> for <@&6> or <@> <@x>"
        );
    }

    #[test]
    fn consecutive_bot_messages_merge_into_one_assistant_message() {
        let history = [
            message(6, ALICE, "next"),
            message(5, BOT, "part 3"),
            message(4, ALICE, "thanks"),
            message(3, BOT, "part 2"),
            message(2, BOT, "part 1"),
            message(1, ALICE, "question"),
        ];

        assert_eq!(
            texts(&build(&history, None)),
            [
                (Role::User, "alice: question"),
                (Role::Assistant, "part 1\npart 2"),
                (Role::User, "alice: thanks"),
                (Role::Assistant, "part 3"),
                (Role::User, "alice: next"),
            ]
        );
    }

    #[test]
    fn footers_under_earlier_answers_are_not_sent() {
        let history = [
            message(5, ALICE, "next"),
            message(4, BOT, "-# Sources\n-# 1. [A](<https://a.example/>)"),
            message(3, BOT, "more\n\n-# 🔧 1 tool call"),
            message(
                2,
                BOT,
                "answer\n\n-# Sources\n-# 1. [A](<https://a.example/>)",
            ),
            message(1, ALICE, "q"),
        ];

        assert_eq!(
            texts(&build(&history, None)),
            [
                (Role::User, "alice: q"),
                (Role::Assistant, "answer\nmore"),
                (Role::User, "alice: next"),
            ]
        );
    }

    #[test]
    fn bot_message_without_content_is_dropped() {
        let mut error_embed = message(3, BOT, "");
        error_embed.embeds =
            serde_json::from_value(json!([{"type": "rich", "title": "Timed out"}])).unwrap();
        let history = [
            message(5, ALICE, "next"),
            message(4, BOT, "after"),
            error_embed,
            message(2, BOT, "before"),
            message(1, ALICE, "q"),
        ];

        assert_eq!(
            texts(&build(&history, None)),
            [
                (Role::User, "alice: q"),
                (Role::Assistant, "before\nafter"),
                (Role::User, "alice: next"),
            ]
        );
    }

    #[test]
    fn other_bots_system_users_and_non_regular_messages_are_dropped() {
        let mut other_bot = message(4, BOB, "beep");
        other_bot.author.bot = true;
        let mut system = message(3, BOB, "system notice");
        system.author.system = Some(true);
        let mut thread_created = message(2, ALICE, "thread");
        thread_created.kind = MessageType::ThreadCreated;
        let mut reply = message(1, ALICE, "reply");
        reply.kind = MessageType::Reply;

        assert_eq!(
            texts(&build(&[other_bot, system, thread_created, reply], None)),
            [(Role::User, "alice: reply")]
        );
    }

    #[test]
    fn starter_message_comes_first_with_its_bot_mention_stripped() {
        let mut starter = message(10, ALICE, "<@1000> what is rust?");
        starter.mentions = vec![mention(BOT, "pythia", None)];
        let history = [
            message(12, ALICE, "and tokio?"),
            message(11, BOT, "a language"),
        ];

        assert_eq!(
            texts(&build(&history, Some(&starter))),
            [
                (Role::User, "alice: what is rust?"),
                (Role::Assistant, "a language"),
                (Role::User, "alice: and tokio?"),
            ]
        );
    }

    #[test]
    fn starter_message_already_in_history_is_not_duplicated() {
        let starter = message(10, ALICE, "hello");
        let history = [message(10, ALICE, "hello")];

        assert_eq!(
            texts(&build(&history, Some(&starter))),
            [(Role::User, "alice: hello")]
        );
    }

    #[test]
    fn attachments_are_appended_as_placeholder_lines() {
        let mut with_text = message(2, ALICE, "look");
        with_text.attachments = vec![
            attachment("a.png", Some("image/png")),
            attachment("b.webp", Some("image/webp")),
            attachment("c.gif", Some("image/gif")),
            attachment("notes.txt", Some("text/plain; charset=utf-8")),
        ];
        let mut without_text = message(1, ALICE, "<@1000>");
        without_text.attachments = vec![attachment("d.JPG", None)];

        assert_eq!(
            texts(&build(&[with_text, without_text], None)),
            [
                (Role::User, "alice: [image: d.JPG]"),
                (
                    Role::User,
                    "alice: look\n[image: a.png]\n[image: b.webp]\n[image: c.gif]\n[attachment: notes.txt]"
                ),
            ]
        );
    }

    #[test]
    fn image_detection_uses_content_type_before_extension() {
        assert!(is_image_attachment(&attachment(
            "x.bin",
            Some("image/jpeg")
        )));
        assert!(is_image_attachment(&attachment("x.webp", None)));
        assert!(!is_image_attachment(&attachment(
            "x.png",
            Some("application/octet-stream")
        )));
        assert!(!is_image_attachment(&attachment(
            "x.svg",
            Some("image/svg+xml")
        )));
        assert!(!is_image_attachment(&attachment("png", None)));
    }

    #[test]
    fn message_with_only_a_bot_mention_and_no_attachment_is_dropped() {
        let mut mention_only = message(2, ALICE, " <@1000> ");
        mention_only.mentions = vec![mention(BOT, "pythia", None)];

        assert_eq!(
            texts(&build(&[mention_only, message(1, ALICE, "hi")], None)),
            [(Role::User, "alice: hi")]
        );
    }

    #[test]
    fn older_messages_beyond_the_budget_are_dropped() {
        let history = [
            message(3, ALICE, "ccc"),
            message(2, ALICE, "bbb"),
            message(1, ALICE, "aaa"),
        ];
        let two_messages = "alice: bbb".chars().count() * 2;

        let context = build_with_budget(&history, None, two_messages).unwrap();

        assert_eq!(
            texts(&context),
            [(Role::User, "alice: bbb"), (Role::User, "alice: ccc")]
        );
    }

    #[test]
    fn newest_message_over_the_budget_is_truncated_and_kept_alone() {
        let history = [message(2, ALICE, &"x".repeat(43)), message(1, ALICE, "old")];

        let context = build_with_budget(&history, None, 20).unwrap();

        assert_eq!(
            texts(&context),
            [(Role::User, "alice: xxxxxxxxxxxxx …(truncated)")]
        );
    }

    #[test]
    fn truncation_keeps_the_speaker_label_and_attachment_lines() {
        let mut long = message(1, ALICE, &"x".repeat(100));
        long.attachments = vec![image_attachment(7, "shot.png")];
        let images = HashMap::from([(
            Id::new(7),
            ImageOutcome::Fetched {
                mime: "image/png",
                base64: "AAAA".to_string(),
            },
        )]);

        let context = build_context(ContextInput {
            history_newest_first: &[long],
            starter: None,
            bot_id: Id::new(BOT),
            max_chars: 30,
            images: &images,
        })
        .unwrap();

        assert_eq!(
            context.messages,
            [ChatMessage {
                role: Role::User,
                content: Content::Parts(vec![
                    Part::Text("alice: xxxxx …(truncated)\n[image: shot.png]".to_string()),
                    Part::ImageDataUrl {
                        mime: "image/png".to_string(),
                        base64: "AAAA".to_string(),
                    },
                ]),
            }]
        );
    }

    #[test]
    fn attachment_only_message_over_the_budget_is_not_marked_truncated() {
        let mut images_only = message(2, ALICE, "");
        images_only.attachments = vec![image_attachment(7, "shot.png")];
        let history = [images_only, message(1, ALICE, "old")];

        let context = build_with_budget(&history, None, 10).unwrap();

        assert_eq!(texts(&context), [(Role::User, "alice: [image: shot.png]")]);
    }

    #[test]
    fn content_cut_entirely_leaves_only_the_marker_after_the_label() {
        let mut with_image = message(1, ALICE, "question");
        with_image.attachments = vec![image_attachment(7, "shot.png")];

        let context = build_with_budget(&[with_image], None, 10).unwrap();

        assert_eq!(
            texts(&context),
            [(Role::User, "alice: …(truncated)\n[image: shot.png]")]
        );
    }

    #[test]
    fn history_without_user_messages_has_nothing_to_answer() {
        let history = [message(2, BOT, "a"), message(1, BOT, "b")];

        assert_eq!(
            build_with_budget(&history, None, 32_000),
            Err(ContextError::NoUserMessage)
        );
        assert_eq!(
            build_with_budget(&[], None, 32_000),
            Err(ContextError::NoUserMessage)
        );
    }

    #[test]
    fn reply_target_is_the_newest_surviving_user_message() {
        let mut dropped = message(4, ALICE, "<@1000>");
        dropped.mentions = vec![mention(BOT, "pythia", None)];
        let history = [
            dropped,
            message(3, BOT, "answer"),
            message(2, BOB, "newest user"),
            message(1, ALICE, "older user"),
        ];

        assert_eq!(build(&history, None).reply_to, Id::new(2));
    }

    #[test]
    fn bot_messages_after_the_newest_user_message_are_not_sent() {
        let history = [
            message(4, BOT, "answer"),
            message(3, ALICE, "question"),
            message(2, BOT, "greeting"),
            message(1, ALICE, "hello"),
        ];

        assert_eq!(
            texts(&build(&history, None)),
            [
                (Role::User, "alice: hello"),
                (Role::Assistant, "greeting"),
                (Role::User, "alice: question"),
            ]
        );
    }

    #[test]
    fn trimmed_conversation_starts_with_a_user_message() {
        let history = [
            message(3, ALICE, "q2"),
            message(2, BOT, "a1"),
            message(1, ALICE, "q1"),
        ];
        let without_the_first_question = "alice: q2".chars().count() + "a1".chars().count();

        let context = build_with_budget(&history, None, without_the_first_question).unwrap();

        assert_eq!(texts(&context), [(Role::User, "alice: q2")]);
        assert_eq!(context.reply_to, Id::new(3));
    }

    #[test]
    fn split_bot_answer_is_sent_as_one_assistant_message() {
        let code: String = (0..300).map(|i| format!("let x{i} = {i};\n")).collect();
        let answer = format!("Here:\n```rust\n{code}```\nDone.");
        let parts = crate::reply::split_message(&answer, 2000);
        assert!(parts.len() > 1);
        let mut history = vec![message(100, ALICE, "thanks")];
        history.extend(
            parts
                .iter()
                .enumerate()
                .rev()
                .map(|(i, part)| message(10 + i as u64, BOT, part)),
        );
        history.push(message(1, ALICE, "show me code"));

        assert_eq!(
            texts(&build(&history, None)),
            [
                (Role::User, "alice: show me code"),
                (Role::Assistant, answer.as_str()),
                (Role::User, "alice: thanks"),
            ]
        );
    }

    #[test]
    fn max_parts_cut_note_under_a_bot_answer_is_not_sent() {
        let history = [
            message(3, ALICE, "next"),
            message(
                2,
                BOT,
                "answer\n\n(response truncated: too long for Discord)",
            ),
            message(1, ALICE, "q"),
        ];

        assert_eq!(
            texts(&build(&history, None)),
            [
                (Role::User, "alice: q"),
                (Role::Assistant, "answer"),
                (Role::User, "alice: next"),
            ]
        );
    }

    #[test]
    fn oversized_bot_answer_after_the_newest_user_message_does_not_hide_it() {
        let history = [message(2, BOT, &"x".repeat(100)), message(1, ALICE, "q")];

        let context = build_with_budget(&history, None, 20).unwrap();

        assert_eq!(texts(&context), [(Role::User, "alice: q")]);
        assert_eq!(context.reply_to, Id::new(1));
    }

    #[test]
    fn removing_a_bot_mention_mid_sentence_leaves_a_single_space() {
        assert_eq!(
            strip_mentions(
                "ask <@1000> about <@1000>  it",
                Id::new(BOT),
                &[],
                &HashMap::new()
            ),
            "ask about it"
        );
    }
}
