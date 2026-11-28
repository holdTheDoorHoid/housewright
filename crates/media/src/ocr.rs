//! Reading the text on an appliance label, on the device (D-29, D-74).
//!
//! Uses `ocrs` (MIT/Apache-2.0) on the `rten` runtime (MIT/Apache-2.0),
//! with the ocrs project's pre-trained detection and recognition models
//! (CC BY-SA 4.0; see `docs/THIRD-PARTY.md`). The models are bundled with
//! the app, never downloaded at run time: Android compiles them into the
//! app; the web build serves them from the app's own origin and loads them
//! when label reading is first used.
//!
//! The engine only returns lines of text. Choosing brand, model and serial
//! is `housewright_core::labels::read_plate`, and the person always
//! confirms what was read.

use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use rten::Model;

use crate::photo;
use crate::{MediaError, Result};

/// The long edge a label photo is reduced to before reading. Rating plates
/// fill most of the frame; larger images only make reading slower.
pub const READ_EDGE: u32 = 1400;

/// A loaded OCR engine. Loading parses about 12 MB of model weights, so
/// front ends load it once, when label reading is first used.
pub struct Ocr {
    engine: OcrEngine,
}

fn err(e: impl std::fmt::Display) -> MediaError {
    MediaError::Ocr(e.to_string())
}

impl Ocr {
    /// From the bytes of `text-detection.onnx` and `text-recognition.onnx`.
    pub fn new(detection: Vec<u8>, recognition: Vec<u8>) -> Result<Self> {
        let detection_model = Model::load(detection).map_err(err)?;
        let recognition_model = Model::load(recognition).map_err(err)?;
        Self::with_models(detection_model, recognition_model)
    }

    /// From model bytes compiled into the program (Android).
    pub fn from_static(detection: &'static [u8], recognition: &'static [u8]) -> Result<Self> {
        let detection_model = Model::load_static_slice(detection).map_err(err)?;
        let recognition_model = Model::load_static_slice(recognition).map_err(err)?;
        Self::with_models(detection_model, recognition_model)
    }

    fn with_models(detection_model: Model, recognition_model: Model) -> Result<Self> {
        let engine = OcrEngine::new(OcrEngineParams {
            detection_model: Some(detection_model),
            recognition_model: Some(recognition_model),
            ..Default::default()
        })
        .map_err(err)?;
        Ok(Self { engine })
    }

    /// The lines of text in a photo, top to bottom.
    pub fn read_lines(&self, photo_bytes: &[u8]) -> Result<Vec<String>> {
        let img = photo::fit(&photo::decode(photo_bytes)?, READ_EDGE);
        let src = ImageSource::from_bytes(img.as_raw(), img.dimensions()).map_err(err)?;
        let input = self.engine.prepare_input(src).map_err(err)?;
        let words = self.engine.detect_words(&input).map_err(err)?;
        let lines = self.engine.find_text_lines(&input, &words);
        let texts = self.engine.recognize_text(&input, &lines).map_err(err)?;
        Ok(texts
            .into_iter()
            .flatten()
            .map(|l| l.to_string().trim().to_string())
            // Single characters are nearly always specks or edges.
            .filter(|l| l.chars().count() > 1)
            .collect())
    }
}
