// SPDX-License-Identifier: MIT
//! Sketches (timeline format study, section 10): points, lines, arcs and
//! circles, the sketch transform, the entity lists, geometric constraints
//! by type bit and dimensions with their parameter holders. Same choices
//! as the reference decoder.

use std::collections::{BTreeMap, HashMap};

use super::classes::{self, *};
use super::ir::{
    ConstraintF3d, DimensionF3d, ErrorRef, Geometry, IDENTITY, Mat4, ModelFrame, RefValue,
    Reference, SketchConstraint, SketchCounts, SketchCurve, SketchDetail, SketchDimension,
    SketchEntityRef, SketchPoint, Vec3,
};
use super::stream::{
    Object, Segment, f64_at, f64s_at, header_end, hex, i32_at, py_sum, slice, str16_at, u32_at,
    u64_at,
};
use super::unicode;

fn vec3(v: &[f64]) -> Vec3 {
    [v[0], v[1], v[2]]
}

/// A sketch point `C2CEDAE7` (section 10.2): after the root part,
/// `ref usage | u8[8] flags | f64 x, y, z`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointGeom {
    pub flags: [u8; 8],
    /// Sketch space, cm.
    pub xyz: Vec3,
}

/// A sketch line `DCA267ED` (and subclasses): `f64 start[3] | delta[3] |
/// dir[3] | (v2) normal[3] | ref end point | ref start point`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineGeom {
    pub start: Vec3,
    /// End minus start.
    pub delta: Vec3,
    /// Unit direction.
    pub dir: Vec3,
    pub normal: Option<Vec3>,
    pub end_point: Option<u64>,
    pub start_point: Option<u64>,
}

impl LineGeom {
    pub fn end(&self) -> Vec3 {
        [
            self.start[0] + self.delta[0],
            self.start[1] + self.delta[1],
            self.start[2] + self.delta[2],
        ]
    }
}

/// A sketch arc or circle `F0130424`: `f64 center[3] | normal[3] |
/// ref_dir[3] | radius | start_angle | end_angle | ref center point |
/// ref end point | ref start point` (a circle has the center point only).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArcGeom {
    pub center: Vec3,
    pub normal: Vec3,
    /// Zero direction of the angles (unit).
    pub ref_dir: Vec3,
    pub radius: f64,
    /// Radians, counter-clockwise about `normal` from `ref_dir`.
    pub start_angle: f64,
    pub end_angle: f64,
    pub center_point: Option<u64>,
    pub end_point: Option<u64>,
    pub start_point: Option<u64>,
}

impl ArcGeom {
    /// A full circle: no start or end point.
    pub fn is_circle(&self) -> bool {
        self.start_point.is_none() && self.end_point.is_none()
    }
}

/// The transform of a sketch (`F47A46FB`, section 10.4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SketchTransform {
    Identity,
    /// Row-major 4x4: sketch space to component space.
    Matrix([f64; 16]),
}

impl SketchTransform {
    pub fn mat4(&self) -> Mat4 {
        match self {
            SketchTransform::Identity => IDENTITY,
            SketchTransform::Matrix(m) => {
                let mut out = [[0.0; 4]; 4];
                for (r, row) in out.iter_mut().enumerate() {
                    for (c, x) in row.iter_mut().enumerate() {
                        *x = m[4 * r + c];
                    }
                }
                out
            }
        }
    }
}

/// Sketch points by object id.
pub fn points(seg: &Segment) -> HashMap<u64, PointGeom> {
    let mut out = HashMap::new();
    for o in seg.objects_of_kind(SKETCH_POINT) {
        let d = seg.data(o);
        let Some(root) = seg.root_part(d) else {
            continue;
        };
        // The corner points a text's box makes (`00D157C2`) keep the
        // point's members in the sub-chunk, right after its header.
        let sub = seg.sub_start(o);
        let at = if seg.guid_of(o.id) == Some(SKETCH_POINT_2) && sub > 0 {
            match d.get(sub..).and_then(header_end) {
                Some(h) => sub + h,
                None => continue,
            }
        } else {
            root.end
        };
        let Some(r) = seg.ref_at(d, at) else {
            continue;
        };
        let q = r.end;
        let (Some(f), Some(xyz)) = (d.get(q..q + 8), f64s_at(d, q + 8, 3)) else {
            continue;
        };
        let mut flags = [0u8; 8];
        flags.copy_from_slice(f);
        out.insert(
            o.id,
            PointGeom {
                flags,
                xyz: vec3(&xyz),
            },
        );
    }
    out
}

/// Sketch lines by object id. The layout follows the version of class
/// `DCA267ED` (v1: 9 doubles, v2: 12), also for its subclasses.
pub fn lines(seg: &Segment) -> HashMap<u64, LineGeom> {
    let mut out = HashMap::new();
    let n = match seg.class_by_guid(SKETCH_LINE) {
        Some(c) if c.version >= 2 => 12,
        _ => 9,
    };
    for o in seg.objects_of_kind(SKETCH_LINE) {
        let d = seg.data(o);
        let Some(root) = seg.root_part(d) else {
            continue;
        };
        // A subclass (`16DEFC4D`) keeps its own members after the root
        // part and the line's in its sub-chunk, right after the chunk's
        // header (mitcad#178: read from the main part, 158 lines of 20
        // sketches in the corpora came out as denormal numbers).
        let sub = seg.sub_start(o);
        let p = if seg.guid_of(o.id) != Some(SKETCH_LINE) && sub > 0 {
            match d.get(sub..).and_then(header_end) {
                Some(h) => sub + h,
                None => continue,
            }
        } else {
            // Another subclass (`AE42BAB6`) has a reference of its own
            // first (mitcad#178).
            let mut p = root.end;
            while let Some(r) = seg.ref_at(d, p)
                && !classes::is_point_class(seg.guid_of(r.id))
            {
                p = r.end;
            }
            p
        };
        let Some(v) = f64s_at(d, p, n) else { continue };
        let mut q = p + 8 * n;
        let mut refs = Vec::new();
        for _ in 0..2 {
            let Some(r) = seg.ref_at(d, q) else { break };
            refs.push(r.id);
            q = r.end;
        }
        out.insert(
            o.id,
            LineGeom {
                start: vec3(&v[0..3]),
                delta: vec3(&v[3..6]),
                dir: vec3(&v[6..9]),
                normal: (n == 12).then(|| vec3(&v[9..12])),
                end_point: refs.first().copied(),
                start_point: refs.get(1).copied(),
            },
        );
    }
    out
}

/// The flags of a sketch curve's sketch-curve part (class `9FF71F46`),
/// which every curve class ends with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CurveFlags {
    /// 0 normal, 1 construction, 2 centre line.
    pub kind: u8,
    /// The curve is fixed (not moved by the solver).
    pub fixed: bool,
    /// The curve is linked: a projection or inclusion of geometry outside
    /// the sketch (the API's `isLinked` and `isReference`, always equal).
    pub linked: bool,
    /// The objects the curve's reference list names (offset chains).
    pub refs: Vec<u64>,
}

/// A sketch curve's flags (mitcad#33; the API reference dumps). The
/// sketch-curve part ends with `01 | u32 n | n references | 13 bytes |
/// f32 1.0, f32 1.0` (the block): found from the last such pair, `n` being
/// the count that fits the references' room. The flag bytes lie before
/// the block's `01`, as many bytes back as the class's version says
/// *(read against the API reference dumps, 29 000 curves of lines, arcs,
/// circles, ellipses and splines; versions 7 to 9 occur)*:
///
/// | version | fixed | linked | kind |
/// |---------|-------|--------|------|
/// | 9       | 9     | 7      | 4    |
/// | 8       | 8     | 6      | 4    |
/// | 7       | 4     | 2      | 7    |
///
/// A flag above 1, a kind above 2 or another version: `None`.
pub fn curve_flags(seg: &Segment, d: &[u8]) -> Option<CurveFlags> {
    let version = seg.class_by_guid(SKETCH_CURVE).map_or(0, |c| c.version);
    curve_flags_of(d, version, &|d, at| seg.ref_at(d, at).map(|r| r.id))
}

fn curve_flags_of(
    d: &[u8],
    version: u32,
    ref_at: &dyn Fn(&[u8], usize) -> Option<u64>,
) -> Option<CurveFlags> {
    const PAIR: [u8; 8] = [0, 0, 0x80, 0x3f, 0, 0, 0x80, 0x3f];
    // Bytes back from the block's `01`: fixed, linked, kind.
    let (fixed, linked, kind) = match version {
        7 => (4, 2, 7),
        8 => (8, 6, 4),
        9.. => (9, 7, 4),
        _ => return None,
    };
    let p = d.windows(8).rposition(|w| w == PAIR)?;
    for n in 0..=16usize {
        let Some(b) = p.checked_sub(18 + 11 * n) else {
            break;
        };
        if d[b] != 1 || u32_at(d, b + 1) != Some(n as u32) {
            continue;
        }
        let refs: Option<Vec<u64>> = (0..n).map(|i| ref_at(d, b + 5 + 11 * i)).collect();
        let Some(refs) = refs else { continue };
        let byte = |back: usize| b.checked_sub(back).and_then(|i| d.get(i)).copied();
        let (f, l, k) = (byte(fixed)?, byte(linked)?, byte(kind)?);
        if f > 1 || l > 1 || k > 2 {
            return None;
        }
        return Some(CurveFlags {
            kind: k,
            fixed: f == 1,
            linked: l == 1,
            refs,
        });
    }
    None
}

/// Arcs and circles by object id.
pub fn arcs(seg: &Segment) -> HashMap<u64, ArcGeom> {
    let mut out = HashMap::new();
    for o in seg.objects_of(SKETCH_ARC) {
        let d = seg.data(o);
        let Some(root) = seg.root_part(d) else {
            continue;
        };
        let p = root.end;
        let Some(v) = f64s_at(d, p, 12) else { continue };
        let mut q = p + 96;
        let mut refs = Vec::new();
        for _ in 0..3 {
            match seg.ref_at(d, q) {
                Some(r) if classes::is_point_class(seg.guid_of(r.id)) => {
                    refs.push(r.id);
                    q = r.end;
                }
                _ => break,
            }
        }
        out.insert(
            o.id,
            ArcGeom {
                center: vec3(&v[0..3]),
                normal: vec3(&v[3..6]),
                ref_dir: vec3(&v[6..9]),
                radius: v[9],
                start_angle: v[10],
                end_angle: v[11],
                center_point: refs.first().copied(),
                end_point: refs.get(1).copied(),
                start_point: refs.get(2).copied(),
            },
        );
    }
    out
}

