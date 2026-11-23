// SPDX-License-Identifier: MIT
//! Point clouds (mitcad#2240): the points of a scan kept in the design as a
//! reference to model from and to check a made part against, listed in the
//! browser's Point Clouds folder and saved in the design file with the
//! points embedded (no path to the file is kept). They are not bodies:
//! nothing is built from them. Inserting, moving, showing, hiding, renaming
//! and deleting one is an undo step that recomputes nothing.
//!
//! The points are single precision, relative to the cloud's own origin
//! (double precision, the middle of the points' box when they came in), so
//! a scan far from the design's origin keeps its detail. A cloud is placed
//! in a component (its `occurrence` from the design root, none for the
//! root) and moved there by a rigid motion (`placement`): a point's place in
//! the design is the occurrence's transform after the placement, applied to
//! the origin plus the point. When that component's occurrence (or one
//! above it) is moved to another parent, the path follows it in the same
//! undo step (`DocState::follow`, mitcad#2265), so the cloud stays where
//! it is in the design.
//!
//! Point Cloud Deviation ([`PointCloudDeviation`], an analysis kept in the
//! document as Draft Analysis is) measures each sampled point's distance
//! from a body's faces (`Kernel::boundary_distances`, or
//! `Kernel::boundary_nearest` for some of its faces), signed by
//! `Kernel::points_inside` for a body that encloses a volume: negative
//! inside the material, positive outside. It changes nothing in the design.

use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Value, json};

use super::{Document, ModelError, invalid};
use crate::exchange::BodyKind;
use crate::features::OccurrencePath;
use crate::ids::{BodyUid, ComponentUid};
use crate::kernel::{Kernel, KernelError};
use crate::point_cloud_files::{self, CloudRead, MAX_POINTS};
use crate::topo::FaceName;
use crate::transform::Transform;

type Vec3 = [f64; 3];

/// The largest coordinate (mm) of a cloud's origin and of a point from it.
pub const MAX_COORD: f64 = 1e9;
/// The tolerance of a deviation when none is given, mm.
pub const DEFAULT_DEVIATION_TOLERANCE: f64 = 0.1;
/// The points a deviation samples when no number is given.
pub const DEFAULT_MAX_SAMPLES: usize = 100_000;
/// The most points a deviation samples.
pub const MAX_SAMPLES: usize = 2_000_000;
/// The worst points a deviation lists when no number is given.
pub const DEFAULT_WORST: usize = 10;
/// Within this of a face (mm) a point is on the boundary.
pub const ON_BOUNDARY: f64 = 1e-6;
/// Points measured per kernel call: progress and a cancel between calls.
const CHUNK: usize = 4096;

/// The points of a cloud from its origin, mm: x, y, z single precision,
/// shared by the copies of the document's state that undo keeps (base64 of
/// little-endian floats in files).
#[derive(Clone, Default)]
pub struct CloudPoints(Arc<[[f32; 3]]>);

impl CloudPoints {
    pub fn new(points: Vec<[f32; 3]>) -> Self {
        Self(points.into())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_slice(&self) -> &[[f32; 3]] {
        &self.0
    }

    /// x, y, z per point, little-endian floats.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.0.len() * 12);
        for p in self.0.iter() {
            for c in p {
                bytes.extend_from_slice(&c.to_le_bytes());
            }
        }
        bytes
    }

    /// The points of stored bytes, or why they are no whole points of
    /// finite coordinates.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if !bytes.len().is_multiple_of(12) {
            return Err("the points are not whole x, y, z triples".to_owned());
        }
        if bytes.len() / 12 > MAX_POINTS {
            return Err(format!("the cloud has more than {MAX_POINTS} points"));
        }
        let points: Vec<[f32; 3]> = bytes
            .as_chunks::<12>()
            .0
            .iter()
            .map(|p| {
                std::array::from_fn(|k| {
                    let mut c = [0u8; 4];
                    c.copy_from_slice(&p[4 * k..4 * k + 4]);
                    f32::from_le_bytes(c)
                })
            })
            .collect();
        if !points.iter().flatten().all(|c| c.is_finite()) {
            return Err("a point's coordinates are not finite".to_owned());
        }
        Ok(Self::new(points))
    }

    pub fn base64(&self) -> String {
        crate::base64::encode(&self.to_bytes())
    }
}

impl PartialEq for CloudPoints {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

impl std::fmt::Debug for CloudPoints {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CloudPoints({} points)", self.0.len())
    }
}

impl Serialize for CloudPoints {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.base64())
    }
}

impl<'de> Deserialize<'de> for CloudPoints {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let bytes = crate::base64::decode(&text)
            .map_err(|e| serde::de::Error::custom(format!("the points are no base64: {e}")))?;
        Self::from_bytes(&bytes).map_err(serde::de::Error::custom)
    }
}

