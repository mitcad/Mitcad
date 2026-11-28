// SPDX-License-Identifier: MIT
//! Beads (mitcad#2645): stiffening ribs pressed into a flat panel of a sheet
//! along a center line of lines and arcs. This module holds what a sheet's
//! state keeps of a bead (in its panel's chart, so that it follows the panel
//! through unfolds and edits) and the bead's plane geometry: its section,
//! the outline it leaves on its face and the distances that keep it clear of
//! the panel's edges. `folded/bead.rs` builds and checks it on the sheet.
//!
//! **The section.** Across the center line, in the section plane (`s`
//! across, `z` along the press direction: into the material for a bead
//! pressed into its face, out of it for one standing out), the bead's face
//! (the panel surface the sketch lies on) leaves its plane at `s = ±half`,
//! runs down to the depth at the centre and back. Its corners are rounded:
//! every bend of the bead has the inside radius `radius`, so a corner whose
//! inside is on the face has that radius on the face and one whose outside
//! is on the face `radius + thickness`. The other surface is the face's
//! curve offset by the thickness towards the material: each line moved,
//! each arc concentric. The sheet therefore keeps its thickness everywhere
//! across the bead, and both surfaces leave their planes at the same `half`.
//!
//! | Profile | The face's curve before its corners are rounded |
//! |---|---|
//! | `round` | a circular arc through the lips `(±w/2, 0)` and `(0, d)`; at most half as deep as it is wide |
//! | `v` | the lips and the apex `(0, d)` |
//! | `u` | straight flanks normal to the face and a bottom of radius `w/2` on the face |
//! | `trapezoid` | flanks at `angle` to the face and a flat crown |
//!
//! **Along the center line** the section is swept along lines (a prism) and
//! turned about arcs' centres (a revolution). A round end is the half
//! section turned half a turn about the end; a corner between two lines is
//! the half section turned on the corner's outer side by the angle the line
//! turns; on its inner side the two legs overlap. A run-out end ends on a
//! straight ramp: along the center line the face rises from the face's
//! plane to the depth over `run_out` (the virtual sharps), with the bead's
//! radius at its foot and at the crown, and the section there is the lower
//! of the two.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use serde::{Deserialize, Serialize};

use super::{Isometry2, PanelId, SheetReferenceSide};
use crate::profile::SegmentGeometry;

/// The shape of a bead's section.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeadProfile {
    /// A circular arc across.
    #[default]
    Round,
    /// Straight flanks and a round bottom.
    U,
    /// Two flanks meeting at the bottom.
    V,
    /// Slanted flanks and a flat crown.
    Trapezoid,
}

/// How a bead ends where its center line ends.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeadEnd {
    /// The section turned half a turn about the end.
    #[default]
    Round,
    /// A straight ramp from the face up to the crown.
    RunOut,
}

/// Which way a bead is pressed, seen from the face its sketch lies on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeadDirection {
    /// The face stands out: a rib on this side, a groove on the other.
    #[default]
    Out,
    /// The face is pressed in: a groove on this side, a rib on the other.
    In,
}

/// A bead's resolved sizes, mm and radians.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BeadSection {
    pub profile: BeadProfile,
    /// Across, between the lips' virtual sharps on the face.
    pub width: f64,
    /// From the face's plane to the face at the centre (its virtual sharp).
    pub depth: f64,
    /// The inside radius of the bead's bends.
    pub radius: f64,
    /// A trapezoid's flank angle to the face.
    pub angle: f64,
    pub direction: BeadDirection,
    pub ends: BeadEnd,
    /// A run-out's length along the center line, between the virtual sharps
    /// at its foot and at the crown.
    pub run_out: f64,
    pub thickness: f64,
}

/// A piece of a bead's center line in its panel's chart.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BeadElement {
    Line {
        start: [f64; 2],
        end: [f64; 2],
    },
    /// About `center` from the angle `start` by `sweep` (radians, positive
    /// counter-clockwise in the chart); a whole circle sweeps a full turn.
    Arc {
        center: [f64; 2],
        radius: f64,
        start: f64,
        sweep: f64,
    },
}

/// A center line: its pieces joined end to end, in the order it runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeadChain {
    pub elements: Vec<BeadElement>,
    /// It closes on itself: no ends.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub closed: bool,
}

/// A bead as the sheet keeps it ([`super::SheetOverlayKind::Bead`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetBead {
    /// The flat panel it is pressed into.
    pub panel: PanelId,
    /// The panel's surface its center lines lie on (its face).
    pub side: SheetReferenceSide,
    pub chains: Vec<BeadChain>,
    pub section: BeadSection,
}

