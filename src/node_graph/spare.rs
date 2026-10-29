//! Spare parameters: values an operator holds that are none of its own (a
//! number, text, a switch, a choice), added from its Parameters with +, for
//! its other parameters, or other operators', to follow (references) and to
//! promote. They are found by the path `spare/<name>`, a part of one as its
//! parameters' are (`spare/offset/1`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::compounds::{self, Parameter, ParameterType};

/// The start of a spare parameter's path.
pub const PREFIX: &str = "spare/";

/// The kinds a spare parameter can be, in the order the + window offers them,
/// each with its name there.
pub const KINDS: &[(ParameterType, &str)] = &[
    (ParameterType::Float, "Float"),
    (ParameterType::Integer, "Integer"),
    (ParameterType::Boolean, "Toggle"),
    (ParameterType::String, "Text"),
    (ParameterType::Vector2, "2D Vector"),
    (ParameterType::Vector3, "3D Vector"),
    (ParameterType::Vector4, "4D Vector"),
    (ParameterType::Colour, "Colour"),
    (ParameterType::Choice, "Choice"),
    (ParameterType::List, "Text List"),
];

/// An operator's spare parameters, and their values where not the default.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Spare {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<Parameter>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, serde_json::Value>,
}

impl Spare {
    pub fn is_empty(&self) -> bool {
        self.parameters.is_empty()
    }

    /// Each value (else the default) by name, under one key as an operator's
    /// parameters are under its kind, so paths read as theirs do.
    pub fn json(&self) -> serde_json::Value {
        let all: serde_json::Map<String, serde_json::Value> = self
            .parameters
            .iter()
            .map(|p| (p.name.clone(), self.values.get(&p.name).unwrap_or(&p.default).clone()))
            .collect();
        serde_json::json!({ "spare": all })
    }

    /// The value at `path` (`spare/size`, `spare/offset/1`); None for a path
    /// that is not a spare parameter's.
    pub fn get(&self, path: &str) -> Option<serde_json::Value> {
        let rest = path.strip_prefix(PREFIX)?;
        compounds::get(&self.json(), rest).cloned()
    }

    /// Set the value at `path`. Whether it is a spare parameter's.
    pub fn put(&mut self, path: &str, x: serde_json::Value) -> bool {
        let Some(rest) = path.strip_prefix(PREFIX) else {
            return false;
        };
        let name = rest.split('/').next().unwrap_or_default();
        let Some(p) = self.parameters.iter().find(|p| p.name == name) else {
            return false;
        };
        let mut v = self.json();
        if !compounds::put(&mut v, rest, x) {
            return false;
        }
        let now = v["spare"][name].clone();
        if now == p.default {
            self.values.remove(name);
        } else {
            self.values.insert(name.to_string(), now);
        }
        true
    }

    /// Its numbers, each by its path: what a reference can follow.
    pub fn numbers(&self) -> Vec<(String, f64)> {
        crate::core::keys::keyable(&self.json())
            .into_iter()
            .map(|(p, x)| (format!("{PREFIX}{p}"), x))
            .collect()
    }