/// The colour of each point of a cloud (r, g, b bytes), shared as
/// [`CloudPoints`] are; empty for a cloud without colours.
#[derive(Clone, Default)]
pub struct CloudColors(Arc<[[u8; 3]]>);

impl CloudColors {
    pub fn new(colors: Vec<[u8; 3]>) -> Self {
        Self(colors.into())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_slice(&self) -> &[[u8; 3]] {
        &self.0
    }

    pub fn base64(&self) -> String {
        let bytes: Vec<u8> = self.0.iter().flatten().copied().collect();
        crate::base64::encode(&bytes)
    }
}

impl PartialEq for CloudColors {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

impl std::fmt::Debug for CloudColors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CloudColors({} colours)", self.0.len())
    }
}

impl Serialize for CloudColors {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.base64())
    }
}

impl<'de> Deserialize<'de> for CloudColors {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let bytes = crate::base64::decode(&text)
            .map_err(|e| serde::de::Error::custom(format!("the colours are no base64: {e}")))?;
        if !bytes.len().is_multiple_of(3) {
            return Err(serde::de::Error::custom(
                "the colours are not whole r, g, b triples",
            ));
        }
        Ok(Self::new(bytes.as_chunks::<3>().0.to_vec()))
    }
}

fn shown() -> bool {
    true
}

fn is_shown(visible: &bool) -> bool {
    *visible
}

fn identity() -> Transform {
    Transform::IDENTITY
}

/// A point cloud kept in the design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointCloud {
    pub name: String,
    /// Its light bulb; left out of files when true.
    #[serde(default = "shown", skip_serializing_if = "is_shown")]
    pub visible: bool,
    /// The occurrence of the component it is placed in, from the design
    /// root; none for the root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<OccurrencePath>,
    /// Where it is moved to in that component: a rigid motion.
    #[serde(default = "identity", skip_serializing_if = "Transform::is_identity")]
    pub placement: Transform,
    /// The points' own origin, mm.
    pub origin: Vec3,
    /// The name of the file it came from, for the user; no path.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file_name: String,
    /// The points the file held (more than kept when it was thinned).
    #[serde(default)]
    pub file_points: usize,
    /// The points from `origin`, mm.
    pub points: CloudPoints,
    /// The file's colour of each point, when it has them.
    #[serde(default, skip_serializing_if = "CloudColors::is_empty")]
    pub colors: CloudColors,
}

/// Points for a new cloud, mm, in its component's coordinates.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CloudData {
    pub points: Vec<Vec3>,
    pub colors: Option<Vec<[u8; 3]>>,
    /// The points the file held (0: as many as given).
    pub file_points: usize,
}

impl From<CloudRead> for CloudData {
    fn from(read: CloudRead) -> Self {
        Self {
            points: read.points,
            colors: read.colors,
            file_points: read.file_points,
        }
    }
}

impl PointCloud {
    /// A cloud of the points, its origin the middle of their box, shown,
    /// not moved, in the root.
    pub fn from_data(name: &str, file_name: &str, data: CloudData) -> Result<Self, String> {
        if data.points.is_empty() {
            return Err("the point cloud has no points".to_owned());
        }
        if data.points.len() > MAX_POINTS {
            return Err(format!("the point cloud has more than {MAX_POINTS} points"));
        }
        if let Some(colors) = &data.colors
            && colors.len() != data.points.len()
        {
            return Err("the point cloud's colours do not match its points".to_owned());
        }
        let mut low = [f64::INFINITY; 3];
        let mut high = [f64::NEG_INFINITY; 3];
        for p in &data.points {
            if !p.iter().all(|c| c.is_finite() && c.abs() <= MAX_COORD) {
                return Err("a point of the cloud is not a finite place in range".to_owned());
            }
            for k in 0..3 {
                low[k] = low[k].min(p[k]);
                high[k] = high[k].max(p[k]);
            }
        }
        let origin: Vec3 = std::array::from_fn(|k| (low[k] + high[k]) / 2.0);
        // Single precision from the middle: within the box's half size.
        let points = data
            .points
            .iter()
            .map(|p| std::array::from_fn(|k| (p[k] - origin[k]) as f32))
            .collect();
        let count = data.points.len();
        Ok(Self {
            name: name.to_owned(),
            visible: true,
            occurrence: None,
            placement: Transform::IDENTITY,
            origin,
            file_name: file_name.to_owned(),
            file_points: if data.file_points == 0 {
                count
            } else {
                data.file_points
            },
            points: CloudPoints::new(points),
            colors: data.colors.map(CloudColors::new).unwrap_or_default(),
        })
    }