/// Turns smaller than this (radians) are no corner.
pub const SMOOTH: f64 = 1e-6;
/// Points this close (mm) are one.
const NEAR: f64 = 1e-6;

pub(crate) fn sub2(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
pub(crate) fn add2(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}
pub(crate) fn scale2(a: [f64; 2], k: f64) -> [f64; 2] {
    [a[0] * k, a[1] * k]
}
pub(crate) fn dot2(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
pub(crate) fn cross2(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
pub(crate) fn len2(a: [f64; 2]) -> f64 {
    a[0].hypot(a[1])
}
pub(crate) fn unit2(a: [f64; 2]) -> [f64; 2] {
    let l = len2(a);
    if l > 0.0 { scale2(a, 1.0 / l) } else { a }
}
/// The vector a quarter turn counter-clockwise.
pub(crate) fn left2(a: [f64; 2]) -> [f64; 2] {
    [-a[1], a[0]]
}
fn polar(c: [f64; 2], r: f64, angle: f64) -> [f64; 2] {
    [c[0] + r * angle.cos(), c[1] + r * angle.sin()]
}

/// A piece of a section curve, in the section plane `(s, z)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Piece {
    Line {
        a: [f64; 2],
        b: [f64; 2],
    },
    /// From the angle `start` by `sweep` (signed: negative clockwise).
    Arc {
        c: [f64; 2],
        r: f64,
        start: f64,
        sweep: f64,
    },
}

impl Piece {
    pub(crate) fn start(&self) -> [f64; 2] {
        match *self {
            Piece::Line { a, .. } => a,
            Piece::Arc { c, r, start, .. } => polar(c, r, start),
        }
    }

    pub(crate) fn end(&self) -> [f64; 2] {
        match *self {
            Piece::Line { b, .. } => b,
            Piece::Arc { c, r, start, sweep } => polar(c, r, start + sweep),
        }
    }

    #[cfg(test)]
    pub(crate) fn length(&self) -> f64 {
        match *self {
            Piece::Line { a, b } => len2(sub2(b, a)),
            Piece::Arc { r, sweep, .. } => r * sweep.abs(),
        }
    }

    /// The piece moved `t` to the side `side` of its run (+1 left, -1
    /// right): a line parallel, an arc concentric.
    fn offset(&self, t: f64, side: f64) -> Piece {
        match *self {
            Piece::Line { a, b } => {
                let n = scale2(left2(unit2(sub2(b, a))), side * t);
                Piece::Line {
                    a: add2(a, n),
                    b: add2(b, n),
                }
            }
            // A counter-clockwise arc has its centre on its left.
            Piece::Arc { c, r, start, sweep } => Piece::Arc {
                c,
                r: r - side * sweep.signum() * t,
                start,
                sweep,
            },
        }
    }

    /// The curve as the kernel takes it (arcs counter-clockwise).
    pub(crate) fn geometry(&self) -> SegmentGeometry {
        match *self {
            Piece::Line { a, b } => SegmentGeometry::Line { start: a, end: b },
            Piece::Arc { c, r, start, sweep } => {
                let (from, to) = if sweep >= 0.0 {
                    (start, start + sweep)
                } else {
                    (start + sweep, start)
                };
                SegmentGeometry::Arc {
                    center: c,
                    radius: r,
                    start_angle: from,
                    end_angle: to,
                }
            }
        }
    }

    /// `∫∫ s dA` over the band between this piece of the face and its
    /// offset by `t` to the side `side` (the volumes of revolutions the
    /// CLI fixture expects).
    #[cfg(test)]
    pub(crate) fn band_moment(&self, t: f64, side: f64) -> f64 {
        match *self {
            Piece::Line { a, b } => {
                let l = len2(sub2(b, a));
                let n = scale2(left2(unit2(sub2(b, a))), side);
                t * l * (a[0] + b[0]) / 2.0 + n[0] * l * t * t / 2.0
            }
            Piece::Arc { c, r, start, sweep } => {
                let other = r - side * sweep.signum() * t;
                let (r1, r2) = (r.min(other), r.max(other));
                let (lo, hi) = (start.min(start + sweep), start.max(start + sweep));
                c[0] * (r2 * r2 - r1 * r1) / 2.0 * sweep.abs()
                    + (r2.powi(3) - r1.powi(3)) / 3.0 * (hi.sin() - lo.sin())
            }
        }
    }
}

/// A rounded corner: its tangent length, its round (none without a turn),
/// where the round starts and where it ends.
type Corner = (f64, Option<Piece>, [f64; 2], [f64; 2]);

/// A polyline's corners rounded: from where the round of its first corner
/// starts to where the round of its last one ends (the first and last
/// vertices only give directions). `radius(corner, turn)` is the radius of
/// a corner (its index in `vertices`) turning left (+1) or right (-1).
fn rounded(
    vertices: &[[f64; 2]],
    radius: &dyn Fn(usize, f64) -> f64,
) -> Result<Vec<Piece>, String> {
    let n = vertices.len();
    // Each corner: the tangent length and its round (None: no turn).
    let mut corners: Vec<Corner> = Vec::new();
    for i in 1..n - 1 {
        let u1 = unit2(sub2(vertices[i], vertices[i - 1]));
        let u2 = unit2(sub2(vertices[i + 1], vertices[i]));
        let turn = cross2(u1, u2).atan2(dot2(u1, u2));
        if turn.abs() < 1e-12 {
            corners.push((0.0, None, vertices[i], vertices[i]));
            continue;
        }
        let sign = turn.signum();
        let r = radius(i, sign);
        let tangent = r * (turn.abs() / 2.0).tan();
        let from = sub2(vertices[i], scale2(u1, tangent));
        let to = add2(vertices[i], scale2(u2, tangent));
        let c = add2(from, scale2(left2(u1), sign * r));
        let start = (from[1] - c[1]).atan2(from[0] - c[0]);
        corners.push((
            tangent,
            Some(Piece::Arc {
                c,
                r,
                start,
                sweep: turn,
            }),
            from,
            to,
        ));
    }
    // The straight pieces between rounds must hold both tangent lengths.
    for i in 1..n.saturating_sub(2) {
        let length = len2(sub2(vertices[i + 1], vertices[i]));
        if corners[i - 1].0 + corners[i].0 > length + 1e-9 {
            return Err(TOO_TIGHT.into());
        }
    }
    let mut pieces = Vec::new();
    for (k, (_, round, _, to)) in corners.iter().enumerate() {
        pieces.extend(round);
        if let Some((_, _, from, _)) = corners.get(k + 1)
            && len2(sub2(*from, *to)) > 1e-12
        {
            pieces.push(Piece::Line { a: *to, b: *from });
        }
    }
    Ok(pieces)
}

const TOO_TIGHT: &str = "the bead's bends do not fit in its section: make it wider or deeper, or \
                         its bend radius smaller";

/// A bead's section: the face's curve and the other surface's, each whole
/// (from lip to lip) and as its right half (from the centre to the lip).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Section {
    pub face: Vec<Piece>,
    pub face_half: Vec<Piece>,
    pub other: Vec<Piece>,
    pub other_half: Vec<Piece>,
    /// Where both surfaces leave their planes, from the center line.
    pub half: f64,
    /// The other surface's plane: `+t` for a bead pressed in, `-t` for one
    /// standing out (`z` along the press direction from the face).
    pub other_level: f64,
    /// The material's side of the face's curve, travelling along `+s`:
    /// +1 left (`+z`), -1 right.
    pub material: f64,
    /// A run-out end's ramp along the center line, `x` from the end into the
    /// bead: the face's curve and the other's, from the foot of the ramp
    /// to where it reaches the crown; empty for round ends.
    pub ramp: Vec<Piece>,
    pub ramp_other: Vec<Piece>,
}

impl Section {
    /// How far past a run-out end the ramp's foot reaches: where the face
    /// leaves its plane (0 for round ends).
    pub(crate) fn foot(&self) -> f64 {
        self.ramp.first().map_or(0.0, |p| -p.start()[0])
    }

    /// Where the ramp reaches the crown, from the end.
    pub(crate) fn crown(&self) -> f64 {
        self.ramp.last().map_or(0.0, |p| p.end()[0])
    }

    /// How far from the center line the bead reaches on its face, around
    /// its ends too.
    pub(crate) fn reach(&self) -> f64 {
        self.half.max(self.foot())
    }
}

/// The right half of a symmetric curve from lip to lip: from where it
/// crosses `s = 0` to its end.
fn right_half(pieces: &[Piece]) -> Result<Vec<Piece>, String> {
    const ON_AXIS: f64 = 1e-9;
    for (i, piece) in pieces.iter().enumerate() {
        let (a, b) = (piece.start(), piece.end());
        // Two rounds meeting on the axis (a U's bottom): the half starts
        // with the second.
        if a[0].abs() <= ON_AXIS && b[0] > ON_AXIS {
            return Ok(pieces[i..].to_vec());
        }
        let crossing = match *piece {
            Piece::Line { a, b } if a[0] < -ON_AXIS && b[0] > ON_AXIS => {
                let k = -a[0] / (b[0] - a[0]);
                Some(Piece::Line {
                    a: [0.0, a[1] + k * (b[1] - a[1])],
                    b,
                })
            }
            Piece::Arc { c, r, start, sweep }
                if c[0].abs() < ON_AXIS && a[0] < -ON_AXIS && b[0] > ON_AXIS =>
            {
                // The angle where the arc is on the axis, within its sweep.
                let mut found = None;
                for axis in [FRAC_PI_2, -FRAC_PI_2, 3.0 * FRAC_PI_2, -3.0 * FRAC_PI_2] {
                    let along = (axis - start) / sweep;
                    if (0.0..=1.0).contains(&along) {
                        found = Some(Piece::Arc {
                            c,
                            r,
                            start: axis,
                            sweep: start + sweep - axis,
                        });
                        break;
                    }
                }
                found
            }
            _ => None,
        };
        if let Some(first) = crossing {
            let mut half = vec![first];
            half.extend_from_slice(&pieces[i + 1..]);
            return Ok(half);
        }
    }
    Err(UNMADE.into())
}

/// What went wrong where the bead's own construction cannot go on (a
/// section without a centre or a piece, a direction of no length).
pub(crate) const UNMADE: &str = "the bead's section could not be made";

impl BeadSection {
    /// The section, or what is wrong with the sizes, in words.
    pub(crate) fn section(&self) -> Result<Section, String> {
        let (w, d, r, t) = (self.width, self.depth, self.radius, self.thickness);
        if !(w.is_finite() && w > 0.0 && d.is_finite() && d > 0.0) {
            return Err("a bead needs a width and a depth above zero".into());
        }
        if !(r.is_finite() && r > 0.0) {
            return Err("a bead's bend radius must be above zero".into());
        }
        if !(t.is_finite() && t > 0.0) {
            return Err("a bead needs a sheet of positive thickness".into());
        }
        // The material lies left of the face's curve (+z) under a bead
        // pressed in, right of it under one standing out.
        let material = match self.direction {
            BeadDirection::In => 1.0,
            BeadDirection::Out => -1.0,
        };
        // A corner whose round turns towards the material has the face on
        // its outside.
        let bend = |turn: f64| if turn == material { r + t } else { r };
        let far = w + d + r + t + 10.0;
        let h = w / 2.0;
        let face = match self.profile {
            BeadProfile::Round => round_profile(w, d, bend(1.0))?,
            BeadProfile::V => rounded(
                &[
                    [-h - far, 0.0],
                    [-h, 0.0],
                    [0.0, d],
                    [h, 0.0],
                    [h + far, 0.0],
                ],
                &|_, turn| bend(turn),
            )?,
            BeadProfile::U => {
                let vertices = [
                    [-h - far, 0.0],
                    [-h, 0.0],
                    [-h, d],
                    [h, d],
                    [h, 0.0],
                    [h + far, 0.0],
                ];
                rounded(&vertices, &|corner, turn| {
                    if corner == 2 || corner == 3 {
                        h
                    } else {
                        bend(turn)
                    }
                })?
            }
            BeadProfile::Trapezoid => {
                let a = self.angle;
                if !(a.is_finite() && a > 1e-6 && a <= FRAC_PI_2 + 1e-12) {
                    return Err(
                        "a trapezoid bead's flank angle must be above 0 and at most 90 degrees"
                            .into(),
                    );
                }
                let run = d / a.tan();
                if h - run <= 1e-9 {
                    return Err(
                        "a trapezoid bead this deep needs a larger width or steeper \
                                flanks: its crown has no width"
                            .into(),
                    );
                }
                let vertices = [
                    [-h - far, 0.0],
                    [-h, 0.0],
                    [-h + run, d],
                    [h - run, d],
                    [h, 0.0],
                    [h + far, 0.0],
                ];
                rounded(&vertices, &|_, turn| bend(turn))?
            }
        };
        let other: Vec<Piece> = face.iter().map(|p| p.offset(t, material)).collect();
        for p in &other {
            if let Piece::Arc { r, .. } = p
                && *r <= 1e-9
            {
                return Err(
                    "the bead is too narrow for the sheet's thickness: its inner surface \
                     would fold over itself; make it wider or less deep"
                        .into(),
                );
            }
        }
        let half_width = face.last().ok_or(TOO_TIGHT)?.end()[0];
        let face_half = right_half(&face)?;
        let other_half: Vec<Piece> = face_half.iter().map(|p| p.offset(t, material)).collect();
        let (ramp, ramp_other) = match self.ends {
            BeadEnd::Round => (Vec::new(), Vec::new()),
            BeadEnd::RunOut => {
                let l = self.run_out;
                if !(l.is_finite() && l > 0.0) {
                    return Err("a bead's run-out must be longer than zero".into());
                }
                let ramp = rounded(
                    &[[-far, 0.0], [0.0, 0.0], [l, d], [l + far, d]],
                    &|_, turn| bend(turn),
                )?;
                let ramp_other = ramp.iter().map(|p| p.offset(t, material)).collect();
                (ramp, ramp_other)
            }
        };
        Ok(Section {
            face,
            face_half,
            other,
            other_half,
            half: half_width,
            other_level: material * t,
            material,
            ramp,
            ramp_other,
        })
    }
}

/// The round profile: the arc through `(±w/2, 0)` and `(0, d)` with the
/// lips rounded at `lip` on the face.
fn round_profile(w: f64, d: f64, lip: f64) -> Result<Vec<Piece>, String> {
    if d > w / 2.0 + 1e-12 {
        return Err(
            "a round bead is at most half as deep as it is wide: make it wider or less deep, \
             or choose the U profile"
                .into(),
        );
    }
    let big = (w * w / 4.0 + d * d) / (2.0 * d);
    let m = [0.0, d - big];
    // The lip's round touches the face's plane at (±f, 0) and the arc from
    // outside: its centre (±f, lip) is big + lip from the arc's.
    let f = ((big + lip).powi(2) - (lip - m[1]).powi(2)).sqrt();
    let left = [-f, lip];
    let right = [f, lip];
    let touch = |c: [f64; 2]| add2(m, scale2(unit2(sub2(c, m)), big));
    let (k_left, k_right) = (touch(left), touch(right));
    let angle = |p: [f64; 2], c: [f64; 2]| (p[1] - c[1]).atan2(p[0] - c[0]);
    let lip_in = angle(k_left, left) + FRAC_PI_2;
    let main_start = angle(k_left, m);
    let main_end = angle(k_right, m);
    let mut main_sweep = main_end - main_start;
    // Clockwise through the bottom (the angle pi/2 about the arc's centre).
    while main_sweep > 0.0 {
        main_sweep -= TAU;
    }
    let lip_out_start = angle(k_right, right);
    Ok(vec![
        Piece::Arc {
            c: left,
            r: lip,
            start: -FRAC_PI_2,
            sweep: lip_in.rem_euclid(TAU),
        },
        Piece::Arc {
            c: m,
            r: big,
            start: main_start,
            sweep: main_sweep,
        },
        Piece::Arc {
            c: right,
            r: lip,
            start: lip_out_start,
            sweep: (-FRAC_PI_2 - lip_out_start).rem_euclid(TAU),
        },
    ])
}

impl BeadElement {
    pub fn start(&self) -> [f64; 2] {
        match *self {
            BeadElement::Line { start, .. } => start,
            BeadElement::Arc {
                center,
                radius,
                start,
                ..
            } => polar(center, radius, start),
        }
    }

    pub fn end(&self) -> [f64; 2] {
        match *self {
            BeadElement::Line { end, .. } => end,
            BeadElement::Arc {
                center,
                radius,
                start,
                sweep,
            } => polar(center, radius, start + sweep),
        }
    }

    /// The unit direction it runs in at its start.
    pub fn start_tangent(&self) -> [f64; 2] {
        match *self {
            BeadElement::Line { start, end } => unit2(sub2(end, start)),
            BeadElement::Arc { start, sweep, .. } => {
                scale2([-start.sin(), start.cos()], sweep.signum())
            }
        }
    }

    pub fn end_tangent(&self) -> [f64; 2] {
        match *self {
            BeadElement::Line { start, end } => unit2(sub2(end, start)),
            BeadElement::Arc { start, sweep, .. } => {
                let a = start + sweep;
                scale2([-a.sin(), a.cos()], sweep.signum())
            }
        }
    }

    pub fn length(&self) -> f64 {
        match *self {
            BeadElement::Line { start, end } => len2(sub2(end, start)),
            BeadElement::Arc { radius, sweep, .. } => radius * sweep.abs(),
        }
    }

    /// The same piece run the other way.
    pub fn reversed(&self) -> Self {
        match *self {
            BeadElement::Line { start, end } => BeadElement::Line {
                start: end,
                end: start,
            },
            BeadElement::Arc {
                center,
                radius,
                start,
                sweep,
            } => BeadElement::Arc {
                center,
                radius,
                start: start + sweep,
                sweep: -sweep,
            },
        }
    }

    /// Points along it, at most `step` apart (arcs within a sag of 1e-4).
    pub(crate) fn points(&self, step: f64) -> Vec<[f64; 2]> {
        let by_step = (self.length() / step.max(1e-3)).ceil();
        let n = match *self {
            BeadElement::Line { .. } => by_step.clamp(1.0, 8192.0),
            BeadElement::Arc { radius, sweep, .. } => {
                let sag = 2.0 * (1.0 - (1e-4 / radius).min(1.0)).acos();
                let by_sag = (sweep.abs() / sag.max(1e-6)).ceil();
                by_sag.max(by_step).clamp(8.0, 8192.0)
            }
        } as usize;
        (0..=n)
            .map(|k| {
                let f = k as f64 / n as f64;
                match *self {
                    BeadElement::Line { start, end } => add2(start, scale2(sub2(end, start), f)),
                    BeadElement::Arc {
                        center,
                        radius,
                        start,
                        sweep,
                    } => polar(center, radius, start + f * sweep),
                }
            })
            .collect()
    }

    /// The curve as the kernel and the DXF take it.
    pub fn geometry(&self) -> SegmentGeometry {
        match *self {
            BeadElement::Line { start, end } => SegmentGeometry::Line { start, end },
            BeadElement::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                if sweep.abs() >= TAU - 1e-12 {
                    SegmentGeometry::Circle {
                        center,
                        radius,
                        start: 0.0,
                    }
                } else {
                    let (from, to) = if sweep >= 0.0 {
                        (start, start + sweep)
                    } else {
                        (start + sweep, start)
                    };
                    SegmentGeometry::Arc {
                        center,
                        radius,
                        start_angle: from,
                        end_angle: to,
                    }
                }
            }
        }
    }
}