/// A sketch spline [`SKETCH_SPLINE`] (mitcad#154): a B-spline in sketch
/// space (cm) and the sketch points at its ends.
#[derive(Clone, Debug, PartialEq)]
pub struct SplineGeom {
    pub degree: u32,
    /// Non-decreasing, `control.len() + degree + 1` of them.
    pub knots: Vec<f64>,
    /// Empty for a non-rational curve, else one per control point.
    pub weights: Vec<f64>,
    pub control: Vec<Vec3>,
    pub start_point: Option<u64>,
    pub end_point: Option<u64>,
    /// The sketch points of the spline's fit-point list between its end
    /// points (`F81C561A`; empty where the list is).
    pub fit_points: Vec<u64>,
    /// The tangent handle lines of the spline's handle list (`4224E565`).
    pub handles: Vec<u64>,
}

impl SplineGeom {
    /// The spline was made through fit points: its fit-point list holds
    /// points or its handle list holds handles. Of the 453 splines of the
    /// API reference dumps, all 6 fitted splines are told by this, and no
    /// other spline has either list filled.
    pub fn has_fit_points(&self) -> bool {
        !self.fit_points.is_empty() || !self.handles.is_empty()
    }
    /// The knot vector is clamped at both ends (the first and last knots
    /// repeated `degree + 1` times): the curve starts at its first control
    /// point and ends at its last. An unclamped vector is taken for a
    /// periodic curve.
    pub fn is_clamped(&self) -> bool {
        let k = self.degree as usize + 1;
        let n = self.knots.len();
        n >= 2 * k
            && self.knots[..k].iter().all(|&x| x == self.knots[0])
            && self.knots[n - k..].iter().all(|&x| x == self.knots[n - 1])
    }
}

/// `u32 count | u32 capacity | u32 8 | count × width f64` at `p`: the
/// values and the end. The capacity is at least the count.
fn counted_f64s(d: &[u8], p: usize, width: usize) -> Option<(Vec<f64>, usize)> {
    let (n, end) = counted_len(d, p, width)?;
    let v = f64s_at(d, p + 12, n * width)?;
    v.iter().all(|x| x.is_finite()).then_some((v, end))
}

/// The count and the end of a [`counted_f64s`] array at `p`, from its
/// header alone (the values are not read).
fn counted_len(d: &[u8], p: usize, width: usize) -> Option<(usize, usize)> {
    let n = u32_at(d, p)? as usize;
    let cap = u32_at(d, p + 4)? as usize;
    if cap < n || u32_at(d, p + 8)? != 8 || n > 100_000 {
        return None;
    }
    let end = p + 12 + 8 * n * width;
    (end <= d.len()).then_some((n, end))
}

/// The NURBS data of a spline record: `u32 degree | f64 fit tolerance |
/// knots | weights | control points`, each array counted as
/// [`counted_f64s`] (control points three values each), found as the first
/// place after `from` where the counts agree (knots = control points +
/// degree + 1, weights none or one per control point) and the knots do not
/// decrease. Layout as the cadmpeg format description (CC-BY-4.0) gives
/// the sketch NURBS payload; checked against the reference models' dumps.
/// (Where it starts, degree, knots, weights, control points.)
#[allow(clippy::type_complexity)]
fn nurbs_at(d: &[u8], from: usize) -> Option<(usize, u32, Vec<f64>, Vec<f64>, Vec<Vec3>)> {
    // The degree and the three counts at `p`, from the headers alone: the
    // number of values a payload there has, if they agree. Reading the
    // arrays at every offset re-read up to 300 000 values per byte of a
    // crafted record (mitcad#934).
    let counts = |p: usize| -> Option<usize> {
        let degree = u32_at(d, p)? as usize;
        if !(1..=25).contains(&degree) || !f64_at(d, p + 4)?.is_finite() {
            return None;
        }
        let (knots, q) = counted_len(d, p + 12, 1)?;
        let (weights, q) = counted_len(d, q, 1)?;
        let (poles, _) = counted_len(d, q, 3)?;
        (poles >= 2 && knots == poles + degree + 1 && (weights == 0 || weights == poles))
            .then_some(knots + weights + 3 * poles)
    };
    // The values read for such candidates, at most as many as the data
    // has bytes (eight times what one payload can hold): in a real record
    // the payload is the first offset whose headers agree, so the bound
    // only stops crafted data.
    let mut budget = d.len();
    let mut p = from;
    while p + 12 + 36 <= d.len() {
        match counts(p) {
            None => {
                p += 1;
                continue;
            }
            Some(n) if n > budget => return None,
            Some(n) => budget -= n,
        }
        let found = (|| {
            let degree = u32_at(d, p)?;
            if !(1..=25).contains(&degree) || !f64_at(d, p + 4)?.is_finite() {
                return None;
            }
            let (knots, q) = counted_f64s(d, p + 12, 1)?;
            let (weights, q) = counted_f64s(d, q, 1)?;
            let (poles, _) = counted_f64s(d, q, 3)?;
            let n = poles.len() / 3;
            let ok = n >= 2
                && knots.len() == n + degree as usize + 1
                && (weights.is_empty() || (weights.len() == n && weights.iter().all(|&w| w > 0.0)))
                && knots.windows(2).all(|w| w[0] <= w[1])
                && knots[0] < knots[knots.len() - 1];
            ok.then(|| (degree, knots, weights, poles.chunks(3).map(vec3).collect()))
        })();
        if let Some((degree, knots, weights, control)) = found {
            return Some((p, degree, knots, weights, control));
        }
        p += 1;
    }
    None
}

/// Sketch splines by object id (mitcad#154). The end points are the
/// sketch points the record refers to before its NURBS data, matched to
/// the curve's first and last control points (a clamped curve passes
/// through them) within 1e-6 cm.
pub fn splines(seg: &Segment, pts: &HashMap<u64, PointGeom>) -> HashMap<u64, SplineGeom> {
    let mut out = HashMap::new();
    for o in seg.objects_of_kind(SKETCH_SPLINE) {
        let d = seg.data(o);
        let Some(root) = seg.root_part(d) else {
            continue;
        };
        let Some((at, degree, knots, weights, control)) = nurbs_at(d, root.end) else {
            continue;
        };
        let mut s = SplineGeom {
            degree,
            knots,
            weights,
            control,
            start_point: None,
            end_point: None,
            fit_points: Vec::new(),
            handles: Vec::new(),
        };
        // The spline's own lists, of fit points and of handles (the first of
        // each kind that refers back to the spline).
        for (_, r) in seg.refs_in(d, root.end, d.len()) {
            let fit = match seg.guid_of(r.id) {
                Some(SPLINE_FIT_POINTS) if s.fit_points.is_empty() => true,
                Some(SPLINE_HANDLES) if s.handles.is_empty() => false,
                _ => continue,
            };
            let ld = seg.data_of(r.id);
            let Some(start) = header_end(ld) else {
                continue;
            };
            let items = seg.refs_in(ld, start, ld.len());
            // The list ends with the reference back to the spline.
            if items.last().map(|(_, b)| b.id) != Some(o.id) {
                continue;
            }
            let ids = items.iter().map(|(_, b)| b.id).filter(|&id| {
                let class = seg.guid_of(id);
                if fit {
                    classes::is_point_class(class)
                } else {
                    classes::is_line_class(class)
                }
            });
            if fit {
                s.fit_points.extend(ids);
            } else {
                s.handles.extend(ids);
            }
        }
        if s.is_clamped() {
            let ends: Vec<u64> = seg
                .refs_in(d, root.end, at)
                .into_iter()
                .map(|(_, r)| r.id)
                .filter(|&id| classes::is_point_class(seg.guid_of(id)))
                .collect();
            let near = |id: u64, to: Vec3| {
                pts.get(&id)
                    .is_some_and(|p| (0..3).all(|i| (p.xyz[i] - to[i]).abs() < 1e-6))
            };
            let first = s.control[0];
            let last = s.control[s.control.len() - 1];
            s.start_point = ends.iter().copied().find(|&id| near(id, first));
            s.end_point = ends
                .iter()
                .copied()
                .find(|&id| near(id, last) && Some(id) != s.start_point);
        }
        out.insert(o.id, s);
    }
    out
}

/// The API type of a sketch spline (mitcad#186): a spline with a fit-point
/// or handle list is a fitted spline; one an offset made (`offset_child`),
/// or a fixed one (the curve flag), a control-point spline; any other a
/// fixed spline. Read against the API reference dumps: all 453 splines.
pub fn spline_kind(s: &SplineGeom, fixed: bool, offset_child: bool) -> &'static str {
    if s.has_fit_points() {
        "SketchFittedSpline"
    } else if fixed || offset_child {
        "SketchControlPointSpline"
    } else {
        "SketchFixedSpline"
    }
}

/// A sketch ellipse or elliptical arc [`SKETCH_ELLIPSE`] (mitcad#154):
/// after the root part `f64 center[3] | major[3] | minor[3] | major
/// radius | minor radius | start angle | end angle`, then `ref center
/// point | ref end point | ref | ref | ref start point`, each reference or
/// a null byte (a full ellipse has no end points; the two in between name
/// its axis lines). `major` and `minor` are unit directions; a point at
/// angle `t` is `center + a cos t major + b sin t minor`, so the arc runs
/// counter-clockwise about `major × minor`. Read on the reference models
/// against their dumps (the format descriptions we have do not give this
/// record).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EllipseGeom {
    pub center: Vec3,
    pub major: Vec3,
    pub minor: Vec3,
    pub major_radius: f64,
    pub minor_radius: f64,
    pub start_angle: f64,
    pub end_angle: f64,
    pub center_point: Option<u64>,
    pub end_point: Option<u64>,
    pub start_point: Option<u64>,
}

impl EllipseGeom {
    /// The normal the angles turn about: `major × minor`.
    pub fn normal(&self) -> Vec3 {
        mitcad_vec3::cross(self.major, self.minor)
    }

    /// A whole ellipse: no end points.
    pub fn is_closed(&self) -> bool {
        self.start_point.is_none() && self.end_point.is_none()
    }
}

