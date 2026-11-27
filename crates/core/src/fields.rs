//! Custom fields (D-22): definitions on templates and items, and their values.

use std::collections::BTreeMap;

use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use crate::ids::AttachmentId;
use crate::model::Sensitivity;

/// The type of a custom field. D-22 lists exactly these six.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FieldType {
    Text,
    /// A number, optionally with a unit ("in", "gal", "psi").
    Number {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
    },
    Date,
    YesNo,
    /// One of a fixed list of options.
    Choice {
        options: Vec<String>,
    },
    /// A photo, stored as an attachment.
    Photo,
}

/// A field definition, on a template, an item, or a task's completion form.
///
/// Fields are never required to complete a task (P1); there is deliberately no
/// `required` flag.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldDef {
    /// Stable machine key, e.g. `filter_size`.
    pub key: String,
    /// What people see, e.g. "Filter size".
    pub label: String,
    #[serde(flatten)]
    pub field_type: FieldType,
    /// Whether values of this field go in the sealed volume (D-41).
    #[serde(default)]
    pub sensitivity: Sensitivity,
    /// Optional help text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

/// A value for a custom field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum FieldValue {
    Text(String),
    Number {
        value: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
    },
    Date(Date),
    YesNo(bool),
    Choice(String),
    Photo(AttachmentId),
}

/// A stored custom value with its own sensitivity flag. The flag defaults to the
/// field definition's, but a person can mark a single value sensitive.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CustomValue {
    pub value: FieldValue,
    #[serde(default)]
    pub sensitivity: Sensitivity,
}

/// Custom values keyed by field key.
pub type CustomValues = BTreeMap<String, CustomValue>;

impl FieldDef {
    /// Check that a value fits this definition.
    pub fn check(&self, value: &FieldValue) -> Result<(), String> {
        let ok = match (&self.field_type, value) {
            (FieldType::Text, FieldValue::Text(_)) => true,
            (FieldType::Number { .. }, FieldValue::Number { value, .. }) => value.is_finite(),
            (FieldType::Date, FieldValue::Date(_)) => true,
            (FieldType::YesNo, FieldValue::YesNo(_)) => true,
            (FieldType::Choice { options }, FieldValue::Choice(c)) => {
                if !options.contains(c) {
                    return Err(format!(
                        "field '{}': '{}' is not one of {:?}",
                        self.key, c, options
                    ));
                }
                true
            }
            (FieldType::Photo, FieldValue::Photo(_)) => true,
            _ => false,
        };
        if ok {
            Ok(())
        } else {
            Err(format!(
                "field '{}': value {:?} does not match type {:?}",
                self.key, value, self.field_type
            ))
        }
    }

    /// Check the definition itself.
    pub fn validate(&self) -> Result<(), String> {
        if !is_key(&self.key) {
            return Err(format!(
                "field key '{}' must be lowercase letters, digits and underscores",
                self.key
            ));
        }
        if self.label.trim().is_empty() {
            return Err(format!("field '{}' has an empty label", self.key));
        }
        if let FieldType::Choice { options } = &self.field_type {
            if options.is_empty() {
                return Err(format!("choice field '{}' has no options", self.key));
            }
            let mut seen = std::collections::BTreeSet::new();
            for o in options {
                if !seen.insert(o) {
                    return Err(format!("choice field '{}' repeats '{}'", self.key, o));
                }
            }
        }
        Ok(())
    }
}

/// A machine key: `[a-z0-9_]+`, starting with a letter.
pub fn is_key(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice() -> FieldDef {
        FieldDef {
            key: "fuel".into(),
            label: "Fuel".into(),
            field_type: FieldType::Choice {
                options: vec!["gas".into(), "electric".into()],
            },
            sensitivity: Sensitivity::Normal,
            help: None,
        }
    }

    #[test]
    fn choice_checks_options() {
        let f = choice();
        assert!(f.check(&FieldValue::Choice("gas".into())).is_ok());
        assert!(f.check(&FieldValue::Choice("oil".into())).is_err());
        assert!(f.check(&FieldValue::Text("gas".into())).is_err());
    }

    #[test]
    fn number_rejects_nan() {
        let f = FieldDef {
            key: "psi".into(),
            label: "Pressure".into(),
            field_type: FieldType::Number {
                unit: Some("psi".into()),
            },
            sensitivity: Sensitivity::Normal,
            help: None,
        };
        assert!(
            f.check(&FieldValue::Number {
                value: 40.0,
                unit: None
            })
            .is_ok()
        );
        assert!(
            f.check(&FieldValue::Number {
                value: f64::NAN,
                unit: None
            })
            .is_err()
        );
    }

    #[test]
    fn keys() {
        assert!(is_key("filter_size"));
        assert!(!is_key("Filter"));
        assert!(!is_key("1abc"));
        assert!(!is_key(""));
    }

    #[test]
    fn all_six_types_round_trip_toml_shape() {
        let defs = vec![
            FieldType::Text,
            FieldType::Number {
                unit: Some("in".into()),
            },
            FieldType::Date,
            FieldType::YesNo,
            FieldType::Choice {
                options: vec!["a".into()],
            },
            FieldType::Photo,
        ];
        for t in defs {
            let d = FieldDef {
                key: "k".into(),
                label: "K".into(),
                field_type: t,
                sensitivity: Sensitivity::Sensitive,
                help: None,
            };
            let j = serde_json::to_string(&d).unwrap();
            let back: FieldDef = serde_json::from_str(&j).unwrap();
            assert_eq!(back, d);
        }
    }
}
