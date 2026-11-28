//! Phase 4 commands: attachments, custom fields, the home facts sheet,
//! costs, QR stickers and label reading.

use std::collections::BTreeMap;

use housewright_core::backup;
use housewright_core::costs;
use housewright_core::facts;
use housewright_core::fields::{CustomValue, FieldDef, FieldType, FieldValue};
use housewright_core::ids::*;
use housewright_core::labels::{self, LabelTarget};
use housewright_core::model::*;
use housewright_core::records::{self, NewFile};
use housewright_core::template::Template;
use housewright_media::{photo, qr, sheet};
use serde_json::Value;

use crate::*;

/// Commands that take a file's bytes as well as JSON arguments.
pub const UPLOAD_COMMANDS: &[&str] = &["attach", "scan_label", "read_label", "restore_backup"];
/// Commands that give back bytes rather than JSON.
pub const DOWNLOAD_COMMANDS: &[&str] = &[
    "attachment_file",
    "full_backup",
    "label_sheet_pdf",
    "binder_pdf",
    "binder_bundle",
    "insurance_pdf",
    "insurance_bundle",
    "insurance_csv",
];

pub(crate) fn attachment_out(a: &Attachment) -> AttachmentOut {
    AttachmentOut {
        id: a.id.to_string(),
        name: a.name.clone(),
        media_type: a.media_type.clone(),
        size: a.size,
        kind: a.kind,
        has_thumb: a.thumb_sha256.is_some(),
        width: a.width,
        height: a.height,
        at: a.at,
        sensitive: a.sensitivity == Sensitivity::Sensitive,
    }
}

fn about_kind(s: &str) -> Result<EntityKind, ApiError> {
    match s {
        "item" => Ok(EntityKind::Item),
        "completion" => Ok(EntityKind::Completion),
        "fact" => Ok(EntityKind::Fact),
        "contractor" => Ok(EntityKind::Contractor),
        // A floor's traced-over background (D-25).
        "floor" => Ok(EntityKind::Floor),
        other => Err(ApiError::invalid(format!(
            "files can be added to a thing, a completion, a fact, a contractor or a floor, not '{other}'"
        ))),
    }
}

/// A machine key from a label: "Gate code" -> "gate_code".
fn key_from_label(label: &str) -> String {
    let mut k = String::new();
    for c in label.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            k.push(c);
        } else if !k.ends_with('_') && !k.is_empty() {
            k.push('_');
        }
    }
    let k = k.trim_end_matches('_').to_string();
    match k.chars().next() {
        Some(c) if c.is_ascii_lowercase() => k,
        _ => format!("field_{k}"),
    }
}

fn blank_to_none(s: Option<String>) -> Option<Option<String>> {
    s.map(|v| {
        let t = v.trim().to_string();
        if t.is_empty() { None } else { Some(t) }
    })
}

