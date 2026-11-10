// SPDX-License-Identifier: MIT
//! Mount frames of any part and placing a part by one (mitcad#525;
//! `commands.md`, "Mount frames"; the library side in
//! `docs/libraries.md`, *Mount frames*).
//!
//! A mount frame is a joint origin with a `mount` (or named
//! `mount:<name>`, as BUILD's parts name theirs): where the part lies on
//! a face, goes into a hole or onto a shaft, takes other parts on a grid
//! of cells, or stands on one. Inserting a part keeps its frames as joint
//! origins of its component, so that Joint finds them ready: a copy has
//! the design's own, a linked part gets joint origins on fixed planes
//! where the design has them (read again with its bodies).
//!
//! Placing a part by a frame (`insert_component` with `mount`) snaps the
//! frame onto what was picked, face to face (the joint is flipped), and
//! makes the joint the frame asks for: rigid, planar (free on the face),
//! revolute, cylindrical or a slider along the axis, its free motions
//! driven where the part was placed, so that it moves with what it stands
//! on and a drag still moves it as the joint lets it.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::FRAC_PI_2;
use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;

use super::{ComponentCopy, DocState, Document, ModelError, invalid};
use crate::assembly::{Assembly, inverse};
use crate::build::value_in;
use crate::datum::{Datum, SurfaceGeometry, Vec3};
use crate::expr::Unit;
use crate::features::geom_ref::{GeomRef, Resolver};
use crate::features::joint::{
    FrameOverride, JointDef, JointOrigin, JointOriginDef, MOUNT_PREFIX, MountKind, MountSpec,
    OccurrencePath, mount_of,
};
use crate::features::{FeatureDef, ValueInput};
use crate::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use crate::joints::{
    JointKind, SlideAxis, alignment, apply_override, find_path_from, frame, frame_parts,
    origin_frame, path_component,
};
use crate::kernel::{Cylinder, Kernel};
use crate::recompute::{Cache, Recomputed, recompute};
use crate::transform::Transform;
use mitcad_vec3::dot;
use mitcad_vec3::{add, scale, unit};

/// A mount frame of a design: its joint origin and where it is in the
/// design's root coordinates.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceMount {
    pub spec: MountSpec,
    pub feature: FeatureUid,
    pub frame: Transform,
}

/// A mount frame of a placed part: the joint origin and the occurrences
/// from the part's component to the one it is in (empty: the part's own).
#[derive(Debug, Clone, PartialEq)]
pub struct MountFrame {
    pub spec: MountSpec,
    pub path: Vec<OccurrenceUid>,
    pub feature: FeatureUid,
}

/// What an insert made (mitcad#525): the component, its occurrence, its
/// mount frames and, placed by one, the joint and the plan it followed.
#[derive(Debug, Clone, PartialEq)]
pub struct Inserted {
    pub component: ComponentUid,
    pub occurrence: OccurrenceUid,
    pub joint: Option<FeatureUid>,
    pub frames: Vec<MountFrame>,
    pub plan: Option<MountPlan>,
    /// Inserted with options (mitcad#801): the constants the source's rules
    /// moved, and the rule a bound value breaks (the part fails with it).
    pub moves: Vec<crate::part_options::OptionMove>,
    pub broken: Option<crate::part_options::BrokenRule>,
}

/// Where `insert_component`'s `mount` places the part: by its frame
/// `frame` (the first when left out) on what `at` picked, turned by
/// `rotate` about the frame's z and lifted by `offset` along it; `flip`
/// turns the target over (a plane's other side, a hole's other end).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MountInput {
    #[serde(default)]
    pub frame: Option<String>,
    pub at: MountAt,
    #[serde(default)]
    pub rotate: Option<ValueInput>,
    #[serde(default)]
    pub offset: Option<ValueInput>,
    #[serde(default)]
    pub flip: bool,
}

/// What a mount placement picked: geometry of an occurrence (a path from
/// the active component, uids or names; empty: its own geometry), where it
/// was clicked and the way towards the viewer, both in the active
/// component's coordinates.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MountAt {
    #[serde(default)]
    pub occurrence: String,
    pub geometry: GeomRef,
    #[serde(default)]
    pub point: Option<Vec3>,
    #[serde(default)]
    pub toward: Option<Vec3>,
}

/// A grid placement: the grid's joint origin and the occurrences from the
/// active component to its component, its cells, and the cells the part
/// takes from `cell` (counted from 0 at the grid's origin along its x and
/// y), its footprint as turned by `turns` quarter turns.
#[derive(Debug, Clone, PartialEq)]
pub struct GridPlace {
    pub feature: FeatureUid,
    pub path: Vec<OccurrenceUid>,
    pub pitch: [f64; 2],
    pub cells: [u32; 2],
    pub cell: [u32; 2],
    pub footprint: [u32; 2],
    pub turns: u32,
}