    /// A new spare parameter of `kind`, labelled `label`, in `group`: its
    /// name made of the label, one no other spare parameter has; its default
    /// the kind's zero (a colour white, a choice its first option). `options`
    /// a choice's, by label; `range` a number's.
    pub fn add(
        &mut self,
        kind: ParameterType,
        label: &str,
        group: &str,
        range: Option<[f64; 2]>,
        options: &[String],
    ) -> Result<String, String> {
        let label = label.trim();
        if label.is_empty() {
            return Err("Give it a name".into());
        }
        let base: String = label
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect();
        let name = (1..)
            .map(|k| if k == 1 { base.clone() } else { format!("{base}{k}") })
            .find(|n| !self.parameters.iter().any(|p| p.name == *n))
            .unwrap_or_default();
        let choices: Vec<(String, serde_json::Value)> = options
            .iter()
            .map(|o| o.trim())
            .filter(|o| !o.is_empty())
            .map(|o| (o.to_string(), serde_json::json!(o)))
            .collect();
        if kind == ParameterType::Choice && choices.is_empty() {
            return Err("A choice needs its options: one a row".into());
        }
        let default = match kind {
            ParameterType::Integer => serde_json::json!(0),
            ParameterType::Boolean => serde_json::json!(false),
            ParameterType::String => serde_json::json!(""),
            ParameterType::Vector2 => serde_json::json!([0.0, 0.0]),
            ParameterType::Vector3 => serde_json::json!([0.0, 0.0, 0.0]),
            ParameterType::Vector4 => serde_json::json!([0.0, 0.0, 0.0, 0.0]),
            ParameterType::Colour => serde_json::json!([1.0, 1.0, 1.0]),
            ParameterType::Choice => choices[0].1.clone(),
            ParameterType::List => serde_json::json!([]),
            ParameterType::Float | ParameterType::Ramp => serde_json::json!(0.0),
        };
        let numeric = matches!(kind, ParameterType::Float | ParameterType::Integer);
        self.parameters.push(Parameter {
            name: name.clone(),
            label: label.to_string(),
            kind,
            default,
            range: range.filter(|_| numeric),
            group: group.trim().to_string(),
            help: String::new(),
            targets: vec![],
            choices: if kind == ParameterType::Choice { choices } else { vec![] },
        });
        Ok(name)
    }

    /// Take away the spare parameter of `path`. Whether there was one.
    pub fn remove(&mut self, path: &str) -> bool {
        let Some(name) = path.strip_prefix(PREFIX).and_then(|r| r.split('/').next()) else {
            return false;
        };
        let had = self.parameters.len();
        self.parameters.retain(|p| p.name != name);
        self.values.remove(name);
        self.parameters.len() != had
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A spare parameter is read and set by its path, a part of one too; set
    /// back to its default it holds no value of its own.
    #[test]
    fn spare_parameters_are_read_and_set_by_path() {
        let mut s = Spare::default();
        assert_eq!(
            s.add(ParameterType::Vector3, "Offset", "Extra", None, &[]),
            Ok("offset".into())
        );
        assert_eq!(
            s.add(ParameterType::Float, "Offset", "", Some([0.0, 1.0]), &[]),
            Ok("offset2".into())
        );
        assert_eq!(s.get("spare/offset/1"), Some(serde_json::json!(0.0)));
        assert!(s.put("spare/offset/1", serde_json::json!(2.5)));
        assert_eq!(s.get("spare/offset"), Some(serde_json::json!([0.0, 2.5, 0.0])));
        assert!(s.numbers().contains(&("spare/offset/1".to_string(), 2.5)));
        assert!(s.put("spare/offset/1", serde_json::json!(0.0)));
        assert!(s.values.is_empty(), "the default again");
        assert!(!s.put("spare/nothing", serde_json::json!(1.0)), "no such parameter");
        assert!(!s.put("size", serde_json::json!(1.0)), "not a spare path");
        assert_eq!(s.get("size"), None);
        assert!(s.remove("spare/offset"));
        assert_eq!(s.parameters.len(), 1);
    }

    /// A choice takes its options, its first the default; without options it
    /// is refused, as is a parameter without a name.
    #[test]
    fn a_spare_choice_has_its_options() {
        let mut s = Spare::default();
        assert!(s.add(ParameterType::Choice, "Mode", "", None, &[]).is_err());
        assert!(s.add(ParameterType::Float, "  ", "", None, &[]).is_err());
        let opts = vec!["Fast".to_string(), " ".to_string(), "Fine".to_string()];
        s.add(ParameterType::Choice, "Mode", "", None, &opts).unwrap();
        assert_eq!(s.parameters[0].choices.len(), 2);
        assert_eq!(s.get("spare/mode"), Some(serde_json::json!("Fast")));
        assert_eq!(s.parameters[0].range, None);
    }
}
