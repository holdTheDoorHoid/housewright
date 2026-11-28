//! Phase 4 records on top of [`Store`]: attachments, completion details,
//! custom fields and conditional tasks.

use std::collections::BTreeSet;

use jiff::civil::Date;

use crate::blobs::{self, BlobStore};
use crate::error::{Error, Result};
use crate::fields::{CustomValue, CustomValues, FieldDef, FieldValue};
use crate::ids::*;
use crate::model::*;
use crate::store::Store;
use crate::template::Template;

/// The largest document we store as-is (D-72): a scanned manual is often
/// 5-15 MB; 25 MB leaves room without letting one file fill a phone.
pub const MAX_DOCUMENT_BYTES: usize = 25 * 1024 * 1024;

/// What is being stored, after any downscaling (done by the caller, so
/// this crate stays free of image code).
pub struct NewFile<'a> {
    pub about: EntityRef,
    pub kind: AttachmentKind,
    pub name: String,
    pub media_type: String,
    pub bytes: &'a [u8],
    pub thumb: Option<&'a [u8]>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub sensitivity: Sensitivity,
    /// Where it was downloaded from (a manufacturer's manual, D-151).
    pub source: Option<String>,
}

impl Store {
    /// Check that an attachment's subject exists.
    fn check_about(&self, about: &EntityRef) -> Result<()> {
        let ok = match self.raw(about.kind, &about.id)? {
            Some(r) => !r.deleted,
            None => false,
        };
        if ok {
            Ok(())
        } else {
            Err(Error::NotFound(format!(
                "{} {}",
                about.kind.as_str(),
                about.id
            )))
        }
    }

    /// Store a file's bytes and record it. The bytes go to the blob store
    /// first: an attachment record never points at missing bytes.
    pub fn add_attachment(&mut self, blobs: &mut dyn BlobStore, f: NewFile) -> Result<Attachment> {
        self.check_about(&f.about)?;
        if f.bytes.is_empty() {
            return Err(Error::Invalid("the file is empty".into()));
        }
        if f.bytes.len() > MAX_DOCUMENT_BYTES {
            return Err(Error::Invalid(format!(
                "that file is {} MB; the most Housewright keeps is {} MB",
                f.bytes.len() / (1024 * 1024),
                MAX_DOCUMENT_BYTES / (1024 * 1024)
            )));
        }
        let sha256 = blobs.put(f.bytes)?;
        let thumb_sha256 = match f.thumb {
            Some(t) => Some(blobs.put(t)?),
            None => None,
        };
        let name = f.name.trim();
        let a = Attachment {
            id: AttachmentId::new(),
            sha256,
            name: if name.is_empty() {
                "file".into()
            } else {
                name.chars().take(120).collect()
            },
            media_type: f.media_type,
            size: f.bytes.len() as u64,
            kind: f.kind,
            about: f.about,
            sensitivity: f.sensitivity,
            thumb_sha256,
            width: f.width,
            height: f.height,
            by: Some(self.author()?),
            at: Some(self.now()),
            source: f.source,
        };
        a.validate().map_err(Error::Invalid)?;
        self.append(&a)?;
        Ok(a)
    }

    /// Live attachments about one thing, oldest first.
    pub fn attachments_about(&self, about: &EntityRef) -> Result<Vec<Attachment>> {
        let mut v: Vec<Attachment> = self
            .list::<Attachment>()?
            .into_iter()
            .filter(|a| a.about == *about)
            .collect();
        v.sort_by_key(|a| (a.at, a.id));
        Ok(v)
    }

    /// Delete an attachment (a tombstone in the log), then free the bytes no
    /// live attachment uses any more. Returns how many blobs were freed.
    pub fn delete_attachment(
        &mut self,
        blobs: &mut dyn BlobStore,
        id: AttachmentId,
    ) -> Result<usize> {
        let a: Attachment = self.fetch(&id.to_string())?;
        // A photo field pointing at it is cleared too.
        if a.about.kind == EntityKind::Item
            && let Some(mut item) = self.get::<Item>(&a.about.id)?
        {
            let before = item.custom.len();
            item.custom
                .retain(|_, v| v.value != FieldValue::Photo(a.id));
            if item.custom.len() != before {
                self.put(&item)?;
            }
        }
        self.delete(EntityKind::Attachment, &id.to_string())?;
        blobs::collect_garbage(self, blobs)
    }

