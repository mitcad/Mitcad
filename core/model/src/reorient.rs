// SPDX-License-Identifier: MIT
//! Reorient Design (mitcad#502): the whole design turned by a multiple of
//! 90 degrees about the origin's X, Y or Z axis, as a rewrite of its
//! definition rather than a move of its bodies.
//!
//! A [`Turn`] is such a rotation: a signed permutation of the axes, so a
//! turned vector's components are the vector's own, some negated, and
//! nothing is rounded. Every component the design can change (all but
//! those linked from other files) turns in its own coordinates, so that
//! its origin planes are where the turned design wants them:
//!
//! - references to the origin's planes and axes become their turned
//!   counterparts ([`Turn::reference`]): the origin datum that the turn
//!   takes them onto, frame and sense included, or else the fixed plane
//!   or axis there (XY turned 90 degrees about X faces -Y: the fixed
//!   plane with normal -Y and x along +X);
//! - fixed planes, axes, points, paths and the other positions and
//!   directions definitions keep in the component's coordinates turn with
//!   it ([`turn_definition`]);
//! - a sketch keeps its plane where the turn takes an origin plane onto
//!   one, with its own frame ([`crate::features::sketch::FrameDef`]) set so
//!   that its geometry turns with the design, not mirrored;
//! - occurrences' placements are turned in both coordinates they join
//!   (`R · T · R⁻¹`; an occurrence of a linked component `R · T`).
//!
//! What builds from these follows by itself: construction geometry,
//! features on faces and edges (by topological name, which the turn keeps)
//! and the occurrences' placements by joints.

use std::collections::{BTreeMap, BTreeSet};

use crate::assembly::Assembly;
use crate::datum::{Datum, OriginDatum, Vec3};
use crate::document::AnalysisDef;
use crate::features::construction::{AxisDefinition, PlaneDefinition, PointDefinition};
use crate::features::geom_ref::{GeomRef, PathRef, to_turned_value};
use crate::features::joint::{FrameOverride, JointOrigin};
use crate::features::moves::MoveSpec;
use crate::features::scale::ScaleSpec;
use crate::features::{
    FeatureDef, FrameDef, HolePlacement, OccurrencePath, SketchDef, SketchPlane,
};
use crate::ids::{BodyUid, ComponentUid, FeatureUid};
use crate::links::OccurrenceLinkExt;
use crate::parameters::ParamId;
use crate::profile::SketchFrame;
use crate::render_settings::{LightSpace, RenderSettings};
use crate::topo::{EdgeName, FaceName, RoleKey, TopoName, VertexName};
use crate::transform::Transform;

/// A rotation by a multiple of 90 degrees about the origin: the turned
/// vector's component `r` is `sign[r] · v[from[r]]` (see the module
/// documentation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Turn {
    from: [usize; 3],
    sign: [bool; 3],
}

/// The origin axes as [`Turn::about`] takes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnAxis {
    X,
    Y,
    Z,
}

impl TurnAxis {
    pub fn parse(id: &str) -> Option<Self> {
        match id {
            "x" => Some(Self::X),
            "y" => Some(Self::Y),
            "z" => Some(Self::Z),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::X => "X",
            Self::Y => "Y",
            Self::Z => "Z",
        }
    }
}

impl Turn {
    pub const IDENTITY: Self = Self {
        from: [0, 1, 2],
        sign: [false; 3],
    };

    /// `quarters` quarter turns about the axis, right-handed
    /// (counter-clockwise looking against the axis).
    pub fn about(axis: TurnAxis, quarters: u32) -> Self {
        // One quarter turn: about X, y → z and z → -y.
        let quarter = match axis {
            TurnAxis::X => Self {
                from: [0, 2, 1],
                sign: [false, true, false],
            },
            TurnAxis::Y => Self {
                from: [2, 1, 0],
                sign: [false, false, true],
            },
            TurnAxis::Z => Self {
                from: [1, 0, 2],
                sign: [true, false, false],
            },
        };
        (0..quarters % 4).fold(Self::IDENTITY, |turn, _| quarter.after(&turn))
    }

    /// The turn that takes an origin plane's frame onto XY's (x onto +X,
    /// y onto +Y, the normal onto +Z): XZ by a quarter turn about X, YZ by
    /// three about Y. None for an axis or the origin point.
    pub fn plane_onto_xy(plane: OriginDatum) -> Option<Self> {
        match plane {
            OriginDatum::Xy => Some(Self::IDENTITY),
            OriginDatum::Xz => Some(Self::about(TurnAxis::X, 1)),
            OriginDatum::Yz => Some(Self::about(TurnAxis::Y, 3)),
            _ => None,
        }
    }

    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    /// The turn that applies `first`, then this one.
    pub fn after(&self, first: &Turn) -> Turn {
        Turn {
            from: std::array::from_fn(|r| first.from[self.from[r]]),
            sign: std::array::from_fn(|r| self.sign[r] != first.sign[self.from[r]]),
        }
    }