impl Api {
    pub(crate) fn records_call(&mut self, cmd: &str, a: Value) -> Result<Value, ApiError> {
        match cmd {
            "attachments" => {
                let a: AttachArgs = args(a)?;
                let about = EntityRef {
                    kind: about_kind(&a.about_kind)?,
                    id: a.about_id,
                };
                ok(self
                    .store
                    .attachments_about(&about)?
                    .iter()
                    .map(attachment_out)
                    .collect::<Vec<_>>())
            }
            "delete_attachment" => {
                let a: IdArgs = args(a)?;
                let id: AttachmentId = parse_id(&a.id, "attachment")?;
                let freed = self.store.delete_attachment(self.blobs.as_mut(), id)?;
                ok(serde_json::json!({ "freed": freed }))
            }
            "update_item" => {
                let a: UpdateItemArgs = args(a)?;
                ok(self.update_item(a)?)
            }
            "add_field" => {
                let a: AddFieldArgs = args(a)?;
                ok(self.add_field(a)?)
            }
            "remove_field" => {
                let a: ItemKeyArgs = args(a)?;
                let id: ItemId = parse_id(&a.item_id, "item")?;
                self.store.remove_item_field(id, &a.key)?;
                ok(self.item_detail(&a.item_id)?)
            }
            "reveal_field" => {
                let a: ItemKeyArgs = args(a)?;
                ok(self.reveal_field(a)?)
            }
            "add_details" => {
                let a: AddDetailsArgs = args(a)?;
                ok(self.add_details(a)?)
            }
            "facts" => ok(self.facts()?),
            "save_fact" => {
                let a: SaveFactArgs = args(a)?;
                ok(self.save_fact(a)?)
            }
            "delete_fact" => {
                let a: IdArgs = args(a)?;
                let id: FactId = parse_id(&a.id, "fact")?;
                let _: Fact = self.store.fetch(&id.to_string())?;
                self.store.delete(EntityKind::Fact, &id.to_string())?;
                ok(())
            }
            "reveal_fact" => {
                let a: IdArgs = args(a)?;
                self.may_see_sensitive()?;
                let f: Fact = self
                    .store
                    .fetch(&parse_id::<FactId>(&a.id, "fact")?.to_string())?;
                ok(RevealOut { value: f.value })
            }
            "costs" => ok(costs::summarise(&self.store)?),
            "set_currency" => {
                let a: CurrencyArgs = args(a)?;
                let c = a.currency.trim().to_ascii_uppercase();
                if c.len() != 3 || !c.chars().all(|x| x.is_ascii_uppercase()) {
                    return Err(ApiError::invalid(
                        "use a three-letter currency code, like USD",
                    ));
                }
                let mut h = self.household()?;
                h.currency = if c == "USD" { None } else { Some(c) };
                self.store.put(&h)?;
                ok(self.status()?)
            }
            "label_sheet" => {
                let a: ItemIdsArgs = args(a)?;
                ok(self.label_sheet(a.item_ids)?)
            }
            "find_label" => {
                let a: TextArgs = args(a)?;
                ok(self.find_label(&a.text)?)
            }
            c if UPLOAD_COMMANDS.contains(&c) || DOWNLOAD_COMMANDS.contains(&c) => Err(
                ApiError::invalid(format!("'{c}' sends or returns a file; use the file call")),
            ),
            other => Err(ApiError::invalid(format!("unknown command '{other}'"))),
        }
    }

    /// Run a command that takes a file (D-63's surface, plus bytes).
    pub fn call_upload(&mut self, cmd: &str, a: Value, bytes: &[u8]) -> Result<Value, ApiError> {
        if cmd == "restore_backup" {
            return ok(self.restore(bytes)?);
        }
        if !UPLOAD_COMMANDS.contains(&cmd) {
            return Err(ApiError::invalid(format!("'{cmd}' does not take a file")));
        }
        if self.store.meta("household_id")?.is_none() {
            return Err(ApiError {
                code: "not_initialized".into(),
                message: "set up a household first".into(),
            });
        }
        match cmd {
            "attach" => {
                let a: AttachArgs = args(a)?;
                ok(self.attach(a, bytes)?)
            }
            "scan_label" => ok(self.scan(bytes)?),
            "read_label" => ok(self.read_label(bytes)?),
            _ => unreachable!("listed above"),
        }
    }

    /// Run a command that returns bytes.
    pub fn call_download(&mut self, cmd: &str, a: Value) -> Result<Vec<u8>, ApiError> {
        if self.store.meta("household_id")?.is_none() {
            return Err(ApiError {
                code: "not_initialized".into(),
                message: "set up a household first".into(),
            });
        }
        if let Some(r) = self.binder_download(cmd, a.clone()) {
            return r;
        }
        if let Some(r) = self.insurance_download(cmd, a.clone()) {
            return r;
        }
        match cmd {
            "attachment_file" => {
                let a: FileArgs = args(a)?;
                let id: AttachmentId = parse_id(&a.id, "attachment")?;
                let att: Attachment = self.store.fetch(&id.to_string())?;
                let hash = match (&att.thumb_sha256, a.thumb) {
                    (Some(t), true) => t.clone(),
                    _ => att.sha256.clone(),
                };
                match self.blobs.get(&hash)? {
                    Some(b) => Ok(b),
                    // Not here yet: fetch it from the sync server (D-97).
                    None => self.fetch_missing_file(&hash).ok_or_else(|| ApiError {
                        code: "not_found".into(),
                        message: "this file's contents are not on this device yet".into(),
                    }),
                }
            }
            "full_backup" => Ok(backup::write(&self.store, self.blobs.as_ref())?),
            "label_sheet_pdf" => {
                let a: SheetPdfArgs = args(a)?;
                self.sheet_pdf(a)
            }
            other => Err(ApiError::invalid(format!(
                "'{other}' does not return a file"
            ))),
        }
    }

