// SPDX-License-Identifier: MIT
//! The BUILD tab's assemblies of slot profiles (mitcad#375; `commands.md`,
//! "BUILD: profile assemblies"): what the design file keeps of them. The
//! library parts a design keeps once as its **sources**, what a library's
//! `build` block says about a part ([`BuildMeta`], `docs/libraries.md`,
//! "BUILD parts"), what BUILD records on the components it makes
//! ([`BuildRecord`]), what BUILD reads of a placed part (`part_info.rs`:
//! the parts list and the properties use it), and the vector helpers the
//! features share. The commands' planning (placing, editing, connecting,
//! panels, removing, the junctions and the cut list) is the crate
//! `mitcad-model-build` above the model (mitcad#777).
//!
//! Everything BUILD makes is ordinary design content: components,
//! occurrences, `derived_part` and `bound_length` features
//! (`features/derived_part.rs`, `features/bound_length.rs`), joints and
//! parameters on the one timeline. The records only say what BUILD made,
//! so that it can edit, connect and remove it again.

// A part centred in a free stretch whose neighbour goes (mitcad#1401).
mod gap;
pub(crate) mod geometry;
mod meta;
// What BUILD reads of a placed part; the parts list and the properties
// use it (mitcad#777).
pub(crate) mod part_info;
pub(crate) mod sources;

pub(crate) use geometry::*;
pub use meta::*;
pub use sources::{LibrarySourceDef, LibrarySources, SourceDesign};

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::document::{Document, ModelError};
use crate::expr::Unit;
use crate::features::ValueInput;
use crate::ids::{ComponentUid, FeatureUid, IdError};
use crate::kernel::Kernel;
use mitcad_model_base::internal::ids::{parse_number, string_serde};

/// A library source of the design, written `S<n>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceId(pub u32);

impl fmt::Display for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "S{}", self.0)
    }
}

impl FromStr for SourceId {
    type Err = IdError;

    fn from_str(text: &str) -> Result<Self, IdError> {
        parse_number(text, "S")
            .map(SourceId)
            .ok_or_else(|| IdError::new("library source", text, "S<number>"))
    }
}

string_serde!(SourceId, "a library source like \"S1\"");

/// Something a connector added: a component (its nuts and screws) or a
/// feature (a hole it machined into a profile), `"C5"` or `"F12"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BuildItem {
    Component(ComponentUid),
    Feature(FeatureUid),
}

impl fmt::Display for BuildItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Component(uid) => uid.fmt(f),
            Self::Feature(uid) => uid.fmt(f),
        }
    }
}

impl FromStr for BuildItem {
    type Err = IdError;

    fn from_str(text: &str) -> Result<Self, IdError> {
        if let Ok(uid) = text.parse::<ComponentUid>() {
            return Ok(Self::Component(uid));
        }
        text.parse::<FeatureUid>()
            .map(Self::Feature)
            .map_err(|_| IdError::new("BUILD item", text, "C<number> or F<number>"))
    }
}

string_serde!(BuildItem, "a component or a feature like \"C5\" or \"F12\"");