/// What a mount placement does: the part's frame, the joint it makes and
/// its side b (the target), where the frame goes and where the new
/// occurrence is then placed (both in the active component).
#[derive(Debug, Clone, PartialEq)]
pub struct MountPlan {
    pub frame: String,
    pub kind: MountKind,
    pub joint: JointKind,
    pub target: JointOrigin,
    /// The target's name: the occurrence path's, or the component's.
    pub target_name: String,
    pub at: Transform,
    pub placement: Transform,
    pub rotate: Option<ValueInput>,
    pub offset: Option<ValueInput>,
    pub grid: Option<GridPlace>,
}

/// A mount frame of the design at the marker below a component: the
/// frame, its joint origin's frame in its own component and in the
/// component asked for.
#[derive(Debug, Clone)]
pub struct FrameAt {
    pub frame: MountFrame,
    pub local: Transform,
    pub at: Transform,
}

/// What a placement's preview needs (`mount_preview`): the part's name,
/// its frames in its own coordinates, the plan (or why it is refused) and
/// where the active component is in the design.
#[derive(Debug, Clone)]
pub struct MountPreview {
    pub name: String,
    pub frames: Vec<(MountSpec, Transform)>,
    pub plan: Option<Result<MountPlan, String>>,
    pub host: Transform,
}

/// The part a mount placement's preview reads: a library's component or a
/// design file.
#[derive(Debug, Clone, Copy)]
pub enum MountSource<'a> {
    Library(&'a super::LibraryPart),
    File {
        path: &'a str,
        base: Option<&'a Path>,
    },
}

/// The mount frames of a design before its marker, each name once (the
/// first): its joint origin and its mount, without evaluating anything.
pub(crate) fn listed_mounts(state: &DocState) -> Vec<(FeatureUid, MountSpec)> {
    let mut seen = BTreeSet::new();
    let end = state.marker.min(state.features.len());
    state.features[..end]
        .iter()
        .filter(|entry| !entry.suppressed)
        .filter_map(|entry| match &entry.def {
            FeatureDef::JointOrigin(def) => mount_of(&entry.name, def).map(|s| (entry.uid, s)),
            _ => None,
        })
        .filter(|(_, spec)| seen.insert(spec.name.clone()))
        .collect()
}

/// The mount frames of an evaluated design, each name once, with their
/// frames in the design's root coordinates (through the first placement
/// of a frame's component).
pub(crate) fn evaluated_mounts<S>(state: &DocState, result: &Recomputed<S>) -> Vec<SourceMount> {
    let placement = |o: OccurrenceUid| {
        result
            .placements
            .get(&o)
            .copied()
            .unwrap_or(Transform::IDENTITY)
    };
    let mut out = Vec::new();
    for (uid, spec) in listed_mounts(state) {
        let Some(entry) = state.entry(uid) else {
            continue;
        };
        let Some(Datum::Plane(plane)) = result.datums.get(&uid).map(|d| d.value) else {
            continue;
        };
        let local = frame(plane.origin, plane.x_axis, plane.normal());
        let at = if entry.component.is_root() {
            local
        } else {
            let Some(path) = state.assembly.paths_to(entry.component).into_iter().next() else {
                continue;
            };
            path.iter()
                .fold(Transform::IDENTITY, |t, o| t.after(&placement(*o)))
                .after(&local)
        };
        out.push(SourceMount {
            spec,
            feature: uid,
            frame: at,
        });
    }
    out
}

/// The occurrences from component `from` down to `to` (the first way);
/// empty when they are the same, None when `to` is not below `from`.
fn path_down(
    assembly: &Assembly,
    from: ComponentUid,
    to: ComponentUid,
) -> Option<Vec<OccurrenceUid>> {
    fn walk(
        assembly: &Assembly,
        at: ComponentUid,
        to: ComponentUid,
        path: &mut Vec<OccurrenceUid>,
    ) -> bool {
        if at == to {
            return true;
        }
        if path.len() >= 64 {
            return false;
        }
        let children: Vec<(OccurrenceUid, ComponentUid)> = assembly
            .children(at)
            .map(|o| (o.uid, o.component))
            .collect();
        for (uid, component) in children {
            if path.contains(&uid) {
                continue;
            }
            path.push(uid);
            if walk(assembly, component, to, path) {
                return true;
            }
            path.pop();
        }
        false
    }
    let mut path = Vec::new();
    walk(assembly, from, to, &mut path).then_some(path)
}

/// A fixed plane with a frame's origin, z and x.
fn fixed_plane(at: &Transform) -> GeomRef {
    let [origin, x_axis, _, z_axis] = frame_parts(at);
    GeomRef::FixedPlane {
        origin,
        normal: z_axis,
        x_axis: Some(x_axis),
    }
}

/// A linked part's joint origin for a frame of its design.
fn fixed_origin(mount: &SourceMount) -> JointOriginDef<ValueInput> {
    JointOriginDef {
        geometry: fixed_plane(&mount.frame),
        frame_override: None,
        offset: None,
        angle: None,
        flip: false,
        mount: Some(mount.spec.clone()),
    }
}