    pub fn inverse(&self) -> Turn {
        let mut inverse = Self::IDENTITY;
        for r in 0..3 {
            inverse.from[self.from[r]] = r;
            inverse.sign[self.from[r]] = self.sign[r];
        }
        inverse
    }

    /// The turned vector or point, exactly (a zero stays +0).
    pub fn vector(&self, v: Vec3) -> Vec3 {
        std::array::from_fn(|r| {
            let value = v[self.from[r]];
            (if self.sign[r] { -value } else { value }) + 0.0
        })
    }

    /// The rows of the rotation matrix, entries 0 and ±1.
    pub fn matrix(&self) -> [[f64; 3]; 3] {
        std::array::from_fn(|r| {
            std::array::from_fn(|c| {
                if self.from[r] != c {
                    0.0
                } else if self.sign[r] {
                    -1.0
                } else {
                    1.0
                }
            })
        })
    }

    pub fn transform(&self) -> Transform {
        Transform {
            linear: self.matrix(),
            translation: [0.0; 3],
        }
    }

    /// `this · t`: a transform into turned coordinates from unturned ones.
    pub fn before(&self, t: &Transform) -> Transform {
        Transform {
            linear: std::array::from_fn(|r| self.turned_row(t, r)),
            translation: self.vector(t.translation),
        }
    }

    /// `t · this⁻¹`: a transform out of turned coordinates into unturned
    /// ones.
    pub fn undone_before(&self, t: &Transform) -> Transform {
        // Column c of t · R⁻¹ is t's column from[c] (R⁻¹ eᶜ = ±e_from[c]).
        let linear = std::array::from_fn(|r| {
            std::array::from_fn(|c| {
                let value = t.linear[r][self.from[c]];
                (if self.sign[c] { -value } else { value }) + 0.0
            })
        });
        Transform {
            linear,
            translation: t.translation,
        }
    }

    /// `this · t · this⁻¹`: a transform within coordinates that both turn.
    pub fn conjugate(&self, t: &Transform) -> Transform {
        self.undone_before(&self.before(t))
    }

    // Row r of this · t's linear part.
    fn turned_row(&self, t: &Transform, r: usize) -> [f64; 3] {
        let row = t.linear[self.from[r]];
        row.map(|value| (if self.sign[r] { -value } else { value }) + 0.0)
    }

    /// The turned sketch frame.
    pub fn frame(&self, frame: &SketchFrame) -> SketchFrame {
        SketchFrame {
            origin: self.vector(frame.origin),
            x_axis: self.vector(frame.x_axis),
            y_axis: self.vector(frame.y_axis),
        }
    }

    /// The turned counterpart of a reference: an origin plane or axis
    /// becomes the origin datum the turn takes it onto, frame and sense
    /// included, else the fixed plane or axis there; fixed geometry turns;
    /// references to features, bodies and sketches stay (what they name
    /// turns with the design).
    pub fn reference(&self, reference: &GeomRef) -> GeomRef {
        match reference {
            GeomRef::Origin(origin) => match origin.datum() {
                Datum::Plane(plane) => {
                    let (x, y) = (self.vector(plane.x_axis), self.vector(plane.y_axis));
                    match origin_plane_with(x, y) {
                        Some(turned) => GeomRef::Origin(turned),
                        None => GeomRef::FixedPlane {
                            origin: [0.0; 3],
                            normal: mitcad_vec3::cross(x, y),
                            x_axis: Some(x),
                        },
                    }
                }
                Datum::Axis(axis) => {
                    let direction = self.vector(axis.direction);
                    match origin_axis_along(direction) {
                        Some(turned) => GeomRef::Origin(turned),
                        None => GeomRef::FixedAxis {
                            origin: [0.0; 3],
                            direction,
                        },
                    }
                }
                Datum::Point(_) => reference.clone(),
            },
            GeomRef::FixedPlane {
                origin,
                normal,
                x_axis,
            } => {
                // Without an x axis the plane's frame would follow the
                // model axes, which the turn does not move: keep the one
                // it has.
                let x = x_axis.unwrap_or_else(|| {
                    mitcad_vec3::unit(*normal)
                        .map(mitcad_model_base::internal::datum::default_x_axis)
                        .unwrap_or([1.0, 0.0, 0.0])
                });
                GeomRef::FixedPlane {
                    origin: self.vector(*origin),
                    normal: self.vector(*normal),
                    x_axis: Some(self.vector(x)),
                }
            }
            GeomRef::FixedAxis { origin, direction } => GeomRef::FixedAxis {
                origin: self.vector(*origin),
                direction: self.vector(*direction),
            },
            GeomRef::FixedPoint(point) => GeomRef::FixedPoint(self.vector(*point)),
            GeomRef::Datum(_)
            | GeomRef::Face { .. }
            | GeomRef::Edge { .. }
            | GeomRef::Vertex { .. }
            | GeomRef::Body(_)
            | GeomRef::SketchCurves { .. }
            | GeomRef::SketchPoint { .. }
            | GeomRef::Mount { .. } => reference.clone(),
        }
    }

