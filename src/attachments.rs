//! Image attachments: which ones to send, downloading, and validation.
//!
//! Selection and validation are pure; only `fetch_*` touch the network.

use crate::context::is_image_attachment;
use base64::Engine as _;
use std::{collections::HashMap, time::Duration};
use tokio::task::JoinSet;
use twilight_model::{
    channel::{Attachment, Message},
    id::{Id, marker::AttachmentMarker},
};

/// Per-download timeout, independent of the LLM timeout on the shared client.
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

/// Which images go to the model (`[attachments]` plus the model's capabilities).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImagePolicy {
    /// `attachments.images` and the model accepts images.
    pub enabled: bool,
    pub max_images: usize,
    /// Only images on this many of the newest user messages are sent.
    pub recent_messages: usize,
    pub max_image_bytes: usize,
}

/// What happened to an image attachment. Images without an entry were not
/// selected (too old, over the count, or images are disabled).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageOutcome {
    Fetched { mime: &'static str, base64: String },
    TooLarge,
    DownloadFailed,
}

/// The result of [`select_images`].
#[derive(Debug, Default, PartialEq)]
pub struct ImagePlan<'a> {
    /// Images to download, newest first.
    pub fetch: Vec<&'a Attachment>,
    /// Images that would have been candidates but exceed `max_image_bytes`.
    pub too_large: Vec<Id<AttachmentMarker>>,
}

/// Picks the images to send from the user messages that make it into the
/// context, newest first.
pub fn select_images<'a>(
    user_messages_newest_first: &[&'a Message],
    policy: &ImagePolicy,
) -> ImagePlan<'a> {
    let mut plan = ImagePlan::default();
    if !policy.enabled {
        return plan;
    }
    let candidates = user_messages_newest_first
        .iter()
        .take(policy.recent_messages)
        .flat_map(|message| &message.attachments)
        .filter(|attachment| is_image_attachment(attachment));
    for attachment in candidates {
        if usize::try_from(attachment.size).map_or(true, |size| size > policy.max_image_bytes) {
            plan.too_large.push(attachment.id);
        } else if plan.fetch.len() < policy.max_images {
            plan.fetch.push(attachment);
        }
    }
    plan
}

/// The MIME type implied by the file's magic bytes, if it is a supported image.
pub fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        [b'G', b'I', b'F', b'8', b'7' | b'9', b'a', ..] => Some("image/gif"),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some("image/webp"),
        _ => None,
    }
}

/// Validates downloaded bytes and encodes them for an image part.
pub fn to_outcome(bytes: &[u8], max_bytes: usize) -> Result<ImageOutcome, FetchError> {
    if bytes.len() > max_bytes {
        return Err(FetchError::TooLarge);
    }
    let mime = sniff_image(bytes).ok_or(FetchError::NotAnImage)?;
    Ok(ImageOutcome::Fetched {
        mime,
        base64: base64::engine::general_purpose::STANDARD.encode(bytes),
    })
}

