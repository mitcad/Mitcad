// SPDX-License-Identifier: MIT
//! Explode Text (mitcad#2163, `sketch.explode_text`): a sketch text or QR
//! code becomes plain sketch curves where it is laid out now
//! ([`crate::sketch::text_explode`]), as one undo step `Explode Text in
//! Sketch1`, and the later features keep working.
//!
//! The names later features use for the text's pieces are rewritten to the
//! new curves, as the 3D sketch conversion rewrites later features' paths
//! (`sketch_conversion.rs`):
//!
//! - A profile region whose key names the text's contours (`r{t5.g2.c0}`)
//!   takes the key of the region with the same area and centre after the
//!   explode (in an object whose `sketch` is this sketch, in a feature that
//!   sweeps its profiles).
//! - A face, edge or vertex name of such a feature is rewritten piece by
//!   piece: a profile segment that names the text (`side(t5.g2.c0)`) maps
//!   to the new segments its pieces became, found by their geometry; a
//!   region in a key (`end(r{t5.g2.c0})`) as above; names in keys (a
//!   fillet's edge) the same way. A side face numbered `#k` among the
//!   pieces of one contour (the pieces of a split face are numbered in
//!   geometric order) takes the piece whose middle it passes nearest to,
//!   found on the bodies before the explode; without `#k` it means every
//!   piece, so it becomes every piece's face in a list (a fillet's edges)
//!   and an edge numbered `#k` between such faces the k-th piece's, in the
//!   order the old loop ran.
//! - A name that cannot be mapped stays as it was and is a warning, as for
//!   the other sketch edits that change the name of a region a later
//!   feature uses ("… uses the region r{…}, whose name changes: check
//!   it"); nothing is refused for it.
//!
//! The sketch edit is taken first (its evaluation gives the new regions),
//! then the rewrite; the two are merged into one undo step.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde_json::Value;

use super::{Document, FeatureEntry, ModelError, invalid};
use crate::features::{FeatureDef, SketchOutput};
use crate::ids::{EntityUid, FeatureUid};
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::profile::{ProfileRegion, SegmentGeometry, SketchFrame};
use crate::sketch::edit::EditReport;
use crate::topo::{EdgeName, FaceName, RegionKey, RoleKey, SegmentKey, TopoName, VertexName};

/// What Explode Text did.
#[derive(Debug, Clone, Default)]
pub struct TextExploded {
    /// The new curves, contour by contour, piece by piece.
    pub curves: Vec<EntityUid>,
    /// The edit of the sketch (its new points and curves, the text removed).
    pub report: EditReport,
    /// The later features whose names of the text's pieces were rewritten.
    pub rewritten: Vec<(FeatureUid, String)>,
    /// What the user is to know, in words: names that could not be mapped,
    /// contours left out.
    pub warnings: Vec<String>,
}

/// Whether a segment's curve or a curve at its ends is a contour of `text`.
fn segment_names(segment: &SegmentKey, text: EntityUid) -> bool {
    segment.curves().any(|c| c.text() == Some(text))
}

fn region_names(region: &RegionKey, text: EntityUid) -> bool {
    region.segments().any(|s| segment_names(s, text))
}

/// A point in the middle of a piece of a profile, for matching pieces by
/// geometry.
fn middle(geometry: &SegmentGeometry) -> [f64; 2] {
    match geometry {
        SegmentGeometry::Line { start, end } => {
            [(start[0] + end[0]) / 2.0, (start[1] + end[1]) / 2.0]
        }
        SegmentGeometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => {
            let end = if end_angle <= start_angle {
                end_angle + std::f64::consts::TAU
            } else {
                *end_angle
            };
            let t = (start_angle + end) / 2.0;
            [center[0] + radius * t.cos(), center[1] + radius * t.sin()]
        }
        SegmentGeometry::Circle { center, radius, .. } => [center[0] + radius, center[1]],
        SegmentGeometry::Ellipse { center, .. } | SegmentGeometry::EllipseArc { center, .. } => {
            *center
        }
        // A Bézier curve (knots 0 and 1) at its middle; another spline at
        // the mean of its poles.
        SegmentGeometry::BSpline { poles, knots, .. } => {
            if knots.len() == 2 {
                let mut points = poles.clone();
                while points.len() > 1 {
                    points = points
                        .windows(2)
                        .map(|w| [(w[0][0] + w[1][0]) / 2.0, (w[0][1] + w[1][1]) / 2.0])
                        .collect();
                }
                points.first().copied().unwrap_or_default()
            } else {
                let n = poles.len().max(1) as f64;
                let sum = poles
                    .iter()
                    .fold([0.0, 0.0], |s, p| [s[0] + p[0], s[1] + p[1]]);
                [sum[0] / n, sum[1] / n]
            }
        }
    }
}