    fn may_see_sensitive(&self) -> Result<(), ApiError> {
        if self.me()?.role.can_see_sensitive() {
            Ok(())
        } else {
            Err(ApiError::invalid(
                "only the household's owner can see sensitive values".to_string(),
            ))
        }
    }

    // -- attachments ------------------------------------------------------

    fn attach(&mut self, a: AttachArgs, bytes: &[u8]) -> Result<AttachmentOut, ApiError> {
        let about = EntityRef {
            kind: about_kind(&a.about_kind)?,
            id: a.about_id,
        };
        let given = a.name.unwrap_or_default();
        let stem = given
            .rsplit_once('.')
            .map(|(s, _)| s.to_string())
            .unwrap_or_else(|| given.clone());
        let sensitivity = if a.sensitive {
            Sensitivity::Sensitive
        } else {
            Sensitivity::Normal
        };
        let att = if photo::sniff_image(bytes).is_some() {
            let p = photo::prepare(bytes).map_err(|e| ApiError::invalid(e.to_string()))?;
            self.store.add_attachment(
                self.blobs.as_mut(),
                NewFile {
                    about,
                    kind: a.kind.unwrap_or(AttachmentKind::Photo),
                    name: format!("{}.jpg", if stem.is_empty() { "photo" } else { &stem }),
                    media_type: "image/jpeg".into(),
                    bytes: &p.jpeg,
                    thumb: Some(&p.thumb),
                    width: Some(p.width),
                    height: Some(p.height),
                    sensitivity,
                },
            )?
        } else if photo::is_pdf(bytes) {
            self.store.add_attachment(
                self.blobs.as_mut(),
                NewFile {
                    about,
                    kind: a.kind.unwrap_or(AttachmentKind::Manual),
                    name: if given.trim().is_empty() {
                        "document.pdf".into()
                    } else {
                        given
                    },
                    media_type: "application/pdf".into(),
                    bytes,
                    thumb: None,
                    width: None,
                    height: None,
                    sensitivity,
                },
            )?
        } else {
            return Err(ApiError::invalid(
                "Housewright keeps photos (JPEG, PNG or WebP) and PDF documents.".to_string(),
            ));
        };
        Ok(attachment_out(&att))
    }

    // -- items and fields -------------------------------------------------

    pub(crate) fn field_edits(&self, item: &Item) -> Result<Vec<FieldEdit>, ApiError> {
        let tpl_keys: Vec<String> = match &item.template {
            Some(t) => self
                .store
                .get::<Template>(t)?
                .map(|t| t.fields.into_iter().map(|f| f.key).collect())
                .unwrap_or_default(),
            None => vec![],
        };
        let house_keys: Vec<String> = match &item.template {
            Some(t) => self
                .store
                .template_fields(t)?
                .into_iter()
                .map(|f| f.key)
                .collect(),
            None => vec![],
        };
        let mut out = Vec::new();
        for def in self.store.field_defs(item)? {
            let origin = if tpl_keys.contains(&def.key) {
                "template"
            } else if house_keys.contains(&def.key) {
                "household"
            } else {
                "item"
            };
            let v = item.custom.get(&def.key);
            let sensitive = v.map_or(def.sensitivity, |v| v.sensitivity) == Sensitivity::Sensitive;
            let photo = match v.map(|v| &v.value) {
                Some(FieldValue::Photo(id)) => self
                    .store
                    .get::<Attachment>(&id.to_string())?
                    .map(|a| attachment_out(&a)),
                _ => None,
            };
            out.push(FieldEdit {
                value: v.filter(|_| !sensitive).map(|v| raw_text(&v.value)),
                has_value: v.is_some(),
                sensitive,
                photo,
                origin,
                def,
            });
        }
        Ok(out)
    }