    /// The turned counterpart of a path: fixed lines, helices and curves turn,
    /// edges and sketch curves stay.
    pub fn path(&self, path: &PathRef) -> PathRef {
        match path {
            PathRef::Line { start, end } => PathRef::Line {
                start: self.vector(*start),
                end: self.vector(*end),
            },
            PathRef::Helix(helix) => {
                let mut turned = helix.clone();
                turned.center = self.vector(helix.center);
                turned.axis = self.vector(helix.axis);
                turned.x_axis = self.vector(helix.x_axis);
                PathRef::Helix(turned)
            }
            // Fixed curves (mitcad#548): their points and directions.
            PathRef::Fixed(curves) => PathRef::Fixed(
                curves
                    .iter()
                    .map(|c| c.mapped(&|v| self.vector(v)))
                    .collect(),
            ),
            PathRef::Edges { .. } | PathRef::SketchCurve { .. } | PathRef::SketchCurves { .. } => {
                path.clone()
            }
        }
    }
}

/// What a rewrite needs besides the turn: which components turn (all but
/// those linked from other files) and the assembly, for the components
/// that occurrence paths end in.
pub(crate) struct Turned<'a> {
    pub turn: Turn,
    pub turned: &'a BTreeSet<ComponentUid>,
    pub assembly: &'a Assembly,
}

impl Turned<'_> {
    /// The turn of a component's coordinates: none for a linked one.
    pub fn of(&self, component: ComponentUid) -> Turn {
        if self.turned.contains(&component) {
            self.turn
        } else {
            Turn::IDENTITY
        }
    }

    /// A transform from `from`'s coordinates into `into`'s, both as they
    /// turn: `Q_into · t · Q_from⁻¹`.
    pub fn between(&self, from: ComponentUid, into: ComponentUid, t: &Transform) -> Transform {
        self.of(into).before(&self.of(from).undone_before(t))
    }

    /// An occurrence's placement of `component` in `parent`.
    pub fn placed_in(
        &self,
        parent: ComponentUid,
        component: ComponentUid,
        t: &Transform,
    ) -> Transform {
        self.between(component, parent, t)
    }

    /// The component an occurrence path from `component` ends in.
    fn end(&self, component: ComponentUid, path: &OccurrencePath) -> Result<ComponentUid, String> {
        crate::joints::path_component(self.assembly, component, &path.0)
    }
}

/// Marks a parameter slot in the JSON form of a turned definition.
const PARAM: char = '\u{1}';

/// A definition of a feature of `component` (a turned one) as it is in
/// the turned design, with the model parameters whose sign the turn
/// changes (a Move's x, y or z that now runs along a negative axis).
pub(crate) fn turn_definition(
    def: &FeatureDef,
    component: ComponentUid,
    ctx: &Turned<'_>,
) -> Result<(FeatureDef, Vec<ParamId>), String> {
    // A base feature holds bodies and no references: a Move after it turns
    // them (`document/reorient.rs`).
    if matches!(def, FeatureDef::Base(_)) {
        return Ok((def.clone(), Vec::new()));
    }
    // Every reference and path, wherever it is, through its JSON form
    // (`geom_ref::to_turned_value`); parameters ride along as markers.
    let mut slots = Vec::new();
    let marked = def
        .map_params(&mut |_, id: &ParamId| {
            slots.push(*id);
            Ok::<_, ()>(format!("{PARAM}{}", slots.len() - 1))
        })
        .expect("never fails");
    let value = to_turned_value(&marked, ctx.turn).map_err(|e| e.to_string())?;
    let turned: FeatureDef<String> = serde_json::from_value(value).map_err(|e| e.to_string())?;
    let mut out = turned.map_params(&mut |slot, marker: &String| {
        marker
            .strip_prefix(PARAM)
            .and_then(|i| i.parse::<usize>().ok())
            .and_then(|i| slots.get(i).copied())
            .ok_or_else(|| format!("{slot} lost its parameter"))
    })?;
    let mut negated = Vec::new();
    turn_fields(&mut out, def, component, ctx, &mut negated)?;
    Ok((out, negated))
}

