// SPDX-License-Identifier: MIT
//! Sketch: entities, constraints and dimensions on a plane (see
//! [`crate::sketch`] and `docs/architecture.md`). Evaluation finds the
//! plane's frame, follows linked projections, solves from the stored
//! positions with the dimension parameters' values and cuts the profile
//! regions, which extrudes and other profile features use by key.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{CheckContext, Evaluate, FeatureInfo, FeatureOutput, References};
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::kernel::Kernel;
use crate::kernel::KernelError;
use crate::parameters::ParamId;
use crate::profile::{ProfileRegion, SketchFrame, dot};
use crate::sketch::geometry::Curve2;
use crate::sketch::legacy::{LegacyShape, convert};
use crate::sketch::offset::{self, SketchOffset, derived_entities};
use crate::sketch::pattern::{self, Binding, SketchPattern};
use crate::sketch::regions::{Region, RegionCurve, best_match, regions};
use crate::sketch::solve::{
    DimensionInput, Extra, Goal, SketchStatus, SolveError, Solved, drag_extra, solve_extra,
};
use crate::sketch::text::{self, TextOutline};
use crate::sketch::{
    Constraint, ConstraintKind, ConstraintUid, Dimension, DimensionKind, Entity, EntityIndex,
    EntityKind, Projection, Ref, SketchText, ValueRange,
};
use crate::topo::{FaceName, RegionKey};

use super::construction::datum_kind;
use super::geom_ref::GeomRef;
use crate::datum::{Datum, DatumKind, DatumPlane, OriginDatum};

/// The plane a sketch lies on. Origin planes have the frames of the origin
/// datums ([`OriginDatum::datum`]): XY x = +X, y = +Y (normal +Z); XZ
/// x = +X, y = −Z (normal +Y); YZ x = −Z, y = +Y (normal +X, awaiting
/// experiment K1). A construction plane (written as its feature, `"F5"`)
/// gives its datum's frame. A planar face is found by name in the bodies
/// before the sketch; its frame is every planar face's
/// ([`DatumPlane::on_plane`]): the face's outward normal, the origin's
/// projection as origin and the x axis along the projection of +X (of +Y
/// when the face faces ±X), so a face facing +Z gets the XY frame and one
/// facing +Y the XZ frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum SketchPlane {
    #[default]
    Xy,
    Xz,
    Yz,
    /// A construction plane feature before the sketch.
    Construction(FeatureUid),
    /// A planar face, written `{"face": "F2:end(r{c1})", "body": "F2.b0"}`;
    /// without `body`, every body is searched.
    Face {
        face: Box<FaceName>,
        body: Option<BodyUid>,
    },
}

impl SketchPlane {
    /// The frame of an origin plane.
    pub fn origin_frame(&self) -> Option<SketchFrame> {
        let origin = match self {
            Self::Xy => OriginDatum::Xy,
            Self::Xz => OriginDatum::Xz,
            Self::Yz => OriginDatum::Yz,
            Self::Construction(_) | Self::Face { .. } => return None,
        };
        match origin.datum() {
            Datum::Plane(plane) => Some(plane.frame()),
            _ => None,
        }
    }

    /// The frame of a plane through `point` with the outward `normal`, as
    /// for every planar face ([`DatumPlane::on_plane`]).
    pub fn face_frame(point: [f64; 3], normal: [f64; 3]) -> SketchFrame {
        DatumPlane::on_plane(point, normalize(normal)).frame()
    }
}

