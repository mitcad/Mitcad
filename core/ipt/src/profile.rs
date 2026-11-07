// SPDX-License-Identifier: MIT
//! The profiles an extrusion or revolution selected, measured: their area
//! and centroid in their sketch's coordinates, so that the import picks the
//! same regions of Mitcad's sketch.
//!
//! A profile selection (`3b2477a4…`): the header, the prefix, a reference
//! to an entity link (`21750fcc…`) and a byte. The entity link: u32, u16,
//! u32, a byte, a reference to an ASM record of the definitions segment (of
//! the B-rep record's type), a u32 id and a u32 index of a lump *(seen)*.
//! That ASM file keeps every selected profile as a wire body whose loops
//! are the region's boundary, one body per selection *(verified: as many
//! wire bodies as selections in the test files)*; its body record's second
//! field is the id. Up to segment major 24 the body is a sheet of one
//! planar face whose loops are the boundary *(seen)*.

use std::collections::HashMap;

use mitcad_asm::asm::file::AsmFile;
use mitcad_asm::asm::token::Token;
use mitcad_asm::convert;
use mitcad_vec3::{cross, dot, unit_div_above};

use crate::dc::{Definitions, type_id};

pub const SELECTION: [u8; 16] = type_id("3b2477a4d1118f96000826bd0663dc09");
pub const ENTITY_LINK: [u8; 16] = type_id("21750fccd1112780e38619962259017a");
/// A path selection (a sweep's path, `473f20fc…`): laid out as a profile
/// selection; its wire body is the path *(seen)*.
pub const PATH_SELECTION: [u8; 16] = type_id("473f20fcd11101cf000835bd0663dc09");
/// A loft's section (`e850314b…`): after the header and the prefix the
/// entity link as a profile selection's, then records not decoded; its
/// wire body is the section's boundary *(seen)*.
pub const LOFT_SECTION: [u8; 16] = type_id("e850314bd111d641000880ba32a3dc09");

/// A profile's area (cm²) and centroid (cm), in its sketch's coordinates,
/// and its outer boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct Measured {
    pub area: f64,
    pub centroid: [f64; 2],
    pub outer: Vec<[f64; 2]>,
    /// Every loop of it (the outer boundary and the holes).
    pub loops: Vec<Vec<[f64; 2]>>,
}

impl Measured {
    /// Whether a point lies inside the outer boundary (even-odd rule).
    pub fn contains(&self, p: [f64; 2]) -> bool {
        let poly = &self.outer;
        let mut inside = false;
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            if (a[1] > p[1]) != (b[1] > p[1])
                && p[0] < a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1])
            {
                inside = !inside;
            }
        }
        inside
    }
}

/// Whether some selection lies inside another one's boundary (then
/// [`even_odd`] makes up the regions).
pub fn nested(measured: &[Measured]) -> bool {
    measured.iter().enumerate().any(|(i, outer)| {
        measured
            .iter()
            .enumerate()
            .any(|(j, inner)| i != j && inner.area < outer.area && outer.contains(inner.centroid))
    })
}

/// Whether selection `inner` lies within selection `outer` and runs along
/// its boundary for part of its own (a region beside the outline, inside
/// it, selected with it): every point of `inner` lies in `outer` or on its
/// boundary, and at least a twentieth of `inner`'s boundary lies on
/// `outer`'s. Such a selection adds nothing to the outline's: the file's
/// extrusion is the outline's region *(seen: a lid's outline and the ring
/// between it and an inner outline touching it at two corners, both
/// selected; the history state adds the whole outline's slab, not the
/// outline less the ring)*. A selection inside another one that does not
/// touch its boundary is a hole of it ([`even_odd`]).
pub fn subsumed(outer: &Measured, inner: &Measured) -> bool {
    if inner.area >= outer.area || outer.loops.is_empty() || inner.loops.is_empty() {
        return false;
    }
    let bbox = |p: &[[f64; 2]]| {
        p.iter()
            .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, q| {
                [
                    b[0].min(q[0]),
                    b[1].min(q[1]),
                    b[2].max(q[0]),
                    b[3].max(q[1]),
                ]
            })
    };
    let tol = 2e-5 * outer.area.sqrt() + 1e-7;
    let (bo, bi) = (bbox(&outer.outer), bbox(&inner.outer));
    if bi[0] < bo[0] - tol || bi[1] < bo[1] - tol || bi[2] > bo[2] + tol || bi[3] > bo[3] + tol {
        return false;
    }
    let near_boundary = |p: [f64; 2]| {
        outer
            .loops
            .iter()
            .any(|l| (0..l.len()).any(|i| segment_distance(p, l[i], l[(i + 1) % l.len()]) <= tol))
    };
    // Inside the outer loop and in none of the holes (even-odd over all of
    // `outer`'s loops).
    let inside = |p: [f64; 2]| {
        let mut odd = false;
        for l in &outer.loops {
            for i in 0..l.len() {
                let (a, c) = (l[i], l[(i + 1) % l.len()]);
                if (a[1] > p[1]) != (c[1] > p[1])
                    && p[0] < a[0] + (p[1] - a[1]) * (c[0] - a[0]) / (c[1] - a[1])
                {
                    odd = !odd;
                }
            }
        }
        odd
    };
    // (A selection all along `outer`'s boundary, its hole selected again,
    // lies in no part of it.)
    let (mut shared, mut within) = (0.0, 0.0);
    for l in &inner.loops {
        for i in 0..l.len() {
            let (a, b) = (l[i], l[(i + 1) % l.len()]);
            let length = (b[0] - a[0]).hypot(b[1] - a[1]);
            let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
            if near_boundary(mid) {
                shared += length;
            } else if inside(mid) {
                within += length;
            } else {
                return false;
            }
        }
    }
    within > 0.0 && shared >= 0.05 * (shared + within)
}

