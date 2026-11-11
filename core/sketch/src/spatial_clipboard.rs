// SPDX-License-Identifier: MIT
//! Copy, Cut and Paste of 3D sketch geometry (mitcad#1661), beside the
//! planar sketch's ([`crate::clipboard`]):
//!
//! - [`copy`] makes a [`SpatialFragment`] of selected points, curves and
//!   texts of a 3D sketch: the entities with the points their curves use (a
//!   text brings the points of its frame), and the relations and dimensions
//!   that name only those or the sketch's reference curves (a reference
//!   curve that a kept relation names comes with it). Relations and
//!   dimensions that name copied geometry and other geometry of the sketch
//!   are left out and counted ([`SpatialFragment::left_out`]). On Sketch
//!   Plane and a shape's Plane-Bound relation keep the copied points they
//!   list. Points keep their three coordinates (their elevation too);
//!   attachments, points on faces and reference curves keep their sources.
//! - [`from_planar`] turns a planar sketch's fragment
//!   ([`crate::clipboard::Fragment`]) into a 3D sketch's on a frame plane
//!   through a place: its points on that plane, its arcs, circles, ellipses
//!   and texts bound to it, a coincidence of two points a 3D coincidence,
//!   its other constraints and dimensions in-plane ones of that plane, its
//!   points kept in the plane by one Plane-Bound relation, a fixed point
//!   fixed where it goes. Constraints and dimensions on the planar sketch's
//!   origin and axes, inactive ones and common spacings are left out and
//!   counted.
//! - [`SpatialEdit::paste`] adds a fragment moved by an offset with new ids.
//!   A source the target sketch cannot take (`keeps` says which) is dropped
//!   with the attachment, the point on a face or the reference curve that
//!   used it, and counted ([`SpatialPasted::dropped`]); the relations that
//!   named a dropped reference curve are left out.
//!
//! Relations, dimensions and curves are renamed to the new ids through
//! their JSON (every id of the sketch a relation names is written `p<n>` or
//! `c<n>`; a source, whose ids are another feature's, is kept as it is), so
//! that a kind of relation added later is copied as well.

use std::collections::{BTreeMap, BTreeSet};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::axes::AXIS_LENGTH;
use crate::clipboard::{FRAGMENT_VERSION, Fragment, FragmentValue, SketchReference, value_of};
use crate::ids::{EntityUid, FeatureUid};
use crate::parameters::Parameters;
use crate::spatial::{
    SpatialAxis, SpatialConstraintDef, SpatialConstraintKind, SpatialCurve, SpatialCurveKind,
    SpatialDimension, SpatialDimensionKind, SpatialDirection, SpatialPoint, SpatialSense,
    SpatialSketchDef, SpatialSource, SpatialSplineEnd, SpatialText,
};
use crate::spatial_edit::SpatialEdit;
use crate::values::ValueInput;
use crate::{ConstraintKind, ConstraintUid, DimensionKind, EntityKind, Ref, said};

/// The 3D fragment format's version: a paste refuses a newer one.
pub const SPATIAL_FRAGMENT_VERSION: u32 = 1;

/// A piece of a 3D sketch on the clipboard (see the module documentation).
/// Its ids are the source sketch's. A planar fragment has `entities`; this
/// one always writes `curves`, which tells the two apart
/// ([`SpatialFragment::is_spatial`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpatialFragment {
    pub version: u32,
    /// The sketch it was copied from (a planar fragment's: none): a source
    /// seen through another component is kept only in that sketch's
    /// component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sketch: Option<FeatureUid>,
    /// Where the copy is held, in the sketch's local coordinates: a paste
    /// at `at` moves it by `at - base`.
    pub base: [f64; 3],
    #[serde(default)]
    pub points: Vec<SpatialPoint>,
    #[serde(default)]
    pub curves: Vec<SpatialCurve>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<SpatialText>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<SpatialConstraintDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dimensions: Vec<SpatialDimension<FragmentValue>>,
    /// The relations and dimensions on copied geometry that were left out:
    /// they also name geometry that was not copied.
    #[serde(default)]
    pub left_out: usize,
}