/// The positions, directions and transforms definitions keep outside
/// references, and the references into components that do not turn.
fn turn_fields(
    out: &mut FeatureDef,
    def: &FeatureDef,
    component: ComponentUid,
    ctx: &Turned<'_>,
    negated: &mut Vec<ParamId>,
) -> Result<(), String> {
    let turn = ctx.turn;
    let v = |p: &mut Vec3| *p = turn.vector(*p);
    let some = |p: &mut Option<Vec3>| {
        if let Some(p) = p {
            *p = turn.vector(*p);
        }
    };
    let openings = |o: &mut Vec<crate::features::sheet_edges::OpeningEdgesRef>| {
        for opening in o {
            opening.at.iter_mut().for_each(|p| *p = turn.vector(*p));
        }
    };
    match out {
        FeatureDef::Sketch(sketch) => turn_sketch_plane(sketch, turn),
        FeatureDef::Move(m) => turn_move(&mut m.transform, turn, negated),
        FeatureDef::MoveOccurrence(m) => turn_move(&mut m.transform, turn, negated),
        FeatureDef::Scale(scale) => {
            if let ScaleSpec::NonUniform { x, y, z } = &mut scale.scale {
                // Factors along the axes: each goes with its axis.
                let old = [*x, *y, *z];
                let [a, b, c] = std::array::from_fn(|r| old[turn.from[r]]);
                (*x, *y, *z) = (a, b, c);
            }
        }
        FeatureDef::CapturePosition(capture) => {
            for position in &mut capture.positions {
                let placed = ctx
                    .assembly
                    .occurrence(position.occurrence)
                    .map(|o| o.component)
                    .ok_or_else(|| format!("occurrence {} does not exist", position.occurrence))?;
                position.transform = ctx.placed_in(component, placed, &position.transform);
            }
        }
        FeatureDef::BodyTransfer(transfer) => {
            if let Some(t) = &mut transfer.transform {
                let source = transfer.link.source_component(ctx.assembly)?;
                *t = ctx.between(source, component, t);
            }
        }
        FeatureDef::AsBuiltJoint(joint) => {
            if let Some(relative) = &mut joint.relative {
                let a = ctx.end(component, &joint.a)?;
                let b = ctx.end(component, &joint.b)?;
                *relative = ctx.between(a, b, relative);
            }
        }
        FeatureDef::JointOrigin(origin) => {
            if let Some(o) = &mut origin.frame_override {
                turn_override(o, turn);
            }
        }
        FeatureDef::ConstructionPlane(plane) => {
            if let PlaneDefinition::Fixed {
                origin,
                x_axis,
                y_axis,
            } = &mut plane.definition
            {
                v(origin);
                v(x_axis);
                v(y_axis);
            }
        }
        FeatureDef::ConstructionAxis(axis) => {
            if let AxisDefinition::Fixed { origin, direction } = &mut axis.definition {
                v(origin);
                v(direction);
            }
        }
        FeatureDef::ConstructionPoint(point) => {
            if let PointDefinition::Fixed { point } = &mut point.definition {
                v(point);
            }
        }
        FeatureDef::Hole(hole) => match &mut hole.placement {
            HolePlacement::Face { points, .. } | HolePlacement::Plane { points, .. } => {
                points.iter_mut().for_each(v);
            }
            HolePlacement::CylinderOffset { toward, .. } => v(toward),
            HolePlacement::FaceOffsets { .. }
            | HolePlacement::SketchPoints { .. }
            | HolePlacement::At { .. } => {}
        },
        FeatureDef::Boss(boss) => {
            v(&mut boss.point);
            some(&mut boss.direction);
        }
        FeatureDef::SnapFit(snap) => {
            v(&mut snap.point);
            some(&mut snap.direction);
            v(&mut snap.hook);
        }
        FeatureDef::Rest(rest) => {
            v(&mut rest.point);
            some(&mut rest.direction);
            some(&mut rest.along);
        }
        FeatureDef::BoundaryFill(fill) => fill.cells.iter_mut().for_each(v),
        FeatureDef::BoundarySurface(surface) => {
            for side in surface
                .boundary
                .iter_mut()
                .chain(&mut surface.u)
                .chain(&mut surface.v)
            {
                some(&mut side.point);
                openings(&mut side.openings);
            }
        }
        FeatureDef::BlendSurface(blend) => {
            openings(&mut blend.from.openings);
            openings(&mut blend.to.openings);
        }
        FeatureDef::ExtendSurface(extend) => openings(&mut extend.openings),
        FeatureDef::MatchSurface(surface) => openings(&mut surface.openings),
        FeatureDef::RuledSurface(ruled) => {
            openings(&mut ruled.openings);
            some(&mut ruled.direction);
        }
        FeatureDef::Patch(patch) => {
            if let Some(boundary) = &mut patch.boundary {
                boundary.points.iter_mut().for_each(v);
            }
        }
        FeatureDef::TrimSurface(trim) => {
            some(&mut trim.direction);
            trim.keep.iter_mut().for_each(v);
        }
        FeatureDef::SplitBody(split) => some(&mut split.direction),
        FeatureDef::SplitFace(split) => some(&mut split.direction),
        // A loft's constant binormal frame along its centre line (mitcad#543).
        FeatureDef::Loft(loft) => {
            if let Some(crate::features::sweep::SweepFrame::ConstantBinormal { binormal }) =
                &mut loft.frame
            {
                v(binormal);
            }
        }
        FeatureDef::Sweep(sweep) => {
            some(&mut sweep.binormal);
            // A constant binormal frame (mitcad#539).
            if let Some(crate::features::sweep::SweepFrame::ConstantBinormal { binormal }) =
                &mut sweep.frame
            {
                v(binormal);
            }
            // Fixed curves (mitcad#548).
            for curve in &mut sweep.fixed_curves {
                *curve = curve.mapped(&|p| turn.vector(p));
            }
        }
        // An extrusion's own direction (mitcad#539).
        FeatureDef::Extrude(extrude) => some(&mut extrude.direction),
        // A dogbone's tool axis, the component's Z without one (mitcad#1011).
        FeatureDef::Dogbone(dogbone) => {
            let axis = dogbone
                .axis
                .unwrap_or(crate::features::dogbone::DEFAULT_AXIS);
            dogbone.axis = Some(turn.vector(axis));
        }
        _ => {}
    }
    // Joint origins: geometry and frame overrides are in the component
    // their path ends in, which turns unless it is linked.
    let old_origins = match def {
        FeatureDef::Joint(_)
        | FeatureDef::AsBuiltJoint(_)
        | FeatureDef::ConstraintSet(_)
        | FeatureDef::MotionLink(_) => joint_origins(def),
        _ => return Ok(()),
    };
    for (new, old) in joint_origins_mut(out).into_iter().zip(old_origins) {
        if ctx.turned.contains(&ctx.end(component, &old.occurrence)?) {
            if let Some(o) = &mut new.frame_override {
                turn_override(o, turn);
            }
        } else {
            new.geometry = old.geometry;
            new.frame_override = old.frame_override;
        }
    }
    Ok(())
}

