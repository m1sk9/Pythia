//! Image attachments: which ones to send, downloading, and downscaling.
//!
//! Selection and downscaling are pure; only `fetch_*` touch the network.

mod downscale;

use crate::context::is_image_attachment;
use base64::Engine as _;
use downscale::{FitError, FitLimits, Fitted};
use std::{collections::HashMap, error::Error as _, time::Duration};
use tokio::{sync::Semaphore, task::JoinSet};
use twilight_model::{
    channel::{Attachment, Message},
    id::{Id, marker::AttachmentMarker},
};

/// Per-download timeout, independent of the LLM timeout on the shared client.
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

/// Decoding is bounded per image only, so concurrent turns could otherwise
/// hold up to `max_images * limits.max_concurrent` full-size frames at once.
static DECODE_PERMITS: Semaphore = Semaphore::const_new(2);

/// Which images go to the model (`[attachments]` plus the model's capabilities).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImagePolicy {
    /// `attachments.images` and the model accepts images.
    pub enabled: bool,
    pub max_images: usize,
    /// Only images on this many of the newest user messages are sent.
    pub recent_messages: usize,
    /// Images whose longest side exceeds this are downscaled before sending.
    pub max_image_edge: u32,
    /// Attachments larger than this are never downloaded.
    pub max_download_bytes: usize,
    /// Images still larger than this after downscaling are left out.
    pub max_image_bytes: usize,
}

/// What happened to an image attachment. Images without an entry were not
/// selected (too old, over the count, or images are disabled).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageOutcome {
    Fetched {
        mime: &'static str,
        base64: String,
    },
    /// Over `max_download_bytes`, over the decoding limits, or over
    /// `max_image_bytes` even after downscaling.
    TooLarge,
    /// Downloaded, but not a supported image or not decodable.
    Unreadable,
    DownloadFailed,
}

impl ImageOutcome {
    pub fn fetched(fitted: Fitted) -> Self {
        Self::Fetched {
            mime: fitted.mime,
            base64: base64::engine::general_purpose::STANDARD.encode(fitted.bytes),
        }
    }

    pub fn from_error(error: &FetchError) -> Self {
        match error {
            FetchError::TooLarge | FetchError::Fit(FitError::TooLarge) => Self::TooLarge,
            FetchError::Fit(FitError::NotAnImage | FitError::Decode(_) | FitError::Encode(_)) => {
                Self::Unreadable
            }
            FetchError::Request(_) | FetchError::Status(_) => Self::DownloadFailed,
        }
    }
}