impl SpatialFragment {
    /// Whether the JSON of a fragment is a 3D sketch's (it has `curves`
    /// and no `entities`, which a planar fragment always has).
    pub fn is_spatial(value: &Value) -> bool {
        value.get("entities").is_none() && value.get("curves").is_some()
    }

    /// The sources of its linked reference records: its attachments', its
    /// points' on faces and its reference curves'.
    pub fn sources(&self) -> Vec<&SpatialSource> {
        let curves = self.curves.iter().filter_map(|c| match &c.kind {
            SpatialCurveKind::Reference { source, .. } => Some(source),
            _ => None,
        });
        let relations = self.constraints.iter().filter_map(|c| match &c.kind {
            SpatialConstraintKind::Attach { source, .. }
            | SpatialConstraintKind::OnFace { source, .. } => Some(source),
            _ => None,
        });
        curves.chain(relations).collect()
    }
}

fn point_name(id: EntityUid) -> String {
    format!("p{}", id.0)
}

fn curve_name(id: EntityUid) -> String {
    format!("c{}", id.0)
}

fn add3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn finite(v: [f64; 3]) -> bool {
    v.iter().all(|x| x.is_finite())
}

/// The entities a relation or dimension names: its points and curves.
fn named(points: Vec<EntityUid>, curves: Vec<EntityUid>) -> Vec<EntityUid> {
    points.into_iter().chain(curves).collect()
}

