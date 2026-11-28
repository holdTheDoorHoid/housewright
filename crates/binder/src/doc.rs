//! The binder's document model: what is printed, independent of how.
//!
//! Builders ([`crate::build`]) turn household data into a [`Document`];
//! [`crate::typst`] turns a document into Typst source. Keeping the two
//! apart means the safety tests (nothing sensitive in the main volume)
//! can look at plain strings, and a section's fingerprint is a hash of
//! exactly what it would print (never of the printed-on date).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Which of the two volumes (D-41).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Volume {
    /// Safe on a shelf: never anything marked sensitive.
    Main,
    /// Only the sensitive material, for a safe or a sealed envelope.
    Sealed,
}

impl Volume {
    pub fn as_str(self) -> &'static str {
        match self {
            Volume::Main => "main",
            Volume::Sealed => "sealed",
        }
    }
}

/// The editions (D-42).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edition {
    /// The emergency page alone.
    Emergency,
    /// The full household binder (main + sealed).
    Full,
    /// For a buyer or new tenant: no personal details, no sealed volume.
    Handover,
    /// For someone inheriting the house: the full binder plus accounts and
    /// utilities to transfer (in the sealed volume).
    Succession,
}

impl Edition {
    pub const ALL: [Edition; 4] = [
        Edition::Emergency,
        Edition::Full,
        Edition::Handover,
        Edition::Succession,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Edition::Emergency => "emergency",
            Edition::Full => "full",
            Edition::Handover => "handover",
            Edition::Succession => "succession",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.as_str() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            Edition::Emergency => "Emergency page",
            Edition::Full => "Household binder",
            Edition::Handover => "Handover binder",
            Edition::Succession => "Succession binder",
        }
    }

    /// Whether this edition has a sealed volume at all.
    pub fn has_sealed(self) -> bool {
        matches!(self, Edition::Full | Edition::Succession)
    }
}

/// A photo to print. `file` names the scaled image handed to the renderer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Photo {
    /// `img/<sha256>.jpg`: content-addressed, so the fingerprint changes
    /// when the photo does.
    pub file: String,
    pub caption: String,
}

/// One emergency action box ("Gas smell").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    pub title: String,
    pub steps: Vec<String>,
}

