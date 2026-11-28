//! A printable sheet of QR stickers, written as a PDF (D-30, D-75).
//!
//! A PDF rather than a web page because it prints the same from every
//! platform (the Android WebView cannot print a page) and can be handed to
//! the phone's own viewer. The grid is fully configurable; the presets are
//! common US layouts, and the app says to check them with a test print on
//! plain paper rather than claiming compatibility with a brand's product.

use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str};
use serde::{Deserialize, Serialize};

use crate::qr;
use crate::{MediaError, Result};

/// Sheet geometry, in millimetres.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub page_width: f32,
    pub page_height: f32,
    pub columns: u32,
    pub rows: u32,
    pub label_width: f32,
    pub label_height: f32,
    /// From the page's left edge to the first column.
    pub margin_left: f32,
    /// From the page's top edge to the first row.
    pub margin_top: f32,
    /// Space between columns and between rows.
    pub gap_x: f32,
    pub gap_y: f32,
    /// Draw a thin outline round each label (for a test print).
    #[serde(default)]
    pub outline: bool,
}

const IN: f32 = 25.4;

/// Built-in grids: (key, description, layout).
pub fn presets() -> Vec<(&'static str, &'static str, Layout)> {
    vec![
        (
            "letter-30",
            "US Letter, 3 × 10 labels, 2⅝ × 1 in (the common 30-per-sheet address label layout)",
            Layout {
                page_width: 8.5 * IN,
                page_height: 11.0 * IN,
                columns: 3,
                rows: 10,
                label_width: 2.625 * IN,
                label_height: 1.0 * IN,
                margin_left: 0.1875 * IN,
                margin_top: 0.5 * IN,
                gap_x: 0.125 * IN,
                gap_y: 0.0,
                outline: false,
            },
        ),
        (
            "letter-24-square",
            "US Letter, 4 × 6 square labels, 1½ in, centred on the page",
            centred(
                8.5 * IN,
                11.0 * IN,
                4,
                6,
                1.5 * IN,
                1.5 * IN,
                0.3125 * IN,
                0.25 * IN,
            ),
        ),
        (
            "a4-24",
            "A4, 3 × 8 labels, 70 × 37 mm, edge to edge",
            Layout {
                page_width: 210.0,
                page_height: 297.0,
                columns: 3,
                rows: 8,
                label_width: 70.0,
                label_height: 37.0,
                margin_left: 0.0,
                margin_top: 0.5 * (297.0 - 8.0 * 37.0),
                gap_x: 0.0,
                gap_y: 0.0,
                outline: false,
            },
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn centred(pw: f32, ph: f32, cols: u32, rows: u32, lw: f32, lh: f32, gx: f32, gy: f32) -> Layout {
    let used_w = cols as f32 * lw + (cols - 1) as f32 * gx;
    let used_h = rows as f32 * lh + (rows - 1) as f32 * gy;
    Layout {
        page_width: pw,
        page_height: ph,
        columns: cols,
        rows,
        label_width: lw,
        label_height: lh,
        margin_left: (pw - used_w) / 2.0,
        margin_top: (ph - used_h) / 2.0,
        gap_x: gx,
        gap_y: gy,
        outline: false,
    }
}

impl Layout {
    pub fn per_page(&self) -> usize {
        (self.columns * self.rows) as usize
    }

    /// Refuse a grid that does not fit the page or makes labels too small
    /// for a QR code a phone can read.
    pub fn validate(&self) -> Result<()> {
        let bad = |m: &str| Err(MediaError::Invalid(m.to_string()));
        let all = [
            self.page_width,
            self.page_height,
            self.label_width,
            self.label_height,
            self.margin_left,
            self.margin_top,
            self.gap_x,
            self.gap_y,
        ];
        if all.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return bad("sizes must be zero or more");
        }
        if !(1..=20).contains(&self.columns) || !(1..=40).contains(&self.rows) {
            return bad("use 1 to 20 columns and 1 to 40 rows");
        }
        if self.label_width.min(self.label_height) < 15.0 {
            return bad("labels smaller than 15 mm are too small for a QR code to scan reliably");
        }
        let right = self.margin_left
            + self.columns as f32 * self.label_width
            + (self.columns - 1) as f32 * self.gap_x;
        let bottom = self.margin_top
            + self.rows as f32 * self.label_height
            + (self.rows - 1) as f32 * self.gap_y;
        if right > self.page_width + 0.5 || bottom > self.page_height + 0.5 {
            return bad("the labels do not fit on the page; check the sizes and margins");
        }
        Ok(())
    }
}

/// One sticker.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sticker {
    /// What the QR code holds (`housewright://item/<id>`).
    pub url: String,
    /// The short code printed under it (`HW-7K3QM`).
    pub code: String,
    /// The thing's name.
    pub name: String,
}

fn pt(mm: f32) -> f32 {
    mm * 72.0 / IN
}

/// Helvetica has no glyphs outside Latin-1 in the standard encoding; keep
/// ASCII and replace the rest so nothing prints as garbage.
fn ascii(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| match c {
            ' '..='~' => c as u8,
            '’' | '‘' => b'\'',
            '“' | '”' => b'"',
            '–' | '—' => b'-',
            _ => b'?',
        })
        .collect()
}