/// A free feature name for a frame's joint origin: `mount:<name>`, else
/// with a number.
fn frame_feature_name(taken: &mut BTreeSet<String>, spec: &MountSpec) -> String {
    crate::exchange::unique_name(&format!("{MOUNT_PREFIX}{}", spec.name), taken)
}

/// Adds a linked part's mount frames to its component: a joint origin on
/// a fixed plane for each frame of its design, with its mount.
pub(crate) fn add_linked_mounts(
    state: &mut DocState,
    component: ComponentUid,
    mounts: &[SourceMount],
) -> Result<Vec<MountFrame>, ModelError> {
    let mut taken: BTreeSet<String> = state.features.iter().map(|f| f.name.clone()).collect();
    let mut frames = Vec::with_capacity(mounts.len());
    for mount in mounts {
        let name = frame_feature_name(&mut taken, &mount.spec);
        let def = FeatureDef::JointOrigin(fixed_origin(mount));
        let added = state.add_feature_in(&def, Some(&name), component)?;
        frames.push(MountFrame {
            spec: mount.spec.clone(),
            path: Vec::new(),
            feature: added.uid,
        });
    }
    Ok(frames)
}

/// A linked part's mount frames after its design or row changed: each
/// frame of the same name takes the new place, a new one is added; one the
/// design no longer has stays where it was (joints may use it).
pub(crate) fn update_linked_mounts(
    state: &mut DocState,
    component: ComponentUid,
    mounts: &[SourceMount],
) -> Result<(), ModelError> {
    let mut have: BTreeMap<String, FeatureUid> = BTreeMap::new();
    for entry in state.features.iter().filter(|e| e.component == component) {
        if let FeatureDef::JointOrigin(def) = &entry.def
            && let Some(spec) = &def.mount
        {
            have.entry(spec.name.clone()).or_insert(entry.uid);
        }
    }
    let mut added = Vec::new();
    for mount in mounts {
        match have.get(&mount.spec.name) {
            Some(uid) => {
                let position = state.require(*uid)?;
                let mut entry = (*state.features[position]).clone();
                if let FeatureDef::JointOrigin(def) = &mut entry.def {
                    def.geometry = fixed_plane(&mount.frame);
                    def.mount = Some(mount.spec.clone());
                }
                state.features[position] = Arc::new(entry);
            }
            None => added.push(mount.clone()),
        }
    }
    if !added.is_empty() {
        add_linked_mounts(state, component, &added)?;
    }
    Ok(())
}

/// A copied part's mount frames: the copies of the design's joint
/// origins, each with its mount (a frame named `mount:<name>` gets one, as
/// a copy's features take new names) and named after it.
pub(crate) fn copied_mounts(
    state: &mut DocState,
    made: ComponentUid,
    listed: &[(FeatureUid, MountSpec)],
    copy: &ComponentCopy,
) -> Vec<MountFrame> {
    let mut taken: BTreeSet<String> = state.features.iter().map(|f| f.name.clone()).collect();
    let mut frames = Vec::new();
    for (source, spec) in listed {
        let Some(&uid) = copy.features.get(source) else {
            continue;
        };
        let Some(position) = state.position(uid) else {
            continue;
        };
        let mut entry = (*state.features[position]).clone();
        let FeatureDef::JointOrigin(def) = &mut entry.def else {
            continue;
        };
        if def.mount.is_none() {
            def.mount = Some(spec.clone());
        }
        taken.remove(&entry.name);
        entry.name = frame_feature_name(&mut taken, spec);
        let component = entry.component;
        state.features[position] = Arc::new(entry);
        let Some(path) = path_down(&state.assembly, made, component) else {
            continue;
        };
        frames.push(MountFrame {
            spec: spec.clone(),
            path,
            feature: uid,
        });
    }
    frames
}

/// Makes a placement's joint in `host` between the new occurrence's frame
/// and the target, flipped (the frames face each other), its free motions
/// driven at 0 where the plan placed the part, and places the occurrence
/// there.
pub(crate) fn attach_mount(
    state: &mut DocState,
    host: ComponentUid,
    occurrence: OccurrenceUid,
    frames: &[MountFrame],
    plan: &MountPlan,
) -> Result<FeatureUid, ModelError> {
    let own = frames
        .iter()
        .find(|f| f.spec.name == plan.frame)
        .ok_or_else(|| invalid(format!("the new part has no mount frame {}", plan.frame)))?;
    let mut path = vec![occurrence];
    path.extend(own.path.iter().copied());
    let position = plan
        .joint
        .motions(SlideAxis::Z)
        .into_iter()
        .map(|m| (m, ValueInput::Number(0.0)))
        .collect();
    let nonzero = |v: &Option<ValueInput>| {
        v.clone()
            .filter(|v| !matches!(v, ValueInput::Number(n) if n.abs() < 1e-12))
    };
    let def = JointDef {
        kind: plan.joint,
        slide_axis: SlideAxis::Z,
        a: JointOrigin {
            occurrence: OccurrencePath(path),
            geometry: GeomRef::Datum(own.feature),
            frame_override: None,
        },
        b: plan.target.clone(),
        offset: nonzero(&plan.offset),
        angle: nonzero(&plan.rotate),
        flip: true,
        limits: BTreeMap::new(),
        position,
    };
    if let Some(o) = state.assembly.occurrence_mut(occurrence) {
        o.transform = plan.placement;
    }
    Ok(state
        .add_feature_in(&FeatureDef::Joint(def), None, host)?
        .uid)
}

