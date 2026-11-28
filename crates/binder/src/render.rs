//! Typesetting with Typst (D-43, D-80): Typst source in, PDF bytes out.
//!
//! The "world" Typst sees is closed: one main source file, the images we
//! hand it by name, and the fonts compiled into this crate. It has no file
//! system, no network and no packages, so a binder renders the same,
//! offline, on Android, the web and the desktop. An `#import` of anything
//! else fails, which is one more guard on top of the escaping in
//! [`crate::typst`].

use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;

use typst::diag::{FileError, FileResult, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;

/// The fonts, compiled in (SIL Open Font License 1.1; see `fonts/OFL.txt`
/// and docs/THIRD-PARTY.md). No font is ever fetched.
const FONT_FILES: &[&[u8]] = &[
    include_bytes!("../fonts/Lato-Regular.ttf"),
    include_bytes!("../fonts/Lato-Bold.ttf"),
];

struct Fonts {
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

static FONTS: LazyLock<Fonts> = LazyLock::new(|| {
    let mut fonts = Vec::new();
    for data in FONT_FILES {
        let bytes = Bytes::new(*data);
        let mut i = 0;
        while let Some(f) = Font::new(bytes.clone(), i) {
            fonts.push(f);
            i += 1;
        }
    }
    Fonts {
        book: LazyHash::new(FontBook::from_fonts(&fonts)),
        fonts,
    }
});

static LIBRARY: LazyLock<LazyHash<Library>> = LazyLock::new(|| LazyHash::new(Library::default()));

fn file_id(path: &str) -> FileId {
    let vpath = VirtualPath::new(path).expect("our own file names are valid paths");
    RootedPath::new(VirtualRoot::Project, vpath).intern()
}

struct BinderWorld {
    main: Source,
    files: HashMap<FileId, Bytes>,
    today: Datetime,
}

impl World for BinderWorld {
    fn library(&self) -> &LazyHash<Library> {
        &LIBRARY
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &FONTS.book
    }
    fn main(&self) -> FileId {
        self.main.id()
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            Err(FileError::AccessDenied)
        }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.files.get(&id).cloned().ok_or(FileError::AccessDenied)
    }
    fn font(&self, index: usize) -> Option<Font> {
        FONTS.fonts.get(index).cloned()
    }
    fn today(&self, _offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        Some(self.today)
    }
}

/// A typeset document.
pub struct Rendered {
    pub pdf: Vec<u8>,
    pub pages: usize,
    /// The laid-out text of each page, exactly as it is drawn into the PDF
    /// (one line per run of text). For checks and tests.
    pub page_text: Vec<String>,
}

fn frame_text(f: &typst::layout::Frame, out: &mut String) {
    for (_, item) in f.items() {
        match item {
            typst::layout::FrameItem::Group(g) => frame_text(&g.frame, out),
            typst::layout::FrameItem::Text(t) => {
                out.push_str(&t.text);
                out.push('\n');
            }
            _ => {}
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("the binder could not be typeset: {0}")]
pub struct RenderError(pub String);

fn describe(diags: &[SourceDiagnostic]) -> String {
    diags
        .iter()
        .map(|d| d.message.to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

/// Typeset `source`. `files` maps a name used in the source (`img/abc.jpg`)
/// to its bytes. `today` is the printed-on date.
pub fn render(
    source: &str,
    files: &BTreeMap<String, Vec<u8>>,
    today: jiff::civil::Date,
) -> Result<Rendered, RenderError> {
    let main = Source::new(file_id("/main.typ"), source.to_string());
    let files = files
        .iter()
        .map(|(name, bytes)| (file_id(&format!("/{name}")), Bytes::new(bytes.clone())))
        .collect();
    let today = Datetime::from_ymd(
        i32::from(today.year()),
        today.month() as u8,
        today.day() as u8,
    )
    .ok_or_else(|| RenderError("bad date".into()))?;
    let world = BinderWorld { main, files, today };
    let result = typst::compile::<PagedDocument>(&world);
    let doc = result.output.map_err(|d| RenderError(describe(&d)))?;
    let options = typst_pdf::PdfOptions {
        ident: typst::foundations::Smart::Custom("housewright-binder".into()),
        creator: typst::foundations::Smart::Custom(Some("Housewright".into())),
        ..Default::default()
    };
    let pdf = typst_pdf::pdf(&doc, &options).map_err(|d| RenderError(describe(&d)))?;
    let pages = doc.pages().len();
    let page_text = doc
        .pages()
        .iter()
        .map(|p| {
            let mut s = String::new();
            frame_text(&p.frame, &mut s);
            s
        })
        .collect();
    // Typst memoises aggressively; drop what this render cached so a long
    // session on a phone does not grow without bound.
    typst::comemo::evict(0);
    Ok(Rendered {
        pdf,
        pages,
        page_text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_page_offline() {
        let r = render(
            "#set page(paper: \"us-letter\")\n= Hello\nWorld.",
            &BTreeMap::new(),
            jiff::civil::date(2026, 11, 27),
        )
        .unwrap();
        assert_eq!(r.pages, 1);
        assert!(r.pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn nothing_outside_the_world_can_be_read() {
        for src in [
            "#import \"@preview/cetz:0.3.0\": *",
            "#read(\"/etc/passwd\")",
            "#image(\"/home/x.png\")",
            "#include \"other.typ\"",
        ] {
            assert!(
                render(src, &BTreeMap::new(), jiff::civil::date(2026, 1, 1)).is_err(),
                "{src}"
            );
        }
    }
}