/// How sharply a center line turns from `a` into `b` (radians, positive to
/// the left).
pub(crate) fn turn(a: &BeadElement, b: &BeadElement) -> f64 {
    let (u, v) = (a.end_tangent(), b.start_tangent());
    cross2(u, v).atan2(dot2(u, v))
}

/// Joins pieces of center lines end to end into chains. Pieces meeting at a
/// point with no third one; a piece's run follows the chain. Each chain
/// starts at its first piece in the given order (an open chain from its
/// free end nearer that piece's start); a lone circle is a closed chain.
pub(crate) fn chains(pieces: &[BeadElement]) -> Result<Vec<BeadChain>, String> {
    let closed_piece =
        |e: &BeadElement| matches!(e, BeadElement::Arc { sweep, .. } if sweep.abs() >= TAU - 1e-12);
    let near = |p: [f64; 2], q: [f64; 2]| len2(sub2(p, q)) <= NEAR;
    // How many piece ends meet at each piece end.
    let ends: Vec<[[f64; 2]; 2]> = pieces.iter().map(|e| [e.start(), e.end()]).collect();
    let meeting = |p: [f64; 2]| {
        ends.iter()
            .zip(pieces)
            .filter(|(_, e)| !closed_piece(e))
            .map(|(pair, _)| pair.iter().filter(|q| near(**q, p)).count())
            .sum::<usize>()
    };
    for (pair, e) in ends.iter().zip(pieces) {
        if closed_piece(e) {
            continue;
        }
        if len2(sub2(pair[0], pair[1])) <= NEAR && !matches!(e, BeadElement::Arc { .. }) {
            return Err("a line of the bead's center line has no length".into());
        }
        for p in pair {
            if meeting(*p) > 2 {
                return Err(
                    "the bead's center lines branch: more than two of its lines and arcs meet \
                     at a point"
                        .into(),
                );
            }
        }
    }
    let mut used = vec![false; pieces.len()];
    let mut out = Vec::new();
    for first in 0..pieces.len() {
        if used[first] {
            continue;
        }
        used[first] = true;
        if closed_piece(&pieces[first]) {
            out.push(BeadChain {
                elements: vec![pieces[first]],
                closed: true,
            });
            continue;
        }
        // Grow both ways from the first piece.
        let mut chain = std::collections::VecDeque::from([pieces[first]]);
        loop {
            let tail = chain.back().expect("a piece").end();
            let next = (0..pieces.len()).find(|&k| {
                !used[k]
                    && !closed_piece(&pieces[k])
                    && (near(ends[k][0], tail) || near(ends[k][1], tail))
            });
            let Some(k) = next else { break };
            used[k] = true;
            chain.push_back(if near(ends[k][0], tail) {
                pieces[k]
            } else {
                pieces[k].reversed()
            });
        }
        loop {
            let head = chain.front().expect("a piece").start();
            let next = (0..pieces.len()).find(|&k| {
                !used[k]
                    && !closed_piece(&pieces[k])
                    && (near(ends[k][0], head) || near(ends[k][1], head))
            });
            let Some(k) = next else { break };
            used[k] = true;
            chain.push_front(if near(ends[k][1], head) {
                pieces[k]
            } else {
                pieces[k].reversed()
            });
        }
        let elements: Vec<BeadElement> = chain.into_iter().collect();
        let closed = elements.len() > 1
            && near(
                elements.first().expect("a piece").start(),
                elements.last().expect("a piece").end(),
            );
        // A single arc closing on itself without being whole is still open.
        out.push(BeadChain { elements, closed });
    }
    Ok(out)
}

