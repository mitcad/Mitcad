// SPDX-License-Identifier: MIT
//! Derived part (mitcad#375, `commands.md`, "BUILD: profile assemblies"):
//! the first body of a component the BUILD tab placed, made from one of
//! the design's library sources (`crate::build::LibrarySources`). The
//! source's design is evaluated with the configuration row `config` and
//! the `parameters` set, its visible bodies are copied in (`<F>.b<i>`, in
//! the source's order, faces `<F>:part(<the source's face>)`), and a
//! cut-to-length part's ends are cut: end A at z = 0 and end B at z = L,
//! by `angle` from square about the section axis `tilt`. The length is the
//! outer length, the long point (mitcad#443): each cut plane runs through
//! the section's outermost edge on its long side (the side the cut leans
//! away from) at the end, so the body never reaches past 0 and L, and its
//! short point is the section's width across the tilt times tan(angle)
//! shorter. The end faces are always `<F>:end_a` and `<F>:end_b`, cut or
//! not. Every `mount:<name>` joint origin of the source becomes a frame of
//! the feature, `<F>:mount(<name>)`, which joints and other features name
//! as `{"mount": ...}`.
//!
//! A library part or a linked design inserted with options is a derived
//! part too (mitcad#801, `crate::part_options`): its `parameters` may be
//! numbers or texts in its source's units, and the source's rules hold. A
//! constant the rules do not allow is moved (a warning); a value bound to
//! the design's parameters that breaks a rule fails the part with the
//! rule's name.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::extrude::Operation;
use super::{BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput};
use super::{FeatureDef, References};
use crate::build::{BuildMeta, LibrarySourceDef, SourceId};
use crate::datum::Datum;
use crate::document::DocState;
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::{BooleanOp, BoundingBox, Kernel};
use crate::parameters::ParamId;
use crate::part_options::PartValue;
use crate::profile::SketchFrame;
use crate::recompute::{Cache, FeatureResult, Read};
use crate::topo::FaceName;
use crate::transform::{PrimitiveShape, PrimitiveSpec, Transform};

/// The section axis an end's cut turns about; the cut's outward normal
/// leans towards +y (`x`), −y (`-x`), +x (`y`) or −x (`-y`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tilt {
    #[default]
    #[serde(rename = "x")]
    X,
    #[serde(rename = "-x")]
    MinusX,
    #[serde(rename = "y")]
    Y,
    #[serde(rename = "-y")]
    MinusY,
}

impl Tilt {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::MinusX => "-x",
            Self::Y => "y",
            Self::MinusY => "-y",
        }
    }

    /// The direction in the section the cut's normal leans towards.
    pub fn lean(self) -> [f64; 3] {
        match self {
            Self::X => [0.0, 1.0, 0.0],
            Self::MinusX => [0.0, -1.0, 0.0],
            Self::Y => [1.0, 0.0, 0.0],
            Self::MinusY => [-1.0, 0.0, 0.0],
        }
    }

    /// The tilt whose lean is nearest to a direction in the section (its
    /// x and y; z is left out); None for one along the axis.
    pub fn towards(direction: [f64; 3]) -> Option<Self> {
        let [x, y, _] = direction;
        if x.abs() < 1e-9 && y.abs() < 1e-9 {
            return None;
        }
        Some(if y.abs() >= x.abs() {
            if y > 0.0 { Self::X } else { Self::MinusX }
        } else if x > 0.0 {
            Self::Y
        } else {
            Self::MinusY
        })
    }
}

/// One end's cut: its angle from square (radians, below 90 degrees) and
/// the axis it turns about.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndCut<P = ParamId> {
    pub angle: P,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tilt: Option<Tilt>,
}

/// The cuts of a cut-to-length part's ends; a missing end is square.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ends<P = ParamId> {
    #[serde(default = "no_cut", skip_serializing_if = "Option::is_none")]
    pub a: Option<EndCut<P>>,
    #[serde(default = "no_cut", skip_serializing_if = "Option::is_none")]
    pub b: Option<EndCut<P>>,
}

fn no_cut<P>() -> Option<EndCut<P>> {
    None
}

fn square<P>() -> Ends<P> {
    Ends { a: None, b: None }
}

impl<P> Ends<P> {
    pub fn is_square(&self) -> bool {
        self.a.is_none() && self.b.is_none()
    }