    fn update_item(&mut self, a: UpdateItemArgs) -> Result<UpdateItemOut, ApiError> {
        let id: ItemId = parse_id(&a.id, "item")?;
        let mut it: Item = self.store.fetch(&id.to_string())?;
        if let Some(n) = a.name {
            if n.trim().is_empty() {
                return Err(ApiError::invalid("give it a name".to_string()));
            }
            it.name = n.trim().to_string();
        }
        if let Some(v) = blank_to_none(a.make) {
            it.make = v;
        }
        if let Some(v) = blank_to_none(a.model) {
            it.model = v;
        }
        if let Some(v) = blank_to_none(a.serial) {
            let sens = it
                .serial
                .as_ref()
                .map(|s| s.sensitivity)
                .unwrap_or_default();
            it.serial = v.map(|value| Flagged {
                value,
                sensitivity: sens,
            });
        }
        let date = |s: String| -> Result<jiff::civil::Date, ApiError> {
            s.parse()
                .map_err(|e| ApiError::invalid(format!("'{s}' is not a date: {e}")))
        };
        if let Some(v) = blank_to_none(a.installed) {
            it.installed = v.map(date).transpose()?;
        }
        if let Some(v) = blank_to_none(a.purchased) {
            it.purchased = v.map(date).transpose()?;
        }
        let currency = self.household()?.currency().to_string();
        let amount = |raw: String| -> Result<Option<Money>, ApiError> {
            let raw = raw.trim().to_string();
            if raw.is_empty() {
                return Ok(None);
            }
            let minor = costs::parse_amount(&raw).ok_or_else(|| {
                ApiError::invalid(format!("'{raw}' is not an amount, like 1299.99"))
            })?;
            Ok(Some(costs::money(minor, &currency)))
        };
        if let Some(v) = a.price {
            it.price = amount(v)?;
        }
        if let Some(v) = a.value {
            it.value = amount(v)?;
        }
        match a.warranty {
            Some(w) if w.remove => it.warranty = None,
            Some(w) => {
                let ends = match w.ends.as_deref().map(str::trim) {
                    None | Some("") => None,
                    Some(e) => Some(date(e.to_string())?),
                };
                let mut nw = housewright_core::warranty::from_input(&it, w.months, ends)
                    .map_err(ApiError::invalid)?;
                if let Some(nw) = nw.as_mut() {
                    if let Some(p) = w.provider {
                        nw.provider = Some(p.trim().to_string()).filter(|p| !p.is_empty());
                    }
                    if let Some(t) = w.transferable {
                        nw.transferable = t;
                    }
                    if let Some(u) = w.register_url {
                        let u = u.trim();
                        nw.register_url = if u.is_empty() {
                            None
                        } else {
                            let full = if u.contains("://") {
                                u.to_string()
                            } else {
                                format!("https://{u}")
                            };
                            if !housewright_core::warranty::is_safe_link(&full) {
                                return Err(ApiError::invalid(
                                    "the registration link must be an https web address"
                                        .to_string(),
                                ));
                            }
                            Some(full)
                        };
                    }
                }
                it.warranty = nw;
            }
            None => {}
        }
        // A warranty given as a length follows the install and purchase dates.
        housewright_core::warranty::refresh(&mut it);
        if let Some(v) = blank_to_none(a.room_id) {
            it.room = match v {
                Some(r) => {
                    let r: RoomId = parse_id(&r, "room")?;
                    let _: Room = self.store.fetch(&r.to_string())?;
                    Some(r)
                }
                None => None,
            };
        }
        let defs = self.store.field_defs(&it)?;
        let mut values = Vec::new();
        for (k, raw) in a.values {
            let def = defs
                .iter()
                .find(|d| d.key == k)
                .ok_or_else(|| ApiError::invalid(format!("no field '{k}' on this thing")))?;
            let raw = raw.map(|r| r.trim().to_string()).filter(|r| !r.is_empty());
            let value = match raw {
                None => None,
                Some(r) => Some(match &def.field_type {
                    FieldType::Photo => FieldValue::Photo(parse_id(&r, "attachment")?),
                    t => parse_value(t, &r)?,
                }),
            };
            values.push((k, value, None));
        }
        let me = self.store.author()?;
        let p: Property = self.store.fetch(&it.property.to_string())?;
        let start = self.today_in(&p.time_zone)?;
        let (offered, stale) = self.store.batch(|s| {
            s.put(&it)?;
            if !values.is_empty() {
                s.set_item_values(id, values)?;
            }
            s.reconcile_conditional_tasks(id, me, start)
        })?;
        let names = self.names()?;
        let now = self.store.now();
        let cards = |ts: &[Task]| -> Result<Vec<Card>, ApiError> {
            ts.iter()
                .map(|t| Ok(self.card(t, &self.store.task_status(t, now)?, &names)))
                .collect()
        };
        Ok(UpdateItemOut {
            offered: cards(&offered)?,
            no_longer_apply: cards(&stale)?,
            detail: self.item_detail(&a.id)?,
        })
    }