fn normalize(v: [f64; 3]) -> [f64; 3] {
    let n = dot(v, v).sqrt();
    if n > 0.0 { v.map(|x| x / n) } else { v }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FacePlane {
    face: FaceName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    body: Option<BodyUid>,
}

impl Serialize for SketchPlane {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Xy => serializer.serialize_str("xy"),
            Self::Xz => serializer.serialize_str("xz"),
            Self::Yz => serializer.serialize_str("yz"),
            Self::Construction(uid) => serializer.collect_str(uid),
            Self::Face { face, body } => FacePlane {
                face: (**face).clone(),
                body: *body,
            }
            .serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for SketchPlane {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match serde_json::Value::deserialize(deserializer)? {
            serde_json::Value::String(name) => match name.as_str() {
                "xy" => Ok(Self::Xy),
                "xz" => Ok(Self::Xz),
                "yz" => Ok(Self::Yz),
                other => other.parse().map(Self::Construction).map_err(|_| {
                    D::Error::custom(format!(
                        "unknown plane '{other}', expected \"xy\", \"xz\", \"yz\", a \
                         construction plane like \"F5\" or {{\"face\": ...}}"
                    ))
                }),
            },
            value @ serde_json::Value::Object(_) => {
                let plane: FacePlane = serde_json::from_value(value).map_err(D::Error::custom)?;
                Ok(Self::Face {
                    face: Box::new(plane.face),
                    body: plane.body,
                })
            }
            _ => Err(D::Error::custom(
                "a plane is \"xy\", \"xz\", \"yz\", a construction plane like \"F5\" or \
                 {\"face\": ...}",
            )),
        }
    }
}

/// The sketch's own frame in its plane's frame (imports keep the file's
/// exact sketch frame this way): the origin and unit axes in the plane
/// frame's coordinates (x, y, normal).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameDef {
    pub origin: [f64; 3],
    pub x_axis: [f64; 3],
    pub y_axis: [f64; 3],
}

impl FrameDef {
    /// This frame placed in `plane`.
    pub fn apply(&self, plane: &SketchFrame) -> SketchFrame {
        let n = plane.normal();
        let place = |v: [f64; 3]| -> [f64; 3] {
            std::array::from_fn(|i| v[0] * plane.x_axis[i] + v[1] * plane.y_axis[i] + v[2] * n[i])
        };
        let o = place(self.origin);
        SketchFrame {
            origin: std::array::from_fn(|i| plane.origin[i] + o[i]),
            x_axis: normalize(place(self.x_axis)),
            y_axis: normalize(place(self.y_axis)),
        }
    }

    fn check(&self) -> Result<(), String> {
        let finite = self
            .origin
            .iter()
            .chain(&self.x_axis)
            .chain(&self.y_axis)
            .all(|v| v.is_finite());
        let (x, y) = (self.x_axis, self.y_axis);
        let unit = |v: [f64; 3]| (dot(v, v).sqrt() - 1.0).abs() < 1e-6;
        if finite && unit(x) && unit(y) && dot(x, y).abs() < 1e-6 {
            Ok(())
        } else {
            Err("frame: the axes must be unit vectors at right angles".to_owned())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SketchDef<P = ParamId> {
    pub plane: SketchPlane,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame: Option<Box<FrameDef>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<Entity>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<Constraint>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dimensions: Vec<Dimension<P>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub projections: Vec<Projection>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<SketchText>,
    /// Circular and rectangular patterns (copy groups, `pattern.rs`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub patterns: Vec<SketchPattern<P>>,
    /// Offsets with their distances (`offset.rs`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub offsets: Vec<SketchOffset<P>>,
}

impl<P> Default for SketchDef<P> {
    fn default() -> Self {
        Self {
            plane: SketchPlane::Xy,
            frame: None,
            entities: Vec::new(),
            constraints: Vec::new(),
            dimensions: Vec::new(),
            projections: Vec::new(),
            texts: Vec::new(),
            patterns: Vec::new(),
            offsets: Vec::new(),
        }
    }
}

/// The file and command form; `shapes` is the transitional form of early
/// version 2 files, converted on reading.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SketchDefIn<P> {
    #[serde(default)]
    plane: SketchPlane,
    #[serde(default)]
    frame: Option<Box<FrameDef>>,
    #[serde(default)]
    entities: Vec<Entity>,
    #[serde(default)]
    constraints: Vec<Constraint>,
    #[serde(default = "Vec::new")]
    dimensions: Vec<Dimension<P>>,
    #[serde(default)]
    projections: Vec<Projection>,
    #[serde(default)]
    texts: Vec<SketchText>,
    #[serde(default = "Vec::new")]
    patterns: Vec<SketchPattern<P>>,
    #[serde(default = "Vec::new")]
    offsets: Vec<SketchOffset<P>>,
    #[serde(default = "Vec::new")]
    shapes: Vec<LegacyShape<P>>,
}

impl<'de, P: Deserialize<'de>> Deserialize<'de> for SketchDef<P> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let def = SketchDefIn::<P>::deserialize(deserializer)?;
        let mut sketch = SketchDef {
            plane: def.plane,
            frame: def.frame,
            entities: def.entities,
            constraints: def.constraints,
            dimensions: def.dimensions,
            projections: def.projections,
            texts: def.texts,
            patterns: def.patterns,
            offsets: def.offsets,
        };
        if !def.shapes.is_empty() {
            if !sketch.entities.is_empty() || !sketch.dimensions.is_empty() {
                return Err(D::Error::custom(
                    "a sketch has \"shapes\" (older files) or \"entities\", not both",
                ));
            }
            let converted = convert(def.shapes, &|_| None);
            sketch.entities = converted.entities;
            sketch.constraints = converted.constraints;
            sketch.dimensions = converted.dimensions;
        }
        Ok(sketch)
    }
}