    pub fn end(&self, b: bool) -> Option<&EndCut<P>> {
        if b { self.b.as_ref() } else { self.a.as_ref() }
    }
}

fn new_body() -> Operation {
    Operation::NewBody
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedPartDef<P = ParamId> {
    pub source: SourceId,
    /// The source's configuration row; its default row when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
    /// Values of the source's user parameters, by their names.
    #[serde(default = "BTreeMap::new", skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, P>,
    #[serde(default = "square", skip_serializing_if = "Ends::is_square")]
    pub ends: Ends<P>,
    #[serde(default = "new_body")]
    pub operation: Operation,
    /// Joinery and rips (mitcad#478): boxes taken away after the ends.
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub cuts: Vec<PartCut<P>>,
}

/// A box a member's joinery (or a rip) takes away (mitcad#478): from
/// `origin` along `x`, `y` and their cross product by `size`, in the part's
/// coordinates; `with` and `junction` name the other member and the
/// junction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartCut<P = ParamId> {
    /// `housing`, `notch`, `half_lap`, `rip` or `birdsmouth` (mitcad#479).
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub with: Option<ComponentUid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub junction: Option<String>,
    pub origin: [P; 3],
    pub x: [f64; 3],
    pub y: [f64; 3],
    pub size: [P; 3],
    /// The joinery's depth as it was asked for (to cut it anew).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<f64>,
}

impl<P> DerivedPartDef<P> {
    pub const TYPE: &'static str = "derived_part";
    pub const BASE_NAME: &'static str = "DerivedComponent";

    // Written by hand: parameters, ends and cuts map under computed slots.
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<DerivedPartDef<Q>, E> {
        let mut parameters = BTreeMap::new();
        for (name, value) in &self.parameters {
            parameters.insert(name.clone(), f(&format!("parameters.{name}"), value)?);
        }
        let mut end = |key: &str, cut: &Option<EndCut<P>>| -> Result<Option<EndCut<Q>>, E> {
            match cut {
                Some(cut) => Ok(Some(EndCut {
                    angle: f(&format!("ends.{key}.angle"), &cut.angle)?,
                    tilt: cut.tilt,
                })),
                None => Ok(None),
            }
        };
        let ends = Ends {
            a: end("a", &self.ends.a)?,
            b: end("b", &self.ends.b)?,
        };
        let mut cuts = Vec::with_capacity(self.cuts.len());
        for (i, cut) in self.cuts.iter().enumerate() {
            let mut origin = Vec::with_capacity(3);
            let mut size = Vec::with_capacity(3);
            for k in 0..3 {
                origin.push(f(&format!("cuts.{i}.origin.{k}"), &cut.origin[k])?);
            }
            for k in 0..3 {
                size.push(f(&format!("cuts.{i}.size.{k}"), &cut.size[k])?);
            }
            let three = |v: Vec<Q>| -> [Q; 3] {
                let mut it = v.into_iter();
                let a = it.next().expect("three");
                let b = it.next().expect("three");
                let c = it.next().expect("three");
                [a, b, c]
            };
            cuts.push(PartCut {
                kind: cut.kind.clone(),
                with: cut.with,
                junction: cut.junction.clone(),
                origin: three(origin),
                x: cut.x,
                y: cut.y,
                size: three(size),
                depth: cut.depth,
            });
        }
        Ok(DerivedPartDef {
            source: self.source,
            config: self.config.clone(),
            parameters,
            ends,
            operation: self.operation,
            cuts,
        })
    }
}

impl FeatureInfo for DerivedPartDef {
    fn references(&self) -> References {
        References::default()
    }