/// The cells of a block from `cell`, `size` cells each way.
fn block(cell: [u32; 2], size: [u32; 2]) -> Vec<[u32; 2]> {
    (cell[0]..cell[0] + size[0])
        .flat_map(|i| (cell[1]..cell[1] + size[1]).map(move |j| [i, j]))
        .collect()
}

/// The first cell of a block of `size` cells centred at `along` (mm from
/// the grid's origin), within `cells`.
fn first_cell(along: f64, pitch: f64, size: u32, cells: u32) -> u32 {
    let first = (along / pitch - f64::from(size) / 2.0).round();
    first.clamp(0.0, f64::from(cells.saturating_sub(size))) as u32
}

/// Quarter turns of an angle (radians), when it is a whole number of them.
fn quarter_turns(angle: f64) -> Option<u32> {
    let quarters = angle / FRAC_PI_2;
    let whole = quarters.round();
    ((quarters - whole).abs() < 1e-6).then(|| (whole as i64).rem_euclid(4) as u32)
}

/// "(1, 0)", "(1, 0) and (2, 0)", "(0, 0), (1, 0) and (2, 0)".
fn cells_text(cells: &[[u32; 2]]) -> String {
    let parts: Vec<String> = cells
        .iter()
        .map(|c| format!("({}, {})", c[0], c[1]))
        .collect();
    and_list(&parts)
}

fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn number(value: f64) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    if rounded == rounded.trunc() {
        format!("{rounded:.0}")
    } else {
        format!("{rounded}")
    }
}

/// What a target gives a frame: a plane, an axis (a cylinder's when it is
/// a cylindrical face) or nothing a part mounts on.
enum TargetClass {
    Plane,
    Axis(Option<Cylinder>),
    Other(String),
}

/// A placement's context: the part, what was picked, the inputs' values.
struct Aim<'a> {
    label: &'a str,
    host: ComponentUid,
    path: Vec<OccurrenceUid>,
    target: ComponentUid,
    target_name: String,
    /// The click and the way towards the viewer, in the target
    /// component's coordinates.
    point: Option<Vec3>,
    toward: Option<Vec3>,
    rotate: f64,
    flip: bool,
}

impl<K: Kernel> Document<K> {
    /// A design's mount frames, evaluated.
    pub(super) fn design_mounts(&self, source: &DocState) -> Result<Vec<SourceMount>, ModelError> {
        let result = recompute(
            &self.kernel,
            source,
            &mut Cache::default(),
            self.monitor.as_ref(),
        )?;
        Ok(evaluated_mounts(source, &result))
    }

    /// The mount frames at the marker in `component` and the components
    /// placed in it, with their frames there.
    pub fn frames_under(&self, component: ComponentUid) -> Vec<FrameAt> {
        let a = &self.state.assembly;
        let end = self.state.marker.min(self.state.features.len());
        let mut out = Vec::new();
        for entry in &self.state.features[..end] {
            if entry.suppressed {
                continue;
            }
            let FeatureDef::JointOrigin(def) = &entry.def else {
                continue;
            };
            let Some(spec) = mount_of(&entry.name, def) else {
                continue;
            };
            let Some(path) = path_down(a, component, entry.component) else {
                continue;
            };
            let Some(Datum::Plane(plane)) = self.datum(entry.uid) else {
                continue;
            };
            let local = frame(plane.origin, plane.x_axis, plane.normal());
            out.push(FrameAt {
                at: self.path_transform(&path).after(&local),
                local,
                frame: MountFrame {
                    spec,
                    path,
                    feature: entry.uid,
                },
            });
        }
        out
    }