/// The shortest distance between the segments `ab` and `cd`.
pub(crate) fn segment_distance(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> f64 {
    let point_segment = |p: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        let ab = sub2(b, a);
        let l = dot2(ab, ab);
        let k = if l > 0.0 {
            (dot2(sub2(p, a), ab) / l).clamp(0.0, 1.0)
        } else {
            0.0
        };
        len2(sub2(add2(a, scale2(ab, k)), p))
    };
    // Crossing segments touch.
    let side = |p: [f64; 2], a: [f64; 2], b: [f64; 2]| cross2(sub2(b, a), sub2(p, a));
    let (d1, d2) = (side(c, a, b), side(d, a, b));
    let (d3, d4) = (side(a, c, d), side(b, c, d));
    if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
        return 0.0;
    }
    point_segment(a, c, d)
        .min(point_segment(b, c, d))
        .min(point_segment(c, a, b))
        .min(point_segment(d, a, b))
}

/// A polyline's segments.
pub(crate) fn segments(points: &[[f64; 2]]) -> impl Iterator<Item = ([f64; 2], [f64; 2])> + '_ {
    points.windows(2).map(|w| (w[0], w[1]))
}

impl SheetBead {
    /// Its center lines as polylines, points at most `step` apart.
    pub(crate) fn polylines(&self, step: f64) -> Vec<Vec<[f64; 2]>> {
        self.chains
            .iter()
            .flat_map(|c| c.elements.iter().map(|e| e.points(step)))
            .collect()
    }