    fn check(&self, _ctx: &CheckContext<'_>) -> Result<(), String> {
        if self.operation != Operation::NewBody {
            return Err("a derived part makes new bodies (operation new_body)".to_owned());
        }
        if let Some(name) = self
            .parameters
            .keys()
            .find(|n| !crate::expr::is_valid_name(n))
        {
            return Err(format!("{name} is no parameter name"));
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        for (key, cut) in [("a", &self.ends.a), ("b", &self.ends.b)] {
            if let Some(cut) = cut {
                check_angle(key, value(cut.angle))?;
            }
        }
        for cut in &self.cuts {
            if cut
                .size
                .iter()
                .any(|s| value(*s).is_nan() || value(*s) <= 0.0)
            {
                return Err(format!(
                    "the {} cut's size must be greater than zero",
                    cut.kind
                ));
            }
        }
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        true
    }
}

/// An end's angle: from 0 up to (not including) 90 degrees.
fn check_angle(end: &str, angle: f64) -> Result<(), String> {
    if !(0.0..89.0_f64.to_radians()).contains(&angle) {
        return Err(format!(
            "end {end}: the cut's angle is {} degrees; it is from 0 up to 89",
            crate::build::round(angle.to_degrees())
        ));
    }
    Ok(())
}

impl<'a, K: Kernel> EvalContext<'a, K> {
    /// A library source of the design (mitcad#375), read for the cache.
    pub(crate) fn source(&mut self, id: SourceId) -> Result<&'a LibrarySourceDef, String> {
        let sources: &'a crate::build::LibrarySources = self.env.sources;
        let found = sources.get(id);
        self.reads
            .push(Read::Source(id, found.and_then(LibrarySourceDef::version)));
        found.ok_or_else(|| format!("library source {id} is not in the design"))
    }
}

/// A source's design evaluated: its visible bodies placed in its
/// coordinates, their names, and its mount frames.
#[doc(hidden)]
pub struct Evaluated<S> {
    pub bodies: Vec<S>,
    pub mounts: BTreeMap<String, Transform>,
}

/// The source's state with the row and the parameters applied (numbers in
/// millimetres or radians), none of them moved by the source's rules
/// (`crate::part_options::configure` for texts and the cascade).
#[doc(hidden)]
pub fn configured(
    source: &LibrarySourceDef,
    config: Option<&str>,
    values: &[(String, f64)],
) -> Result<DocState, String> {
    let values: Vec<(String, PartValue)> = values
        .iter()
        .map(|(name, value)| (name.clone(), PartValue::Number(*value)))
        .collect();
    let pinned: BTreeSet<String> = values.iter().map(|(name, _)| name.clone()).collect();
    Ok(crate::part_options::configure(source, config, &values, &pinned)?.state)
}

/// Evaluates a source's state with the kernel.
#[doc(hidden)]
pub fn evaluate_source<K: Kernel>(
    kernel: &K,
    state: &DocState,
) -> Result<Evaluated<K::Shape>, String> {
    let result = crate::recompute::recompute(kernel, state, &mut Cache::default(), None)
        .map_err(|e| e.to_string())?;
    let assembly = state.assembly.at_placements(&result.placements);
    let placement = |o: OccurrenceUid| {
        result
            .placements
            .get(&o)
            .copied()
            .unwrap_or(Transform::IDENTITY)
    };
    let owned = |c: ComponentUid| {
        result
            .owners
            .iter()
            .filter(|(_, owner)| **owner == c)
            .map(|(b, _)| *b)
            .collect::<Vec<_>>()
    };
    let mut bodies = Vec::new();
    for instance in assembly.instances(&placement, &owned) {
        let shown = state
            .body_attributes
            .get(&instance.body)
            .is_none_or(|b| b.visible);
        if !(instance.visible && shown) {
            continue;
        }
        let Some(shape) = result.bodies.get(&instance.body) else {
            continue;
        };
        bodies.push(if instance.transform.is_identity() {
            shape.value.clone()
        } else {
            kernel
                .transform_shape(&shape.value, &instance.transform, None)
                .map_err(|e| e.to_string())?
        });
    }
    if bodies.is_empty() {
        let failed = result.results.iter().find_map(|r| {
            let entry = state.entry(r.uid)?;
            // (With its tail: its message, places and detail, mitcad#1612.)
            r.status.failure().map(|e| format!("{}: {e}", entry.name))
        });
        return Err(match failed {
            Some(failure) => format!("its design builds no bodies ({failure})"),
            None => "its design has no bodies to show".to_owned(),
        });
    }
    let mut mounts = BTreeMap::new();
    for entry in &state.features {
        // Named `mount:<name>`, or with a mount of its own (mitcad#525).
        let FeatureDef::JointOrigin(def) = &entry.def else {
            continue;
        };
        let Some(spec) = super::joint::mount_of(&entry.name, def) else {
            continue;
        };
        let name = spec.name.as_str();
        let Some(Datum::Plane(plane)) = result.datums.get(&entry.uid).map(|d| d.value) else {
            continue;
        };
        let local = crate::joints::frame(plane.origin, plane.x_axis, plane.normal());
        let at = if entry.component.is_root() {
            local
        } else {
            match assembly.paths_to(entry.component).first() {
                Some(path) => path
                    .iter()
                    .fold(Transform::IDENTITY, |t, o| t.after(&placement(*o)))
                    .after(&local),
                None => continue,
            }
        };
        mounts.insert(name.to_owned(), at);
    }
    Ok(Evaluated { bodies, mounts })
}

