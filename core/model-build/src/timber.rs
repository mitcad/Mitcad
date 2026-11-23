// SPDX-License-Identifier: MIT
//! Timber members placed by two points (mitcad#478; `commands.md`, "BUILD:
//! timber construction"): a member from a start to an end point, lying on
//! the plane it was picked on (or grounded through the points), turned
//! flat or on edge about its axis.

use serde::{Deserialize, Serialize};

use crate::PlaceInput;
use crate::geometry::{V3, Z, add, cross, dot, inverse, scale, sub, unit, z_axis};
use crate::place::{JointPlan, LengthPlan, Plan, RayPlan, SourceUse, Spot, refuse};
use crate::prelude::*;
use mitcad_model::features::bound_length::RayDirection;
use mitcad_model::features::{FrameOverride, JointOrigin, OccurrencePath};
use mitcad_model::ids::ComponentUid;
use mitcad_model::joints::{JointKind, SlideAxis};
use mitcad_model::kernel::{BoundingBox, Kernel};
use mitcad_model::transform::Transform;
use mitcad_model::{Document, ModelError};

/// Where a member by two points ends.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointTo {
    /// In the BUILD host's coordinates.
    pub point: V3,
}

/// How a member by two points lies about its axis.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lie {
    /// The section's smaller dimension across the plane it lies on.
    #[default]
    Flat,
    /// The larger one.
    Edge,
}

/// A member by two points, worked out: where it goes.
pub(crate) struct PointsPlan {
    pub spot: Spot,
}