    /// The pieces of its outline on its face and its center lines, in its
    /// panel's chart.
    pub(crate) fn outline(&self, section: &Section) -> Result<Vec<SegmentGeometry>, String> {
        let mut out = Vec::new();
        for chain in &self.chains {
            chain_outline(chain, section, &mut out)?;
        }
        Ok(out)
    }

    pub(crate) fn centre_lines(&self) -> Vec<SegmentGeometry> {
        self.chains
            .iter()
            .flat_map(|c| c.elements.iter().map(BeadElement::geometry))
            .collect()
    }
}

/// An offset of a center line piece: `side` +1 left, -1 right, `by` mm.
fn offset_element(e: &BeadElement, side: f64, by: f64) -> BeadElement {
    match *e {
        BeadElement::Line { start, end } => {
            let n = scale2(left2(unit2(sub2(end, start))), side * by);
            BeadElement::Line {
                start: add2(start, n),
                end: add2(end, n),
            }
        }
        BeadElement::Arc {
            center,
            radius,
            start,
            sweep,
        } => BeadElement::Arc {
            center,
            radius: radius - side * sweep.signum() * by,
            start,
            sweep,
        },
    }
}

/// Where the lines through `a` along `u` and through `b` along `v` meet:
/// the parameters along each.
fn line_meet(a: [f64; 2], u: [f64; 2], b: [f64; 2], v: [f64; 2]) -> Option<(f64, f64)> {
    let det = cross2(u, v);
    if det.abs() < 1e-12 {
        return None;
    }
    let w = sub2(b, a);
    Some((cross2(w, v) / det, cross2(w, u) / det))
}