    /// What a pick is for a mount: a plane, an axis or neither.
    fn target_class(&self, geometry: &GeomRef) -> TargetClass {
        let of_datum = |datum: Datum| match datum {
            Datum::Plane(_) => TargetClass::Plane,
            Datum::Axis(_) => TargetClass::Axis(None),
            Datum::Point(_) => TargetClass::Other("a point".to_owned()),
        };
        match geometry {
            GeomRef::Origin(origin) => of_datum(origin.datum()),
            GeomRef::Datum(uid) => match self.datum(*uid) {
                Some(datum) => of_datum(datum),
                None => TargetClass::Other(format!("{uid}, which has no result at the marker")),
            },
            GeomRef::FixedPlane { .. } | GeomRef::Mount { .. } => TargetClass::Plane,
            GeomRef::FixedAxis { .. } => TargetClass::Axis(None),
            GeomRef::Face { body, face } => {
                let mut r = self.resolver();
                let shape = match r.shape(*body) {
                    Ok(shape) => shape,
                    Err(e) => return TargetClass::Other(e),
                };
                let face = match crate::features::renamed::face_for(&mut r, &shape, face) {
                    Ok(face) => face,
                    Err(e) => return TargetClass::Other(e),
                };
                match self.kernel().face_geometry(&shape, &face) {
                    Ok(SurfaceGeometry::Plane { .. }) => TargetClass::Plane,
                    Ok(SurfaceGeometry::Cylinder { .. } | SurfaceGeometry::Cone { .. }) => {
                        TargetClass::Axis(self.kernel().face_cylinder(&shape, &face).ok())
                    }
                    Ok(other) => TargetClass::Other(format!("a {} face", other.kind())),
                    Err(e) => TargetClass::Other(crate::notice::about_place(
                        crate::notice::NoticePlace::face(face.to_string()),
                        &e.to_string(),
                    )),
                }
            }
            GeomRef::Edge { body, edge } => {
                let mut r = self.resolver();
                let shape = match r.shape(*body) {
                    Ok(shape) => shape,
                    Err(e) => return TargetClass::Other(e),
                };
                let edge = match crate::features::renamed::edge_for(&mut r, &shape, edge) {
                    Ok(edge) => edge,
                    Err(e) => return TargetClass::Other(e),
                };
                match self.kernel().edge_geometry(&shape, &edge) {
                    Ok(crate::datum::CurveGeometry::Circle { .. }) => TargetClass::Axis(None),
                    Ok(other) => TargetClass::Other(format!("a {} edge", other.kind())),
                    Err(e) => TargetClass::Other(crate::notice::about_place(
                        crate::notice::NoticePlace::edge(edge.to_string()),
                        &e.to_string(),
                    )),
                }
            }
            other => TargetClass::Other(format!("{other}")),
        }
    }

    /// The component a geometry reference belongs to, when it names one.
    fn owner_of(&self, geometry: &GeomRef) -> Option<ComponentUid> {
        match geometry {
            GeomRef::Face { body, .. }
            | GeomRef::Edge { body, .. }
            | GeomRef::Vertex { body, .. }
            | GeomRef::Body(body) => self.body_component(*body),
            GeomRef::Datum(uid)
            | GeomRef::SketchCurves { sketch: uid, .. }
            | GeomRef::SketchPoint { sketch: uid, .. }
            | GeomRef::Mount { feature: uid, .. } => self.feature(*uid).map(|f| f.component),
            _ => None,
        }
    }

    /// Plans a placement of a part with mount frames `mounts` (its
    /// design's, in its coordinates) by `input`; refused with the reason
    /// when the pick does not take the frame, a grid's cells are taken or
    /// the values cannot be.
    pub(crate) fn plan_mount(
        &self,
        mounts: &[SourceMount],
        input: &MountInput,
        label: &str,
    ) -> Result<MountPlan, ModelError> {
        let names = || {
            let names: Vec<String> = mounts.iter().map(|m| m.spec.name.clone()).collect();
            and_list(&names)
        };
        let own = match &input.frame {
            Some(name) => mounts
                .iter()
                .find(|m| m.spec.name == *name)
                .ok_or_else(|| {
                    invalid(if mounts.is_empty() {
                        format!("{label} has no mount frames")
                    } else {
                        format!(
                            "{label} has no mount frame {name} (its frames: {})",
                            names()
                        )
                    })
                })?,
            None => mounts
                .first()
                .ok_or_else(|| invalid(format!("{label} has no mount frames to place it by")))?,
        };
        let a = &self.state.assembly;
        let host = a.active;
        let path = find_path_from(a, host, &input.at.occurrence).ok_or_else(|| {
            invalid(format!(
                "no occurrence {} in {}",
                input.at.occurrence,
                a.name(host)
            ))
        })?;
        let target = path_component(a, host, &path).map_err(invalid)?;
        if let Some(owner) = self.owner_of(&input.at.geometry)
            && owner != target
        {
            return Err(invalid(format!(
                "{} is in {}, not in {}",
                input.at.geometry,
                a.name(owner),
                a.name(target)
            )));
        }
        let placement = self.path_transform(&path);
        let back = inverse(&placement);
        let rotate = match &input.rotate {
            Some(value) => value_in(self, value, Unit::DEG)?,
            None => 0.0,
        };
        let offset = match &input.offset {
            Some(value) => value_in(self, value, Unit::MM)?,
            None => 0.0,
        };
        let aim = Aim {
            label,
            host,
            target_name: if path.is_empty() {
                a.name(host)
            } else {
                a.path_name(&path)
            },
            point: input.at.point.map(|p| back.apply_point(p)),
            toward: input.at.toward.and_then(unit).map(|t| back.apply_vector(t)),
            path,
            target,
            rotate,
            flip: input.flip,
        };
        let (b, at_local, grid) = match own.spec.kind {
            MountKind::Face | MountKind::Grid => {
                let (b, at) = self.face_target(&aim, &input.at.geometry)?;
                (b, at, None)
            }
            MountKind::Hole | MountKind::Shaft => {
                let (b, at) = self.axis_target(&aim, &input.at.geometry)?;
                (b, at, None)
            }
            MountKind::Foot => {
                let (b, at, grid) = self.grid_target(&aim, &input.at.geometry, &own.spec)?;
                (b, at, Some(grid))
            }
        };
        let at = placement.after(&at_local);
        let placed = at
            .after(&alignment(rotate, offset, true))
            .after(&inverse(&own.frame));
        Ok(MountPlan {
            frame: own.spec.name.clone(),
            kind: own.spec.kind,
            joint: own.spec.joint,
            target: b,
            target_name: aim.target_name,
            at,
            placement: placed,
            rotate: input.rotate.clone(),
            offset: input.offset.clone(),
            grid,
        })
    }