    /// Why the cloud cannot be kept, if it cannot.
    pub(crate) fn check(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("the point cloud needs a name".to_owned());
        }
        let bad = |what: &str| Err(format!("{}: {what}", self.name));
        if self.points.is_empty() {
            return bad("the point cloud has no points");
        }
        if !self.colors.is_empty() && self.colors.len() != self.points.len() {
            return bad("its colours do not match its points");
        }
        if !self
            .origin
            .iter()
            .all(|c| c.is_finite() && c.abs() <= MAX_COORD)
        {
            return bad("its origin is out of range");
        }
        if !self.placement.is_finite() || !self.placement.is_rigid() {
            return bad("its placement is not a rigid motion");
        }
        Ok(())
    }

    /// The `i`-th point in its component's coordinates before the
    /// placement (the origin plus the point).
    pub fn local_point(&self, i: usize) -> Vec3 {
        let p = self.points.as_slice()[i];
        std::array::from_fn(|k| self.origin[k] + f64::from(p[k]))
    }

    /// The lowest and highest coordinates of the points from the origin.
    fn box_from_origin(&self) -> ([f64; 3], [f64; 3]) {
        let mut low = [f64::INFINITY; 3];
        let mut high = [f64::NEG_INFINITY; 3];
        for p in self.points.as_slice() {
            for k in 0..3 {
                low[k] = low[k].min(f64::from(p[k]));
                high[k] = high[k].max(f64::from(p[k]));
            }
        }
        (low, high)
    }
}

/// The inverse of a rigid motion.
fn rigid_inverse(t: &Transform) -> Transform {
    let linear: [[f64; 3]; 3] = std::array::from_fn(|r| std::array::from_fn(|c| t.linear[c][r]));
    let back = Transform {
        linear,
        translation: [0.0; 3],
    };
    let moved = back.apply_vector(t.translation);
    Transform {
        linear,
        translation: moved.map(|v| -v),
    }
}

/// An insert's file and settings.
#[derive(Debug, Clone, PartialEq)]
pub struct CloudSource {
    pub bytes: Vec<u8>,
    /// Its name (no path), which also tells CSV from XYZ text.
    pub file_name: String,
}

/// Where a new cloud goes and how its file is read.
#[derive(Debug, Clone, PartialEq)]
pub struct CloudInsert {
    /// The next free `PointCloud<n>` when none.
    pub name: Option<String>,
    pub occurrence: Option<OccurrencePath>,
    /// Millimetres per length of the file (1: millimetres).
    pub scale: f64,
    /// The most points kept; more are thinned evenly.
    pub max_points: usize,
}

impl Default for CloudInsert {
    fn default() -> Self {
        Self {
            name: None,
            occurrence: None,
            scale: 1.0,
            max_points: point_cloud_files::DEFAULT_MAX_POINTS,
        }
    }
}

fn default_tolerance() -> f64 {
    DEFAULT_DEVIATION_TOLERANCE
}

fn default_max_samples() -> usize {
    DEFAULT_MAX_SAMPLES
}

/// Point Cloud Deviation (mitcad#2240), kept as an analysis: each sampled
/// point of a cloud by its distance from a body's faces (some of them, or
/// all), signed for a body that encloses a volume (negative inside the
/// material), with the statistics and the worst points. The body is
/// measured where Print Analysis places it
/// ([`Document::print_placements`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointCloudDeviation {
    /// The point cloud, by its name.
    pub cloud: String,
    pub body: BodyUid,
    /// Measured from these faces only; every face of the body when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<FaceName>,
    /// A point at most this far from the faces (mm) is within tolerance.
    #[serde(default = "default_tolerance")]
    pub tolerance: f64,
    /// At most this many points are measured: every n-th of a larger cloud.
    #[serde(default = "default_max_samples")]
    pub max_samples: usize,
    /// Signed by the side of the faces: true asks for it (refused for a
    /// body that encloses no volume), false measures without a sign, none
    /// signs for a closed solid only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed: Option<bool>,
    /// The colour map's limit (mm): the scale runs from minus it to it;
    /// none: the largest distance measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
}

impl PointCloudDeviation {
    /// Why the settings cannot be kept, if they cannot (not the geometry).
    pub(crate) fn check(&self) -> Result<(), String> {
        if self.cloud.trim().is_empty() {
            return Err("the deviation needs a point cloud".to_owned());
        }
        check_deviation_settings(self.tolerance, self.max_samples, self.limit)
    }
}

/// Why a deviation's tolerance, sample count or colour limit is not one.
pub fn check_deviation_settings(
    tolerance: f64,
    max_samples: usize,
    limit: Option<f64>,
) -> Result<(), String> {
    if !(tolerance > 0.0 && tolerance.is_finite()) {
        return Err(format!(
            "the tolerance must be greater than zero, got {tolerance}"
        ));
    }
    if max_samples == 0 || max_samples > MAX_SAMPLES {
        return Err(format!(
            "the points measured must be from 1 to {MAX_SAMPLES}, got {max_samples}"
        ));
    }
    if let Some(limit) = limit
        && !(limit > 0.0 && limit.is_finite())
    {
        return Err(format!(
            "the colour scale's limit must be greater than zero, got {limit}"
        ));
    }
    Ok(())
}

