// SPDX-License-Identifier: MIT
//! What BUILD reads of a placed part from the stored design: its record,
//! derived part, source, metadata, mount frames and length at the timeline
//! marker, and the designation, supplier and part number a list gives it.
//! The parts list and the properties read it here, so it stays in the
//! model; BUILD's planning (`mitcad-model-build`, mitcad#777) re-exports it.

use std::collections::BTreeMap;

use super::{BuildMeta, BuildRecord, LibrarySourceDef};
use crate::document::Document;
use crate::features::FeatureDef;
use crate::features::derived_part::{DerivedPartDef, Ends};
use crate::ids::{ComponentUid, FeatureUid};
use crate::kernel::Kernel;
use crate::parameters::{ParamId, Parameters};
use crate::transform::Transform;

/// A BUILD part as the rules see it.
#[derive(Debug, Clone)]
pub struct PartInfo {
    pub component: ComponentUid,
    pub name: String,
    pub record: BuildRecord,
    pub part: Option<FeatureUid>,
    pub def: Option<DerivedPartDef>,
    pub source: Option<LibrarySourceDef>,
    pub meta: BuildMeta,
    /// The mount frames at the marker, in the part's coordinates.
    pub mounts: BTreeMap<String, Transform>,
    /// The design's parameter that sets its length, and its value.
    pub length: Option<(ParamId, f64)>,
}

impl PartInfo {
    pub fn mount(&self, name: &str) -> Option<&Transform> {
        self.mounts.get(name)
    }

    /// The slot frames, by name.
    pub fn slots(&self) -> Vec<(&String, &Transform)> {
        self.mounts
            .iter()
            .filter(|(name, _)| self.frame_kind(name) == "slot")
            .collect()
    }

    /// The frames a part goes on a side at (mitcad#478): the slot frames,
    /// and a member's `face` frames on the sides that have no slot frame.
    pub fn sides(&self) -> Vec<(&String, &Transform)> {
        let mut out = self.slots();
        if self.meta.role != super::BuildRole::Profile {
            return out;
        }
        let faces: Vec<(&String, &Transform)> = self
            .mounts
            .iter()
            .filter(|(name, _)| self.frame_kind(name) == "face")
            .collect();
        for (name, t) in faces {
            let z = super::z_axis(t);
            if !out
                .iter()
                .any(|(_, s)| super::dot(super::z_axis(s), z) > 1.0 - 1e-6)
            {
                out.push((name, t));
            }
        }
        out
    }

    /// Whether a frame is a side frame (a slot, or a member's face).
    pub fn is_side(&self, name: &str) -> bool {
        self.sides().iter().any(|(n, _)| *n == name)
    }

    /// Whether the part is a timber member (mitcad#478).
    pub fn is_timber(&self) -> bool {
        self.meta.is_timber()
    }

    /// A frame's kind: the metadata's, else guessed from its name
    /// (`end_a`, `slot_x+_1`).
    pub fn frame_kind(&self, name: &str) -> String {
        if let Some(frame) = self.meta.frame(name).filter(|f| !f.kind.is_empty()) {
            return frame.kind.clone();
        }
        if name == "end_a" || name == "end_b" {
            "end".to_owned()
        } else if name.starts_with("slot") {
            "slot".to_owned()
        } else {
            String::new()
        }
    }

    pub fn is_cut_to_length(&self) -> bool {
        self.meta.length.is_some()
    }

    /// The configuration row it is made in.
    pub fn config(&self) -> Option<String> {
        self.def.as_ref().and_then(|d| d.config.clone())
    }

    /// The derived part's end cuts.
    pub fn ends(&self) -> Option<&Ends> {
        self.def.as_ref().map(|d| &d.ends)
    }
}