    /// A face mount's target: a plane (a flat face, a construction or
    /// origin plane, a joint origin), its frame moved in its plane to the
    /// click (`shift`), turned over with `flip`. Returns side b and the
    /// frame in the target component's coordinates.
    fn face_target(
        &self,
        aim: &Aim<'_>,
        geometry: &GeomRef,
    ) -> Result<(JointOrigin, Transform), ModelError> {
        match self.target_class(geometry) {
            TargetClass::Plane => {}
            TargetClass::Axis(_) => {
                return Err(invalid(format!(
                    "{} lies on a face: pick a flat face or a plane, not a round one",
                    aim.label
                )));
            }
            TargetClass::Other(what) => {
                return Err(invalid(format!(
                    "{} lies on a face: pick a flat face or a plane, not {what}",
                    aim.label
                )));
            }
        }
        let local = origin_frame(&mut self.resolver(), geometry)
            .map_err(|e| invalid(format!("{geometry}: {e}")))?;
        let mut over = FrameOverride::default();
        if aim.flip {
            over.z_axis = Some(scale(frame_parts(&local)[3], -1.0));
        }
        let turned = apply_override(&local, &over).map_err(invalid)?;
        if let Some(point) = aim.point {
            let s = inverse(&turned).apply_point(point);
            if s[0].abs() > 1e-9 || s[1].abs() > 1e-9 {
                over.shift = Some([s[0], s[1], 0.0]);
            }
        }
        let at = apply_override(&local, &over).map_err(invalid)?;
        Ok((self.side_b(aim, geometry, over), at))
    }

    /// A hole or shaft mount's target: an axis (a cylindrical face, a
    /// circular edge, an axis), at the end of a cylinder towards the viewer
    /// (`toward`; the other with `flip`) with z out of that end, else at
    /// the edge's centre with z towards the viewer.
    fn axis_target(
        &self,
        aim: &Aim<'_>,
        geometry: &GeomRef,
    ) -> Result<(JointOrigin, Transform), ModelError> {
        let cylinder = match self.target_class(geometry) {
            TargetClass::Axis(cylinder) => cylinder,
            TargetClass::Plane => {
                return Err(invalid(format!(
                    "{} goes into a hole or onto a shaft: pick its round face or its rim",
                    aim.label
                )));
            }
            TargetClass::Other(what) => {
                return Err(invalid(format!(
                    "{} goes into a hole or onto a shaft: pick its round face or its rim, not \
                     {what}",
                    aim.label
                )));
            }
        };
        let local = origin_frame(&mut self.resolver(), geometry)
            .map_err(|e| invalid(format!("{geometry}: {e}")))?;
        let [origin, _, _, z] = frame_parts(&local);
        let towards = |direction: Vec3| aim.toward.is_none_or(|t| dot(direction, t) >= 0.0);
        let (rim, out) = match cylinder {
            Some(c) => {
                let along = unit(c.axis.direction).unwrap_or(z);
                let low = c.axis.origin;
                let high = add(low, scale(along, c.length));
                if towards(along) != aim.flip {
                    (high, along)
                } else {
                    (low, scale(along, -1.0))
                }
            }
            None => {
                if towards(z) != aim.flip {
                    (origin, z)
                } else {
                    (origin, scale(z, -1.0))
                }
            }
        };
        let mut over = FrameOverride::default();
        if dot(out, z) < 1.0 - 1e-9 {
            over.z_axis = Some(out);
        }
        let turned = apply_override(&local, &over).map_err(invalid)?;
        let s = inverse(&turned).apply_point(rim);
        if s.iter().any(|v| v.abs() > 1e-9) {
            over.shift = Some(s);
        }
        let at = apply_override(&local, &over).map_err(invalid)?;
        Ok((self.side_b(aim, geometry, over), at))
    }