/// What BUILD made a component of (the component's `build` in the design
/// file): its role, the source and the derived part its body is made from,
/// the joint that places it, the bound lengths that size it, the connector
/// that added it (a nut, a screw) and what a connector added. A component
/// without one is no BUILD part; other commands treat it as any other.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildRecord {
    pub role: BuildRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceId>,
    /// The `derived_part` feature (a fastener from a library without
    /// `build` has a linked base feature instead, and none here).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<FeatureUid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub joint: Option<FeatureUid>,
    #[serde(default)]
    pub bound: Vec<FeatureUid>,
    #[serde(default)]
    pub added_by: Option<ComponentUid>,
    #[serde(default)]
    pub adds: Vec<BuildItem>,
    /// A connector's junction (`build_junctions`' id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub junction: Option<String>,
    /// What a part on a side or a rail is placed from (mitcad#417): its
    /// joint's slide is bound to that end or the middle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<SideFrom>,
    /// A profile on a side tilted along its host (mitcad#444), radians,
    /// positive towards the host's end B.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tilt: Option<f64>,
    // Timber construction (mitcad#478).
    /// The part's own grade (a timber member's), else its row's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grade: Option<String>,
    /// A member of a spacing set: the set's lead (its first member).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set: Option<ComponentUid>,
    /// The lead's: the set's definition (`spacing.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing: Option<serde_json::Value>,
    /// A member by two points: the placement it was made with (`build_place`'s
    /// fields), to place it anew.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points: Option<serde_json::Value>,
    // Timber frames (mitcad#479).
    /// A member of a timber frame: the frame's lead (its first member).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<ComponentUid>,
    /// The lead's: the frame's definition (`frames.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framing: Option<serde_json::Value>,
    /// The member's place in its frame (`stud.3`, `plate.top.0`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member: Option<String>,
    // Corner connectors (mitcad#1115).
    /// The rule a slot connector or its fastener was placed by: 2 since
    /// mitcad#1115 (inside the frame at every corner, a nut into the slot,
    /// a screw's head on the connector's wall); without it, the first rule,
    /// which `build_upgrade` replaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<u32>,
    // The middle of a free stretch (mitcad#1401).
    /// A part on a side placed from the middle of the free stretch it
    /// stands in (`from` `gap`): the profiles on the same slot it is
    /// centred between, towards end A and towards end B; None for the
    /// side's end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<[Option<ComponentUid>; 2]>,
}

/// What a placement's offset along a profile is measured from
/// (mitcad#417): end A, end B or the middle; on a side also the middle of
/// the free stretch between the profiles standing on it (mitcad#1401).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SideFrom {
    #[default]
    #[serde(rename = "a")]
    A,
    #[serde(rename = "b")]
    B,
    #[serde(rename = "middle")]
    Middle,
    #[serde(rename = "gap")]
    Gap,
}

impl SideFrom {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::A => "a",
            Self::B => "b",
            Self::Middle => "middle",
            Self::Gap => "gap",
        }
    }
}

/// The value of a command's value in the document: a number as it is
/// (millimetres, radians), a text evaluated with the parameters.
#[doc(hidden)]
pub fn value_in<K: Kernel>(
    doc: &Document<K>,
    value: &ValueInput,
    unit: Unit,
) -> Result<f64, ModelError> {
    match value {
        ValueInput::Number(n) if n.is_finite() => Ok(*n),
        ValueInput::Number(n) => Err(ModelError::Invalid(format!("{n} is not a finite number"))),
        ValueInput::Name(text) => {
            let expression = crate::document::split_named(text).map_or(text.as_str(), |(_, e)| e);
            let params = doc.parameters();
            crate::expr::evaluate(expression, params.table(), &params.context(), unit)
                .map(|q| q.value)
                .map_err(|e| ModelError::Invalid(format!("“{text}”: {e}")))
        }
    }
}

/// What a BUILD part is (`docs/libraries.md`, "BUILD parts").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildRole {
    #[default]
    Profile,
    Connector,
    Fastener,
    Panel,
    Linear,
    Cap,
}

impl BuildRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Profile => "profile",
            Self::Connector => "connector",
            Self::Fastener => "fastener",
            Self::Panel => "panel",
            Self::Linear => "linear",
            Self::Cap => "cap",
        }
    }
}

impl fmt::Display for BuildRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl crate::document::DocState {
    /// The library sources the derived parts use.
    fn used_library_sources(&self) -> std::collections::BTreeSet<SourceId> {
        self.features
            .iter()
            .filter_map(|f| match &f.def {
                crate::features::FeatureDef::DerivedPart(part) => Some(part.source),
                _ => None,
            })
            .collect()
    }

    /// Removes the library sources whose last derived part this edit removed:
    /// a source goes with its last use, and one no part has used yet (added by
    /// `update_library_parts` or a file) stays.
    pub(crate) fn prune_library_sources(&mut self, before: &Self) {
        if self.library_sources.is_empty() {
            return;
        }
        let used_before = before.used_library_sources();
        let used_now = self.used_library_sources();
        let lost: std::collections::BTreeSet<SourceId> =
            used_before.difference(&used_now).copied().collect();
        if lost.is_empty() {
            return;
        }
        self.library_sources.remove_all(&lost);
    }
}