    /// The household's own fields on a template (D-22).
    pub fn template_fields(&self, template: &str) -> Result<Vec<FieldDef>> {
        Ok(self
            .get::<TemplateFields>(template)?
            .map(|t| t.fields)
            .unwrap_or_default())
    }

    /// Every field definition an item has: its template's, the household's
    /// additions to that template, then the item's own.
    pub fn field_defs(&self, item: &Item) -> Result<Vec<FieldDef>> {
        let mut out: Vec<FieldDef> = Vec::new();
        if let Some(t) = &item.template {
            if let Some(tpl) = self.get::<Template>(t)? {
                out.extend(tpl.fields);
            }
            out.extend(self.template_fields(t)?);
        }
        out.extend(item.extra_fields.iter().cloned());
        let mut seen = BTreeSet::new();
        out.retain(|f| seen.insert(f.key.clone()));
        Ok(out)
    }

    /// Add a field to one item, or to every item made from its template.
    /// The key must be new for that item.
    pub fn add_field(&mut self, item: ItemId, def: FieldDef, to_template: bool) -> Result<()> {
        def.validate().map_err(Error::Invalid)?;
        let mut it: Item = self.fetch(&item.to_string())?;
        if self.field_defs(&it)?.iter().any(|f| f.key == def.key) {
            return Err(Error::Invalid(format!(
                "this thing already has a field called '{}'",
                def.label
            )));
        }
        match (&it.template, to_template) {
            (Some(t), true) => {
                let mut tf = self.get::<TemplateFields>(t)?.unwrap_or(TemplateFields {
                    id: t.clone(),
                    fields: vec![],
                });
                tf.fields.push(def);
                self.put(&tf)?;
            }
            (None, true) => {
                return Err(Error::Invalid(
                    "this thing was not made from a template; add the field to it alone".into(),
                ));
            }
            (_, false) => {
                it.extra_fields.push(def);
                self.put(&it)?;
            }
        }
        Ok(())
    }

    /// Remove a field the person added to this one item, and its value.
    pub fn remove_item_field(&mut self, item: ItemId, key: &str) -> Result<()> {
        let mut it: Item = self.fetch(&item.to_string())?;
        let before = it.extra_fields.len();
        it.extra_fields.retain(|f| f.key != key);
        if it.extra_fields.len() == before {
            return Err(Error::NotFound(format!(
                "a field '{key}' added to this thing"
            )));
        }
        it.custom.remove(key);
        self.put(&it)?;
        Ok(())
    }

    /// Set (or clear, with `None`) custom values on an item. Each value's
    /// sensitivity comes from its field definition unless given.
    pub fn set_item_values(
        &mut self,
        item: ItemId,
        values: Vec<(String, Option<FieldValue>, Option<Sensitivity>)>,
    ) -> Result<Item> {
        let mut it: Item = self.fetch(&item.to_string())?;
        let defs = self.field_defs(&it)?;
        for (key, value, sens) in values {
            match value {
                None => {
                    it.custom.remove(&key);
                }
                Some(v) => {
                    let def = defs
                        .iter()
                        .find(|d| d.key == key)
                        .ok_or_else(|| Error::Invalid(format!("no field '{key}' on this thing")))?;
                    def.check(&v).map_err(Error::Invalid)?;
                    if let FieldValue::Photo(a) = &v {
                        let att: Attachment = self.fetch(&a.to_string())?;
                        if att.about.id != item.to_string() {
                            return Err(Error::Invalid(
                                "that photo belongs to something else".into(),
                            ));
                        }
                    }
                    it.custom.insert(
                        key,
                        CustomValue {
                            value: v,
                            sensitivity: sens.unwrap_or(def.sensitivity),
                        },
                    );
                }
            }
        }
        self.put(&it)?;
        Ok(it)
    }

