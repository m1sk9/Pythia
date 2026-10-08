//! Fits a downloaded image into the size limits: pass through, downscale, or re-encode.

use super::sniff_image;
use image::{
    DynamicImage, ImageDecoder, ImageError, ImageFormat, ImageReader, Limits, RgbImage,
    codecs::jpeg::JpegEncoder, imageops::FilterType, metadata::Orientation,
};
use std::io::Cursor;

/// 256 MiB is an 8192x8192 RGBA frame; Discord screenshots never need more.
const MAX_DECODE_ALLOC: u64 = 256 * 1024 * 1024;
const MAX_DECODE_DIMENSION: u32 = 16_384;
const JPEG_QUALITY: u8 = 85;
/// Halvings of `max_edge` tried when the re-encoded image still exceeds `max_bytes`.
const MAX_SHRINK_STEPS: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FitLimits {
    pub max_edge: u32,
    /// Measured on the base64 payload, which is what providers limit.
    pub max_bytes: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Fitted {
    pub mime: &'static str,
    pub bytes: Vec<u8>,
    /// Original `(width, height)` when the image was re-encoded; `None` when passed through.
    pub resized_from: Option<(u32, u32)>,
}

#[derive(thiserror::Error, Debug)]
pub enum FitError {
    #[error("file is not a PNG, JPEG, GIF, or WebP image")]
    NotAnImage,
    #[error("image could not be decoded")]
    Decode(#[source] ImageError),
    #[error("image could not be re-encoded")]
    Encode(#[source] ImageError),
    #[error("image exceeds the size limits")]
    TooLarge,
}

impl FitError {
    fn from_decode(error: ImageError) -> Self {
        match error {
            ImageError::Limits(_) => Self::TooLarge,
            error => Self::Decode(error),
        }
    }
}

/// Returns `bytes` untouched when they are within both limits and need no
/// rotation; otherwise applies the EXIF orientation, downscales the longest
/// side to `max_edge`, and re-encodes, keeping JPEG sources as JPEG and
/// turning the rest into PNG unless only JPEG fits.
pub fn fit(bytes: Vec<u8>, limits: &FitLimits) -> Result<Fitted, FitError> {
    let source_mime = sniff_image(&bytes).ok_or(FitError::NotAnImage)?;
    let mut decoder = decoder(&bytes)?;
    let (width, height) = decoder.dimensions();
    // Providers ignore EXIF, so a rotated photo has to be re-encoded upright.
    // An unreadable EXIF chunk only loses the rotation; the pixels still decode.
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    if width.max(height) <= limits.max_edge
        && base64_len(bytes.len()) <= limits.max_bytes
        && orientation == Orientation::NoTransforms
    {
        drop(decoder);
        return Ok(Fitted {
            mime: source_mime,
            bytes,
            resized_from: None,
        });
    }

    let image = decode(decoder, orientation)?;
    let longest = image.width().max(image.height());
    let lossless = source_mime != "image/jpeg";
    let mut max_edge = limits.max_edge.min(longest);
    for _ in 0..=MAX_SHRINK_STEPS {
        let shrunk;
        let candidate = if longest > max_edge {
            shrunk = shrink(&image, longest, max_edge);
            &shrunk
        } else {
            &image
        };
        if let Some((mime, bytes)) = encode(candidate, lossless, limits.max_bytes)? {
            return Ok(Fitted {
                mime,
                bytes,
                resized_from: Some((width, height)),
            });
        }
        max_edge = (max_edge / 2).max(1);
    }
    Err(FitError::TooLarge)
}

/// `thumbnail` is a fast box filter, but it drops or doubles thin lines when
/// the scale is close to 1, so gentle reductions use a triangle filter.
fn shrink(image: &DynamicImage, longest: u32, max_edge: u32) -> DynamicImage {
    if max_edge.saturating_mul(2) > longest {
        image.resize(max_edge, max_edge, FilterType::Triangle)
    } else {
        image.thumbnail(max_edge, max_edge)
    }
}

fn decode_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DECODE_DIMENSION);
    limits.max_image_height = Some(MAX_DECODE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    limits
}

fn decoder(bytes: &[u8]) -> Result<impl ImageDecoder + '_, FitError> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| FitError::Decode(ImageError::IoError(error)))?;
    if reader.format().is_none() {
        return Err(FitError::NotAnImage);
    }
    reader.limits(decode_limits());
    reader.into_decoder().map_err(FitError::from_decode)
}

fn decode(decoder: impl ImageDecoder, orientation: Orientation) -> Result<DynamicImage, FitError> {
    // Not every decoder honours `max_alloc`, so the output buffer is checked up front.
    decode_limits()
        .reserve(decoder.total_bytes())
        .map_err(FitError::from_decode)?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(FitError::from_decode)?;
    image.apply_orientation(orientation);
    Ok(image)
}

/// PNG first for lossless sources so that text in screenshots stays sharp;
/// JPEG when PNG does not fit. `None` when neither fits in `max_bytes`.
fn encode(
    image: &DynamicImage,
    lossless: bool,
    max_bytes: usize,
) -> Result<Option<(&'static str, Vec<u8>)>, FitError> {
    if lossless {
        let mut png = Cursor::new(Vec::new());
        image
            .write_to(&mut png, ImageFormat::Png)
            .map_err(FitError::Encode)?;
        let png = png.into_inner();
        if base64_len(png.len()) <= max_bytes {
            return Ok(Some(("image/png", png)));
        }
    }
    let mut jpeg = Cursor::new(Vec::new());
    flatten_onto_white(image)
        .write_with_encoder(JpegEncoder::new_with_quality(&mut jpeg, JPEG_QUALITY))
        .map_err(FitError::Encode)?;
    let jpeg = jpeg.into_inner();
    Ok((base64_len(jpeg.len()) <= max_bytes).then_some(("image/jpeg", jpeg)))
}

/// Length of the padded base64 encoding of `len` bytes.
fn base64_len(len: usize) -> usize {
    len.div_ceil(3) * 4
}

/// JPEG has no alpha, and `to_rgb8` alone would show transparent pixels in
/// their stored colour, usually black.
fn flatten_onto_white(image: &DynamicImage) -> RgbImage {
    if !image.color().has_alpha() {
        return image.to_rgb8();
    }
    let rgba = image.to_rgba8();
    RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let [r, g, b, a] = rgba.get_pixel(x, y).0;
        let over_white = |channel: u8| {
            let (channel, alpha) = (u16::from(channel), u16::from(a));
            ((channel * alpha + 255 * (255 - alpha) + 127) / 255) as u8
        };
        image::Rgb([over_white(r), over_white(g), over_white(b)])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, RgbImage, RgbaImage};

    const ROOMY: FitLimits = FitLimits {
        max_edge: 2048,
        max_bytes: 1024 * 1024,
    };

    fn encoded(image: DynamicImage, format: ImageFormat) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, format).unwrap();
        out.into_inner()
    }

    fn gradient(width: u32, height: u32) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
        }))
    }

    fn noise() -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_fn(256, 256, |x, y| {
            let v = x * 7 + y * 13 + x * y;
            image::Rgb([v as u8, (v >> 3) as u8, (v >> 5) as u8])
        }))
    }

    fn dimensions(bytes: &[u8]) -> (u32, u32) {
        image::load_from_memory(bytes).unwrap().dimensions()
    }

    #[test]
    fn images_within_both_limits_are_passed_through_untouched() {
        let png = encoded(gradient(100, 50), ImageFormat::Png);

        let fitted = fit(png.clone(), &ROOMY).unwrap();

        assert_eq!(
            fitted,
            Fitted {
                mime: "image/png",
                bytes: png,
                resized_from: None,
            }
        );
    }

    #[test]
    fn images_over_the_edge_limit_are_downscaled_keeping_the_aspect_ratio() {
        let png = encoded(gradient(3000, 1000), ImageFormat::Png);
        let limits = FitLimits {
            max_edge: 300,
            ..ROOMY
        };

        let fitted = fit(png, &limits).unwrap();

        assert_eq!(fitted.mime, "image/png");
        assert_eq!(fitted.resized_from, Some((3000, 1000)));
        assert_eq!(dimensions(&fitted.bytes), (300, 100));
    }

    #[test]
    fn jpeg_sources_stay_jpeg() {
        let jpeg = encoded(
            DynamicImage::ImageRgb8(gradient(600, 400).to_rgb8()),
            ImageFormat::Jpeg,
        );
        let limits = FitLimits {
            max_edge: 300,
            ..ROOMY
        };

        let fitted = fit(jpeg, &limits).unwrap();

        assert_eq!(fitted.mime, "image/jpeg");
        assert_eq!(dimensions(&fitted.bytes), (300, 200));
    }

    #[test]
    fn gif_and_webp_sources_become_png() {
        let limits = FitLimits {
            max_edge: 300,
            ..ROOMY
        };
        for format in [ImageFormat::Gif, ImageFormat::WebP] {
            let source = encoded(gradient(600, 400), format);

            let fitted = fit(source, &limits).unwrap();

            assert_eq!(fitted.mime, "image/png", "{format:?}");
            assert_eq!(dimensions(&fitted.bytes), (300, 200), "{format:?}");
        }
    }

    #[test]
    fn png_over_the_byte_limit_falls_back_to_jpeg() {
        let png = encoded(noise(), ImageFormat::Png);
        let limits = FitLimits {
            max_bytes: base64_len(png.len()) - 1,
            ..ROOMY
        };

        let fitted = fit(png, &limits).unwrap();

        assert_eq!(fitted.mime, "image/jpeg");
        assert!(base64_len(fitted.bytes.len()) <= limits.max_bytes);
        assert_eq!(dimensions(&fitted.bytes), (256, 256));
    }

    #[test]
    fn the_byte_limit_applies_to_the_base64_payload() {
        let png = encoded(noise(), ImageFormat::Png);
        let limits = FitLimits {
            max_bytes: png.len(),
            ..ROOMY
        };

        let fitted = fit(png, &limits).unwrap();

        assert_eq!(fitted.mime, "image/jpeg");
        assert!(base64_len(fitted.bytes.len()) <= limits.max_bytes);
    }

    #[test]
    fn base64_length_counts_padding() {
        assert_eq!(base64_len(0), 0);
        assert_eq!(base64_len(1), 4);
        assert_eq!(base64_len(3), 4);
        assert_eq!(base64_len(4), 8);
    }

    #[test]
    fn transparent_pixels_are_flattened_onto_white_in_jpeg() {
        let opaque = noise().to_rgb8();
        let half_transparent = DynamicImage::ImageRgba8(RgbaImage::from_fn(256, 256, |x, y| {
            let [r, g, b] = opaque.get_pixel(x, y).0;
            image::Rgba([r, g, b, if x < 128 { 0 } else { 255 }])
        }));
        let png = encoded(half_transparent, ImageFormat::Png);
        let limits = FitLimits {
            max_bytes: base64_len(png.len()) - 1,
            ..ROOMY
        };

        let fitted = fit(png, &limits).unwrap();

        assert_eq!(fitted.mime, "image/jpeg");
        let decoded = image::load_from_memory(&fitted.bytes).unwrap().to_rgb8();
        assert!(
            decoded.get_pixel(32, 128).0.iter().all(|&c| c >= 250),
            "{:?}",
            decoded.get_pixel(32, 128)
        );
    }

    #[test]
    fn images_still_over_the_byte_limit_after_shrinking_are_too_large() {
        let png = encoded(noise(), ImageFormat::Png);
        let limits = FitLimits {
            max_bytes: 10,
            ..ROOMY
        };

        assert!(matches!(fit(png, &limits), Err(FitError::TooLarge)));
    }

    fn jpeg_rotated_90_cw(width: u32, height: u32) -> Vec<u8> {
        let jpeg = encoded(
            DynamicImage::ImageRgb8(gradient(width, height).to_rgb8()),
            ImageFormat::Jpeg,
        );
        let exif_rotate_90_cw: &[u8] = &[
            0xFF, 0xE1, 0x00, 0x22, 0x45, 0x78, 0x69, 0x66, 0x00, 0x00, 0x49, 0x49, 0x2A, 0x00,
            0x08, 0x00, 0x00, 0x00, 0x01, 0x00, 0x12, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00,
            0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        [&jpeg[..2], exif_rotate_90_cw, &jpeg[2..]].concat()
    }

    #[test]
    fn exif_orientation_is_applied_before_re_encoding() {
        let limits = FitLimits {
            max_edge: 100,
            ..ROOMY
        };

        let fitted = fit(jpeg_rotated_90_cw(400, 200), &limits).unwrap();

        assert_eq!(dimensions(&fitted.bytes), (50, 100));
    }

    #[test]
    fn rotated_images_within_both_limits_are_re_encoded_upright() {
        let fitted = fit(jpeg_rotated_90_cw(400, 200), &ROOMY).unwrap();

        assert_eq!(fitted.mime, "image/jpeg");
        assert_eq!(fitted.resized_from, Some((400, 200)));
        assert_eq!(dimensions(&fitted.bytes), (200, 400));
    }

    #[test]
    fn slight_downscales_keep_the_aspect_ratio() {
        let png = encoded(gradient(2400, 1200), ImageFormat::Png);
        let limits = FitLimits {
            max_edge: 2000,
            ..ROOMY
        };

        let fitted = fit(png, &limits).unwrap();

        assert_eq!(dimensions(&fitted.bytes), (2000, 1000));
    }

    #[test]
    fn non_images_are_rejected() {
        assert!(matches!(
            fit(b"<html>".to_vec(), &ROOMY),
            Err(FitError::NotAnImage)
        ));
        assert!(matches!(
            fit(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec(), &ROOMY),
            Err(FitError::Decode(_))
        ));
    }

    #[test]
    fn images_over_the_decode_limits_are_too_large() {
        let png = encoded(gradient(MAX_DECODE_DIMENSION + 1, 1), ImageFormat::Png);

        assert!(matches!(fit(png, &ROOMY), Err(FitError::TooLarge)));
    }
}