/// The result of [`select_images`].
#[derive(Debug, Default, PartialEq)]
pub struct ImagePlan<'a> {
    /// Images to download, newest first.
    pub fetch: Vec<&'a Attachment>,
    /// Images that would have been candidates but exceed `max_download_bytes`.
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
        if usize::try_from(attachment.size).map_or(true, |size| size > policy.max_download_bytes) {
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

#[derive(thiserror::Error, Debug)]
pub enum FetchError {
    #[error("request failed")]
    Request(#[from] reqwest::Error),
    #[error("CDN returned HTTP {0}")]
    Status(u16),
    #[error("image exceeds the download limit")]
    TooLarge,
    #[error(transparent)]
    Fit(#[from] FitError),
}

/// Downloads and downscales every image in `plan` concurrently. Failures are
/// logged and recorded in the outcome; they never fail the turn.
#[cfg_attr(coverage_nightly, coverage(off))]
pub async fn fetch_images(
    http: &reqwest::Client,
    plan: &ImagePlan<'_>,
    policy: &ImagePolicy,
) -> HashMap<Id<AttachmentMarker>, ImageOutcome> {
    let mut outcomes: HashMap<_, _> = plan
        .too_large
        .iter()
        .map(|&id| (id, ImageOutcome::TooLarge))
        .collect();

    let mut downloads = JoinSet::new();
    for attachment in &plan.fetch {
        let http = http.clone();
        let policy = *policy;
        let (id, url, filename) = (
            attachment.id,
            attachment.url.clone(),
            attachment.filename.clone(),
        );
        downloads.spawn(async move {
            let outcome = match fetch_image(&http, &url, &policy).await {
                Ok(fitted) => {
                    if let Some((width, height)) = fitted.resized_from {
                        tracing::debug!(
                            %filename,
                            from = %format!("{width}x{height}"),
                            bytes = fitted.bytes.len(),
                            mime = fitted.mime,
                            "downscaled image"
                        );
                    }
                    ImageOutcome::fetched(fitted)
                }
                Err(error) => {
                    // reqwest's source would log the signed CDN URL.
                    let cause = match &error {
                        FetchError::Fit(_) => error.source().map(ToString::to_string),
                        FetchError::Request(_) | FetchError::Status(_) | FetchError::TooLarge => {
                            None
                        }
                    };
                    tracing::warn!(%error, cause, %filename, "failed to fetch image");
                    ImageOutcome::from_error(&error)
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

/// Downloads one image, reading at most `max_download_bytes + 1` bytes, and
/// fits it into `max_image_edge` and `max_image_bytes`.
#[cfg_attr(coverage_nightly, coverage(off))]
async fn fetch_image(
    http: &reqwest::Client,
    url: &str,
    policy: &ImagePolicy,
) -> Result<Fitted, FetchError> {
    let mut response = http.get(url).timeout(FETCH_TIMEOUT).send().await?;
    if !response.status().is_success() {
        return Err(FetchError::Status(response.status().as_u16()));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > policy.max_download_bytes {
            return Err(FetchError::TooLarge);
        }
    }
    let limits = FitLimits {
        max_edge: policy.max_image_edge,
        max_bytes: policy.max_image_bytes,
    };
    // A panicking decoder is reported like an undecodable image rather than
    // through a separate variant, since the user-facing outcome is the same.
    let permit = DECODE_PERMITS
        .acquire()
        .await
        .expect("DECODE_PERMITS is never closed");
    let fitted = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        downscale::fit(bytes, &limits)
    })
    .await
    .unwrap_or_else(|error| {
        Err(FitError::Decode(image::ImageError::IoError(
            std::io::Error::other(error),
        )))
    })?;
    Ok(fitted)
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
            max_image_edge: 2048,
            max_download_bytes: 1000,
            max_image_bytes: 500,
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
    fn images_over_the_download_limit_are_never_selected_and_marked_too_large() {
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
            ImageOutcome::fetched(Fitted {
                mime: "image/png",
                bytes: PNG.to_vec(),
                resized_from: None,
            }),
            ImageOutcome::Fetched {
                mime: "image/png",
                base64: "iVBORw0KGgoAAAANSUhEUg==".to_string(),
            }
        );
    }

    #[test]
    fn fetch_errors_map_to_the_outcome_shown_in_the_context() {
        assert_eq!(
            ImageOutcome::from_error(&FetchError::TooLarge),
            ImageOutcome::TooLarge
        );
        assert_eq!(
            ImageOutcome::from_error(&FetchError::Fit(FitError::TooLarge)),
            ImageOutcome::TooLarge
        );
        assert_eq!(
            ImageOutcome::from_error(&FetchError::Fit(FitError::NotAnImage)),
            ImageOutcome::Unreadable
        );
        assert_eq!(
            ImageOutcome::from_error(&FetchError::Fit(FitError::Encode(
                image::ImageError::IoError(std::io::Error::other("full"))
            ))),
            ImageOutcome::Unreadable
        );
        assert_eq!(
            ImageOutcome::from_error(&FetchError::Status(500)),
            ImageOutcome::DownloadFailed
        );
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

        let policy = ImagePolicy {
            enabled: true,
            max_images: config.attachments.max_images,
            recent_messages: config.attachments.recent_messages,
            max_image_edge: config.attachments.max_image_edge,
            max_download_bytes: config.attachments.max_download_bytes,
            max_image_bytes: config.attachments.max_image_bytes,
        };
        let fitted = fetch_image(&http, &url, &policy).await.unwrap();
        println!(
            "{}, {} bytes, resized_from: {:?}",
            fitted.mime,
            fitted.bytes.len(),
            fitted.resized_from
        );
        let ImageOutcome::Fetched { mime, base64 } = ImageOutcome::fetched(fitted) else {
            unreachable!();
        };

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