/// A derived part's mount frame in a recompute's results (for joints and
/// the document's queries).
pub(crate) fn mount_frame<S>(
    history: &[FeatureResult<S>],
    feature: FeatureUid,
    name: &str,
    label: &str,
) -> Result<Transform, String> {
    let result = history
        .iter()
        .find(|r| r.uid == feature)
        .ok_or_else(|| format!("{label} has no result at this point of the timeline"))?;
    if !result.status.is_ok() {
        return Err(format!("{label} has no frames: it failed"));
    }
    result
        .output
        .as_ref()
        .and_then(|o| o.mounts.as_ref())
        .and_then(|m| m.get(name))
        .copied()
        .ok_or_else(|| format!("{label} has no frame {name}"))
}

/// The box of all the shapes, None when the kernel cannot tell.
fn bounds_of<K: Kernel>(kernel: &K, shapes: &[K::Shape]) -> Option<BoundingBox> {
    let mut all: Option<BoundingBox> = None;
    for shape in shapes {
        let b = kernel.bounding_box(shape).ok()??;
        all = Some(match all {
            Some(a) => a.union(&b),
            None => b,
        });
    }
    all
}

/// The faces of `shape` on the plane at height `z` facing `normal` (+z or
/// −z): a square end's faces.
fn end_faces<K: Kernel>(kernel: &K, shape: &K::Shape, z: f64, up: bool) -> Vec<FaceName> {
    let Ok(faces) = kernel.faces(shape) else {
        return Vec::new();
    };
    let tolerance = 1e-6 * z.abs().max(1.0);
    let sign = if up { 1.0 } else { -1.0 };
    let mut out = Vec::new();
    for info in faces {
        for text in &info.names {
            let Ok(name) = text.parse::<FaceName>() else {
                continue;
            };
            if let Ok(crate::datum::SurfaceGeometry::Plane { origin, normal }) =
                kernel.face_geometry(shape, &name)
                && normal[2] * sign > 1.0 - 1e-9
                && (origin[2] - z).abs() <= tolerance
            {
                out.push(name);
            }
        }
    }
    out
}