/// A point of a deviation: its index in the cloud, its place in the design,
/// its distance and the nearest point of the faces, when known.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviationPoint {
    pub index: usize,
    pub point: Vec3,
    pub distance: f64,
    pub closest: Option<Vec3>,
}

/// What a deviation measured (mm).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Deviation {
    pub signed: bool,
    pub tolerance: f64,
    /// The cloud's points.
    pub points: usize,
    /// Every `step`-th point was measured: `sampled` of them.
    pub step: usize,
    pub sampled: usize,
    /// Points whose distance could not be measured.
    pub failures: usize,
    pub mean_abs: f64,
    /// The mean of the signed distances (of the distances without a sign).
    pub mean_signed: f64,
    pub rms: f64,
    pub max_abs: f64,
    /// The smallest and largest distance (signed when signed).
    pub min: f64,
    pub max: f64,
    pub within_tolerance: usize,
    pub outside: usize,
    pub inside: usize,
    pub on_boundary: usize,
    /// The farthest points, the farthest first.
    pub worst: Vec<DeviationPoint>,
    /// Each sampled point's distance (NaN where it failed), in the cloud's
    /// order: the `i`-th is point `i * step`'s.
    pub distances: Vec<f64>,
}

impl Deviation {
    /// The points measured.
    pub fn measured(&self) -> usize {
        self.sampled - self.failures
    }

    /// The share of the measured points within tolerance, 0 to 1.
    pub fn within_tolerance_fraction(&self) -> f64 {
        let measured = self.measured();
        if measured == 0 {
            0.0
        } else {
            self.within_tolerance as f64 / measured as f64
        }
    }

    /// The answer of the `point_cloud_deviation` query; the distances
    /// only when asked for.
    pub fn json(&self, with_distances: bool) -> Value {
        let worst: Vec<Value> = self
            .worst
            .iter()
            .map(|w| {
                let mut value = json!({
                    "index": w.index,
                    "point": w.point,
                    "distance": w.distance,
                });
                if let Some(closest) = w.closest {
                    value["closest"] = json!(closest);
                }
                value
            })
            .collect();
        let mut value = json!({
            "signed": self.signed,
            "tolerance": self.tolerance,
            "points": self.points,
            "step": self.step,
            "sampled": self.sampled,
            "measured": self.measured(),
            "failures": self.failures,
            "mean_abs_mm": self.mean_abs,
            "mean_signed_mm": self.mean_signed,
            "rms_mm": self.rms,
            "max_abs_mm": self.max_abs,
            "min_mm": self.min,
            "max_mm": self.max,
            "within_tolerance": self.within_tolerance,
            "within_tolerance_fraction": self.within_tolerance_fraction(),
            "outside": self.outside,
            "inside": self.inside,
            "on_boundary": self.on_boundary,
            "worst": worst,
        });
        if with_distances {
            // NaN is no JSON number: a failed point is null.
            value["distances"] = Value::Array(
                self.distances
                    .iter()
                    .map(|d| if d.is_finite() { json!(d) } else { Value::Null })
                    .collect(),
            );
        }
        value
    }
}

/// The statistics of measured distances (NaN where a point failed).
fn statistics(distances: &[f64], tolerance: f64, signed: bool) -> Deviation {
    let mut out = Deviation {
        signed,
        tolerance,
        sampled: distances.len(),
        min: f64::INFINITY,
        max: f64::NEG_INFINITY,
        ..Deviation::default()
    };
    let (mut sum_abs, mut sum, mut squares) = (0.0, 0.0, 0.0);
    for &d in distances {
        if !d.is_finite() {
            out.failures += 1;
            continue;
        }
        let a = d.abs();
        sum_abs += a;
        sum += d;
        squares += d * d;
        out.max_abs = out.max_abs.max(a);
        out.min = out.min.min(d);
        out.max = out.max.max(d);
        if a <= tolerance {
            out.within_tolerance += 1;
        }
        if a <= ON_BOUNDARY {
            out.on_boundary += 1;
        } else if d < 0.0 {
            out.inside += 1;
        } else {
            out.outside += 1;
        }
    }
    let measured = out.measured();
    if measured > 0 {
        let n = measured as f64;
        out.mean_abs = sum_abs / n;
        out.mean_signed = sum / n;
        out.rms = (squares / n).sqrt();
    } else {
        out.min = 0.0;
        out.max = 0.0;
    }
    out
}

impl<K: Kernel> Document<K> {
    /// The point clouds in the order they were inserted.
    pub fn point_clouds(&self) -> &[PointCloud] {
        &self.state.point_clouds
    }

    fn cloud_index(&self, name: &str) -> Result<usize, ModelError> {
        self.state
            .point_clouds
            .iter()
            .position(|c| c.name == name)
            .ok_or_else(|| invalid(format!("there is no point cloud {name}")))
    }