fn chain_outline(
    chain: &BeadChain,
    section: &Section,
    out: &mut Vec<SegmentGeometry>,
) -> Result<(), String> {
    let half = section.half;
    let n = chain.elements.len();
    for side in [1.0, -1.0] {
        let mut sides: Vec<BeadElement> = chain
            .elements
            .iter()
            .map(|e| offset_element(e, side, half))
            .collect();
        let joints = if chain.closed { n } else { n - 1 };
        for j in 0..joints {
            let (a, b) = (j, (j + 1) % n);
            let bend = turn(&chain.elements[a], &chain.elements[b]);
            if bend.abs() < SMOOTH {
                continue;
            }
            let corner = chain.elements[a].end();
            if bend.signum() == side {
                // The inner side: the two offsets cut back to where they meet.
                let (
                    BeadElement::Line { start: s1, end: e1 },
                    BeadElement::Line { start: s2, end: e2 },
                ) = (sides[a], sides[b])
                else {
                    return Err(CORNER_ARCS.into());
                };
                let (u, v) = (unit2(sub2(e1, s1)), unit2(sub2(e2, s2)));
                let (k1, k2) = line_meet(s1, u, s2, v).ok_or(CORNER_ARCS)?;
                if k1 < -1e-9
                    || k1 > len2(sub2(e1, s1)) + 1e-9
                    || k2 < -1e-9
                    || k2 > len2(sub2(e2, s2)) + 1e-9
                {
                    return Err(SHORT_CORNER.into());
                }
                let meet = add2(s1, scale2(u, k1));
                sides[a] = BeadElement::Line {
                    start: s1,
                    end: meet,
                };
                sides[b] = BeadElement::Line {
                    start: meet,
                    end: e2,
                };
            } else {
                // The outer side: an arc about the corner.
                let from = sides[a].end();
                let start = (from[1] - corner[1]).atan2(from[0] - corner[0]);
                out.push(
                    BeadElement::Arc {
                        center: corner,
                        radius: half,
                        start,
                        sweep: bend,
                    }
                    .geometry(),
                );
            }
        }
        if !chain.closed {
            // A run-out's outline reaches past the end by the ramp's foot.
            let foot = section.foot();
            if foot > 0.0 {
                if let BeadElement::Line { start, end } = sides[0] {
                    let u = unit2(sub2(end, start));
                    sides[0] = BeadElement::Line {
                        start: sub2(start, scale2(u, foot)),
                        end,
                    };
                }
                if let BeadElement::Line { start, end } = sides[n - 1] {
                    let u = unit2(sub2(end, start));
                    sides[n - 1] = BeadElement::Line {
                        start,
                        end: add2(end, scale2(u, foot)),
                    };
                }
            }
        }
        out.extend(sides.iter().map(BeadElement::geometry));
    }
    if !chain.closed {
        let first = &chain.elements[0];
        let last = &chain.elements[n - 1];
        match section.foot() > 0.0 {
            // Run-out ends: a line across at the ramp's foot.
            true => {
                for (point, tangent, outward) in [
                    (first.start(), first.start_tangent(), -1.0),
                    (last.end(), last.end_tangent(), 1.0),
                ] {
                    let at = add2(point, scale2(tangent, outward * section.foot()));
                    let across = scale2(left2(tangent), half);
                    out.push(SegmentGeometry::Line {
                        start: add2(at, across),
                        end: sub2(at, across),
                    });
                }
            }
            // Round ends: half circles about the ends.
            false => {
                let angle = |v: [f64; 2]| v[1].atan2(v[0]);
                let end = last.end_tangent();
                out.push(
                    BeadElement::Arc {
                        center: last.end(),
                        radius: half,
                        start: angle(end) + FRAC_PI_2,
                        sweep: -PI,
                    }
                    .geometry(),
                );
                let start = first.start_tangent();
                out.push(
                    BeadElement::Arc {
                        center: first.start(),
                        radius: half,
                        start: angle(start) - FRAC_PI_2,
                        sweep: -PI,
                    }
                    .geometry(),
                );
            }
        }
    }
    Ok(())
}