/// A frame override's origin and axes (its shift is along the frame's own
/// axes).
fn turn_override(o: &mut FrameOverride, turn: Turn) {
    for p in [&mut o.origin, &mut o.z_axis, &mut o.x_axis]
        .into_iter()
        .flatten()
    {
        *p = turn.vector(*p);
    }
}

/// A move's motion: a matrix in turned coordinates; values along the axes
/// go with their axes, negated where an axis now runs the other way.
fn turn_move(spec: &mut MoveSpec, turn: Turn, negated: &mut Vec<ParamId>) {
    let mut permute = |x: &mut ParamId, y: &mut ParamId, z: &mut ParamId| {
        let old = [*x, *y, *z];
        let new: [ParamId; 3] = std::array::from_fn(|r| old[turn.from[r]]);
        negated.extend((0..3).filter(|&r| turn.sign[r]).map(|r| new[r]));
        (*x, *y, *z) = (new[0], new[1], new[2]);
    };
    match spec {
        MoveSpec::Free { matrix } => {
            let rows: Vec<Vec<f64>> = matrix.iter().map(|r| r.to_vec()).collect();
            if let Ok(t) = crate::assembly::from_rows(&rows) {
                *matrix = crate::assembly::matrix_rows(&turn.conjugate(&t));
            } else {
                // Not rigid (a file's rounding): the rows turned directly.
                let t = Transform {
                    linear: std::array::from_fn(|r| std::array::from_fn(|c| matrix[r][c])),
                    translation: std::array::from_fn(|r| matrix[r][3]),
                };
                *matrix = crate::assembly::matrix_rows(&turn.conjugate(&t));
            }
        }
        MoveSpec::TranslateXyz { x, y, z } | MoveSpec::PointToPosition { x, y, z, .. } => {
            permute(x, y, z)
        }
        MoveSpec::TranslateAlong { .. }
        | MoveSpec::Rotate { .. }
        | MoveSpec::PointToPoint { .. } => {}
    }
}

/// A sketch on an origin plane (of its own component): the origin plane
/// the turn takes its plane onto, and the own frame that keeps its
/// geometry turned with the design (in-plane orientation and side). Other
/// planes give their frames when evaluated (the command corrects them).
fn turn_sketch_plane(sketch: &mut SketchDef, turn: Turn) {
    if sketch.plane_link.is_some() {
        return;
    }
    let origin = match sketch.plane {
        SketchPlane::Xy => OriginDatum::Xy,
        SketchPlane::Xz => OriginDatum::Xz,
        SketchPlane::Yz => OriginDatum::Yz,
        SketchPlane::Construction(_) | SketchPlane::Face { .. } => return,
    };
    let Datum::Plane(plane) = origin.datum() else {
        return;
    };
    let axes = [plane.x_axis, plane.y_axis, plane.normal()].map(|a| turn.vector(a));
    let Some(onto) = origin_plane_facing(axes[2]) else {
        return;
    };
    let Datum::Plane(target) = onto.datum() else {
        return;
    };
    let target_axes = [target.x_axis, target.y_axis, target.normal()];
    // m[i][k]: the turned plane's axis k in the new plane's axis i (0, ±1).
    let m: [[f64; 3]; 3] =
        std::array::from_fn(|i| std::array::from_fn(|k| mitcad_vec3::dot(axes[k], target_axes[i])));
    let apply = |p: Vec3| -> Vec3 {
        std::array::from_fn(|i| {
            (0..3)
                .filter(|&k| m[i][k] != 0.0)
                .map(|k| if m[i][k] > 0.0 { p[k] } else { -p[k] })
                .sum::<f64>()
                + 0.0
        })
    };
    let own = sketch.frame.as_deref().copied().unwrap_or(FrameDef {
        origin: [0.0; 3],
        x_axis: [1.0, 0.0, 0.0],
        y_axis: [0.0, 1.0, 0.0],
    });
    let frame = FrameDef {
        origin: apply(own.origin),
        x_axis: apply(own.x_axis),
        y_axis: apply(own.y_axis),
    };
    let identity = frame.origin == [0.0; 3]
        && frame.x_axis == [1.0, 0.0, 0.0]
        && frame.y_axis == [0.0, 1.0, 0.0];
    sketch.plane = match onto {
        OriginDatum::Xz => SketchPlane::Xz,
        OriginDatum::Yz => SketchPlane::Yz,
        _ => SketchPlane::Xy,
    };
    sketch.frame = (!identity).then(|| Box::new(frame));
}