/// The points that say where a piece runs: its ends (either way round)
/// and its middle.
fn ends(geometry: &SegmentGeometry) -> Option<([f64; 2], [f64; 2])> {
    match geometry {
        SegmentGeometry::Line { start, end } => Some((*start, *end)),
        SegmentGeometry::BSpline { poles, .. } => Some((*poles.first()?, *poles.last()?)),
        _ => None,
    }
}

/// Whether two pieces of profiles are the same piece (within a tolerance
/// far below a drawing's, far above rounding).
fn same_piece(a: &SegmentGeometry, b: &SegmentGeometry) -> bool {
    let close = |p: [f64; 2], q: [f64; 2]| {
        (p[0] - q[0]).hypot(p[1] - q[1]) <= 1e-7 * (1.0 + q[0].abs() + q[1].abs())
    };
    if std::mem::discriminant(a) != std::mem::discriminant(b) {
        return false;
    }
    if !close(middle(a), middle(b)) {
        return false;
    }
    match (ends(a), ends(b)) {
        (Some((a0, a1)), Some((b0, b1))) => {
            (close(a0, b0) && close(a1, b1)) || (close(a0, b1) && close(a1, b0))
        }
        // Arcs, circles and ellipses: the same middle and kind, and the
        // same size.
        _ => match (a, b) {
            (
                SegmentGeometry::Arc {
                    radius: r, center, ..
                },
                SegmentGeometry::Arc {
                    radius: s,
                    center: c,
                    ..
                },
            )
            | (
                SegmentGeometry::Circle {
                    radius: r, center, ..
                },
                SegmentGeometry::Circle {
                    radius: s,
                    center: c,
                    ..
                },
            ) => (r - s).abs() <= 1e-7 * (1.0 + r.abs()) && close(*center, *c),
            _ => a == b,
        },
    }
}

/// The pieces of each profile segment that names `text`, in the order its
/// loop runs: first in the outer loops of the regions whose keys name it
/// (the loop an extrusion of the region numbers its faces by), then in any
/// other loop.
fn old_pieces(
    regions: &[ProfileRegion],
    text: EntityUid,
) -> BTreeMap<SegmentKey, Vec<SegmentGeometry>> {
    let mut out: BTreeMap<SegmentKey, Vec<SegmentGeometry>> = BTreeMap::new();
    for named_first in [true, false] {
        for region in regions {
            for (i, l) in region.loops.iter().enumerate() {
                let mut found: BTreeMap<SegmentKey, Vec<SegmentGeometry>> = BTreeMap::new();
                for s in &l.segments {
                    if !segment_names(&s.key, text) {
                        continue;
                    }
                    let named = i == 0 && region.key.segments().any(|k| *k == s.key);
                    if named_first && !named {
                        continue;
                    }
                    found
                        .entry(s.key.clone())
                        .or_default()
                        .push(s.geometry.clone());
                }
                for (key, pieces) in found {
                    out.entry(key).or_insert(pieces);
                }
            }
        }
    }
    out
}

/// The new segment of each old piece, found by its geometry.
fn new_segments(
    old: &BTreeMap<SegmentKey, Vec<SegmentGeometry>>,
    regions: &[ProfileRegion],
) -> BTreeMap<SegmentKey, Vec<Option<SegmentKey>>> {
    let new: Vec<(&SegmentKey, &SegmentGeometry)> = regions
        .iter()
        .flat_map(|r| &r.loops)
        .flat_map(|l| &l.segments)
        .map(|s| (&s.key, &s.geometry))
        .collect();
    old.iter()
        .map(|(key, pieces)| {
            let found = pieces
                .iter()
                .map(|g| {
                    new.iter()
                        .find(|(_, h)| same_piece(g, h))
                        .map(|(k, _)| (*k).clone())
                })
                .collect();
            (key.clone(), found)
        })
        .collect()
}