    /// After an item's fields change, compare its template's conditional
    /// tasks with what it has. Tasks that now apply and are missing are
    /// added switched off, as suggestions to offer (never switched on by
    /// themselves). Returns them, plus switched-on tasks whose condition no
    /// longer holds, so the person can decide.
    pub fn reconcile_conditional_tasks(
        &mut self,
        item: ItemId,
        owner: MemberId,
        start: Date,
    ) -> Result<(Vec<Task>, Vec<Task>)> {
        let it: Item = self.fetch(&item.to_string())?;
        let Some(tid) = &it.template else {
            return Ok((vec![], vec![]));
        };
        let Some(tpl) = self.get::<Template>(tid)? else {
            return Ok((vec![], vec![]));
        };
        let existing: Vec<Task> = self.list_by("item", &item.to_string())?;
        let has = |key: &str| {
            existing.iter().any(|t| {
                t.template
                    .as_ref()
                    .is_some_and(|r| r.template == *tid && r.task == key)
            })
        };
        let mut offered = Vec::new();
        let mut stale = Vec::new();
        for tt in &tpl.tasks {
            let Some(cond) = &tt.when else {
                continue;
            };
            let holds = cond.holds(&it);
            if holds && !has(&tt.key) {
                let t = tt.to_task(tid, it.property, Some(it.id), owner, start);
                self.put(&t)?;
                offered.push(t);
            } else if !holds {
                stale.extend(
                    existing
                        .iter()
                        .filter(|t| {
                            t.active
                                && !t.not_applicable
                                && t.template
                                    .as_ref()
                                    .is_some_and(|r| r.template == *tid && r.task == tt.key)
                        })
                        .cloned(),
                );
            }
        }
        Ok((offered, stale))
    }

    /// Append details to a completion after Done (P1).
    pub fn add_completion_detail(
        &mut self,
        completion: CompletionId,
        costs: Option<Vec<CostLine>>,
        values: CustomValues,
    ) -> Result<CompletionDetail> {
        let c: Completion = self.fetch(&completion.to_string())?;
        if c.voids.is_some() {
            return Err(Error::Invalid("that record cancels another".into()));
        }
        let task: Task = self.fetch(&c.task.to_string())?;
        for (k, v) in &values {
            let def = task
                .completion_fields
                .iter()
                .find(|d| &d.key == k)
                .ok_or_else(|| Error::Invalid(format!("no field '{k}' on this task")))?;
            def.check(&v.value).map_err(Error::Invalid)?;
        }
        if let Some(lines) = &costs {
            for l in lines {
                if l.amount.minor < 0 {
                    return Err(Error::Invalid("a cost cannot be negative".into()));
                }
            }
        }
        let d = CompletionDetail {
            id: CompletionDetailId::new(),
            completion,
            by: self.author()?,
            at: self.now(),
            costs,
            values,
            contractor: None,
            contractor_cleared: false,
        };
        self.append(&d)?;
        Ok(d)
    }

    /// Say who did a completed job after Done (P1, D-28): a contractor, or
    /// `None` for "not a contractor after all". Append-only, like every
    /// detail; the latest one wins.
    pub fn link_contractor(
        &mut self,
        completion: CompletionId,
        contractor: Option<crate::ids::ContractorId>,
    ) -> Result<CompletionDetail> {
        let c: Completion = self.fetch(&completion.to_string())?;
        if c.voids.is_some() {
            return Err(Error::Invalid("that record cancels another".into()));
        }
        if let Some(k) = contractor {
            let _: crate::model::Contractor = self.fetch(&k.to_string())?;
        }
        let d = CompletionDetail {
            id: CompletionDetailId::new(),
            completion,
            by: self.author()?,
            at: self.now(),
            costs: None,
            values: Default::default(),
            contractor,
            contractor_cleared: contractor.is_none(),
        };
        self.append(&d)?;
        Ok(d)
    }

    pub fn completion_details(&self, completion: CompletionId) -> Result<Vec<CompletionDetail>> {
        let mut v: Vec<CompletionDetail> = self.list_by("completion", &completion.to_string())?;
        v.sort_by(|a, b| a.at.cmp(&b.at).then(a.id.cmp(&b.id)));
        Ok(v)
    }
}

/// The values that count for a completion: its own, overlaid by details in
/// order (a later detail's value for a key wins).
pub fn effective_values(c: &Completion, details: &[CompletionDetail]) -> CustomValues {
    let mut out = c.values.clone();
    for d in details.iter().filter(|d| d.completion == c.id) {
        for (k, v) in &d.values {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}
