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
    SketchPoint, Vec3,
};
use super::stream::{
    Object, Segment, f64s_at, header_end, hex, py_sum, slice, str16_at, u32_at, u64_at,
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
        let Some(r) = seg.ref_at(d, root.end) else {
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
        let p = root.end;
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
    let mut best: Vec<u64> = Vec::new();
    let mut q = seg.sub_start(s);
    while q + 4 < d.len() {
        let n = u32_at(d, q).unwrap_or(0) as usize;
        if (1..=100_000).contains(&n) && n > best.len() {
            let list = refs_from(seg, d, q + 4, n);
            if list.len() == n {
                best = list;
            }
        }
        q += 1;
    }
    best
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
    let (mut cons, mut dims): (Vec<u64>, Vec<u64>) = (Vec::new(), Vec::new());
    let Some(start) = header_end(d) else {
        return (cons, dims);
    };
    let mut q = start;
    while q + 4 < end {
        let n = u32_at(d, q).unwrap_or(0) as usize;
        if (1..=100_000).contains(&n) {
            let list = refs_from(seg, d, q + 4, n);
            if list.len() == n {
                if n > cons.len() && list.iter().all(|&x| seg.is_kind_of(x, SKETCH_CONSTRAINT)) {
                    cons = list;
                } else if n > dims.len()
                    && list.iter().all(|&x| seg.is_kind_of(x, SKETCH_DIMENSION))
                {
                    dims = list;
                }
            }
        }
        q += 1;
    }
    (cons, dims)
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
    for q in p..d.len().min(p + 200) {
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
        });
    }
    None
}

/// A sketch dimension (classes under `855F0A64`, section 10.5): its tail
/// is `str16 expression | f64 text[3] | u8 flags[k] | ref holder | u32 n |
/// ref entities[n]`, anchored at the last holder reference.
#[derive(Clone, Debug, PartialEq)]
pub struct DimensionRaw {
    /// Parameter holder (`D91D429C`) of the dimension's parameter.
    pub holder: u64,
    pub entities: Vec<u64>,
    /// Copy of the parameter expression.
    pub expression: Option<String>,
    /// Text position, sketch space.
    pub text: Option<Vec3>,
    pub flags: Option<Vec<u8>>,
}

pub fn parse_dimension(seg: &Segment, o: &Object) -> Option<DimensionRaw> {
    let d = seg.data(o);
    let (hp, hr) = seg
        .refs_in(d, 0, d.len())
        .into_iter()
        .rev()
        .find(|(_, r)| seg.guid_of(r.id) == Some(PARAMETER_HOLDER))?;
    let n = u32_at(d, hr.end)?;
    let mut q = hr.end + 4;
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
        holder: hr.id,
        entities,
        expression,
        text,
        flags,
    })
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
        0x400 => "SymmetryConstraint",
        0x800 => "EqualConstraint",
        0x1000 => "MidPointConstraint",
        0x2000 => "PolygonConstraint",
        0x1000_0000 => "CircularPatternConstraint",
        0x2000_0000 => "RectangularPatternConstraint",
        0x20_0000_0000 => "OffsetConstraint",
        _ => return None,
    })
}

/// Object id -> sketch-local id, in insertion order (a repeated object
/// keeps its first position and takes the new id, like a Python dict).
#[derive(Default)]
struct IdTable {
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
}

/// All sketches' details, and object id -> (sketch id, local id) for every
/// entity, constraint and dimension (later sketches win).
pub fn sketch_details(seg: &Segment) -> (Vec<SketchInfo>, HashMap<u64, (u64, String)>) {
    let pts = points(seg);
    let lns = lines(seg);
    let arcs = arcs(seg);
    let tfs = transforms(seg);
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
            } else if seg.is_kind_of(e, SKETCH_CURVE) {
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
                None => RefValue::Ref(external_ref(seg, e)),
            }
        };

        let mut connected: HashMap<u64, Vec<String>> = HashMap::new();
        let mut curves = Vec::new();
        for (e, c) in cid.iter() {
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
            } else {
                rec.f3d_class = seg.guid_of(e).map(str::to_string);
            }
            curves.push(rec);
        }

        let points = pid
            .iter()
            .map(|(e, p)| SketchPoint {
                id: Some(p.clone()),
                xyz: pts.get(&e).map(|g| g.xyz),
                connected: Some(Some(connected.get(&e).cloned().unwrap_or_default())),
                ..SketchPoint::default()
            })
            .collect();

        let mut constraints = Vec::new();
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
            holders.push(pd.as_ref().map(|pd| pd.holder));
            if let Some(pd) = pd {
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
                        } else {
                            '?'
                        }
                    })
                    .collect();
                let mut typ = match g.as_str() {
                    ANGULAR_DIMENSION => Some("SketchAngularDimension"),
                    RADIAL_DIMENSION => Some("SketchRadialDimension"),
                    DIAMETER_DIMENSION => Some("SketchDiameterDimension"),
                    _ => None,
                };
                let linear = g == LINEAR_DIMENSION;
                if linear && matches!(kinds.as_str(), "LL" | "LP" | "PL") {
                    // value = distance between parallel lines / point and
                    // line (354/366 in the corpus, section 10.5)
                    typ = Some("SketchOffsetDimension");
                } else if linear && kinds == "AA" {
                    // concentric circles, value = radius difference (38/38)
                    typ = Some("SketchConcentricCircleDimension");
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
                rec.dimension_type = typ.map(str::to_string);
                rec.text_position = pd.text;
                rec.refs = Some(entities_refs(pd.entities.iter().map(|&e| local(e))));
                if !props.is_empty() {
                    rec.props = Some(props);
                }
                rec.f3d = Some(DimensionF3d {
                    class: Some(g),
                    flags: Some(pd.flags.as_deref().map(hex)),
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
        });
    }
    (out, local_ids)
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
