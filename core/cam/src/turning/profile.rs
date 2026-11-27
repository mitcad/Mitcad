// SPDX-License-Identifier: MIT
//! The part's revolved profile and the contours the tools follow.
//!
//! A profile is a chain of vertices `[x, z]` (x a radius) with bulges, as
//! the milling outlines have them, drawn with Z to the right and X up: a
//! positive bulge turns counter-clockwise (G3). The outside chain runs
//! from the part's face towards the chuck; the inside chain (a bore) the
//! same way, from the face into the bore.
//!
//! A tool that cuts towards the chuck can reach only what it sees from the
//! face's side: the **envelope** fills grooves (left to grooving) and
//! takes a wall that leans back towards the face as a straight shoulder.
//! The **tool path** is the envelope offset by the nose radius, moved from
//! the nose's centre to the controlled point (the imaginary tip).

use cavalier_contours::core::math::Vector2;
use cavalier_contours::polyline::{
    PlineSource, PlineSourceMut, PlineVertex, Polyline, seg_arc_radius_and_center,
};
use serde::{Deserialize, Serialize};

use super::{PXZ, TurnMove};

const EPS: f64 = 1e-9;

/// A vertex of a profile: `bulge` is that of the segment to the next
/// vertex (tan of a quarter of its sweep, positive counter-clockwise).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PV {
    pub x: f64,
    pub z: f64,
    #[serde(default)]
    pub bulge: f64,
}

impl PV {
    pub fn new(x: f64, z: f64) -> Self {
        PV { x, z, bulge: 0.0 }
    }

    pub fn arc(x: f64, z: f64, bulge: f64) -> Self {
        PV { x, z, bulge }
    }

    pub fn pos(&self) -> PXZ {
        [self.x, self.z]
    }
}

/// The part as the lathe sees it, and its stock (work coordinates, mm).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurnPart {
    /// The outside from the face towards the chuck; its first vertex is on
    /// the face, its last where the part ends.
    pub outside: Vec<PV>,
    /// The bore from the face inwards; empty for a solid part.
    #[serde(default)]
    pub inside: Vec<PV>,
    /// The stock bar's radius.
    pub stock_radius: f64,
    /// The stock's bore radius (a tube, a drilled hole); 0 for a bar.
    #[serde(default)]
    pub stock_inner_radius: f64,
    /// The Z of the stock's face (the part's face or beyond it).
    pub stock_face: f64,
}

impl TurnPart {
    /// The Z of the part's face.
    pub fn face_z(&self) -> f64 {
        self.outside.first().map_or(0.0, |v| v.z)
    }

    /// The Z where the part ends towards the chuck.
    pub fn back_z(&self) -> f64 {
        self.outside.last().map_or(0.0, |v| v.z)
    }

    /// What is wrong with it, in words.
    pub fn validate(&self) -> Result<(), String> {
        if self.outside.len() < 2 {
            return Err("The part has no outside profile to turn.".to_owned());
        }
        let finite = |c: &[PV]| {
            c.iter()
                .all(|v| v.x.is_finite() && v.z.is_finite() && v.bulge.is_finite() && v.x >= -EPS)
        };
        if !finite(&self.outside) || !finite(&self.inside) {
            return Err("The part's profile must lie on one side of the spindle axis.".to_owned());
        }
        if !(self.stock_radius.is_finite() && self.stock_radius > 0.0) {
            return Err("The stock needs a diameter above zero.".to_owned());
        }
        let widest = self.outside.iter().map(|v| v.x).fold(0.0, f64::max);
        if widest > self.stock_radius + 1e-6 {
            return Err("The part is wider than its stock.".to_owned());
        }
        if !(self.stock_inner_radius.is_finite()
            && self.stock_inner_radius >= 0.0
            && self.stock_inner_radius < self.stock_radius)
        {
            return Err("The stock's bore must be narrower than the stock.".to_owned());
        }
        if self.stock_face < self.face_z() - 1e-6 {
            return Err("The stock's face must not be behind the part's face.".to_owned());
        }
        if self.back_z() >= self.face_z() {
            return Err("The part's outside must run from its face towards the chuck.".to_owned());
        }
        Ok(())
    }
}