/// The regions selections inside one another make up: a region of the
/// sketch is taken when an odd number of the selections cover it. A
/// boundary selected inside another selection's (a circle in the outline
/// of a plate, both selected, the outline's selection a single loop) is a
/// hole of it *(seen: the history states agree, 1 face and the circle's
/// area off when the circle is filled)*, and a selection that keeps the
/// circle as its hole and the circle's own one make up the whole plate.
///
/// The distinct loops of all selections nest in a tree; each loop with
/// its children taken off is one of the sketch's regions as far as the
/// selections tell. A selection covers the regions of its outer loop and
/// the loops inside it, but none inside its holes. The regions covered an
/// odd number of times come out, each with its area, centroid and loops
/// (its own and its children's). None when the loops cannot be told apart
/// (too many, or a region without area).
pub fn even_odd(measured: &[Measured]) -> Option<Vec<Measured>> {
    // The distinct loops: (points, area, centroid, bounding box).
    struct Loop {
        points: Vec<[f64; 2]>,
        area: f64,
        centroid: [f64; 2],
        bbox: [f64; 4],
    }
    let mut loops: Vec<Loop> = Vec::new();
    let same = |l: &Loop, area: f64, centroid: [f64; 2]| {
        let scale = l.area.sqrt().max(1e-6);
        (l.area - area).abs() <= 1e-6 * l.area.max(1e-12)
            && (l.centroid[0] - centroid[0]).hypot(l.centroid[1] - centroid[1]) <= 1e-6 * scale
    };
    // Each selection's (outer, holes) as indices into `loops`.
    let mut selections: Vec<(usize, Vec<usize>)> = Vec::new();
    for m in measured {
        let mut ids = Vec::new();
        for points in &m.loops {
            let (a, c) = shoelace(points);
            let a = a.abs();
            if a <= 1e-12 {
                return None;
            }
            let id = match loops.iter().position(|l| same(l, a, c)) {
                Some(k) => k,
                None => {
                    let mut bbox = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
                    for p in points {
                        bbox = [
                            bbox[0].min(p[0]),
                            bbox[1].min(p[1]),
                            bbox[2].max(p[0]),
                            bbox[3].max(p[1]),
                        ];
                    }
                    loops.push(Loop {
                        points: points.clone(),
                        area: a,
                        centroid: c,
                        bbox,
                    });
                    loops.len() - 1
                }
            };
            ids.push(id);
        }
        let (&outer, holes) = ids.split_first()?;
        selections.push((outer, holes.to_vec()));
        if loops.len() > 2000 {
            return None;
        }
    }
    // Whether loop `inner` lies inside loop `outer`: its points away from
    // `outer`'s boundary (the selections share edges, and their arcs are
    // sampled at other places) vote.
    let inside = |outer: &Loop, inner: &Loop| -> bool {
        if inner.area >= outer.area {
            return false;
        }
        let tol = 2e-5 * outer.area.sqrt() + 1e-7;
        let b = (outer.bbox, inner.bbox);
        if b.1[0] < b.0[0] - tol
            || b.1[1] < b.0[1] - tol
            || b.1[2] > b.0[2] + tol
            || b.1[3] > b.0[3] + tol
        {
            return false;
        }
        let poly = &outer.points;
        let n = inner.points.len();
        let samples = n.min(32);
        let (mut yes, mut no) = (0, 0);
        for k in 0..samples {
            let p = inner.points[k * n / samples];
            let mut odd = false;
            let mut near = false;
            for i in 0..poly.len() {
                let (a, c) = (poly[i], poly[(i + 1) % poly.len()]);
                if (a[1] > p[1]) != (c[1] > p[1])
                    && p[0] < a[0] + (p[1] - a[1]) * (c[0] - a[0]) / (c[1] - a[1])
                {
                    odd = !odd;
                }
                if segment_distance(p, a, c) <= tol {
                    near = true;
                }
            }
            match (near, odd) {
                (true, _) => {}
                (false, true) => yes += 1,
                (false, false) => no += 1,
            }
        }
        yes > no
    };
    // Each loop's parent: the smallest loop it lies inside.
    let mut order: Vec<usize> = (0..loops.len()).collect();
    order.sort_by(|&a, &b| loops[b].area.total_cmp(&loops[a].area));
    let mut parent: Vec<Option<usize>> = vec![None; loops.len()];
    for (k, &i) in order.iter().enumerate() {
        parent[i] = order[..k]
            .iter()
            .rev()
            .copied()
            .find(|&j| inside(&loops[j], &loops[i]));
    }
    // Whether loop `i` is loop `a` or lies inside it.
    let within = |mut i: usize, a: usize| loop {
        if i == a {
            return true;
        }
        match parent[i] {
            Some(p) => i = p,
            None => return false,
        }
    };
    let mut out = Vec::new();
    for i in 0..loops.len() {
        let covered = selections
            .iter()
            .filter(|(outer, holes)| within(i, *outer) && !holes.iter().any(|&h| within(i, h)))
            .count();
        if covered % 2 == 0 {
            continue;
        }
        let children: Vec<usize> = (0..loops.len()).filter(|&c| parent[c] == Some(i)).collect();
        let mut area = loops[i].area;
        let mut moment = [loops[i].centroid[0] * area, loops[i].centroid[1] * area];
        for &c in &children {
            area -= loops[c].area;
            moment[0] -= loops[c].centroid[0] * loops[c].area;
            moment[1] -= loops[c].centroid[1] * loops[c].area;
        }
        if area <= 1e-9 * loops[i].area {
            return None;
        }
        out.push(Measured {
            area,
            centroid: [moment[0] / area, moment[1] / area],
            outer: loops[i].points.clone(),
            loops: std::iter::once(i)
                .chain(children)
                .map(|k| loops[k].points.clone())
                .collect(),
        });
    }
    Some(out)
}

