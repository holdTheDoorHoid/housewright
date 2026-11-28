//! A [`Document`] as Typst source (D-43, D-80).
//!
//! **User text never becomes Typst markup.** Every string from the
//! household (names, notes, facts) is written as a Typst *string literal*
//! by [`lit`] and handed to a fixed function from [`PRELUDE`], which shows
//! it as plain text. Inside a string literal only `\` and `"` mean
//! anything, and both are escaped, so `#set`, `#import`, `$`, `*`, `_`,
//! `` ` `` and `\` print as themselves. The tests feed hostile strings
//! through the whole renderer to check it.

use std::fmt::Write;

use crate::doc::{Action, Block, Document, Photo, Section, Volume};

/// Page size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Paper {
    #[default]
    Letter,
    A4,
}

impl Paper {
    fn typst(self) -> &'static str {
        match self {
            Paper::Letter => "us-letter",
            Paper::A4 => "a4",
        }
    }
}

/// A Typst string literal holding exactly `s`.
pub fn lit(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            '\t' => out.push(' '),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A Typst array of string literals. The trailing comma keeps a
/// one-element array an array.
fn arr(items: &[String]) -> String {
    let mut s = String::from("(");
    for i in items {
        s.push_str(&lit(i));
        s.push_str(", ");
    }
    s.push(')');
    s
}

fn arr2(rows: &[Vec<String>]) -> String {
    let mut s = String::from("(");
    for r in rows {
        s.push_str(&arr(r));
        s.push_str(", ");
    }
    s.push(')');
    s
}

fn pairs(rows: &[(String, String)]) -> String {
    let v: Vec<Vec<String>> = rows
        .iter()
        .map(|(a, b)| vec![a.clone(), b.clone()])
        .collect();
    arr2(&v)
}

fn photos(ps: &[Photo]) -> String {
    let v: Vec<Vec<String>> = ps
        .iter()
        .map(|p| vec![format!("/{}", p.file), p.caption.clone()])
        .collect();
    arr2(&v)
}

fn actions(a: &[Action]) -> String {
    let mut s = String::from("(");
    for x in a {
        let _ = write!(s, "({}, {}), ", lit(&x.title), arr(&x.steps));
    }
    s.push(')');
    s
}

/// The fixed part: page setup and one function per block type. Nothing in
/// here comes from the household.
pub const PRELUDE: &str = r##"
#let hw-sec = state("hw-sec", "")
#let hw-ink = luma(25)
#let hw-muted = luma(85)
#let hw-rule = luma(170)
#let hw-tint = luma(235)
#set text(font: "Lato", size: 11pt, fill: hw-ink, lang: "en", hyphenate: false)
#set par(leading: 0.62em, spacing: 0.9em)
#set table(stroke: 0.5pt + hw-rule, inset: (x: 5pt, y: 4pt))
#show table.cell.where(y: 0): set text(weight: "bold")

#let hw-section(short, title, body) = {
  pagebreak(weak: true)
  hw-sec.update(short)
  counter(page).update(1)
  block(below: 1.1em, {
    text(size: 22pt, weight: "bold", title)
    v(-0.5em)
    line(length: 100%, stroke: 1.2pt + hw-ink)
  })
  body
}
#let hw-h(t) = block(above: 1.4em, below: 0.6em, sticky: true, {
  line(length: 100%, stroke: 0.5pt + hw-rule)
  v(-0.2em)
  text(size: 15pt, weight: "bold", t)
})
#let hw-sub(t) = block(above: 0.9em, below: 0.4em, sticky: true, text(size: 12pt, weight: "bold", t))
#let hw-p(t) = par(t)
#let hw-note(t) = block(text(size: 10pt, fill: hw-muted, t))
#let hw-key(k) = align(center, block(stroke: 1.5pt, inset: 14pt, radius: 4pt, above: 16pt, below: 16pt,
  text(size: 17pt, weight: "bold", tracking: 1.5pt, k)))