    /// The transform of a cloud's points (its origin plus a point) into
    /// the design's coordinates at the marker.
    pub fn point_cloud_transform(&self, cloud: &PointCloud) -> Transform {
        let occurrence = cloud
            .occurrence
            .as_ref()
            .map(|o| self.path_transform(&o.0))
            .unwrap_or(Transform::IDENTITY);
        occurrence.after(&cloud.placement)
    }

    /// Why an occurrence path is no path of the design, if it is not.
    fn check_cloud_occurrence(&self, occurrence: Option<&OccurrencePath>) -> Result<(), String> {
        let Some(path) = occurrence else {
            return Ok(());
        };
        for uid in &path.0 {
            if self.state.assembly.occurrence(*uid).is_none() {
                return Err(format!("there is no occurrence {uid}"));
            }
        }
        Ok(())
    }

    /// The next free `PointCloud<n>`.
    fn free_cloud_name(&self) -> String {
        (1..)
            .map(|n| format!("PointCloud{n}"))
            .find(|name| self.cloud_index(name).is_err())
            .expect("a free name")
    }

    /// Inserts a point cloud read from a file (see
    /// [`crate::point_cloud_files`]), shown, in the occurrence given (the
    /// active component's first occurrence without one, the root when the
    /// root is active). Returns its name and the reader's warnings
    /// (thinning, unreadable points).
    pub fn insert_point_cloud(
        &mut self,
        source: CloudSource,
        insert: CloudInsert,
    ) -> Result<(String, Vec<String>), ModelError> {
        let read = point_cloud_files::read_point_cloud(
            &source.bytes,
            &source.file_name,
            insert.scale,
            insert.max_points,
        )
        .map_err(|e| invalid(format!("{}: {e}", source.file_name)))?;
        let warnings = read.warnings.clone();
        // Where new things go: the active component's first occurrence.
        let occurrence = insert
            .occurrence
            .or_else(|| self.component_cloud_occurrence(self.active_component()));
        let name = self.add_point_cloud(
            insert.name.as_deref(),
            &source.file_name,
            CloudData::from(read),
            occurrence,
            Transform::IDENTITY,
            true,
        )?;
        Ok((name, warnings))
    }

    /// Adds a point cloud of the points given (in the occurrence's
    /// component's coordinates before `placement`): what an insert reads
    /// and what the imports bring (which take a free name first,
    /// [`Self::free_point_cloud_name`]). Without a name it is the next free
    /// `PointCloud<n>`; a name taken is refused. Returns the name.
    pub fn add_point_cloud(
        &mut self,
        name: Option<&str>,
        file_name: &str,
        data: CloudData,
        occurrence: Option<OccurrencePath>,
        placement: Transform,
        visible: bool,
    ) -> Result<String, ModelError> {
        let name = match name.map(str::trim).filter(|n| !n.is_empty()) {
            Some(name) => name.to_owned(),
            None => self.free_cloud_name(),
        };
        if self.cloud_index(&name).is_ok() {
            return Err(invalid(format!("a point cloud {name} exists")));
        }
        self.check_cloud_occurrence(occurrence.as_ref())
            .map_err(|e| invalid(format!("the point cloud's component: {e}")))?;
        let mut cloud = PointCloud::from_data(&name, file_name, data).map_err(invalid)?;
        cloud.occurrence = occurrence.filter(|o| !o.is_empty());
        cloud.placement = placement;
        cloud.visible = visible;
        cloud.check().map_err(invalid)?;
        let label = format!("Insert {name}");
        self.apply_with(|state| {
            state.point_clouds.push(cloud);
            Ok((label, (), false))
        })?;
        Ok(name)
    }

    /// The first occurrence path from the root to a component (none for
    /// the root): where an import puts the clouds it brings into it.
    pub fn component_cloud_occurrence(&self, component: ComponentUid) -> Option<OccurrencePath> {
        if component.is_root() {
            return None;
        }
        self.state
            .assembly
            .paths_to(component)
            .into_iter()
            .next()
            .map(OccurrencePath)
    }

    /// A free name for an imported cloud: `name`, else `name (2)` …
    pub fn free_point_cloud_name(&self, name: &str) -> String {
        let name = name.trim();
        if name.is_empty() {
            return self.free_cloud_name();
        }
        if self.cloud_index(name).is_err() {
            return name.to_owned();
        }
        (2..)
            .map(|n| format!("{name} ({n})"))
            .find(|candidate| self.cloud_index(candidate).is_err())
            .expect("a free name")
    }