/// The designation of a part made from a source: the own designation of
/// the row of its combination (the row `config` with the part's
/// parameters applied, mitcad#798), else the library's pattern with
/// `{standard}`, `{name}`, `{config}` (empty when the combination is no
/// row), the options' values and the part's parameters (`{L}`, in
/// millimetres) replaced; without a pattern the name and the row.
pub fn designation(
    source: &LibrarySourceDef,
    config: Option<&str>,
    values: &BTreeMap<String, f64>,
) -> String {
    let state = source.design.state().ok();
    let named = state.as_ref().and_then(|s| {
        let table = &s.configurations;
        match config {
            Some(name) => table.row(name).map(|r| r.name.clone()),
            None => table.default_row().map(|r| r.name.clone()),
        }
    });
    // The design in the part's combination: its row and parameters.
    let list: Vec<(String, f64)> = values.iter().map(|(k, v)| (k.clone(), *v)).collect();
    let configured = state.as_ref().and_then(|_| {
        crate::features::derived_part::configured(source, named.as_deref(), &list).ok()
    });
    let row = configured.as_ref().and_then(|s| {
        s.configurations
            .combination_row(&s.parameters, named.as_deref())
            .cloned()
    });
    let options = configured
        .as_ref()
        .map(|s| s.configurations.designation_values(&s.parameters))
        .unwrap_or_default();
    if let Some(own) = row.as_ref().and_then(|r| r.designation.clone()) {
        return own;
    }
    let pattern = source.library.designation.trim();
    let pattern = if pattern.is_empty() {
        "{name} {config}"
    } else {
        pattern
    };
    let mut text = pattern
        .replace("{standard}", &source.standard)
        .replace("{name}", &source.label())
        .replace("{config}", row.as_ref().map_or("", |r| r.name.as_str()));
    for (option, value) in &options {
        text = text.replace(&format!("{{{option}}}"), value);
    }
    for (name, value) in values {
        text = text.replace(&format!("{{{name}}}"), &super::round(*value));
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A part's section of a typed size in its combination (mitcad#1328: a
/// steel tube of any size within its options' limits).
#[derive(Debug, Clone, PartialEq)]
pub struct TypedSection {
    /// The value of the source's area parameter, mm²; None when it has no
    /// such parameter or its value is not above 0.
    pub area: Option<f64>,
    /// The options' values in the table's order, the length's left out
    /// (`B 76.2, T 4.8`).
    pub size: String,
}

/// The typed section of a part made in the row `config` (the default row
/// when None) with its `values` applied: the source's parameter `area`
/// and its options but the parameter `length`; None when the source
/// cannot be configured so.
pub fn typed_section(
    source: &LibrarySourceDef,
    config: Option<&str>,
    values: &BTreeMap<String, f64>,
    area: &str,
    length: Option<&str>,
) -> Option<TypedSection> {
    let state = source.design.state().ok()?;
    let table = &state.configurations;
    let named = match config {
        Some(row) => table.row(row).map(|r| r.name.clone()),
        None => table.default_row().map(|r| r.name.clone()),
    };
    let list: Vec<(String, f64)> = values.iter().map(|(k, v)| (k.clone(), *v)).collect();
    let configured =
        crate::features::derived_part::configured(source, named.as_deref(), &list).ok()?;
    let parameters = &configured.parameters;
    let texts = configured.configurations.designation_values(parameters);
    let size = configured
        .configurations
        .options
        .iter()
        .filter(|option| Some(option.as_str()) != length)
        .filter_map(|option| Some(format!("{option} {}", texts.get(option)?)))
        .collect::<Vec<_>>()
        .join(", ");
    let area = parameters
        .find(area)
        .and_then(|id| parameters.value(id))
        .filter(|a| a.is_finite() && *a > 0.0);
    Some(TypedSection { area, size })
}

/// The part number of a part made in a row: the row's own, else the
/// source's.
pub fn part_number(source: &LibrarySourceDef, config: Option<&str>) -> String {
    let own = source.design.state().ok().and_then(|s| {
        let table = &s.configurations;
        let row = match config {
            Some(name) => table.row(name),
            None => table.default_row(),
        };
        row.and_then(|r| r.part_number.clone())
    });
    own.unwrap_or_else(|| source.part_number.clone())
}

/// A derived part's parameter values in millimetres (radians), by name.
pub fn part_values(def: &DerivedPartDef, params: &Parameters) -> BTreeMap<String, f64> {
    def.parameters
        .iter()
        .filter_map(|(name, id)| Some((name.clone(), params.value(*id)?)))
        .collect()
}

/// What a list says of a BUILD part: its source's library record, its
/// designation and its article number.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ListEntry {
    pub library: crate::library::LibraryRef,
    pub designation: String,
    pub part_number: String,
}

impl<K: Kernel> Document<K> {
    /// The BUILD part of a component, if it is one.
    pub fn part_info(&self, component: ComponentUid) -> Option<PartInfo> {
        let state = self.state();
        let def = state.assembly.component(component)?;
        let record = def.build.clone()?;
        let part_def = record.part.and_then(|uid| match &state.entry(uid)?.def {
            FeatureDef::DerivedPart(d) => Some(d.clone()),
            _ => None,
        });
        let source = record
            .source
            .and_then(|id| state.library_sources.get(id).cloned());
        let meta = source
            .as_ref()
            .and_then(|s| s.build.as_ref())
            .and_then(|b| {
                let row = part_def.as_ref().and_then(|d| d.config.as_deref());
                BuildMeta::read_row(b, row).ok()
            })
            .unwrap_or_else(|| BuildMeta {
                role: record.role,
                ..BuildMeta::default()
            });
        let mounts = record
            .part
            .and_then(|uid| self.last_result().result(uid))
            .filter(|r| r.status.is_ok())
            .and_then(|r| r.output.as_ref())
            .and_then(|o| o.mounts.as_ref())
            .map(|m| (**m).clone())
            .unwrap_or_default();
        let length = meta.length.as_ref().and_then(|l| {
            let id = *part_def.as_ref()?.parameters.get(&l.parameter)?;
            Some((id, self.parameters().value(id)?))
        });
        let part = record.part.filter(|_| part_def.is_some());
        Some(PartInfo {
            component,
            name: def.name.clone(),
            record,
            part,
            def: part_def,
            source,
            meta,
            mounts,
            length,
        })
    }

    /// A BUILD part's entry in the parts list, if the component is one made
    /// from a source.
    pub(crate) fn build_list_entry(&self, component: ComponentUid) -> Option<ListEntry> {
        let info = self.part_info(component)?;
        let source = info.source.as_ref()?;
        let def = info.def.as_ref()?;
        let config = def.config.as_deref();
        let values = part_values(def, self.parameters());
        let mut library = source.library.clone();
        library.config = def.config.clone();
        let designation = designation(source, config, &values);
        library.designation = designation.clone();
        let part_number = part_number(source, config);
        library.part_number = part_number.clone();
        Some(ListEntry {
            library,
            designation,
            part_number,
        })
    }
}
