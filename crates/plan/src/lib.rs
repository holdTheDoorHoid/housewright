//! Floor plan geometry for Housewright (D-25). Pure: no I/O, no clock.
//!
//! Every coordinate and length is a whole number of **millimetres** (`i64`),
//! so the numbers stored in the operation log are exact and two devices that
//! merge the same edits hold identical plans (no floating-point drift). The
//! plan's x axis points right and its y axis points **down**, as on a screen
//! and in SVG; angles shown to people are measured the way they see them
//! (counter-clockwise on the screen is positive).
//!
//! Floating point is used only for derived values that are shown, never
//! stored: lengths of slanted walls, angles, areas in square metres.
//!
//! Licensed AGPL-3.0-only.

pub mod faces;
pub mod geom;
pub mod shapes;
pub mod snap;
pub mod units;

pub use faces::faces;
pub use geom::*;
pub use shapes::{l_shape, rectangle};
pub use snap::{SnapKind, SnapOptions, Snapped, snap};
pub use units::{Units, format_area, format_length, parse_length};