/// Rough Helvetica width (bold, average) for wrapping.
fn text_width(s: &str, size: f32) -> f32 {
    s.chars()
        .map(|c| match c {
            'i' | 'l' | 'j' | '.' | ',' | '\'' | ' ' | 'I' | 'f' | 't' | 'r' => 0.32,
            'm' | 'w' | 'M' | 'W' => 0.88,
            'A'..='Z' => 0.70,
            _ => 0.58,
        })
        .sum::<f32>()
        * size
}

/// Up to `max_lines` lines of at most `width` points; the last line gets
/// an ellipsis if the name is longer.
fn wrap(s: &str, size: f32, width: f32, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let trial = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if text_width(&trial, size) <= width || cur.is_empty() {
            cur = trial;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        let last = lines.last_mut().expect("at least one line");
        while !last.is_empty() && text_width(&format!("{last}..."), size) > width {
            last.pop();
        }
        last.push_str("...");
    }
    for l in &mut lines {
        while l.chars().count() > 1 && text_width(l, size) > width {
            l.pop();
        }
    }
    lines
}

/// Draw the stickers, `layout.per_page()` to a page, as many pages as needed.
pub fn pdf(layout: &Layout, stickers: &[Sticker]) -> Result<Vec<u8>> {
    layout.validate()?;
    if stickers.is_empty() {
        return Err(MediaError::Invalid("choose at least one thing".to_string()));
    }
    let mut pdf = Pdf::new();
    let catalog = Ref::new(1);
    let tree = Ref::new(2);
    let font = Ref::new(3);
    let font_bold = Ref::new(4);
    let pages: Vec<&[Sticker]> = stickers.chunks(layout.per_page()).collect();
    let page_ids: Vec<Ref> = (0..pages.len())
        .map(|i| Ref::new(10 + 2 * i as i32))
        .collect();
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree)
        .kids(page_ids.iter().copied())
        .count(pages.len() as i32);
    pdf.type1_font(font).base_font(Name(b"Helvetica"));
    pdf.type1_font(font_bold).base_font(Name(b"Helvetica-Bold"));
    let (pw, ph) = (pt(layout.page_width), pt(layout.page_height));
    for (pi, chunk) in pages.iter().enumerate() {
        let page_id = page_ids[pi];
        let content_id = Ref::new(11 + 2 * pi as i32);
        let mut page = pdf.page(page_id);
        page.media_box(Rect::new(0.0, 0.0, pw, ph));
        page.parent(tree);
        page.contents(content_id);
        {
            let mut res = page.resources();
            let mut fonts = res.fonts();
            fonts.pair(Name(b"F1"), font);
            fonts.pair(Name(b"F2"), font_bold);
        }
        page.finish();
        let mut c = Content::new();
        for (i, s) in chunk.iter().enumerate() {
            let col = (i as u32) % layout.columns;
            let row = (i as u32) / layout.columns;
            let x = pt(layout.margin_left + col as f32 * (layout.label_width + layout.gap_x));
            let top =
                ph - pt(layout.margin_top + row as f32 * (layout.label_height + layout.gap_y));
            draw_sticker(
                &mut c,
                s,
                x,
                top,
                pt(layout.label_width),
                pt(layout.label_height),
                layout.outline,
            )?;
        }
        pdf.stream(content_id, &c.finish());
    }
    Ok(pdf.finish())
}

