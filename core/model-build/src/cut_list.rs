// SPDX-License-Identifier: MIT
//! The cut list (`build_cut_list`): one row per group of identical cut
//! parts (the same source, row, length, end cuts, machining and own
//! features), the stock lengths they are cut from in their cutting order,
//! and the parts list's rows for everything else. Timber members
//! (mitcad#478) add their material, grade, sections, running metres and
//! volume, their joinery in the machining, and their stock fitted per
//! configuration row with each stock piece shortened to what holds it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::parts::{PartInfo, designation, part_number, part_values, typed_section};
use crate::prelude::*;
use crate::{BuildItem, MaterialMeta, SourceId};
use mitcad_model::Document;
use mitcad_model::features::FeatureDef;
use mitcad_model::features::derived_part::Tilt;
use mitcad_model::ids::{ComponentUid, FeatureUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::parameters::ParamId;

/// How the cut list is worked out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CutListOptions {
    /// The saw's cut, mm.
    pub kerf: f64,
}

impl Default for CutListOptions {
    fn default() -> Self {
        Self { kerf: 3.0 }
    }
}

/// A hole in a cut part: where, its diameter, depth and thread; or a
/// joinery cut (mitcad#478): its kind, size and the other member.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MachiningItem {
    /// The frame it is on (`end_b`, `slot_x+_1`, ...); empty for joinery.
    pub at: String,
    #[serde(with = "nan_null")]
    pub diameter: f64,
    pub depth: Option<f64>,
    pub thread: Option<String>,
    /// Added by a connector (else the part's own).
    pub connector: Option<ComponentUid>,
    /// Joinery: `housing`, `notch`, `half_lap`, `rip` or `birdsmouth`.
    pub kind: Option<String>,
    pub size: Option<[f64; 3]>,
    pub with: Option<ComponentUid>,
}

/// A timber row's own columns (mitcad#478).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimberColumns {
    pub material: MaterialMeta,
    /// The part's own grade, else its row's.
    pub grade: String,
    pub nominal: Option<[f64; 2]>,
    pub section: [f64; 2],
    pub availability: Option<String>,
    /// Over the row's quantity.
    pub running_metres: f64,
    /// m3, over the row's quantity.
    pub volume: f64,
    /// A profiled board's (mitcad#477): its profile, the width it covers
    /// (mm) and the area it covers (m², over the row's quantity).
    pub board: Option<BoardColumns>,
}

/// A profiled board's cut list columns (mitcad#477).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoardColumns {
    /// `tongue_and_groove`, `shiplap`, `feather_edge`, `deck`.
    pub profile: String,
    pub cover: f64,
    pub area: f64,
    /// The width each board of the row covers: the cover width, or less
    /// for a ripped board.
    pub each: f64,
}

/// A group of identical cut parts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutRow {
    pub source: SourceId,
    pub designation: String,
    pub supplier: String,
    pub part_number: String,
    pub config: Option<String>,
    pub length: f64,
    /// End A's and B's cut: angle (radians) and tilt.
    pub ends: [Option<(f64, Tilt)>; 2],
    pub machining: Vec<MachiningItem>,
    /// Its own features (holes, cuts) besides machining.
    pub features: usize,
    pub quantity: usize,
    /// Each, kg.
    pub mass: Option<f64>,
    pub components: Vec<ComponentUid>,
    /// A timber member's columns.
    pub timber: Option<TimberColumns>,
    /// The timber frame its parts are members of (mitcad#522): the rows go
    /// by frame, each frame's members apart from another's.
    pub frame: Option<String>,
    /// A section of a typed size (mitcad#1328): its options' values but
    /// the length (`B 76.2, T 4.8`). Its rows and stock go apart from its
    /// source's other sizes. None for other parts.
    pub size: Option<String>,
}

