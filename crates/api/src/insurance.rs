//! Phase 9: the insurance inventory export (D-27), as a PDF and a CSV.
//!
//! Sensitive things and fields are left out unless `include_sensitive` is
//! set, and only the household's owner may set it (D-23, like the sealed
//! volume). Limited members cannot export.

use housewright_binder::insurance::{self, InsuranceOptions};
use housewright_binder::{self as binder, Paper};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::*;

/// Thumbnails in the inventory print about 30 mm wide: 400 pixels is
/// plenty and keeps the PDF small.
const THUMB_EDGE: u32 = 400;

#[derive(Deserialize, Default)]
pub struct InsuranceArgs {
    #[serde(default)]
    pub include_sensitive: bool,
    #[serde(default)]
    pub paper: Option<Paper>,
    #[serde(default)]
    pub property_id: Option<String>,
}

#[derive(Serialize)]
pub struct InsuranceOut {
    pub property_name: String,
    pub things: usize,
    pub with_photo: usize,
    pub with_value: usize,
    pub with_receipt: usize,
    pub total_value: i64,
    pub total_price: i64,
    pub currency: String,
    /// Things marked sensitive (left out unless asked for).
    pub sensitive_things: usize,
    pub sensitive_details: usize,
    /// Only the owner may include sensitive items.
    pub can_include_sensitive: bool,
    pub filename: String,
}

impl Api {
    fn insurance_data(
        &self,
        a: &InsuranceArgs,
    ) -> Result<(binder::HouseData, insurance::Inventory, InsuranceOptions), ApiError> {
        let me = self.me()?;
        if !me.role.can_edit_plan() {
            return Err(ApiError::invalid(
                "ask the household's owner for the inventory".to_string(),
            ));
        }
        if a.include_sensitive && !me.role.can_see_sensitive() {
            return Err(ApiError::invalid(
                "only the household's owner can include sensitive items".to_string(),
            ));
        }
        let pid = a
            .property_id
            .as_deref()
            .map(|p| parse_id::<PropertyId>(p, "property"))
            .transpose()?;
        let data = binder::gather(&self.store, pid, self.store.now())?;
        let opts = InsuranceOptions {
            include_sensitive: a.include_sensitive,
        };
        let inv = insurance::inventory(&data, opts);
        Ok((data, inv, opts))
    }

    pub(crate) fn insurance_summary(&self) -> Result<InsuranceOut, ApiError> {
        let (data, inv, _) = self.insurance_data(&InsuranceArgs::default())?;
        Ok(InsuranceOut {
            property_name: data.property.name.clone(),
            things: inv.lines.len(),
            with_photo: inv.lines.iter().filter(|l| l.photo.is_some()).count(),
            with_value: inv.lines.iter().filter(|l| l.value.is_some()).count(),
            with_receipt: inv.lines.iter().filter(|l| !l.receipts.is_empty()).count(),
            total_value: inv.total_value,
            total_price: inv.total_price,
            currency: inv.currency.clone(),
            sensitive_things: inv.withheld_items,
            sensitive_details: inv.withheld_details,
            can_include_sensitive: self.me()?.role.can_see_sensitive(),
            filename: format!("home-inventory-{}", data.today),
        })
    }

    fn insurance_bundle(&self, a: InsuranceArgs) -> Result<binder::bundle::Bundle, ApiError> {
        let (data, inv, opts) = self.insurance_data(&a)?;
        let doc = insurance::document(&data, &inv, opts);
        if !opts.include_sensitive {
            // The same last check as the binder's main volume (D-83).
            binder::check_main(&data, &doc).map_err(|e| ApiError::internal(e.to_string()))?;
        }
        let images = self.print_images_at(&doc, THUMB_EDGE)?;
        let paper = match a.paper {
            Some(p) => p,
            None => match self.store.meta("binder_paper")?.as_deref() {
                Some("a4") => Paper::A4,
                _ => Paper::Letter,
            },
        };
        Ok(binder::prepare(
            &doc,
            data.property.id,
            data.today,
            paper,
            &images,
        ))
    }

    pub(crate) fn insurance_download(
        &mut self,
        cmd: &str,
        a: Value,
    ) -> Option<Result<Vec<u8>, ApiError>> {
        Some(match cmd {
            "insurance_csv" => args(a).and_then(|a: InsuranceArgs| {
                let (_, inv, _) = self.insurance_data(&a)?;
                Ok(insurance::csv(&inv).into_bytes())
            }),
            "insurance_bundle" => args(a).and_then(|a: InsuranceArgs| {
                self.insurance_bundle(a)?
                    .to_zip()
                    .map_err(|e| ApiError::internal(e.to_string()))
            }),
            "insurance_pdf" => args(a).and_then(|a: InsuranceArgs| self.insurance_pdf(a)),
            _ => return None,
        })
    }

    #[cfg(feature = "typeset")]
    fn insurance_pdf(&self, a: InsuranceArgs) -> Result<Vec<u8>, ApiError> {
        let b = self.insurance_bundle(a)?;
        binder::render::render(&b.source, &b.files, b.receipt.printed_on)
            .map(|r| r.pdf)
            .map_err(|e| ApiError::internal(e.to_string()))
    }

    #[cfg(not(feature = "typeset"))]
    fn insurance_pdf(&self, _a: InsuranceArgs) -> Result<Vec<u8>, ApiError> {
        Err(ApiError::invalid(
            "this build typesets in its separate module: use insurance_bundle".to_string(),
        ))
    }
}