/// Sketch ellipses and elliptical arcs by object id (mitcad#154).
pub fn ellipses(seg: &Segment) -> HashMap<u64, EllipseGeom> {
    let mut out = HashMap::new();
    for o in seg.objects_of_kind(SKETCH_ELLIPSE) {
        let d = seg.data(o);
        let Some(root) = seg.root_part(d) else {
            continue;
        };
        let p = root.end;
        let Some(v) = f64s_at(d, p, 13) else { continue };
        let (major, minor) = (vec3(&v[3..6]), vec3(&v[6..9]));
        use mitcad_vec3::dot;
        let valid = v.iter().all(|x| x.is_finite())
            && (dot(major, major) - 1.0).abs() < 1e-9
            && (dot(minor, minor) - 1.0).abs() < 1e-9
            && dot(major, minor).abs() < 1e-9
            && v[9] > 0.0
            && v[10] > 0.0;
        if !valid {
            continue;
        }
        // Five slots, each a reference or a null byte.
        let mut q = p + 104;
        let mut slots = Vec::new();
        for _ in 0..5 {
            if d.get(q) == Some(&0) {
                slots.push(None);
                q += 1;
            } else if let Some(r) = seg.ref_at(d, q) {
                slots.push(Some(r.id));
                q = r.end;
            } else {
                break;
            }
        }
        if slots.len() < 5 {
            continue;
        }
        let point = |i: usize| {
            slots
                .get(i)
                .copied()
                .flatten()
                .filter(|&id| classes::is_point_class(seg.guid_of(id)))
        };
        let Some(center_point) = point(0) else {
            continue;
        };
        out.insert(
            o.id,
            EllipseGeom {
                center: vec3(&v[0..3]),
                major,
                minor,
                major_radius: v[9],
                minor_radius: v[10],
                start_angle: v[11],
                end_angle: v[12],
                center_point: Some(center_point),
                end_point: point(1),
                start_point: point(4),
            },
        );
    }
    out
}

/// Sketch object id -> transform, from the first `F47A46FB` object the
/// sketch references: after its root part (`u8 0, u8 0`), `u8 identity`
/// and, when it is 0, `f64 m[16]`.
pub fn transforms(seg: &Segment) -> HashMap<u64, SketchTransform> {
    let mut out = HashMap::new();
    for s in seg.objects_of(SKETCH) {
        let sd = seg.data(s);
        let Some((_, r)) = seg
            .refs_in(sd, 0, sd.len())
            .into_iter()
            .find(|(_, r)| seg.guid_of(r.id) == Some(SKETCH_TRANSFORM))
        else {
            continue;
        };
        let d = seg.data_of(r.id);
        let Some(p) = header_end(d) else { continue };
        match d.get(p + 2) {
            Some(1) => {
                out.insert(s.id, SketchTransform::Identity);
            }
            Some(0) if p + 3 + 128 <= d.len() => {
                if let Some(v) = f64s_at(d, p + 3, 16) {
                    let mut m = [0.0; 16];
                    m.copy_from_slice(&v);
                    out.insert(s.id, SketchTransform::Matrix(m));
                }
            }
            _ => {}
        }
    }
    out
}

/// The entity list of a sketch: the longest valid `u32 n, ref[n]` list
/// in its sub-chunk (points, curves, texts and dimensions in creation
/// order).
pub fn entity_list(seg: &Segment, s: &Object) -> Vec<u64> {
    let d = seg.data(s);
    let mut runs = RefRuns::new(seg, d, |_| true);
    // Where the longest list's references start, and how many.
    let mut best = (0, 0);
    let mut q = seg.sub_start(s);
    while q + 4 < d.len() {
        let n = u32_at(d, q).unwrap_or(0) as usize;
        if (1..=MAX_LIST).contains(&n) && n > best.1 && runs.run(q + 4) >= n {
            best = (q + 4, n);
        }
        q += 1;
    }
    refs_from(seg, d, best.0, best.1)
}

/// The longest reference list the sketch scans accept.
const MAX_LIST: usize = 100_000;

/// The runs of consecutive references in an object's data whose ids `keep`
/// accepts: how many start at each offset, each offset read at most once.
/// The sketch scans try a `u32 n, ref[n]` list at every byte; reading each
/// candidate's references afresh cost an object of back-to-back references
/// one walk per byte, about 10^10 reads for 11 MB (mitcad#934).
struct RefRuns<'a, F: Fn(u64) -> bool> {
    seg: &'a Segment,
    d: &'a [u8],
    keep: F,
    /// Run length by offset (at most `MAX_LIST + 1`), `u32::MAX` while
    /// not read; allocated on first use.
    len: Vec<u32>,
}

impl<'a, F: Fn(u64) -> bool> RefRuns<'a, F> {
    fn new(seg: &'a Segment, d: &'a [u8], keep: F) -> Self {
        RefRuns {
            seg,
            d,
            keep,
            len: Vec::new(),
        }
    }

    /// How many consecutive kept references start at `p` (counted up to
    /// `MAX_LIST + 1`).
    fn run(&mut self, p: usize) -> usize {
        if p > self.d.len() {
            return 0;
        }
        if self.len.is_empty() {
            self.len = vec![u32::MAX; self.d.len() + 1];
        }
        // Follow the references to an offset already counted (or the end
        // of the run), then count back along the way.
        let mut path = Vec::new();
        let mut q = p;
        let mut n = loop {
            let known = self.len[q];
            if known != u32::MAX {
                break known;
            }
            match self.seg.ref_at(self.d, q).filter(|r| (self.keep)(r.id)) {
                // A reference ends after its start and within the data.
                Some(r) if r.end > q && r.end <= self.d.len() => {
                    path.push(q);
                    q = r.end;
                }
                _ => {
                    self.len[q] = 0;
                    break 0;
                }
            }
        };
        for &x in path.iter().rev() {
            n = (n + 1).min(MAX_LIST as u32 + 1);
            self.len[x] = n;
        }
        self.len[p] as usize
    }
}

/// A light bulb as the file stores it: on or off, and its raw bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LightBulb {
    pub on: bool,
    pub raw: Vec<u8>,
}

/// A sketch's light bulb (mitcad#6). In the sketch object, the reference
/// to the [`SKETCH_NAME_ANCHOR`] object that the sketch's name and its
/// entity list follow (`ref | str16 name | u8 0 | u32 n | ref[n]`) comes
/// after `u32 k | u8 0 | u8 light bulb` (1 on, 0 off; `k` is open). It
/// equals the reference dumps' `isVisible` for every sketch of the reference
/// models (`tests/design_models.rs`). Files whose sketch class is version
/// 16 or older, and some of 17, lay this part out otherwise: None, as
/// when the place is not found exactly once.
pub fn light_bulb(seg: &Segment, s: &Object) -> Option<LightBulb> {
    let d = seg.data(s);
    let start = header_end(d)?;
    let mut found = None;
    for (p, r) in seg.refs_in(d, start, d.len()) {
        if seg.guid_of(r.id) != Some(SKETCH_NAME_ANCHOR) {
            continue;
        }
        let Some((_, e)) = str16_at(d, r.end) else {
            continue;
        };
        let n = u32_at(d, e + 1).unwrap_or(u32::MAX) as usize;
        if d.get(e) != Some(&0) || n > 100_000 || refs_from(seg, d, e + 5, n).len() != n {
            continue;
        }
        if p < 6 || d[p - 2] != 0 || d[p - 1] > 1 || found.is_some() {
            return None;
        }
        found = Some(LightBulb {
            on: d[p - 1] == 1,
            raw: d[p - 6..p].to_vec(),
        });
    }
    found
}

/// The name a user gave a sketch: the `str16` after the reference to the
/// [`SKETCH_NAME_ANCHOR`] object (`ref | str16 name | u8 0 | u32 n | ref[n]`,
/// see [`light_bulb`]). It is empty while the sketch has its default name
/// (`Sketch<n>`, which the item's tail carries); the item's tail does not
/// hold a name the user gave a sketch (a custom name there is one the sketch
/// no longer has). None when the place is not found exactly once (sketch
/// classes older than version 17 lay it out otherwise).
pub fn sketch_name(seg: &Segment, s: &Object) -> Option<String> {
    let d = seg.data(s);
    let start = header_end(d)?;
    let mut found = None;
    for (_, r) in seg.refs_in(d, start, d.len()) {
        if seg.guid_of(r.id) != Some(SKETCH_NAME_ANCHOR) {
            continue;
        }
        let Some((name, e)) = str16_at(d, r.end) else {
            continue;
        };
        let n = u32_at(d, e + 1).unwrap_or(u32::MAX) as usize;
        if d.get(e) != Some(&0) || n > 100_000 || refs_from(seg, d, e + 5, n).len() != n {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some(name);
    }
    found.filter(|n| unicode::is_printable(n))
}

/// Up to `n` consecutive references from `p` (stops at the first invalid).
fn refs_from(seg: &Segment, d: &[u8], mut p: usize, n: usize) -> Vec<u64> {
    let mut out = Vec::new();
    while out.len() < n {
        let Some(r) = seg.ref_at(d, p) else { break };
        out.push(r.id);
        p = r.end;
    }
    out
}

/// The constraint and dimension lists of the sketch's main part: the
/// longest valid lists whose items are all constraints (`60403D47`) or
/// all dimensions (`855F0A64`).
pub fn main_lists(seg: &Segment, s: &Object) -> (Vec<u64>, Vec<u64>) {
    let d = seg.data(s);
    let end = seg.main_end(s).min(d.len());
    let Some(start) = header_end(d) else {
        return (Vec::new(), Vec::new());
    };
    // Runs of constraints and of dimensions, each offset read once
    // ([`RefRuns`], mitcad#934); the lists' starts and lengths.
    let mut cons_runs = RefRuns::new(seg, d, |x| seg.is_kind_of(x, SKETCH_CONSTRAINT));
    let mut dims_runs = RefRuns::new(seg, d, |x| seg.is_kind_of(x, SKETCH_DIMENSION));
    let (mut cons, mut dims) = ((0, 0), (0, 0));
    let mut q = start;
    while q + 4 < end {
        let n = u32_at(d, q).unwrap_or(0) as usize;
        if (1..=MAX_LIST).contains(&n) {
            if n > cons.1 && cons_runs.run(q + 4) >= n {
                cons = (q + 4, n);
            } else if n > dims.1 && dims_runs.run(q + 4) >= n {
                dims = (q + 4, n);
            }
        }
        q += 1;
    }
    (
        refs_from(seg, d, cons.0, cons.1),
        refs_from(seg, d, dims.0, dims.1),
    )
}

/// A geometric constraint `60403D47` (section 10.3): root part with
/// `(ref entity, u32 role)` items, then `ref sketch | u64 mask | u32 n |
/// ref entities[n]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstraintRaw {
    pub sketch: u64,
    /// One bit per constraint type.
    pub mask: u64,
    pub entities: Vec<u64>,
    /// The root list: (entity, role).
    pub roles: Vec<(u64, u32)>,
    /// The subclass part between the root part's attributes and the
    /// sketch reference (pattern constraints keep their values there).
    pub tail: Option<(usize, usize)>,
}