/// The distance from a point to a segment.
fn segment_distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let l2 = d[0] * d[0] + d[1] * d[1];
    let t = if l2 > 0.0 {
        (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p[0] - a[0] - t * d[0]).hypot(p[1] - a[1] - t * d[1])
}

/// The ASM files of the definitions segment, parsed when first needed.
#[derive(Default)]
pub struct Profiles {
    files: HashMap<usize, Option<AsmFile>>,
    /// The loops of the profiles measured for features: (the sketch
    /// record, the selection they belong to (the record of a feature's
    /// profiles or of a loft's section), the loop in its coordinates).
    pub outlines: Vec<(usize, usize, Vec<[f64; 2]>)>,
    /// The profile selections features made: (the sketch record, the
    /// selection, the sketch's transform), for the curves of their
    /// outlines (`Profiles::outline_edges`).
    pub selected: Vec<(usize, usize, [[f64; 4]; 4])>,
}

impl Profiles {
    fn file(&mut self, dc: &Definitions, record: usize) -> Option<&AsmFile> {
        self.files
            .entry(record)
            .or_insert_with(|| {
                let bytes = dc.bytes(record).get(crate::BREP_RECORD_HEAD..)?;
                AsmFile::parse(bytes).ok()
            })
            .as_ref()
    }

    /// The body record and ASM file a profile selection names.
    fn body(&mut self, dc: &Definitions, selection: usize) -> Option<(usize, &AsmFile)> {
        if !dc.is(selection, &SELECTION)
            && !dc.is(selection, &PATH_SELECTION)
            && !dc.is(selection, &LOFT_SECTION)
        {
            return None;
        }
        let mut r = dc.fields(selection);
        let link = r.reference().ok()??;
        if !dc.is(link, &ENTITY_LINK) {
            return None;
        }
        let mut r = dc.reader(link);
        r.skip(6 + 4 + 1).ok()?;
        let asm = r.reference().ok()??;
        let id = i64::from(r.u32().ok()?);
        if !dc.is(asm, &crate::BREP_RECORD_TYPE) {
            return None;
        }
        let file = self.file(dc, asm)?;
        let record = (0..file.records.len()).find(|&i| {
            file.records[i].type_name == "body"
                && matches!(file.fields(i).get(1), Some(Token::Int(v)) if *v == id)
        })?;
        Some((record, file))
    }

    /// The loops of a profile selection's wire body: points along them
    /// (mm, model coordinates).
    pub fn loops(&mut self, dc: &Definitions, selection: usize) -> Option<Vec<Vec<[f64; 3]>>> {
        let (body, file) = self.body(dc, selection)?;
        wire_loops(file, body)
    }

    /// A profile selection's area and centroid in the sketch whose
    /// transform (sketch to model, cm) is `m`.
    pub fn measure(
        &mut self,
        dc: &Definitions,
        selection: usize,
        m: &[[f64; 4]; 4],
    ) -> Option<Measured> {
        let loops = self.loops(dc, selection)?;
        let column = |c: usize| [m[0][c], m[1][c], m[2][c]];
        let (x, y, o) = (column(0), column(1), column(3));
        let local = |p: [f64; 3]| {
            let d = [p[0] / 10.0 - o[0], p[1] / 10.0 - o[1], p[2] / 10.0 - o[2]];
            [
                d[0] * x[0] + d[1] * x[1] + d[2] * x[2],
                d[0] * y[0] + d[1] * y[1] + d[2] * y[2],
            ]
        };
        let mut parts: Vec<(f64, [f64; 2], Vec<[f64; 2]>)> = loops
            .iter()
            .map(|l| {
                let pts: Vec<[f64; 2]> = l.iter().map(|&p| local(p)).collect();
                let (a, c) = shoelace(&pts);
                (a, c, pts)
            })
            .collect();
        parts.sort_by(|a, b| b.0.abs().total_cmp(&a.0.abs()));
        let (outer, inner) = parts.split_first()?;
        let mut area = outer.0.abs();
        let mut moment = [outer.1[0] * area, outer.1[1] * area];
        for (a, c, _) in inner {
            area -= a.abs();
            moment[0] -= c[0] * a.abs();
            moment[1] -= c[1] * a.abs();
        }
        (area > 1e-12).then(|| Measured {
            area,
            centroid: [moment[0] / area, moment[1] / area],
            outer: outer.2.clone(),
            loops: parts.iter().map(|p| p.2.clone()).collect(),
        })
    }
}

impl Profiles {
    /// The edges of a profile selection's loops as curves in the sketch
    /// whose transform (sketch to model, cm) is `m` (mitcad#235: the
    /// outlines of what the sketch draws that is not decoded, as texts'
    /// glyphs). None when an edge is not a line, an ellipse's arc or a
    /// B-spline, or leaves the sketch's plane.
    pub fn outline_edges(
        &mut self,
        dc: &Definitions,
        selection: usize,
        m: &[[f64; 4]; 4],
    ) -> Option<Vec<OutlineEdge>> {
        let (body, file) = self.body(dc, selection)?;
        let options = crate::convert_options();
        let column = |c: usize| [m[0][c], m[1][c], m[2][c]];
        let (x, y, o) = (column(0), column(1), column(3));
        let z = unit_div_above(cross(x, y), 0.0)?;
        // Model (mm) to the sketch (cm), None off its plane.
        let local = |p: [f64; 3]| -> Option<[f64; 2]> {
            let d = [p[0] / 10.0 - o[0], p[1] / 10.0 - o[1], p[2] / 10.0 - o[2]];
            (dot(d, z).abs() <= 1e-6).then(|| [dot(d, x), dot(d, y)])
        };
        let mut out = Vec::new();
        for edge in wire_edges(file, body)? {
            let (curve, t, transform) =
                convert::edge_curve(file, body, edge, &options, None).ok()?;
            let moved = |p: [f64; 3]| match &transform {
                Some(tr) => [
                    tr.m[0][0] * p[0] + tr.m[0][1] * p[1] + tr.m[0][2] * p[2] + tr.t[0],
                    tr.m[1][0] * p[0] + tr.m[1][1] * p[1] + tr.m[1][2] * p[2] + tr.t[1],
                    tr.m[2][0] * p[0] + tr.m[2][1] * p[1] + tr.m[2][2] * p[2] + tr.t[2],
                ],
                None => p,
            };
            let to = |p: [f64; 3]| local(moved(p));
            out.push(outline_edge(&curve, t, &to)?);
        }
        (!out.is_empty()).then_some(out)
    }
}