/// Evaluated sketch: its frame, solved geometry and profile regions.
#[derive(Debug, Clone, PartialEq)]
pub struct SketchOutput {
    pub frame: SketchFrame,
    pub regions: Vec<ProfileRegion>,
    /// Areas, centroids and outlines of the regions, in the same order.
    pub region_info: Vec<Region>,
    pub solved: Arc<Solved>,
    /// The texts' glyph outlines.
    pub texts: Arc<Vec<TextOutline>>,
}

impl SketchOutput {
    pub fn region(&self, key: &RegionKey) -> Option<&ProfileRegion> {
        self.regions.iter().find(|r| &r.key == key)
    }

    /// The region a reference means: the one with its key, else the one
    /// with the most curves in common (the sketch changed), carrying the
    /// referenced key so that the faces made from it keep their names.
    /// None when no region shares a curve with it.
    pub fn resolve_region(&self, key: &RegionKey) -> Option<ProfileRegion> {
        if let Some(region) = self.region(key) {
            return Some(region.clone());
        }
        best_match(&self.region_info, key).map(|region| ProfileRegion {
            key: key.clone(),
            loops: region.profile.loops.clone(),
        })
    }

    pub fn status(&self) -> &SketchStatus {
        &self.solved.status
    }
}

impl<P> SketchDef<P> {
    pub const TYPE: &'static str = "sketch";
    pub const BASE_NAME: &'static str = "Sketch";

    pub fn new(plane: SketchPlane) -> Self {
        Self {
            plane,
            ..Self::default()
        }
    }

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<SketchDef<Q>, E> {
        Ok(SketchDef {
            plane: self.plane.clone(),
            frame: self.frame.clone(),
            entities: self.entities.clone(),
            constraints: self.constraints.clone(),
            dimensions: self
                .dimensions
                .iter()
                .map(|d| d.map_value(f))
                .collect::<Result<_, _>>()?,
            projections: self.projections.clone(),
            texts: self.texts.clone(),
            patterns: self
                .patterns
                .iter()
                .map(|p| p.map_value(f))
                .collect::<Result<_, _>>()?,
            offsets: self
                .offsets
                .iter()
                .map(|o| o.map_value(f))
                .collect::<Result<_, _>>()?,
        })
    }

    pub fn entity(&self, id: EntityUid) -> Option<&Entity> {
        self.entities.iter().find(|e| e.id == id)
    }

    pub fn entity_mut(&mut self, id: EntityUid) -> Option<&mut Entity> {
        self.entities.iter_mut().find(|e| e.id == id)
    }

    /// The first entity id not used yet (points, curves and texts share
    /// the numbers).
    pub fn next_entity(&self) -> EntityUid {
        let used = self
            .entities
            .iter()
            .map(|e| e.id.0)
            .chain(self.texts.iter().map(|t| t.id.0));
        EntityUid(used.max().map_or(1, |n| n + 1))
    }

    /// The first constraint id not used yet (constraints, dimensions,
    /// patterns and offsets share the numbers).
    pub fn next_constraint(&self) -> ConstraintUid {
        let used = self
            .constraints
            .iter()
            .map(|c| c.id.0)
            .chain(self.dimensions.iter().map(|d| d.id.0))
            .chain(self.patterns.iter().map(|p| p.id.0))
            .chain(self.offsets.iter().map(|o| o.id.0));
        ConstraintUid(used.max().map_or(1, |n| n + 1))
    }

    /// The curves that bound profiles: not construction.
    pub fn profile_curves(&self) -> impl Iterator<Item = &Entity> {
        self.entities
            .iter()
            .filter(|e| !e.is_point() && !e.construction)
    }

    /// The frame from the plane's frame and the stored override.
    pub fn place(&self, plane: &SketchFrame) -> SketchFrame {
        match &self.frame {
            Some(frame) => frame.apply(plane),
            None => *plane,
        }
    }
}