/// The fragment of `items` (points `p3`, curves `c4` and texts `t5`;
/// relations and dimensions come with the geometry they name) of the 3D
/// sketch `def`, owned by `owner`. Refused when an item does not exist or
/// nothing is selected.
pub fn copy(
    def: &SpatialSketchDef,
    params: &Parameters,
    owner: FeatureUid,
    items: &[String],
) -> Result<SpatialFragment, String> {
    let mut copied: BTreeSet<EntityUid> = BTreeSet::new();
    let mut chosen_texts: BTreeSet<EntityUid> = BTreeSet::new();
    let mut base: Option<[f64; 3]> = None;
    for item in items {
        // Relations and dimensions come with their geometry.
        if item.starts_with('k') {
            continue;
        }
        if let Some(number) = item.strip_prefix('t') {
            let id = number
                .parse::<u32>()
                .map(EntityUid)
                .map_err(|_| format!("\"{item}\" is not an id like \"t5\""))?;
            let text = def
                .texts
                .iter()
                .find(|t| t.planar.id == id)
                .ok_or_else(|| {
                    said(
                        "the text does not exist",
                        format!("text {item} does not exist"),
                    )
                })?;
            chosen_texts.insert(id);
            copied.extend(text.planar.frame.iter().copied());
            if base.is_none() {
                base = Some(text.normal.place(text.planar.at, text.z));
            }
            continue;
        }
        match item.parse::<Ref>().map_err(|e| e.to_string())? {
            Ref::Point(id) => {
                let at = def.position(id).ok_or_else(|| {
                    said(
                        "the point does not exist",
                        format!("point {item} does not exist"),
                    )
                })?;
                copied.insert(id);
                if base.is_none() {
                    base = Some(at);
                }
            }
            Ref::Curve(id) => {
                let curve = def.curve(id).ok_or_else(|| {
                    said(
                        "the curve does not exist",
                        format!("curve {item} does not exist"),
                    )
                })?;
                copied.insert(id);
                let points = curve.points();
                copied.extend(points.iter().copied());
                if base.is_none() {
                    let first = curve
                        .ends()
                        .map(|[start, _]| start)
                        .or_else(|| points.first().copied());
                    base = first.and_then(|p| def.position(p));
                }
            }
        }
    }
    if copied.is_empty() && chosen_texts.is_empty() {
        return Err("nothing to copy: select points, curves or texts of the 3D sketch".to_owned());
    }
    let is_reference = |id: EntityUid| def.curve(id).is_some_and(SpatialCurve::is_reference);
    let mut brought: BTreeSet<EntityUid> = BTreeSet::new();
    let mut left_out = 0;
    // Whether a relation or a dimension naming `names` is kept: it names
    // copied geometry, and nothing else but reference curves (which come
    // with it).
    let mut keeps = |names: &[EntityUid], brought: &mut BTreeSet<EntityUid>| -> bool {
        if !names.iter().any(|e| copied.contains(e)) {
            return false;
        }
        if names.iter().all(|e| copied.contains(e) || is_reference(*e)) {
            brought.extend(names.iter().copied().filter(|e| !copied.contains(e)));
            true
        } else {
            left_out += 1;
            false
        }
    };
    let mut constraints = Vec::new();
    for c in &def.constraints {
        match &c.kind {
            // Memberships of a plane keep the copied points they list.
            SpatialConstraintKind::OnSketchPlane { points } => {
                let kept: Vec<EntityUid> = points
                    .iter()
                    .copied()
                    .filter(|p| copied.contains(p))
                    .collect();
                if !kept.is_empty() {
                    constraints.push(SpatialConstraintDef {
                        id: c.id,
                        kind: SpatialConstraintKind::OnSketchPlane { points: kept },
                    });
                }
            }
            SpatialConstraintKind::PlaneBound { points, normal } => {
                let kept: Vec<EntityUid> = points
                    .iter()
                    .copied()
                    .filter(|p| copied.contains(p))
                    .collect();
                if kept.len() >= 2 {
                    constraints.push(SpatialConstraintDef {
                        id: c.id,
                        kind: SpatialConstraintKind::PlaneBound {
                            points: kept,
                            normal: *normal,
                        },
                    });
                }
            }
            kind => {
                if keeps(&named(kind.points(), kind.curves()), &mut brought) {
                    constraints.push(c.clone());
                }
            }
        }
    }
    let mut dimensions = Vec::new();
    for d in &def.dimensions {
        if !keeps(&named(d.kind.points(), d.kind.curves()), &mut brought) {
            continue;
        }
        let value = d.value.map(|id| {
            let (expression, parameters) = value_of(params, owner, id);
            FragmentValue {
                value: params.value(id).unwrap_or(f64::NAN),
                expression,
                parameters,
            }
        });
        dimensions.push(SpatialDimension::new(d.id, d.kind.clone(), value));
    }
    let all: BTreeSet<EntityUid> = copied.union(&brought).copied().collect();
    Ok(SpatialFragment {
        version: SPATIAL_FRAGMENT_VERSION,
        sketch: Some(owner),
        base: base.unwrap_or_default(),
        points: def
            .points
            .iter()
            .filter(|p| all.contains(&p.id))
            .cloned()
            .collect(),
        curves: def
            .curves
            .iter()
            .filter(|c| all.contains(&c.id))
            .cloned()
            .collect(),
        texts: def
            .texts
            .iter()
            .filter(|t| chosen_texts.contains(&t.planar.id))
            .cloned()
            .collect(),
        constraints,
        dimensions,
        left_out,
    })
}