/// The stock pieces one source (in one row) is cut from.
#[derive(Debug, Clone, PartialEq)]
pub struct StockEntry {
    pub source: SourceId,
    pub designation: String,
    pub stock_length: f64,
    /// The pieces' lengths in cutting order, stock piece after stock piece.
    pub pieces: Vec<f64>,
    /// Where each stock piece's cuts end (indices into `pieces`).
    pub bars: Vec<usize>,
    pub needed: usize,
    pub offcut: f64,
    /// Pieces longer than the stock.
    pub too_long: Vec<f64>,
    /// The configuration row (mitcad#478).
    pub config: Option<String>,
    /// A timber entry's stock pieces' lengths, each the shortest that
    /// holds its cuts; empty for others.
    pub stock_lengths: Vec<f64>,
}

/// Totals over the timber rows.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TimberTotals {
    pub running_metres: f64,
    pub volume: f64,
    pub mass: f64,
    /// The area the profiled boards cover, m² (mitcad#477); None without
    /// profiled boards.
    pub area: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CutList {
    pub rows: Vec<CutRow>,
    pub stock: Vec<StockEntry>,
    /// The components the rows are of (the parts list lists the others).
    pub cut: Vec<ComponentUid>,
    /// The totals when the design has timber.
    pub timber: Option<TimberTotals>,
    /// The end caps' plates (mitcad#2243).
    pub plates: Vec<PlateRow>,
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub trait CutListBuild<K: Kernel> {
    /// The holes in a part: those connectors added and its own machining.
    fn part_machining(&self, info: &PartInfo) -> Vec<(FeatureUid, MachiningItem)>;

    /// The cut list.
    fn build_cut_list(&self, options: CutListOptions) -> CutList;

    /// What Convert to Features (mitcad#1815) keeps of a part for the
    /// cut list: its row for itself alone, the area its volume is
    /// reckoned with and its stock, as JSON for its component's `listed`;
    /// None for a component the cut list has no row of.
    fn listed_row(&self, component: ComponentUid) -> Option<serde_json::Value>;
}

impl<K: Kernel> CutListBuild<K> for Document<K> {
    fn listed_row(&self, component: ComponentUid) -> Option<serde_json::Value> {
        let list = self.build_cut_list(CutListOptions::default());
        let mut row = list
            .rows
            .into_iter()
            .find(|r| r.components.contains(&component))?;
        row.quantity = 0;
        row.components.clear();
        if let Some(t) = &mut row.timber {
            t.running_metres = 0.0;
            t.volume = 0.0;
            if let Some(b) = &mut t.board {
                b.area = 0.0;
            }
        }
        let info = self.part_info(component)?;
        let stock = self
            .state()
            .library_sources
            .get(row.source)
            .and_then(|source| {
                let meta = source
                    .build
                    .as_ref()
                    .and_then(|b| crate::BuildMeta::read_row(b, row.config.as_deref()).ok())
                    .unwrap_or_default();
                meta.length.as_ref().map(|length| ListedStock {
                    label: source.label(),
                    lengths: length.stock_lengths(),
                    timber: meta.is_timber(),
                })
            });
        serde_json::to_value(Listed {
            row,
            area: info.meta.section_area(),
            stock,
        })
        .ok()
    }

    fn part_machining(&self, info: &PartInfo) -> Vec<(FeatureUid, MachiningItem)> {
        let mut added: BTreeMap<FeatureUid, Option<ComponentUid>> = BTreeMap::new();
        for c in &self.assembly().components {
            let Some(record) = &c.build else { continue };
            for item in &record.adds {
                if let BuildItem::Feature(f) = item {
                    added.insert(*f, (c.uid != info.component).then_some(c.uid));
                }
            }
        }
        let params = self.parameters();
        let value = |id: &ParamId| params.value(*id).unwrap_or(f64::NAN);
        let mut out = Vec::new();
        for entry in self.features() {
            if entry.component != info.component {
                continue;
            }
            let Some(connector) = added.get(&entry.uid) else {
                continue;
            };
            let FeatureDef::Hole(hole) = &entry.def else {
                continue;
            };
            let at = match &hole.placement {
                mitcad_model::features::HolePlacement::Plane {
                    plane: mitcad_model::features::GeomRef::Mount { name, .. },
                    ..
                } => name.clone(),
                _ => String::new(),
            };
            let depth = match &hole.extent {
                mitcad_model::features::HoleExtent::Distance { depth } => Some(value(depth)),
                _ => None,
            };
            out.push((
                entry.uid,
                MachiningItem {
                    at,
                    diameter: value(&hole.diameter),
                    depth,
                    thread: hole.thread.as_ref().map(|t| t.designation.clone()),
                    connector: *connector,
                    kind: None,
                    size: None,
                    with: None,
                },
            ));
        }
        out
    }

    fn build_cut_list(&self, options: CutListOptions) -> CutList {
        let params = self.parameters();
        let quantities: BTreeMap<ComponentUid, usize> = self
            .parts_list()
            .into_iter()
            .map(|r| (r.component, r.quantity))
            .collect();
        let mut groups: BTreeMap<String, CutRow> = BTreeMap::new();
        let mut cut = Vec::new();
        for info in self.build_parts_info() {
            let (Some(source), Some(def), Some((_, length))) =
                (&info.source, &info.def, info.length)
            else {
                continue;
            };
            // Cut back by inward end caps (mitcad#2243): the piece is that
            // much shorter.
            let length = length
                - [def.cut_back.a, def.cut_back.b]
                    .into_iter()
                    .flatten()
                    .filter_map(|id| params.value(id))
                    .sum::<f64>();
            cut.push(info.component);
            let ends = [&def.ends.a, &def.ends.b].map(|e| {
                e.as_ref().and_then(|cut| {
                    let angle = params.value(cut.angle)?;
                    (angle.abs() > 1e-12).then(|| (angle, cut.tilt.unwrap_or_default()))
                })
            });
            let mut machining: Vec<MachiningItem> = self
                .part_machining(&info)
                .into_iter()
                .map(|(_, m)| m)
                .collect();
            let joinery = self.part_joinery(&info);
            let holes: Vec<FeatureUid> = self
                .part_machining(&info)
                .into_iter()
                .map(|(f, _)| f)
                .collect();
            // Its own features, with their values, for the grouping.
            let own: Vec<String> = self
                .features()
                .filter(|f| f.component == info.component)
                .filter(|f| Some(f.uid) != info.part && !holes.contains(&f.uid))
                .map(|f| {
                    let valued = f
                        .def
                        .map_params(&mut |_, id: &ParamId| {
                            Ok::<_, ()>(params.value(*id).unwrap_or(f64::NAN))
                        })
                        .expect("values never fail");
                    serde_json::to_string(&valued).unwrap_or_default()
                })
                .collect();
            let values = part_values(def, params);
            let config = def.config.as_deref();
            let timber = info.is_timber();
            let grade = info
                .record
                .grade
                .clone()
                .or_else(|| info.meta.material.as_ref().map(|m| m.grade.clone()))
                .unwrap_or_default();
            // A timber frame's members by frame (mitcad#522), after the
            // parts of no frame.
            let frame = self.frame_name_of(info.component);
            let mut key = format!(
                "{}|{}|{:?}|{:.6}|{:?}|{:?}|{:?}",
                frame.as_deref().unwrap_or_default(),
                source.id,
                config,
                length,
                ends.map(|e| e.map(|(a, t)| (format!("{a:.9}"), t.as_str()))),
                machining
                    .iter()
                    .map(|m| (m.at.clone(), format!("{:.6}", m.diameter), m.thread.clone()))
                    .collect::<Vec<_>>(),
                own
            );
            // A timber member's grade and joinery group it too.
            if timber {
                key.push_str(&format!(
                    "|{grade}|{:?}",
                    joinery
                        .iter()
                        .map(|m| (m.kind.clone(), m.size.map(|s| s.map(|v| format!("{v:.6}")))))
                        .collect::<Vec<_>>()
                ));
            }
            // A ripped board (the last of a row) covers what is left of it
            // (mitcad#477): the rip box reaches 1 mm past the board on
            // its three other sides.
            let kept = joinery
                .iter()
                .find(|m| m.kind.as_deref() == Some("rip"))
                .and_then(|m| m.size)
                .zip(info.meta.section_size())
                .and_then(|(s, [w, _])| (s[0] < w + 2.0 - 1e-6).then(|| w - (s[0] - 1.0)));
            machining.extend(joinery);
            let quantity = quantities.get(&info.component).copied().unwrap_or(1).max(1);
            let section = info.meta.section_size();
            // A profiled board's area is its own (mitcad#477).
            let area = info.meta.section_area();
            // A section of a typed size (mitcad#1328): its area in the
            // part's combination (the source's parameter's value) and its
            // options, which keep its rows and stock apart from its
            // source's other sizes.
            let cut_by = info.meta.length.as_ref().map(|l| l.parameter.as_str());
            let section_of = info
                .meta
                .area_parameter
                .as_deref()
                .and_then(|name| typed_section(source, config, &values, name, cut_by));
            let typed: Option<f64> = section_of.as_ref().and_then(|s| s.area);
            let size: Option<String> = section_of.map(|s| s.size).filter(|s| !s.is_empty());
            if let Some(size) = &size {
                key.push_str(&format!("|{size}"));
            }
            let row = groups.entry(key).or_insert_with(|| CutRow {
                source: source.id,
                designation: designation(source, config, &values),
                supplier: source.supplier.clone(),
                part_number: part_number(source, config),
                config: def.config.clone(),
                length,
                ends,
                machining,
                features: own.len(),
                quantity: 0,
                mass: match (timber, info.meta.density(), area, typed) {
                    (true, Some(density), Some(area), _)
                    | (false, Some(density), _, Some(area)) => Some(density * area * length / 1e9),
                    _ => info
                        .meta
                        .kilograms_per_metre()
                        .map(|kg| kg * length / 1000.0),
                },
                components: Vec::new(),
                timber: timber.then(|| TimberColumns {
                    material: info.meta.material.clone().unwrap_or_default(),
                    grade: grade.clone(),
                    nominal: info.meta.nominal_size(),
                    section: section.unwrap_or([0.0; 2]),
                    availability: info.meta.availability.clone(),
                    running_metres: 0.0,
                    volume: 0.0,
                    board: info.meta.board.as_ref().zip(info.meta.cover_width()).map(
                        |(b, cover)| BoardColumns {
                            profile: b.profile.clone(),
                            cover,
                            area: 0.0,
                            each: kept.map_or(cover, |k| k.min(cover)),
                        },
                    ),
                }),
                frame: frame.clone(),
                size: size.clone(),
            });
            add_to_row(row, info.component, quantity, area);
        }
        // The parts converted to features (mitcad#1815): as they were
        // listed, after the rows of their frame, with their stock.
        type StockKey = (SourceId, Option<String>, Option<String>);
        let mut listed_stock: BTreeMap<StockKey, ListedStock> = BTreeMap::new();
        for c in &self.assembly().components {
            let Some(listed) = c
                .listed
                .as_ref()
                .and_then(|value| serde_json::from_value::<Listed>(value.clone()).ok())
            else {
                continue;
            };
            cut.push(c.uid);
            let key = format!(
                "{}|listed|{}",
                listed.row.frame.as_deref().unwrap_or_default(),
                serde_json::to_string(&listed.row).unwrap_or_default()
            );
            let row = groups.entry(key).or_insert_with(|| listed.row.clone());
            let quantity = quantities.get(&c.uid).copied().unwrap_or(1).max(1);
            add_to_row(row, c.uid, quantity, listed.area);
            if let Some(stock) = listed.stock {
                listed_stock.insert(
                    (
                        listed.row.source,
                        listed.row.config.clone(),
                        listed.row.size.clone(),
                    ),
                    stock,
                );
            }
        }
        let rows: Vec<CutRow> = groups.into_values().collect();
        // The stock: first fit of the longest pieces into the longest stock,
        // per source and configuration row, and per typed size (mitcad#1328).
        let mut by_source: BTreeMap<StockKey, Vec<f64>> = BTreeMap::new();
        for row in &rows {
            for _ in 0..row.quantity {
                by_source
                    .entry((row.source, row.config.clone(), row.size.clone()))
                    .or_default()
                    .push(row.length);
            }
        }
        let mut stock = Vec::new();
        for ((id, config, size), mut pieces) in by_source {
            // The source's stock, or a converted part's as it was listed.
            let (lengths, timber, label) = match self.state().library_sources.get(id) {
                Some(source) => {
                    let meta = source
                        .build
                        .as_ref()
                        .and_then(|b| crate::BuildMeta::read_row(b, config.as_deref()).ok())
                        .unwrap_or_default();
                    let Some(length) = meta.length.as_ref() else {
                        continue;
                    };
                    (length.stock_lengths(), meta.is_timber(), source.label())
                }
                None => match listed_stock.get(&(id, config.clone(), size.clone())) {
                    Some(stock) => (stock.lengths.clone(), stock.timber, stock.label.clone()),
                    None => continue,
                },
            };
            let Some(&stock_length) = lengths.first() else {
                continue;
            };
            pieces.sort_by(|a, b| b.total_cmp(a));
            let mut bars: Vec<(f64, Vec<f64>)> = Vec::new();
            let mut too_long = Vec::new();
            for piece in pieces {
                if piece > stock_length + 1e-9 {
                    too_long.push(piece);
                    continue;
                }
                let need = piece + options.kerf;
                match bars.iter_mut().find(|(left, _)| {
                    *left + 1e-9 >= need || (*left + 1e-9 >= piece && !left.is_nan())
                }) {
                    Some((left, cuts)) => {
                        *left = (*left - need).max(0.0);
                        cuts.push(piece);
                    }
                    None => bars.push(((stock_length - need).max(0.0), vec![piece])),
                }
            }
            let mut flat = Vec::new();
            let mut ends = Vec::new();
            for (_, cuts) in &bars {
                flat.extend(cuts.iter().copied());
                ends.push(flat.len());
            }
            // A timber member's stock pieces: each the shortest stock length
            // that holds its cuts (the last cut needs no kerf after it).
            let mut offcut: f64 = bars.iter().map(|(left, _)| *left).sum();
            let mut stock_lengths = Vec::new();
            if timber {
                offcut = 0.0;
                for (_, cuts) in &bars {
                    let used: f64 = cuts.iter().sum::<f64>()
                        + options.kerf * cuts.len().saturating_sub(1) as f64;
                    let shortest = lengths
                        .iter()
                        .copied()
                        .filter(|l| *l + 1e-9 >= used)
                        .fold(stock_length, f64::min);
                    stock_lengths.push(shortest);
                    offcut += (shortest - used).max(0.0);
                }
            }
            let designation = match (&config, &size) {
                (_, Some(size)) => format!("{label} ({size})"),
                (Some(row), None) if timber => format!("{label} {row}"),
                _ => label,
            };
            stock.push(StockEntry {
                source: id,
                designation,
                stock_length,
                pieces: flat,
                bars: ends,
                needed: bars.len(),
                offcut,
                too_long,
                config,
                stock_lengths,
            });
        }
        // Totals over the timber rows.
        let timber_rows: Vec<&CutRow> = rows.iter().filter(|r| r.timber.is_some()).collect();
        let timber = (!timber_rows.is_empty()).then(|| TimberTotals {
            running_metres: timber_rows
                .iter()
                .map(|r| r.timber.as_ref().map_or(0.0, |t| t.running_metres))
                .sum(),
            volume: timber_rows
                .iter()
                .map(|r| r.timber.as_ref().map_or(0.0, |t| t.volume))
                .sum(),
            mass: timber_rows
                .iter()
                .map(|r| r.mass.unwrap_or(0.0) * r.quantity as f64)
                .sum(),
            area: timber_rows
                .iter()
                .filter_map(|r| r.timber.as_ref()?.board.as_ref().map(|b| b.area))
                .reduce(|a, b| a + b),
        });
        // End caps' plates (mitcad#2243), by their outline and thickness.
        let mut plates: BTreeMap<String, PlateRow> = BTreeMap::new();
        for cap in crate::end_cap::EndCapBuild::end_caps(self) {
            let Some(made) = self.end_cap_plate(cap.component) else {
                continue;
            };
            cut.push(cap.component);
            let member = self.part_info(cap.member);
            let density = member.as_ref().and_then(|m| m.meta.density());
            let material = member
                .as_ref()
                .and_then(|m| m.meta.material.as_ref())
                .map(|m| m.species.clone())
                .filter(|s| !s.is_empty());
            let designation = made.designation();
            let key = format!(
                "{designation}|{}|{:?}|{material:?}",
                made.plate.shape_key(),
                density.map(|d| format!("{d:.6}"))
            );
            let quantity = quantities.get(&cap.component).copied().unwrap_or(1).max(1);
            let row = plates.entry(key).or_insert_with(|| PlateRow {
                designation,
                width: made.plate.width(),
                height: made.plate.height(),
                thickness: made.thickness,
                area: made.plate.area(),
                disc: made.plate.is_disc(),
                material: material.clone(),
                quantity: 0,
                mass: density.map(|d| d * made.volume() / 1e9),
                components: Vec::new(),
                members: Vec::new(),
            });
            row.quantity += quantity;
            row.components.push(cap.component);
            if !row.members.contains(&cap.member) {
                row.members.push(cap.member);
            }
        }
        CutList {
            rows,
            stock,
            cut,
            timber,
            plates: plates.into_values().collect(),
        }
    }
}

/// A group of identical end cap plates (mitcad#2243).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlateRow {
    /// `Plate 40 x 40 x 5`.
    pub designation: String,
    /// The outline's width and height across the member, mm.
    pub width: f64,
    pub height: f64,
    pub thickness: f64,
    /// The outline's area, mm².
    pub area: f64,
    /// A round plate (a round tube's): `width` is its diameter.
    pub disc: bool,
    /// The members' material (their library's species), when it names one.
    pub material: Option<String>,
    pub quantity: usize,
    /// Each, kg, by the members' density.
    pub mass: Option<f64>,
    pub components: Vec<ComponentUid>,
    /// The members the plates close.
    pub members: Vec<ComponentUid>,
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub(crate) trait CutListCrateOps<K: Kernel> {
    /// A member's joinery and rips, from its derived part (mitcad#478).
    fn part_joinery(&self, info: &PartInfo) -> Vec<MachiningItem>;
}

impl<K: Kernel> CutListCrateOps<K> for Document<K> {
    fn part_joinery(&self, info: &PartInfo) -> Vec<MachiningItem> {
        let params = self.parameters();
        let value = |id: ParamId| params.value(id).unwrap_or(f64::NAN);
        info.def
            .as_ref()
            .map(|d| {
                d.cuts
                    .iter()
                    .map(|c| MachiningItem {
                        at: String::new(),
                        diameter: f64::NAN,
                        depth: c.depth,
                        thread: None,
                        connector: None,
                        kind: Some(c.kind.clone()),
                        size: Some(c.size.map(value)),
                        with: c.with,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// What Convert to Features (mitcad#1815) keeps of a part for the cut
/// list ([`CutListBuild::listed_row`]): its row for itself alone (no
/// quantity, components or totals), the section area its volume is
/// reckoned with, and the stock its source cut it from.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Listed {
    row: CutRow,
    #[serde(default)]
    area: Option<f64>,
    #[serde(default)]
    stock: Option<ListedStock>,
}

/// A converted part's stock: its source's name and stock lengths, and
/// whether it is timber (each stock piece the shortest that holds it).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ListedStock {
    label: String,
    lengths: Vec<f64>,
    timber: bool,
}

/// Counts a part into its row: its quantity, its component, and a timber
/// row's running metres, volume (by `area`, else the section) and boards'
/// area over the row.
fn add_to_row(row: &mut CutRow, component: ComponentUid, quantity: usize, area: Option<f64>) {
    row.quantity += quantity;
    row.components.push(component);
    if let Some(t) = &mut row.timber {
        let n = row.quantity as f64;
        t.running_metres = row.length * n / 1000.0;
        t.volume = area.unwrap_or(t.section[0] * t.section[1]) * row.length * n / 1e9;
        if let Some(b) = &mut t.board {
            b.area = b.each * row.length * n / 1e6;
        }
    }
}

/// A number that may be NaN (a joinery cut's diameter), null in JSON.
mod nan_null {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
        if value.is_nan() {
            serializer.serialize_none()
        } else {
            serializer.serialize_f64(*value)
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
        Ok(Option::<f64>::deserialize(deserializer)?.unwrap_or(f64::NAN))
    }
}