    /// Moves a cloud by a rigid motion in its component's coordinates
    /// (after its placement). A motion that moves nothing is no undo step.
    pub fn move_point_cloud(&mut self, name: &str, motion: Transform) -> Result<(), ModelError> {
        let i = self.cloud_index(name)?;
        if !motion.is_finite() || !motion.is_rigid() {
            return Err(invalid("a point cloud is moved by a rigid motion only"));
        }
        if motion.is_identity() {
            return Ok(());
        }
        let mut cloud = self.state.point_clouds[i].clone();
        cloud.placement = motion.after(&cloud.placement);
        cloud.check().map_err(invalid)?;
        let label = format!("Move {name}");
        self.apply_with(|state| {
            state.point_clouds[i] = cloud;
            Ok((label, (), false))
        })
    }

    /// Puts a cloud into another occurrence's component (none: the root),
    /// its placement kept, or back where it came in (`reset`: no
    /// placement). Nothing that changes nothing is an undo step.
    pub fn place_point_cloud(
        &mut self,
        name: &str,
        occurrence: Option<Option<OccurrencePath>>,
        reset: bool,
    ) -> Result<(), ModelError> {
        let i = self.cloud_index(name)?;
        let mut cloud = self.state.point_clouds[i].clone();
        if let Some(occurrence) = occurrence {
            self.check_cloud_occurrence(occurrence.as_ref())
                .map_err(|e| invalid(format!("the point cloud's component: {e}")))?;
            cloud.occurrence = occurrence.filter(|o| !o.is_empty());
        }
        if reset {
            cloud.placement = Transform::IDENTITY;
        }
        if cloud == self.state.point_clouds[i] {
            return Ok(());
        }
        let label = format!("Move {name}");
        self.apply_with(|state| {
            state.point_clouds[i] = cloud;
            Ok((label, (), false))
        })
    }

    /// Shows or hides a cloud; an undo step unless nothing changes.
    pub fn set_point_cloud_visible(&mut self, name: &str, visible: bool) -> Result<(), ModelError> {
        let i = self.cloud_index(name)?;
        if self.state.point_clouds[i].visible == visible {
            return Ok(());
        }
        let label = format!("{} {name}", if visible { "Show" } else { "Hide" });
        self.apply_with(|state| {
            state.point_clouds[i].visible = visible;
            Ok((label, (), false))
        })
    }