impl<K: Kernel> Evaluate<K> for DerivedPartDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let source = ctx.source(self.source)?;
        let meta = match &source.build {
            Some(build) => BuildMeta::read(build)?,
            None => BuildMeta::default(),
        };
        let length_name = meta
            .length
            .as_ref()
            .map(|l| l.parameter.clone())
            .filter(|p| !p.is_empty());
        // Its values, numbers or texts (mitcad#801); those bound to the
        // design's parameters are never moved by the source's rules.
        let mut values = Vec::with_capacity(self.parameters.len());
        let mut pinned = BTreeSet::new();
        for (name, id) in &self.parameters {
            let value = if ctx.env.params.text(*id).is_some() {
                PartValue::Text(ctx.param_text(*id)?)
            } else {
                PartValue::Number(ctx.param(*id)?)
            };
            if crate::part_options::is_bound(ctx.env.params, *id, ctx.uid) {
                pinned.insert(name.clone());
            }
            values.push((name.clone(), value));
        }
        let mut angles = [0.0; 2];
        let mut tilts = [Tilt::X; 2];
        for (i, (key, cut)) in [("a", &self.ends.a), ("b", &self.ends.b)]
            .into_iter()
            .enumerate()
        {
            if let Some(cut) = cut {
                angles[i] = ctx.param(cut.angle)?;
                check_angle(key, angles[i])?;
                tilts[i] = cut.tilt.unwrap_or_default();
            }
        }
        let angled = angles.map(|a| a > 1e-12);
        if (angled[0] || angled[1]) && length_name.is_none() {
            return Err(format!(
                "{} is not cut to length: its ends cannot be cut",
                source.label()
            ));
        }
        let label = format!("{} ({})", source.label(), source.id);
        let configured =
            crate::part_options::configure(source, self.config.as_deref(), &values, &pinned)
                .map_err(|e| format!("{label}: {e}"))?;
        // The source's rules (mitcad#801): a constant moved is a warning,
        // a bound value that breaks a rule fails the part.
        if let Some(broken) = configured.broken {
            return Err(format!("{label}: {}", broken.text));
        }
        for moved in &configured.moves {
            ctx.warn(format!("{label}: {}", moved.text));
        }
        let state = configured.state;
        let length = match &length_name {
            Some(name) => {
                let id = state.parameters.find(name).ok_or_else(|| {
                    format!("{label}: its length parameter {name} does not exist")
                })?;
                let length = state.parameters.value(id).unwrap_or(f64::NAN);
                if let Some(problem) = meta.length.as_ref().and_then(|l| l.outside(length)) {
                    ctx.warn(problem);
                }
                Some(length)
            }
            None => None,
        };
        let kernel = ctx.kernel;
        let at_length = evaluate_source(kernel, &state).map_err(|e| format!("{label}: {e}"))?;
        let bodies = at_length.bodies;
        // The section's bounds, where the angled ends' long sides are; the
        // cuts take away material only, the body stays within its length.
        let section = if angled[0] || angled[1] {
            bounds_of(kernel, &bodies)
        } else {
            None
        };
        let uid = ctx.uid;
        let prefix = format!("{uid}:part");
        let end_name = |b: bool| FaceName::new(uid, if b { "end_b" } else { "end_a" }, None);
        let mut changes = Vec::with_capacity(bodies.len());
        for (i, body) in bodies.iter().enumerate() {
            // Square ends take the end names; angled ones get them from
            // their cuts.
            let mut renames = Vec::new();
            if let Some(length) = length {
                for (b, z) in [(false, 0.0), (true, length)] {
                    if !angled[usize::from(b)] {
                        for face in end_faces(kernel, body, z, b) {
                            renames.push((face, end_name(b)));
                        }
                    }
                }
            }
            let mut shape = kernel
                .rename_faces(body, &prefix, &renames)
                .map_err(|e| format!("{label}: {e}"))?;
            if let Some(length) = length {
                for b in [false, true] {
                    let end = usize::from(b);
                    if !angled[end] {
                        continue;
                    }
                    let plane = end_plane(section.as_ref(), b, length, angles[end], tilts[end])
                        .ok_or("the cut has no normal")?;
                    shape = cut_end(ctx, &shape, b, length, plane)?;
                }
            }
            // Joinery and rips (mitcad#478).
            for (k, cut) in self.cuts.iter().enumerate() {
                let origin = [
                    ctx.param(cut.origin[0])?,
                    ctx.param(cut.origin[1])?,
                    ctx.param(cut.origin[2])?,
                ];
                let size = [
                    ctx.param(cut.size[0])?,
                    ctx.param(cut.size[1])?,
                    ctx.param(cut.size[2])?,
                ];
                shape = cut_box(ctx, &shape, k, &cut.kind, origin, cut.x, cut.y, size)?;
            }
            changes.push(BodyChange::Set(BodyUid::new(uid, i as u32), shape));
        }
        Ok(FeatureOutput {
            changes,
            mounts: at_length.mounts,
            ..FeatureOutput::default()
        })
    }
}

/// The plane end B (`b`) or A of a part `length` long is cut by, as a
/// point on it and its outward normal (mitcad#443): the normal leans from
/// the end's axis by `angle` towards the tilt's side, and the plane runs
/// through the section's outermost edge on the other side, the long side,
/// at the end (z = 0 or `length`), so the long point is the length. With
/// no `section` bounds known it runs through the end's centre.
#[doc(hidden)]
pub fn end_plane(
    section: Option<&BoundingBox>,
    b: bool,
    length: f64,
    angle: f64,
    tilt: Tilt,
) -> Option<([f64; 3], [f64; 3])> {
    use crate::build::{add, scale, unit};
    let lean = tilt.lean();
    let axis = [0.0, 0.0, if b { 1.0 } else { -1.0 }];
    let normal = unit(add(scale(axis, angle.cos()), scale(lean, angle.sin())))?;
    // The least of the section along the lean: its long side.
    let k = if lean[0] != 0.0 { 0 } else { 1 };
    let long = section.map_or(0.0, |s| (lean[k] * s.min[k]).min(lean[k] * s.max[k]));
    let mut point = scale(lean, long);
    point[2] = if b { length } else { 0.0 };
    Some((point, normal))
}

