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

use mitcad_f3d::asm::file::AsmFile;
use mitcad_f3d::asm::token::Token;
use mitcad_f3d::convert;

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
    let mut faces = 0;
    while let Some(l) = lump {
        let mut shell = ptr(file, l, 4);
        while let Some(s) = shell {
            let mut wire = ptr(file, s, 6);
            while let Some(w) = wire {
                if let Some(points) = coedge_loop(file, body, ptr(file, w, 4)?, &mut guard)? {
                    out.push(points);
                }
                wire = ptr(file, w, 3);
            }
            let mut face = ptr(file, s, 5);
            while let Some(f) = face {
                faces += 1;
                let mut lp = ptr(file, f, 4);
                while let Some(l) = lp {
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
        let n = if matches!(curve, mitcad_f3d::brep::Curve::Line { .. }) {
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