    fn add_field(&mut self, a: AddFieldArgs) -> Result<ItemDetail, ApiError> {
        let id: ItemId = parse_id(&a.item_id, "item")?;
        let it: Item = self.store.fetch(&id.to_string())?;
        let label = a.label.trim().to_string();
        if label.is_empty() {
            return Err(ApiError::invalid("give the field a name".to_string()));
        }
        let field_type = match a.field_type.as_str() {
            "text" => FieldType::Text,
            "number" => FieldType::Number {
                unit: a
                    .unit
                    .map(|u| u.trim().to_string())
                    .filter(|u| !u.is_empty()),
            },
            "date" => FieldType::Date,
            "yes_no" => FieldType::YesNo,
            "choice" => FieldType::Choice {
                options: a
                    .options
                    .iter()
                    .map(|o| o.trim().to_string())
                    .filter(|o| !o.is_empty())
                    .collect(),
            },
            "photo" => FieldType::Photo,
            other => return Err(ApiError::invalid(format!("unknown field type '{other}'"))),
        };
        let existing: Vec<String> = self
            .store
            .field_defs(&it)?
            .into_iter()
            .map(|f| f.key)
            .collect();
        let base = key_from_label(&label);
        let mut key = base.clone();
        let mut n = 2;
        while existing.contains(&key) {
            key = format!("{base}_{n}");
            n += 1;
        }
        let sensitivity = match a.sensitive {
            Some(true) => Sensitivity::Sensitive,
            Some(false) => Sensitivity::Normal,
            None => facts::default_sensitivity("other", &label),
        };
        let def = FieldDef {
            key,
            label,
            field_type,
            sensitivity,
            help: None,
        };
        self.store.add_field(id, def, a.to_template)?;
        self.item_detail(&a.item_id)
    }

    fn reveal_field(&self, a: ItemKeyArgs) -> Result<RevealOut, ApiError> {
        self.may_see_sensitive()?;
        let id: ItemId = parse_id(&a.item_id, "item")?;
        let it: Item = self.store.fetch(&id.to_string())?;
        if a.key == "serial" {
            return Ok(RevealOut {
                value: it.serial.map(|s| s.value).unwrap_or_default(),
            });
        }
        let v = it
            .custom
            .get(&a.key)
            .ok_or_else(|| ApiError::invalid("that field has no value".to_string()))?;
        Ok(RevealOut {
            value: raw_text(&v.value),
        })
    }

    // -- completions --------------------------------------------------------

    fn add_details(&mut self, a: AddDetailsArgs) -> Result<TaskDetail, ApiError> {
        let cid: CompletionId = parse_id(&a.completion_id, "completion")?;
        let c: Completion = self.store.fetch(&cid.to_string())?;
        let task: Task = self.store.fetch(&c.task.to_string())?;
        let currency = self.household()?.currency().to_string();
        let costs = match a.costs {
            None => None,
            Some(ci) => {
                let mut lines = Vec::new();
                for (kind, raw) in [
                    (CostKind::Parts, ci.parts),
                    (CostKind::Labour, ci.labour),
                    (CostKind::Contractor, ci.contractor),
                    (CostKind::Other, ci.other),
                ] {
                    let Some(raw) = raw.filter(|r| !r.trim().is_empty()) else {
                        continue;
                    };
                    let minor = costs::parse_amount(&raw).ok_or_else(|| {
                        ApiError::invalid(format!("'{raw}' is not an amount, like 12.50"))
                    })?;
                    lines.push(CostLine {
                        kind,
                        amount: costs::money(minor, &currency),
                    });
                }
                (!lines.is_empty()).then_some(lines)
            }
        };
        let mut values = BTreeMap::new();
        for (k, raw) in a.values {
            let raw = raw.trim();
            if raw.is_empty() {
                continue;
            }
            let def = task
                .completion_fields
                .iter()
                .find(|d| d.key == k)
                .ok_or_else(|| ApiError::invalid(format!("no field '{k}' on this task")))?;
            values.insert(
                k.clone(),
                CustomValue {
                    value: parse_value(&def.field_type, raw)?,
                    sensitivity: def.sensitivity,
                },
            );
        }
        let note = a.note.filter(|n| !n.trim().is_empty());
        let who: Option<Option<ContractorId>> = match a.contractor_id.as_deref().map(str::trim) {
            None => None,
            Some("") => Some(None),
            Some(k) => Some(Some(parse_id(k, "contractor")?)),
        };
        // Only a change is recorded, so saving the form again adds nothing.
        let who = match who {
            Some(k) => {
                let details = self.store.completion_details(cid)?;
                let now_who = housewright_core::contractors::effective_contractor(&c, &details);
                (now_who != k).then_some(k)
            }
            None => None,
        };
        let me = self.store.author()?;
        let now = self.store.now();
        self.store.batch(|s| {
            if costs.is_some() || !values.is_empty() {
                s.add_completion_detail(cid, costs, values)?;
            }
            if let Some(k) = who {
                s.link_contractor(cid, k)?;
            }
            if let Some(n) = note {
                s.append(&Note {
                    id: NoteId::new(),
                    about: EntityRef {
                        kind: EntityKind::Completion,
                        id: cid.to_string(),
                    },
                    by: me,
                    at: now,
                    text: Flagged::normal(n.trim().to_string()),
                })?;
            }
            Ok(())
        })?;
        self.task_detail(&task.id.to_string())
    }

