// SPDX-License-Identifier: MIT
//! The DXF drawing of a Flat Pattern (mitcad#210).
//!
//! The drawing is made from the evaluated 2D outline and bend lines of the
//! pattern (`flat.rs`), never from the developed solid: each physical cut
//! contour is written once, without front/back duplicates, and consecutive
//! pieces of one line or circle that the development split at chart seams
//! are joined again, so a straight edge across a bend is one `LINE`. The
//! frame is the pattern's: the fixed panel's material axes on the viewed
//! side (`reference_side: back` mirrors it), so the drawing does not move
//! when the body is edited.

use std::f64::consts::TAU;

use mitcad_vec3::vec2::distance as dist;

use super::flat::FlatBend;
use super::process::{OutputPreset, SheetProcess};
use super::{BoundedCurve2Ext, MaterialRegion};
use crate::expr::LengthUnit;
use crate::profile::SegmentGeometry;

/// The exterior contour of the material.
pub const CUT_OUTER: &str = "CUT_OUTER";
/// Holes and other inner contours.
pub const CUT_INNER: &str = "CUT_INNER";
/// Centre lines of bends whose inside is on the viewed side.
pub const BEND_UP: &str = "BEND_UP";
/// Centre lines of bends whose inside is on the other side.
pub const BEND_DOWN: &str = "BEND_DOWN";
/// Tangent lines at both ends of each bend allowance (optional).
pub const BEND_TANGENT: &str = "BEND_TANGENT";

/// What a Flat Pattern drawing holds and how it is written.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlatDxfOptions {
    /// The drawing unit (mitcad#1408: the export gives the design's unless
    /// it names another).
    pub unit: LengthUnit,
    /// Also the tangent lines of the bends.
    pub tangents: bool,
    /// The sheet's process profile, whose output preset names the layers
    /// (mitcad#1939): metal's as they always were.
    pub process: SheetProcess,
}

impl Default for FlatDxfOptions {
    fn default() -> Self {
        Self {
            unit: LengthUnit::Millimetre,
            tangents: false,
            process: SheetProcess::Metal,
        }
    }
}

/// Distance below which two points of the drawing are one, in mm.
const JOIN: f64 = 1e-6;

fn same_angle(a: f64, b: f64) -> bool {
    let d = (a - b).rem_euclid(TAU);
    d < 1e-9 || TAU - d < 1e-9
}

/// One line or arc made of two consecutive pieces of the same line or
/// circle, or `None` when they are not.
fn join(a: &SegmentGeometry, b: &SegmentGeometry) -> Option<SegmentGeometry> {
    match (a, b) {
        (
            SegmentGeometry::Line { start: a0, end: a1 },
            SegmentGeometry::Line { start: b0, end: b1 },
        ) => {
            // The ends that meet, and the two that stay.
            let (far_a, far_b) = [
                (*a0, *b0, *a1, *b1),
                (*a0, *b1, *a1, *b0),
                (*a1, *b0, *a0, *b1),
                (*a1, *b1, *a0, *b0),
            ]
            .into_iter()
            .find(|(shared_a, shared_b, _, _)| dist(*shared_a, *shared_b) <= JOIN)
            .map(|(_, _, far_a, far_b)| (far_a, far_b))?;
            let u = [a1[0] - a0[0], a1[1] - a0[1]];
            let v = [b1[0] - b0[0], b1[1] - b0[1]];
            let (lu, lv) = (u[0].hypot(u[1]), v[0].hypot(v[1]));
            if lu <= JOIN || lv <= JOIN || (u[0] * v[1] - u[1] * v[0]).abs() > 1e-9 * lu * lv {
                return None;
            }
            // Collinear and meeting end to end, not folding back.
            let whole = dist(far_a, far_b);
            ((whole - (lu + lv)).abs() <= JOIN).then_some(SegmentGeometry::Line {
                start: far_a,
                end: far_b,
            })
        }
        (
            SegmentGeometry::Arc {
                center: c,
                radius: r,
                start_angle: s1,
                end_angle: e1,
            },
            SegmentGeometry::Arc {
                center: c2,
                radius: r2,
                start_angle: s2,
                end_angle: e2,
            },
        ) => {
            if dist(*c, *c2) > JOIN || (r - r2).abs() > JOIN {
                return None;
            }
            let span = (e1 - s1) + (e2 - s2);
            let start = if same_angle(*e1, *s2) {
                *s1
            } else if same_angle(*e2, *s1) {
                *s2
            } else {
                return None;
            };
            Some(if (span - TAU).abs() < 1e-9 {
                SegmentGeometry::Circle {
                    center: *c,
                    radius: *r,
                    start: 0.0,
                }
            } else {
                SegmentGeometry::Arc {
                    center: *c,
                    radius: *r,
                    start_angle: start,
                    end_angle: start + span,
                }
            })
        }
        _ => None,
    }
}