    fn side_b(&self, aim: &Aim<'_>, geometry: &GeomRef, over: FrameOverride) -> JointOrigin {
        JointOrigin {
            occurrence: OccurrencePath(aim.path.clone()),
            geometry: geometry.clone(),
            frame_override: (over != FrameOverride::default()).then_some(over),
        }
    }

    /// A foot's target: a grid of the picked part (its joint origin when
    /// that was picked, else the one the click is on), the cells under the
    /// click for the footprint as `rotate` turns it (quarter turns only),
    /// refused when one of them is taken. Returns side b, the frame in the
    /// target component's coordinates and the grid's place.
    fn grid_target(
        &self,
        aim: &Aim<'_>,
        geometry: &GeomRef,
        foot: &MountSpec,
    ) -> Result<(JointOrigin, Transform, GridPlace), ModelError> {
        let grids: Vec<FrameAt> = self
            .frames_under(aim.target)
            .into_iter()
            .filter(|f| f.frame.spec.kind == MountKind::Grid)
            .collect();
        let picked = match geometry {
            GeomRef::Datum(uid) => grids.iter().find(|g| g.frame.feature == *uid),
            _ => None,
        };
        let score = |g: &FrameAt| -> (u8, f64) {
            let Some(point) = aim.point else {
                return (0, 0.0);
            };
            let q = inverse(&g.at).apply_point(point);
            let [px, py] = g.frame.spec.pitch.unwrap_or([1.0, 1.0]);
            let [nx, ny] = g.frame.spec.cells.unwrap_or([1, 1]);
            let inside = (-px / 2.0..=f64::from(nx) * px + px / 2.0).contains(&q[0])
                && (-py / 2.0..=f64::from(ny) * py + py / 2.0).contains(&q[1]);
            (u8::from(!inside), q[2].abs())
        };
        let grid = picked
            .or_else(|| {
                grids
                    .iter()
                    .min_by(|a, b| score(a).partial_cmp(&score(b)).unwrap_or(Ordering::Equal))
            })
            .ok_or_else(|| {
                invalid(format!(
                    "{} has no grid for {} to stand on (a joint origin with a grid mount)",
                    aim.target_name, aim.label
                ))
            })?;
        let pitch = grid.frame.spec.pitch.unwrap_or([1.0, 1.0]);
        let cells = grid.frame.spec.cells.unwrap_or([1, 1]);
        let turns = quarter_turns(aim.rotate).ok_or_else(|| {
            invalid(format!(
                "on a grid {} turns in quarter turns (0, 90, 180 or 270 deg)",
                aim.label
            ))
        })?;
        let odd = turns % 2 == 1;
        let swap = |v: [u32; 2]| if odd { [v[1], v[0]] } else { v };
        let footprint = swap(foot.footprint.unwrap_or([1, 1]));
        if let Some(own) = foot.pitch {
            let own = if odd { [own[1], own[0]] } else { own };
            if (own[0] - pitch[0]).abs() > 1e-6 || (own[1] - pitch[1]).abs() > 1e-6 {
                return Err(invalid(format!(
                    "{} stands on a grid of {} × {} mm; {}'s grid is {} × {} mm",
                    aim.label,
                    number(own[0]),
                    number(own[1]),
                    aim.target_name,
                    number(pitch[0]),
                    number(pitch[1])
                )));
            }
        }
        if footprint[0] > cells[0] || footprint[1] > cells[1] {
            return Err(invalid(format!(
                "{} takes {} × {} cells, more than {}'s grid of {} × {} cells",
                aim.label, footprint[0], footprint[1], aim.target_name, cells[0], cells[1]
            )));
        }
        let cell = match aim.point {
            Some(point) => {
                let q = inverse(&grid.at).apply_point(point);
                [
                    first_cell(q[0], pitch[0], footprint[0], cells[0]),
                    first_cell(q[1], pitch[1], footprint[1], cells[1]),
                ]
            }
            None => [0, 0],
        };
        let full: Vec<OccurrenceUid> = aim.path.iter().chain(&grid.frame.path).copied().collect();
        let wanted = block(cell, footprint);
        let mut taken: Vec<[u32; 2]> = Vec::new();
        let mut by: Vec<String> = Vec::new();
        for (c, occupant) in self.grid_taken(aim.host, &full, grid.frame.feature, pitch) {
            if wanted.contains(&c) {
                if !taken.contains(&c) {
                    taken.push(c);
                }
                if !by.contains(&occupant) {
                    by.push(occupant);
                }
            }
        }
        if !taken.is_empty() {
            taken.sort_unstable();
            by.sort();
            return Err(invalid(format!(
                "the {} {} of {}'s grid {} taken by {}",
                if taken.len() == 1 { "cell" } else { "cells" },
                cells_text(&taken),
                aim.target_name,
                if taken.len() == 1 { "is" } else { "are" },
                and_list(&by)
            )));
        }
        let shift = [
            (f64::from(cell[0]) + f64::from(footprint[0]) / 2.0) * pitch[0],
            (f64::from(cell[1]) + f64::from(footprint[1]) / 2.0) * pitch[1],
            0.0,
        ];
        let over = FrameOverride {
            shift: Some(shift),
            ..FrameOverride::default()
        };
        let at = grid
            .at
            .after(&inverse(&grid.local))
            .after(&apply_override(&grid.local, &over).map_err(invalid)?);
        let b = JointOrigin {
            occurrence: OccurrencePath(full.clone()),
            geometry: GeomRef::Datum(grid.frame.feature),
            frame_override: Some(over),
        };
        let place = GridPlace {
            feature: grid.frame.feature,
            path: full,
            pitch,
            cells,
            cell,
            footprint,
            turns,
        };
        Ok((b, at, place))
    }