/// An edge of a profile's outline in its sketch (cm): a line, a circle's
/// arc (from the angle `start` by `sweep`, radians, counterclockwise when
/// positive), or a NURBS curve clamped at its ends (knots in full, one
/// weight per control point when rational).
#[derive(Clone, Debug, PartialEq)]
pub enum OutlineEdge {
    Line([f64; 2], [f64; 2]),
    Arc {
        center: [f64; 2],
        radius: f64,
        start: f64,
        sweep: f64,
    },
    Nurbs {
        degree: usize,
        knots: Vec<f64>,
        poles: Vec<[f64; 2]>,
        weights: Option<Vec<f64>>,
    },
}

impl OutlineEdge {
    /// The point at `s` of its parameter range, 0 at its start and 1 at its
    /// end.
    pub fn at(&self, s: f64) -> [f64; 2] {
        match self {
            OutlineEdge::Line(a, b) => [a[0] + s * (b[0] - a[0]), a[1] + s * (b[1] - a[1])],
            OutlineEdge::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let a = start + s * sweep;
                [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
            }
            OutlineEdge::Nurbs {
                degree,
                knots,
                poles,
                weights,
            } => {
                let (t0, t1) = (knots[*degree], knots[poles.len()]);
                nurbs_point(
                    *degree,
                    knots,
                    poles,
                    weights.as_deref(),
                    t0 + s * (t1 - t0),
                )
            }
        }
    }
}

/// The edges of a wire body's loops (as [`wire_loops`] walks them), each
/// once.
fn wire_edges(file: &AsmFile, body: usize) -> Option<Vec<usize>> {
    let mut edges = Vec::new();
    let mut walk = |first: usize, guard: &mut usize| -> Option<()> {
        let mut coedge = Some(first);
        while let Some(c) = coedge {
            *guard += 1;
            if *guard > 100_000 {
                return None;
            }
            let edge = ptr(file, c, 6)?;
            if !edges.contains(&edge) {
                edges.push(edge);
            }
            let next = ptr(file, c, 3);
            coedge = next.filter(|&n| n != first && n != c);
        }
        Some(())
    };
    let mut guard = 0;
    let mut steps = ChainSteps::new(file);
    let mut lump = ptr(file, body, 3);
    while let Some(l) = lump {
        steps.step()?;
        let mut shell = ptr(file, l, 4);
        while let Some(s) = shell {
            steps.step()?;
            let mut wire = ptr(file, s, 6);
            while let Some(w) = wire {
                steps.step()?;
                walk(ptr(file, w, 4)?, &mut guard)?;
                wire = ptr(file, w, 3);
            }
            let mut face = ptr(file, s, 5);
            while let Some(f) = face {
                steps.step()?;
                let mut lp = ptr(file, f, 4);
                while let Some(l) = lp {
                    steps.step()?;
                    walk(ptr(file, l, 4)?, &mut guard)?;
                    lp = ptr(file, l, 3);
                }
                face = ptr(file, f, 3);
            }
            shell = ptr(file, s, 3);
        }
        lump = ptr(file, l, 3);
    }
    (!edges.is_empty()).then_some(edges)
}