/// A piece of a chain: a line or an arc about `center`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Piece {
    pub a: PXZ,
    pub b: PXZ,
    pub center: Option<PXZ>,
    /// Counter-clockwise as drawn (Z right, X up).
    pub ccw: bool,
}

fn pline_vertex(v: &PV) -> PlineVertex<f64> {
    PlineVertex::new(v.z, v.x, v.bulge)
}

/// The pieces of a chain, arcs split into quarters at their extremes in X
/// and Z, so that every piece is monotone in both.
pub(crate) fn pieces(chain: &[PV]) -> Vec<Piece> {
    let mut out = Vec::new();
    for w in chain.windows(2) {
        let (v, n) = (w[0], w[1]);
        if (v.x - n.x).abs() < EPS && (v.z - n.z).abs() < EPS {
            continue;
        }
        if v.bulge.abs() < 1e-12 {
            out.push(Piece {
                a: v.pos(),
                b: n.pos(),
                center: None,
                ccw: false,
            });
            continue;
        }
        let (r, c) = seg_arc_radius_and_center(pline_vertex(&v), pline_vertex(&n));
        let c = [c.y, c.x];
        let ccw = v.bulge > 0.0;
        let sweep = 4.0 * v.bulge.atan();
        let a0 = (v.x - c[0]).atan2(v.z - c[1]);
        // The quarter angles strictly inside the sweep.
        let quarter = std::f64::consts::FRAC_PI_2;
        let mut cuts = Vec::new();
        let (lo, hi) = if sweep > 0.0 {
            (a0, a0 + sweep)
        } else {
            (a0 + sweep, a0)
        };
        let mut k = (lo / quarter).floor() as i64 + 1;
        while (k as f64) * quarter < hi - 1e-9 {
            let a = k as f64 * quarter;
            if a > lo + 1e-9 {
                cuts.push(a);
            }
            k += 1;
        }
        if sweep < 0.0 {
            cuts.reverse();
        }
        let mut at = v.pos();
        for a in cuts {
            let p = [c[0] + r * a.sin(), c[1] + r * a.cos()];
            out.push(Piece {
                a: at,
                b: p,
                center: Some(c),
                ccw,
            });
            at = p;
        }
        out.push(Piece {
            a: at,
            b: n.pos(),
            center: Some(c),
            ccw,
        });
    }
    out
}

/// The point of a monotone piece where coordinate `k` (0: x, 1: z) is
/// `value`, when it is within the piece.
pub(crate) fn piece_at(p: &Piece, k: usize, value: f64) -> Option<PXZ> {
    let (lo, hi) = if p.a[k] <= p.b[k] {
        (p.a[k], p.b[k])
    } else {
        (p.b[k], p.a[k])
    };
    if value < lo - EPS || value > hi + EPS {
        return None;
    }
    if (p.a[k] - value).abs() < EPS {
        return Some(p.a);
    }
    if (p.b[k] - value).abs() < EPS {
        return Some(p.b);
    }
    let o = 1 - k;
    match p.center {
        None => {
            let t = (value - p.a[k]) / (p.b[k] - p.a[k]);
            let mut q = [0.0; 2];
            q[k] = value;
            q[o] = p.a[o] + t * (p.b[o] - p.a[o]);
            Some(q)
        }
        Some(c) => {
            let r = (p.a[0] - c[0]).hypot(p.a[1] - c[1]);
            let h = (r * r - (value - c[k]).powi(2)).max(0.0).sqrt();
            let (olo, ohi) = if p.a[o] <= p.b[o] {
                (p.a[o], p.b[o])
            } else {
                (p.b[o], p.a[o])
            };
            [c[o] + h, c[o] - h]
                .into_iter()
                .find(|v| *v >= olo - 1e-7 && *v <= ohi + 1e-7)
                .map(|v| {
                    let mut q = [0.0; 2];
                    q[k] = value;
                    q[o] = v.clamp(olo, ohi);
                    q
                })
        }
    }
}