/// The new key of each old region that names `text`: the one region with
/// the same area and centre; None where there is not exactly one.
fn new_regions(
    old: &[ProfileRegion],
    new: &[ProfileRegion],
    text: EntityUid,
) -> BTreeMap<RegionKey, Option<RegionKey>> {
    let measured: Vec<(f64, [f64; 2], &RegionKey)> = new
        .iter()
        .map(|r| {
            let (area, centre) = r.area_and_centre();
            (area, centre, &r.key)
        })
        .collect();
    old.iter()
        .filter(|r| region_names(&r.key, text))
        .map(|r| {
            let (area, centre) = r.area_and_centre();
            let tolerance = 1e-7 * (1.0 + area.abs());
            let size = 1e-7 * (1.0 + centre[0].abs() + centre[1].abs());
            let found: Vec<&RegionKey> = measured
                .iter()
                .filter(|(a, c, _)| {
                    (a - area).abs() <= tolerance
                        && (c[0] - centre[0]).hypot(c[1] - centre[1]) <= size
                })
                .map(|(_, _, k)| *k)
                .collect();
            let key = match found.as_slice() {
                [one] => Some((*one).clone()),
                _ => None,
            };
            (r.key.clone(), key)
        })
        .collect()
}

/// A name mapped to the new curves.
enum Mapped<T> {
    /// It names nothing of the text.
    Same,
    One(T),
    /// Every piece's (a face without `#k` on a contour of several pieces).
    Many(Vec<T>),
    /// It cannot be mapped.
    Lost,
}

/// The rewrite of later features' names of the text's pieces.
struct Rename {
    sketch: FeatureUid,
    text: EntityUid,
    /// The features that sweep the sketch's profiles: their faces are named
    /// by its segments and regions.
    users: BTreeSet<FeatureUid>,
    segments: BTreeMap<SegmentKey, Vec<Option<SegmentKey>>>,
    regions: BTreeMap<RegionKey, Option<RegionKey>>,
    /// The piece a side face numbered `#k` among a contour's pieces lies
    /// on, by its name up to that number.
    splits: BTreeMap<String, usize>,
}

impl Rename {
    fn segment_face(&self, face: &FaceName, segment: &SegmentKey) -> Mapped<FaceName> {
        let Some(pieces) = self.segments.get(segment) else {
            return Mapped::Lost;
        };
        let with = |s: &SegmentKey, split: &[u32]| FaceName {
            key: Some(RoleKey::Segment(s.clone())),
            split: split.to_vec(),
            ..face.clone()
        };
        if face.split.is_empty() || pieces.len() == 1 {
            let Some(mut all) = pieces.iter().cloned().collect::<Option<Vec<SegmentKey>>>() else {
                return Mapped::Lost;
            };
            let mut seen = BTreeSet::new();
            all.retain(|s| seen.insert(s.clone()));
            let mut faces: Vec<FaceName> = all.iter().map(|s| with(s, &face.split)).collect();
            return match faces.len() {
                0 => Mapped::Lost,
                1 => Mapped::One(faces.remove(0)),
                _ => Mapped::Many(faces),
            };
        }
        let first = FaceName {
            split: face.split[..1].to_vec(),
            ..face.clone()
        };
        match self
            .splits
            .get(&first.to_string())
            .and_then(|i| pieces.get(*i))
        {
            Some(Some(s)) => Mapped::One(with(s, &face.split[1..])),
            _ => Mapped::Lost,
        }
    }

    fn face(&self, face: &FaceName) -> Mapped<FaceName> {
        let user = self.users.contains(&face.feature);
        let wrap = |key: RoleKey| FaceName {
            key: Some(key),
            ..face.clone()
        };
        match &face.key {
            Some(RoleKey::Segment(s)) if user && segment_names(s, self.text) => {
                self.segment_face(face, s)
            }
            Some(RoleKey::Region(r)) if user && region_names(r, self.text) => {
                match self.regions.get(r) {
                    Some(Some(new)) => Mapped::One(wrap(RoleKey::Region(new.clone()))),
                    _ => Mapped::Lost,
                }
            }
            Some(RoleKey::Edge(e)) => match self.edge(e) {
                Mapped::Same => Mapped::Same,
                Mapped::One(n) => Mapped::One(wrap(RoleKey::Edge(Box::new(n)))),
                Mapped::Many(ns) => Mapped::Many(
                    ns.into_iter()
                        .map(|n| wrap(RoleKey::Edge(Box::new(n))))
                        .collect(),
                ),
                Mapped::Lost => Mapped::Lost,
            },
            Some(RoleKey::Vertex(v)) => match self.vertex(v) {
                Mapped::Same => Mapped::Same,
                Mapped::One(n) => Mapped::One(wrap(RoleKey::Vertex(Box::new(n)))),
                Mapped::Many(_) | Mapped::Lost => Mapped::Lost,
            },
            Some(RoleKey::Face(f)) => match self.face(f) {
                Mapped::Same => Mapped::Same,
                Mapped::One(n) => Mapped::One(wrap(RoleKey::Face(Box::new(n)))),
                Mapped::Many(ns) => Mapped::Many(
                    ns.into_iter()
                        .map(|n| wrap(RoleKey::Face(Box::new(n))))
                        .collect(),
                ),
                Mapped::Lost => Mapped::Lost,
            },
            _ => Mapped::Same,
        }
    }

