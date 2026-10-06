//! Posting answers and errors to Discord.
//!
//! Splitting and embed building are pure; only `post_*` touch the network.

use crate::llm::{ChatResponse, Citation, Finish, LlmError};
use twilight_model::{
    channel::message::{AllowedMentions, Embed},
    id::{
        Id,
        marker::{ChannelMarker, MessageMarker},
    },
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder};

/// Discord's limit on message content, in characters.
pub const MESSAGE_LIMIT: usize = 2000;
const OUTPUT_TRUNCATED_NOTE: &str = "\n\n(output truncated: max_output_tokens reached)";
const RESPONSE_TRUNCATED_NOTE: &str = "\n\n(response truncated: too long for Discord)";
/// First line of the source list under an answer; also how the context
/// recognises the list in earlier answers.
pub const SOURCES_HEADING: &str = "-# Sources";
const MAX_SOURCES: usize = 5;
const MAX_SOURCE_TITLE_CHARS: usize = 100;
const FENCE: &str = "```";
const CLOSE_FENCE: &str = "\n```";
/// Longer info strings are not repeated when a fence is reopened, so that the
/// reopening line can never eat a whole part.
const MAX_REOPENED_INFO_CHARS: usize = 32;
const ERROR_COLOR: u32 = 0xED4245;
/// Discord's limit on an embed field value.
const MAX_FIELD_CHARS: usize = 1024;

/// Splits `text` into parts of at most `limit` characters, preferring
/// paragraph breaks in the second half of the part, then line breaks, then
/// spaces. A code block open at a split is closed at the end of
/// the part and reopened, with the same info string, at the start of the next.
pub fn split_message(text: &str, limit: usize) -> Vec<String> {
    let mut parts = Vec::new();
    let mut rest = text.to_string();
    loop {
        if rest.chars().count() <= limit {
            parts.push(rest);
            return parts;
        }

        let window_end = byte_index_of_char(&rest, limit.saturating_sub(CLOSE_FENCE.len()).max(1));
        let window = &rest[..window_end];
        // An earlier paragraph break would leave short parts and reach
        // `response.max_parts` sooner than needed.
        let paragraph = window
            .rfind("\n\n")
            .filter(|&i| i >= window.len() / 2)
            .map(|i| (i, 2));
        let line_or_space = || {
            window
                .rfind('\n')
                .filter(|&i| i > 0)
                .or_else(|| window.rfind(' ').filter(|&i| i > 0))
                .map(|i| (i, 1))
        };
        let (cut, skip) = paragraph.or_else(line_or_space).unwrap_or((window_end, 0));

        let mut part = rest[..cut].to_string();
        let tail = &rest[cut + skip..];
        rest = match open_fence_info(&part).map(str::to_string) {
            Some(info) => {
                part.push_str(CLOSE_FENCE);
                let info = if info.chars().count() <= MAX_REOPENED_INFO_CHARS {
                    info.as_str()
                } else {
                    ""
                };
                format!("{FENCE}{info}\n{tail}")
            }
            None => tail.to_string(),
        };
        parts.push(part);
    }
}

fn byte_index_of_char(s: &str, chars: usize) -> usize {
    s.char_indices().nth(chars).map_or(s.len(), |(i, _)| i)
}

/// The info string of a code block left open at the end of `text`.
fn open_fence_info(text: &str) -> Option<&str> {
    let mut open = None;
    for line in text.lines() {
        if let Some(info) = line.trim_start().strip_prefix(FENCE) {
            open = match open {
                None => Some(info.trim()),
                Some(_) => None,
            };
        }
    }
    open
}

/// The messages to post for an answer: the truncation note when the model hit
/// `max_output_tokens` and the cited sources, split into at most `max_parts`
/// messages.
pub fn answer_parts(response: &ChatResponse, max_parts: usize) -> Vec<String> {
    let mut text = response.text.clone();
    if response.finish == Finish::Length {
        text.push_str(OUTPUT_TRUNCATED_NOTE);
    }
    if let Some(sources) = sources_list(&response.citations) {
        text.push_str("\n\n");
        text.push_str(&sources);
    }

    let mut parts = split_message(&text, MESSAGE_LIMIT);
    if parts.len() > max_parts {
        parts.truncate(max_parts);
        if let Some(last) = parts.last_mut() {
            let room = MESSAGE_LIMIT - RESPONSE_TRUNCATED_NOTE.chars().count();
            let mut kept = split_message(last, room).swap_remove(0);
            kept.push_str(RESPONSE_TRUNCATED_NOTE);
            *last = kept;
        }
    }
    parts
}