/// The pieces of a closed loop with consecutive pieces of one line or
/// circle joined, the last with the first too.
pub(crate) fn joined(pieces: Vec<SegmentGeometry>) -> Vec<SegmentGeometry> {
    let mut out: Vec<SegmentGeometry> = Vec::with_capacity(pieces.len());
    for piece in pieces {
        match out.last().and_then(|last| join(last, &piece)) {
            Some(both) => *out.last_mut().expect("a last piece") = both,
            None => out.push(piece),
        }
    }
    while out.len() > 1 {
        let Some(both) = join(out.last().expect("a last piece"), &out[0]) else {
            break;
        };
        out[0] = both;
        out.pop();
    }
    out
}

/// The drawing of a valid Flat Pattern (its `regions` and `bends`), in the
/// chosen unit.
pub fn drawing(
    regions: &[MaterialRegion],
    bends: &[FlatBend],
    options: &FlatDxfOptions,
) -> Result<mitcad_dxf::Drawing, String> {
    use mitcad_dxf::Layer;

    let preset = OutputPreset::of(options.process);
    let mut curves: Vec<(String, String, SegmentGeometry)> = Vec::new();
    for (r, region) in regions.iter().enumerate() {
        for (l, contour) in region.loops.iter().enumerate() {
            let pieces = contour
                .edges
                .iter()
                .map(|e| e.curve.geometry_piece())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("contour {l} of region {r}: {e}"))?;
            let layer = if l == 0 {
                &preset.cut_outer
            } else {
                &preset.cut_inner
            };
            for (i, piece) in joined(pieces).into_iter().enumerate() {
                curves.push((layer.clone(), format!("contour {l} piece {i}"), piece));
            }
        }
    }
    for bend in bends {
        let layer = if bend.direction == "up" {
            &preset.bend_up
        } else {
            &preset.bend_down
        };
        let name = format!("bend {} slot {}", bend.bend.feature, bend.bend.slot);
        // A kerf bend (mitcad#1939) has no centre line to mark: its slits
        // are cut, on the inner cut layer.
        let lines = bend
            .centerlines
            .iter()
            .filter(|_| bend.slits.is_empty())
            .map(|line| (layer.as_str(), line));
        let tangents = bend
            .tangents
            .iter()
            .filter(|_| options.tangents)
            .map(|line| (BEND_TANGENT, line));
        let slits = bend
            .slits
            .iter()
            .map(|line| (preset.cut_inner.as_str(), line));
        for (layer, line) in lines.chain(tangents).chain(slits) {
            curves.push((
                layer.to_owned(),
                name.clone(),
                SegmentGeometry::Line {
                    start: line.start,
                    end: line.end,
                },
            ));
        }
    }
    let mut drawing = crate::exchange::sketch_drawing(
        curves
            .iter()
            .map(|(layer, name, geometry)| (layer.as_str(), name.as_str(), geometry)),
    )?;
    let mut names: Vec<&str> = Vec::new();
    for name in [
        preset.cut_outer.as_str(),
        preset.cut_inner.as_str(),
        preset.bend_up.as_str(),
        preset.bend_down.as_str(),
    ] {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    if options.tangents {
        names.push(BEND_TANGENT);
    }
    drawing.layers = names
        .into_iter()
        .map(|name| Layer {
            color: preset.colour(name),
            ..Layer::new(name)
        })
        .collect();
    Ok(crate::exchange::dxf_in_unit(drawing, options.unit))
}