    /// The parts of a completion added after Done.
    pub(crate) fn completion_extras(
        &self,
        c: &Completion,
        task: &Task,
    ) -> Result<CompletionExtras, ApiError> {
        let atts = self
            .store
            .attachments_about(&EntityRef {
                kind: EntityKind::Completion,
                id: c.id.to_string(),
            })?
            .iter()
            .map(attachment_out)
            .collect();
        let details = self.store.completion_details(c.id)?;
        let costs = costs::effective_costs(c, &details)
            .into_iter()
            .map(|l| CostLineOut {
                kind: l.kind,
                minor: l.amount.minor,
                currency: l.amount.currency,
            })
            .collect();
        let values = records::effective_values(c, &details)
            .into_iter()
            .map(|(k, v)| {
                let sensitive = v.sensitivity == Sensitivity::Sensitive;
                FieldOut {
                    label: task
                        .completion_fields
                        .iter()
                        .find(|d| d.key == k)
                        .map(|d| d.label.clone())
                        .unwrap_or_else(|| k.clone()),
                    key: k,
                    value: if sensitive {
                        String::new()
                    } else {
                        value_text(&v.value)
                    },
                    sensitive,
                }
            })
            .collect();
        Ok((atts, costs, values))
    }

    // -- facts ------------------------------------------------------------

    fn fact_out(&self, f: &Fact) -> Result<FactOut, ApiError> {
        let sensitive = f.sensitivity == Sensitivity::Sensitive;
        Ok(FactOut {
            id: f.id.to_string(),
            group: f.category.clone(),
            label: f.label.clone(),
            value: (!sensitive).then(|| f.value.clone()),
            sensitive,
            key: f.key.clone(),
            room_id: f.room.map(|r| r.to_string()),
            pin_id: f.pin.map(|p| p.to_string()),
            room_name: match f.room {
                Some(r) => self.store.get::<Room>(&r.to_string())?.map(|r| r.name),
                None => None,
            },
            attachments: self
                .store
                .attachments_about(&EntityRef {
                    kind: EntityKind::Fact,
                    id: f.id.to_string(),
                })?
                .iter()
                .map(attachment_out)
                .collect(),
        })
    }

    fn facts(&self) -> Result<FactsOut, ApiError> {
        let p = self.first_property()?;
        let all: Vec<Fact> = self
            .store
            .list::<Fact>()?
            .into_iter()
            .filter(|f| f.property == p.id)
            .collect();
        let mut groups = Vec::new();
        for (key, label, sens) in facts::GROUPS {
            let mut fs: Vec<&Fact> = all
                .iter()
                .filter(|f| {
                    f.category == *key || (*key == "other" && !facts::is_group(&f.category))
                })
                .collect();
            fs.sort_by_key(|f| f.label.to_lowercase());
            groups.push(FactGroupOut {
                key: key.to_string(),
                label: label.to_string(),
                sensitive_by_default: *sens,
                facts: fs
                    .into_iter()
                    .map(|f| self.fact_out(f))
                    .collect::<Result<_, _>>()?,
            });
        }
        let items: Vec<Item> = self.store.list()?;
        let profile = self.store.profile(p.id)?;
        Ok(FactsOut {
            property_name: p.name,
            groups,
            prompts: facts::prompts(profile.as_ref(), &items, &all),
        })
    }