/// A line, arc or circle moved by an isometry of the plane (a reflection
/// runs an arc the other way), for the Flat Pattern.
pub(crate) fn mapped_geometry(g: &SegmentGeometry, map: Isometry2) -> SegmentGeometry {
    let sign = map.x_axis[0] * map.y_axis[1] - map.x_axis[1] * map.y_axis[0];
    let rotation = map.x_axis[1].atan2(map.x_axis[0]);
    match *g {
        SegmentGeometry::Line { start, end } => SegmentGeometry::Line {
            start: map.point(start),
            end: map.point(end),
        },
        SegmentGeometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => {
            let (from, to) = if sign > 0.0 {
                (rotation + start_angle, rotation + end_angle)
            } else {
                (rotation - end_angle, rotation - start_angle)
            };
            SegmentGeometry::Arc {
                center: map.point(center),
                radius,
                start_angle: from,
                end_angle: to,
            }
        }
        SegmentGeometry::Circle {
            center,
            radius,
            start,
        } => SegmentGeometry::Circle {
            center: map.point(center),
            radius,
            start: rotation + sign.signum() * start,
        },
        ref other => other.clone(),
    }
}

pub(crate) const CORNER_ARCS: &str = "a bead's center line turns at a corner only where two lines \
                                      meet: join an arc to its neighbours without a corner";
pub(crate) const SHORT_CORNER: &str =
    "the lines at a corner of the bead's center line are too short for its width";

#[cfg(test)]
#[path = "bead_tests.rs"]
mod tests;