impl SketchDef {
    /// The solver inputs for driving dimensions with `value`'s values
    /// (checked against their ranges), driven ones without.
    pub fn dimension_inputs(
        &self,
        value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
        name: &dyn Fn(ParamId) -> String,
    ) -> Result<Vec<DimensionInput<'_>>, String> {
        self.dimensions
            .iter()
            .map(|d| {
                let v = match d.value {
                    Some(id) => {
                        let v = value(id)?;
                        check_range(d.kind.range(), v).map_err(|e| format!("{} {e}", name(id)))?;
                        Some(v)
                    }
                    None => None,
                };
                Ok(DimensionInput {
                    id: d.id,
                    kind: &d.kind,
                    value: v,
                })
            })
            .collect()
    }

    /// The solver bindings of the patterns' copies with `value`'s values
    /// (checked against their ranges).
    pub fn pattern_bindings(
        &self,
        value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
        name: &dyn Fn(ParamId) -> String,
    ) -> Result<Vec<Binding>, String> {
        let index = EntityIndex::new(&self.entities);
        let mut out = Vec::new();
        for pattern in &self.patterns {
            let mut values = Vec::new();
            for (slot, id) in pattern.slots().iter().zip(pattern.values()) {
                let v = value(*id)?;
                pattern::check_value(slot, v).map_err(|e| format!("{} {e}", name(*id)))?;
                values.push(v);
            }
            out.extend(pattern.bindings(&values, &index));
        }
        Ok(out)
    }

    /// Solves with the given dimension, pattern and offset values (mm,
    /// rad): the solver moves everything but the curves of derived offsets,
    /// which are then computed from the solution.
    pub fn solve_with(
        &self,
        value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
        name: &dyn Fn(ParamId) -> String,
    ) -> Result<Solved, SolveError> {
        self.solve_goals(value, name, &[])
    }

    /// [`SketchDef::solve_with`], then pulls the goals as far as the
    /// constraints allow (a drag) when there are any.
    pub fn solve_goals(
        &self,
        value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
        name: &dyn Fn(ParamId) -> String,
        goals: &[Goal],
    ) -> Result<Solved, SolveError> {
        let inputs = self
            .dimension_inputs(value, name)
            .map_err(SolveError::new)?;
        let bindings = self
            .pattern_bindings(value, name)
            .map_err(SolveError::new)?;
        let index = EntityIndex::new(&self.entities);
        let skip = derived_entities(&self.offsets, &index);
        self.check_derived(&skip).map_err(SolveError::new)?;
        let extra = Extra {
            bindings: &bindings,
            skip: &skip,
        };
        let mut solved = if goals.is_empty() {
            solve_extra(&self.entities, &self.constraints, &inputs, &extra)?
        } else {
            drag_extra(&self.entities, &self.constraints, &inputs, &extra, goals)?
        };
        for o in &self.offsets {
            let v = value(o.distance).map_err(SolveError::new)?;
            if !(v.is_finite() && v > 0.0) {
                return Err(SolveError::new(format!(
                    "{} must be greater than zero, got {v}",
                    name(o.distance)
                )));
            }
        }
        offset::regenerate(&self.offsets, &self.entities, &mut solved, value)
            .map_err(SolveError::new)?;
        // Derived curves are as determined as their sources.
        for o in self.offsets.iter().filter(|o| o.derived) {
            if o.curves
                .iter()
                .all(|c| solved.status.fully_constrained.contains(c))
            {
                let made = derived_entities(std::slice::from_ref(o), &index);
                solved.status.fully_constrained.extend(made);
            }
        }
        Ok(solved)
    }

    /// The profile regions of solved geometry and laid out texts.
    pub fn regions_of(&self, solved: &Solved, texts: &[TextOutline]) -> Vec<Region> {
        let mut curves: Vec<RegionCurve> = self
            .profile_curves()
            .filter_map(|e| Some(RegionCurve::new(e.id, solved.curves.get(&e.id)?.clone())))
            .collect();
        for t in texts {
            curves.extend(t.region_curves());
        }
        regions(&curves)
    }

    /// The texts laid out with the kernel's fonts. Without font support
    /// (a test kernel) the texts have no outlines.
    pub fn text_outlines<K: Kernel + ?Sized>(
        &self,
        solved: &Solved,
        kernel: &K,
    ) -> Result<Vec<TextOutline>, String> {
        let mut out = Vec::new();
        for t in &self.texts {
            let glyphs = match kernel.font_glyphs(&text::request(t)) {
                Ok(glyphs) => glyphs,
                Err(KernelError::Unsupported(_)) => return Ok(Vec::new()),
                Err(e) => return Err(format!("text t{}: {e}", t.id.0)),
            };
            out.push(text::layout(t, &glyphs, &solved.points, &solved.curves)?);
        }
        Ok(out)
    }
}

