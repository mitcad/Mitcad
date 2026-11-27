// SPDX-License-Identifier: MIT
//! Point cloud tools on the design's clouds (mitcad#2516; the algorithms in
//! `crate::point_cloud_tools`):
//!
//! - Align ([`Document::align_point_cloud`]): a cloud moved onto a body
//!   (its exact faces through `Kernel::boundary_nearest`, or some of them),
//!   a mesh body (its triangles) or another cloud: from where it is, from
//!   picked point pairs or from a rough start by its shape, refined by the
//!   best fit, the remaining deviation stated. The motion goes into the
//!   cloud's placement as Move Point Cloud's does (an undo step `Align
//!   PointCloud1`); [`Document::point_cloud_alignment`] works it out
//!   without moving the cloud.
//! - Filter ([`Document::filter_point_cloud`]): outliers, noise, a crop by
//!   a box or a lasso: a new cloud of the points kept, in the same
//!   component and placement; the cloud it came from is kept, hidden in
//!   the same undo step (`Filter PointCloud1`).
//! - Reconstruct ([`Document::reconstruct_point_cloud`]): a closed mesh of
//!   the cloud as a new mesh body (a base feature, undo step `Reconstruct
//!   PointCloud1`) in the cloud's component, which the mesh tools then
//!   work on.
//!
//! The places the tools take (point pairs, the region, the crop box and
//! lasso) are in the design's coordinates at the marker, as the
//! `point_clouds` query's `bounds` are.

use serde_json::{Value, json};

use super::point_clouds::{CloudData, PointCloud, rigid_inverse};
use super::{Document, ModelError, invalid};
use crate::exchange::{BaseInput, BodyKind, ImportBody, MeshTolerance};
use crate::ids::{BodyUid, ComponentUid};
use crate::kernel::Kernel;
use crate::point_cloud_tools::filter::{self, FilterOptions};
use crate::point_cloud_tools::matching::{
    self, FitError, FitOptions, PointsTarget, Target, TrianglesTarget,
};
use crate::point_cloud_tools::reconstruct::{self, ReconstructError, ReconstructOptions};
use crate::point_cloud_tools::thinned;
use crate::topo::FaceName;
use crate::transform::Transform;

type Vec3 = [f64; 3];

/// The cloud's points an alignment matches when no number is given.
pub const DEFAULT_ALIGN_SAMPLES: usize = 20_000;
/// The most points an alignment matches.
pub const MAX_ALIGN_SAMPLES: usize = 1_000_000;
/// The most rounds of a best fit.
pub const MAX_ALIGN_ITERATIONS: usize = 1000;
/// The points a rough start by the shape tries its starts with.
const SHAPE_SAMPLES: usize = 1000;
/// The rounds of the best fit from each rough start.
const SHAPE_ROUNDS: usize = 20;
/// The points of a target body or mesh a rough start compares shapes with.
const TARGET_SAMPLES: usize = 5000;
/// The most points a mesh is built of: a larger cloud is thinned evenly.
pub const MAX_RECONSTRUCT_POINTS: usize = 500_000;
/// Points asked of the kernel per call.
const CHUNK: usize = 4096;
/// A mesh body's triangles as they are stored, and a body's triangles for
/// comparing shapes.
const MESH_TOLERANCE: MeshTolerance = MeshTolerance {
    deviation: 0.01,
    angle: 0.1,
    face_triangles: 0,
    max_edge: 0.0,
};

/// What a cloud is aligned to.
#[derive(Debug, Clone, PartialEq)]
pub enum AlignTarget {
    /// A body: its exact faces (only `faces` when given); a mesh body's
    /// triangles.
    Body { body: BodyUid, faces: Vec<FaceName> },
    /// Another point cloud, by its name.
    Cloud(String),
}

/// Where the best fit starts.
#[derive(Debug, Clone, PartialEq)]
pub enum AlignStart {
    /// Where the cloud is.
    Placed,
    /// The cloud's middle and main axes matched to the target's.
    Shape,
    /// Picked point pairs: a point on the cloud and its place on the
    /// target, in the design's coordinates; at least three.
    Pairs(Vec<(Vec3, Vec3)>),
}