/// A small-text list of the first [`MAX_SOURCES`] http(s) citations, linked
/// in angle brackets so that Discord does not embed every page.
fn sources_list(citations: &[Citation]) -> Option<String> {
    let lines: Vec<String> = citations
        .iter()
        .filter_map(|citation| {
            let url = reqwest::Url::parse(&citation.url).ok()?;
            matches!(url.scheme(), "http" | "https").then_some((citation, url))
        })
        .take(MAX_SOURCES)
        .enumerate()
        .map(|(i, (citation, url))| {
            let label = match &citation.title {
                Some(title) => escape_link_text(title),
                None => url.host_str().unwrap_or(url.as_str()).to_string(),
            };
            format!("-# {}. [{label}](<{url}>)", i + 1)
        })
        .collect();
    (!lines.is_empty()).then(|| format!("{SOURCES_HEADING}\n{}", lines.join("\n")))
}

/// Keeps a title on one line and inside the brackets of a markdown link.
fn escape_link_text(title: &str) -> String {
    let one_line = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = one_line.chars();
    let mut escaped = String::new();
    for c in chars.by_ref().take(MAX_SOURCE_TITLE_CHARS) {
        if matches!(c, '[' | ']' | '\\') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    if chars.next().is_some() {
        escaped.push('…');
    }
    escaped
}

/// The embed shown when a turn fails. Holds nothing that is not already
/// visible to the provider's users: no API key, paths, or backtraces.
pub fn error_embed(error: &LlmError) -> Embed {
    let (title, description) = match error {
        LlmError::Timeout { .. } => (
            "Timed out",
            "The model did not answer in time. Try again later.",
        ),
        LlmError::RateLimited { .. } => (
            "Rate limited",
            "The provider is rate limiting requests. Try again in a moment.",
        ),
        LlmError::Provider { .. } => ("Provider error", "The provider rejected the request."),
        LlmError::Transport { .. } => ("Connection failed", "Pythia could not reach the provider."),
        LlmError::ContentFilter { .. } => (
            "Content filtered",
            "The model refused to answer because of a content filter.",
        ),
        LlmError::OutputTokenLimit { .. } => (
            "Output token limit reached",
            "The model used up `max_output_tokens` before writing an answer.",
        ),
        LlmError::EmptyResponse { .. } => ("Empty response", "The model returned no text."),
        LlmError::InResponse { .. } => (
            "Generation failed",
            "The provider reported an error while generating the answer.",
        ),
        LlmError::Decode { .. } => (
            "Unexpected response",
            "The provider's response could not be read.",
        ),
    };

    let details = error.details();
    let mut fields = Vec::new();
    if let Some(status) = details.status {
        fields.push(("Status", status.to_string(), true));
    }
    fields.push(("Model", details.model.to_string(), true));
    fields.push(("Retries", details.retries.to_string(), true));
    if let Some(request_id) = details.request_id {
        fields.push(("Request ID", request_id.to_string(), false));
    }
    if let Some(message) = details.provider_message {
        fields.push(("Provider message", truncate_field(message), false));
    }

    fields
        .into_iter()
        .fold(
            EmbedBuilder::new()
                .title(title)
                .description(description)
                .color(ERROR_COLOR),
            |embed, (name, value, inline)| {
                let field = EmbedFieldBuilder::new(name, value);
                embed.field(if inline { field.inline() } else { field })
            },
        )
        .build()
}

/// An embed for failures outside the model call, e.g. thread creation.
pub fn notice_embed(title: &str, description: &str) -> Embed {
    EmbedBuilder::new()
        .title(title)
        .description(description)
        .color(ERROR_COLOR)
        .build()
}

fn truncate_field(value: &str) -> String {
    if value.chars().count() <= MAX_FIELD_CHARS {
        return value.to_string();
    }
    let mut truncated: String = value.chars().take(MAX_FIELD_CHARS - 1).collect();
    truncated.push('…');
    truncated
}

/// Posts `parts` in order; the first replies to `reply_to` when given.
/// Mentions in model output never ping anyone.
#[cfg_attr(coverage_nightly, coverage(off))]
pub async fn post_answer(
    http: &twilight_http::Client,
    channel: Id<ChannelMarker>,
    reply_to: Option<Id<MessageMarker>>,
    parts: &[String],
) -> Result<(), twilight_http::Error> {
    let no_pings = AllowedMentions::default();
    for (i, part) in parts.iter().enumerate() {
        let request = http
            .create_message(channel)
            .content(part)
            .allowed_mentions(Some(&no_pings));
        match reply_to.filter(|_| i == 0) {
            Some(reply_to) => request.reply(reply_to).fail_if_not_exists(false).await?,
            None => request.await?,
        };
    }
    Ok(())
}

/// Posts `embed`, replying to `reply_to` when given.
#[cfg_attr(coverage_nightly, coverage(off))]
pub async fn post_embed(
    http: &twilight_http::Client,
    channel: Id<ChannelMarker>,
    reply_to: Option<Id<MessageMarker>>,
    embed: Embed,
) -> Result<(), twilight_http::Error> {
    let no_pings = AllowedMentions::default();
    let embeds = [embed];
    let request = http
        .create_message(channel)
        .embeds(&embeds)
        .allowed_mentions(Some(&no_pings));
    match reply_to {
        Some(reply_to) => request.reply(reply_to).fail_if_not_exists(false).await?,
        None => request.await?,
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(text: &str, finish: Finish) -> ChatResponse {
        ChatResponse {
            text: text.to_string(),
            finish,
            model: "test/model".to_string(),
            request_id: None,
            usage: None,
            citations: Vec::new(),
        }
    }

    fn cited(text: &str, citations: &[(&str, Option<&str>)]) -> ChatResponse {
        ChatResponse {
            citations: citations
                .iter()
                .map(|(url, title)| Citation {
                    url: url.to_string(),
                    title: title.map(str::to_string),
                })
                .collect(),
            ..response(text, Finish::Stop)
        }
    }

    fn chars(s: &str) -> usize {
        s.chars().count()
    }

    #[test]
    fn text_within_the_limit_is_one_part() {
        let text = "a".repeat(2000);
        assert_eq!(split_message(&text, 2000), [text]);
    }

    #[test]
    fn text_over_the_limit_splits_at_the_last_line_break() {
        let text = format!("{}\n{}", "a".repeat(1500), "b".repeat(501));

        assert_eq!(
            split_message(&text, 2000),
            ["a".repeat(1500), "b".repeat(501)]
        );
    }

    #[test]
    fn paragraph_break_is_preferred_over_a_later_line_break() {
        let text = format!(
            "{}\n\n{}\n{}",
            "a".repeat(1000),
            "b".repeat(500),
            "c".repeat(600)
        );

        assert_eq!(
            split_message(&text, 2000),
            [
                "a".repeat(1000),
                format!("{}\n{}", "b".repeat(500), "c".repeat(600))
            ]
        );
    }

    #[test]
    fn paragraph_break_in_the_first_half_falls_back_to_the_last_line_break() {
        let text = format!(
            "{}\n\n{}\n{}",
            "a".repeat(300),
            "b".repeat(1500),
            "c".repeat(300)
        );

        assert_eq!(
            split_message(&text, 2000),
            [
                format!("{}\n\n{}", "a".repeat(300), "b".repeat(1500)),
                "c".repeat(300)
            ]
        );
    }

    #[test]
    fn text_without_line_breaks_splits_at_a_space_then_anywhere() {
        let spaced = format!("{} {}", "a".repeat(1500), "b".repeat(600));
        assert_eq!(
            split_message(&spaced, 2000),
            ["a".repeat(1500), "b".repeat(600)]
        );

        let solid = "a".repeat(4500);
        let parts = split_message(&solid, 2000);
        assert_eq!(parts.concat(), solid);
        assert!(parts.iter().all(|p| chars(p) <= 2000), "{parts:?}");
    }

    #[test]
    fn multibyte_text_splits_on_char_boundaries() {
        let text = "あ".repeat(4100);
        let parts = split_message(&text, 2000);

        assert_eq!(parts.concat(), text);
        assert!(parts.iter().all(|p| chars(p) <= 2000));
    }

    #[test]
    fn open_code_block_is_closed_and_reopened_with_its_language() {
        let code: String = (0..300).map(|i| format!("let x{i} = {i};\n")).collect();
        let text = format!("Here:\n```rust\n{code}```\nDone.");

        let parts = split_message(&text, 2000);

        assert!(parts.len() > 1);
        assert!(parts.iter().all(|p| chars(p) <= 2000), "{parts:?}");
        assert!(parts[0].ends_with("\n```"));
        for part in &parts[1..] {
            assert!(part.starts_with("```rust\n"), "{part:.40}");
        }
        for part in &parts {
            assert_eq!(open_fence_info(part), None, "{part}");
        }
    }

    #[test]
    fn length_finish_appends_the_output_truncated_note() {
        let parts = answer_parts(&response("partial", Finish::Length), 5);
        assert_eq!(
            parts,
            ["partial\n\n(output truncated: max_output_tokens reached)"]
        );
    }

    #[test]
    fn answer_without_citations_has_no_source_list() {
        assert_eq!(answer_parts(&cited("answer", &[]), 5), ["answer"]);
    }

    #[test]
    fn citations_are_listed_as_small_unembedded_links_under_the_answer() {
        let response = cited(
            "answer",
            &[
                ("https://a.example/page", Some("Page A")),
                ("https://b.example/x?q=1", None),
            ],
        );

        assert_eq!(
            answer_parts(&response, 5),
            ["answer\n\n-# Sources\n\
              -# 1. [Page A](<https://a.example/page>)\n\
              -# 2. [b.example](<https://b.example/x?q=1>)"]
        );
    }

    #[test]
    fn source_list_follows_the_output_truncated_note() {
        let response = ChatResponse {
            finish: Finish::Length,
            ..cited("partial", &[("https://a.example/", Some("A"))])
        };

        assert_eq!(
            answer_parts(&response, 5),
            [
                "partial\n\n(output truncated: max_output_tokens reached)\n\n\
              -# Sources\n-# 1. [A](<https://a.example/>)"
            ]
        );
    }

    #[test]
    fn source_list_keeps_the_first_five_http_links() {
        let urls: Vec<String> = (0..7).map(|i| format!("https://{i}.example/")).collect();
        let mut citations: Vec<(&str, Option<&str>)> = vec![("javascript:alert(1)", None)];
        citations.extend(urls.iter().map(|url| (url.as_str(), None)));

        let parts = answer_parts(&cited("answer", &citations), 5);

        let lines: Vec<&str> = parts[0].lines().filter(|l| l.starts_with("-# ")).collect();
        assert_eq!(lines.len(), 6, "{parts:?}");
        assert_eq!(lines[1], "-# 1. [0.example](<https://0.example/>)");
        assert_eq!(lines[5], "-# 5. [4.example](<https://4.example/>)");
    }

    #[test]
    fn source_titles_stay_on_one_line_inside_the_link_text() {
        let long = "t".repeat(150);
        let response = cited(
            "answer",
            &[
                ("https://a.example/", Some("[Draft]\nnotes \\ more")),
                ("https://b.example/", Some(&long)),
            ],
        );

        let parts = answer_parts(&response, 5);

        assert!(
            parts[0].contains("-# 1. [\\[Draft\\] notes \\\\ more](<https://a.example/>)"),
            "{parts:?}"
        );
        assert!(
            parts[0].contains(&format!(
                "-# 2. [{}…](<https://b.example/>)",
                "t".repeat(100)
            )),
            "{parts:?}"
        );
    }

    #[test]
    fn source_list_counts_against_max_parts() {
        let text = "a".repeat(1990);
        let response = cited(&text, &[("https://a.example/", Some("A"))]);

        assert_eq!(answer_parts(&response, 2).len(), 2);
        let cut = answer_parts(&response, 1);
        assert_eq!(cut.len(), 1);
        assert!(!cut[0].contains(SOURCES_HEADING), "{cut:?}");
    }

    #[test]
    fn answers_over_max_parts_are_cut_with_a_note() {
        let text = (0..10)
            .map(|i| i.to_string().repeat(1900))
            .collect::<Vec<_>>()
            .join("\n");

        let parts = answer_parts(&response(&text, Finish::Stop), 3);

        assert_eq!(parts.len(), 3);
        assert!(parts.iter().all(|p| chars(p) <= 2000));
        assert!(parts[2].ends_with("(response truncated: too long for Discord)"));
        assert!(parts[2].starts_with('2'));
    }

    #[test]
    fn cut_inside_a_code_block_still_closes_it_before_the_note() {
        let code = "x\n".repeat(3000);
        let text = format!("```\n{code}```");

        let parts = answer_parts(&response(&text, Finish::Stop), 1);

        assert_eq!(parts.len(), 1);
        assert!(chars(&parts[0]) <= 2000);
        assert!(parts[0].ends_with("\n```\n\n(response truncated: too long for Discord)"));
    }

    fn every_error(api_key: &str) -> Vec<LlmError> {
        let model = || "test/model".to_string();
        let transport = reqwest::Client::new()
            .get(format!("not a url {api_key}"))
            .build()
            .unwrap_err();
        vec![
            LlmError::Timeout {
                model: model(),
                retries: 0,
                status: Some(408),
            },
            LlmError::RateLimited {
                model: model(),
                retries: 2,
                status: Some(429),
                provider_message: Some("slow down".to_string()),
                request_id: None,
            },
            LlmError::Provider {
                model: model(),
                retries: 0,
                status: 400,
                provider_message: Some("bad model".to_string()),
                request_id: None,
            },
            LlmError::Transport {
                model: model(),
                retries: 2,
                source: transport,
            },
            LlmError::ContentFilter {
                model: model(),
                retries: 0,
                request_id: Some("gen-1".to_string()),
            },
            LlmError::OutputTokenLimit {
                model: model(),
                retries: 0,
                request_id: Some("gen-2".to_string()),
            },
            LlmError::EmptyResponse {
                model: model(),
                retries: 0,
                request_id: Some("gen-3".to_string()),
            },
            LlmError::InResponse {
                model: model(),
                retries: 0,
                status: Some(502),
                provider_message: Some("upstream".to_string()),
                request_id: Some("gen-4".to_string()),
            },
            LlmError::Decode {
                model: model(),
                retries: 0,
                status: Some(200),
                request_id: None,
                source: serde_json::from_str::<()>("{").unwrap_err(),
            },
        ]
    }

    fn field<'a>(embed: &'a Embed, name: &str) -> Option<&'a str> {
        embed
            .fields
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.value.as_str())
    }

    #[test]
    fn every_error_renders_a_titled_embed_with_retries_and_model() {
        let titles: Vec<_> = every_error("sk-or-secret")
            .iter()
            .map(|error| {
                let embed = error_embed(error);
                assert_eq!(field(&embed, "Model"), Some("test/model"));
                assert!(field(&embed, "Retries").is_some());
                assert_eq!(embed.color, Some(ERROR_COLOR));
                embed.title.unwrap()
            })
            .collect();

        assert_eq!(
            titles,
            [
                "Timed out",
                "Rate limited",
                "Provider error",
                "Connection failed",
                "Content filtered",
                "Output token limit reached",
                "Empty response",
                "Generation failed",
                "Unexpected response",
            ]
        );
    }

    #[test]
    fn error_embed_shows_status_provider_message_and_request_id() {
        let embed = error_embed(&every_error("k")[7]);

        assert_eq!(field(&embed, "Status"), Some("502"));
        assert_eq!(field(&embed, "Provider message"), Some("upstream"));
        assert_eq!(field(&embed, "Request ID"), Some("gen-4"));
        assert_eq!(field(&embed, "Retries"), Some("0"));
    }

    #[test]
    fn long_provider_message_is_cut_to_the_field_limit() {
        let embed = error_embed(&LlmError::Provider {
            model: "m".to_string(),
            retries: 0,
            status: 400,
            provider_message: Some("x".repeat(5000)),
            request_id: None,
        });

        assert_eq!(chars(field(&embed, "Provider message").unwrap()), 1024);
    }

    #[test]
    fn error_embed_never_contains_the_api_key() {
        let key = "sk-or-v1-0123456789abcdef";
        for error in every_error(key) {
            let rendered = format!("{:?}", error_embed(&error));
            assert!(!rendered.contains(key), "{rendered}");
        }
    }
}