fn check_range(range: ValueRange, v: f64) -> Result<(), String> {
    let ok = match range {
        ValueRange::Positive => v.is_finite() && v > 0.0,
        ValueRange::NotNegative => v.is_finite() && v >= 0.0,
        ValueRange::Angle => (0.0..=std::f64::consts::PI).contains(&v),
    };
    if ok {
        Ok(())
    } else {
        Err(match range {
            ValueRange::Positive => format!("must be greater than zero, got {v}"),
            ValueRange::NotNegative => format!("must not be negative, got {v}"),
            ValueRange::Angle => format!(
                "must be between 0 and 180 degrees, got {} deg",
                v.to_degrees()
            ),
        })
    }
}

/// What kinds of entities a constraint or dimension accepts.
fn check_kinds(index: &EntityIndex<'_>, what: &str, refs: &[(Ref, &[&str])]) -> Result<(), String> {
    for (r, kinds) in refs {
        let entity = index
            .get(r.uid())
            .ok_or_else(|| format!("{what}: {r} does not exist"))?;
        if entity.is_point() != r.is_point() {
            return Err(format!("{what}: {r} is a {}", entity.kind.type_name()));
        }
        if !kinds.is_empty() && !kinds.contains(&entity.kind.type_name()) {
            return Err(format!(
                "{what}: {r} is a {}, expected {}",
                entity.kind.type_name(),
                kinds.join(" or ")
            ));
        }
    }
    Ok(())
}

const LINE: &[&str] = &["line"];
const ROUND: &[&str] = &["circle", "arc"];
const ANY: &[&str] = &[];

impl<P> SketchDef<P> {
    /// Derived offset curves are computed, not solved: nothing may
    /// constrain or pattern them.
    fn check_derived(&self, derived: &BTreeSet<EntityUid>) -> Result<(), String> {
        if derived.is_empty() {
            return Ok(());
        }
        let refs = self
            .constraints
            .iter()
            .map(|c| (c.id, c.kind.refs()))
            .chain(self.dimensions.iter().map(|d| (d.id, d.kind.refs())));
        for (id, refs) in refs {
            if let Some(r) = refs.iter().find(|r| derived.contains(&r.uid())) {
                return Err(format!(
                    "{id}: {r} follows an offset of a spline or an ellipse and cannot be \
                     constrained; constrain its source"
                ));
            }
        }
        for p in &self.patterns {
            if let Some(r) = p.entities.iter().find(|r| derived.contains(&r.uid())) {
                return Err(format!(
                    "{}: {r} follows an offset and cannot be patterned",
                    p.id
                ));
            }
        }
        Ok(())
    }