/// A curve's piece over `t` as an [`OutlineEdge`], its points mapped by
/// `to` (None when one is not mapped).
fn outline_edge(
    curve: &mitcad_asm::brep::Curve,
    t: [f64; 2],
    to: &dyn Fn([f64; 3]) -> Option<[f64; 2]>,
) -> Option<OutlineEdge> {
    use mitcad_asm::brep::Curve;
    let (t0, t1) = (t[0].min(t[1]), t[0].max(t[1]));
    if t1.partial_cmp(&t0) != Some(std::cmp::Ordering::Greater) {
        return None;
    }
    match curve {
        Curve::Line { .. } => Some(OutlineEdge::Line(to(curve.eval(t0))?, to(curve.eval(t1))?)),
        // An ellipse's arc as rational quadratic pieces of at most a
        // quarter turn: each the affine image of a circle's.
        Curve::Ellipse {
            center,
            normal,
            major_dir,
            major,
            minor,
        } => {
            let v = cross(*normal, *major_dir);
            // A circle in the sketch's plane: its arc as one (a profile's
            // hole edge, which a taper needs as an arc, mitcad#235).
            if (major - minor).abs() <= 1e-9 * major.abs() {
                let c = to(*center)?;
                let point = |a: f64| curve.eval(a);
                let (p0, p1) = (to(point(t0))?, to(point(t1))?);
                let step = to(point(t0 + ((t1 - t0) / 4.0).min(0.1)))?;
                let radius = (p0[0] - c[0]).hypot(p0[1] - c[1]);
                let round = [p1, step, to(point(0.5 * (t0 + t1)))?]
                    .iter()
                    .all(|p| ((p[0] - c[0]).hypot(p[1] - c[1]) - radius).abs() <= 1e-9 * radius);
                if round && radius > 0.0 {
                    let ccw =
                        (p0[0] - c[0]) * (step[1] - c[1]) - (p0[1] - c[1]) * (step[0] - c[0]) > 0.0;
                    return Some(OutlineEdge::Arc {
                        center: c,
                        radius,
                        start: (p0[1] - c[1]).atan2(p0[0] - c[0]),
                        sweep: if ccw { t1 - t0 } else { t0 - t1 },
                    });
                }
            }
            let at = |a: f64, r: f64| {
                [
                    center[0] + r * (major * a.cos() * major_dir[0] + minor * a.sin() * v[0]),
                    center[1] + r * (major * a.cos() * major_dir[1] + minor * a.sin() * v[1]),
                    center[2] + r * (major * a.cos() * major_dir[2] + minor * a.sin() * v[2]),
                ]
            };
            let n = ((t1 - t0) / std::f64::consts::FRAC_PI_2 - 1e-9)
                .ceil()
                .max(1.0) as usize;
            let h = (t1 - t0) / (2.0 * n as f64);
            let mut poles = vec![to(at(t0, 1.0))?];
            let mut weights = vec![1.0];
            let mut knots = vec![0.0; 3];
            for k in 0..n {
                let a = t0 + 2.0 * h * k as f64;
                poles.push(to(at(a + h, 1.0 / h.cos()))?);
                weights.push(h.cos());
                poles.push(to(at(a + 2.0 * h, 1.0))?);
                weights.push(1.0);
                let end = (k + 1) as f64;
                knots.extend(if k + 1 == n {
                    vec![end; 3]
                } else {
                    vec![end; 2]
                });
            }
            Some(OutlineEdge::Nurbs {
                degree: 2,
                knots,
                poles,
                weights: Some(weights),
            })
        }
        Curve::BSpline(b) => {
            let knots: Vec<f64> = b
                .knots
                .iter()
                .zip(&b.mults)
                .flat_map(|(&k, &m)| std::iter::repeat_n(k, m))
                .collect();
            let p = b.degree;
            if p == 0 || knots.len() != b.poles.len() + p + 1 {
                return None;
            }
            let poles: Option<Vec<[f64; 2]>> = b.poles.iter().map(|&q| to(q)).collect();
            let weights = b.weights.clone();
            if weights.as_ref().is_some_and(|w| w.len() != b.poles.len()) {
                return None;
            }
            let (poles, knots, weights) = trimmed(p, knots, poles?, weights, t0, t1)?;
            Some(OutlineEdge::Nurbs {
                degree: p,
                knots,
                poles,
                weights,
            })
        }
        Curve::Interpolated { .. }
        | Curve::OnSurface { .. }
        | Curve::Helix(_)
        | Curve::Unknown(_) => None,
    }
}

/// A NURBS curve's piece over `[t0, t1]`, clamped at both ends: knots
/// inserted at the ends until each is there `degree` times (Boehm's
/// insertion on the weighted control points). None when the piece leaves
/// the curve's range.
#[allow(clippy::type_complexity)]
fn trimmed(
    p: usize,
    mut knots: Vec<f64>,
    poles: Vec<[f64; 2]>,
    weights: Option<Vec<f64>>,
    t0: f64,
    t1: f64,
) -> Option<(Vec<[f64; 2]>, Vec<f64>, Option<Vec<f64>>)> {
    let n = poles.len();
    let (lo, hi) = (knots[p], knots[n]);
    let slack = 1e-9 * (hi - lo).abs().max(1.0);
    if t0 < lo - slack || t1 > hi + slack {
        return None;
    }
    let (t0, t1) = (t0.max(lo), t1.min(hi));
    // Weighted control points (x w, y w, w).
    let mut q: Vec<[f64; 3]> = poles
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let w = weights.as_ref().map_or(1.0, |w| w[i]);
            [a[0] * w, a[1] * w, w]
        })
        .collect();
    let same = |a: f64, b: f64| (a - b).abs() <= slack;
    for u in [t0, t1] {
        let times = knots.iter().filter(|&&k| same(k, u)).count();
        for _ in times..p {
            // The span: the last knot at or below u (among the poles').
            let k = (p..q.len()).rev().find(|&i| knots[i] <= u + slack)?;
            let mut next = Vec::with_capacity(q.len() + 1);
            next.extend_from_slice(&q[..=k - p]);
            for i in k - p + 1..=k {
                let d = knots[i + p] - knots[i];
                let a = if d > 0.0 { (u - knots[i]) / d } else { 0.0 };
                next.push([
                    (1.0 - a) * q[i - 1][0] + a * q[i][0],
                    (1.0 - a) * q[i - 1][1] + a * q[i][1],
                    (1.0 - a) * q[i - 1][2] + a * q[i][2],
                ]);
            }
            next.extend_from_slice(&q[k..]);
            q = next;
            knots.insert(k + 1, u);
        }
    }
    // t0's last place and t1's first: the poles between.
    let s0 = knots.iter().rposition(|&k| same(k, t0))?;
    let r1 = knots.iter().position(|&k| same(k, t1))?;
    if s0 < p || r1 < 1 || r1 - 1 < s0 - p {
        return None;
    }
    let piece = &q[s0 - p..r1];
    let mut out = vec![t0; p + 1];
    out.extend(knots[s0 + 1..r1].iter().copied());
    out.extend(std::iter::repeat_n(t1, p + 1));
    let poles: Vec<[f64; 2]> = piece.iter().map(|a| [a[0] / a[2], a[1] / a[2]]).collect();
    let weights = weights.map(|_| piece.iter().map(|a| a[2]).collect());
    (out.len() == poles.len() + p + 1).then_some((poles, out, weights))
}

/// A NURBS curve's point at `t` (de Boor's algorithm on the weighted
/// control points).
fn nurbs_point(
    p: usize,
    knots: &[f64],
    poles: &[[f64; 2]],
    weights: Option<&[f64]>,
    t: f64,
) -> [f64; 2] {
    let n = poles.len();
    let k = (p..n).rev().find(|&i| knots[i] <= t).unwrap_or(p);
    let mut d: Vec<[f64; 3]> = (0..=p)
        .map(|j| {
            let i = k - p + j;
            let w = weights.map_or(1.0, |w| w[i]);
            [poles[i][0] * w, poles[i][1] * w, w]
        })
        .collect();
    for r in 1..=p {
        for j in (r..=p).rev() {
            let i = k - p + j;
            let den = knots[i + p + 1 - r] - knots[i];
            let a = if den > 0.0 { (t - knots[i]) / den } else { 0.0 };
            let before = d[j - 1];
            for (x, b) in d[j].iter_mut().zip(before) {
                *x = (1.0 - a) * b + a * *x;
            }
        }
    }
    [d[p][0] / d[p][2], d[p][1] / d[p][2]]
}