    fn edge(&self, edge: &EdgeName) -> Mapped<EdgeName> {
        let [a, b] = edge.faces();
        let (ma, mb) = (self.face(a), self.face(b));
        let many = matches!(ma, Mapped::Many(_)) || matches!(mb, Mapped::Many(_));
        let list = |m: Mapped<FaceName>, old: &FaceName| -> Option<Vec<FaceName>> {
            match m {
                Mapped::Same => Some(vec![old.clone()]),
                Mapped::One(f) => Some(vec![f]),
                Mapped::Many(fs) => Some(fs),
                Mapped::Lost => None,
            }
        };
        if matches!((&ma, &mb), (Mapped::Same, Mapped::Same)) {
            return Mapped::Same;
        }
        let (Some(la), Some(lb)) = (list(ma, a), list(mb, b)) else {
            return Mapped::Lost;
        };
        if !many {
            let mut new = EdgeName::new(la[0].clone(), lb[0].clone());
            new.index = edge.index;
            return Mapped::One(new);
        }
        // Between two pieces of one contour's faces: each piece and the
        // next, as the loop runs; otherwise every pair.
        let edges: Vec<EdgeName> = if a == b {
            (0..la.len())
                .map(|i| EdgeName::new(la[i].clone(), la[(i + 1) % la.len()].clone()))
                .collect()
        } else {
            la.iter()
                .flat_map(|x| lb.iter().map(move |y| EdgeName::new(x.clone(), y.clone())))
                .collect()
        };
        match edge.index {
            Some(k) => edges
                .get(k as usize)
                .cloned()
                .map_or(Mapped::Lost, Mapped::One),
            None if edges.len() == 1 => Mapped::One(edges[0].clone()),
            None => Mapped::Many(edges),
        }
    }

    fn vertex(&self, vertex: &VertexName) -> Mapped<VertexName> {
        let mut changed = false;
        let mut faces = Vec::new();
        for f in vertex.faces() {
            match self.face(f) {
                Mapped::Same => faces.push(f.clone()),
                Mapped::One(n) => {
                    changed = true;
                    faces.push(n);
                }
                Mapped::Many(_) | Mapped::Lost => return Mapped::Lost,
            }
        }
        if !changed {
            return Mapped::Same;
        }
        match VertexName::new(faces) {
            Some(mut new) => {
                new.index = vertex.index;
                Mapped::One(new)
            }
            None => Mapped::Lost,
        }
    }

    /// A string of a definition mapped (`in_sketch`: in an object whose
    /// `sketch` is this sketch, where region keys and curve ids are its).
    fn text(&self, text: &str, in_sketch: bool) -> Mapped<String> {
        if text.starts_with("r{") {
            if !in_sketch {
                return Mapped::Same;
            }
            let Ok(key) = text.parse::<RegionKey>() else {
                return Mapped::Same;
            };
            if !region_names(&key, self.text) {
                return Mapped::Same;
            }
            return match self.regions.get(&key) {
                Some(Some(new)) => Mapped::One(new.to_string()),
                _ => Mapped::Lost,
            };
        }
        if let Ok(name) = text.parse::<TopoName>() {
            return match &name {
                TopoName::Face(f) => self.face(f).texts(),
                TopoName::Edge(e) => self.edge(e).texts(),
                TopoName::Vertex(v) => self.vertex(v).texts(),
            };
        }
        // A contour by its id, or a segment of it, alone (a path's curve):
        // no one new curve is it.
        if in_sketch
            && let Ok(segment) = text.parse::<SegmentKey>()
            && segment_names(&segment, self.text)
        {
            return Mapped::Lost;
        }
        Mapped::Same
    }