    /// The consistency of entities, constraints and dimensions.
    pub fn check_definition(&self) -> Result<(), String> {
        let mut ids = BTreeSet::new();
        for e in &self.entities {
            if e.id.0 == 0 || !ids.insert(e.id) {
                return Err(format!(
                    "entity {} is used more than once or is 0",
                    e.as_ref()
                ));
            }
        }
        for t in &self.texts {
            if t.id.0 == 0 || !ids.insert(t.id) {
                return Err(format!("text t{} is used more than once or is 0", t.id.0));
            }
        }
        let index = EntityIndex::new(&self.entities);
        for t in &self.texts {
            text::check(t, &index)?;
        }
        let point = |what: &str, id: EntityUid| -> Result<(), String> {
            match index.get(id) {
                Some(e) if e.is_point() => Ok(()),
                Some(e) => Err(format!("{what}: p{} is a {}", id.0, e.kind.type_name())),
                None => Err(format!("{what}: point p{} does not exist", id.0)),
            }
        };
        for e in &self.entities {
            let what = e.as_ref().to_string();
            for p in e.kind.points() {
                point(&what, p)?;
            }
            let finite = |v: f64| v.is_finite();
            match &e.kind {
                EntityKind::Point { at } => {
                    if !at.iter().all(|v| v.is_finite()) {
                        return Err(format!("{what} must be a finite point, got {at:?}"));
                    }
                }
                EntityKind::Line { start, end, .. } if start == end => {
                    return Err(format!("{what}: a line needs two points"));
                }
                EntityKind::Circle { radius, .. } if !(finite(*radius) && *radius > 0.0) => {
                    return Err(format!("{what}: the radius must be positive"));
                }
                EntityKind::Arc { center, start, end }
                    if center == start || center == end || start == end =>
                {
                    return Err(format!("{what}: an arc needs three points"));
                }
                EntityKind::Ellipse { minor_radius, .. }
                | EntityKind::EllipticalArc { minor_radius, .. }
                    if !(finite(*minor_radius) && *minor_radius > 0.0) =>
                {
                    return Err(format!("{what}: the minor radius must be positive"));
                }
                EntityKind::Spline {
                    degree,
                    control,
                    weights,
                    knots,
                } => {
                    let (p, n) = (*degree as usize, control.len());
                    if p == 0 || n < p + 1 {
                        return Err(format!(
                            "{what}: a spline of degree {p} needs at least {} control points",
                            p + 1
                        ));
                    }
                    if !weights.is_empty()
                        && (weights.len() != n
                            || weights.iter().any(|w| !(w.is_finite() && *w > 0.0)))
                    {
                        return Err(format!("{what}: one positive weight per control point"));
                    }
                    if !knots.is_empty()
                        && (knots.len() != n + p + 1 || knots.windows(2).any(|w| w[1] < w[0]))
                    {
                        return Err(format!("{what}: {} non-decreasing knots", n + p + 1));
                    }
                }
                EntityKind::FittedSpline { points } if points.len() < 2 => {
                    return Err(format!("{what}: a spline needs two fit points"));
                }
                _ => {}
            }
            if let EntityKind::Line {
                centerline: true, ..
            } = e.kind
                && e.construction
            {
                return Err(format!(
                    "{what}: a centre line is not construction geometry"
                ));
            }
        }
        let mut constraint_ids = BTreeSet::new();
        for c in &self.constraints {
            if c.id.0 == 0 || !constraint_ids.insert(c.id) {
                return Err(format!(
                    "constraint {} is used more than once or is k0",
                    c.id
                ));
            }
            let what = c.id.to_string();
            use ConstraintKind as K;
            let p = Ref::Point;
            let cv = Ref::Curve;
            match &c.kind {
                K::Coincident { point, entity } => {
                    check_kinds(&index, &what, &[(p(*point), ANY), (*entity, ANY)])?;
                    if *entity == p(*point) {
                        return Err(format!("{what}: a point is coincident with itself"));
                    }
                }
                K::Horizontal { line } | K::Vertical { line } => {
                    check_kinds(&index, &what, &[(cv(*line), LINE)])?;
                }
                K::HorizontalPoints { a, b } | K::VerticalPoints { a, b } => {
                    check_kinds(&index, &what, &[(p(*a), ANY), (p(*b), ANY)])?;
                }
                K::Parallel { a, b } | K::Perpendicular { a, b } | K::Collinear { a, b } => {
                    check_kinds(&index, &what, &[(cv(*a), LINE), (cv(*b), ANY)])?;
                }
                K::Tangent { a, b } | K::Smooth { a, b } | K::Equal { a, b } => {
                    check_kinds(&index, &what, &[(cv(*a), ANY), (cv(*b), ANY)])?;
                }
                K::Concentric { a, b } => {
                    let round = &["circle", "arc", "ellipse", "elliptical_arc"][..];
                    check_kinds(&index, &what, &[(cv(*a), round), (cv(*b), round)])?;
                }
                K::Midpoint { point, curve } => {
                    check_kinds(
                        &index,
                        &what,
                        &[(p(*point), ANY), (cv(*curve), &["line", "arc"])],
                    )?;
                }
                K::Symmetric { a, b, axis } => {
                    check_kinds(&index, &what, &[(*a, ANY), (*b, ANY), (cv(*axis), LINE)])?;
                    if a.is_point() != b.is_point() {
                        return Err(format!("{what}: two points or two curves"));
                    }
                }
            }
            if c.kind.refs().windows(2).any(|w| w[0] == w[1]) {
                return Err(format!("{what}: an entity is constrained to itself"));
            }
        }
        for d in &self.dimensions {
            if d.id.0 == 0 || !constraint_ids.insert(d.id) {
                return Err(format!(
                    "dimension {} is used more than once or is k0",
                    d.id
                ));
            }
            let what = d.id.to_string();
            use DimensionKind as D;
            let p = Ref::Point;
            let cv = Ref::Curve;
            let refs: Vec<(Ref, &[&str])> = match &d.kind {
                D::Distance { a, b }
                | D::HorizontalDistance { a, b }
                | D::VerticalDistance { a, b } => {
                    if a == b {
                        return Err(format!("{what}: two different points"));
                    }
                    vec![(p(*a), ANY), (p(*b), ANY)]
                }
                D::PointLineDistance { point, line } => vec![(p(*point), ANY), (cv(*line), LINE)],
                D::LineDistance { a, b } | D::Angle { a, b } => {
                    if a == b {
                        return Err(format!("{what}: two different lines"));
                    }
                    vec![(cv(*a), LINE), (cv(*b), LINE)]
                }
                D::Length { line } => vec![(cv(*line), LINE)],
                D::Radius { curve } | D::Diameter { curve } => vec![(cv(*curve), ROUND)],
                D::ArcLength { arc } => vec![(cv(*arc), &["arc"][..])],
                D::MajorRadius { ellipse } | D::MinorRadius { ellipse } => {
                    vec![(cv(*ellipse), &["ellipse", "elliptical_arc"][..])]
                }
                D::LinearDiameter { axis, entity } => {
                    let other: &[&str] = if entity.is_point() { ANY } else { LINE };
                    vec![(cv(*axis), LINE), (*entity, other)]
                }
            };
            check_kinds(&index, &what, &refs)?;
            if let Some(text) = d.text
                && !text.iter().all(|v| v.is_finite())
            {
                return Err(format!("{what}: the text position must be finite"));
            }
        }
        for (i, projection) in self.projections.iter().enumerate() {
            if projection.entities.is_empty() {
                return Err(format!("projections[{i}]: no entities"));
            }
            for r in &projection.entities {
                check_kinds(&index, &format!("projections[{i}]"), &[(*r, ANY)])?;
            }
        }
        for p in &self.patterns {
            if p.id.0 == 0 || !constraint_ids.insert(p.id) {
                return Err(format!("pattern {} is used more than once or is k0", p.id));
            }
            p.check(&index)?;
        }
        for o in &self.offsets {
            if o.id.0 == 0 || !constraint_ids.insert(o.id) {
                return Err(format!("offset {} is used more than once or is k0", o.id));
            }
            o.check(&index)?;
        }
        self.check_derived(&derived_entities(&self.offsets, &index))?;
        if let Some(frame) = &self.frame {
            frame.check()?;
        }
        Ok(())
    }
}

