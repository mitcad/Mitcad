// SPDX-License-Identifier: MIT
//! The profiles an extrusion or revolution selected, measured: their area
//! and centroid in their sketch's coordinates, so that the import picks the
//! same regions of Mitcad's sketch.
//!
//! A profile selection (`3b2477a4…`): the header, eight bytes, a reference
//! to an entity link (`21750fcc…`) and a byte. The entity link: u32, u16,
//! u32, a byte, a reference to an ASM record of the definitions segment (of
//! the B-rep record's type), a u32 id and a u32 index of a lump *(seen)*.
//! That ASM file keeps every selected profile as a wire body whose loops
//! are the region's boundary, one body per selection *(verified: as many
//! wire bodies as selections in the test files)*; its body record's second
//! field is the id.

use std::collections::HashMap;

use mitcad_f3d::asm::file::AsmFile;
use mitcad_f3d::asm::token::Token;
use mitcad_f3d::convert;

use crate::dc::{Definitions, type_id};

pub const SELECTION: [u8; 16] = type_id("3b2477a4d1118f96000826bd0663dc09");
pub const ENTITY_LINK: [u8; 16] = type_id("21750fccd1112780e38619962259017a");

/// A profile's area (cm²) and centroid (cm), in its sketch's coordinates,
/// and its outer boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct Measured {
    pub area: f64,
    pub centroid: [f64; 2],
    pub outer: Vec<[f64; 2]>,
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

/// Whether some selection lies inside another one's boundary. How the
/// file's selections then make up the sketch's regions (a hole of the
/// outer one, or a region of its own taken off it) is not settled: on the
/// test files both readings give other bodies than the history states, so
/// such sets are not measured.
pub fn nested(measured: &[Measured]) -> bool {
    measured.iter().enumerate().any(|(i, outer)| {
        measured
            .iter()
            .enumerate()
            .any(|(j, inner)| i != j && inner.area < outer.area && outer.contains(inner.centroid))
    })
}

/// The ASM files of the definitions segment, parsed when first needed.
#[derive(Default)]
pub struct Profiles {
    files: HashMap<usize, Option<AsmFile>>,
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
        if !dc.is(selection, &SELECTION) {
            return None;
        }
        let mut r = dc.body(selection);
        r.skip(8).ok()?;
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
fn wire_loops(file: &AsmFile, body: usize) -> Option<Vec<Vec<[f64; 3]>>> {
    let options = crate::convert_options();
    let mut out = Vec::new();
    let mut lump = ptr(file, body, 3);
    let mut guard = 0;
    while let Some(l) = lump {
        let mut shell = ptr(file, l, 4);
        while let Some(s) = shell {
            let mut wire = ptr(file, s, 6);
            while let Some(w) = wire {
                let first = ptr(file, w, 4)?;
                let mut points = Vec::new();
                let mut coedge = Some(first);
                while let Some(c) = coedge {
                    guard += 1;
                    if guard > 100_000 {
                        return None;
                    }
                    let edge = ptr(file, c, 6)?;
                    let reversed = matches!(file.fields(c).get(7), Some(Token::True));
                    let (curve, t, transform) =
                        convert::edge_curve(file, body, edge, &options, None).ok()?;
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
                    s.pop();
                    points.extend(s);
                    coedge = ptr(file, c, 3).filter(|&n| n != first);
                }
                if points.len() >= 3 {
                    out.push(points);
                }
                wire = ptr(file, w, 3);
            }
            shell = ptr(file, s, 3);
        }
        lump = ptr(file, l, 3);
    }
    (!out.is_empty()).then_some(out)
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