pub fn parse_constraint(seg: &Segment, o: &Object) -> Option<ConstraintRaw> {
    let d = seg.data(o);
    let mut p = header_end(d)?;
    let mut roles = Vec::new();
    if *d.get(p)? == 1 {
        let n = u32_at(d, p + 1)?;
        let mut q = p + 5;
        for _ in 0..n {
            let r = seg.ref_at(d, q)?;
            roles.push((r.id, u32_at(d, r.end)?));
            q = r.end + 4;
        }
        p = q;
    } else {
        p += 1;
    }
    let tail_start = attributes_end(d, p);
    // (A pattern constraint of newer writers keeps its values before the
    // sketch: over 200 bytes.)
    for q in p..d.len().min(p + 1000) {
        let Some(r) = seg.ref_at(d, q) else { continue };
        if seg.guid_of(r.id) != Some(SKETCH) {
            continue;
        }
        let mask = u64_at(d, r.end)?;
        let n = u32_at(d, r.end + 8)?;
        let mut q3 = r.end + 12;
        let mut entities = Vec::new();
        for _ in 0..n {
            let rr = seg.ref_at(d, q3)?;
            entities.push(rr.id);
            q3 = rr.end;
        }
        return Some(ConstraintRaw {
            sketch: r.id,
            mask,
            entities,
            roles,
            tail: tail_start.filter(|s| *s <= q).map(|s| (s, q)),
        });
    }
    None
}

/// Where the root part's attributes (`u8 has_attrs [u32 n, n ×
/// attribute]`) starting at `p` end.
fn attributes_end(d: &[u8], p: usize) -> Option<usize> {
    match *d.get(p)? {
        0 => Some(p + 1),
        1 => {
            let n = u32_at(d, p + 1)?;
            let mut q = p + 5;
            for _ in 0..n.min(1000) {
                q = super::stream::attribute_at(d, q)?.1;
            }
            Some(q)
        }
        _ => None,
    }
}

/// A circular pattern constraint's part (class `8269E861`, mitcad#36):
/// `ref angle holder | ref quantity holder | f64 angle | u32 quantity`,
/// then bytes not decoded (zero in every file seen).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CircularPatternRaw {
    /// Total angle, radians.
    pub angle: f64,
    pub angle_holder: u64,
    pub quantity: u32,
    pub quantity_holder: u64,
}

pub fn circular_pattern(
    seg: &Segment,
    d: &[u8],
    tail: (usize, usize),
) -> Option<CircularPatternRaw> {
    let a = seg.ref_at(d, tail.0)?;
    let q = seg.ref_at(d, a.end)?;
    if seg.guid_of(a.id) != Some(PARAMETER_HOLDER) || seg.guid_of(q.id) != Some(INT_HOLDER) {
        return None;
    }
    Some(CircularPatternRaw {
        angle: f64_at(d, q.end)?,
        angle_holder: a.id,
        quantity: u32_at(d, q.end + 8)?,
        quantity_holder: q.id,
    })
}

/// One direction of a rectangular pattern: `u32 quantity | ref quantity
/// holder | f64 direction[3] | f64 distance | ref distance holder`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatternDirectionRaw {
    pub quantity: u32,
    pub quantity_holder: u64,
    /// Unit vector, sketch space.
    pub direction: Vec3,
    /// Signed, along `direction`, cm.
    pub distance: f64,
    pub distance_holder: u64,
}

/// A rectangular pattern constraint's part (class `40800FB9`, mitcad#36):
/// `u8 flags[3] | u32 n, ref[n]` (the first direction's sketch line, if
/// any), eight bytes, then the two directions ([`PatternDirectionRaw`]).
/// The second direction is the first turned a quarter counter-clockwise
/// in every file seen. `flags[1]` is 1 where the instances lie on both
/// sides of the originals along the first direction *(1 of 34 patterns
/// seen; the other flags are open)*.
#[derive(Clone, Debug, PartialEq)]
pub struct RectangularPatternRaw {
    pub flags: [u8; 3],
    pub direction_entity: Option<u64>,
    pub directions: [PatternDirectionRaw; 2],
}

pub fn rectangular_pattern(
    seg: &Segment,
    d: &[u8],
    tail: (usize, usize),
) -> Option<RectangularPatternRaw> {
    let (start, end) = tail;
    let refs = seg.refs_in(d, start + 3, end);
    let first = refs
        .iter()
        .position(|(_, r)| seg.guid_of(r.id) == Some(INT_HOLDER))?;
    let direction_entity = refs[..first]
        .iter()
        .map(|(_, r)| r.id)
        .find(|&id| seg.is_kind_of(id, SKETCH_CURVE));
    let direction = |at: usize| -> Option<(PatternDirectionRaw, usize)> {
        let q = seg.ref_at(d, at)?;
        if seg.guid_of(q.id) != Some(INT_HOLDER) {
            return None;
        }
        let v = f64s_at(d, q.end, 4)?;
        let h = seg.ref_at(d, q.end + 32)?;
        if seg.guid_of(h.id) != Some(PARAMETER_HOLDER) {
            return None;
        }
        Some((
            PatternDirectionRaw {
                quantity: u32_at(d, at.checked_sub(4)?)?,
                quantity_holder: q.id,
                direction: vec3(&v[0..3]),
                distance: v[3],
                distance_holder: h.id,
            },
            h.end,
        ))
    };
    let (one, after) = direction(refs[first].0)?;
    let (two, _) = direction(after + 4)?;
    let mut flags = [0u8; 3];
    flags.copy_from_slice(d.get(start..start + 3)?);
    Some(RectangularPatternRaw {
        flags,
        direction_entity,
        directions: [one, two],
    })
}

/// The object of an offset (class `AFC07C10`, mitcad#36) that holds an
/// offset constraint's curves: its root list refers to the curve offset,
/// the parent and the child chains (class `04E598FF`, each a root list
/// of curves in chain order), the offset dimension and the constraint;
/// the object ends with the signed distance (f64, cm).
#[derive(Clone, Debug, PartialEq)]
pub struct OffsetRaw {
    pub parents: Vec<u64>,
    pub children: Vec<u64>,
    pub dimension: Option<u64>,
    /// Signed, cm; its sign is the side.
    pub distance: Option<f64>,
}

/// Offset constraint id -> what its offset object holds.
pub fn offsets(seg: &Segment) -> HashMap<u64, OffsetRaw> {
    let mut out = HashMap::new();
    for o in seg.objects_of(OFFSET) {
        let d = seg.data(o);
        let Some(start) = header_end(d) else { continue };
        let refs: Vec<u64> = seg
            .refs_in(d, start, d.len())
            .into_iter()
            .map(|(_, r)| r.id)
            .collect();
        let Some(constraint) = refs
            .iter()
            .copied()
            .find(|&r| seg.is_kind_of(r, SKETCH_CONSTRAINT))
        else {
            continue;
        };
        let mut chains = Vec::new();
        for &r in &refs {
            if seg.guid_of(r) == Some(OFFSET_CHAIN) && !chains.contains(&r) {
                chains.push(r);
            }
        }
        let chain = |id: Option<&u64>| -> Vec<u64> {
            let Some(&id) = id else { return Vec::new() };
            let cd = seg.data_of(id);
            let Some(s) = header_end(cd) else {
                return Vec::new();
            };
            seg.refs_in(cd, s, cd.len())
                .into_iter()
                .map(|(_, r)| r.id)
                .take_while(|&r| seg.is_kind_of(r, SKETCH_CURVE))
                .collect()
        };
        let distance = d
            .len()
            .checked_sub(8)
            .and_then(|p| f64_at(d, p))
            .filter(|v| v.is_finite() && *v != 0.0);
        out.insert(
            constraint,
            OffsetRaw {
                parents: chain(chains.first()),
                children: chain(chains.get(1)),
                dimension: refs
                    .iter()
                    .copied()
                    .find(|&r| seg.is_kind_of(r, SKETCH_DIMENSION)),
                distance,
            },
        );
    }
    out
}

/// A sketch dimension (classes under `855F0A64`, section 10.5): its tail
/// is `str16 expression | f64 text[3] | u8 flags[k] | ref holder | u32 n |
/// ref entities[n]`, anchored at the last holder reference. A driven
/// dimension (one that measures, without a parameter) has a null
/// reference, a single zero byte, where the holder would be (mitcad#178).
#[derive(Clone, Debug, PartialEq)]
pub struct DimensionRaw {
    /// Parameter holder (`D91D429C`) of the dimension's parameter; `None`
    /// for a driven dimension.
    pub holder: Option<u64>,
    pub entities: Vec<u64>,
    /// Copy of the parameter expression (of a driven dimension: the value
    /// it shows).
    pub expression: Option<String>,
    /// Text position, sketch space.
    pub text: Option<Vec3>,
    pub flags: Option<Vec<u8>>,
}

/// Where a driven dimension's entity list starts: `u8 0 | u32 n | ref
/// entities[n]` after the root part, its entities sketch points or curves;
/// the last such list. Returns the position of the null reference and of
/// the count.
fn driven_entities_at(seg: &Segment, d: &[u8]) -> Option<(usize, usize)> {
    let after = seg.root_part(d).map_or(0, |r| r.end);
    let is_entity =
        |id: u64| classes::is_point_class(seg.guid_of(id)) || seg.is_kind_of(id, SKETCH_CURVE);
    seg.refs_in(d, after, d.len())
        .into_iter()
        .rev()
        .find_map(|(p, r)| {
            let q = p.checked_sub(5)?;
            if q < after || d[q] != 0 || !is_entity(r.id) {
                return None;
            }
            let n = u32_at(d, q + 1)?;
            if !(1..=4).contains(&n) {
                return None;
            }
            let mut at = p;
            for _ in 0..n {
                let e = seg.ref_at(d, at)?;
                if !is_entity(e.id) {
                    return None;
                }
                at = e.end;
            }
            Some((q, q + 1))
        })
}

pub fn parse_dimension(seg: &Segment, o: &Object) -> Option<DimensionRaw> {
    let d = seg.data(o);
    let holder = seg
        .refs_in(d, 0, d.len())
        .into_iter()
        .rev()
        .find(|(_, r)| seg.guid_of(r.id) == Some(PARAMETER_HOLDER));
    let (hp, holder, count_at) = match holder {
        Some((hp, hr)) => (hp, Some(hr.id), hr.end),
        None => {
            let (hp, count_at) = driven_entities_at(seg, d)?;
            (hp, None, count_at)
        }
    };
    let n = u32_at(d, count_at)?;
    let mut q = count_at + 4;
    let mut entities = Vec::new();
    for _ in 0..n {
        let r = seg.ref_at(d, q)?;
        entities.push(r.id);
        q = r.end;
    }
    let mut found = None;
    let lo = (hp as isize - 400).max(0);
    let mut b = hp as isize - 4;
    while b > lo {
        if let Some((s, e)) = str16_at(d, b as usize)
            && !s.is_empty()
            && unicode::is_printable(&s)
            && e + 24 <= hp
            // A driven dimension's string is the value it shows.
            && (holder.is_some() || s.bytes().any(|c| c.is_ascii_digit()))
        {
            found = Some((s, e));
            break;
        }
        b -= 1;
    }
    let (expression, text, flags) = match found {
        Some((s, e)) => (
            Some(s),
            f64s_at(d, e, 3).map(|v| vec3(&v)),
            Some(slice(d, e + 24, hp).to_vec()),
        ),
        None => (None, None, None),
    };
    Some(DimensionRaw {
        holder,
        entities,
        expression,
        text,
        flags,
    })
}