/// An alignment's settings.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignSettings {
    pub target: AlignTarget,
    pub start: AlignStart,
    /// The best fit refines the start; false: the start only.
    pub best_fit: bool,
    /// Only the cloud's points in this box (lowest and highest corner, in
    /// the design's coordinates where the cloud is) take part.
    pub region: Option<(Vec3, Vec3)>,
    /// Points farther than this from the target (mm) are left out of a
    /// round; none: those farther than the mean distance and three standard deviations.
    pub max_distance: Option<f64>,
    pub max_iterations: usize,
    /// At most this many of the cloud's points are matched.
    pub max_samples: usize,
}

impl AlignSettings {
    /// The settings of a target: from where the cloud is, refined by the
    /// best fit.
    pub fn new(target: AlignTarget) -> Self {
        Self {
            target,
            start: AlignStart::Placed,
            best_fit: true,
            region: None,
            max_distance: None,
            max_iterations: FitOptions::default().max_iterations,
            max_samples: DEFAULT_ALIGN_SAMPLES,
        }
    }

    fn check(&self) -> Result<(), String> {
        if matches!(self.start, AlignStart::Placed) && !self.best_fit {
            return Err(
                "nothing to align: pick point pairs, start by the shape, or use the best fit"
                    .to_owned(),
            );
        }
        if let Some(d) = self.max_distance
            && !(d > 0.0 && d.is_finite())
        {
            return Err(format!(
                "the largest distance must be greater than zero, got {d} mm"
            ));
        }
        if self.max_iterations == 0 || self.max_iterations > MAX_ALIGN_ITERATIONS {
            return Err(format!(
                "the rounds must be from 1 to {MAX_ALIGN_ITERATIONS}, got {}",
                self.max_iterations
            ));
        }
        if self.max_samples < matching::MIN_POINTS || self.max_samples > MAX_ALIGN_SAMPLES {
            return Err(format!(
                "the points matched must be from {} to {MAX_ALIGN_SAMPLES}, got {}",
                matching::MIN_POINTS,
                self.max_samples
            ));
        }
        if let Some((low, high)) = &self.region
            && !(0..3).all(|k| low[k].is_finite() && high[k].is_finite() && low[k] < high[k])
        {
            return Err("the region's lowest corner must be below its highest one".to_owned());
        }
        if let AlignStart::Pairs(pairs) = &self.start
            && pairs
                .iter()
                .any(|(a, b)| !a.iter().chain(b).all(|v| v.is_finite()))
        {
            return Err("a picked point is not a finite place".to_owned());
        }
        Ok(())
    }
}

/// What an alignment found: the motion of the cloud in the design's
/// coordinates and the remaining deviation of the points matched.
#[derive(Debug, Clone, PartialEq)]
pub struct Alignment {
    pub motion: Transform,
    /// The point pairs' root mean square distance after the motion, when
    /// aligned by pairs.
    pub pairs_rms: Option<f64>,
    pub rms: f64,
    pub mean: f64,
    pub max: f64,
    pub matched: usize,
    pub sampled: usize,
    pub iterations: usize,
    pub converged: bool,
    pub best_fit: bool,
}

impl Alignment {
    /// The answer of `align_point_cloud` and `point_cloud_alignment`.
    pub fn json(&self) -> Value {
        let mut value = json!({
            "motion": self.motion,
            "rms_mm": self.rms,
            "mean_mm": self.mean,
            "max_mm": self.max,
            "matched": self.matched,
            "sampled": self.sampled,
            "iterations": self.iterations,
            "converged": self.converged,
            "best_fit": self.best_fit,
        });
        if let Some(rms) = self.pairs_rms {
            value["pairs_rms_mm"] = json!(rms);
        }
        value
    }
}

/// A filter's settings.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CloudFilter {
    pub options: FilterOptions,
    /// The new cloud's name; `<name> Filtered` (or the next free one) when
    /// none.
    pub name: Option<String>,
    /// The cloud it came from stays shown.
    pub keep_shown: bool,
}

/// What a filter made.
#[derive(Debug, Clone, PartialEq)]
pub struct CloudFiltered {
    pub name: String,
    pub count: usize,
    pub from: usize,
    pub cropped: usize,
    pub statistical: usize,
    pub radius: usize,
    pub smoothed: usize,
    pub largest_move: f64,
}

impl CloudFiltered {
    pub fn json(&self) -> Value {
        json!({
            "name": self.name,
            "count": self.count,
            "from": self.from,
            "cropped": self.cropped,
            "statistical": self.statistical,
            "radius": self.radius,
            "smoothed": self.smoothed,
            "largest_move_mm": self.largest_move,
        })
    }
}