    fn save_fact(&mut self, a: SaveFactArgs) -> Result<FactOut, ApiError> {
        let p = self.first_property()?;
        let label = a.label.trim().to_string();
        if label.is_empty() {
            return Err(ApiError::invalid("give it a name".to_string()));
        }
        let group = if facts::is_group(&a.group) {
            a.group.clone()
        } else {
            "other".to_string()
        };
        let existing = match &a.id {
            Some(id) => Some(
                self.store
                    .fetch::<Fact>(&parse_id::<FactId>(id, "fact")?.to_string())?,
            ),
            None => None,
        };
        let value = match (a.value, &existing) {
            (Some(v), _) => v.trim().to_string(),
            (None, Some(f)) => f.value.clone(),
            (None, None) => return Err(ApiError::invalid("write the answer".to_string())),
        };
        if value.is_empty() {
            return Err(ApiError::invalid("write the answer".to_string()));
        }
        let sensitivity = match a.sensitive {
            Some(true) => Sensitivity::Sensitive,
            Some(false) => Sensitivity::Normal,
            None => match &existing {
                Some(f) => f.sensitivity,
                None => facts::default_sensitivity(&group, &label),
            },
        };
        let room = match a.room_id.filter(|r| !r.is_empty()) {
            Some(r) => {
                let r: RoomId = parse_id(&r, "room")?;
                let _: Room = self.store.fetch(&r.to_string())?;
                Some(r)
            }
            None => None,
        };
        let pin = match a.pin_id.as_deref() {
            None => existing.as_ref().and_then(|f| f.pin),
            Some("") => None,
            Some(id) => {
                let id: PlanPinId = parse_id(id, "pin")?;
                let _: PlanPin = self.store.fetch(&id.to_string())?;
                Some(id)
            }
        };
        let f = Fact {
            pin,
            id: existing.as_ref().map(|f| f.id).unwrap_or_else(FactId::new),
            property: existing.as_ref().map(|f| f.property).unwrap_or(p.id),
            category: group,
            label,
            value,
            sensitivity,
            key: a
                .key
                .filter(|k| !k.is_empty())
                .or_else(|| existing.as_ref().and_then(|f| f.key.clone())),
            room,
        };
        self.store.put(&f)?;
        self.fact_out(&f)
    }

    // -- labels -----------------------------------------------------------

    fn chosen_items(&self, ids: Option<Vec<String>>) -> Result<Vec<Item>, ApiError> {
        let mut items: Vec<Item> = match ids {
            None => self.store.list()?,
            Some(ids) => ids
                .iter()
                .map(|i| {
                    let id: ItemId = parse_id(i, "item")?;
                    Ok(self.store.fetch::<Item>(&id.to_string())?)
                })
                .collect::<Result<_, ApiError>>()?,
        };
        items.sort_by_key(|i| i.name.to_lowercase());
        Ok(items)
    }

    fn label_sheet(&self, ids: Option<Vec<String>>) -> Result<LabelSheetOut, ApiError> {
        let names = self.names()?;
        let mut stickers = Vec::new();
        for it in self.chosen_items(ids)? {
            let url = labels::item_url(it.id);
            stickers.push(StickerOut {
                item_id: it.id.to_string(),
                name: it.name.clone(),
                room_name: it.room.and_then(|r| names.rooms.get(&r).cloned()),
                code: labels::short_code(it.id),
                qr_svg: qr::svg(&url).map_err(|e| ApiError::internal(e.to_string()))?,
                url,
            });
        }
        Ok(LabelSheetOut {
            presets: sheet::presets()
                .into_iter()
                .map(|(k, l, layout)| SheetPreset {
                    key: k.into(),
                    label: l.into(),
                    layout,
                })
                .collect(),
            stickers,
        })
    }