/// A planar sketch's fragment as a 3D sketch's (see the module
/// documentation): on the frame plane square to `normal`, its base point
/// at `at` (local coordinates).
pub fn from_planar(
    fragment: &Fragment,
    normal: SpatialAxis,
    at: [f64; 3],
) -> Result<SpatialFragment, String> {
    if fragment.version > FRAGMENT_VERSION {
        return Err(format!(
            "the copied geometry is of a newer version of MitCad (fragment version {})",
            fragment.version
        ));
    }
    if !finite(at) {
        return Err(format!("the place must be finite, got {at:?}"));
    }
    let base = fragment.base;
    let through = normal.planar(at);
    let shift = [through[0] - base[0], through[1] - base[1]];
    let elevation = at[normal.index()];
    // A planar point (x, y) on the plane, the base point at `at`.
    let place = |xy: [f64; 2]| normal.place([xy[0] + shift[0], xy[1] + shift[1]], elevation);
    let direction = SpatialDirection {
        axis: normal,
        sense: SpatialSense::Positive,
    };
    let references: BTreeMap<EntityUid, SketchReference> = fragment
        .references
        .iter()
        .map(|r| (r.id.uid(), r.is))
        .collect();
    let mut out = SpatialFragment {
        version: SPATIAL_FRAGMENT_VERSION,
        sketch: None,
        base: at,
        points: Vec::new(),
        curves: Vec::new(),
        texts: Vec::new(),
        constraints: Vec::new(),
        dimensions: Vec::new(),
        left_out: fragment.left_out,
    };
    // The origin's and the axes' end points a copied curve uses become
    // points of their own where they were.
    let mut made: BTreeSet<EntityUid> = BTreeSet::new();
    for e in &fragment.entities {
        for p in e.kind.points() {
            let xy = match references.get(&p) {
                Some(SketchReference::Origin) => [0.0, 0.0],
                Some(SketchReference::XEnd) => [AXIS_LENGTH, 0.0],
                Some(SketchReference::YEnd) => [0.0, AXIS_LENGTH],
                Some(SketchReference::X | SketchReference::Y) | None => continue,
            };
            if made.insert(p) {
                out.points.push(SpatialPoint {
                    id: p,
                    at: place(xy),
                });
            }
        }
    }
    let mut fixed: Vec<Ref> = Vec::new();
    for e in &fragment.entities {
        if e.fixed && !e.reference {
            fixed.push(e.as_ref());
        }
        let kind = match &e.kind {
            EntityKind::Point { at } => {
                out.points.push(SpatialPoint {
                    id: e.id,
                    at: place(*at),
                });
                continue;
            }
            EntityKind::Line { start, end, .. } => SpatialCurveKind::Line {
                start: *start,
                end: *end,
            },
            EntityKind::Circle { center, radius, .. } => SpatialCurveKind::Circle {
                center: *center,
                radius: *radius,
                normal: direction.unit(),
                plane: Some(direction),
            },
            EntityKind::Arc { center, start, end } => SpatialCurveKind::Arc {
                center: *center,
                start: *start,
                end: *end,
                normal: direction.unit(),
                plane: Some(direction),
            },
            EntityKind::Ellipse {
                center,
                major,
                minor_radius,
            } => SpatialCurveKind::Ellipse {
                center: *center,
                major: *major,
                minor_radius: *minor_radius,
                normal,
                frame: None,
            },
            EntityKind::EllipticalArc {
                center,
                major,
                minor_radius,
                start,
                end,
            } => SpatialCurveKind::EllipticalArc {
                center: *center,
                major: *major,
                minor_radius: *minor_radius,
                start: *start,
                end: *end,
                normal,
            },
            EntityKind::Spline {
                degree,
                control,
                weights,
                knots,
            } => SpatialCurveKind::Spline {
                degree: *degree,
                control: control.clone(),
                weights: weights.clone(),
                knots: knots.clone(),
            },
            EntityKind::FittedSpline { points, closed } => SpatialCurveKind::FittedSpline {
                points: points.clone(),
                closed: *closed,
                ends: [SpatialSplineEnd::Natural; 2],
                tangents: [None, None],
            },
        };
        // A centre line has no meaning in a 3D sketch: a construction line.
        let centerline = matches!(
            e.kind,
            EntityKind::Line {
                centerline: true,
                ..
            }
        );
        out.curves
            .push(SpatialCurve::new(e.id, kind, e.construction || centerline));
    }
    for t in &fragment.texts {
        let mut planar = t.clone();
        planar.at = normal.planar(place(t.at));
        out.texts.push(SpatialText {
            planar,
            z: elevation,
            normal,
            frame: None,
        });
    }
    let on_reference = |refs: &[Ref]| refs.iter().any(|r| references.contains_key(&r.uid()));
    for c in &fragment.constraints {
        if !c.active || on_reference(&c.kind.refs()) {
            out.left_out += 1;
            continue;
        }
        let kind = match &c.kind {
            ConstraintKind::Coincident {
                point,
                entity: Ref::Point(b),
            } => SpatialConstraintKind::Coincident { a: *point, b: *b },
            relation => SpatialConstraintKind::InPlane {
                relation: relation.clone(),
                normal,
            },
        };
        out.constraints
            .push(SpatialConstraintDef { id: c.id, kind });
    }
    for d in &fragment.dimensions {
        // A common spacing names its equal spacing by id: left out.
        if !d.active
            || matches!(d.kind, DimensionKind::Spacing { .. })
            || on_reference(&d.kind.refs())
        {
            out.left_out += 1;
            continue;
        }
        out.dimensions.push(SpatialDimension::new(
            d.id,
            SpatialDimensionKind::InPlane {
                dimension: d.kind.clone(),
                normal,
            },
            d.value.clone(),
        ));
    }
    // New relation ids after the planar ones.
    let mut next = fragment
        .constraints
        .iter()
        .map(|c| c.id.0)
        .chain(fragment.dimensions.iter().map(|d| d.id.0))
        .max()
        .unwrap_or(0);
    let mut new_id = || {
        next += 1;
        ConstraintUid(next)
    };
    // A fixed point is fixed where it goes; a fixed curve keeps its place
    // in the plane (by its points off the sketch plane).
    let position =
        |out: &SpatialFragment, id: EntityUid| out.points.iter().find(|p| p.id == id).map(|p| p.at);
    let mut fixes = Vec::new();
    for entity in fixed {
        match entity {
            Ref::Point(point) => {
                if let Some(at) = position(&out, point) {
                    fixes.push(SpatialConstraintKind::Fix { point, at });
                }
            }
            Ref::Curve(_) if normal == SpatialAxis::Z => {
                fixes.push(SpatialConstraintKind::InPlaneFixed { entity });
            }
            Ref::Curve(curve) => {
                let points = out
                    .curves
                    .iter()
                    .find(|c| c.id == curve)
                    .map(SpatialCurve::points)
                    .unwrap_or_default();
                for point in points {
                    if let Some(at) = position(&out, point) {
                        fixes.push(SpatialConstraintKind::Fix { point, at });
                    }
                }
            }
        }
    }
    for kind in fixes {
        out.constraints
            .push(SpatialConstraintDef { id: new_id(), kind });
    }
    // Its points kept in one plane, as a plane-bound tool keeps a shape's.
    if out.points.len() >= 2 {
        out.constraints.push(SpatialConstraintDef {
            id: new_id(),
            kind: SpatialConstraintKind::PlaneBound {
                points: out.points.iter().map(|p| p.id).collect(),
                normal,
            },
        });
    }
    if out.points.is_empty() && out.texts.is_empty() {
        return Err("there is nothing to paste".to_owned());
    }
    Ok(out)
}