/// A floor plan drawn on the page (D-25). Every coordinate is plan
/// millimetres, shifted so the drawing starts at (0, 0); the typesetter
/// scales it to fit. Only numbers and the room names, areas and label texts
/// (all printed as plain strings) come from the household.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Drawing {
    pub width: i64,
    pub height: i64,
    pub rooms: Vec<DrawnRoom>,
    /// Walls: (x1, y1, x2, y2, thickness).
    pub walls: Vec<[i64; 5]>,
    /// Gaps cut in walls for doors, windows and openings: (x1, y1, x2, y2, width).
    pub gaps: Vec<[i64; 5]>,
    /// The glass line of each window: (x1, y1, x2, y2).
    pub windows: Vec<[i64; 4]>,
    /// Door leaves and their swing, as open polylines.
    pub swings: Vec<Vec<[i64; 2]>>,
    pub pins: Vec<DrawnPin>,
    pub labels: Vec<DrawnLabel>,
    /// A scale bar: its length in plan millimetres and what it says.
    pub bar: (i64, String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawnRoom {
    pub points: Vec<[i64; 2]>,
    pub name: String,
    pub area: String,
    pub at: [i64; 2],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawnPin {
    pub at: [i64; 2],
    pub number: u32,
    /// Shown dark: a shutoff on the emergency page.
    pub strong: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawnLabel {
    pub at: [i64; 2],
    pub text: String,
}

/// The building blocks of a section.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    /// A heading inside a section (a thing's name, a group).
    Heading {
        text: String,
    },
    /// A smaller heading under a [`Block::Heading`].
    Subheading {
        text: String,
    },
    Para {
        text: String,
    },
    /// Quieter text: explanations, "nothing recorded yet".
    Note {
        text: String,
    },
    /// Label and value pairs, in two columns.
    Pairs {
        rows: Vec<(String, String)>,
    },
    /// A table with a header row.
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Bullets {
        items: Vec<String>,
    },
    Steps {
        items: Vec<String>,
    },
    Photos {
        photos: Vec<Photo>,
    },
    /// A label and a blank line to write on by hand.
    WriteIn {
        label: String,
    },
    /// Groups of short lines in two columns (the 12-month calendar).
    Agenda {
        groups: Vec<(String, Vec<String>)>,
    },
    /// A table with a tick box per row (accounts to transfer).
    Checklist {
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    /// The emergency page's large-type layout.
    Emergency {
        shutoffs: Vec<(String, String)>,
        photos: Vec<Photo>,
        calls: Vec<(String, String)>,
        actions: Vec<Action>,
    },
    /// The recovery key, large, in a box (sealed volume only, D-91). The
    /// key itself is left out of the fingerprint, so the copy printed at
    /// setup (with the key) and later reprints (without it) count as the
    /// same section and nobody is nudged to replace the page with the key.
    RecoveryKey {
        #[serde(skip)]
        key: Option<String>,
    },
    /// A floor plan drawing.
    Plan {
        drawing: Drawing,
    },
    /// A big title for a volume's cover.
    Cover {
        title: String,
        subtitle: String,
        lines: Vec<String>,
    },
}

/// Printed in place of the key on every print but the one made at setup.
pub const RECOVERY_NOT_HERE: &str = "The key was printed on the copy of this page made when sync was set up. Keep that copy here; this reprint does not show it.";

impl Block {
    /// Every string the block would print.
    pub fn texts(&self, out: &mut Vec<String>) {
        let pairs = |out: &mut Vec<String>, rows: &[(String, String)]| {
            for (a, b) in rows {
                out.push(a.clone());
                out.push(b.clone());
            }
        };
        match self {
            Block::Heading { text }
            | Block::Subheading { text }
            | Block::Para { text }
            | Block::Note { text }
            | Block::WriteIn { label: text } => out.push(text.clone()),
            Block::Pairs { rows } => pairs(out, rows),
            Block::Agenda { groups } => {
                for (h, lines) in groups {
                    out.push(h.clone());
                    out.extend(lines.iter().cloned());
                }
            }
            Block::Table { columns, rows } | Block::Checklist { columns, rows } => {
                out.extend(columns.iter().cloned());
                for r in rows {
                    out.extend(r.iter().cloned());
                }
            }
            Block::Bullets { items } | Block::Steps { items } => out.extend(items.iter().cloned()),
            Block::Photos { photos } => out.extend(photos.iter().map(|p| p.caption.clone())),
            Block::Emergency {
                shutoffs,
                photos,
                calls,
                actions,
            } => {
                pairs(out, shutoffs);
                out.extend(photos.iter().map(|p| p.caption.clone()));
                pairs(out, calls);
                for a in actions {
                    out.push(a.title.clone());
                    out.extend(a.steps.iter().cloned());
                }
            }
            Block::Plan { drawing } => {
                for r in &drawing.rooms {
                    out.push(r.name.clone());
                    out.push(r.area.clone());
                }
                out.extend(drawing.labels.iter().map(|l| l.text.clone()));
                out.push(drawing.bar.1.clone());
            }
            Block::RecoveryKey { key } => {
                out.push(key.clone().unwrap_or_else(|| RECOVERY_NOT_HERE.to_string()))
            }
            Block::Cover {
                title,
                subtitle,
                lines,
            } => {
                out.push(title.clone());
                out.push(subtitle.clone());
                out.extend(lines.iter().cloned());
            }
        }
    }

    pub fn photos(&self) -> Vec<&Photo> {
        match self {
            Block::Photos { photos } | Block::Emergency { photos, .. } => photos.iter().collect(),
            _ => vec![],
        }
    }
}

/// A loose-leaf section: starts on its own page, numbered on its own
/// ("Water 3"), so it can be reprinted alone (D-43).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    /// Stable key, used for change tracking (`things:plumbing`).
    pub key: String,
    /// Short name for the page footer ("Water").
    pub short: String,
    /// Title at the top of its first page.
    pub title: String,
    pub blocks: Vec<Block>,
}

impl Section {
    pub fn new(key: impl Into<String>, short: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            short: short.into(),
            title: title.into(),
            blocks: Vec::new(),
        }
    }

    pub fn push(&mut self, b: Block) {
        self.blocks.push(b);
    }

    pub fn texts(&self) -> Vec<String> {
        let mut out = vec![self.short.clone(), self.title.clone()];
        for b in &self.blocks {
            b.texts(&mut out);
        }
        out
    }

    /// A hash of exactly what this section prints. The printed-on date is
    /// not part of it, so reprinting unchanged content gives the same value.
    pub fn fingerprint(&self) -> String {
        let json = serde_json::to_vec(self).expect("sections serialise");
        let mut h = Sha256::new();
        h.update(&json);
        let d = h.finalize();
        d.iter().take(16).map(|b| format!("{b:02x}")).collect()
    }
}

/// One printable volume of one edition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub edition: Edition,
    pub volume: Volume,
    /// The PDF's title ("Household binder: 12 Elm Street").
    pub title: String,
    pub sections: Vec<Section>,
}

impl Document {
    /// Every string the document would print, in order.
    pub fn texts(&self) -> Vec<String> {
        let mut out = vec![self.title.clone()];
        for s in &self.sections {
            out.extend(s.texts());
        }
        out
    }

    /// Only the named sections, in their usual order (a partial reprint).
    pub fn only(&self, keys: &[String]) -> Document {
        Document {
            sections: self
                .sections
                .iter()
                .filter(|s| keys.contains(&s.key))
                .cloned()
                .collect(),
            ..self.clone()
        }
    }

    pub fn photo_files(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .sections
            .iter()
            .flat_map(|s| s.blocks.iter().flat_map(|b| b.photos()))
            .map(|p| p.file.clone())
            .collect();
        v.sort();
        v.dedup();
        v
    }
}