#let hw-pairs(rows) = grid(
  columns: (32%, 1fr), column-gutter: 10pt, row-gutter: 6pt,
  ..rows.map(r => (text(fill: hw-muted, r.at(0)), r.at(1))).flatten()
)
#let hw-table(cols, rows) = table(
  columns: cols.len(), fill: (_, y) => if y == 0 { hw-tint },
  table.header(..cols),
  ..rows.flatten()
)
#let hw-check(cols, rows) = table(
  columns: (auto,) + (1fr,) * cols.len(), fill: (_, y) => if y == 0 { hw-tint },
  table.header([Done], ..cols),
  ..rows.map(r => (box(width: 10pt, height: 10pt, stroke: 0.8pt + hw-ink),) + r).flatten()
)
#let hw-agenda(groups) = columns(2, gutter: 14pt, {
  for g in groups {
    block(above: 0.9em, below: 0.35em, sticky: true, text(weight: "bold", size: 12pt, g.at(0)))
    for l in g.at(1) { block(above: 0.45em, below: 0.45em, par(hanging-indent: 1.6em, l)) }
  }
})
#let hw-bullets(items) = list(..items)
#let hw-steps(items) = enum(..items)
#let hw-photos(ps) = grid(
  columns: (1fr, 1fr, 1fr), gutter: 8pt,
  ..ps.map(p => block(breakable: false, {
    align(center, image(p.at(0), width: 100%, height: 48mm, fit: "contain"))
    align(center, text(size: 9pt, fill: hw-muted, p.at(1)))
  }))
)
#let hw-write(label) = block(above: 1em, below: 1.2em, grid(
  columns: (auto, 1fr), gutter: 6pt,
  text(weight: "bold", label), align(bottom, line(length: 100%, stroke: 0.6pt + hw-ink))
))
#let hw-cover(title, subtitle, lines) = {
  v(18%)
  text(size: 30pt, weight: "bold", title)
  v(0.2em)
  text(size: 16pt, subtitle)
  v(1.5em)
  for l in lines { par(text(size: 12pt, l)) }
}
#let hw-emergency-at(f, shutoffs, photos, calls, actions) = {
  set text(size: 13pt * f)
  set par(spacing: 0.5em, leading: 0.5em)
  grid(columns: (1.15fr, 1fr), gutter: 14pt,
    block({
      text(size: 16pt * f, weight: "bold")[Shut it off]
      v(0.2em)
      if shutoffs.len() == 0 {
        text(fill: hw-muted)[Not recorded yet. Write it here:]
        v(2.2em)
        line(length: 100%, stroke: 0.6pt + hw-ink)
      }
      for s in shutoffs {
        block(below: 0.8em, breakable: false, {
          text(weight: "bold", s.at(0))
          linebreak()
          s.at(1)
        })
      }
    }),
    block(fill: hw-tint, inset: 9pt, radius: 3pt, width: 100%, {
      text(size: 16pt * f, weight: "bold")[Who to call]
      v(0.2em)
      block(below: 0.8em, {
        text(weight: "bold")[Fire, gas, carbon monoxide, injury]
        linebreak()
        text(size: 15pt * f, weight: "bold")[911]
        text(size: 10pt * f)[ (or your local emergency number)]
      })
      for c in calls {
        block(below: 0.8em, breakable: false, {
          text(weight: "bold", c.at(0))
          linebreak()
          c.at(1)
        })
      }
    }),
  )
  if photos.len() > 0 {
    v(0.3em)
    grid(columns: (1fr,) * calc.max(photos.len(), 3), gutter: 8pt,
      ..photos.map(p => block(breakable: false, {
        align(center, image(p.at(0), width: 100%, height: 36mm * f, fit: "contain"))
        align(center, text(size: 9pt, fill: hw-muted, p.at(1)))
      })))
  }
  v(0.5em)
  set text(size: 12pt * f)
  grid(columns: (1fr, 1fr), gutter: 9pt,
    ..actions.map(a => block(stroke: 1.2pt + hw-ink, inset: 8pt, radius: 3pt, width: 100%, breakable: false, {
      text(size: 14pt * f, weight: "bold", a.at(0))
      v(0.15em)
      enum(spacing: 0.45em, ..a.at(1))
    })))
}
// The emergency page is one page, always (D-42): if a household has
// recorded a lot, the type gets smaller until it fits. 18mm is left for
// the section title above it.
#let hw-emergency(shutoffs, photos, calls, actions) = layout(size => {
  let room = size.height - 18mm
  let chosen = 0.55
  for f in (1.0, 0.94, 0.88, 0.82, 0.76, 0.7, 0.64, 0.58) {
    let h = measure(block(width: size.width, hw-emergency-at(f, shutoffs, photos, calls, actions))).height
    if h <= room {
      chosen = f
      break
    }
  }
  hw-emergency-at(chosen, shutoffs, photos, calls, actions)
})
"##;