/// What [`SpatialEdit::paste`] made.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpatialPasted {
    /// The new points, curves and texts (`p<n>`, `c<n>`, `t<n>`), in the
    /// fragment's order.
    pub made: Vec<String>,
    /// The relations and dimensions added.
    pub kept: usize,
    /// The relations and dimensions left out: the fragment's, and those
    /// that name geometry this paste lacks (a reference curve dropped).
    pub left_out: usize,
    /// The linked reference records dropped (attachments, points on faces,
    /// reference curves): their sources are not this sketch's to use.
    pub dropped: usize,
    /// Dimensions that took their value instead of their expression: a
    /// parameter it uses is not in the design.
    pub values: Vec<String>,
}

/// `item` with every id `names` has (`p3`, `c4`) renamed, through its JSON;
/// a source keeps its own (another feature's) ids.
fn renamed<T: Serialize + DeserializeOwned>(
    item: &T,
    names: &BTreeMap<String, String>,
) -> Result<T, String> {
    fn rename(value: &mut Value, names: &BTreeMap<String, String>) {
        match value {
            Value::String(text) => {
                if let Some(name) = names.get(text.as_str()) {
                    *text = name.clone();
                }
            }
            Value::Array(items) => {
                for item in items {
                    rename(item, names);
                }
            }
            Value::Object(map) => {
                for (key, item) in map.iter_mut() {
                    if key != "source" {
                        rename(item, names);
                    }
                }
            }
            _ => {}
        }
    }
    let mut value = serde_json::to_value(item).map_err(|e| e.to_string())?;
    rename(&mut value, names);
    serde_json::from_value(value).map_err(|e| e.to_string())
}