/// What a reconstruction made.
#[derive(Debug, Clone, PartialEq)]
pub struct CloudReconstructed {
    pub feature: String,
    pub body: BodyUid,
    pub body_name: String,
    pub points: usize,
    pub vertices: usize,
    pub triangles: usize,
    pub cell: f64,
    pub shells: usize,
    pub dropped_shells: usize,
    pub small_shells: usize,
    pub closed: bool,
    pub volume: f64,
    pub mean_deviation: f64,
    pub max_deviation: f64,
}

impl CloudReconstructed {
    pub fn json(&self) -> Value {
        json!({
            "feature": self.feature,
            "body": self.body,
            "body_name": self.body_name,
            "points": self.points,
            "vertices": self.vertices,
            "triangles": self.triangles,
            "cell_mm": self.cell,
            "shells": self.shells,
            "dropped_shells": self.dropped_shells,
            "small_shells": self.small_shells,
            "closed": self.closed,
            "volume_mm3": self.volume,
            "mean_deviation_mm": self.mean_deviation,
            "max_deviation_mm": self.max_deviation,
        })
    }
}

/// A body's exact faces as a target, through the kernel: the points taken
/// into the body's coordinates and its nearest points back.
struct BodyTarget<'a, K: Kernel> {
    kernel: &'a K,
    shape: &'a K::Shape,
    faces: &'a [FaceName],
    placement: Transform,
    inverse: Transform,
}

impl<K: Kernel> Target for BodyTarget<'_, K> {
    fn nearest(&self, points: &[Vec3]) -> Result<Vec<Option<Vec3>>, String> {
        let mut out = Vec::with_capacity(points.len());
        for chunk in points.chunks(CHUNK) {
            let local: Vec<Vec3> = chunk.iter().map(|p| self.inverse.apply_point(*p)).collect();
            let found = self
                .kernel
                .boundary_nearest(self.shape, self.faces, &local)
                .map_err(|e| e.to_string())?;
            if found.len() != chunk.len() {
                return Err("the kernel measured a different number of points".to_owned());
            }
            out.extend(
                found
                    .into_iter()
                    .map(|(d, q)| d.is_finite().then(|| self.placement.apply_point(q))),
            );
        }
        Ok(out)
    }
}

/// Points spread over triangles by their areas (`count` about), each
/// triangle's along a low-discrepancy sequence: a surface's shape for a
/// rough start.
fn sample_triangles(vertices: &[Vec3], triangles: &[[u32; 3]], count: usize) -> Vec<Vec3> {
    let corners = |t: &[u32; 3]| t.map(|i| vertices[i as usize]);
    let area = |t: &[u32; 3]| {
        let [a, b, c] = corners(t);
        0.5 * mitcad_vec3::norm(mitcad_vec3::cross(
            mitcad_vec3::sub(b, a),
            mitcad_vec3::sub(c, a),
        ))
    };
    let total: f64 = triangles.iter().map(area).sum();
    if total.is_nan() || total <= 0.0 {
        return vertices.to_vec();
    }
    let mut out = Vec::with_capacity(count + triangles.len());
    let mut carried = 0.0;
    for t in triangles {
        carried += count as f64 * area(t) / total;
        let n = carried.floor();
        carried -= n;
        let [a, b, c] = corners(t);
        for k in 0..n as usize {
            // The plastic number's sequence in the unit square, folded
            // into the triangle.
            let mut u = (0.5 + k as f64 * 0.754_877_666_246_692_7).fract();
            let mut v = (0.5 + k as f64 * 0.569_840_290_998_053_3).fract();
            if u + v > 1.0 {
                (u, v) = (1.0 - u, 1.0 - v);
            }
            out.push(std::array::from_fn(|i| {
                a[i] + u * (b[i] - a[i]) + v * (c[i] - a[i])
            }));
        }
    }
    if out.is_empty() {
        vertices.to_vec()
    } else {
        out
    }
}

fn fit_error(error: FitError) -> ModelError {
    match error {
        FitError::Cancelled => ModelError::Cancelled,
        other => invalid(other.to_string()),
    }
}

impl<K: Kernel> Document<K> {
    fn cloud_at(&self, name: &str) -> Result<usize, ModelError> {
        self.state
            .point_clouds
            .iter()
            .position(|c| c.name == name)
            .ok_or_else(|| invalid(format!("there is no point cloud {name}")))
    }