    /// Rewrites the strings of a definition in its JSON form; returns
    /// whether any changed and adds the names it could not map to `lost`.
    fn value(&self, value: &mut Value, in_sketch: bool, lost: &mut Vec<String>) -> bool {
        match value {
            Value::String(text) if !text.starts_with(PARAM) => match self.text(text, in_sketch) {
                Mapped::Same => false,
                Mapped::One(new) => {
                    *text = new;
                    true
                }
                Mapped::Many(_) | Mapped::Lost => {
                    lost.push(text.clone());
                    false
                }
            },
            Value::Array(items) => {
                let mut changed = false;
                let mut out = Vec::with_capacity(items.len());
                for mut item in std::mem::take(items) {
                    if let Value::String(text) = &item
                        && !text.starts_with(PARAM)
                        && let Mapped::Many(new) = self.text(text, in_sketch)
                    {
                        for n in new {
                            let n = Value::String(n);
                            if !out.contains(&n) {
                                out.push(n);
                            }
                        }
                        changed = true;
                        continue;
                    }
                    changed |= self.value(&mut item, in_sketch, lost);
                    out.push(item);
                }
                *items = out;
                changed
            }
            Value::Object(fields) => {
                let in_sketch = match fields.get("sketch") {
                    Some(Value::String(s)) => *s == self.sketch.to_string(),
                    _ => in_sketch,
                };
                let mut changed = false;
                for (key, item) in fields.iter_mut() {
                    if !TEXT_FIELDS.contains(&key.as_str()) {
                        changed |= self.value(item, in_sketch, lost);
                    }
                }
                changed
            }
            _ => false,
        }
    }

    /// A definition with its names of the text's pieces rewritten, and the
    /// names it could not map; None when nothing changed.
    fn def(&self, def: &FeatureDef) -> Result<(Option<FeatureDef>, Vec<String>), ModelError> {
        let (mut value, slots) =
            json_form(def).ok_or_else(|| invalid("explode text: a definition has no JSON form"))?;
        let mut lost = Vec::new();
        if !self.value(&mut value, false, &mut lost) {
            return Ok((None, lost));
        }
        let renamed: FeatureDef<String> =
            serde_json::from_value(value).map_err(|e| invalid(format!("explode text: {e}")))?;
        let renamed = renamed.map_params(&mut |slot, marker: &String| {
            marker
                .strip_prefix(PARAM)
                .and_then(|i| i.parse::<usize>().ok())
                .and_then(|i| slots.get(i).copied())
                .ok_or_else(|| invalid(format!("explode text: {slot} lost its parameter")))
        })?;
        Ok((Some(renamed), lost))
    }
}

impl<T: std::fmt::Display> Mapped<T> {
    /// The names in their text form.
    fn texts(self) -> Mapped<String> {
        match self {
            Mapped::Same => Mapped::Same,
            Mapped::One(n) => Mapped::One(n.to_string()),
            Mapped::Many(ns) => Mapped::Many(ns.iter().map(ToString::to_string).collect()),
            Mapped::Lost => Mapped::Lost,
        }
    }
}

/// Marks a parameter in the JSON form of a definition (as Reorient Design's
/// and Auto Fix's).
const PARAM: char = '\u{1}';

/// A definition in its JSON form, each parameter a marker, with the
/// parameters by marker.
fn json_form(def: &FeatureDef) -> Option<(Value, Vec<ParamId>)> {
    let mut slots = Vec::new();
    let marked = def
        .map_params(&mut |_, id: &ParamId| {
            slots.push(*id);
            Ok::<_, ()>(format!("{PARAM}{}", slots.len() - 1))
        })
        .ok()?;
    Some((serde_json::to_value(&marked).ok()?, slots))
}

/// Fields of definitions that are texts, not references.
const TEXT_FIELDS: [&str; 5] = ["name", "source", "text", "font", "brep"];

/// What a lost name is, for its warning.
fn what(name: &str) -> &'static str {
    if name.starts_with("r{") {
        "the region"
    } else if name.starts_with("E{") {
        "the edge"
    } else if name.starts_with("V{") {
        "the vertex"
    } else if name.starts_with('F') {
        "the face"
    } else {
        "the curve"
    }
}