/// The joint origins a definition names, in [`joint_origins_mut`]'s order.
fn joint_origins(def: &FeatureDef) -> Vec<JointOrigin> {
    let mut copy = def.clone();
    joint_origins_mut(&mut copy)
        .into_iter()
        .map(|o| o.clone())
        .collect()
}

/// The joint origins a definition names, to change.
fn joint_origins_mut(def: &mut FeatureDef) -> Vec<&mut JointOrigin> {
    match def {
        FeatureDef::Joint(joint) => vec![&mut joint.a, &mut joint.b],
        FeatureDef::AsBuiltJoint(joint) => joint.origin.iter_mut().collect(),
        FeatureDef::ConstraintSet(set) => set
            .rows
            .iter_mut()
            .flat_map(|row| {
                [
                    Some(&mut row.a),
                    row.a2.as_mut(),
                    Some(&mut row.b),
                    row.b2.as_mut(),
                ]
                .into_iter()
                .flatten()
            })
            .collect(),
        FeatureDef::MotionLink(link) => link.coupling.origins_mut(),
        _ => Vec::new(),
    }
}

/// A section analysis's plane, in the component its occurrence ends in.
/// Whether it changed.
pub(crate) fn turn_analysis(def: &mut AnalysisDef, ctx: &Turned<'_>) -> bool {
    match def {
        AnalysisDef::Section(section) => {
            let end = |occurrence: &Option<OccurrencePath>| match occurrence {
                Some(path) => ctx
                    .end(ComponentUid::ROOT, path)
                    .unwrap_or(ComponentUid::ROOT),
                None => ComponentUid::ROOT,
            };
            let turned = ctx.of(end(&section.occurrence)).reference(&section.plane);
            let mut changed = turned != section.plane;
            section.plane = turned;
            // The axis the plane turns about (mitcad#823), in its own
            // occurrence's component.
            let component = end(&section.axis_occurrence);
            if let Some(axis) = &mut section.axis {
                let turned = ctx.of(component).reference(axis);
                changed |= turned != *axis;
                *axis = turned;
            }
            changed
        }
        // A bed face is a face by its name, which a turn keeps.
        AnalysisDef::Print(_) => false,
    }
}

/// The lights placed in the design (not those that follow the camera).
/// Returns how many turned.
pub(crate) fn turn_lights(render: &mut RenderSettings, turn: Turn) -> usize {
    let mut count = 0;
    for light in &mut render.lights {
        if light.space == LightSpace::World {
            light.position = turn.vector(light.position);
            light.direction = turn.vector(light.direction);
            count += 1;
        }
    }
    count
}

/// What split features remember of their pieces: centres of mass in their
/// components' coordinates.
pub(crate) fn turn_split_pieces(
    state: &mut crate::DocState,
    turn: Turn,
    turned: &BTreeSet<ComponentUid>,
) {
    let components: BTreeMap<FeatureUid, ComponentUid> = state
        .features
        .iter()
        .map(|f| (f.uid, f.component))
        .collect();
    for (feature, groups) in &mut state.split_pieces {
        if !components.get(feature).is_some_and(|c| turned.contains(c)) {
            continue;
        }
        let mut changed = (**groups).clone();
        for group in &mut changed {
            for piece in &mut group.pieces {
                piece.centre = turn.vector(piece.centre);
            }
        }
        *groups = std::sync::Arc::new(changed);
    }
}

/// The origin plane whose frame has exactly these axes.
pub(crate) fn origin_plane_with(x: Vec3, y: Vec3) -> Option<OriginDatum> {
    [OriginDatum::Xy, OriginDatum::Xz, OriginDatum::Yz]
        .into_iter()
        .find(|origin| match origin.datum() {
            Datum::Plane(plane) => plane.x_axis == x && plane.y_axis == y,
            _ => false,
        })
}

/// The origin axis along exactly this direction.
fn origin_axis_along(direction: Vec3) -> Option<OriginDatum> {
    [OriginDatum::X, OriginDatum::Y, OriginDatum::Z]
        .into_iter()
        .find(|origin| match origin.datum() {
            Datum::Axis(axis) => axis.direction == direction,
            _ => false,
        })
}

/// The origin plane that holds a plane through the origin with this
/// (axis-aligned) normal, either way round.
pub(crate) fn origin_plane_facing(normal: Vec3) -> Option<OriginDatum> {
    [OriginDatum::Xy, OriginDatum::Xz, OriginDatum::Yz]
        .into_iter()
        .find(|origin| match origin.datum() {
            Datum::Plane(plane) => {
                let n = plane.normal();
                n == normal || n.map(|v| -v + 0.0) == normal.map(|v| v + 0.0)
            }
            _ => false,
        })
}