    /// The cloud's points in the design's coordinates at the marker.
    fn cloud_design_points(&self, cloud: &PointCloud, indices: &[usize]) -> Vec<Vec3> {
        let transform = self.point_cloud_transform(cloud);
        indices
            .iter()
            .map(|&i| transform.apply_point(cloud.local_point(i)))
            .collect()
    }

    /// The transform of the cloud's component's occurrence (the identity in
    /// the root).
    fn cloud_occurrence_transform(&self, cloud: &PointCloud) -> Transform {
        cloud
            .occurrence
            .as_ref()
            .map(|o| self.path_transform(&o.0))
            .unwrap_or(Transform::IDENTITY)
    }

    /// The component a cloud is placed in.
    fn cloud_component(&self, cloud: &PointCloud) -> ComponentUid {
        cloud
            .occurrence
            .as_ref()
            .and_then(|o| o.0.last())
            .and_then(|uid| self.state.assembly.occurrence(*uid))
            .map(|o| o.component)
            .unwrap_or(ComponentUid::ROOT)
    }

    /// Works an alignment out (see the module) without moving the cloud.
    pub fn point_cloud_alignment(
        &self,
        name: &str,
        settings: &AlignSettings,
    ) -> Result<Alignment, ModelError> {
        settings.check().map_err(invalid)?;
        let cloud = &self.state.point_clouds[self.cloud_at(name)?];
        // The cloud's points that take part, in the design.
        let all = self.cloud_design_points(cloud, &(0..cloud.points.len()).collect::<Vec<_>>());
        let inside: Vec<Vec3> = match &settings.region {
            Some((low, high)) => all
                .iter()
                .filter(|p| (0..3).all(|k| p[k] >= low[k] && p[k] <= high[k]))
                .copied()
                .collect(),
            None => all,
        };
        if inside.len() < matching::MIN_POINTS {
            return Err(invalid(if settings.region.is_some() {
                format!(
                    "aligning needs at least {} points of the cloud in the region, it has {} there",
                    matching::MIN_POINTS,
                    inside.len()
                )
            } else {
                format!(
                    "aligning needs at least {} points of the cloud, it has {}",
                    matching::MIN_POINTS,
                    inside.len()
                )
            }));
        }
        let points: Vec<Vec3> = thinned(inside.len(), settings.max_samples)
            .into_iter()
            .map(|i| inside[i])
            .collect();
        // The target, and its points for a rough start by the shape.
        let kernel = self.kernel();
        let placement_of = |body: BodyUid| {
            self.print_placements(&[body])
                .into_iter()
                .next()
                .unwrap_or(Transform::IDENTITY)
        };
        let shape_wanted = matches!(settings.start, AlignStart::Shape);
        let (target, target_points): (Box<dyn Target + '_>, Vec<Vec3>) = match &settings.target {
            AlignTarget::Cloud(other) => {
                if other == name {
                    return Err(invalid("a point cloud cannot be aligned to itself"));
                }
                let other = &self.state.point_clouds[self.cloud_at(other)?];
                let points =
                    self.cloud_design_points(other, &(0..other.points.len()).collect::<Vec<_>>());
                let sample = if shape_wanted {
                    thinned(points.len(), TARGET_SAMPLES)
                        .into_iter()
                        .map(|i| points[i])
                        .collect()
                } else {
                    Vec::new()
                };
                (Box::new(PointsTarget::new(points)), sample)
            }
            AlignTarget::Body { body, faces } => {
                let shape = self.body_shape(*body).ok_or_else(|| {
                    invalid(format!("body {body} does not exist at the timeline marker"))
                })?;
                let placement = placement_of(*body);
                let mesh_body = matches!(kernel.body_kind(shape), Ok(BodyKind::Mesh));
                let triangles = if mesh_body || shape_wanted {
                    let mesh = kernel
                        .triangle_mesh(shape, &MESH_TOLERANCE)
                        .map_err(|e| invalid(format!("the body's triangles: {e}")))?;
                    let vertices: Vec<Vec3> = mesh
                        .vertices
                        .iter()
                        .map(|p| placement.apply_point(*p))
                        .collect();
                    Some((vertices, mesh.triangles))
                } else {
                    None
                };
                let sample = match (&triangles, shape_wanted) {
                    (Some((vertices, triangles)), true) => {
                        sample_triangles(vertices, triangles, TARGET_SAMPLES)
                    }
                    _ => Vec::new(),
                };
                if mesh_body {
                    if !faces.is_empty() {
                        return Err(invalid(
                            "a mesh body is matched by all its triangles: pick no faces of it",
                        ));
                    }
                    let (vertices, triangles) = triangles.unwrap_or_default();
                    let target = TrianglesTarget::new(vertices, triangles);
                    if target.is_empty() {
                        return Err(invalid("the mesh body has no triangles"));
                    }
                    (Box::new(target), sample)
                } else {
                    let target = BodyTarget {
                        kernel,
                        shape,
                        faces,
                        placement,
                        inverse: rigid_inverse(&placement),
                    };
                    (Box::new(target), sample)
                }
            }
        };
        let monitor = self.monitor();
        let mut progress =
            |i: usize, n: usize| monitor.is_none_or(|monitor| monitor.enter(i, n).is_ok());
        let (start, pairs) = match &settings.start {
            AlignStart::Placed => (Transform::IDENTITY, None),
            AlignStart::Pairs(pairs) => {
                let from: Vec<Vec3> = pairs.iter().map(|(a, _)| *a).collect();
                let to: Vec<Vec3> = pairs.iter().map(|(_, b)| *b).collect();
                let motion = matching::pairs_motion(&from, &to).map_err(invalid)?;
                (motion, Some((from, to)))
            }
            AlignStart::Shape => {
                if target_points.len() < matching::MIN_POINTS {
                    return Err(invalid(
                        "the target has too few points to compare its shape with the cloud's",
                    ));
                }
                let few: Vec<Vec3> = thinned(points.len(), SHAPE_SAMPLES)
                    .into_iter()
                    .map(|i| points[i])
                    .collect();
                let starts = matching::shape_starts(&few, &target_points);
                let start = matching::best_start(
                    &few,
                    target.as_ref(),
                    &starts,
                    SHAPE_ROUNDS,
                    &mut progress,
                )
                .map_err(fit_error)?;
                (start, None)
            }
        };
        let options = FitOptions {
            max_iterations: if settings.best_fit {
                settings.max_iterations
            } else {
                0
            },
            max_distance: settings.max_distance,
            ..FitOptions::default()
        };
        let fit = matching::best_fit(&points, target.as_ref(), start, &options, &mut progress)
            .map_err(fit_error)?;
        Ok(Alignment {
            pairs_rms: pairs.map(|(from, to)| matching::pairs_rms(&fit.motion, &from, &to)),
            motion: fit.motion,
            rms: fit.rms,
            mean: fit.mean,
            max: fit.max,
            matched: fit.matched,
            sampled: fit.points,
            iterations: fit.iterations,
            converged: fit.converged,
            best_fit: settings.best_fit,
        })
    }

