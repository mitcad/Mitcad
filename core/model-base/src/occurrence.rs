// SPDX-License-Identifier: MIT
//! Occurrence paths and links to geometry of other components (moved down
//! from the model's `features::joint` and `links`, mitcad#780, so that a
//! sketch below the model's core can keep its links): the plain data. What
//! needs the assembly (resolving a link's components and its transform)
//! stays in the model's `links.rs`.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ids::OccurrenceUid;

/// Occurrences from a component down, `"O1/O4"` in files and commands;
/// empty (`""`) for the component itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OccurrencePath(pub Vec<OccurrenceUid>);

impl OccurrencePath {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for OccurrencePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self.0.iter().map(ToString::to_string).collect();
        f.write_str(&parts.join("/"))
    }
}

impl Serialize for OccurrencePath {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for OccurrencePath {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        text.split('/')
            .map(|part| part.trim().parse::<OccurrenceUid>())
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
            .map_err(|_| {
                serde::de::Error::custom(format!(
                    "invalid occurrence path “{text}”: occurrence ids like \"O1/O4\""
                ))
            })
    }
}

/// Where linked geometry and the feature that uses it are placed: two
/// occurrence paths from the root (empty for the root itself).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OccurrenceLink {
    /// To the component the geometry is in.
    #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
    pub source: OccurrencePath,
    /// To the component of the feature that uses it.
    #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
    pub target: OccurrencePath,
}