    fn sheet_pdf(&self, a: SheetPdfArgs) -> Result<Vec<u8>, ApiError> {
        let layout = match (a.layout, a.preset) {
            (Some(l), _) => l,
            (None, p) => {
                let key = p.unwrap_or_else(|| "letter-30".into());
                sheet::presets()
                    .into_iter()
                    .find(|(k, _, _)| *k == key)
                    .map(|(_, _, l)| l)
                    .ok_or_else(|| ApiError::invalid(format!("no sheet layout '{key}'")))?
            }
        };
        let stickers: Vec<sheet::Sticker> = self
            .chosen_items(a.item_ids)?
            .into_iter()
            .map(|it| sheet::Sticker {
                url: labels::item_url(it.id),
                code: labels::short_code(it.id),
                name: it.name,
            })
            .collect();
        sheet::pdf(&layout, &stickers).map_err(|e| ApiError::invalid(e.to_string()))
    }

    /// Things a sticker's text points at: our URL, or a typed short code.
    fn find_label(&self, text: &str) -> Result<Vec<ItemRow>, ApiError> {
        let rows = self.items()?;
        Ok(match labels::parse_label(text) {
            Some(LabelTarget::Item(id)) => rows
                .into_iter()
                .filter(|r| r.id == id.to_string())
                .collect(),
            Some(LabelTarget::Code(code)) => rows
                .into_iter()
                .filter(|r| {
                    r.id.parse::<ItemId>()
                        .is_ok_and(|i| labels::short_code(i) == code)
                })
                .collect(),
            None => vec![],
        })
    }

    fn scan(&self, bytes: &[u8]) -> Result<ScanOut, ApiError> {
        let texts = qr::decode_photo(bytes).map_err(|e| ApiError::invalid(e.to_string()))?;
        let mut items = Vec::new();
        for t in &texts {
            for r in self.find_label(t)? {
                if !items.iter().any(|x: &ItemRow| x.id == r.id) {
                    items.push(r);
                }
            }
        }
        Ok(ScanOut { texts, items })
    }

    fn read_label(&mut self, bytes: &[u8]) -> Result<LabelReadOut, ApiError> {
        if photo::sniff_image(bytes).is_none() {
            return Err(ApiError::invalid("use a photo of the label".to_string()));
        }
        if self.ocr.is_none() {
            let load = self.ocr_loader.as_ref().ok_or_else(|| ApiError {
                code: "ocr_unavailable".into(),
                message: "label reading is not available in this version of the app".into(),
            })?;
            self.ocr = Some(load().map_err(|e| ApiError::internal(e.to_string()))?);
        }
        let ocr = self.ocr.as_ref().expect("loaded above");
        let lines = ocr
            .read_lines(bytes)
            .map_err(|e| ApiError::invalid(e.to_string()))?;
        let r = labels::read_plate(&lines);
        let template = match &r.template {
            Some(t) => self.library.get(t).map(|t| TemplateRef {
                id: t.id.clone(),
                name: t.name.clone(),
            }),
            None => None,
        };
        Ok(LabelReadOut {
            lines,
            brand: r.brand,
            model: r.model,
            serial: r.serial,
            model_alternatives: r.model_alternatives,
            serial_alternatives: r.serial_alternatives,
            template,
        })
    }

    // -- backup -----------------------------------------------------------

    fn restore(&mut self, bytes: &[u8]) -> Result<StatusOut, ApiError> {
        if backup::is_zip(bytes) {
            backup::restore(&mut self.store, self.blobs.as_mut(), bytes)?;
            self.status()
        } else {
            let text = std::str::from_utf8(bytes)
                .map_err(|_| ApiError::invalid("this is not a Housewright export or backup"))?;
            self.import(text)
        }
    }
}

/// A completion's photos, costs and field values.
pub(crate) type CompletionExtras = (Vec<AttachmentOut>, Vec<CostLineOut>, Vec<FieldOut>);

/// A value as the edit form shows it (and accepts it back).
pub(crate) fn raw_text(v: &FieldValue) -> String {
    match v {
        FieldValue::Text(s) | FieldValue::Choice(s) => s.clone(),
        FieldValue::Number { value, .. } => value.to_string(),
        FieldValue::Date(d) => d.to_string(),
        FieldValue::YesNo(b) => if *b { "yes" } else { "no" }.into(),
        FieldValue::Photo(id) => id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::key_from_label;

    #[test]
    fn keys_from_labels() {
        assert_eq!(key_from_label("Gate code"), "gate_code");
        assert_eq!(key_from_label("  Filter size (in) "), "filter_size_in");
        assert_eq!(key_from_label("2nd filter"), "field_2nd_filter");
        assert_eq!(key_from_label("Ünïcode"), "n_code");
    }
}