fn draw_sticker(
    c: &mut Content,
    s: &Sticker,
    x: f32,
    top: f32,
    w: f32,
    h: f32,
    outline: bool,
) -> Result<()> {
    let pad = 4.0_f32.min(w * 0.06);
    if outline {
        c.set_stroke_gray(0.7);
        c.set_line_width(0.3);
        c.rect(x, top - h, w, h);
        c.stroke();
    }
    let m = qr::encode(&s.url)?;
    // Side by side when the label is wide; QR above the text when square.
    let wide = w >= 1.6 * h;
    let qr_size = if wide {
        h - 2.0 * pad
    } else {
        (h * 0.66).min(w - 2.0 * pad)
    };
    let (qx, qy_top) = if wide {
        (x + pad, top - pad)
    } else {
        (x + (w - qr_size) / 2.0, top - pad)
    };
    let module = qr_size / m.size as f32;
    c.set_fill_gray(0.0);
    for yy in 0..m.size {
        for xx in 0..m.size {
            if m.is_dark(xx, yy) {
                c.rect(
                    qx + xx as f32 * module,
                    qy_top - (yy + 1) as f32 * module,
                    module + 0.02,
                    module + 0.02,
                );
            }
        }
    }
    c.fill_nonzero();
    let (tx, text_w, mut ty) = if wide {
        let tx = qx + qr_size + pad;
        (tx, x + w - pad - tx, top - pad - 9.0)
    } else {
        (x + pad, w - 2.0 * pad, qy_top - qr_size - 9.0)
    };
    let code_size = if wide { 12.0 } else { 9.0 };
    let name_size = if wide { 8.5 } else { 6.5 };
    let name_lines = if wide { 3 } else { 1 };
    let line = |c: &mut Content, font: &[u8], size: f32, text: &str, y: f32| {
        let width = text_width(text, size);
        let lx = if wide {
            tx
        } else {
            tx + (text_w - width).max(0.0) / 2.0
        };
        c.begin_text();
        c.set_font(Name(font), size);
        c.next_line(lx, y);
        c.show(Str(&ascii(text)));
        c.end_text();
    };
    line(c, b"F2", code_size, &s.code, ty);
    ty -= code_size + 1.0;
    for l in wrap(&s.name, name_size, text_w, name_lines) {
        if ty < top - h + pad {
            break;
        }
        line(c, b"F1", name_size, &l, ty);
        ty -= name_size + 1.5;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stickers(n: usize) -> Vec<Sticker> {
        (0..n)
            .map(|i| Sticker {
                url: format!("housewright://item/01a21fbc-5607-72ea-b419-dc88b45dc{i:03x}"),
                code: format!("HW-AB{i:03}"),
                name: format!("Water heater in the basement number {i}"),
            })
            .collect()
    }

    #[test]
    fn presets_fit_their_pages() {
        for (key, _, l) in presets() {
            l.validate().unwrap_or_else(|e| panic!("{key}: {e}"));
        }
    }

    #[test]
    fn pages_are_filled_in_order() {
        let (_, _, l) = presets().into_iter().next().unwrap();
        let bytes = pdf(&l, &stickers(31)).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("%PDF-"));
        assert!(
            text.contains("/Count 2"),
            "31 stickers on 30-per-page = 2 pages"
        );
        assert!(text.contains("HW-AB030"));
    }

    #[test]
    fn grids_that_do_not_fit_are_refused() {
        let (_, _, mut l) = presets().into_iter().next().unwrap();
        l.columns = 4;
        assert!(l.validate().is_err());
        let (_, _, mut l) = presets().into_iter().next().unwrap();
        l.label_height = 10.0;
        assert!(l.validate().is_err());
        assert!(pdf(&presets()[0].2, &[]).is_err());
    }

    #[test]
    fn long_names_wrap_and_trim() {
        let lines = wrap(
            "A very long name for a thing in the house that goes on",
            8.0,
            80.0,
            2,
        );
        assert_eq!(lines.len(), 2);
        assert!(lines[1].ends_with("..."));
        assert_eq!(ascii("Café – “x”"), b"Caf? - \"x\"".to_vec());
    }
}