/// The steps left along a body's lump, shell, wire, face and loop chains
/// (mitcad#933). A sound file visits each of its records at most once on
/// them, so a walk of more steps than the file has records follows a
/// chain that comes back on itself (a `next` naming its own record or an
/// earlier one), which would otherwise never end: the walk gives up (None).
struct ChainSteps(usize);

impl ChainSteps {
    fn new(file: &AsmFile) -> ChainSteps {
        ChainSteps(file.records.len())
    }

    /// One step; None once they are spent.
    fn step(&mut self) -> Option<()> {
        self.0 = self.0.checked_sub(1)?;
        Some(())
    }
}

/// The record a pointer field of a record names.
fn ptr(file: &AsmFile, record: usize, field: usize) -> Option<usize> {
    match file.fields(record).get(field)? {
        Token::Ptr(p) if *p >= 0 => file.record_of(*p),
        _ => None,
    }
}

/// The loops of a wire body: body → lump (field 3) → shell (field 4) →
/// wires (shell field 6, next wire field 3) → coedges (wire field 4; next
/// field 3, edge field 6, reversed field 7), each sampled along its edge.
/// Older files (segment majors up to 24) keep a selected profile as a body
/// of one planar face instead: shell → face (field 5) → loops (face field
/// 4, next loop field 3) → coedges (loop field 4) *(seen)*; a body of
/// several faces is not measured.
fn wire_loops(file: &AsmFile, body: usize) -> Option<Vec<Vec<[f64; 3]>>> {
    let mut out = Vec::new();
    let mut lump = ptr(file, body, 3);
    let mut guard = 0;
    let mut steps = ChainSteps::new(file);
    let mut faces = 0;
    while let Some(l) = lump {
        steps.step()?;
        let mut shell = ptr(file, l, 4);
        while let Some(s) = shell {
            steps.step()?;
            let mut wire = ptr(file, s, 6);
            while let Some(w) = wire {
                steps.step()?;
                if let Some(points) = coedge_loop(file, body, ptr(file, w, 4)?, &mut guard)? {
                    out.push(points);
                }
                wire = ptr(file, w, 3);
            }
            let mut face = ptr(file, s, 5);
            while let Some(f) = face {
                steps.step()?;
                faces += 1;
                let mut lp = ptr(file, f, 4);
                while let Some(l) = lp {
                    steps.step()?;
                    if let Some(points) = coedge_loop(file, body, ptr(file, l, 4)?, &mut guard)? {
                        out.push(points);
                    }
                    lp = ptr(file, l, 3);
                }
                face = ptr(file, f, 3);
            }
            shell = ptr(file, s, 3);
        }
        lump = ptr(file, l, 3);
    }
    (!out.is_empty() && faces <= 1).then_some(out)
}

/// The points along a loop of coedges from `first` (None when it has
/// fewer than three, an open one fewer than two; an error, None outside,
/// when it cannot be read).
fn coedge_loop(
    file: &AsmFile,
    body: usize,
    first: usize,
    guard: &mut usize,
) -> Option<Option<Vec<[f64; 3]>>> {
    let options = crate::convert_options();
    let mut points = Vec::new();
    let mut open = false;
    let mut coedge = Some(first);
    while let Some(c) = coedge {
        *guard += 1;
        if *guard > 100_000 {
            return None;
        }
        let edge = ptr(file, c, 6)?;
        let reversed = matches!(file.fields(c).get(7), Some(Token::True));
        let (curve, t, transform) = convert::edge_curve(file, body, edge, &options, None).ok()?;
        let n = if matches!(curve, mitcad_asm::brep::Curve::Line { .. }) {
            1
        } else {
            720
        };
        let mut s: Vec<[f64; 3]> = (0..=n)
            .map(|k| curve.eval(t[0] + (t[1] - t[0]) * k as f64 / n as f64))
            .map(|p| match &transform {
                Some(tr) => [
                    tr.m[0][0] * p[0] + tr.m[0][1] * p[1] + tr.m[0][2] * p[2] + tr.t[0],
                    tr.m[1][0] * p[0] + tr.m[1][1] * p[1] + tr.m[1][2] * p[2] + tr.t[1],
                    tr.m[2][0] * p[0] + tr.m[2][1] * p[1] + tr.m[2][2] * p[2] + tr.t[2],
                ],
                None => p,
            })
            .collect();
        if reversed {
            s.reverse();
        }
        let end = s.pop();
        points.extend(s);
        let next = ptr(file, c, 3);
        // An open wire (a path) keeps its end: its last coedge has no next
        // one, or itself.
        if next.is_none() || next == Some(c) {
            open = true;
            points.extend(end);
        }
        coedge = next.filter(|&n| n != first && n != c);
    }
    Some((points.len() >= if open { 2 } else { 3 }).then_some(points))
}