/// Cuts end B (`b`) or A of a body by the plane `(point, normal)`, taking
/// away what is beyond it along the normal; the cut face is the end's face.
fn cut_end<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    body: &K::Shape,
    b: bool,
    length: f64,
    (centre, normal): ([f64; 3], [f64; 3]),
) -> Result<K::Shape, String> {
    let uid = ctx.uid;
    let key = if b { "b" } else { "a" };
    let kernel = ctx.kernel;
    let failed = |e: crate::kernel::KernelError| format!("end {key}: {e}");
    let tool = end_tool(kernel, uid, b, length, (centre, normal)).map_err(failed)?;
    let output = ctx
        .boolean(BooleanOp::Cut, &[body], &tool)
        .map_err(failed)?;
    if !output.touched.first().copied().unwrap_or(false) {
        ctx.warn(format!("end {key}: the cut does not meet the component"));
        return Ok(body.clone());
    }
    let mut pieces = output
        .pieces
        .into_iter()
        .filter(|p| p.sources.contains(&0))
        .map(|p| p.shape);
    let first = pieces
        .next()
        .ok_or_else(|| format!("end {key}: the cut leaves nothing of the component"))?;
    if pieces.next().is_some() {
        ctx.warn(format!(
            "end {key}: the cut parts the body; its first piece is kept"
        ));
    }
    Ok(first)
}

/// Takes a joinery box away from a body (mitcad#478); the tool's faces are
/// named `<F>:joint_<k>(...)`.
#[allow(clippy::too_many_arguments)]
fn cut_box<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    body: &K::Shape,
    k: usize,
    kind: &str,
    origin: [f64; 3],
    x: [f64; 3],
    y: [f64; 3],
    size: [f64; 3],
) -> Result<K::Shape, String> {
    let uid = ctx.uid;
    let kernel = ctx.kernel;
    let failed = |e: crate::kernel::KernelError| format!("the {kind} cut: {e}");
    let spec = PrimitiveSpec {
        feature: uid,
        frame: SketchFrame {
            origin,
            x_axis: x,
            y_axis: y,
        },
        shape: PrimitiveShape::Box {
            length: size[0],
            width: size[1],
            height: size[2],
        },
    };
    let tool = kernel.primitive(&spec).map_err(failed)?;
    let tool = kernel
        .rename_faces(&tool, &format!("{uid}:joint_{k}"), &[])
        .map_err(failed)?;
    let output = ctx
        .boolean(BooleanOp::Cut, &[body], &tool)
        .map_err(failed)?;
    if !output.touched.first().copied().unwrap_or(false) {
        ctx.warn(format!("the {kind} cut does not meet the component"));
        return Ok(body.clone());
    }
    output
        .pieces
        .into_iter()
        .find(|p| p.sources.contains(&0))
        .map(|p| p.shape)
        .ok_or_else(|| format!("the {kind} cut leaves nothing of the component"))
}

/// The tool an end's cut takes away: a box beyond the plane `(point,
/// normal)`, its face on the plane named the end's (`<F>:end_a`,
/// `<F>:end_b`).
#[doc(hidden)]
pub fn end_tool<K: Kernel>(
    kernel: &K,
    uid: FeatureUid,
    b: bool,
    length: f64,
    (centre, normal): ([f64; 3], [f64; 3]),
) -> Result<K::Shape, crate::kernel::KernelError> {
    use crate::build::{add, cross, norm, perpendicular, scale};
    let key = if b { "b" } else { "a" };
    let size = 4.0 * (length.abs() + 1000.0);
    let u = perpendicular(normal);
    let v = cross(normal, u);
    let corner = add(centre, add(scale(u, -size / 2.0), scale(v, -size / 2.0)));
    debug_assert!(norm(v) > 0.5);
    let spec = PrimitiveSpec {
        feature: uid,
        frame: SketchFrame {
            origin: corner,
            x_axis: u,
            y_axis: v,
        },
        shape: PrimitiveShape::Box {
            length: size,
            width: size,
            height: size,
        },
    };
    let tool = kernel.primitive(&spec)?;
    let end = FaceName::new(uid, if b { "end_b" } else { "end_a" }, None);
    kernel.rename_faces(
        &tool,
        &format!("{uid}:cut_{key}"),
        &[(FaceName::new(uid, "bottom", None), end)],
    )
}

/// The mount frames of a derived part, shared by the results.
pub(crate) type Mounts = Arc<BTreeMap<String, Transform>>;