    /// Aligns a cloud (see the module): its placement moved by the motion
    /// found, an undo step `Align <name>` (none when it moves nothing).
    pub fn align_point_cloud(
        &mut self,
        name: &str,
        settings: &AlignSettings,
    ) -> Result<Alignment, ModelError> {
        let alignment = self.point_cloud_alignment(name, settings)?;
        let i = self.cloud_at(name)?;
        let mut cloud = self.state.point_clouds[i].clone();
        // The design's motion in the cloud's component: the occurrence's
        // transform undone, the motion, the transform again.
        let occurrence = self.cloud_occurrence_transform(&cloud);
        let motion = rigid_inverse(&occurrence)
            .after(&alignment.motion)
            .after(&occurrence);
        if motion.is_identity() {
            return Ok(alignment);
        }
        if !motion.is_finite() || !motion.is_rigid() {
            return Err(invalid("the alignment found no rigid motion"));
        }
        cloud.placement = motion.after(&cloud.placement);
        cloud.check().map_err(invalid)?;
        let label = format!("Align {name}");
        self.apply_with(|state| {
            state.point_clouds[i] = cloud;
            Ok((label, (), false))
        })?;
        Ok(alignment)
    }

    /// Filters a cloud into a new one (see the module): its name and what
    /// the filter took out; refused in words for settings that are none or
    /// no point left.
    pub fn filter_point_cloud(
        &mut self,
        name: &str,
        settings: &CloudFilter,
    ) -> Result<CloudFiltered, ModelError> {
        let i = self.cloud_at(name)?;
        let source = &self.state.point_clouds[i];
        let count = source.points.len();
        let design = self.cloud_design_points(source, &(0..count).collect::<Vec<_>>());
        let filtered = filter::filter(&design, &settings.options)
            .map_err(|e| invalid(format!("{name}: {e}")))?;
        let new_name = match settings
            .name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            Some(given) => {
                if self.cloud_at(given).is_ok() {
                    return Err(invalid(format!("a point cloud {given} exists")));
                }
                given.to_owned()
            }
            None => self.free_point_cloud_name(&format!("{name} Filtered")),
        };
        // Back into the cloud's own coordinates (before its placement).
        let back = rigid_inverse(&self.point_cloud_transform(source));
        let points: Vec<Vec3> = filtered
            .points
            .iter()
            .map(|p| back.apply_point(*p))
            .collect();
        let colors = (!source.colors.is_empty()).then(|| {
            filtered
                .kept
                .iter()
                .map(|&k| source.colors.as_slice()[k])
                .collect()
        });
        let mut cloud = PointCloud::from_data(
            &new_name,
            &source.file_name,
            CloudData {
                points,
                colors,
                file_points: 0,
            },
        )
        .map_err(invalid)?;
        cloud.occurrence = source.occurrence.clone();
        cloud.placement = source.placement;
        cloud.check().map_err(invalid)?;
        let result = CloudFiltered {
            name: new_name,
            count: cloud.points.len(),
            from: count,
            cropped: filtered.cropped,
            statistical: filtered.statistical,
            radius: filtered.radius,
            smoothed: filtered.smoothed,
            largest_move: filtered.largest_move,
        };
        let hide = !settings.keep_shown;
        let label = format!("Filter {name}");
        self.apply_with(|state| {
            if hide {
                state.point_clouds[i].visible = false;
            }
            state.point_clouds.push(cloud);
            Ok((label, (), false))
        })?;
        Ok(result)
    }

    /// Builds a closed mesh of a cloud as a new mesh body in its component
    /// (see the module); refused in words for too few points, settings out
    /// of range or points that enclose no volume.
    pub fn reconstruct_point_cloud(
        &mut self,
        name: &str,
        options: &ReconstructOptions,
    ) -> Result<CloudReconstructed, ModelError> {
        reconstruct::check_options(options).map_err(invalid)?;
        let i = self.cloud_at(name)?;
        let cloud = &self.state.point_clouds[i];
        // In the component's coordinates: the placement on the points.
        let placement = cloud.placement;
        let points: Vec<Vec3> = thinned(cloud.points.len(), MAX_RECONSTRUCT_POINTS)
            .into_iter()
            .map(|k| placement.apply_point(cloud.local_point(k)))
            .collect();
        let component = self.cloud_component(cloud);
        let monitor = self.monitor();
        let mut progress =
            |i: usize, n: usize| monitor.is_none_or(|monitor| monitor.enter(i, n).is_ok());
        let made =
            reconstruct::reconstruct(&points, options, &mut progress).map_err(|e| match e {
                ReconstructError::Cancelled => ModelError::Cancelled,
                ReconstructError::Refused(why) => invalid(format!("{name}: {why}")),
            })?;
        let shape = self
            .kernel()
            .triangle_body(&made.mesh.vertices, &made.mesh.triangles)
            .map_err(|e| invalid(format!("the mesh body of {name}: {e}")))?;
        let input = BaseInput {
            source: Some(name.to_owned()),
            component: Some(component),
            ..BaseInput::new(vec![ImportBody {
                name: Some(format!("{name} Mesh")),
                color: None,
                shape,
                face_colors: Vec::new(),
            }])
        };
        let mut added = self.add_base_features(&format!("Reconstruct {name}"), vec![input])?;
        let imported = added.remove(0);
        let body = imported
            .bodies
            .first()
            .ok_or_else(|| invalid(format!("the mesh body of {name} was not kept")))?;
        Ok(CloudReconstructed {
            feature: imported.feature.uid.to_string(),
            body: body.uid,
            body_name: body.name.clone(),
            points: points.len(),
            vertices: made.mesh.vertices.len(),
            triangles: made.mesh.triangles.len(),
            cell: made.cell,
            shells: made.shells,
            dropped_shells: made.dropped_shells,
            small_shells: made.small_shells,
            closed: made.closed,
            volume: made.volume,
            mean_deviation: made.mean_deviation,
            max_deviation: made.max_deviation,
        })
    }
}