/// A chain from pieces (joined end to start).
pub(crate) fn chain_of(pieces: &[Piece]) -> Vec<PV> {
    let mut out: Vec<PV> = Vec::new();
    for p in pieces {
        if (p.a[0] - p.b[0]).abs() < EPS && (p.a[1] - p.b[1]).abs() < EPS {
            continue;
        }
        let bulge = match p.center {
            None => 0.0,
            Some(c) => {
                let a0 = (p.a[0] - c[0]).atan2(p.a[1] - c[1]);
                let a1 = (p.b[0] - c[0]).atan2(p.b[1] - c[1]);
                let mut d = a1 - a0;
                let tau = std::f64::consts::TAU;
                if p.ccw {
                    while d <= 0.0 {
                        d += tau;
                    }
                } else {
                    while d >= 0.0 {
                        d -= tau;
                    }
                }
                (d / 4.0).tan()
            }
        };
        match out.last_mut() {
            Some(last) if (last.x - p.a[0]).abs() < 1e-7 && (last.z - p.a[1]).abs() < 1e-7 => {
                last.bulge = bulge;
            }
            _ => out.push(PV {
                x: p.a[0],
                z: p.a[1],
                bulge,
            }),
        }
        out.push(PV::new(p.b[0], p.b[1]));
    }
    merge_lines(out)
}

/// Joins consecutive collinear lines.
fn merge_lines(chain: Vec<PV>) -> Vec<PV> {
    let mut out: Vec<PV> = Vec::with_capacity(chain.len());
    for v in chain {
        if out.len() >= 2 {
            let b = out[out.len() - 1];
            let a = out[out.len() - 2];
            if a.bulge.abs() < 1e-12 && b.bulge.abs() < 1e-12 {
                let cross = (b.x - a.x) * (v.z - b.z) - (b.z - a.z) * (v.x - b.x);
                let dot = (b.x - a.x) * (v.x - b.x) + (b.z - a.z) * (v.z - b.z);
                if cross.abs() < 1e-9 && dot > 0.0 {
                    let n = out.len();
                    out[n - 1] = v;
                    continue;
                }
            }
        }
        out.push(v);
    }
    out
}

/// The envelope a tool cutting towards the chuck can follow: on the
/// outside (`inside` false) the deepest the part is at any Z towards the
/// face, on the inside the narrowest. Grooves are bridged at the height of
/// their higher side; a wall leaning back towards the face becomes a
/// shoulder where it starts.
pub fn envelope(chain: &[PV], inside: bool) -> Vec<PV> {
    let s = if inside { -1.0 } else { 1.0 };
    let depth = |p: PXZ| s * p[0];
    let ps = pieces(chain);
    let Some(first) = ps.first() else {
        return chain.to_vec();
    };
    let mut cur = first.a;
    let mut out: Vec<Piece> = Vec::new();
    let push_line = |out: &mut Vec<Piece>, from: PXZ, to: PXZ| {
        if (from[0] - to[0]).abs() > EPS || (from[1] - to[1]).abs() > EPS {
            out.push(Piece {
                a: from,
                b: to,
                center: None,
                ccw: false,
            });
        }
    };
    for p in &ps {
        let m = depth(cur);
        let zc = cur[1];
        // What the piece holds towards the face of where the envelope is
        // raises it there.
        let mut behind = Vec::new();
        for q in [p.a, p.b] {
            if q[1] >= zc - EPS {
                behind.push(q);
            }
        }
        if let Some(q) = piece_at(p, 1, zc) {
            behind.push(q);
        }
        let deepest = behind
            .iter()
            .map(|q| depth(*q))
            .fold(f64::NEG_INFINITY, f64::max);
        if deepest > m + EPS {
            let to = [s * deepest, zc];
            push_line(&mut out, cur, to);
            cur = to;
        }
        let m = depth(cur);
        let zc = cur[1];
        // A piece going towards the chuck and outwards: its part beyond
        // the envelope is on it.
        let staircase = p.b[1] <= p.a[1] + EPS && depth(p.b) >= depth(p.a) - EPS;
        if !staircase || p.b[1] > zc - EPS {
            continue;
        }
        if depth(p.b) < m - EPS {
            continue;
        }
        // Where the visible part starts: at the envelope's depth or its Z,
        // whichever comes later along the piece.
        let start_depth = if depth(p.a) < m {
            piece_at(p, 0, s * m)
        } else {
            Some(p.a)
        };
        let start_z = if p.a[1] > zc {
            piece_at(p, 1, zc)
        } else {
            Some(p.a)
        };
        let (Some(u), Some(v)) = (start_depth, start_z) else {
            continue;
        };
        let p0 = if u[1] <= v[1] { u } else { v };
        // Across (or up) to it, then along the piece.
        push_line(&mut out, cur, p0);
        if (p0[0] - p.b[0]).abs() > EPS || (p0[1] - p.b[1]).abs() > EPS {
            out.push(Piece {
                a: p0,
                b: p.b,
                center: p.center,
                ccw: p.ccw,
            });
        }
        cur = p.b;
    }
    let end = ps.last().map_or(cur, |p| p.b);
    if end[1] < cur[1] - EPS {
        push_line(&mut out, cur, [cur[0], end[1]]);
    }
    chain_of(&out)
}