/// New numbers of the pieces of split faces (`#k`) and of the edges between
/// the same two faces (`E{…}#k`), which the kernel numbers in a geometric
/// order: a turn changes it, so references to them name other pieces in the
/// turned design until they are renamed (mitcad#537). Keyed by the names as
/// the definitions have them.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct PieceRenames {
    pub faces: BTreeMap<String, FaceName>,
    pub edges: BTreeMap<String, EdgeName>,
}

impl PieceRenames {
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty() && self.edges.is_empty()
    }

    /// The face as it is named after the renames: itself renamed, or the
    /// names in its key (a fillet's face is named after its edge).
    pub fn face(&self, face: &FaceName) -> FaceName {
        if let Some(renamed) = self.faces.get(&face.to_string()) {
            return renamed.clone();
        }
        let key = match &face.key {
            Some(RoleKey::Edge(edge)) => Some(RoleKey::Edge(Box::new(self.edge(edge)))),
            Some(RoleKey::Vertex(vertex)) => Some(RoleKey::Vertex(Box::new(self.vertex(vertex)))),
            // A derived part's face names its source's face, in the source's
            // own names (mitcad#375).
            Some(RoleKey::Face(inner)) if face.role != "part" => {
                Some(RoleKey::Face(Box::new(self.face(inner))))
            }
            other => other.clone(),
        };
        FaceName {
            key,
            ..face.clone()
        }
    }

    pub fn edge(&self, edge: &EdgeName) -> EdgeName {
        if let Some(renamed) = self.edges.get(&edge.to_string()) {
            return renamed.clone();
        }
        let [a, b] = edge.faces();
        let renamed = EdgeName::new(self.face(a), self.face(b));
        match edge.index {
            Some(k) => renamed.with_index(k),
            None => renamed,
        }
    }

    pub fn vertex(&self, vertex: &VertexName) -> VertexName {
        let faces = vertex.faces().iter().map(|f| self.face(f));
        match VertexName::new(faces) {
            Some(renamed) => match vertex.index {
                Some(k) => renamed.with_index(k),
                None => renamed,
            },
            None => vertex.clone(),
        }
    }

    /// The renamed text of a topological name; None for text that is no
    /// name or a name the renames leave as it is.
    fn text(&self, text: &str) -> Option<String> {
        if !text.contains(':') {
            return None;
        }
        let renamed = match text.parse::<TopoName>().ok()? {
            TopoName::Face(face) => self.face(&face).to_string(),
            TopoName::Edge(edge) => self.edge(&edge).to_string(),
            TopoName::Vertex(vertex) => self.vertex(&vertex).to_string(),
        };
        (renamed != text).then_some(renamed)
    }
}

/// Every string of a definition's JSON form (parameters as markers), for
/// `visit` to read or change; the definition back when it changed one.
fn visit_strings(
    def: &FeatureDef,
    visit: &mut dyn FnMut(&mut String) -> bool,
) -> Result<Option<FeatureDef>, String> {
    // A base feature holds bodies and no references.
    if matches!(def, FeatureDef::Base(_)) {
        return Ok(None);
    }
    let mut slots = Vec::new();
    let marked = def
        .map_params(&mut |_, id: &ParamId| {
            slots.push(*id);
            Ok::<_, ()>(format!("{PARAM}{}", slots.len() - 1))
        })
        .expect("never fails");
    let mut value = serde_json::to_value(&marked).map_err(|e| e.to_string())?;
    fn walk(value: &mut serde_json::Value, visit: &mut dyn FnMut(&mut String) -> bool) -> bool {
        // Every string is visited, also after one that changed.
        let mut changed = false;
        match value {
            serde_json::Value::String(text) => changed = visit(text),
            serde_json::Value::Array(items) => {
                for item in items {
                    changed |= walk(item, visit);
                }
            }
            serde_json::Value::Object(map) => {
                for item in map.values_mut() {
                    changed |= walk(item, visit);
                }
            }
            _ => {}
        }
        changed
    }
    if !walk(&mut value, visit) {
        return Ok(None);
    }
    let changed: FeatureDef<String> = serde_json::from_value(value).map_err(|e| e.to_string())?;
    changed
        .map_params(&mut |slot, marker: &String| {
            marker
                .strip_prefix(PARAM)
                .and_then(|i| i.parse::<usize>().ok())
                .and_then(|i| slots.get(i).copied())
                .ok_or_else(|| format!("{slot} lost its parameter"))
        })
        .map(Some)
}

