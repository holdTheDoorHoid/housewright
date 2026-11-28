//! QR codes on stickers (D-30): drawn with `qrcode`, read back from a photo
//! with `rqrr`, both pure Rust, so scanning works in the browser too.

use image::RgbImage;
use qrcode::{Color, EcLevel, QrCode};

use crate::photo;
use crate::{MediaError, Result};

/// A QR code as a square grid of modules (true = dark).
pub struct Matrix {
    pub size: usize,
    pub dark: Vec<bool>,
}

impl Matrix {
    pub fn is_dark(&self, x: usize, y: usize) -> bool {
        self.dark[y * self.size + x]
    }
}

/// Encode text at error correction level M (survives a scuffed sticker).
pub fn encode(text: &str) -> Result<Matrix> {
    let code = QrCode::with_error_correction_level(text.as_bytes(), EcLevel::M)
        .map_err(|e| MediaError::Invalid(format!("QR code: {e}")))?;
    Ok(Matrix {
        size: code.width(),
        dark: code
            .to_colors()
            .into_iter()
            .map(|c| c == Color::Dark)
            .collect(),
    })
}

/// The QR code as a small SVG (one path, quiet zone of 4 modules), for the
/// on-screen preview of a sticker sheet.
pub fn svg(text: &str) -> Result<String> {
    let m = encode(text)?;
    let q = 4;
    let n = m.size + 2 * q;
    let mut d = String::new();
    for y in 0..m.size {
        for x in 0..m.size {
            if m.is_dark(x, y) {
                d.push_str(&format!("M{} {}h1v1h-1z", x + q, y + q));
            }
        }
    }
    Ok(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {n} {n}\" shape-rendering=\"crispEdges\"><rect width=\"{n}\" height=\"{n}\" fill=\"#fff\"/><path d=\"{d}\" fill=\"#000\"/></svg>"
    ))
}

fn decode_image(img: &RgbImage) -> Vec<String> {
    let (w, h) = img.dimensions();
    let mut prepared =
        rqrr::PreparedImage::prepare_from_greyscale(w as usize, h as usize, |x, y| {
            let p = img.get_pixel(x as u32, y as u32);
            ((u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114) / 1000) as u8
        });
    prepared
        .detect_grids()
        .into_iter()
        .filter_map(|g| g.decode().ok().map(|(_, text)| text))
        .collect()
}

/// Every QR code found in a photo. Tries a reduced copy first (fast, and
/// a phone photo of a sticker is mostly sticker), then the full size.
pub fn decode_photo(bytes: &[u8]) -> Result<Vec<String>> {
    let img = photo::decode(bytes)?;
    for edge in [1000, 2000] {
        let small = photo::fit(&img, edge);
        let found = decode_image(&small);
        if !found.is_empty() {
            return Ok(found);
        }
        if small.dimensions() == img.dimensions() {
            break;
        }
    }
    Ok(vec![])
}

/// Draw a QR code into an image (tests, and evidence pictures).
pub fn render(text: &str, module_px: u32) -> Result<RgbImage> {
    let m = encode(text)?;
    let q = 4;
    let n = (m.size as u32 + 2 * q) * module_px;
    Ok(RgbImage::from_fn(n, n, |x, y| {
        let (mx, my) = (x / module_px, y / module_px);
        let dark = mx >= q
            && my >= q
            && mx < q + m.size as u32
            && my < q + m.size as u32
            && m.is_dark((mx - q) as usize, (my - q) as usize);
        if dark {
            image::Rgb([0, 0, 0])
        } else {
            image::Rgb([255, 255, 255])
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgb};
    use std::io::Cursor;

    const URL: &str = "housewright://item/01a21fbc-5607-72ea-b419-dc88b45dcfbf";

    #[test]
    fn encode_then_decode_round_trips() {
        let img = render(URL, 6).unwrap();
        let mut png = Cursor::new(Vec::new());
        img.write_to(&mut png, ImageFormat::Png).unwrap();
        assert_eq!(
            decode_photo(&png.into_inner()).unwrap(),
            vec![URL.to_string()]
        );
    }

    #[test]
    fn a_sticker_in_a_larger_photo_is_found() {
        let code = render(URL, 5).unwrap();
        let mut photo = RgbImage::from_pixel(1800, 1400, Rgb([180, 170, 160]));
        image::imageops::overlay(&mut photo, &code, 700, 500);
        let mut jpg = Cursor::new(Vec::new());
        photo.write_to(&mut jpg, ImageFormat::Jpeg).unwrap();
        assert_eq!(
            decode_photo(&jpg.into_inner()).unwrap(),
            vec![URL.to_string()]
        );
    }

    #[test]
    fn no_code_is_not_an_error() {
        let blank = RgbImage::from_pixel(200, 200, Rgb([255, 255, 255]));
        let mut png = Cursor::new(Vec::new());
        blank.write_to(&mut png, ImageFormat::Png).unwrap();
        assert!(decode_photo(&png.into_inner()).unwrap().is_empty());
    }

    #[test]
    fn svg_is_well_formed() {
        let s = svg(URL).unwrap();
        assert!(s.starts_with("<svg") && s.ends_with("</svg>"));
        assert!(s.contains("viewBox"));
    }
}