impl FeatureInfo for SketchDef {
    fn references(&self) -> References {
        let mut references = References::default();
        match &self.plane {
            SketchPlane::Face { face, body } => {
                references.features.extend(face.features());
                if let Some(body) = body {
                    references.body(*body);
                }
            }
            SketchPlane::Construction(uid) => {
                references.features.insert(*uid);
            }
            SketchPlane::Xy | SketchPlane::Xz | SketchPlane::Yz => {}
        }
        for projection in &self.projections {
            references
                .features
                .extend(topo_features(&projection.source));
            if let Some(body) = projection.body {
                references.body(body);
            }
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        self.check_definition()?;
        match &self.plane {
            SketchPlane::Face { face, body } => {
                for feature in face.features() {
                    ctx.feature(feature)
                        .map_err(|e| format!("plane {face}: {e}"))?;
                }
                if let Some(body) = body {
                    ctx.body(*body)?;
                }
            }
            SketchPlane::Construction(uid) => {
                let entry = ctx.feature(*uid)?;
                if datum_kind(&entry.def) != Some(DatumKind::Plane) {
                    return Err(format!(
                        "{} ({uid}) is not a construction plane",
                        entry.name
                    ));
                }
            }
            SketchPlane::Xy | SketchPlane::Xz | SketchPlane::Yz => {}
        }
        for projection in &self.projections {
            for feature in topo_features(&projection.source) {
                ctx.feature(feature)
                    .map_err(|e| format!("projection of {}: {e}", projection.source))?;
            }
            if let Some(body) = projection.body {
                ctx.body(body)?;
            }
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        for d in &self.dimensions {
            if let Some(id) = d.value {
                check_range(d.kind.range(), value(id))
                    .map_err(|e| format!("{} {e}", d.kind.type_name()))?;
            }
        }
        for p in &self.patterns {
            for (slot, id) in p.slots().iter().zip(p.values()) {
                pattern::check_value(slot, value(*id))
                    .map_err(|e| format!("{} {e}", p.type_name()))?;
            }
        }
        for o in &self.offsets {
            check_range(ValueRange::Positive, value(o.distance))
                .map_err(|e| format!("offset distance {e}"))?;
        }
        Ok(())
    }
}

pub(crate) fn topo_features(name: &crate::topo::TopoName) -> BTreeSet<FeatureUid> {
    match name {
        crate::topo::TopoName::Face(f) => f.features(),
        crate::topo::TopoName::Edge(e) => e.features(),
        crate::topo::TopoName::Vertex(v) => v.faces().iter().flat_map(FaceName::features).collect(),
    }
}

impl<K: Kernel> Evaluate<K> for SketchDef {
    fn evaluate(
        &self,
        ctx: &mut super::EvalContext<'_, K>,
    ) -> Result<FeatureOutput<K::Shape>, String> {
        let plane = match (&self.plane, self.plane.origin_frame()) {
            (_, Some(frame)) => frame,
            (SketchPlane::Construction(uid), None) => ctx.datum_plane(*uid)?.frame(),
            (_, None) => face_frame(&self.plane, ctx)?,
        };
        let frame = self.place(&plane);
        let def = crate::sketch::project::follow_links(self, &frame, ctx)?;
        let def = def.as_ref().unwrap_or(self);
        let names: BTreeMap<ParamId, String> = def
            .dimensions
            .iter()
            .filter_map(|d| d.value)
            .map(|id| (id, ctx.param_name(id)))
            .collect();
        let solved = def
            .solve_with(&mut |id| ctx.param(id), &|id| {
                names.get(&id).cloned().unwrap_or_default()
            })
            .map_err(|e| e.message)?;
        let texts = def.text_outlines(&solved, ctx.kernel)?;
        // A font this computer lacks is replaced (P9: with a warning).
        for outline in texts.iter().filter(|t| t.fallback) {
            if let Some(text) = def.texts.iter().find(|t| t.id == outline.id) {
                ctx.warn(format!(
                    "text t{}: the font {} is not installed; {} is used",
                    text.id.0, text.font, outline.family
                ));
            }
        }
        let region_info = def.regions_of(&solved, &texts);
        Ok(FeatureOutput {
            sketch: Some(SketchOutput {
                frame,
                regions: region_info.iter().map(|r| r.profile.clone()).collect(),
                region_info,
                solved: Arc::new(solved),
                texts: Arc::new(texts),
            }),
            ..FeatureOutput::default()
        })
    }
}

/// The frame of a face plane from the bodies before the sketch.
fn face_frame<K: Kernel>(
    plane: &SketchPlane,
    ctx: &mut super::EvalContext<'_, K>,
) -> Result<SketchFrame, String> {
    let SketchPlane::Face { face, body } = plane else {
        unreachable!("only a face plane is found on the bodies");
    };
    let shapes = match body {
        Some(body) => vec![(*body, ctx.body(*body)?)],
        None => ctx.bodies(),
    };
    let mut found = None;
    for (uid, shape) in shapes {
        let count = ctx
            .kernel
            .count_faces(&shape, face)
            .map_err(|e| format!("plane {face}: {e}"))?;
        if count > 0 {
            if found.is_some() {
                return Err(format!(
                    "plane {face}: the face is on several bodies; name the body"
                ));
            }
            found = Some(uid);
        }
    }
    let body = found.ok_or_else(|| {
        format!("plane {face}: the face does not exist at this point of the timeline")
    })?;
    let face = GeomRef::Face {
        body,
        face: (**face).clone(),
    };
    Ok(ctx
        .plane(&face)
        .map_err(|e| format!("the sketch plane: {e}"))?
        .frame())
}

/// The curves of solved geometry by id, for callers outside the module.
pub fn curve_of(solved: &Solved, id: EntityUid) -> Option<&Curve2> {
    solved.curves.get(&id)
}
