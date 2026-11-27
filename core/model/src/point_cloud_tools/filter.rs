// SPDX-License-Identifier: MIT
//! Cleaning a point cloud (mitcad#2516): a crop by a box or a lasso,
//! outliers taken out (statistical: points whose neighbours are much
//! farther than the cloud's are on average; by radius: points with too few
//! neighbours within a radius) and noise smoothed (each point moved onto
//! the plane of its neighbours). The points are kept or not and smoothed
//! as plain points; the document makes a new cloud of those kept and keeps
//! the cloud it came from.

use mitcad_vec3::{add_scaled, cross, dot, sub, unit};

use super::kd::KdTree;
use super::{Vec3, centroid_and_scatter, symmetric_eigen};

/// The most neighbours a filter looks at.
pub const MAX_NEIGHBOURS: usize = 200;

/// Statistical outliers: a point whose mean distance to its `neighbours`
/// nearest points is more than `ratio` standard deviations above the mean
/// of those distances over the cloud goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Statistical {
    pub neighbours: usize,
    pub ratio: f64,
}

/// Outliers by radius: a point with fewer than `neighbours` other points
/// within `radius` (mm) goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ByRadius {
    pub radius: f64,
    pub neighbours: usize,
}

/// Noise: each point moved onto the plane fitted to its `neighbours`
/// nearest points (itself among them), `passes` times.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Smooth {
    pub neighbours: usize,
    pub passes: usize,
}

/// A box, its lowest and highest corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CropBox {
    pub min: Vec3,
    pub max: Vec3,
    /// Keeps the points inside (true) or those outside.
    pub keep_inside: bool,
}

/// A lasso: a polygon on a plane whose normal points to the viewer; a
/// point is inside when it lies inside the polygon seen along the normal.
#[derive(Debug, Clone, PartialEq)]
pub struct CropLasso {
    pub normal: Vec3,
    pub points: Vec<Vec3>,
    pub keep_inside: bool,
}

/// What a filter does; each part is optional. The crops come first, then
/// the outliers (statistical, then by radius) of what they keep, then the
/// smoothing of what is left.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FilterOptions {
    pub crop_box: Option<CropBox>,
    pub lasso: Option<CropLasso>,
    pub statistical: Option<Statistical>,
    pub radius: Option<ByRadius>,
    pub smooth: Option<Smooth>,
}

impl FilterOptions {
    pub fn is_empty(&self) -> bool {
        self.crop_box.is_none()
            && self.lasso.is_none()
            && self.statistical.is_none()
            && self.radius.is_none()
            && self.smooth.is_none()
    }

    /// Why the settings are none, if they are not.
    pub fn check(&self) -> Result<(), String> {
        if self.is_empty() {
            return Err(
                "choose what the filter does: outliers, noise, or a crop by a box or a lasso"
                    .to_owned(),
            );
        }
        if let Some(s) = &self.statistical {
            if !(1..=MAX_NEIGHBOURS).contains(&s.neighbours) {
                return Err(format!(
                    "the statistical outliers must use from 1 to {MAX_NEIGHBOURS} neighbors, got {}",
                    s.neighbours
                ));
            }
            if !(s.ratio > 0.0 && s.ratio.is_finite()) {
                return Err(format!(
                    "the statistical outliers' spread must be greater than zero, got {}",
                    s.ratio
                ));
            }
        }
        if let Some(r) = &self.radius {
            if !(1..=MAX_NEIGHBOURS).contains(&r.neighbours) {
                return Err(format!(
                    "the outliers by radius must use from 1 to {MAX_NEIGHBOURS} neighbors, got {}",
                    r.neighbours
                ));
            }
            if !(r.radius > 0.0 && r.radius.is_finite()) {
                return Err(format!(
                    "the outliers' radius must be greater than zero, got {} mm",
                    r.radius
                ));
            }
        }
        if let Some(s) = &self.smooth {
            if !(3..=MAX_NEIGHBOURS).contains(&s.neighbours) {
                return Err(format!(
                    "the noise smoothing must use from 3 to {MAX_NEIGHBOURS} neighbors, got {}",
                    s.neighbours
                ));
            }
            if !(1..=20).contains(&s.passes) {
                return Err(format!(
                    "the noise smoothing's passes must be from 1 to 20, got {}",
                    s.passes
                ));
            }
        }
        if let Some(b) = &self.crop_box
            && !(0..3).all(|k| b.min[k].is_finite() && b.max[k].is_finite() && b.min[k] < b.max[k])
        {
            return Err("the crop box's lowest corner must be below its highest one".to_owned());
        }
        if let Some(l) = &self.lasso {
            if l.points.len() < 3 {
                return Err(format!(
                    "a lasso needs at least three points, got {}",
                    l.points.len()
                ));
            }
            if unit(l.normal).is_none() {
                return Err("the lasso's view direction has no length".to_owned());
            }
        }
        Ok(())
    }
}

/// What a filter did: which of the points given are kept (in their order),
/// where the kept ones are after the smoothing, and how many each part took.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Filtered {
    pub kept: Vec<usize>,
    pub points: Vec<Vec3>,
    pub cropped: usize,
    pub statistical: usize,
    pub radius: usize,
    /// The points the smoothing moved (by more than a nanometre).
    pub smoothed: usize,
    /// The largest move of the smoothing (mm).
    pub largest_move: f64,
}