fn block(out: &mut String, b: &Block) {
    match b {
        Block::Heading { text } => {
            let _ = writeln!(out, "#hw-h({})", lit(text));
        }
        Block::Subheading { text } => {
            let _ = writeln!(out, "#hw-sub({})", lit(text));
        }
        Block::Para { text } => {
            let _ = writeln!(out, "#hw-p({})", lit(text));
        }
        Block::Note { text } => {
            let _ = writeln!(out, "#hw-note({})", lit(text));
        }
        Block::RecoveryKey { key } => match key {
            Some(k) => {
                let _ = writeln!(out, "#hw-key({})", lit(k));
            }
            None => {
                let _ = writeln!(out, "#hw-p({})", lit(crate::doc::RECOVERY_NOT_HERE));
            }
        },
        Block::Pairs { rows } => {
            let _ = writeln!(out, "#hw-pairs({})", pairs(rows));
        }
        Block::Table { columns, rows } => {
            let _ = writeln!(out, "#hw-table({}, {})", arr(columns), arr2(rows));
        }
        Block::Agenda { groups } => {
            let mut g = String::from("(");
            for (h, lines) in groups {
                let _ = write!(g, "({}, {}), ", lit(h), arr(lines));
            }
            g.push(')');
            let _ = writeln!(out, "#hw-agenda({g})");
        }
        Block::Checklist { columns, rows } => {
            let _ = writeln!(out, "#hw-check({}, {})", arr(columns), arr2(rows));
        }
        Block::Bullets { items } => {
            let _ = writeln!(out, "#hw-bullets({})", arr(items));
        }
        Block::Steps { items } => {
            let _ = writeln!(out, "#hw-steps({})", arr(items));
        }
        Block::Photos { photos: ps } => {
            let _ = writeln!(out, "#hw-photos({})", photos(ps));
        }
        Block::WriteIn { label } => {
            let _ = writeln!(out, "#hw-write({})", lit(label));
        }
        Block::Cover {
            title,
            subtitle,
            lines,
        } => {
            let _ = writeln!(
                out,
                "#hw-cover({}, {}, {})",
                lit(title),
                lit(subtitle),
                arr(lines)
            );
        }
        Block::Emergency {
            shutoffs,
            photos: ps,
            calls,
            actions: a,
        } => {
            let _ = writeln!(
                out,
                "#hw-emergency({}, {}, {}, {})",
                pairs(shutoffs),
                photos(ps),
                pairs(calls),
                actions(a)
            );
        }
    }
}

fn section(out: &mut String, s: &Section) {
    let _ = writeln!(out, "#hw-section({}, {})[", lit(&s.short), lit(&s.title));
    for b in &s.blocks {
        block(out, b);
    }
    out.push_str("]\n");
}

/// The whole Typst source for one document.
pub fn source(doc: &Document, printed: &str, paper: Paper) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "#set document(title: {})", lit(&doc.title));
    out.push_str(PRELUDE);
    let footer_left = match doc.volume {
        Volume::Main => doc.title.clone(),
        Volume::Sealed => format!("SEALED. {}", doc.title),
    };
    let _ = writeln!(
        out,
        "#set page(paper: \"{}\", margin: (x: 18mm, top: 16mm, bottom: 20mm), footer: context {{\n  set text(size: 9pt, fill: hw-muted)\n  grid(columns: (1fr, auto), gutter: 8pt, {}, [Printed {} · #hw-sec.get() #counter(page).display()])\n}})",
        paper.typst(),
        lit(&footer_left),
        // `printed` is our own formatted date, but escape it anyway.
        format_args!("#{}", lit(printed)),
    );
    for s in &doc.sections {
        section(&mut out, s);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_escape_everything_that_means_something() {
        assert_eq!(lit("plain"), "\"plain\"");
        assert_eq!(lit("a\"b"), "\"a\\\"b\"");
        assert_eq!(lit("back\\slash"), "\"back\\\\slash\"");
        assert_eq!(lit("two\nlines"), "\"two\\nlines\"");
        assert_eq!(lit("bell\u{7}"), "\"bell \"");
        // Markup characters need nothing inside a string literal.
        assert_eq!(lit("#set $x$ *b* _i_ `c`"), "\"#set $x$ *b* _i_ `c`\"");
    }

    #[test]
    fn arrays_of_one_stay_arrays() {
        assert_eq!(arr(&["x".into()]), "(\"x\", )");
        assert_eq!(arr(&[]), "()");
    }
}