/// A sketch text `E0618268` (mitcad#186). After the root part's
/// properties: `u8 flag | f64 width factor | f32 rgba[4] | str16 font | u8 0
/// | f64 height (cm) | [ref height holder] | u32 horizontal alignment (1
/// left, 2 right, 3 centre) | u8 flags[3] | str16 text | [ref content
/// holder] | u32 vertical alignment (1 top, 2 bottom, 3 middle) | u8 flag
/// | i32 font weight | u32 kind (0 text in a box, 1 text on a path) | tail
/// | ref sketch`. The two optional references (and their null bytes) are
/// framed by requiring the record to end on the sketch reference. The
/// tail of a text in a box is, in some files, a flag byte and the text's
/// 4 x 4 placement matrix (row-major, sketch space in cm: the corner of
/// its box and the direction of its baseline) ahead of fixed bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRaw {
    pub text: String,
    pub font: String,
    /// Height of the capital letters, cm.
    pub height: f64,
    pub width_factor: f64,
    pub horizontal: u32,
    pub vertical: u32,
    pub flags: [u8; 3],
    pub vertical_flag: u8,
    pub weight: i32,
    /// 0: in a box; 1: along a path.
    pub kind: u32,
    /// Row-major placement of a text in a box, when the file stores it.
    pub matrix: Option<[f64; 16]>,
    /// The parameter holders that drive the height and the content.
    pub height_holder: Option<u64>,
    pub content_holder: Option<u64>,
}

/// The bytes a text's tail has after its matrix: a `u32`, a null
/// reference, two bytes, two `f32` and five bytes.
const TEXT_MATRIX_TAIL: usize = 1 + 128 + 4 + 11 + 2 + 8 + 5;

pub fn parse_text(seg: &Segment, o: &Object) -> Option<TextRaw> {
    let d = seg.data(o);
    let root = seg.root_part(d)?;
    // The record ends on the reference to its sketch.
    let end = d.len().checked_sub(11)?;
    let sketch = seg.ref_at(d, end)?;
    if seg.guid_of(sketch.id) != Some(SKETCH) {
        return None;
    }
    let p = root.end + 1;
    let width_factor = f64_at(d, p)?;
    let (font, q) = str16_at(d, p + 8 + 16)?;
    let height = f64_at(d, q + 1)?;
    if !(height.is_finite() && height > 0.0) {
        return None;
    }
    let after_height = q + 9;
    // Each optional slot: nothing, a null byte or a reference.
    let slot = |at: usize| -> Vec<(usize, Option<u64>)> {
        let mut options = Vec::new();
        if let Some(r) = seg.ref_at(d, at)
            && seg.guid_of(r.id).is_some()
        {
            options.push((r.end, Some(r.id)));
        }
        options.push((at, None));
        if d.get(at) == Some(&0) {
            options.push((at + 1, None));
        }
        options
    };
    for (a_end, height_holder) in slot(after_height) {
        let Some(horizontal) = u32_at(d, a_end).filter(|h| (1..=3).contains(h)) else {
            continue;
        };
        let flags = [*d.get(a_end + 4)?, *d.get(a_end + 5)?, *d.get(a_end + 6)?];
        let Some((text, t_end)) = str16_at(d, a_end + 7).filter(|(s, _)| !s.is_empty()) else {
            continue;
        };
        for (b_end, content_holder) in slot(t_end) {
            let Some(vertical) = u32_at(d, b_end).filter(|v| (1..=3).contains(v)) else {
                continue;
            };
            let Some(vertical_flag) = d.get(b_end + 4).copied() else {
                continue;
            };
            let (Some(weight), Some(kind)) = (i32_at(d, b_end + 5), u32_at(d, b_end + 9)) else {
                continue;
            };
            if !(1..=1000).contains(&weight) || kind > 1 {
                continue;
            }
            let tail = b_end + 13;
            if tail > end {
                continue;
            }
            let matrix = (kind == 0 && end - tail >= TEXT_MATRIX_TAIL)
                .then(|| f64s_at(d, tail + 1, 16))
                .flatten()
                .and_then(|m| <[f64; 16]>::try_from(m).ok())
                .filter(|m| m.iter().all(|x| x.is_finite()) && m[12..16] == [0.0, 0.0, 0.0, 1.0]);
            return Some(TextRaw {
                text,
                font,
                height,
                width_factor,
                horizontal,
                vertical,
                flags,
                vertical_flag,
                weight,
                kind,
                matrix,
                height_holder,
                content_holder,
            });
        }
    }
    None
}

/// A text as SCHEMA.md `Text` (cm). The API gives no position and angle
/// for a text in a box or on a path; they are written when the file stores
/// the text's placement. The box's lines and the path's curve come from the
/// text's own constraint (`TextBoxConstraint`, `TextPathConstraint`).
pub fn text_value(
    seg: &Segment,
    text: u64,
    id: &str,
    raw: &TextRaw,
    constraints: &[u64],
    curves: &IdTable,
) -> serde_json::Value {
    use serde_json::json;
    let alignment =
        |v: u32, names: [&'static str; 3]| names.get(v.wrapping_sub(1) as usize).copied();
    let horizontal = alignment(
        raw.horizontal,
        [
            "LeftHorizontalAlignment",
            "RightHorizontalAlignment",
            "CenterHorizontalAlignment",
        ],
    );
    let vertical = alignment(
        raw.vertical,
        [
            "TopVerticalAlignment",
            "BottomVerticalAlignment",
            "MiddleVerticalAlignment",
        ],
    );
    // The constraint that ties the text to its box or path.
    let tie = constraints.iter().find_map(|&c| {
        let pc = parse_constraint(seg, seg.object(c)?)?;
        let ours = pc.roles.iter().any(|(e, _)| *e == text);
        (ours && matches!(pc.mask, 0x100_0000_0000 | 0x200_0000_0000)).then_some(pc)
    });
    let entity = |e: u64| {
        curves
            .get(e)
            .map(|c| json!({"kind": "sketch_entity", "id": c}))
    };
    let mut definition = serde_json::Map::new();
    match (&tie, raw.kind) {
        (Some(pc), 1) if pc.mask == 0x200_0000_0000 => {
            definition.insert("_type".into(), json!("AlongPathTextDefinition"));
            if let Some(path) = pc.entities.first().copied().and_then(entity) {
                definition.insert("path".into(), path);
            }
        }
        _ => {
            definition.insert("_type".into(), json!("MultiLineTextDefinition"));
            if let Some(pc) = tie.as_ref().filter(|pc| pc.mask == 0x100_0000_0000) {
                let lines: Vec<serde_json::Value> =
                    pc.entities.iter().filter_map(|&e| entity(e)).collect();
                if lines.len() == 4 {
                    definition.insert("rectangleLines".into(), json!(lines));
                }
            }
            if let Some(v) = vertical {
                definition.insert("verticalAlignment".into(), json!(v));
            }
        }
    }
    if let Some(h) = horizontal {
        definition.insert("horizontalAlignment".into(), json!(h));
    }
    let mut value = json!({
        "id": id,
        "text": raw.text,
        "height": raw.height,
        "fontName": raw.font,
        "isHorizontalFlip": false,
        "isVerticalFlip": false,
        // Bit 1 bold (the weights the files store are 400 and up to 750),
        // bit 2 italic (the second flag byte, mitcad#25).
        "textStyle": i64::from(raw.weight >= 600) | (i64::from(raw.flags[1] == 1) << 1),
        "definition": definition,
        "_f3d": {
            "flags": hex(&raw.flags),
            "verticalFlag": raw.vertical_flag,
            "weight": raw.weight,
            "widthFactor": raw.width_factor,
            "kind": raw.kind,
        },
    });
    if let Some(m) = raw.matrix {
        value["position"] = json!([m[3], m[7], 0.0]);
        value["angle"] = json!(m[4].atan2(m[0]));
    }
    value
}

/// Type of a constraint mask (section 10.3).
pub fn constraint_type(mask: u64) -> Option<&'static str> {
    Some(match mask {
        0x1 => "CoincidentConstraint",
        0x2 => "CollinearConstraint",
        0x4 => "ConcentricConstraint",
        0x10 => "ParallelConstraint",
        0x20 => "PerpendicularConstraint",
        0x40 => "HorizontalConstraint",
        0x80 => "VerticalConstraint",
        0x100 => "TangentConstraint",
        // A spline and an arc or line joined smoothly (the reference
        // model `con_smooth`: a fitted spline and an arc).
        0x200 => "SmoothConstraint",
        0x400 => "SymmetryConstraint",
        0x800 => "EqualConstraint",
        0x1000 => "MidPointConstraint",
        0x2000 => "PolygonConstraint",
        0x1000_0000 => "CircularPatternConstraint",
        0x2000_0000 => "RectangularPatternConstraint",
        // The control polygon of a control point spline: its lines
        // (construction lines through the control points) and the spline
        // (the reference model `sk_spline_control`; the API lists no
        // constraint for it). Not a constraint of the geometry.
        0x8000_0000 => "ControlPolygonConstraint",
        0x20_0000_0000 => "OffsetConstraint",
        // The frame of a sketch text in a box (the reference model
        // `sk_text`: the four construction lines of its rectangle, which
        // the API gives as the definition's `rectangleLines`, with the
        // box's corner point in some files). Not a constraint of the
        // geometry.
        0x100_0000_0000 => "TextBoxConstraint",
        // A text and one curve, with four matrices (a text along a path,
        // unverified: its record names the text and the curve; the API
        // lists no constraint for it).
        0x200_0000_0000 => "TextPathConstraint",
        _ => return None,
    })
}

/// Object id -> sketch-local id, in insertion order (a repeated object
/// keeps its first position and takes the new id, like a Python dict).
#[derive(Default)]
pub struct IdTable {
    order: Vec<u64>,
    ids: HashMap<u64, String>,
}