/// The signed area and centroid of a closed polygon.
fn shoelace(p: &[[f64; 2]]) -> (f64, [f64; 2]) {
    let n = p.len();
    let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (u, v) = (p[i], p[(i + 1) % n]);
        let cross = u[0] * v[1] - v[0] * u[1];
        a += cross;
        cx += (u[0] + v[0]) * cross;
        cy += (u[1] + v[1]) * cross;
    }
    a /= 2.0;
    if a.abs() < 1e-300 {
        return (0.0, [0.0, 0.0]);
    }
    (a, [cx / (6.0 * a), cy / (6.0 * a)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_a_profile_kept_as_a_face() {
        for major in [21, 24] {
            let mut w = crate::testdesign::DesignWriter::new(major);
            let selection = w.selection(&crate::testdesign::square_sheet(), 4);
            let dc = Definitions::from_records(major, &w.records);
            let mut profiles = Profiles::default();
            let loops = profiles.loops(&dc, selection).expect("the face's loop");
            assert_eq!(loops.len(), 1);
            assert_eq!(loops[0].len(), 4);
            let identity = [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ];
            let m = profiles
                .measure(&dc, selection, &identity)
                .expect("measured");
            assert!((m.area - 1.0).abs() < 1e-9, "{m:?}");
            assert!((m.centroid[0] - 0.5).abs() < 1e-9 && (m.centroid[1] - 0.5).abs() < 1e-9);
        }
    }

    /// mitcad#933: a lump, shell, face or loop whose `next` is its own
    /// record made the walk endless; now the walk gives up (None) after as
    /// many steps as the file has records.
    #[test]
    fn a_chain_that_comes_back_ends_the_walk() {
        use crate::testdesign::square_sheet_with_next;
        for (kind, next) in [("lump", 2), ("shell", 3), ("face", 4), ("loop", 5)] {
            let asm = square_sheet_with_next(0.0, kind, next);
            let mut w = crate::testdesign::DesignWriter::new(24);
            let selection = w.selection(&asm, 4);
            let dc = Definitions::from_records(24, &w.records);
            let mut profiles = Profiles::default();
            // The first call parses the ASM file; the walk alone is timed
            // (the best of three, so one preemption does not count).
            assert_eq!(profiles.loops(&dc, selection), None, "{kind}");
            let fastest = (0..3)
                .map(|_| {
                    let start = std::time::Instant::now();
                    assert_eq!(profiles.loops(&dc, selection), None, "{kind}");
                    start.elapsed()
                })
                .min()
                .unwrap_or_default();
            assert!(
                fastest < std::time::Duration::from_millis(10),
                "{kind}: {fastest:?}"
            );
            let file = AsmFile::parse(&asm).expect("parsed");
            assert_eq!(wire_edges(&file, 1), None, "{kind}");
        }
        // The same sheet without the loop still has its loop and edges.
        let asm = square_sheet_with_next(0.0, "", -1);
        let file = AsmFile::parse(&asm).expect("parsed");
        assert_eq!(wire_loops(&file, 1).map(|l| l.len()), Some(1));
        assert_eq!(wire_edges(&file, 1).map(|e| e.len()), Some(4));
    }

    /// A selection of these loops, measured as `Profiles::measure` does.
    fn selection(loops: &[Vec<[f64; 2]>]) -> Measured {
        let mut parts: Vec<(f64, [f64; 2], Vec<[f64; 2]>)> = loops
            .iter()
            .map(|l| {
                let (a, c) = shoelace(l);
                (a.abs(), c, l.clone())
            })
            .collect();
        parts.sort_by(|a, b| b.0.total_cmp(&a.0));
        let area = parts[0].0 - parts[1..].iter().map(|p| p.0).sum::<f64>();
        let mut m = [parts[0].1[0] * parts[0].0, parts[0].1[1] * parts[0].0];
        for p in &parts[1..] {
            m[0] -= p.1[0] * p.0;
            m[1] -= p.1[1] * p.0;
        }
        Measured {
            area,
            centroid: [m[0] / area, m[1] / area],
            outer: parts[0].2.clone(),
            loops: parts.into_iter().map(|p| p.2).collect(),
        }
    }

    /// A square about the origin, sampled along its sides.
    fn square(half: f64, start: usize) -> Vec<[f64; 2]> {
        let corners = [[-half, -half], [half, -half], [half, half], [-half, half]];
        let mut out = Vec::new();
        for i in 0..4 {
            let (a, b) = (corners[(i + start) % 4], corners[(i + start + 1) % 4]);
            for k in 0..4 {
                let t = f64::from(k) / 4.0;
                out.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
            }
        }
        out
    }

    #[test]
    fn selections_inside_one_another_follow_the_even_odd_rule() {
        let areas = |v: &[Measured]| -> Vec<f64> {
            let mut a: Vec<f64> = v.iter().map(|m| (m.area * 1e9).round() / 1e9).collect();
            a.sort_by(f64::total_cmp);
            a
        };
        // An outline and a hole in it, each a selection of one loop.
        let plate = [selection(&[square(2.0, 0)]), selection(&[square(0.5, 0)])];
        assert!(nested(&plate));
        let r = even_odd(&plate).unwrap();
        assert_eq!(areas(&r), [15.0]);
        assert_eq!(r[0].loops.len(), 2);
        assert!(r[0].centroid.iter().all(|c| c.abs() < 1e-12));
        // A ring that keeps the hole and the hole's own selection: both.
        let filled = [
            selection(&[square(2.0, 0), square(0.5, 0)]),
            selection(&[square(0.5, 0)]),
        ];
        assert_eq!(areas(&even_odd(&filled).unwrap()), [1.0, 15.0]);
        // Four loops, the middle one twice (from other starts): the ring
        // between the outer and the inner loop, in the regions of the
        // loops between.
        let rings = [
            selection(&[square(3.0, 0)]),
            selection(&[square(2.0, 0)]),
            selection(&[square(2.0, 2)]),
            selection(&[square(1.0, 0)]),
        ];
        assert_eq!(areas(&even_odd(&rings).unwrap()), [12.0, 20.0]);
    }

    #[test]
    fn a_selection_along_the_outline_inside_it_adds_nothing() {
        // An outline and the ring between it and an inner outline that
        // touches its bottom side, both selected: the outline's region.
        let outline = selection(&[vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]]);
        let ring = selection(&[vec![
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 9.0],
            [9.0, 9.0],
            [9.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [0.0, 10.0],
        ]]);
        assert!((ring.area - 28.0).abs() < 1e-9);
        assert!(subsumed(&outline, &ring));
        assert!(!subsumed(&ring, &outline));
        // By the even-odd rule the ring would be a hole of the outline.
        let both = [outline.clone(), ring];
        assert!(nested(&both));
        let odd = even_odd(&both).unwrap();
        assert_eq!(odd.len(), 1);
        assert!((odd[0].area - 72.0).abs() < 1e-9);
        // A hole inside the outline, apart from its boundary, stays a
        // hole; a ring's hole selected again lies in no part of the ring.
        let hole = selection(&[square(0.5, 0)]);
        assert!(!subsumed(&selection(&[square(2.0, 0)]), &hole));
        assert!(!subsumed(
            &selection(&[square(2.0, 0), square(0.5, 0)]),
            &hole
        ));
        // A region beside the outline, outside it.
        let beside = selection(&[vec![[10.0, 0.0], [12.0, 0.0], [12.0, 5.0], [10.0, 5.0]]]);
        assert!(!subsumed(&outline, &beside));
    }

    #[test]
    fn outline_edges_are_the_curves_pieces() {
        use mitcad_asm::brep::{BSplineCurve, Curve};
        let flat = |p: [f64; 3]| Some([p[0], p[1]]);
        // A quarter and a bit of an ellipse, as rational quadratic pieces.
        let ellipse = Curve::Ellipse {
            center: [1.0, 2.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            major_dir: [1.0, 0.0, 0.0],
            major: 3.0,
            minor: 2.0,
        };
        let e = outline_edge(&ellipse, [0.3, 2.2], &flat).unwrap();
        let OutlineEdge::Nurbs { degree, poles, .. } = &e else {
            panic!("{e:?}")
        };
        assert_eq!((*degree, poles.len()), (2, 5));
        for k in 0..=20 {
            let p = e.at(f64::from(k) / 20.0);
            let (u, v) = ((p[0] - 1.0) / 3.0, (p[1] - 2.0) / 2.0);
            assert!((u * u + v * v - 1.0).abs() < 1e-12, "{p:?}");
        }
        let (a, b) = (e.at(0.0), e.at(1.0));
        assert!((a[0] - (1.0 + 3.0 * 0.3f64.cos())).abs() < 1e-12);
        assert!((b[1] - (2.0 + 2.0 * 2.2f64.sin())).abs() < 1e-12);
        // A circle's arc as an arc, either way round in the sketch (here the
        // circle's normal against the sketch's: clockwise).
        let circle = Curve::Ellipse {
            center: [1.0, 2.0, 0.0],
            normal: [0.0, 0.0, -1.0],
            major_dir: [1.0, 0.0, 0.0],
            major: 2.0,
            minor: 2.0,
        };
        let e = outline_edge(&circle, [0.0, 1.0], &flat).unwrap();
        let OutlineEdge::Arc {
            center,
            radius,
            start,
            sweep,
        } = e
        else {
            panic!("{e:?}")
        };
        assert_eq!((center, radius, start), ([1.0, 2.0], 2.0, 0.0));
        assert!((sweep + 1.0).abs() < 1e-12);
        for k in 0..=4 {
            let t = f64::from(k) / 4.0;
            let (p, q) = (e.at(t), circle.eval(t));
            assert!((p[0] - q[0]).hypot(p[1] - q[1]) < 1e-12, "{p:?} {q:?}");
        }
        // A rational cubic B-spline's piece between two interior
        // parameters, one of them a knot: the same points.
        let b = BSplineCurve {
            degree: 3,
            knots: vec![0.0, 1.0, 2.0, 3.0],
            mults: vec![4, 1, 1, 4],
            poles: vec![
                [0.0, 0.0, 0.0],
                [1.0, 2.0, 0.0],
                [2.0, -1.0, 0.0],
                [3.0, 3.0, 0.0],
                [4.0, 0.0, 0.0],
                [5.0, 1.0, 0.0],
            ],
            weights: Some(vec![1.0, 2.0, 0.5, 1.0, 1.5, 1.0]),
            periodic: false,
        };
        let whole = outline_edge(&Curve::BSpline(b.clone()), [0.0, 3.0], &flat).unwrap();
        let piece = outline_edge(&Curve::BSpline(b), [0.4, 2.0], &flat).unwrap();
        let OutlineEdge::Nurbs { knots, poles, .. } = &piece else {
            panic!("{piece:?}")
        };
        assert_eq!(knots.len(), poles.len() + 4);
        for k in 0..=10 {
            let s = f64::from(k) / 10.0;
            let (p, q) = (piece.at(s), whole.at((0.4 + s * 1.6) / 3.0));
            assert!((p[0] - q[0]).hypot(p[1] - q[1]) < 1e-12, "{p:?} {q:?}");
        }
        // Lines; and nothing for a range beyond the curve.
        let line = Curve::Line {
            origin: [0.0, 0.0, 0.0],
            dir: [1.0, 1.0, 0.0],
        };
        assert_eq!(
            outline_edge(&line, [1.0, 2.0], &flat),
            Some(OutlineEdge::Line([1.0, 1.0], [2.0, 2.0]))
        );
        let curve = BSplineCurve {
            degree: 1,
            knots: vec![0.0, 1.0],
            mults: vec![2, 2],
            poles: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
            weights: None,
            periodic: false,
        };
        assert!(outline_edge(&Curve::BSpline(curve), [0.5, 1.5], &flat).is_none());
    }

    #[test]
    fn measures_polygons() {
        let square = [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]];
        let (a, c) = shoelace(&square);
        assert!((a - 4.0).abs() < 1e-12);
        assert!((c[0] - 1.0).abs() < 1e-12 && (c[1] - 1.0).abs() < 1e-12);
        let mut cw = square;
        cw.reverse();
        assert!((shoelace(&cw).0 + 4.0).abs() < 1e-12);
    }
}