/// The chain moved from the nose's centre line to the controlled point:
/// offset by `nose_radius` away from the material, then by the nose's
/// offset to the imaginary tip, then by the allowances (radius and Z)
/// away from the material.
pub fn tool_path(
    chain: &[PV],
    inside: bool,
    nose_radius: f64,
    allowance_x: f64,
    allowance_z: f64,
) -> Vec<PV> {
    let s = if inside { -1.0 } else { 1.0 };
    let offset = if nose_radius > EPS {
        let mut pl: Polyline<f64> = Polyline::new();
        for v in chain {
            // `last` of an empty polyline underflows in cavalier_contours.
            let last = if pl.vertex_count() == 0 {
                None
            } else {
                pl.last()
            };
            if let Some(last) = last
                && (last.x - v.z).abs() < 1e-9
                && (last.y - v.x).abs() < 1e-9
            {
                continue;
            }
            pl.add(v.z, v.x, v.bulge);
        }
        if pl.vertex_count() < 2 {
            chain.to_vec()
        } else {
            // The chain runs towards the chuck: the air is on its right
            // outside, on its left inside.
            let parts = pl.parallel_offset(-s * nose_radius);
            let mut out: Vec<PV> = Vec::new();
            let mut parts: Vec<Polyline<f64>> = parts;
            parts.sort_by(|a, b| {
                let za = a.get(0).map_or(0.0, |v| v.x);
                let zb = b.get(0).map_or(0.0, |v| v.x);
                zb.total_cmp(&za)
            });
            for part in parts {
                if let Some(last) = out.last_mut() {
                    last.bulge = 0.0;
                }
                for i in 0..part.vertex_count() {
                    let v = part.at(i);
                    out.push(PV {
                        x: v.y,
                        z: v.x,
                        bulge: v.bulge,
                    });
                }
            }
            if let Some(last) = out.last_mut() {
                last.bulge = 0.0;
            }
            out
        }
    } else {
        chain.to_vec()
    };
    let dx = -s * nose_radius + s * allowance_x;
    let dz = -nose_radius + allowance_z;
    let mut out: Vec<PV> = offset
        .into_iter()
        .map(|v| PV {
            x: v.x + dx,
            z: v.z + dz,
            bulge: v.bulge,
        })
        .collect();
    if let Some(last) = out.last_mut() {
        last.bulge = 0.0;
    }
    out
}

/// The first point along a chain where it reaches `x` (a radius), and Z
/// there.
pub fn first_reach(chain: &[PV], x: f64) -> Option<f64> {
    for p in pieces(chain) {
        if let Some(q) = piece_at(&p, 0, x) {
            return Some(q[1]);
        }
    }
    None
}