impl IdTable {
    fn insert(&mut self, e: u64, id: String) {
        if self.ids.insert(e, id).is_none() {
            self.order.push(e);
        }
    }

    fn len(&self) -> usize {
        self.ids.len()
    }

    fn get(&self, e: u64) -> Option<&String> {
        self.ids.get(&e)
    }

    fn contains(&self, e: u64) -> bool {
        self.ids.contains_key(&e)
    }

    fn iter(&self) -> impl Iterator<Item = (u64, &String)> {
        self.order.iter().map(|e| (*e, &self.ids[e]))
    }
}

/// A sketch's SCHEMA.md detail and the decoder data that goes with it.
#[derive(Clone, Debug, PartialEq)]
pub struct SketchInfo {
    /// Sketch object id (`44A64366`).
    pub id: u64,
    /// Detail without `name` and `referencePlane` (from the timeline).
    pub detail: SketchDetail,
    /// Parameter holder of each dimension (`None`: not parsed).
    pub holders: Vec<Option<u64>>,
    /// Parameters of constraints (patterns): (constraint index, property,
    /// holder). The property holds the stored value until the builder
    /// puts the parameter there.
    pub constraint_holders: Vec<(usize, &'static str, u64)>,
}

/// A value as a parameter reference without a parameter (the builder
/// replaces it with the parameter its holder names).
fn value_ref(value: f64, unit: &str) -> serde_json::Value {
    serde_json::json!({"kind": "parameter", "value": value, "unit": unit})
}

/// All sketches' details, and object id -> (sketch id, local id) for every
/// entity, constraint and dimension (later sketches win).
pub fn sketch_details(seg: &Segment) -> (Vec<SketchInfo>, HashMap<u64, (u64, String)>) {
    let pts = points(seg);
    let lns = lines(seg);
    let arcs = arcs(seg);
    let splines = splines(seg, &pts);
    // The tangent handles of fitted splines are no sketch curves of their
    // own (the API lists none; the handle lines' points stay).
    let handle_lines: std::collections::HashSet<u64> = splines
        .values()
        .flat_map(|s| s.handles.iter().copied())
        .collect();
    let ellipses = ellipses(seg);
    let tfs = transforms(seg);
    let offsets = offsets(seg);
    let offset_dimensions: std::collections::HashSet<u64> =
        offsets.values().filter_map(|o| o.dimension).collect();
    // The curves the offsets made.
    let offset_children: std::collections::HashSet<u64> = offsets
        .values()
        .flat_map(|o| o.children.iter().copied())
        .collect();
    let mut local_ids = HashMap::new();
    let mut out = Vec::new();
    for s in seg.objects_of(SKETCH) {
        let ents = entity_list(seg, s);
        let (mut pid, mut cid, mut tid) =
            (IdTable::default(), IdTable::default(), IdTable::default());
        for &e in &ents {
            let g = seg.guid_of(e);
            if classes::is_point_class(g) {
                let n = pid.len();
                pid.insert(e, format!("p{n}"));
            } else if seg.is_kind_of(e, SKETCH_CURVE) && !handle_lines.contains(&e) {
                let n = cid.len();
                cid.insert(e, format!("c{n}"));
            } else if g == Some(SKETCH_TEXT) {
                let n = tid.len();
                tid.insert(e, format!("t{n}"));
            }
        }
        let (cons, dims) = main_lists(seg, s);
        let mut kid = IdTable::default();
        for (i, &c) in cons.iter().enumerate() {
            kid.insert(c, format!("k{i}"));
        }
        let mut did = IdTable::default();
        for (i, &x) in dims.iter().enumerate() {
            did.insert(x, format!("d{i}"));
        }
        for table in [&pid, &cid, &tid, &kid, &did] {
            for (e, id) in table.iter() {
                local_ids.insert(e, (s.id, id.clone()));
            }
        }
        let local = |e: u64| -> RefValue {
            match [&pid, &cid, &tid, &kid, &did].iter().find_map(|t| t.get(e)) {
                Some(id) => RefValue::Id(id.clone()),
                None => match lns.get(&e).filter(|_| handle_lines.contains(&e)) {
                    // A spline's handle line is no curve of the sketch; as
                    // in the API, a constraint or dimension that names it
                    // holds the line itself.
                    Some(h) => RefValue::Ref(Reference::SketchEntity(SketchEntityRef {
                        object_type: Some("SketchLine".into()),
                        geometry: Some(serde_json::json!({
                            "type": "Line3D",
                            "startPoint": h.start,
                            "endPoint": h.end(),
                        })),
                        ..SketchEntityRef::default()
                    })),
                    None => RefValue::Ref(external_ref(seg, e)),
                },
            }
        };

        let mut connected: HashMap<u64, Vec<String>> = HashMap::new();
        let mut curves = Vec::new();
        for (e, c) in cid.iter() {
            let flags = curve_flags(seg, seg.data_of(e));
            let mut rec = SketchCurve {
                id: Some(c.clone()),
                ..SketchCurve::default()
            };
            if let Some(ln) = lns.get(&e) {
                let d = ln.delta;
                rec.curve_type = Some("SketchLine".into());
                rec.start_sketch_point = Some(ln.start_point.and_then(|p| pid.get(p).cloned()));
                rec.end_sketch_point = Some(ln.end_point.and_then(|p| pid.get(p).cloned()));
                rec.length = Some(py_sum(d.iter().map(|x| x * x)).sqrt());
                rec.geometry = Some(Geometry {
                    geometry_type: Some("Line3D".into()),
                    start_point: Some(ln.start),
                    end_point: Some(ln.end()),
                    ..Geometry::default()
                });
                for p in [ln.start_point, ln.end_point].into_iter().flatten() {
                    if pid.contains(p) {
                        connected.entry(p).or_default().push(c.clone());
                    }
                }
            } else if let Some(a) = arcs.get(&e) {
                let pt = |p: Option<u64>| Some(p.and_then(|p| pid.get(p).cloned()));
                rec.center_sketch_point = pt(a.center_point);
                rec.radius = Some(a.radius);
                if a.is_circle() {
                    rec.curve_type = Some("SketchCircle".into());
                    rec.geometry = Some(Geometry {
                        geometry_type: Some("Circle3D".into()),
                        center: Some(a.center),
                        normal: Some(a.normal),
                        radius: Some(a.radius),
                        ..Geometry::default()
                    });
                } else {
                    rec.curve_type = Some("SketchArc".into());
                    rec.start_sketch_point = pt(a.start_point);
                    rec.end_sketch_point = pt(a.end_point);
                    rec.geometry = Some(Geometry {
                        geometry_type: Some("Arc3D".into()),
                        center: Some(a.center),
                        normal: Some(a.normal),
                        reference_vector: Some(a.ref_dir),
                        radius: Some(a.radius),
                        start_angle: Some(a.start_angle),
                        end_angle: Some(a.end_angle),
                        ..Geometry::default()
                    });
                }
                for p in [a.center_point, a.start_point, a.end_point]
                    .into_iter()
                    .flatten()
                {
                    if pid.contains(p) {
                        connected.entry(p).or_default().push(c.clone());
                    }
                }
            } else if let Some(s) = splines.get(&e) {
                // mitcad#154; the kind mitcad#186 ([`spline_kind`]). The curve's
                // NURBS data is kept as stored.
                let pt = |p: Option<u64>| Some(p.and_then(|p| pid.get(p).cloned()));
                let fixed = flags.as_ref().is_some_and(|f| f.fixed);
                rec.curve_type = Some(spline_kind(s, fixed, offset_children.contains(&e)).into());
                if s.has_fit_points()
                    && let (Some(a), Some(b)) = (s.start_point, s.end_point)
                {
                    // The fit points in order: the ends and those between.
                    let ids: Option<Vec<String>> = [a]
                        .into_iter()
                        .chain(s.fit_points.iter().copied())
                        .chain([b])
                        .map(|p| pid.get(p).cloned())
                        .collect();
                    rec.fit_points = ids;
                }
                rec.start_sketch_point = pt(s.start_point);
                rec.end_sketch_point = pt(s.end_point);
                rec.degree = Some(s.degree as i64);
                rec.f3d_class = seg.guid_of(e).map(str::to_string);
                rec.geometry = Some(Geometry {
                    geometry_type: Some("NurbsCurve3D".into()),
                    nurbs: Some(serde_json::json!({
                        "degree": s.degree,
                        "isRational": !s.weights.is_empty(),
                        "isPeriodic": !s.is_clamped(),
                        "controlPointCount": s.control.len(),
                        "knots": s.knots,
                        "controlPoints": s.control,
                        "weights": s.weights,
                    })),
                    ..Geometry::default()
                });
            } else if let Some(el) = ellipses.get(&e) {
                // mitcad#154.
                let pt = |p: Option<u64>| Some(p.and_then(|p| pid.get(p).cloned()));
                rec.center_sketch_point = pt(el.center_point);
                rec.major_axis_radius = Some(el.major_radius);
                rec.minor_axis_radius = Some(el.minor_radius);
                rec.major_axis = Some(el.major.map(|x| x * el.major_radius));
                rec.f3d_class = seg.guid_of(e).map(str::to_string);
                let mut g = Geometry {
                    geometry_type: Some("Ellipse3D".into()),
                    center: Some(el.center),
                    normal: Some(el.normal()),
                    major_axis: Some(el.major),
                    major_radius: Some(el.major_radius),
                    minor_radius: Some(el.minor_radius),
                    ..Geometry::default()
                };
                if el.is_closed() {
                    rec.curve_type = Some("SketchEllipse".into());
                } else {
                    rec.curve_type = Some("SketchEllipticalArc".into());
                    rec.start_sketch_point = pt(el.start_point);
                    rec.end_sketch_point = pt(el.end_point);
                    g.geometry_type = Some("EllipticalArc3D".into());
                    g.start_angle = Some(el.start_angle);
                    g.end_angle = Some(el.end_angle);
                }
                rec.geometry = Some(g);
            } else {
                rec.f3d_class = seg.guid_of(e).map(str::to_string);
            }
            if let Some(f) = &flags {
                match f.kind {
                    1 => rec.is_construction = Some(true),
                    2 => rec.is_center_line = Some(true),
                    _ => {}
                }
                if f.fixed {
                    rec.is_fixed = Some(true);
                }
                if f.linked {
                    rec.is_linked = Some(true);
                    rec.is_reference = Some(true);
                }
            }
            curves.push(rec);
        }

        let points = pid
            .iter()
            .map(|(e, p)| SketchPoint {
                id: Some(p.clone()),
                xyz: pts.get(&e).map(|g| g.xyz),
                // The sketch's origin point is fixed (mitcad#641): flag
                // bytes 3 and 5 set, on exactly one point of each sketch
                // (the first, at the origin) in the 40 designs looked at.
                // As a free point it let a sketch tied to the origin slide
                // when a dimension changed.
                is_fixed: pts
                    .get(&e)
                    .filter(|g| g.flags[3] == 1 && g.flags[5] == 1)
                    .map(|_| true),
                connected: Some(Some(connected.get(&e).cloned().unwrap_or_default())),
                ..SketchPoint::default()
            })
            .collect();

        let mut constraints = Vec::new();
        let mut constraint_holders = Vec::new();
        for &c in &cons {
            let mut rec = SketchConstraint {
                id: kid.get(c).cloned(),
                ..SketchConstraint::default()
            };
            if let Some(pc) = seg.object(c).and_then(|o| parse_constraint(seg, o)) {
                let mut typ = constraint_type(pc.mask);
                let two_points = pc.entities.len() == 2
                    && pc
                        .entities
                        .iter()
                        .all(|&e| classes::is_point_class(seg.guid_of(e)));
                if two_points {
                    match pc.mask {
                        0x40 => typ = Some("HorizontalPointsConstraint"),
                        0x80 => typ = Some("VerticalPointsConstraint"),
                        _ => {}
                    }
                }
                rec.constraint_type = typ.map(str::to_string);
                rec.refs = Some(entities_refs(pc.entities.iter().map(|&e| local(e))));
                rec.f3d = Some(ConstraintF3d {
                    mask: Some(pc.mask),
                    ..ConstraintF3d::default()
                });
                let props = constraint_props(seg, c, &pc, &offsets, &|e| {
                    [&pid, &cid, &did].iter().find_map(|t| t.get(e)).cloned()
                });
                for (name, holder) in props.holders {
                    constraint_holders.push((constraints.len(), name, holder));
                }
                if !props.values.is_empty() {
                    rec.props = Some(props.values);
                }
            }
            constraints.push(rec);
        }

        let mut dimensions = Vec::new();
        let mut holders = Vec::new();
        for &x in &dims {
            let g = seg.guid_of(x).unwrap_or_default().to_string();
            let mut rec = SketchDimension {
                id: did.get(x).cloned(),
                ..SketchDimension::default()
            };
            let pd = seg.object(x).and_then(|o| parse_dimension(seg, o));
            holders.push(pd.as_ref().and_then(|pd| pd.holder));
            if pd.is_none() {
                // Its class, so that a dimension not read can be told
                // apart in reports (mitcad#178).
                rec.f3d = Some(DimensionF3d {
                    class: Some(g.clone()),
                    ..DimensionF3d::default()
                });
            }
            if let Some(pd) = pd {
                if pd.holder.is_none() {
                    rec.is_driving = Some(false);
                }
                let kinds: String = pd
                    .entities
                    .iter()
                    .map(|&e| {
                        let k = seg.guid_of(e);
                        if classes::is_point_class(k) {
                            'P'
                        } else if classes::is_line_class(k) {
                            'L'
                        } else if k == Some(SKETCH_ARC) {
                            'A'
                        } else if k == Some(SKETCH_ELLIPSE) {
                            'E'
                        } else {
                            '?'
                        }
                    })
                    .collect();
                let mut axis_length = false;
                let mut typ = match g.as_str() {
                    ANGULAR_DIMENSION => Some("SketchAngularDimension"),
                    RADIAL_DIMENSION => Some("SketchRadialDimension"),
                    DIAMETER_DIMENSION => Some("SketchDiameterDimension"),
                    // A line and a circle or arc: the distance between them
                    // (mitcad#186: the reference model `diff_dim_kind_tangent_
                    // distance`, and the three such dimensions of the corpus
                    // by their sketch's geometry).
                    TANGENT_DISTANCE_DIMENSION => Some("SketchTangentDistanceDimension"),
                    _ => None,
                };
                let linear = g == LINEAR_DIMENSION;
                if linear && matches!(kinds.as_str(), "LL" | "LP" | "PL") {
                    // value = distance between parallel lines / point and
                    // line (354/366 in the corpus, section 10.5)
                    typ = Some("SketchOffsetDimension");
                    // Byte 4 of the last 8 flag bytes is 2 where the value
                    // is twice the distance, a diameter about the line
                    // (mitcad#33: 13 of 13 such dimensions in the corpus,
                    // and in 2026 designs; never on the 361 others).
                    let flags = pd.flags.as_deref().unwrap_or_default();
                    if flags.len() >= 8 && flags[flags.len() - 4] == 2 {
                        typ = Some("SketchLinearDiameterDimension");
                    }
                } else if linear && kinds == "AA" {
                    // concentric circles, value = radius difference (38/38)
                    typ = Some("SketchConcentricCircleDimension");
                } else if linear && kinds == "E" {
                    // An ellipse's major or minor radius (mitcad#186): of
                    // the last eight flag bytes one is set, 1 or 4 for the
                    // major radius and 2 or 8 for the minor *(the reference
                    // models `diff_dim_kind_ellipse_major` and `_minor`: 1
                    // and 2; the four ellipse dimensions of one real design:
                    // 4 and 8, which the API dump confirms)*.
                    let flags = pd.flags.as_deref().unwrap_or_default();
                    let last = &flags[flags.len().saturating_sub(8)..];
                    typ = match last.iter().filter(|&&b| b != 0).collect::<Vec<_>>()[..] {
                        [1 | 4] => Some("SketchEllipseMajorRadiusDimension"),
                        [2 | 8] => Some("SketchEllipseMinorRadiusDimension"),
                        _ => None,
                    };
                    // With 4 or 8 the value is the whole axis: the four
                    // dimensions of the one real design have twice their
                    // ellipse's radii, the reference models' (1, 2) the
                    // radii.
                    axis_length = last
                        .iter()
                        .find(|&&b| b != 0)
                        .is_some_and(|b| matches!(b, 4 | 8));
                } else if linear && matches!(kinds.as_str(), "P?" | "?P") {
                    // A point and a face or plane outside the sketch (the
                    // reference models `diff_dim_target_face` and
                    // `_construction_plane`).
                    typ = Some("SketchDistanceBetweenPointAndSurfaceDimension");
                } else if linear && matches!(kinds.as_str(), "L?" | "?L") {
                    // A line and a planar face (`diff_dim_kind_line_to_face`).
                    typ = Some("SketchDistanceBetweenLineAndPlanarSurfaceDimension");
                }
                if offset_dimensions.contains(&x) {
                    // The distance of an offset (its offset object names
                    // it, mitcad#36); it refers to one curve of the offset.
                    typ = Some("SketchOffsetCurvesDimension");
                }
                let mut props = serde_json::Map::new();
                if linear && kinds == "PP" {
                    typ = Some("SketchLinearDimension");
                    let flags = pd.flags.as_deref().unwrap_or_default();
                    let last = &flags[flags.len().saturating_sub(8)..];
                    if last.len() == 8 {
                        let orientation = if last[1] & 0x40 != 0 {
                            Some("AlignedDimensionOrientation")
                        } else if last[2] & 0x40 != 0 {
                            Some("HorizontalDimensionOrientation")
                        } else if last[2] & 0x80 != 0 {
                            Some("VerticalDimensionOrientation")
                        } else {
                            None
                        };
                        if let Some(o) = orientation {
                            props.insert("orientation".into(), o.into());
                        }
                    }
                }
                // A distance between two lines, or a point and a line, with
                // no parameter holder is driving: the 63 such dimensions of
                // the reference dumps all are; the driven ones have a
                // parameter.
                if pd.holder.is_none() && typ == Some("SketchOffsetDimension") {
                    rec.is_driving = None;
                }
                rec.dimension_type = typ.map(str::to_string);
                rec.text_position = pd.text;
                rec.refs = Some(entities_refs(pd.entities.iter().map(|&e| local(e))));
                if !props.is_empty() {
                    rec.props = Some(props);
                }
                rec.f3d = Some(DimensionF3d {
                    class: Some(g),
                    flags: Some(pd.flags.as_deref().map(hex)),
                    expression: pd.expression.clone().filter(|_| pd.holder.is_none()),
                    axis_length: axis_length.then_some(true),
                    ..DimensionF3d::default()
                });
            }
            dimensions.push(rec);
        }

        let mut detail = SketchDetail::default();
        if let Some(t) = tfs.get(&s.id) {
            // The stored matrix maps sketch space to the component's space
            // (section 10.4): SCHEMA.md's `model_frame`. The stored
            // sketch transform of external dumps may be its inverse, so
            // `transform` is not written.
            let m = t.mat4();
            let col = |c: usize| [m[0][c], m[1][c], m[2][c]];
            detail.origin = Some(col(3));
            detail.x_direction = Some(col(0));
            detail.y_direction = Some(col(1));
            detail.model_frame = Some(ModelFrame {
                origin: Some(col(3)),
                x_axis: Some(col(0)),
                y_axis: Some(col(1)),
                z_axis: Some(col(2)),
                sketch_to_model: Some(m),
                ..ModelFrame::default()
            });
        }
        let texts: Vec<serde_json::Value> = tid
            .iter()
            .filter_map(|(e, id)| {
                let raw = seg.object(e).and_then(|o| parse_text(seg, o))?;
                Some(text_value(seg, e, id, &raw, &cons, &cid))
            })
            .collect();
        if !texts.is_empty() {
            detail.texts = Some(texts);
        }
        detail.counts = Some(SketchCounts {
            points: Some(pid.len() as i64),
            curves: Some(cid.len() as i64),
            texts: Some(tid.len() as i64),
            constraints: Some(cons.len() as i64),
            dimensions: Some(dims.len() as i64),
            ..SketchCounts::default()
        });
        detail.points = Some(points);
        detail.curves = Some(curves);
        detail.constraints = Some(constraints);
        detail.dimensions = Some(dimensions);
        out.push(SketchInfo {
            id: s.id,
            detail,
            holders,
            constraint_holders,
        });
    }
    (out, local_ids)
}

/// What a constraint adds to its entities (mitcad#36), under the API's
/// property names: an offset's `distance` (cm, signed), `dimension`,
/// `parentCurves` and `childCurves`; a pattern's `createdEntities` (the
/// copies, after the originals in the entity list), a circular pattern's
/// `centerPoint` (its last entity), `quantity` and `totalAngle`, a
/// rectangular one's `quantityOne`/`Two`, `distanceOne`/`Two`,
/// `directionOne`/`Two`, `directionOneEntity` and its undecoded `flags`.
/// Parameter values are references, completed by the builder (`holders`).
#[derive(Default)]
struct ConstraintProps {
    values: serde_json::Map<String, serde_json::Value>,
    holders: Vec<(&'static str, u64)>,
}

fn constraint_props(
    seg: &Segment,
    id: u64,
    c: &ConstraintRaw,
    offsets: &HashMap<u64, OffsetRaw>,
    local: &dyn Fn(u64) -> Option<String>,
) -> ConstraintProps {
    use serde_json::json;
    let mut out = ConstraintProps::default();
    let ids = |list: &[u64]| -> serde_json::Value {
        list.iter()
            .map(|&e| local(e).map_or(serde_json::Value::Null, serde_json::Value::from))
            .collect()
    };
    // The originals come first, then their copies (instance by instance),
    // then a circular pattern's centre: the copies are all but the first
    // n / instances entities.
    let created = |entities: &[u64], instances: u32| -> Option<serde_json::Value> {
        let n = entities.len();
        let instances = instances as usize;
        if instances < 2 || !n.is_multiple_of(instances) {
            return None;
        }
        Some(ids(&entities[n / instances..]))
    };
    let d = seg.data_of(id);
    let v = &mut out.values;
    match c.mask {
        0x20_0000_0000 => {
            let Some(o) = offsets.get(&id) else {
                return out;
            };
            if let Some(dist) = o.distance {
                v.insert("distance".into(), json!(dist));
            }
            if let Some(dim) = o.dimension.and_then(local) {
                v.insert("dimension".into(), json!(dim));
            }
            v.insert("parentCurves".into(), ids(&o.parents));
            v.insert("childCurves".into(), ids(&o.children));
        }
        0x1000_0000 => {
            let Some(p) = c.tail.and_then(|t| circular_pattern(seg, d, t)) else {
                return out;
            };
            if let Some((&center, rest)) = c
                .entities
                .split_last()
                .filter(|(e, _)| classes::is_point_class(seg.guid_of(**e)))
            {
                if let Some(center) = local(center) {
                    v.insert("centerPoint".into(), json!(center));
                }
                if let Some(made) = created(rest, p.quantity) {
                    v.insert("createdEntities".into(), made);
                }
            }
            v.insert("quantity".into(), value_ref(f64::from(p.quantity), ""));
            v.insert("totalAngle".into(), value_ref(p.angle, "rad"));
            out.holders.push(("quantity", p.quantity_holder));
            out.holders.push(("totalAngle", p.angle_holder));
        }
        0x2000_0000 => {
            let Some(p) = c.tail.and_then(|t| rectangular_pattern(seg, d, t)) else {
                return out;
            };
            let instances = p.directions[0]
                .quantity
                .saturating_mul(p.directions[1].quantity);
            if let Some(made) = created(&c.entities, instances) {
                v.insert("createdEntities".into(), made);
            }
            if let Some(e) = p.direction_entity.and_then(local) {
                v.insert("directionOneEntity".into(), json!(e));
            }
            let names = [
                ("quantityOne", "distanceOne", "directionOne"),
                ("quantityTwo", "distanceTwo", "directionTwo"),
            ];
            for (dir, (q, dist, along)) in p.directions.iter().zip(names) {
                v.insert(q.into(), value_ref(f64::from(dir.quantity), ""));
                v.insert(dist.into(), value_ref(dir.distance, "cm"));
                v.insert(along.into(), json!(dir.direction));
                out.holders.push((q, dir.quantity_holder));
                out.holders.push((dist, dir.distance_holder));
            }
            v.insert("flags".into(), json!(p.flags));
        }
        _ => {}
    }
    out
}

fn entities_refs(items: impl Iterator<Item = RefValue>) -> BTreeMap<String, RefValue> {
    let mut m = BTreeMap::new();
    m.insert("entities".to_string(), RefValue::List(items.collect()));
    m
}

/// A reference to an entity outside the sketch (not decoded).
fn external_ref(seg: &Segment, id: u64) -> Reference {
    let g = seg.guid_of(id);
    Reference::Error(ErrorRef {
        object_type: Some(g.map(|g| classes::known_name(g).unwrap_or(g).to_string())),
        error: Some("not decoded".into()),
        ..ErrorRef::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The type bits the reference models settle (mitcad#186): a smooth
    /// join, a control point spline's polygon and a text's box.
    #[test]
    fn constraint_type_bits() {
        assert_eq!(constraint_type(0x200), Some("SmoothConstraint"));
        assert_eq!(constraint_type(1 << 31), Some("ControlPolygonConstraint"));
        assert_eq!(constraint_type(1 << 40), Some("TextBoxConstraint"));
        assert_eq!(constraint_type(0x40), Some("HorizontalConstraint"));
        // Two bits at once are no type.
        assert_eq!(constraint_type(0x200 | 0x40), None);
    }

    fn counted(out: &mut Vec<u8>, values: &[f64], count: usize) {
        out.extend((count as u32).to_le_bytes());
        out.extend((count as u32).to_le_bytes());
        out.extend(8u32.to_le_bytes());
        for v in values {
            out.extend(v.to_le_bytes());
        }
    }

    /// A NURBS payload behind other bytes: degree, tolerance and the three
    /// counted arrays are found, the knots counted against the poles.
    #[test]
    fn finds_the_nurbs_payload() {
        let mut d = vec![1u8, 0, 7, 0, 0, 0, 1];
        d.extend(3u32.to_le_bytes());
        d.extend(1e-9f64.to_le_bytes());
        counted(&mut d, &[0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0], 8);
        counted(&mut d, &[], 0);
        let poles = [0.0, 0.0, 0.0, 1.0, 2.0, 0.0, 3.0, 2.0, 0.0, 4.0, 0.0, 0.0];
        counted(&mut d, &poles, 4);
        d.extend([0x11, 0xea]);
        let (at, degree, knots, weights, control) = nurbs_at(&d, 0).expect("found");
        assert_eq!((at, degree), (7, 3));
        assert_eq!(knots.len(), 8);
        assert!(weights.is_empty());
        assert_eq!(
            control,
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 2.0, 0.0],
                [3.0, 2.0, 0.0],
                [4.0, 0.0, 0.0]
            ]
        );
        let s = SplineGeom {
            degree,
            knots,
            weights,
            control,
            start_point: None,
            end_point: None,
            fit_points: Vec::new(),
            handles: Vec::new(),
        };
        assert!(s.is_clamped());
        // Knots that do not count up to the poles: not a payload.
        let mut bad = d.clone();
        bad[7 + 12] = 7;
        assert!(nurbs_at(&bad, 0).is_none());
    }

    /// The end of a curve's sketch-curve part for a class version: `fixed
    /// | a | linked | ...` then the block `01 | u32 n | n references | 13
    /// zeros | f32 1.0 x 2`, with `n` references of the form `01 id`.
    fn curve_end(version: u32, kind: u8, fixed: u8, linked: u8, refs: &[u8]) -> Vec<u8> {
        let mut d = vec![7u8; 20];
        // Bytes back from the block: fixed, linked, kind (see
        // `curve_flags`).
        let (f, l, k) = match version {
            7 => (4, 2, 7),
            8 => (8, 6, 4),
            _ => (9, 7, 4),
        };
        let block = d.len() + 12;
        d.resize(block, 0);
        d[block - f] = fixed;
        d[block - f + 1] = 1;
        d[block - l] = linked;
        d[block - k] = kind;
        d.push(1);
        d.extend((refs.len() as u32).to_le_bytes());
        for &id in refs {
            d.push(1);
            d.extend([id, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        }
        d.extend([0; 13]);
        d.extend([0, 0, 0x80, 0x3f, 0, 0, 0x80, 0x3f]);
        d.extend([0; 5]);
        d
    }

    /// Fixed, linked and kind are read where the class version puts them,
    /// before a block that holds none or several references.
    #[test]
    fn curve_flags_by_version() {
        // A reference is any `01 id` that is odd.
        let ref_at = |d: &[u8], at: usize| (d[at] == 1).then(|| u64::from(d[at + 1]));
        for version in [7, 8, 9, 12] {
            for (kind, fixed, linked) in [(0, false, false), (1, true, false), (2, false, true)] {
                for refs in [&[][..], &[5u8][..], &[5, 6][..]] {
                    let d = curve_end(version, kind, u8::from(fixed), u8::from(linked), refs);
                    let f = curve_flags_of(&d, version, &ref_at).expect("flags");
                    assert_eq!((f.kind, f.fixed, f.linked), (kind, fixed, linked));
                    let ids: Vec<u64> = refs.iter().map(|&r| u64::from(r)).collect();
                    assert_eq!(f.refs, ids, "version {version}");
                }
            }
        }
        // A kind above 2, a flag above 1, an unknown version, no pair.
        assert_eq!(
            curve_flags_of(&curve_end(9, 3, 0, 0, &[]), 9, &ref_at),
            None
        );
        assert_eq!(
            curve_flags_of(&curve_end(9, 0, 2, 0, &[]), 9, &ref_at),
            None
        );
        assert_eq!(
            curve_flags_of(&curve_end(9, 0, 0, 2, &[]), 9, &ref_at),
            None
        );
        let d = curve_end(9, 0, 0, 0, &[]);
        assert_eq!(curve_flags_of(&d, 5, &ref_at), None);
        assert_eq!(curve_flags_of(&[0; 60], 9, &ref_at), None);
    }

    /// The spline kind: fit lists make a fitted spline, an offset or a
    /// fixed flag a control-point spline, else it stays a fixed spline.
    #[test]
    fn spline_kinds() {
        let mut s = SplineGeom {
            degree: 3,
            knots: Vec::new(),
            weights: Vec::new(),
            control: Vec::new(),
            start_point: None,
            end_point: None,
            fit_points: Vec::new(),
            handles: Vec::new(),
        };
        assert_eq!(spline_kind(&s, false, false), "SketchFixedSpline");
        assert_eq!(spline_kind(&s, true, false), "SketchControlPointSpline");
        assert_eq!(spline_kind(&s, false, true), "SketchControlPointSpline");
        s.handles = vec![4];
        assert_eq!(spline_kind(&s, true, true), "SketchFittedSpline");
        s.handles.clear();
        s.fit_points = vec![5];
        assert_eq!(spline_kind(&s, false, false), "SketchFittedSpline");
    }

    /// An ellipse's normal is its major direction across its minor one.
    #[test]
    fn ellipse_normal() {
        let e = EllipseGeom {
            center: [0.0; 3],
            major: [1.0, 0.0, 0.0],
            minor: [0.0, -1.0, 0.0],
            major_radius: 2.0,
            minor_radius: 1.0,
            start_angle: 0.0,
            end_angle: std::f64::consts::TAU,
            center_point: None,
            end_point: None,
            start_point: None,
        };
        assert_eq!(e.normal(), [0.0, 0.0, -1.0]);
        assert!(e.is_closed());
    }
}
