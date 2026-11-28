//! The house binder's typesetter for the web (D-80).
//!
//! Typst adds about 11 MB (gzipped) to WebAssembly, more than three times
//! the rest of the app. Most people print a binder once or twice a year, so
//! the web app keeps Typst out of its main module: the main module makes a
//! *bundle* (Typst source, photos, receipt; see
//! `housewright_binder::bundle`) and this separate module, fetched from the
//! app's own origin the first time someone prints, turns it into a PDF.
//! Android and the desktop typeset in-process with the same code.
//!
//! Licensed AGPL-3.0-only.

#[cfg(target_arch = "wasm32")]
mod wasm {
    use housewright_binder::bundle::Bundle;
    use housewright_binder::render::render_bundle;
    use wasm_bindgen::prelude::*;

    /// A bundle's PDF.
    #[wasm_bindgen]
    pub fn render(bundle: &[u8]) -> Result<Vec<u8>, JsValue> {
        render_bundle(bundle)
            .map(|r| r.pdf)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// A bundle's receipt, as JSON, to record once the PDF is made.
    #[wasm_bindgen]
    pub fn receipt(bundle: &[u8]) -> Result<String, JsValue> {
        let b = Bundle::from_zip(bundle).map_err(|e| JsValue::from_str(&e.to_string()))?;
        serde_json::to_string(&b.receipt).map_err(|e| JsValue::from_str(&e.to_string()))
    }
}