/// The chain's moves after its first vertex: lines and arcs at `feed`.
pub fn chain_moves(chain: &[PV], feed: f64) -> Vec<TurnMove> {
    let mut out = Vec::new();
    for w in chain.windows(2) {
        let (v, n) = (w[0], w[1]);
        if (v.x - n.x).abs() < 1e-9 && (v.z - n.z).abs() < 1e-9 {
            continue;
        }
        if v.bulge.abs() < 1e-12 {
            out.push(TurnMove::Line { to: n.pos(), feed });
        } else {
            let (_, c) = seg_arc_radius_and_center(
                PlineVertex::new(v.z, v.x, v.bulge),
                PlineVertex::new(n.z, n.x, 0.0),
            );
            let c: Vector2<f64> = c;
            out.push(TurnMove::Arc {
                to: n.pos(),
                center: [c.y, c.x],
                cw: v.bulge < 0.0,
                feed,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn the_envelope_bridges_a_groove() {
        // 20 dia to z -10, a groove 16 dia from -10 to -13, 20 dia again,
        // a shoulder up to 30 dia at -20.
        let chain = vec![
            PV::new(10.0, 0.0),
            PV::new(10.0, -10.0),
            PV::new(8.0, -10.0),
            PV::new(8.0, -13.0),
            PV::new(10.0, -13.0),
            PV::new(10.0, -20.0),
            PV::new(15.0, -20.0),
            PV::new(15.0, -30.0),
        ];
        let env = envelope(&chain, false);
        let pts: Vec<PXZ> = env.iter().map(|v| v.pos()).collect();
        assert_eq!(
            pts,
            vec![[10.0, 0.0], [10.0, -20.0], [15.0, -20.0], [15.0, -30.0]]
        );
    }

    #[test]
    fn a_bore_envelope_bridges_its_recess() {
        // A bore 12 dia with a recess 14 dia, then 10 dia deeper.
        let chain = vec![
            PV::new(6.0, 0.0),
            PV::new(6.0, -5.0),
            PV::new(7.0, -5.0),
            PV::new(7.0, -8.0),
            PV::new(6.0, -8.0),
            PV::new(6.0, -12.0),
            PV::new(5.0, -12.0),
            PV::new(5.0, -20.0),
        ];
        let env = envelope(&chain, true);
        let pts: Vec<PXZ> = env.iter().map(|v| v.pos()).collect();
        assert_eq!(
            pts,
            vec![[6.0, 0.0], [6.0, -12.0], [5.0, -12.0], [5.0, -20.0]]
        );
    }

    #[test]
    fn a_wall_leaning_back_becomes_a_shoulder() {
        let chain = vec![
            PV::new(10.0, 0.0),
            PV::new(10.0, -10.0),
            PV::new(15.0, -8.0),
            PV::new(15.0, -20.0),
        ];
        let env = envelope(&chain, false);
        let pts: Vec<PXZ> = env.iter().map(|v| v.pos()).collect();
        assert_eq!(
            pts,
            vec![[10.0, 0.0], [10.0, -10.0], [15.0, -10.0], [15.0, -20.0]]
        );
    }

    #[test]
    fn the_tool_path_of_a_shoulder_puts_the_tip_on_the_part() {
        // Straight in X and Z the imaginary tip runs on the profile.
        let chain = vec![
            PV::new(10.0, 0.0),
            PV::new(10.0, -20.0),
            PV::new(15.0, -20.0),
            PV::new(15.0, -30.0),
        ];
        let path = tool_path(&chain, false, 0.4, 0.0, 0.0);
        let first = path.first().unwrap();
        assert!(close(first.x, 10.0) && close(first.z, -0.4), "{path:?}");
        // The shoulder's wall: the tip at Z -20.
        assert!(
            path.iter()
                .any(|v| close(v.z, -20.0) && v.x > 10.5 && v.x < 15.0)
        );
        let last = path.last().unwrap();
        assert!(close(last.x, 15.0) && close(last.z, -30.4), "{path:?}");
        // With allowances it stands off.
        let rough = tool_path(&chain, false, 0.4, 0.2, 0.1);
        assert!(close(rough[0].x, 10.2));
    }

    #[test]
    fn arcs_split_into_monotone_quarters() {
        // A half circle bead of radius 2 about (10, -5).
        let chain = vec![PV::arc(10.0, -3.0, 1.0), PV::new(10.0, -7.0)];
        let ps = pieces(&chain);
        assert_eq!(ps.len(), 2);
        assert!(close(ps[0].b[0], 12.0) && close(ps[0].b[1], -5.0), "{ps:?}");
        assert_eq!(
            first_reach(&chain, 11.0).map(|z| (z * 1e6).round() / 1e6),
            Some(-3.267949)
        );
    }
}