impl<K: Kernel> Document<K> {
    /// Explode Text (see the module documentation): text `text` of sketch
    /// `sketch` becomes plain sketch curves, one undo step `Explode Text in
    /// <sketch>`.
    pub fn explode_sketch_text(
        &mut self,
        sketch: FeatureUid,
        text: EntityUid,
    ) -> Result<TextExploded, ModelError> {
        let position = self.state.require(sketch)?;
        let entry = self.state.features[position].clone();
        self.state.editable(entry.component)?;
        let name = entry.name.clone();
        let FeatureDef::Sketch(def) = &entry.def else {
            return Err(invalid(format!("{name} ({sketch}) is not a sketch")));
        };
        if !def.texts.iter().any(|t| t.id == text) {
            return Err(invalid(format!("text t{} does not exist", text.0)));
        }
        let output: SketchOutput = self.sketch_output(sketch).cloned().ok_or_else(|| {
            invalid(format!(
                "{name} has no result to explode a text of (it failed, or it is suppressed or \
                 after the timeline marker)"
            ))
        })?;
        let outline = output
            .texts
            .iter()
            .find(|o| o.id == text)
            .cloned()
            .ok_or_else(|| {
                invalid(crate::notice::said(
                    "the text has no outlines to explode",
                    format!("text t{} has no outlines to explode", text.0),
                ))
            })?;
        let users: BTreeSet<FeatureUid> = self
            .state
            .features
            .iter()
            .skip(position + 1)
            .filter(|f| f.def.info().profiles().iter().any(|p| p.sketch == sketch))
            .map(|f| f.uid)
            .collect();
        let old = old_pieces(&output.regions, text);
        let splits = self.split_pieces(position, &users, text, &old, &output.frame);

        let depth = self.undo_depth();
        let (made, report) = self.edit_sketch(sketch, "Explode Text", |edit| {
            edit.explode_text(text, &outline)
        })?;
        let label = format!("Explode Text in {name}");
        let mut warnings = Vec::new();
        if made.dropped > 0 {
            warnings.push(if made.dropped == 1 {
                "an outline of the text that is not closed was left out".to_owned()
            } else {
                format!(
                    "{} outlines of the text that are not closed were left out",
                    made.dropped
                )
            });
        }

        // The later features' names of the text's pieces, rewritten from
        // the regions the sketch has now.
        let mut rewritten = Vec::new();
        if let Some(new) = self.sketch_output(sketch).cloned() {
            let rename = Rename {
                sketch,
                text,
                users,
                segments: new_segments(&old, &new.regions),
                regions: new_regions(&output.regions, &new.regions, text),
                splits,
            };
            let mut changed: Vec<(usize, FeatureDef)> = Vec::new();
            for (at, f) in self.state.features.iter().enumerate().skip(position + 1) {
                let (def, lost) = rename.def(&f.def)?;
                let mut seen = BTreeSet::new();
                for reference in lost {
                    if seen.insert(reference.clone()) {
                        warnings.push(format!(
                            "{} uses {} {reference}, whose name changes: check it",
                            f.name,
                            what(&reference)
                        ));
                    }
                }
                if let Some(def) = def {
                    rewritten.push((f.uid, f.name.clone()));
                    changed.push((at, def));
                }
            }
            if !changed.is_empty() {
                self.apply(|state| {
                    for (at, def) in changed {
                        let edited = FeatureEntry {
                            def,
                            ..(*state.features[at]).clone()
                        };
                        state
                            .check_feature(at, &edited)
                            .map_err(ModelError::Invalid)?;
                        state.features[at] = Arc::new(edited);
                    }
                    Ok((label.clone(), ()))
                })?;
                self.merge_undo(depth, &label);
            }
        }
        Ok(TextExploded {
            curves: made.curves,
            report,
            rewritten,
            warnings,
        })
    }

