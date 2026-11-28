//! Photos are downscaled on the device before they are stored (D-72): the
//! long edge to at most [`MAX_EDGE`] pixels, saved as JPEG at quality
//! [`QUALITY`]. Re-encoding also drops the camera's metadata, including
//! the GPS location a phone may have written into the file.

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageReader, RgbImage};

use crate::{MediaError, Result};

/// Long edge of a stored photo, in pixels. Enough to read a rating plate or
/// a receipt; a 12-megapixel phone photo becomes about 300 KB.
pub const MAX_EDGE: u32 = 1600;
/// JPEG quality of a stored photo.
pub const QUALITY: u8 = 80;
/// Long edge of a thumbnail.
pub const THUMB_EDGE: u32 = 320;
pub const THUMB_QUALITY: u8 = 70;
/// The largest photo we try to decode (pixels), so a hostile or broken file
/// cannot exhaust memory: 50 megapixels.
pub const MAX_PIXELS: u64 = 50_000_000;

/// A photo ready to store.
pub struct Prepared {
    pub jpeg: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub thumb: Vec<u8>,
}

/// Whether bytes are an image format we accept (JPEG, PNG, WebP).
pub fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Whether bytes are a PDF.
pub fn is_pdf(bytes: &[u8]) -> bool {
    bytes.starts_with(b"%PDF-")
}

/// Decode a photo, turning it upright as the camera recorded (EXIF
/// orientation) and flattening any transparency onto white.
pub fn decode(bytes: &[u8]) -> Result<RgbImage> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| MediaError::Decode(e.to_string()))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| MediaError::Decode(e.to_string()))?;
    let (w, h) = decoder.dimensions();
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(MediaError::Invalid(format!(
            "the photo is {w} x {h} pixels; the most Housewright reads is 50 megapixels"
        )));
    }
    let orientation = decoder.orientation().ok();
    let mut img =
        DynamicImage::from_decoder(decoder).map_err(|e| MediaError::Decode(e.to_string()))?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }
    Ok(flatten(img))
}

fn flatten(img: DynamicImage) -> RgbImage {
    if !img.color().has_alpha() {
        return img.into_rgb8();
    }
    let rgba = img.into_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (o, p) in out.pixels_mut().zip(rgba.pixels()) {
        let a = u16::from(p[3]);
        for c in 0..3 {
            o[c] = ((u16::from(p[c]) * a + 255 * (255 - a)) / 255) as u8;
        }
    }
    out
}

/// Shrink so the long edge is at most `edge` (never enlarges).
pub fn fit(img: &RgbImage, edge: u32) -> RgbImage {
    let (w, h) = img.dimensions();
    if w.max(h) <= edge {
        return img.clone();
    }
    let scale = f64::from(edge) / f64::from(w.max(h));
    let nw = ((f64::from(w) * scale).round() as u32).max(1);
    let nh = ((f64::from(h) * scale).round() as u32).max(1);
    image::imageops::resize(img, nw, nh, FilterType::Triangle)
}

pub fn encode_jpeg(img: &RgbImage, quality: u8) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality)
        .encode_image(img)
        .map_err(|e| MediaError::Encode(e.to_string()))?;
    Ok(out)
}

/// Downscale and re-encode a photo, and make its thumbnail.
pub fn prepare(bytes: &[u8]) -> Result<Prepared> {
    if sniff_image(bytes).is_none() {
        return Err(MediaError::Decode(
            "use a JPEG, PNG or WebP photo".to_string(),
        ));
    }
    let img = decode(bytes)?;
    let stored = fit(&img, MAX_EDGE);
    let thumb = fit(&stored, THUMB_EDGE);
    Ok(Prepared {
        jpeg: encode_jpeg(&stored, QUALITY)?,
        width: stored.width(),
        height: stored.height(),
        thumb: encode_jpeg(&thumb, THUMB_QUALITY)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgb, Rgba, RgbaImage};

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = RgbImage::from_fn(w, h, |x, y| Rgb([(x % 256) as u8, (y % 256) as u8, 128]));
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn a_big_photo_is_shrunk_to_the_long_edge() {
        let p = prepare(&png(4000, 3000)).unwrap();
        assert_eq!((p.width, p.height), (1600, 1200));
        assert_eq!(sniff_image(&p.jpeg), Some("image/jpeg"));
        let t = image::load_from_memory(&p.thumb).unwrap();
        assert_eq!((t.width(), t.height()), (320, 240));
        assert!(p.thumb.len() < p.jpeg.len());
    }

    #[test]
    fn portrait_and_small_photos() {
        let p = prepare(&png(900, 2400)).unwrap();
        assert_eq!((p.width, p.height), (600, 1600));
        let p = prepare(&png(640, 480)).unwrap();
        assert_eq!((p.width, p.height), (640, 480), "never enlarged");
    }

    #[test]
    fn transparency_becomes_white() {
        let img = RgbaImage::from_pixel(10, 10, Rgba([0, 0, 0, 0]));
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, ImageFormat::Png).unwrap();
        let rgb = decode(&out.into_inner()).unwrap();
        assert_eq!(rgb.get_pixel(5, 5), &Rgb([255, 255, 255]));
    }

    #[test]
    fn not_a_photo() {
        assert!(prepare(b"%PDF-1.7 hello").is_err());
        assert!(prepare(b"\xFF\xD8\xFF garbage").is_err());
        assert!(is_pdf(b"%PDF-1.4\n"));
    }
}