impl SpatialEdit<'_> {
    /// Adds a fragment moved by `by` (see the module documentation): its
    /// points, curves and texts with new ids, its relations and dimensions
    /// on them. A source `keeps` refuses is dropped with what used it; a
    /// reference curve of a source the sketch has a record of already is
    /// that record.
    pub fn paste(
        &mut self,
        fragment: &SpatialFragment,
        by: [f64; 3],
        keeps: &dyn Fn(&SpatialSource) -> bool,
    ) -> Result<SpatialPasted, String> {
        if fragment.version > SPATIAL_FRAGMENT_VERSION {
            return Err(format!(
                "the copied geometry is of a newer version of MitCad (fragment version {})",
                fragment.version
            ));
        }
        if !finite(by) {
            return Err(format!("the place must be finite, got {by:?}"));
        }
        if fragment.points.is_empty() && fragment.curves.is_empty() && fragment.texts.is_empty() {
            return Err("there is nothing to paste".to_owned());
        }
        // The source's ids: whether each is a point.
        let mut known: BTreeMap<EntityUid, bool> = BTreeMap::new();
        for p in &fragment.points {
            if known.insert(p.id, true).is_some() || !finite(p.at) {
                return Err(said(
                    "the copied geometry cannot be read",
                    format!(
                        "{} is in the copied geometry twice or not at a finite place",
                        point_name(p.id)
                    ),
                ));
            }
        }
        for c in &fragment.curves {
            if known.insert(c.id, false).is_some() {
                return Err(said(
                    "the copied geometry cannot be read",
                    format!("{} is in the copied geometry twice", curve_name(c.id)),
                ));
            }
            for p in c.points() {
                if known.get(&p) != Some(&true) {
                    return Err(said(
                        "the copied geometry cannot be read",
                        format!(
                            "{} uses {}, which the copied geometry lacks",
                            curve_name(c.id),
                            point_name(p)
                        ),
                    ));
                }
            }
        }
        let mut out = SpatialPasted {
            left_out: fragment.left_out,
            ..SpatialPasted::default()
        };
        let mut map: BTreeMap<EntityUid, EntityUid> = BTreeMap::new();
        let mut names: BTreeMap<String, String> = BTreeMap::new();
        for p in &fragment.points {
            let id = self.add_point(add3(p.at, by))?;
            map.insert(p.id, id);
            names.insert(point_name(p.id), point_name(id));
            out.made.push(point_name(id));
        }
        for c in &fragment.curves {
            if let SpatialCurveKind::Reference { source, projected } = &c.kind {
                if !keeps(source) {
                    out.dropped += 1;
                    continue;
                }
                // One record per source and mode (mitcad#1543).
                if let Some(id) = self.def.reference_of(source, *projected) {
                    map.insert(c.id, id);
                    names.insert(curve_name(c.id), curve_name(id));
                    continue;
                }
            }
            let kind = renamed(&c.kind, &names)?;
            let id = self.add_curve(kind, c.construction)?;
            map.insert(c.id, id);
            names.insert(curve_name(c.id), curve_name(id));
            out.made.push(curve_name(id));
        }
        for t in &fragment.texts {
            let mut text = t.clone();
            let id = self.def.next_entity();
            text.planar.id = id;
            let shift = t.normal.planar(by);
            text.planar.at = [t.planar.at[0] + shift[0], t.planar.at[1] + shift[1]];
            text.z += by[t.normal.index()];
            for p in &mut text.planar.frame {
                *p = *map.get(p).ok_or_else(|| {
                    said(
                        "the copied geometry cannot be read",
                        format!("text t{} has a frame point it lacks", t.planar.id.0),
                    )
                })?;
            }
            if let Some(curve) = text.planar.path.as_ref().map(|path| path.curve) {
                match map.get(&curve) {
                    Some(made) => {
                        if let Some(path) = &mut text.planar.path {
                            path.curve = *made;
                        }
                    }
                    None => text.planar.path = None,
                }
            }
            self.def.texts.push(text);
            self.report.texts.push(id);
            out.made.push(format!("t{}", id.0));
        }
        let mapped = |ids: &[EntityUid]| ids.iter().all(|e| map.contains_key(e));
        let mut on_plane: Vec<EntityUid> = Vec::new();
        for c in &fragment.constraints {
            let source = match &c.kind {
                SpatialConstraintKind::Attach { source, .. }
                | SpatialConstraintKind::OnFace { source, .. } => Some(source),
                _ => None,
            };
            if source.is_some_and(|s| !keeps(s)) {
                out.dropped += 1;
                continue;
            }
            if !mapped(&named(c.kind.points(), c.kind.curves())) {
                out.left_out += 1;
                continue;
            }
            let mut kind = renamed(&c.kind, &names)?;
            match &mut kind {
                // Fixed where it is pasted.
                SpatialConstraintKind::Fix { at, .. } => *at = add3(*at, by),
                // One On Sketch Plane relation per sketch.
                SpatialConstraintKind::OnSketchPlane { points } => {
                    on_plane.append(points);
                    out.kept += 1;
                    continue;
                }
                _ => {}
            }
            self.add_constraint(kind)?;
            out.kept += 1;
        }
        if !on_plane.is_empty() {
            let existing = self
                .def
                .constraints
                .iter_mut()
                .find_map(|c| match &mut c.kind {
                    SpatialConstraintKind::OnSketchPlane { points } => Some(points),
                    _ => None,
                });
            match existing {
                Some(points) => points.extend(on_plane),
                None => {
                    self.add_constraint(SpatialConstraintKind::OnSketchPlane { points: on_plane })?;
                }
            }
        }
        for d in &fragment.dimensions {
            if !mapped(&named(d.kind.points(), d.kind.curves())) {
                out.left_out += 1;
                continue;
            }
            let kind = renamed(&d.kind, &names)?;
            match &d.value {
                None => {
                    self.add_dimension(kind, None, true)?;
                }
                Some(value) => {
                    self.paste_value(kind, value, d.id, &mut out.values)?;
                }
            }
            out.kept += 1;
        }
        Ok(out)
    }

    /// A pasted driving dimension: with its expression where the design
    /// has every parameter it uses, else with its value (`source` names it
    /// in `values` then).
    fn paste_value(
        &mut self,
        kind: SpatialDimensionKind,
        value: &FragmentValue,
        source: ConstraintUid,
        values: &mut Vec<String>,
    ) -> Result<ConstraintUid, String> {
        if let Some(expression) = &value.expression {
            if value
                .parameters
                .iter()
                .all(|p| self.params().find(p).is_some())
                && let Ok(id) = self.add_dimension(
                    kind.clone(),
                    Some(&ValueInput::Name(expression.clone())),
                    false,
                )
            {
                return Ok(id);
            }
            values.push(source.to_string());
        }
        if !value.value.is_finite() {
            return Err(said(
                "a dimension of the copied geometry has no value",
                format!("dimension {source} has no value"),
            ));
        }
        self.add_dimension(kind, Some(&ValueInput::Number(value.value)), false)
    }
}
