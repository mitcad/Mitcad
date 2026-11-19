// SPDX-License-Identifier: MIT
//! QR codes in sketches (mitcad#473, `sketch.add_code`): a sketch text
//! whose `code` is set draws its content as a QR code (the `qrcode` crate)
//! instead of characters. Its dark modules are squares of `height` mm from
//! `at` (the lower left corner), turned by `angle`; modules that share a
//! side are one region, bounded by one contour along their outside and
//! one round each light island inside, so a code extrudes as a few
//! regions rather than hundreds of squares. Modules that touch only at a
//! corner stay apart there (their contours meet at that point).
//!
//! The contours are glyph contours of the text, `t<n>.g0.c<j>`, numbered
//! from the lowest, leftmost one up, as a text's outlines are: they bound
//! profiles, a contour round dark modules a region with its islands as
//! holes.

use std::collections::{BTreeMap, BTreeSet};

use qrcode::{Color, EcLevel, QrCode};

use super::geometry::{Curve2, P2};
use super::text::{PlacedContour, TextOutline};
use super::{CodeEcc, SketchCode, SketchText};
use crate::topo::CurveId;

/// The modules of a code: its width and whether each module is dark, row
/// by row from the top.
pub fn modules(content: &str, code: &SketchCode) -> Result<(usize, Vec<bool>), String> {
    let level = match code.ecc {
        CodeEcc::L => EcLevel::L,
        CodeEcc::M => EcLevel::M,
        CodeEcc::Q => EcLevel::Q,
        CodeEcc::H => EcLevel::H,
    };
    let qr = QrCode::with_error_correction_level(content.as_bytes(), level)
        .map_err(|e| format!("no QR code holds this content: {e}"))?;
    let dark = qr
        .to_colors()
        .into_iter()
        .map(|c| c == Color::Dark)
        .collect();
    Ok((qr.width(), dark))
}

/// The closed boundaries of the dark modules of a `width` × `width` grid
/// (row by row from the top) in grid points from the lower left corner,
/// y up: counter-clockwise round dark areas, clockwise round light islands
/// in them, corners only.
pub fn boundaries(width: usize, dark: &[bool]) -> Vec<Vec<[i64; 2]>> {
    let is_dark = |i: i64, j: i64| {
        i >= 0
            && j >= 0
            && (i as usize) < width
            && (j as usize) < width
            && dark[(width - 1 - j as usize) * width + i as usize]
    };
    // The sides of dark modules that light ones (or the outside) share,
    // each run with the dark module on its left.
    let mut edges: BTreeMap<[i64; 2], Vec<[i64; 2]>> = BTreeMap::new();
    let mut count = 0usize;
    let n = width as i64;
    for j in 0..n {
        for i in 0..n {
            if !is_dark(i, j) {
                continue;
            }
            let sides = [
                ((i, j - 1), [i, j], [i + 1, j]),
                ((i + 1, j), [i + 1, j], [i + 1, j + 1]),
                ((i, j + 1), [i + 1, j + 1], [i, j + 1]),
                ((i - 1, j), [i, j + 1], [i, j]),
            ];
            for ((ni, nj), from, to) in sides {
                if !is_dark(ni, nj) {
                    edges.entry(from).or_default().push(to);
                    count += 1;
                }
            }
        }
    }
    let mut loops = Vec::new();
    let mut used: BTreeSet<([i64; 2], [i64; 2])> = BTreeSet::new();
    let starts: Vec<([i64; 2], [i64; 2])> = edges
        .iter()
        .flat_map(|(from, tos)| tos.iter().map(move |to| (*from, *to)))
        .collect();
    for start in starts {
        if used.contains(&start) {
            continue;
        }
        let mut points = vec![start.0];
        let (mut from, mut to) = start;
        used.insert(start);
        while used.len() <= count {
            let direction = [to[0] - from[0], to[1] - from[1]];
            // Where two dark modules touch at a corner, the turn to the
            // left keeps to the module the side came along.
            let left = [-direction[1], direction[0]];
            let right = [direction[1], -direction[0]];
            let next = [left, direction, right]
                .into_iter()
                .map(|d| [to[0] + d[0], to[1] + d[1]])
                .find(|p| {
                    edges.get(&to).is_some_and(|tos| tos.contains(p))
                        && ((to, *p) == start || !used.contains(&(to, *p)))
                });
            let Some(next) = next else { break };
            if (to, next) == start {
                break;
            }
            points.push(to);
            used.insert((to, next));
            from = to;
            to = next;
        }
        // A boundary that comes back to a corner it passed (a light area
        // that meets the outside at a corner only) is two loops that touch
        // there: profile curves do not touch themselves.
        let mut stack: Vec<[i64; 2]> = Vec::new();
        for p in points {
            if let Some(i) = stack.iter().position(|q| *q == p) {
                loops.push(corners_of(&stack.split_off(i)));
            }
            stack.push(p);
        }
        loops.push(corners_of(&stack));
    }
    loops.retain(|l| l.len() >= 4);
    loops
}

/// The corners of a closed path along grid points.
fn corners_of(points: &[[i64; 2]]) -> Vec<[i64; 2]> {
    let k = points.len();
    (0..k)
        .filter(|&a| {
            let (p, q, r) = (points[(a + k - 1) % k], points[a], points[(a + 1) % k]);
            (q[0] - p[0]) * (r[1] - q[1]) - (q[1] - p[1]) * (r[0] - q[0]) != 0
        })
        .map(|a| points[a])
        .collect()
}

/// A code's outline: its boundaries placed in the sketch, as a text's
/// glyph contours.
pub fn outline(text: &SketchText, code: &SketchCode) -> Result<TextOutline, String> {
    let what = format!("t{}", text.id.0);
    let (width, dark) = modules(&text.text, code).map_err(|e| format!("{what}: {e}"))?;
    let (s, c) = text.angle.sin_cos();
    let m = text.height;
    let place = |p: [i64; 2]| -> P2 {
        let (x, y) = (p[0] as f64 * m, p[1] as f64 * m);
        [text.at[0] + c * x - s * y, text.at[1] + s * x + c * y]
    };
    let contours = boundaries(width, &dark)
        .into_iter()
        .enumerate()
        .map(|(j, corners)| {
            let k = corners.len();
            PlacedContour {
                id: CurveId::Glyph {
                    text: text.id,
                    glyph: 0,
                    contour: j as u32,
                },
                parts: (0..k)
                    .map(|a| Curve2::Line {
                        a: place(corners[a]),
                        b: place(corners[(a + 1) % k]),
                    })
                    .collect(),
            }
        })
        .collect();
    Ok(TextOutline {
        id: text.id,
        family: "QR code".to_owned(),
        fallback: false,
        stored: false,
        contours,
        dropped: Vec::new(),
        lines: 0,
    })
}