/// Filters the points (see [`FilterOptions`]): refused in words for
/// settings that are none, too few points for the neighbours asked, or
/// nothing left.
pub fn filter(points: &[Vec3], options: &FilterOptions) -> Result<Filtered, String> {
    options.check()?;
    let mut kept: Vec<usize> = (0..points.len()).collect();
    let mut out = Filtered::default();
    if let Some(b) = &options.crop_box {
        let before = kept.len();
        kept.retain(|&i| {
            let p = points[i];
            (0..3).all(|k| p[k] >= b.min[k] && p[k] <= b.max[k]) == b.keep_inside
        });
        out.cropped += before - kept.len();
    }
    if let Some(l) = &options.lasso {
        let before = kept.len();
        let normal = unit(l.normal).unwrap_or([0.0, 0.0, 1.0]);
        let u = crate::transform::perpendicular(normal);
        let v = cross(normal, u);
        let polygon: Vec<[f64; 2]> = l.points.iter().map(|p| [dot(*p, u), dot(*p, v)]).collect();
        kept.retain(|&i| {
            let p = points[i];
            inside(&polygon, [dot(p, u), dot(p, v)]) == l.keep_inside
        });
        out.cropped += before - kept.len();
    }
    if kept.is_empty() {
        return Err("the crop leaves no point".to_owned());
    }
    if let Some(s) = &options.statistical {
        let before = kept.len();
        kept = statistical(points, &kept, s)?;
        out.statistical = before - kept.len();
    }
    if let Some(r) = &options.radius {
        let before = kept.len();
        let tree = KdTree::new(kept.iter().map(|&i| points[i]).collect());
        kept = kept
            .iter()
            .enumerate()
            .filter(|&(k, &i)| {
                // The point itself is within the radius too.
                let near = tree.within(points[i], r.radius);
                near.iter().filter(|&&j| j != k).count() >= r.neighbours
            })
            .map(|(_, &i)| i)
            .collect();
        out.radius = before - kept.len();
    }
    if kept.is_empty() {
        return Err("the filter leaves no point: every point was taken for an outlier".to_owned());
    }
    let mut moved: Vec<Vec3> = kept.iter().map(|&i| points[i]).collect();
    if let Some(s) = &options.smooth {
        if moved.len() < s.neighbours {
            return Err(format!(
                "smoothing the noise with {} neighbors needs as many points, {} are left",
                s.neighbours,
                moved.len()
            ));
        }
        let start = moved.clone();
        for _ in 0..s.passes {
            moved = smooth(&moved, s.neighbours);
        }
        for (a, b) in start.iter().zip(&moved) {
            let d = mitcad_vec3::norm(sub(*a, *b));
            if d > 1e-9 {
                out.smoothed += 1;
            }
            out.largest_move = out.largest_move.max(d);
        }
    }
    out.kept = kept;
    out.points = moved;
    Ok(out)
}

/// The kept points (indices into `points`) that are no statistical outliers.
fn statistical(points: &[Vec3], kept: &[usize], s: &Statistical) -> Result<Vec<usize>, String> {
    if kept.len() <= s.neighbours {
        return Err(format!(
            "the statistical outliers with {} neighbors need more points than that, {} are left",
            s.neighbours,
            kept.len()
        ));
    }
    let tree = KdTree::new(kept.iter().map(|&i| points[i]).collect());
    let means: Vec<f64> = kept
        .iter()
        .enumerate()
        .map(|(k, &i)| {
            let near = tree.nearest_k(points[i], s.neighbours + 1);
            let others: Vec<f64> = near
                .iter()
                .filter(|(j, _)| *j != k)
                .take(s.neighbours)
                .map(|(_, d)| *d)
                .collect();
            others.iter().sum::<f64>() / others.len().max(1) as f64
        })
        .collect();
    let n = means.len() as f64;
    let mean = means.iter().sum::<f64>() / n;
    let deviation = (means.iter().map(|m| (m - mean).powi(2)).sum::<f64>() / n).sqrt();
    let limit = mean + s.ratio * deviation;
    Ok(kept
        .iter()
        .zip(&means)
        .filter(|(_, m)| **m <= limit)
        .map(|(i, _)| *i)
        .collect())
}

/// Each point moved onto the plane fitted to its `k` nearest points.
fn smooth(points: &[Vec3], k: usize) -> Vec<Vec3> {
    let tree = KdTree::new(points.to_vec());
    points
        .iter()
        .map(|&p| {
            let near: Vec<Vec3> = tree
                .nearest_k(p, k)
                .into_iter()
                .map(|(j, _)| tree.point(j))
                .collect();
            let (centre, scatter) = centroid_and_scatter(&near);
            let (values, vectors) = symmetric_eigen(scatter);
            // A neighbourhood along a line has no plane.
            if values[1] <= 1e-12 * values[2].max(1e-300) {
                return p;
            }
            let normal = [vectors[0][0], vectors[1][0], vectors[2][0]];
            add_scaled(p, normal, -dot(sub(p, centre), normal))
        })
        .collect()
}

/// Whether `p` lies inside the polygon (even-odd rule).
pub(crate) fn inside(polygon: &[[f64; 2]], p: [f64; 2]) -> bool {
    let n = polygon.len();
    let mut odd = false;
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                odd = !odd;
            }
        }
    }
    odd
}