/// The member's X axis (in the host's coordinates) for an axis `d`, its
/// section's box, the plane's normal `up` and the lie; and how far its axis
/// is lifted off the plane (0 when it stands square to it).
pub(crate) fn lie_axes(
    d: V3,
    section: Option<&BoundingBox>,
    up: V3,
    lie: Lie,
    quarter_turns: f64,
) -> (V3, f64) {
    let up_perp = unit(sub(up, scale(d, dot(d, up))));
    let (w, t) = section.map_or((0.0, 0.0), |b| (b.max[0] - b.min[0], b.max[1] - b.min[1]));
    let (mut x, lift) = match up_perp {
        None => {
            // Standing square to the plane: X along the host's X.
            let hint = [crate::geometry::X, crate::geometry::Y]
                .into_iter()
                .find(|a| dot(*a, d).abs() < 1.0 - 1e-6)
                .unwrap_or(crate::geometry::X);
            (
                unit(sub(hint, scale(d, dot(d, hint)))).unwrap_or(crate::geometry::X),
                0.0,
            )
        }
        Some(u) => {
            // The section's Y along `u` puts T across the plane; X along it
            // puts W across.
            let y_up = match lie {
                Lie::Flat => w >= t,
                Lie::Edge => w < t,
            };
            if y_up {
                let lift = section.map_or(0.0, |b| -b.min[1]);
                (cross(u, d), lift)
            } else {
                let lift = section.map_or(0.0, |b| -b.min[0]);
                (u, lift)
            }
        }
    };
    if quarter_turns.abs() > 1e-12 {
        let (s, c) = quarter_turns.sin_cos();
        x = add(scale(x, c), scale(cross(d, x), s));
    }
    (x, lift)
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub(crate) trait PointsCrateOps<K: Kernel> {
    /// Plans a member by two points: from `at.point` to `to.point`, lying
    /// on `at.geometry` when given (a rigid joint with its frame
    /// overridden), else grounded through the points.
    #[allow(clippy::too_many_arguments)]
    fn plan_points(
        &self,
        source: &SourceUse,
        at: &crate::PlaceAt,
        to: &PointTo,
        lie: Lie,
        flip: bool,
        rotate: f64,
        mounts: &std::collections::BTreeMap<String, Transform>,
        bounds: Option<&BoundingBox>,
    ) -> Result<PointsPlan, ModelError>;
}

impl<K: Kernel> PointsCrateOps<K> for Document<K> {
    fn plan_points(
        &self,
        source: &SourceUse,
        at: &crate::PlaceAt,
        to: &PointTo,
        lie: Lie,
        flip: bool,
        rotate: f64,
        mounts: &std::collections::BTreeMap<String, Transform>,
        bounds: Option<&BoundingBox>,
    ) -> Result<PointsPlan, ModelError> {
        let label = source.def.label();
        if source.meta.length.is_none() {
            return Err(refuse(format!(
                "{label} is not cut to length: it is not placed by two points"
            )));
        }
        let start = at
            .point
            .ok_or_else(|| refuse("a member by two points needs its start (at.point)"))?;
        let (start, end) = if flip {
            (to.point, start)
        } else {
            (start, to.point)
        };
        let d = unit(sub(end, start))
            .ok_or_else(|| refuse("the two points are the same: the member has no length"))?;
        // The plane it lies on, in the host's coordinates.
        let on = match &at.geometry {
            Some(geometry) => {
                let path = self.path_from_host(&at.occurrence)?;
                let local =
                    mitcad_model::internal::joints::origin_frame(&mut self.resolver(), geometry)
                        .map_err(|e| refuse(format!("{geometry}: {e}")))?;
                Some((path, geometry.clone(), local))
            }
            None => None,
        };
        let up = match &on {
            Some((path, _, local)) => self.path_transform(path).apply_vector(z_axis(local)),
            None => Z,
        };
        let (x, lift) = lie_axes(d, bounds, up, lie, rotate);
        let lift = if on.is_some() { lift } else { 0.0 };
        let up_perp = unit(sub(up, scale(d, dot(d, up)))).unwrap_or(up);
        let origin = add(start, scale(up_perp, lift));
        let frame = crate::geometry::frame(origin, x, d);
        let end_a = mounts.get("end_a").map_or([0.0; 3], |t| t.translation);
        let spot = match on {
            None => Spot::Grounded(frame.after(&Transform::translation(scale(end_a, -1.0)))),
            Some((path, geometry, local)) => {
                let placement = self.path_transform(&path);
                let to_local = inverse(&placement);
                let origin_local = to_local.apply_point(origin);
                let d_local = to_local.apply_vector(d);
                let x_local = to_local.apply_vector(x);
                let frame_inverse = inverse(&local);
                Spot::Joint(Box::new(JointPlan {
                    kind: JointKind::Rigid,
                    slide: SlideAxis::Z,
                    own: "end_a".to_owned(),
                    b: JointOrigin {
                        occurrence: OccurrencePath(path.clone()),
                        geometry: geometry.clone(),
                        frame_override: Some(FrameOverride {
                            origin: Some(origin_local),
                            z_axis: Some(d_local),
                            x_axis: Some(x_local),
                            shift: None,
                            relative: None,
                        }),
                    },
                    frame_b: frame,
                    flip: true,
                    slide_value: None,
                    ray: Some(RayPlan {
                        from: JointOrigin {
                            occurrence: OccurrencePath(path),
                            geometry,
                            frame_override: None,
                        },
                        shift: frame_inverse.apply_point(origin_local),
                        shift_by_joint: false,
                        slide_extra: 0.0,
                        direction: RayDirection::Vector(frame_inverse.apply_vector(d_local)),
                        start: origin,
                        along: d,
                    }),
                    side: None,
                    end: None,
                    own_override: None,
                    cuts: Vec::new(),
                    from: None,
                    warnings: Vec::new(),
                }))
            }
        };
        Ok(PointsPlan { spot })
    }
}

/// How near a point must be to a member's box to be on it, mm.
const ON: f64 = 1e-3;

/// What the two points of a member by two points are on (mitcad#509).
#[derive(Debug, Clone, Default)]
pub struct Between {
    /// The member the start (`at.point`) and the end (`to.point`) are on
    /// or in: its component and name.
    pub on: [Option<(ComponentUid, String)>; 2],
    /// Whether the new member's end at the start, at the end, was cut back
    /// to the member it was in.
    pub cut: [bool; 2],
    /// Whether it lies on a plane (`at.geometry`); else it is free.
    pub plane: bool,
}

/// A member of the design a point may be on: its box in its coordinates
/// and where it is in the BUILD host.
pub(crate) struct Near {
    component: ComponentUid,
    name: String,
    placement: Transform,
    bounds: BoundingBox,
}

impl Near {
    /// Whether `point` (in the host) is in its box or on it.
    fn holds(&self, point: V3) -> bool {
        let p = inverse(&self.placement).apply_point(point);
        (0..3).all(|i| p[i] >= self.bounds.min[i] - ON && p[i] <= self.bounds.max[i] + ON)
    }

    /// Whether `point` (in the host) is inside its box, not on its faces.
    fn inside(&self, point: V3) -> bool {
        let p = inverse(&self.placement).apply_point(point);
        (0..3).all(|i| p[i] > self.bounds.min[i] + ON && p[i] < self.bounds.max[i] - ON)
    }

    /// Where a ray from `start` along `along` (in the host) runs through
    /// its box: the distances along it where it enters and leaves, when it
    /// runs through the inside between them (not along one of its faces).
    fn through(&self, start: V3, along: V3, until: f64) -> Option<(f64, f64)> {
        let to_local = inverse(&self.placement);
        let o = to_local.apply_point(start);
        let v = to_local.apply_vector(along);
        let (mut enter, mut leave) = (f64::NEG_INFINITY, f64::INFINITY);
        for i in 0..3 {
            if v[i].abs() < 1e-12 {
                if o[i] < self.bounds.min[i] || o[i] > self.bounds.max[i] {
                    return None;
                }
                continue;
            }
            let a = (self.bounds.min[i] - o[i]) / v[i];
            let b = (self.bounds.max[i] - o[i]) / v[i];
            enter = enter.max(a.min(b));
            leave = leave.min(a.max(b));
        }
        if enter > leave {
            return None;
        }
        let middle = add(o, scale(v, (enter + leave.min(until)) / 2.0));
        (0..3)
            .all(|i| middle[i] > self.bounds.min[i] + ON && middle[i] < self.bounds.max[i] - ON)
            .then_some((enter, leave))
    }
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub(crate) trait BetweenCrateOps<K: Kernel> {
    /// The members cut to length placed in the BUILD host, but `except`.
    fn members_near(&self, except: Option<ComponentUid>) -> Vec<Near>;

    /// Plans a placing as `build_place` makes it ([`Self::plan_place`]).
    /// A member by two points without a `length` (mitcad#509) runs between
    /// the members its points are in: an end whose point is inside another
    /// member (not `except`, the member placed anew), while the other point
    /// is not on it, is cut back to where its axis enters that member, a
    /// square butt on its face (or the corner it closes, mitcad#492), so
    /// that a member clicked between two members fits between them, not
    /// into them. A free member (no plane) is cut so too where its point is
    /// on a member's face and its axis runs through that member (a click
    /// on its far side); on the near face it ends there already. Its
    /// `points` stay the points given, to be placed anew from them.
    fn plan_place_between(
        &self,
        input: &PlaceInput,
        except: Option<ComponentUid>,
    ) -> Result<Plan, ModelError>;
}

impl<K: Kernel> BetweenCrateOps<K> for Document<K> {
    fn members_near(&self, except: Option<ComponentUid>) -> Vec<Near> {
        self.build_parts_info()
            .into_iter()
            .filter(|info| info.is_cut_to_length() && Some(info.component) != except)
            .filter_map(|info| {
                let path = self.placed_in_host(info.component)?;
                let bounds = self.part_bounds(info.component)?;
                Some(Near {
                    component: info.component,
                    placement: self.path_transform(&path),
                    name: info.name,
                    bounds,
                })
            })
            .collect()
    }

    fn plan_place_between(
        &self,
        input: &PlaceInput,
        except: Option<ComponentUid>,
    ) -> Result<Plan, ModelError> {
        let mut plan = self.plan_place(input)?;
        let (Some(to), None, None) = (&input.to, &input.length, &input.spacing) else {
            return Ok(plan);
        };
        let Some(start) = input.at.as_ref().and_then(|a| a.point) else {
            return Ok(plan);
        };
        let length = match &plan.length {
            LengthPlan::Value(_, length) => Some(*length),
            _ => None,
        };
        let Some(length) = length.filter(|_| plan.place == "points") else {
            return Ok(plan);
        };
        let near = self.members_near(except);
        let points = [start, to.point];
        let mut between = Between {
            on: points.map(|p| {
                near.iter()
                    .find(|m| m.holds(p))
                    .map(|m| (m.component, m.name.clone()))
            }),
            cut: [false, false],
            plane: input.at.as_ref().is_some_and(|a| a.geometry.is_some()),
        };
        // Its axis from end A (at the first point as `flip` orders them).
        let end_a = plan.mounts.get("end_a").map_or([0.0; 3], |t| t.translation);
        let a0 = plan.placement.apply_point(end_a);
        let d = plan.placement.apply_vector(Z);
        let a1 = add(a0, scale(d, length));
        let back = scale(d, -1.0);
        // The points given at end A and end B.
        let flip = input.flip;
        let (pa, pb) = if flip {
            (points[1], points[0])
        } else {
            (points[0], points[1])
        };
        // How far end A moves along the axis, and where end B ends: an end
        // whose point is inside a member (a centre line snapped to), or on
        // its face with the axis running into it when the member is free
        // (its axis through the points), while the other one is not on it.
        // On its near face it ends there already. A member lying on a
        // plane has its axis off the points: only a point inside counts
        // (members lapped over each other on a plane stay so).
        let free = !between.plane;
        let at_member = |m: &Near, p: V3| m.inside(p) || (free && m.holds(p));
        let (mut from, mut until) = (0.0_f64, length);
        let mut cut = [false, false];
        for m in &near {
            if at_member(m, pa)
                && !m.holds(pb)
                && let Some((enter, _)) = m.through(a1, back, length)
                && length - enter > from.max(1e-6)
            {
                from = length - enter;
                cut[0] = true;
            }
            if at_member(m, pb)
                && !m.holds(pa)
                && let Some((enter, _)) = m.through(a0, d, length)
                && enter < until.min(length - 1e-6)
            {
                until = enter;
                cut[1] = true;
            }
        }
        between.cut = if flip { [cut[1], cut[0]] } else { cut };
        if !cut[0] && !cut[1] {
            plan.between = Some(between);
            return Ok(plan);
        }
        if until - from < 1.0 {
            return Err(refuse(
                "the members its points are in meet: nothing is left between them",
            ));
        }
        // The points moved along the axis (it runs parallel to them).
        let moved = [add(pa, scale(d, from)), add(pa, scale(d, until))];
        let (start, end) = if flip {
            (moved[1], moved[0])
        } else {
            (moved[0], moved[1])
        };
        let mut trimmed = input.clone();
        let mut at = trimmed.at.clone().unwrap_or_default();
        at.point = Some(start);
        trimmed.at = Some(at);
        trimmed.to = Some(PointTo { point: end });
        let given = plan.points.take();
        let mut again = self.plan_place(&trimmed)?;
        again.points = given;
        if cut[1] {
            // Its far end is on the member's face now, however its axis
            // crosses it.
            again
                .warnings
                .retain(|w| !w.starts_with("its far end ends inside"));
        }
        again.between = Some(between);
        Ok(again)
    }
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub(crate) trait FaceSheetCrateOps<K: Kernel> {
    /// Plans a sheet laid on members' faces (`build_panel` with a sheet,
    /// `panel.mount: "face"`): over the rectangle of the picked faces (of
    /// every member face on the picked one's plane facing its way when no
    /// more are picked), its back on the face.
    fn plan_face_sheet(
        &self,
        input: &crate::PanelInput,
        source: SourceUse,
    ) -> Result<crate::connect::PanelPlan, ModelError>;
}

impl<K: Kernel> FaceSheetCrateOps<K> for Document<K> {
    fn plan_face_sheet(
        &self,
        input: &crate::PanelInput,
        source: SourceUse,
    ) -> Result<crate::connect::PanelPlan, ModelError> {
        use crate::geometry::parts;
        use mitcad_model::expr::Unit;
        use mitcad_model::features::ValueInput;
        let label = source.def.label();
        let panel = source.meta.panel.clone().unwrap_or_default();
        let pick = |occurrence: &str,
                    geometry: Option<&mitcad_model::features::GeomRef>,
                    point: Option<V3>|
         -> Result<(crate::place::Target, String), ModelError> {
            let geometry = geometry.ok_or_else(|| refuse("pick a face of a member"))?;
            let path = self.path_from_host(occurrence)?;
            let target = self
                .target(&path)
                .ok_or_else(|| refuse("pick a face of a member"))?;
            let face = match geometry {
                mitcad_model::features::GeomRef::Mount { name, .. }
                    if target.info.is_side(name) =>
                {
                    name.clone()
                }
                mitcad_model::features::GeomRef::Face { body, face } => {
                    let local = point.map(|p| inverse(&target.placement).apply_point(p));
                    self.pick_slot(&target, Some(&(*body, face.clone())), local, false)?
                }
                _ => return Err(refuse("pick a face of a member")),
            };
            Ok((target, face))
        };
        let (target, face) = pick(
            &input.at.occurrence,
            input.at.geometry.as_ref(),
            input.at.point,
        )?;
        let s1 = target
            .frame(&face)
            .ok_or_else(|| refuse(format!("{} has no frame {face}", target.info.name)))?;
        let [o1, x1, y1, z1] = parts(&s1);
        // The faces it covers, as members and their boxes.
        let mut covered: Vec<crate::wood::Member> = Vec::new();
        let first = self
            .member(target.info.component)
            .ok_or_else(|| refuse("the member is not placed"))?;
        covered.push(first);
        if input.faces.is_empty() {
            for info in self.build_parts_info() {
                if info.component == target.info.component || !info.is_timber() {
                    continue;
                }
                let Some(m) = self.member(info.component) else {
                    continue;
                };
                let on_plane = info.sides().iter().any(|(_, t)| {
                    let f = m.placement.after(t);
                    dot(z_axis(&f), z1) > 1.0 - 1e-6 && dot(sub(f.translation, o1), z1).abs() < 1e-3
                });
                if on_plane {
                    covered.push(m);
                }
            }
        } else {
            for more in &input.faces {
                let (t, _) = pick(&more.occurrence, Some(&more.geometry), None)?;
                if let Some(m) = self.member(t.info.component) {
                    covered.push(m);
                }
            }
        }
        let range = |along: V3| {
            covered
                .iter()
                .map(|m| m.extent(along))
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), (lo, hi)| {
                    (a.min(lo), b.max(hi))
                })
        };
        let (ul, uh) = range(x1);
        let (vl, vh) = range(y1);
        let width = uh - ul;
        let height = vh - vl;
        // Its values: the sizes where they are parameters, the thickness.
        let design = source
            .def
            .design
            .state()
            .map_err(|e| refuse(format!("{label}: {e}")))?;
        let is_param = |name: &Option<String>| {
            name.clone()
                .filter(|n| design.parameters.find(n.trim()).is_some())
        };
        let mut values = input.parameters.clone();
        let mut warnings = Vec::new();
        if let Some(thickness) = &input.thickness {
            match panel
                .thickness
                .as_ref()
                .map(|t| t.text())
                .filter(|t| design.parameters.find(t.trim()).is_some())
            {
                Some(name) => {
                    values.insert(name.trim().to_owned(), thickness.clone());
                }
                None => warnings.push(format!("{label} has a fixed thickness")),
            }
        }
        if let Some(name) = is_param(&panel.width) {
            values.insert(name, ValueInput::Number(width));
        }
        if let Some(name) = is_param(&panel.height) {
            values.insert(name, ValueInput::Number(height));
        }
        let mut numbers = Vec::new();
        for (name, value) in &values {
            numbers.push((name.clone(), crate::place::value_in(self, value, Unit::MM)?));
        }
        let state = crate::parts::source_state(&source.def, source.config.as_deref(), &numbers)
            .map_err(|e| refuse(format!("{label}: {e}")))?;
        let evaluated = mitcad_model::internal::features::evaluate_source(self.kernel(), &state)
            .map_err(|e| refuse(format!("{label}: {e}")))?;
        let b = crate::place::bounds_of(self.kernel(), &evaluated.bodies)
            .ok_or_else(|| refuse(format!("{label} has no size")))?;
        let (sw, sh) = (b.max[0] - b.min[0], b.max[1] - b.min[1]);
        let mut problems = Vec::new();
        if sw < width - 1e-6 {
            problems.push(format!(
                "its width of {} mm does not cover the {} mm of the faces",
                mitcad_model::internal::build::round(sw),
                mitcad_model::internal::build::round(width)
            ));
        }
        if sh < height - 1e-6 {
            problems.push(format!(
                "its height of {} mm does not cover the {} mm of the faces",
                mitcad_model::internal::build::round(sh),
                mitcad_model::internal::build::round(height)
            ));
        }
        // The middle of its box on the middle of the rectangle, its back
        // on the face.
        let centre = add(
            add(o1, scale(x1, (ul + uh) / 2.0 - dot(o1, x1))),
            scale(y1, (vl + vh) / 2.0 - dot(o1, y1)),
        );
        let middle = [
            -(b.min[0] + b.max[0]) / 2.0,
            -(b.min[1] + b.max[1]) / 2.0,
            -b.min[2],
        ];
        let placement =
            crate::geometry::frame(centre, x1, z1).after(&Transform::translation(middle));
        let local = inverse(&target.placement).after(&placement);
        let [lo, lx, _, lz] = parts(&local);
        let b_origin = JointOrigin {
            occurrence: OccurrencePath(target.path.clone()),
            geometry: mitcad_model::features::GeomRef::Mount {
                feature: target.part()?,
                name: face.clone(),
            },
            frame_override: Some(FrameOverride {
                origin: Some(lo),
                z_axis: Some(lz),
                x_axis: Some(lx),
                shift: None,
                relative: None,
            }),
        };
        let shown: std::collections::BTreeMap<String, f64> = numbers.into_iter().collect();
        let designation = crate::parts::designation(&source.def, source.config.as_deref(), &shown);
        let name = input.name.clone().unwrap_or_else(|| {
            if designation.is_empty() {
                label.clone()
            } else {
                designation.clone()
            }
        });
        Ok(crate::connect::PanelPlan {
            panel: crate::connect::Piece {
                source,
                values,
                name,
                kind: JointKind::Rigid,
                slide: SlideAxis::Z,
                own: None,
                own_override: None,
                b: b_origin,
                flip: false,
                angle: 0.0,
                slide_value: None,
                record: crate::BuildRecord {
                    role: crate::BuildRole::Panel,
                    ..crate::BuildRecord::default()
                },
            },
            bounds: Vec::new(),
            clamps: Vec::new(),
            fits: problems.is_empty(),
            reason: problems.join("; "),
            width,
            height,
            placement,
            warnings,
            designation,
        })
    }
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub(crate) trait EditPointsCrateOps<K: Kernel> {
    /// `build_edit` of a member by two points with `to`, `at.point` or
    /// `lie`: placed anew from them (its component, derived part and the
    /// connectors on it stay).
    fn edit_points(
        &mut self,
        component: mitcad_model::ids::ComponentUid,
        input: &crate::EditInput,
    ) -> Result<Vec<String>, ModelError>;
}

impl<K: Kernel> EditPointsCrateOps<K> for Document<K> {
    fn edit_points(
        &mut self,
        component: mitcad_model::ids::ComponentUid,
        input: &crate::EditInput,
    ) -> Result<Vec<String>, ModelError> {
        let info = self
            .part_info(component)
            .ok_or_else(|| refuse("no such BUILD part"))?;
        let mut base: crate::PlaceInput = info
            .record
            .points
            .clone()
            .and_then(|v| serde_json::from_value(v).ok())
            .ok_or_else(|| refuse(format!("{} was not placed by two points", info.name)))?;
        if let Some(to) = &input.to {
            base.to = Some(to.clone());
        }
        if let Some(lie) = input.lie {
            base.lie = Some(lie);
        }
        if let Some(point) = input.at.as_ref().and_then(|a| a.point) {
            let mut at = base.at.clone().unwrap_or_default();
            at.point = Some(point);
            base.at = Some(at);
        }
        if let Some(length) = &input.length {
            base.length = Some(length.clone());
        }
        if let Some(config) = &input.config {
            base.part.config = Some(config.clone());
        }
        let plan = self.plan_place_between(&base, Some(component))?;
        let active = self.build_host();
        self.state().editable(active)?;
        let label = format!("Edit {}", info.name);
        self.apply(|state| {
            crate::spacing::replace_member(state, &plan, active, component)?;
            Ok((label, vec!["points".to_owned(), "length".to_owned()]))
        })
    }
}