    /// The piece each side face numbered among a contour's pieces lies on
    /// (`F3:side(t5.g2.c0)#1`), that the later features after `position`
    /// name: on a body before the explode, the piece whose middle the face
    /// passes nearest to.
    fn split_pieces(
        &self,
        position: usize,
        users: &BTreeSet<FeatureUid>,
        text: EntityUid,
        old: &BTreeMap<SegmentKey, Vec<SegmentGeometry>>,
        frame: &SketchFrame,
    ) -> BTreeMap<String, usize> {
        let mut wanted: BTreeMap<String, (FaceName, &[SegmentGeometry])> = BTreeMap::new();
        for f in self.state.features.iter().skip(position + 1) {
            let Some(value) = json_form(&f.def).map(|(value, _)| value) else {
                continue;
            };
            let mut found = Vec::new();
            strings(&value, &mut found);
            for text_form in found {
                let Ok(name) = text_form.parse::<TopoName>() else {
                    continue;
                };
                for_each_face(&name, &mut |face| {
                    let Some(RoleKey::Segment(segment)) = &face.key else {
                        return;
                    };
                    if face.split.is_empty()
                        || !users.contains(&face.feature)
                        || !segment_names(segment, text)
                    {
                        return;
                    }
                    let Some(pieces) = old.get(segment).filter(|p| p.len() > 1) else {
                        return;
                    };
                    let first = FaceName {
                        split: face.split[..1].to_vec(),
                        ..face.clone()
                    };
                    wanted
                        .entry(first.to_string())
                        .or_insert((first, pieces.as_slice()));
                });
            }
        }
        let kernel = self.kernel();
        let bodies = self.bodies();
        let mut out = BTreeMap::new();
        for (key, (face, pieces)) in wanted {
            let Some(body) = bodies
                .iter()
                .find(|b| kernel.count_faces(b.shape, &face).unwrap_or(0) > 0)
            else {
                continue;
            };
            let mut best: Option<(usize, f64)> = None;
            for (i, piece) in pieces.iter().enumerate() {
                let near = frame.point(middle(piece));
                let Ok(found) = kernel.face_point_normal(body.shape, &face, near) else {
                    best = None;
                    break;
                };
                let d = mitcad_vec3::distance(found.point, near);
                if best.is_none_or(|(_, b)| d < b - 1e-9) {
                    best = Some((i, d));
                }
            }
            if let Some((i, _)) = best {
                out.insert(key, i);
            }
        }
        out
    }
}

/// Every string of a JSON value (outside the text fields).
fn strings(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => out.push(s.clone()),
        Value::Array(items) => items.iter().for_each(|i| strings(i, out)),
        Value::Object(fields) => {
            for (key, item) in fields {
                if !TEXT_FIELDS.contains(&key.as_str()) {
                    strings(item, out);
                }
            }
        }
        _ => {}
    }
}

