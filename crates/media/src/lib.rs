//! Pixel work for Housewright, shared by Android and the web so both behave
//! the same: photos are downscaled on the device before they are stored,
//! QR codes are drawn and read, sticker sheets are written as PDF, and
//! appliance labels are read with an offline OCR engine (D-29, D-74).
//!
//! Nothing here touches storage or the network. Licensed AGPL-3.0-only.

pub mod ocr;
pub mod photo;
pub mod qr;
pub mod sheet;

/// What can go wrong with a picture or a file.
#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("this does not look like a photo Housewright can read ({0})")]
    Decode(String),
    #[error("could not save the photo: {0}")]
    Encode(String),
    #[error("{0}")]
    Invalid(String),
    #[error("label reading failed: {0}")]
    Ocr(String),
}

pub type Result<T> = std::result::Result<T, MediaError>;