/// The names a definition refers to that a geometric order numbers: faces
/// that are pieces of split faces, on their own or in edges and vertices,
/// and edges numbered among those between the same faces; with the bodies
/// it names.
pub(crate) fn numbered_names(def: &FeatureDef) -> (Vec<TopoName>, Vec<BodyUid>) {
    let mut names = Vec::new();
    let mut bodies = Vec::new();
    let _ = visit_strings(def, &mut |text| {
        if let Ok(body) = text.parse::<BodyUid>() {
            if !bodies.contains(&body) {
                bodies.push(body);
            }
        } else if text.contains('#')
            && text.contains(':')
            && let Ok(name) = text.parse::<TopoName>()
        {
            let numbered = match &name {
                TopoName::Face(face) => !face.split.is_empty(),
                TopoName::Edge(edge) => {
                    edge.index.is_some() || edge.faces().iter().any(|f| !f.split.is_empty())
                }
                TopoName::Vertex(vertex) => vertex.faces().iter().any(|f| !f.split.is_empty()),
            };
            if numbered && !names.contains(&name) {
                names.push(name);
            }
        }
        false
    });
    if names.is_empty() {
        bodies.clear();
    }
    (names, bodies)
}

/// The definition with the renames applied to every name it refers to;
/// None when it names nothing they rename.
pub(crate) fn rename_pieces(
    def: &FeatureDef,
    renames: &PieceRenames,
) -> Result<Option<FeatureDef>, String> {
    if renames.is_empty() {
        return Ok(None);
    }
    visit_strings(def, &mut |text| match renames.text(text) {
        Some(renamed) => {
            *text = renamed;
            true
        }
        None => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarter_turns_follow_the_right_hand_rule() {
        let x = Turn::about(TurnAxis::X, 1);
        assert_eq!(x.vector([1.0, 2.0, 3.0]), [1.0, -3.0, 2.0]);
        let y = Turn::about(TurnAxis::Y, 1);
        assert_eq!(y.vector([1.0, 2.0, 3.0]), [3.0, 2.0, -1.0]);
        let z = Turn::about(TurnAxis::Z, 1);
        assert_eq!(z.vector([1.0, 2.0, 3.0]), [-2.0, 1.0, 3.0]);
        for axis in [TurnAxis::X, TurnAxis::Y, TurnAxis::Z] {
            assert_eq!(Turn::about(axis, 4), Turn::IDENTITY);
            assert_eq!(Turn::about(axis, 3), Turn::about(axis, 1).inverse());
            let two = Turn::about(axis, 2);
            assert_eq!(two.after(&two), Turn::IDENTITY);
            // The matrix is the rotation's.
            let angle = std::f64::consts::FRAC_PI_2;
            let direction = match axis {
                TurnAxis::X => [1.0, 0.0, 0.0],
                TurnAxis::Y => [0.0, 1.0, 0.0],
                TurnAxis::Z => [0.0, 0.0, 1.0],
            };
            let exact = Transform::rotation([0.0; 3], direction, angle).unwrap();
            let turn = Turn::about(axis, 1).matrix();
            for (exact, turn) in exact.linear.iter().flatten().zip(turn.iter().flatten()) {
                assert!((exact - turn).abs() < 1e-15);
            }
        }
    }

    #[test]
    fn transforms_turn_in_both_coordinates() {
        let turn = Turn::about(TurnAxis::X, 1);
        let t = Transform::rotation([5.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.3).unwrap();
        let conjugated = turn.conjugate(&t);
        let p = [1.0, 2.0, 3.0];
        let expected = turn.vector(t.apply_point(turn.inverse().vector(p)));
        let got = conjugated.apply_point(p);
        for i in 0..3 {
            assert!((expected[i] - got[i]).abs() < 1e-12, "{expected:?} {got:?}");
        }
        let before = turn.before(&t).apply_point(p);
        let expected = turn.vector(t.apply_point(p));
        assert_eq!(before, expected);
    }

    #[test]
    fn origin_planes_become_their_turned_counterparts() {
        let turn = Turn::about(TurnAxis::X, 1);
        // XZ (normal +Y, y = -Z) turns onto XY exactly.
        assert_eq!(
            turn.reference(&GeomRef::Origin(OriginDatum::Xz)),
            GeomRef::Origin(OriginDatum::Xy)
        );
        // XY faces -Y: a fixed plane with XZ's other side.
        assert_eq!(
            turn.reference(&GeomRef::Origin(OriginDatum::Xy)),
            GeomRef::FixedPlane {
                origin: [0.0; 3],
                normal: [0.0, -1.0, 0.0],
                x_axis: Some([1.0, 0.0, 0.0])
            }
        );
        // The Z axis runs along -Y; Y along +Z.
        assert_eq!(
            turn.reference(&GeomRef::Origin(OriginDatum::Z)),
            GeomRef::FixedAxis {
                origin: [0.0; 3],
                direction: [0.0, -1.0, 0.0]
            }
        );
        assert_eq!(
            turn.reference(&GeomRef::Origin(OriginDatum::Y)),
            GeomRef::Origin(OriginDatum::Z)
        );
        for plane in [OriginDatum::Xy, OriginDatum::Xz, OriginDatum::Yz] {
            let onto = Turn::plane_onto_xy(plane).unwrap();
            assert_eq!(
                onto.reference(&GeomRef::Origin(plane)),
                GeomRef::Origin(OriginDatum::Xy)
            );
        }
    }
}