/// Every face name a name holds, itself and those in its keys.
fn for_each_face(name: &TopoName, f: &mut dyn FnMut(&FaceName)) {
    fn face(name: &FaceName, f: &mut dyn FnMut(&FaceName)) {
        f(name);
        match &name.key {
            Some(RoleKey::Edge(e)) => e.faces().iter().for_each(|x| face(x, f)),
            Some(RoleKey::Vertex(v)) => v.faces().iter().for_each(|x| face(x, f)),
            Some(RoleKey::Face(x)) => face(x, f),
            _ => {}
        }
    }
    match name {
        TopoName::Face(x) => face(x, f),
        TopoName::Edge(e) => e.faces().iter().for_each(|x| face(x, f)),
        TopoName::Vertex(v) => v.faces().iter().for_each(|x| face(x, f)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{ProfileLoop, ProfileSegment};

    fn line(a: [f64; 2], b: [f64; 2]) -> SegmentGeometry {
        SegmentGeometry::Line { start: a, end: b }
    }

    fn region(key: &str, segments: Vec<(&str, SegmentGeometry)>) -> ProfileRegion {
        ProfileRegion {
            key: key.parse().unwrap(),
            loops: vec![ProfileLoop {
                segments: segments
                    .into_iter()
                    .map(|(k, geometry)| ProfileSegment {
                        key: k.parse().unwrap(),
                        geometry,
                    })
                    .collect(),
            }],
        }
    }

    /// A square letter t5.g0.c0 of four pieces, and the same square as the
    /// curves c10..c13 after the explode.
    fn square() -> (ProfileRegion, ProfileRegion) {
        let corners = [[0.0, 0.0], [2.0, 0.0], [2.0, 3.0], [0.0, 3.0]];
        let side = |i: usize| line(corners[i], corners[(i + 1) % 4]);
        let old = region(
            "r{t5.g0.c0}",
            (0..4).map(|i| ("t5.g0.c0", side(i))).collect(),
        );
        let names = [
            "c10[c13,c11]",
            "c11[c10,c12]",
            "c12[c11,c13]",
            "c13[c12,c10]",
        ];
        let new = region(
            "r{c10[c13,c11],c11[c10,c12],c12[c11,c13],c13[c12,c10]}",
            (0..4).map(|i| (names[i], side(i))).collect(),
        );
        (old, new)
    }

    fn rename(old: &ProfileRegion, new: &ProfileRegion) -> Rename {
        let text = EntityUid(5);
        let pieces = old_pieces(std::slice::from_ref(old), text);
        Rename {
            sketch: FeatureUid(1),
            text,
            users: [FeatureUid(2)].into_iter().collect(),
            segments: new_segments(&pieces, std::slice::from_ref(new)),
            regions: new_regions(std::slice::from_ref(old), std::slice::from_ref(new), text),
            splits: [("F2:side(t5.g0.c0)#2".to_owned(), 1)]
                .into_iter()
                .collect(),
        }
    }

    fn mapped(r: &Rename, text: &str, in_sketch: bool) -> Result<Vec<String>, &'static str> {
        match r.text(text, in_sketch) {
            Mapped::Same => Err("same"),
            Mapped::One(n) => Ok(vec![n]),
            Mapped::Many(ns) => Ok(ns),
            Mapped::Lost => Err("lost"),
        }
    }

    #[test]
    fn regions_faces_and_edges_of_a_letter_follow_its_new_curves() {
        let (old, new) = square();
        let r = rename(&old, &new);
        let region = "r{c10[c13,c11],c11[c10,c12],c12[c11,c13],c13[c12,c10]}";
        assert_eq!(mapped(&r, "r{t5.g0.c0}", true), Ok(vec![region.to_owned()]));
        // Another sketch's region of the same name stays.
        assert_eq!(mapped(&r, "r{t5.g0.c0}", false), Err("same"));
        // A cap of the letter's extrusion.
        assert_eq!(
            mapped(&r, "F2:end(r{t5.g0.c0})", false),
            Ok(vec![format!("F2:end({region})")])
        );
        // A side face numbered among the pieces: the piece it lies on.
        assert_eq!(
            mapped(&r, "F2:side(t5.g0.c0)#2", false),
            Ok(vec!["F2:side(c11[c10,c12])".to_owned()])
        );
        // Without a number: every piece's face.
        assert_eq!(mapped(&r, "F2:side(t5.g0.c0)", false).unwrap().len(), 4);
        // An edge numbered between the cap and the pieces: the k-th piece's.
        let edge = format!("E{{F2:end({region})|F2:side(c12[c11,c13])}}");
        assert_eq!(
            mapped(&r, "E{F2:end(r{t5.g0.c0})|F2:side(t5.g0.c0)}#2", false),
            Ok(vec![edge])
        );
        // Between two pieces: each piece and the next.
        assert_eq!(
            mapped(&r, "E{F2:side(t5.g0.c0)|F2:side(t5.g0.c0)}#0", false),
            Ok(vec![
                "E{F2:side(c10[c13,c11])|F2:side(c11[c10,c12])}".to_owned()
            ])
        );
        // A fillet's face on such an edge follows it.
        assert!(
            mapped(
                &r,
                "F4:fillet(E{F2:end(r{t5.g0.c0})|F2:side(t5.g0.c0)}#1)",
                false
            )
            .unwrap()[0]
                .starts_with("F4:fillet(E{F2:end(r{c10")
        );
        // A feature that does not sweep the sketch keeps its names; a
        // numbered face whose piece was not found is lost.
        assert_eq!(mapped(&r, "F7:side(t5.g0.c0)", false), Err("same"));
        assert_eq!(mapped(&r, "F2:side(t5.g0.c0)#3", false), Err("lost"));
    }

    #[test]
    fn a_list_takes_every_piece_and_a_lone_name_is_lost() {
        let (old, new) = square();
        let r = rename(&old, &new);
        let mut value = serde_json::json!({
            "edges": ["E{F2:end(r{t5.g0.c0})|F2:side(t5.g0.c0)}"],
            "face": "F2:side(t5.g0.c0)",
            "profiles": [{"sketch": "F1", "region": "r{t5.g0.c0}"}],
        });
        let mut lost = Vec::new();
        assert!(r.value(&mut value, false, &mut lost));
        assert_eq!(value["edges"].as_array().unwrap().len(), 4);
        assert_eq!(
            value["profiles"][0]["region"],
            "r{c10[c13,c11],c11[c10,c12],c12[c11,c13],c13[c12,c10]}"
        );
        assert_eq!(lost, ["F2:side(t5.g0.c0)"]);
    }
}