#[derive(thiserror::Error, Debug)]
pub enum FetchError {
    #[error("request failed")]
    Request(#[from] reqwest::Error),
    #[error("CDN returned HTTP {0}")]
    Status(u16),
    #[error("image exceeds the size limit")]
    TooLarge,
    #[error("file is not a PNG, JPEG, GIF, or WebP image")]
    NotAnImage,
}

/// Downloads every image in `plan` concurrently. Failures are logged and
/// recorded as [`ImageOutcome::DownloadFailed`]; they never fail the turn.
#[cfg_attr(coverage_nightly, coverage(off))]
pub async fn fetch_images(
    http: &reqwest::Client,
    plan: &ImagePlan<'_>,
    max_bytes: usize,
) -> HashMap<Id<AttachmentMarker>, ImageOutcome> {
    let mut outcomes: HashMap<_, _> = plan
        .too_large
        .iter()
        .map(|&id| (id, ImageOutcome::TooLarge))
        .collect();

    let mut downloads = JoinSet::new();
    for attachment in &plan.fetch {
        let http = http.clone();
        let (id, url, filename) = (
            attachment.id,
            attachment.url.clone(),
            attachment.filename.clone(),
        );
        downloads.spawn(async move {
            let outcome = match fetch_image(&http, &url, max_bytes).await {
                Ok(outcome) => outcome,
                Err(error) => {
                    tracing::warn!(%error, %filename, "failed to download image");
                    ImageOutcome::DownloadFailed
                }
            };
            (id, outcome)
        });
    }
    while let Some(joined) = downloads.join_next().await {
        match joined {
            Ok((id, outcome)) => {
                outcomes.insert(id, outcome);
            }
            Err(error) => tracing::error!(%error, "image download task failed"),
        }
    }
    for attachment in &plan.fetch {
        outcomes
            .entry(attachment.id)
            .or_insert(ImageOutcome::DownloadFailed);
    }
    outcomes
}

/// Downloads one image, reading at most `max_bytes + 1` bytes.
#[cfg_attr(coverage_nightly, coverage(off))]
async fn fetch_image(
    http: &reqwest::Client,
    url: &str,
    max_bytes: usize,
) -> Result<ImageOutcome, FetchError> {
    let mut response = http.get(url).timeout(FETCH_TIMEOUT).send().await?;
    if !response.status().is_success() {
        return Err(FetchError::Status(response.status().as_u16()));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > max_bytes {
            return Err(FetchError::TooLarge);
        }
    }
    to_outcome(&bytes, max_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    fn image(id: u64, size: u64) -> Attachment {
        serde_json::from_value(json!({
            "id": id.to_string(),
            "filename": format!("{id}.png"),
            "content_type": "image/png",
            "size": size,
            "url": format!("https://cdn.discordapp.com/attachments/1/{id}/{id}.png"),
            "proxy_url": format!("https://media.discordapp.net/attachments/1/{id}/{id}.png"),
        }))
        .unwrap()
    }

    fn message(attachments: Vec<Attachment>) -> Message {
        let mut message: Message = serde_json::from_value(json!({
            "id": "1",
            "channel_id": "1",
            "author": {"id": "2", "username": "alice", "discriminator": "0", "avatar": null},
            "content": "look",
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
        .unwrap();
        message.attachments = attachments;
        message
    }

    fn policy() -> ImagePolicy {
        ImagePolicy {
            enabled: true,
            max_images: 3,
            recent_messages: 2,
            max_image_bytes: 1000,
        }
    }

    fn ids(attachments: &[&Attachment]) -> Vec<u64> {
        attachments.iter().map(|a| a.id.get()).collect()
    }

    #[test]
    fn only_the_newest_messages_images_are_selected_up_to_max_images() {
        let messages = [
            message(vec![image(41, 10), image(42, 10)]),
            message(vec![image(31, 10), image(32, 10)]),
            message(vec![image(21, 10)]),
            message(vec![image(11, 10)]),
        ];
        let newest_first: Vec<_> = messages.iter().collect();

        let plan = select_images(&newest_first, &policy());

        assert_eq!(ids(&plan.fetch), [41, 42, 31]);
        assert!(plan.too_large.is_empty());
    }

    #[test]
    fn oversized_images_are_never_selected_and_marked_too_large() {
        let messages = [message(vec![image(1, 1001), image(2, 1000)])];
        let newest_first: Vec<_> = messages.iter().collect();

        let plan = select_images(&newest_first, &policy());

        assert_eq!(ids(&plan.fetch), [2]);
        assert_eq!(plan.too_large, [Id::new(1)]);
    }

    #[test]
    fn non_image_attachments_are_never_selected() {
        let mut text = image(1, 10);
        text.filename = "notes.txt".to_string();
        text.content_type = Some("text/plain".to_string());
        let messages = [message(vec![text])];
        let newest_first: Vec<_> = messages.iter().collect();

        assert_eq!(
            select_images(&newest_first, &policy()),
            ImagePlan::default()
        );
    }

    #[test]
    fn nothing_is_selected_when_images_are_disabled() {
        let messages = [message(vec![image(1, 10), image(2, 5000)])];
        let newest_first: Vec<_> = messages.iter().collect();
        let disabled = ImagePolicy {
            enabled: false,
            ..policy()
        };

        assert_eq!(
            select_images(&newest_first, &disabled),
            ImagePlan::default()
        );
    }

    #[test]
    fn supported_formats_are_recognised_by_magic_bytes() {
        assert_eq!(sniff_image(PNG), Some("image/png"));
        assert_eq!(
            sniff_image(b"\xFF\xD8\xFF\xE0\0\x10JFIF"),
            Some("image/jpeg")
        );
        assert_eq!(sniff_image(b"GIF87a\x01\0"), Some("image/gif"));
        assert_eq!(sniff_image(b"GIF89a\x01\0"), Some("image/gif"));
        assert_eq!(sniff_image(b"RIFF\x24\0\0\0WEBPVP8 "), Some("image/webp"));
    }

    #[test]
    fn text_empty_and_non_webp_riff_files_are_rejected() {
        assert_eq!(sniff_image(b"hello, world"), None);
        assert_eq!(sniff_image(b""), None);
        assert_eq!(sniff_image(b"RIFF\x24\0\0\0WAVEfmt "), None);
        assert_eq!(sniff_image(b"\x89PN"), None);
    }

    #[test]
    fn downloaded_image_is_encoded_as_padded_standard_base64() {
        assert_eq!(
            to_outcome(PNG, 1000).unwrap(),
            ImageOutcome::Fetched {
                mime: "image/png",
                base64: "iVBORw0KGgoAAAANSUhEUg==".to_string(),
            }
        );
    }

    #[test]
    fn downloaded_bytes_over_the_limit_or_not_an_image_are_rejected() {
        assert!(matches!(to_outcome(PNG, 4), Err(FetchError::TooLarge)));
        assert!(matches!(
            to_outcome(b"<html>", 1000),
            Err(FetchError::NotAnImage)
        ));
    }

    /// Downloads `LIVE_IMAGE_URL` (default: a GitHub avatar) and asks
    /// `OPENROUTER_MODEL`, or `llm.model`, to describe it. Reads `.env` like `live_ping`.
    #[tokio::test]
    #[ignore = "calls the live OpenRouter API and downloads an image"]
    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn live_image_round_trip() {
        use crate::llm::{
            ChatMessage, ChatRequest, Content, Part, Role, openrouter::OpenRouterClient,
        };

        dotenvy::dotenv().ok();
        let api_key = std::env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY");
        let path = std::env::var("CONFIG_FILE_PATH").expect("CONFIG_FILE_PATH");
        let config: crate::config::PythiaConfig =
            std::fs::read_to_string(path).unwrap().parse().unwrap();
        let url = std::env::var("LIVE_IMAGE_URL")
            .unwrap_or_else(|_| "https://github.com/m1sk9.png".to_string());
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap();

        let ImageOutcome::Fetched { mime, base64 } =
            fetch_image(&http, &url, config.attachments.max_image_bytes)
                .await
                .unwrap()
        else {
            panic!("not fetched");
        };
        println!("{mime}, {} base64 chars", base64.len());

        let model = std::env::var("OPENROUTER_MODEL").unwrap_or(config.llm.model);
        let client = OpenRouterClient::new(
            http,
            api_key,
            model,
            "Describe images in one sentence.".to_string(),
            2,
        );
        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: Role::User,
                content: Content::Parts(vec![
                    Part::Text("alice: what is in this image?\n[image: avatar.png]".to_string()),
                    Part::ImageDataUrl {
                        mime: mime.to_string(),
                        base64,
                    },
                ]),
            }],
            max_output_tokens: 256,
            tools: Vec::new(),
        };
        println!("{:#?}", client.chat(&request).await.unwrap());
    }
}