/// A piece of a Flat Pattern's outline as a cut sheet takes it
/// (mitcad#1980), in flat coordinates, mm.
#[derive(Debug, Clone, PartialEq)]
pub enum FlatPiece {
    Line([f64; 2], [f64; 2]),
    /// Counter-clockwise from `start` (radians) by `sweep`; a whole circle
    /// sweeps 2 pi.
    Arc {
        center: [f64; 2],
        radius: f64,
        start: f64,
        sweep: f64,
    },
    /// A curve that is no line or circle (an ellipse, a B-spline), as
    /// points along it.
    Polyline(Vec<[f64; 2]>),
}

/// A closed loop of a Flat Pattern's outline: the exterior of a region
/// (`outer`) or one of its holes. Its pieces are in the outline's order;
/// the cut sheet puts them end to end and turns the loop as it needs.
#[derive(Debug, Clone, PartialEq)]
pub struct FlatLoop {
    pub outer: bool,
    pub pieces: Vec<FlatPiece>,
}

/// How many points a curve that is no line or circle gets per mm of its
/// chord, at least [`MIN_SAMPLES`] and at most [`MAX_SAMPLES`].
const SAMPLES_PER_MM: f64 = 2.0;
const MIN_SAMPLES: usize = 16;
const MAX_SAMPLES: usize = 512;

/// The outline of a valid Flat Pattern's `regions` as a cut sheet takes
/// it (mitcad#1980): each region's exterior and holes, consecutive pieces
/// of one line or circle joined as the DXF drawing joins them; a curve
/// that is no line or circle as points along it.
pub fn outline_loops(regions: &[MaterialRegion]) -> Result<Vec<FlatLoop>, String> {
    let mut out = Vec::new();
    for (r, region) in regions.iter().enumerate() {
        for (l, contour) in region.loops.iter().enumerate() {
            let mut exact: Vec<SegmentGeometry> = Vec::new();
            let mut curved = false;
            let mut pieces: Vec<FlatPiece> = Vec::new();
            for edge in &contour.edges {
                let piece = edge
                    .curve
                    .geometry_piece()
                    .map_err(|e| format!("contour {l} of region {r}: {e}"))?;
                match piece {
                    SegmentGeometry::Line { .. }
                    | SegmentGeometry::Arc { .. }
                    | SegmentGeometry::Circle { .. } => {
                        exact.push(piece.clone());
                        pieces.push(flat_piece(&piece).expect("a line or an arc"));
                    }
                    _ => {
                        curved = true;
                        let (t0, t1) = (edge.curve.range.first, edge.curve.range.last);
                        let (a, b) = edge
                            .curve
                            .ends()
                            .map_err(|e| format!("contour {l} of region {r}: {e}"))?;
                        let chord = dist(a, b).max(1.0);
                        let n = ((chord * SAMPLES_PER_MM) as usize).clamp(MIN_SAMPLES, MAX_SAMPLES);
                        let points = (0..=n)
                            .map(|i| edge.curve.point(t0 + (t1 - t0) * i as f64 / n as f64))
                            .collect::<Result<Vec<_>, _>>()
                            .map_err(|e| format!("contour {l} of region {r}: {e}"))?;
                        pieces.push(FlatPiece::Polyline(points));
                    }
                }
            }
            // Lines and arcs split at chart seams joined again, as in the
            // DXF drawing; a loop with other curves keeps its pieces.
            if !curved {
                pieces = joined(exact).iter().filter_map(flat_piece).collect();
            }
            out.push(FlatLoop {
                outer: l == 0,
                pieces,
            });
        }
    }
    Ok(out)
}

/// A line, arc or circle as a piece of a cut sheet's outline.
fn flat_piece(geometry: &SegmentGeometry) -> Option<FlatPiece> {
    match geometry {
        SegmentGeometry::Line { start, end } => Some(FlatPiece::Line(*start, *end)),
        SegmentGeometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => {
            let mut sweep = (end_angle - start_angle).rem_euclid(TAU);
            if sweep < 1e-12 {
                sweep = TAU;
            }
            Some(FlatPiece::Arc {
                center: *center,
                radius: *radius,
                start: *start_angle,
                sweep,
            })
        }
        SegmentGeometry::Circle {
            center,
            radius,
            start,
        } => Some(FlatPiece::Arc {
            center: *center,
            radius: *radius,
            start: *start,
            sweep: TAU,
        }),
        _ => None,
    }
}

#[cfg(test)]
#[path = "flat_dxf_tests.rs"]
mod tests;