    /// Renames a cloud; the deviations of it follow it.
    pub fn rename_point_cloud(&mut self, name: &str, new_name: &str) -> Result<(), ModelError> {
        let i = self.cloud_index(name)?;
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(invalid("the point cloud needs a name"));
        }
        if new_name == name {
            return Ok(());
        }
        if self.cloud_index(new_name).is_ok() {
            return Err(invalid(format!("a point cloud {new_name} exists")));
        }
        let label = format!("Rename {name}");
        let (old, new_name) = (name.to_owned(), new_name.to_owned());
        self.apply_with(|state| {
            state.point_clouds[i].name.clone_from(&new_name);
            for analysis in &mut state.analyses {
                if let super::AnalysisDef::PointCloudDeviation(deviation) = &mut analysis.def
                    && deviation.cloud == old
                {
                    deviation.cloud.clone_from(&new_name);
                }
            }
            Ok((label, (), false))
        })
    }

    /// Deletes a cloud; a deviation of it then says it is gone.
    pub fn delete_point_cloud(&mut self, name: &str) -> Result<(), ModelError> {
        let i = self.cloud_index(name)?;
        let label = format!("Delete {name}");
        self.apply_with(|state| {
            state.point_clouds.remove(i);
            Ok((label, (), false))
        })
    }

    /// The `point_clouds` query: each cloud's settings without its points,
    /// with its point count, whether it has colours, and its box in the
    /// design at the marker (`bounds`, the box of its box's corners).
    pub fn point_clouds_json(&self) -> Value {
        Value::Array(
            self.state
                .point_clouds
                .iter()
                .map(|cloud| {
                    let transform = self.point_cloud_transform(cloud);
                    let (low, high) = cloud.box_from_origin();
                    let mut min = [f64::INFINITY; 3];
                    let mut max = [f64::NEG_INFINITY; 3];
                    for corner in 0..8 {
                        let local: Vec3 = std::array::from_fn(|k| {
                            cloud.origin[k]
                                + if corner & (1 << k) == 0 {
                                    low[k]
                                } else {
                                    high[k]
                                }
                        });
                        let at = transform.apply_point(local);
                        for k in 0..3 {
                            min[k] = min[k].min(at[k]);
                            max[k] = max[k].max(at[k]);
                        }
                    }
                    json!({
                        "name": cloud.name,
                        "visible": cloud.visible,
                        "occurrence": cloud.occurrence,
                        "placement": cloud.placement,
                        "origin": cloud.origin,
                        "file_name": cloud.file_name,
                        "file_points": cloud.file_points,
                        "count": cloud.points.len(),
                        "colored": !cloud.colors.is_empty(),
                        "bounds": [min, max],
                    })
                })
                .collect(),
        )
    }

    /// The `point_cloud_points` query: a cloud's points for drawing, base64
    /// of little-endian floats from its origin, with the transform that
    /// takes them into the design (`linear`, row by row, and `translation`,
    /// which includes the origin), and its colours (bytes, base64) when it
    /// has them.
    pub fn point_cloud_points_json(&self, name: &str) -> Result<Value, ModelError> {
        let cloud = &self.state.point_clouds[self.cloud_index(name)?];
        let transform = self.point_cloud_transform(cloud);
        Ok(json!({
            "name": cloud.name,
            "count": cloud.points.len(),
            "linear": transform.linear,
            "translation": transform.apply_point(cloud.origin),
            "points": cloud.points.base64(),
            "colors": if cloud.colors.is_empty() { Value::Null } else { json!(cloud.colors.base64()) },
        }))
    }

    /// Why a deviation cannot be measured at the marker, if it cannot: its
    /// cloud or body gone (no measuring).
    pub fn point_cloud_deviation_check(&self, def: &PointCloudDeviation) -> Result<(), String> {
        if self.cloud_index(&def.cloud).is_err() {
            return Err(format!("the point cloud {} is gone", def.cloud));
        }
        if self.body_shape(def.body).is_none() {
            return Err(format!(
                "body {} does not exist at the timeline marker",
                def.body
            ));
        }
        Ok(())
    }

    /// Measures a deviation (see the module): each sampled point's distance
    /// from the body's faces, its statistics and the `worst` farthest
    /// points with the nearest points of the faces. Errors are in words:
    /// the cloud or body gone, a sign asked of an open body, nothing
    /// measured. A cancelled model job stops between the kernel's calls.
    pub fn point_cloud_deviation(
        &self,
        def: &PointCloudDeviation,
        worst: usize,
    ) -> Result<Deviation, ModelError> {
        def.check().map_err(invalid)?;
        self.point_cloud_deviation_check(def).map_err(invalid)?;
        let cloud = &self.state.point_clouds[self.cloud_index(&def.cloud)?];
        let shape = self
            .body_shape(def.body)
            .ok_or_else(|| invalid(format!("body {} does not exist", def.body)))?;
        let kernel = self.kernel();
        let closed = matches!(kernel.body_kind(shape), Ok(BodyKind::Solid));
        let signed = match def.signed {
            Some(true) if !closed => {
                return Err(invalid(
                    "the body encloses no volume (it is open or a mesh), so its points have no \
                     inside: measure the deviation without a sign",
                ));
            }
            Some(asked) => asked,
            None => closed,
        };
        // The cloud's points into the body's own coordinates.
        let body_placement = self
            .print_placements(&[def.body])
            .into_iter()
            .next()
            .unwrap_or(Transform::IDENTITY);
        let to_body = rigid_inverse(&body_placement).after(&self.point_cloud_transform(cloud));
        let count = cloud.points.len();
        let step = point_cloud_files::thinning_step(count, def.max_samples);
        let indices: Vec<usize> = (0..count).step_by(step).collect();
        let points: Vec<Vec3> = indices
            .iter()
            .map(|&i| to_body.apply_point(cloud.local_point(i)))
            .collect();
        let mut distances = vec![f64::NAN; points.len()];
        let mut nearest: Vec<Option<Vec3>> = vec![None; points.len()];
        let chunks = points.len().div_ceil(CHUNK);
        let monitor = self.monitor();
        let mut last_error = None;
        for (c, chunk) in points.chunks(CHUNK).enumerate() {
            if let Some(monitor) = monitor {
                monitor
                    .enter(c, chunks)
                    .map_err(|_| ModelError::Cancelled)?;
            }
            let at = c * CHUNK;
            let measured: Result<Vec<f64>, KernelError> = if def.faces.is_empty() {
                kernel.boundary_distances(shape, chunk)
            } else {
                kernel
                    .boundary_nearest(shape, &def.faces, chunk)
                    .map(|found| {
                        found
                            .into_iter()
                            .enumerate()
                            .map(|(k, (d, q))| {
                                nearest[at + k] = Some(q);
                                d
                            })
                            .collect()
                    })
            };
            let measured = match measured {
                Ok(d) if d.len() == chunk.len() => d,
                Ok(_) => {
                    last_error =
                        Some("the kernel measured a different number of points".to_owned());
                    continue;
                }
                Err(KernelError::Unsupported(what)) => {
                    return Err(invalid(format!("the kernel cannot measure {what}")));
                }
                Err(error) => {
                    last_error = Some(error.to_string());
                    continue;
                }
            };
            let inside = if signed {
                match kernel.points_inside(shape, chunk) {
                    Ok(inside) if inside.len() == chunk.len() => Some(inside),
                    Ok(_) | Err(_) => {
                        last_error =
                            Some("the points could not be told inside or outside".to_owned());
                        continue;
                    }
                }
            } else {
                None
            };
            for (k, d) in measured.into_iter().enumerate() {
                let sign = match &inside {
                    Some(inside) if inside[k] => -1.0,
                    _ => 1.0,
                };
                distances[at + k] = sign * d.abs();
            }
        }
        if let Some(monitor) = monitor {
            monitor
                .enter(chunks, chunks)
                .map_err(|_| ModelError::Cancelled)?;
        }
        let mut result = statistics(&distances, def.tolerance, signed);
        if result.measured() == 0 {
            let why = last_error.unwrap_or_else(|| "no point was measured".to_owned());
            return Err(invalid(format!(
                "the deviation could not be measured: {why}"
            )));
        }
        result.points = count;
        result.step = step;
        // The farthest points, with the nearest points of the faces.
        let mut order: Vec<usize> = (0..distances.len())
            .filter(|&k| distances[k].is_finite())
            .collect();
        order.sort_by(|&a, &b| distances[b].abs().total_cmp(&distances[a].abs()));
        order.truncate(worst);
        if def.faces.is_empty() && !order.is_empty() {
            let asked: Vec<Vec3> = order.iter().map(|&k| points[k]).collect();
            if let Ok(found) = kernel.boundary_nearest(shape, &[], &asked)
                && found.len() == asked.len()
            {
                for (&k, (_, q)) in order.iter().zip(found) {
                    nearest[k] = Some(q);
                }
            }
        }
        result.worst = order
            .iter()
            .map(|&k| DeviationPoint {
                index: indices[k],
                point: body_placement.apply_point(points[k]),
                distance: distances[k],
                closest: nearest[k].map(|q| body_placement.apply_point(q)),
            })
            .collect();
        result.distances = distances;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_round_trip_through_their_bytes() {
        let points = CloudPoints::new(vec![[1.0, -2.5, 3.25], [0.0, 1e6, -7.0]]);
        let back = CloudPoints::from_bytes(&points.to_bytes()).unwrap();
        assert_eq!(back, points);
        let json = serde_json::to_string(&points).unwrap();
        assert_eq!(serde_json::from_str::<CloudPoints>(&json).unwrap(), points);
        assert!(CloudPoints::from_bytes(&[0; 13]).is_err());
        let nan = f32::NAN.to_le_bytes();
        let bad: Vec<u8> = [nan, nan, nan].concat();
        assert!(CloudPoints::from_bytes(&bad).is_err());
    }

    #[test]
    fn a_cloud_far_from_the_origin_keeps_a_micrometre() {
        // A part 2 km off the origin, its points a micrometre apart.
        let base = [2.0e6, -1.5e6, 3.0e5];
        let points: Vec<Vec3> = (0..50)
            .map(|i| {
                let t = f64::from(i);
                [
                    base[0] + 0.001 * t,
                    base[1] + 40.0 - 0.001 * t,
                    base[2] + 0.5 * t,
                ]
            })
            .collect();
        let cloud = PointCloud::from_data(
            "Scan",
            "scan.xyz",
            CloudData {
                points: points.clone(),
                colors: None,
                file_points: 0,
            },
        )
        .unwrap();
        for (i, p) in points.iter().enumerate() {
            let back = cloud.local_point(i);
            for k in 0..3 {
                assert!((back[k] - p[k]).abs() < 1e-4, "{back:?} {p:?}");
            }
        }
        // Neighbours stay a micrometre apart.
        let gap = cloud.local_point(1)[0] - cloud.local_point(0)[0];
        assert!((gap - 0.001).abs() < 1e-5, "{gap}");
        assert_eq!(cloud.file_points, 50);
    }

    #[test]
    fn statistics_of_signed_distances() {
        let d = [0.5, 0.5, -0.3, -0.3, f64::NAN, 0.0];
        let s = statistics(&d, 0.4, true);
        assert_eq!(s.sampled, 6);
        assert_eq!(s.failures, 1);
        assert_eq!(s.measured(), 5);
        assert!((s.mean_abs - 1.6 / 5.0).abs() < 1e-12);
        assert!((s.mean_signed - 0.4 / 5.0).abs() < 1e-12);
        assert!((s.max_abs - 0.5).abs() < 1e-12);
        assert_eq!((s.min, s.max), (-0.3, 0.5));
        assert_eq!((s.outside, s.inside, s.on_boundary), (2, 2, 1));
        assert_eq!(s.within_tolerance, 3);
        assert!((s.rms - ((0.25 * 2.0 + 0.09 * 2.0) / 5.0_f64).sqrt()).abs() < 1e-12);
    }

    #[test]
    fn a_rigid_inverse_undoes_its_motion() {
        let motion = Transform::rotation([1.0, 2.0, 3.0], [0.3, -1.0, 0.5], 0.7).unwrap();
        let back = rigid_inverse(&motion);
        let p = [5.0, -6.0, 7.5];
        let q = back.apply_point(motion.apply_point(p));
        for k in 0..3 {
            assert!((q[k] - p[k]).abs() < 1e-12);
        }
    }
}
