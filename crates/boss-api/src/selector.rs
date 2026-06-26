use serde::{Deserialize, Serialize};

use crate::meta::Labels;

/// A label selector: match labels + match expressions
/// (In/NotIn/Exists/DoesNotExist).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelSelector {
    #[serde(
        default,
        rename = "matchLabels",
        skip_serializing_if = "Option::is_none"
    )]
    pub match_labels: Option<Labels>,
    #[serde(
        default,
        rename = "matchExpressions",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub match_expressions: Vec<LabelSelectorRequirement>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelSelectorRequirement {
    pub key: String,
    pub operator: String, // In | NotIn | Exists | DoesNotExist
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
}

impl LabelSelector {
    /// Returns true if `labels` satisfies this selector.
    pub fn matches(&self, labels: &Labels) -> bool {
        if let Some(ml) = &self.match_labels {
            for (k, v) in ml {
                if labels.get(k) != Some(v) {
                    return false;
                }
            }
        }
        for req in &self.match_expressions {
            let value = labels.get(&req.key);
            match req.operator.as_str() {
                "In" => match value {
                    Some(value) if req.values.iter().any(|expected| expected == value) => {}
                    _ => return false,
                },
                "NotIn" => {
                    if let Some(value) = value
                        && req.values.iter().any(|expected| expected == value)
                    {
                        return false;
                    }
                }
                "Exists" => {
                    if value.is_none() {
                        return false;
                    }
                }
                "DoesNotExist" => {
                    if value.is_some() {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        true
    }
}