    /// The cells of a grid that parts placed on it take: the joints in
    /// `host` whose side b is the grid (through `path`) and whose side a is
    /// a foot, each cell with the occurrence that stands there.
    pub(crate) fn grid_taken(
        &self,
        host: ComponentUid,
        path: &[OccurrenceUid],
        grid: FeatureUid,
        pitch: [f64; 2],
    ) -> Vec<([u32; 2], String)> {
        let a = &self.state.assembly;
        let params = self.parameters();
        let end = self.state.marker.min(self.state.features.len());
        let mut out = Vec::new();
        for entry in &self.state.features[..end] {
            if entry.suppressed || entry.component != host {
                continue;
            }
            let FeatureDef::Joint(joint) = &entry.def else {
                continue;
            };
            if joint.b.occurrence.0.as_slice() != path || joint.b.geometry != GeomRef::Datum(grid) {
                continue;
            }
            let GeomRef::Datum(foot) = &joint.a.geometry else {
                continue;
            };
            let Some(FeatureDef::JointOrigin(def)) = self.state.entry(*foot).map(|e| &e.def) else {
                continue;
            };
            let Some(spec) = def.mount.as_ref().filter(|s| s.kind == MountKind::Foot) else {
                continue;
            };
            let angle = joint.angle.and_then(|id| params.value(id)).unwrap_or(0.0);
            let turns = quarter_turns(angle).unwrap_or(0);
            let own = spec.footprint.unwrap_or([1, 1]);
            let size = if turns % 2 == 1 {
                [own[1], own[0]]
            } else {
                own
            };
            let shift = joint
                .b
                .frame_override
                .and_then(|o| o.shift)
                .unwrap_or([0.0; 3]);
            let first =
                |s: f64, p: f64, n: u32| (s / p - f64::from(n) / 2.0).round().max(0.0) as u32;
            let cell = [
                first(shift[0], pitch[0], size[0]),
                first(shift[1], pitch[1], size[1]),
            ];
            let name = joint
                .a
                .occurrence
                .0
                .first()
                .map_or_else(|| a.name(host), |o| a.occurrence_name(*o));
            for c in block(cell, size) {
                out.push((c, name.clone()));
            }
        }
        out
    }

    /// A placement's preview (`mount_preview`): the part's name and frames
    /// and, with `input`, the plan or why it is refused; nothing changes.
    pub fn mount_preview(
        &self,
        source: MountSource<'_>,
        input: Option<&MountInput>,
    ) -> Result<MountPreview, ModelError> {
        let (state, name) = self.mount_source(source)?;
        let mounts = self.design_mounts(&state)?;
        let plan = input.map(|input| {
            self.plan_mount(&mounts, input, &name)
                .map_err(|e| e.to_string())
        });
        Ok(MountPreview {
            name,
            frames: mounts.iter().map(|m| (m.spec.clone(), m.frame)).collect(),
            plan,
            host: self.host_world(),
        })
    }

    /// The shapes of a part placed as a mount placement would place it, in
    /// the design's coordinates (`analysis_shape` `mount_preview`).
    pub fn mount_preview_shapes(
        &self,
        source: MountSource<'_>,
        input: &MountInput,
    ) -> Result<Vec<K::Shape>, ModelError> {
        let (state, name) = self.mount_source(source)?;
        let (bodies, mounts) = self.design_shapes(&state, &name)?;
        let plan = self.plan_mount(&mounts, input, &name)?;
        let world = self.host_world().after(&plan.placement);
        bodies
            .iter()
            .map(|body| {
                self.kernel()
                    .transform_shape(&body.shape, &world, None)
                    .map_err(|e| invalid(format!("{name}: {e}")))
            })
            .collect()
    }

    fn mount_source(&self, source: MountSource<'_>) -> Result<(DocState, String), ModelError> {
        match source {
            MountSource::Library(part) => self.library_design(part),
            MountSource::File { path, base } => self.file_design(path, base),
        }
    }

    /// Where the active component is in the design: its first placement.
    pub fn host_world(&self) -> Transform {
        let a = &self.state.assembly;
        crate::links::first_path(a, a.active, None)
            .map(|path| self.path_transform(&path))
            .unwrap_or(Transform::IDENTITY)
    }
}
