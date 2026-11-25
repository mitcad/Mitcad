// SPDX-License-Identifier: MIT
//! A flange on several connected straight edges, its walls mitred where the
//! edges meet (mitcad#1872, GitHub mitcad/Mitcad#337; commands.md, "A flange
//! on several edges").
//!
//! Each edge gets one bend and one wall, as a single flange does (bend and
//! wall slot `k` for the `k`-th edge, curves `100 k` on). Where two edges
//! meet, the corner gives each flange a half-space its material keeps to
//! ([`Keep`]): the plane bisecting the corner, half the corner gap off it,
//! or for a wall that runs past the other, the other wall's surfaces. A
//! sheet's walls are cut normal to their surfaces (as a laser cuts), so a
//! wall is cut on the straight line where the half-space's plane meets the
//! one of its two surfaces that reaches further, and a bend ends square
//! where all its material clears the plane. The panel beside a bend that a
//! corner shortens is cut back to the bend's tangent line up to the edge's
//! end (a corner relief of the setback's depth; across a bend there, as a
//! flange's trim goes on through it, mitcad#211); the wall runs past the
//! bend's end to the corner's line (or as far as the corner lets it past
//! the edge's end), its root there free.
//!
//! A free end that meets an existing flange of the sheet across a bend
//! keeps the corner gap short of that flange's wall, which is never cut
//! (mitcad#1877, [`existing_walls`]). The refusals at a corner say what is
//! wrong in words; their places are the walls of the edges that meet there
//! (as the input sheet has them) and the corner, painted on hover
//! (`docs/ui-conventions.md`, "What a message says").
//!
//! Two edge ends that face each other across a corner gap the sheet
//! records ([`crate::sheet::SheetGap`]: a rip of Convert to Sheet Metal,
//! or the gap between two walls of an earlier flange on several edges) are
//! one corner too (mitcad#2098), its point where the two edges' lines come
//! closest; the walls are mitred there exactly as at a touching corner.
//! Each flange on several edges records the gap at each of its corners.

use std::f64::consts::{PI, TAU};

use mitcad_vec3::{add, cross, dot, norm, scale, sub};

use super::bends::{
    BendMetrics, PortFrame, add_bend_strip, construction, coverage, cut_back, flange_steps, free,
    outline_overreach, port_frame, side,
};
use super::*;
use crate::features::sheet_folded::*;
use crate::notice::{FeatureError, NoticePlace, PLACED_TAIL};

type V3 = [f64; 3];

/// Ends of two edges this close (mm) are one corner.
const MEET: f64 = 1e-3;

/// Where a clipped wall's vertices count as on a line (mm).
const CLIP: f64 = 1e-9;

/// The wall polygon's label of its root line before it is split into the
/// bend's port and the free root beside it.
const ROOT: u32 = u32::MAX;

/// A half-space `normal . p >= offset` that the material of one flange's
/// bend or wall keeps to.
#[derive(Debug, Clone, Copy)]
pub(super) struct Keep {
    pub(super) normal: V3,
    pub(super) offset: f64,
}

impl Keep {
    fn flipped(normal: V3, point: V3, gap: f64) -> [Keep; 2] {
        [
            Keep {
                normal,
                offset: dot(normal, point) + gap / 2.0,
            },
            Keep {
                normal: scale(normal, -1.0),
                offset: -dot(normal, point) + gap / 2.0,
            },
        ]
    }
}

/// What one end of an edge's flange keeps to: none at a free end.
#[derive(Debug, Clone, Default)]
struct EndRule {
    /// The end's point (the edge's end on its face's front surface, as the
    /// input sheet has it).
    at: V3,
    mitred: bool,
    bend: Vec<Keep>,
    wall: Vec<Keep>,
    /// Where a refusal about this end paints: the walls that meet there
    /// and the corner (mitcad#1877).
    places: Vec<NoticePlace>,
}

/// The wall of a flange's edge in the input sheet (the side face the edge
/// was picked by): a corner refusal's place.
fn edge_place(edge: &BoundaryRef) -> NoticePlace {
    NoticePlace::face(crate::sheet::pick::wall_name(edge.panel, &edge.curve))
}

/// The front face of a panel or a bend of the input sheet: the wall of an
/// existing flange that a refusal is about.
fn domain_place(domain: SheetDomainRef) -> NoticePlace {
    crate::sheet::wall_place(domain)
}

/// A refusal at a corner (mitcad#1877): `message` in words, `places` (the
/// walls that meet there, the corner) painted on hover.
fn at_corner(message: &str, places: &[NoticePlace]) -> String {
    FeatureError::new(message)
        .with_places(places.iter().cloned())
        .text()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    Start,
    End,
}

impl End {
    fn index(self) -> usize {
        match self {
            End::Start => 0,
            End::End => 1,
        }
    }
}

/// An edge's flange as the input sheet places it: the edge along its
/// canonical axial direction `e` from `origin`, and its wall's plane.
struct EdgeGeometry {
    origin: V3,
    e: V3,
    width: f64,
    /// The wall's front frame where the bend starts at the edge's start.
    wall_origin: V3,
    /// The wall's normal from its back surface to its front (`x × y`).
    wall_normal: V3,
    /// Along the wall away from its root (its chart's `x`).
    wall_x: V3,
    thickness: f64,
}

impl EdgeGeometry {
    fn new(
        state: &SheetState,
        edge: &BoundaryRef,
        tangent: f64,
        metrics: &BendMetrics,
    ) -> Result<Self, String> {
        let frame = port_frame(state, edge)?;
        let built = construction(&frame, tangent, 0.0, metrics);
        let SheetEmbedding::Plane { front_frame, .. } = built.moving else {
            return Err("a flange's wall is not planar".into());
        };
        Ok(Self {
            origin: frame.origin,
            e: frame.e,
            width: frame.width,
            wall_origin: front_frame.origin,
            wall_normal: cross(front_frame.x_axis, front_frame.y_axis),
            wall_x: front_frame.x_axis,
            thickness: metrics.thickness,
        })
    }
    fn point(&self, end: End) -> V3 {
        match end {
            End::Start => self.origin,
            End::End => add(self.origin, scale(self.e, self.width)),
        }
    }
    /// Along the edge away from its end `end`.
    fn away(&self, end: End) -> V3 {
        match end {
            End::Start => self.e,
            End::End => scale(self.e, -1.0),
        }
    }
    /// A point of the wall's root line in the middle of its thickness.
    fn root(&self) -> V3 {
        sub(
            self.wall_origin,
            scale(self.wall_normal, self.thickness / 2.0),
        )
    }
    /// The wall's slab along `s` (± its normal): its least and greatest
    /// `s . p`.
    fn slab(&self, s: V3) -> [f64; 2] {
        let middle = dot(s, self.root());
        [middle - self.thickness / 2.0, middle + self.thickness / 2.0]
    }
}

/// Two edges whose ends meet: edge `edges[0]` at its end `ends[0]` and so on.
#[derive(Debug, Clone, Copy)]
struct Corner {
    edges: [usize; 2],
    ends: [End; 2],
    /// Whether the ends face each other across a recorded corner gap
    /// (mitcad#2098) rather than touch.
    gap: bool,
}

/// An edge of a flange as [`corners`] reads it: its panel, its two ends
/// and its direction.
#[derive(Debug, Clone, Copy)]
struct EdgeEnds {
    panel: PanelId,
    points: [V3; 2],
    e: V3,
}

impl EdgeEnds {
    fn of(state: &SheetState, edge: &BoundaryRef) -> Result<Self, String> {
        let frame = port_frame(state, edge)?;
        Ok(Self {
            panel: edge.panel,
            points: [frame.origin, add(frame.origin, scale(frame.e, frame.width))],
            e: frame.e,
        })
    }
}

/// Where a point lies on a bend's tangent line: its axial coordinate and
/// whether on the moving line.
struct SeamPoint {
    v: f64,
    moving: bool,
}

/// The point on one of a bend's two tangent lines (either surface), or none.
fn on_seam(c: &CylinderEmbedding, thickness: f64, p: V3) -> Option<SeamPoint> {
    let d = sub(p, c.centre);
    let v = dot(d, c.axis);
    let w = sub(d, scale(c.axis, v));
    let rho = norm(w);
    if rho < c.inside_radius - MEET || rho > c.inside_radius + thickness + MEET {
        return None;
    }
    let theta = dot(w, c.angular_direction).atan2(dot(w, c.radial_start));
    let theta = if theta < 0.0 { theta + TAU } else { theta };
    let tolerance = MEET / rho.max(MEET);
    let stationary = theta <= tolerance || theta >= TAU - tolerance;
    let moving = (theta - c.angle_extent).abs() <= tolerance;
    (stationary != moving).then_some(SeamPoint { v, moving })
}

/// Whether two edge ends meet: at one point, or across the side of a bend
/// between their faces (one on each tangent line, at the same place along
/// the bend).
fn meet(state: &SheetState, thickness: f64, p: V3, q: V3) -> bool {
    if norm(sub(p, q)) <= MEET {
        return true;
    }
    state.bends.iter().any(|b| {
        match (
            on_seam(&b.folded_embedding, thickness, p),
            on_seam(&b.folded_embedding, thickness, q),
        ) {
            (Some(a), Some(c)) => a.moving != c.moving && (a.v - c.v).abs() <= MEET,
            _ => false,
        }
    })
}

/// Whether the end `p` of an edge on panel `a` and the end `q` of an edge
/// on panel `b` face each other across a corner gap the sheet records
/// (mitcad#2098): the two panels are the gap's, and both ends lie at the
/// same place of the gap's corner line, each no further from it than the
/// gap and the sheet's thickness twice over (an end on a panel cut at the
/// gap lies on its cut, on either surface). The gap itself is never
/// guessed from the ends' distance.
fn across_gap(
    state: &SheetState,
    thickness: f64,
    (a, p): (PanelId, V3),
    (b, q): (PanelId, V3),
) -> bool {
    a != b
        && state.gaps.iter().any(|gap| {
            if gap.panels != [a, b] && gap.panels != [b, a] {
                return false;
            }
            let [start, end] = gap.line;
            let along = sub(end, start);
            let length = norm(along);
            if length <= MEET {
                return false;
            }
            let u = scale(along, 1.0 / length);
            let bound = gap.width.max(0.0) + 2.0 * thickness + MEET;
            let at = |x: V3| {
                let d = sub(x, start);
                let v = dot(d, u);
                let off = norm(sub(d, scale(u, v)));
                (v >= -bound && v <= length + bound && off <= bound).then_some(v)
            };
            match (at(p), at(q)) {
                (Some(v), Some(w)) => (v - w).abs() <= bound,
                _ => false,
            }
        })
}

/// The middle of the shortest segment between two lines, or none when
/// they are parallel.
fn closest(p: V3, d: V3, q: V3, f: V3) -> Option<V3> {
    let w = sub(p, q);
    let b = dot(d, f);
    let denominator = 1.0 - b * b;
    if denominator < 1e-12 {
        return None;
    }
    let (dd, ff) = (dot(d, w), dot(f, w));
    let s = (b * ff - dd) / denominator;
    let t = (ff - b * dd) / denominator;
    Some(scale(add(add(p, scale(d, s)), add(q, scale(f, t))), 0.5))
}

/// The corners of the flange's edges, each given by its two ends (by the
/// edge's start and end, [`End`]): each pair of ends that meet, or that
/// face each other across a recorded corner gap (mitcad#2098). Each end
/// meets at most one other, and the edges make one chain or loop.
fn corners(state: &SheetState, thickness: f64, ends: &[EdgeEnds]) -> Result<Vec<Corner>, String> {
    let mut found = Vec::new();
    for i in 0..ends.len() {
        for j in i + 1..ends.len() {
            for a in [End::Start, End::End] {
                for b in [End::Start, End::End] {
                    let (p, q) = (ends[i].points[a.index()], ends[j].points[b.index()]);
                    let touching = meet(state, thickness, p, q);
                    if touching
                        || across_gap(state, thickness, (ends[i].panel, p), (ends[j].panel, q))
                    {
                        found.push(Corner {
                            edges: [i, j],
                            ends: [a, b],
                            gap: !touching,
                        });
                    }
                }
            }
        }
    }
    let mut used = vec![[0_usize; 2]; ends.len()];
    for c in &found {
        for (edge, end) in c.edges.iter().zip(c.ends) {
            used[*edge][end.index()] += 1;
        }
    }
    if used.iter().flatten().any(|n| *n > 1) {
        return Err(
            "More than two of the flange's edges meet at one corner; a flange's edges make one \
             chain, each corner joining two of them."
                .into(),
        );
    }
    if ends.len() > 1 {
        // One chain or loop: every edge reached from the first.
        let mut reached = vec![false; ends.len()];
        reached[0] = true;
        let mut grew = true;
        while grew {
            grew = false;
            for c in &found {
                let [a, b] = c.edges;
                if reached[a] != reached[b] {
                    reached[a] = true;
                    reached[b] = true;
                    grew = true;
                }
            }
        }
        if reached.contains(&false) {
            return Err(
                "The flange's edges must be connected at their ends, at a common corner or across \
                 the side of a bend between their faces; one of them meets none of the others."
                    .into(),
            );
        }
    }
    Ok(found)
}

/// Where the corner `c` of the edges `ends` is: the middle of their two
/// ends (on the front surface, as the sheet has them); across a corner gap
/// (mitcad#2098) where the two edges' lines come closest, the gap's corner.
fn corner_point(ends: &[EdgeEnds], c: &Corner) -> V3 {
    let [i, j] = c.edges;
    let (p, q) = (
        ends[i].points[c.ends[0].index()],
        ends[j].points[c.ends[1].index()],
    );
    let middle = scale(add(p, q), 0.5);
    if !c.gap {
        return middle;
    }
    closest(p, ends[i].e, q, ends[j].e).unwrap_or(middle)
}

/// The corners a flange on `edges` of the sheet `state` would have (the
/// `sheet_flange_corners` query, mitcad#1877: the Flange panel's Corners
/// map a picked corner to its two edges): each pair of edges that meet, by
/// their indices in `edges`, and where ([`corner_point`]).
pub(crate) fn edge_corners(
    state: &SheetState,
    thickness: f64,
    edges: &[BoundaryRef],
) -> Result<Vec<([usize; 2], V3)>, String> {
    let ends = edges
        .iter()
        .map(|edge| EdgeEnds::of(state, edge))
        .collect::<Result<Vec<_>, String>>()?;
    Ok(corners(state, thickness, &ends)?
        .into_iter()
        .map(|c| (c.edges, corner_point(&ends, &c)))
        .collect())
}

/// The miter of the corner between edges `a` and `b`.
fn miter_of(d: &SheetFlangeDef<f64>, a: usize, b: usize) -> SheetMiter {
    d.corner_miters
        .iter()
        .find(|c| {
            let [x, y] = c.edges.map(|e| e as usize);
            (x, y) == (a, b) || (x, y) == (b, a)
        })
        .map_or(d.miter.unwrap_or_default(), |c| c.miter)
}

/// Whether two walls lie in one plane (their slabs coincide).
fn coplanar(a: &EdgeGeometry, b: &EdgeGeometry) -> bool {
    norm(cross(a.wall_normal, b.wall_normal)) < 1e-9
        && dot(a.wall_normal, sub(b.root(), a.root())).abs() < 1e-6
}

/// The keep of a wall that ends `gap` short of the wall `other`'s surface
/// it comes to, moving `along` (towards the corner), or none when the
/// wall runs beside the other.
fn short_of(other: &EdgeGeometry, along: V3, gap: f64) -> Option<Keep> {
    let n = other.wall_normal;
    let s = dot(n, along);
    if s.abs() < 1e-6 {
        return None;
    }
    let s = if s > 0.0 { n } else { scale(n, -1.0) };
    let near = other.slab(s)[0];
    Some(Keep {
        normal: scale(s, -1.0),
        offset: -(near - gap),
    })
}

/// The point of a corner's bisecting plane: where the two walls' root
/// lines come closest, or across from `vertex` on them when they are
/// parallel.
fn mitre_point(gi: &EdgeGeometry, gj: &EdgeGeometry, vertex: V3) -> V3 {
    closest(gi.root(), gi.e, gj.root(), gj.e).unwrap_or_else(|| {
        let on = |g: &EdgeGeometry| add(g.root(), scale(g.e, dot(sub(vertex, g.root()), g.e)));
        scale(add(on(gi), on(gj)), 0.5)
    })
}

/// The corner gap a flange records between the walls of the edges
/// `c.edges` (mitcad#2098): its line in the bisecting plane, along the
/// walls from where their roots meet, as far as a wall reaches (`reach`).
fn corner_gap(
    uid: FeatureUid,
    geometry: &[EdgeGeometry],
    ends: &[EdgeEnds],
    c: &Corner,
    (gap, reach): (f64, f64),
) -> Option<crate::sheet::SheetGap> {
    let [i, j] = c.edges;
    let (gi, gj) = (&geometry[i], &geometry[j]);
    let difference = sub(gi.away(c.ends[0]), gj.away(c.ends[1]));
    let length = norm(difference);
    if length < 1e-9 {
        return None;
    }
    let n = scale(difference, 1.0 / length);
    let point = mitre_point(gi, gj, corner_point(ends, c));
    // In the bisecting plane and in the wall's plane (where the two walls
    // lie in one plane, the line between them).
    let mut along = cross(n, gi.wall_normal);
    let size = norm(along);
    along = if size < 1e-9 {
        gi.wall_x
    } else {
        scale(along, 1.0 / size)
    };
    if dot(along, gi.wall_x) < 0.0 {
        along = scale(along, -1.0);
    }
    Some(crate::sheet::SheetGap {
        panels: [i, j].map(|k| PanelId {
            feature: uid,
            slot: k as u32,
        }),
        line: [point, add(point, scale(along, reach))],
        width: gap,
    })
}

/// The keeps of each edge's two ends from the corners between the edges
/// (`edges` the flange's edges, for the places of a refusal).
#[allow(clippy::too_many_arguments)]
fn corner_rules(
    d: &SheetFlangeDef<f64>,
    edges: &[&BoundaryRef],
    geometry: &[EdgeGeometry],
    ends: &[EdgeEnds],
    corners: &[Corner],
    gap: f64,
    rules: &mut [[EndRule; 2]],
) -> Result<(), String> {
    for c in corners {
        let [i, j] = c.edges;
        let (gi, gj) = (&geometry[i], &geometry[j]);
        let (ui, uj) = (gi.away(c.ends[0]), gj.away(c.ends[1]));
        // The corner's place: across a gap, the gap's corner (mitcad#2098).
        let vertex = corner_point(ends, c);
        // Where a refusal about this corner paints: both walls, the corner.
        let places = vec![
            edge_place(edges[i]),
            edge_place(edges[j]),
            NoticePlace::point(vertex),
        ];
        let difference = sub(ui, uj);
        let length = norm(difference);
        if length < 1e-9 {
            return Err(at_corner(
                "Two of the flange's edges meet running the same way from their corner; a corner \
                 joins edges that leave it in different directions.",
                &places,
            ));
        }
        let n = scale(difference, 1.0 / length);
        let point = mitre_point(gi, gj, vertex);
        let [keep_i, keep_j] = Keep::flipped(n, point, gap);
        let flat = coplanar(gi, gj);
        let (ri, rj) = (c.ends[0].index(), c.ends[1].index());
        rules[i][ri].mitred = true;
        rules[j][rj].mitred = true;
        rules[i][ri].places.extend(places.iter().cloned());
        rules[j][rj].places.extend(places.iter().cloned());
        // The bends always clear the bisecting plane.
        rules[i][ri].bend.push(keep_i);
        rules[j][rj].bend.push(keep_j);
        let miter = miter_of(d, i, j);
        if miter == SheetMiter::Bisect {
            if flat && gap <= TOLERANCE {
                return Err(at_corner(
                    "Walls of the flange that lie in one plane need a Corner Gap above 0 where they \
                     meet; at 0 they would touch over their cut faces.",
                    &places,
                ));
            }
            rules[i][ri].wall.push(keep_i);
            rules[j][rj].wall.push(keep_j);
            continue;
        }
        if flat {
            return Err(at_corner(
                "Walls of the flange that lie in one plane meet only by a bisecting miter; choose \
                 Bisect for that corner.",
                &places,
            ));
        }
        if gap <= TOLERANCE {
            return Err(at_corner(
                "A corner where one wall runs past the other needs a Corner Gap above 0; walls that \
                 touch over a face cannot make one sound body.",
                &places,
            ));
        }
        // The passing wall `p` runs on to the far surface of the other
        // wall `q`; `q` ends the gap short of `p`'s near surface.
        let (p, q) = if miter == SheetMiter::EarlierRunsPast {
            ((i, ri, ui), (j, rj, uj))
        } else {
            ((j, rj, uj), (i, ri, ui))
        };
        let beside = || {
            at_corner(
                "These two walls of the flange run side by side at their corner, so neither can \
                 run past the other; choose Bisect for that corner.",
                &places,
            )
        };
        let far = short_of(&geometry[q.0], scale(p.2, -1.0), 0.0).ok_or_else(beside)?;
        // (`short_of` gives the near surface; the far one is a thickness on.)
        let far = Keep {
            normal: far.normal,
            offset: far.offset - geometry[q.0].thickness,
        };
        let short = short_of(&geometry[p.0], scale(q.2, -1.0), gap).ok_or_else(beside)?;
        rules[p.0][p.1].wall.push(far);
        rules[q.0][q.1].wall.push(short);
        rules[q.0][q.1].bend.push(short);
    }
    Ok(())
}

/// A flat wall of the sheet as an edge's flange would place it (for
/// [`short_of`]), with its chart; none when it is not flat.
fn flat_wall(
    state: &SheetState,
    wall: PanelId,
    thickness: f64,
) -> Option<(EdgeGeometry, &SheetPatch)> {
    let patch = state
        .charts
        .iter()
        .find(|p| p.domain == SheetDomainRef::Panel(wall))?;
    let (frame, parity) = plane(patch).ok()?;
    let normal = frame.normal();
    let geometry = EdgeGeometry {
        origin: frame.origin,
        e: frame.x_axis,
        width: 0.0,
        // The front surface, the back a thickness along `-normal`.
        wall_origin: frame.origin,
        wall_normal: scale(normal, -parity.sign()),
        wall_x: frame.y_axis,
        thickness,
    };
    Some((geometry, patch))
}

/// The keeps of the free end `end` of the flange's edge `edge` that meets
/// existing flanges of the sheet across a bend (mitcad#1872, mitcad#1877),
/// each with the face of the existing wall it keeps short of. The end lies
/// at an end of a bend's tangent line, on either side of the bend; across
/// the bend lies another panel of the sheet:
///
/// - that panel is a wall of another flange (the end lies where that
///   flange's bend starts, or on the far side of a bend from a flange's
///   wall): the existing wall runs past, and this flange ends the gap short
///   of its surface;
/// - another flange stands on the edge of that panel that meets this end
///   across the bend (its bend ends where the bend across ends, beside the
///   end; the bend across cut back by that flange's trim): this flange ends
///   the gap short of that flange's wall, short of its side where the two
///   walls lie in one plane (or touch face to face).
///
/// The existing flanges are never cut: only this flange's walls and bends
/// keep to the keeps.
fn existing_walls(
    state: &SheetState,
    feature: FeatureUid,
    is_flange: &dyn Fn(FeatureUid) -> bool,
    thickness: f64,
    edge: &EdgeGeometry,
    end: End,
    gap: f64,
) -> Vec<(Keep, NoticePlace, bool)> {
    let point = edge.point(end);
    let toward = scale(edge.away(end), -1.0);
    let theirs = |wall: PanelId| wall.feature != feature && is_flange(wall.feature);
    let mut found = Vec::new();
    for b in &state.bends {
        let Some(seam) = on_seam(&b.folded_embedding, thickness, point) else {
            continue;
        };
        let interval = [b.axial_interval.first, b.axial_interval.last];
        let across = if seam.moving {
            b.stationary.domain
        } else {
            b.moving.domain
        };
        let SheetDomainRef::Panel(panel) = across else {
            continue;
        };
        // The panel across is another flange's wall, the end at an end of
        // the bend.
        if interval.iter().any(|v| (v - seam.v).abs() <= MEET)
            && theirs(panel)
            && let Some((other, _)) = flat_wall(state, panel, thickness)
            && let Some(keep) = short_of(&other, toward, gap)
        {
            found.push((keep, domain_place(across), false));
        }
        // A flange on the panel across, its bend ending across from this
        // end.
        for e in &state.bends {
            if e.id == b.id || e.stationary.domain != across {
                continue;
            }
            let SheetDomainRef::Panel(wall) = e.moving.domain else {
                continue;
            };
            if !theirs(wall) {
                continue;
            }
            let c = &e.folded_embedding;
            // How far this end may lie past the bend across's end: the
            // other flange's trim beside its bend, at most its setback.
            let reach = (c.inside_radius + thickness)
                * (c.angle_extent.min(3.0) / 2.0).tan().max(1.0)
                + MEET;
            let middle = c.inside_radius + thickness / 2.0;
            let meets = [e.axial_interval.first, e.axial_interval.last]
                .iter()
                .any(|v| {
                    // The ends of its bend's tangent lines at `v`, in the
                    // middle of the thickness: on the panel across, and
                    // on its wall.
                    let on_axis = add(c.centre, scale(c.axis, *v));
                    let q = add(on_axis, scale(c.radial_start, middle));
                    let turned = add(
                        scale(c.radial_start, c.angle_extent.cos()),
                        scale(c.angular_direction, c.angle_extent.sin()),
                    );
                    let q_wall = add(on_axis, scale(turned, middle));
                    let Some(there) = on_seam(&b.folded_embedding, thickness, q) else {
                        return false;
                    };
                    let offset = seam.v - there.v;
                    if there.moving == seam.moving || offset.abs() > reach {
                        return false;
                    }
                    // This end lies on the side of the other flange's
                    // tangent line where its bend and wall are (the
                    // other flange's trim beside them), or across from
                    // the line itself.
                    let wall_side = dot(b.folded_embedding.axis, sub(q_wall, q));
                    offset.abs() <= MEET || offset * wall_side > 0.0
                });
            if !meets {
                continue;
            }
            let Some((other, patch)) = flat_wall(state, wall, thickness) else {
                continue;
            };
            let parallel = norm(cross(edge.wall_normal, other.wall_normal)) < 1e-6;
            let keep = if parallel {
                if dot(edge.wall_normal, sub(other.root(), edge.root())).abs() > thickness + 1e-6 {
                    // Walls side by side, apart: they never meet.
                    continue;
                }
                // Walls in one plane: short of the other wall's side, as
                // far as it and its bend reach along their edge.
                let along = dot(c.axis, edge.away(end));
                if along.abs() < 1e-6 {
                    continue;
                }
                let n = scale(c.axis, along.signum());
                let Ok((min, max)) = bounds(patch) else {
                    continue;
                };
                let Ok((frame, _)) = plane(patch) else {
                    continue;
                };
                let chart_corners = [min, [min[0], max[1]], max, [max[0], min[1]]];
                let wall_reach = chart_corners
                    .iter()
                    .map(|p| dot(n, frame.point(*p)))
                    .fold(f64::NEG_INFINITY, f64::max);
                let bend_reach = [e.axial_interval.first, e.axial_interval.last]
                    .iter()
                    .map(|v| dot(n, add(c.centre, scale(c.axis, *v))))
                    .fold(f64::NEG_INFINITY, f64::max);
                Keep {
                    normal: n,
                    offset: wall_reach.max(bend_reach) + gap,
                }
            } else {
                match short_of(&other, toward, gap) {
                    Some(keep) => keep,
                    None => continue,
                }
            };
            found.push((keep, domain_place(e.moving.domain), parallel));
        }
    }
    found
}

/// Where a bend that keeps to `keep` may lie along its axis.
pub(super) enum Limit {
    Lower(f64),
    Upper(f64),
    Free,
    Never,
}

/// The axial limit of a bend on `c` (its axial coordinate from the centre)
/// whose whole cross-section, radii `r` to `r + t` over its angle, keeps
/// to `keep`.
pub(super) fn bend_limit(c: &CylinderEmbedding, thickness: f64, keep: &Keep) -> Limit {
    let n = keep.normal;
    let (a, b) = (dot(n, c.radial_start), dot(n, c.angular_direction));
    // The least of a cos(theta) + b sin(theta) over the bend's angle.
    let mut least = a.min(a * c.angle_extent.cos() + b * c.angle_extent.sin());
    let lowest = (b.atan2(a) + PI).rem_euclid(TAU);
    if lowest <= c.angle_extent {
        least = least.min(-a.hypot(b));
    }
    let reach = if least < 0.0 {
        (c.inside_radius + thickness) * least
    } else {
        c.inside_radius * least
    };
    let need = keep.offset - dot(n, c.centre) - reach;
    let along = dot(n, c.axis);
    if along > 1e-9 {
        Limit::Lower(need / along)
    } else if along < -1e-9 {
        Limit::Upper(need / along)
    } else if need <= TOLERANCE {
        Limit::Free
    } else {
        Limit::Never
    }
}

/// A wall polygon vertex and the label of the edge that starts at it.
type Vertex = ([f64; 2], u32);

/// The convex polygon `polygon` cut by the half-plane `a x + b y >= c`
/// (`(a, b)` a unit vector), the edges along its line labelled `label`.
fn clip(polygon: &[Vertex], [a, b, c]: [f64; 3], label: u32) -> Vec<Vertex> {
    let h = |p: [f64; 2]| a * p[0] + b * p[1] - c;
    let mut out = Vec::new();
    for (i, &(p, l)) in polygon.iter().enumerate() {
        let q = polygon[(i + 1) % polygon.len()].0;
        let (hp, hq) = (h(p), h(q));
        let crossing = || {
            let s = hp / (hp - hq);
            [p[0] + s * (q[0] - p[0]), p[1] + s * (q[1] - p[1])]
        };
        match (hp >= -CLIP, hq >= -CLIP) {
            (true, true) => out.push((p, l)),
            (true, false) => {
                out.push((p, l));
                out.push((crossing(), label));
            }
            (false, true) => out.push((crossing(), l)),
            (false, false) => {}
        }
    }
    // Edges of no length: the vertex after them keeps its label.
    loop {
        let n = out.len();
        if n < 3 {
            break;
        }
        let Some(i) = (0..n).find(|&i| {
            let (p, q) = (out[i].0, out[(i + 1) % n].0);
            (p[0] - q[0]).hypot(p[1] - q[1]) <= TOLERANCE
        }) else {
            break;
        };
        out.remove(i);
    }
    out
}

/// A wall of `length` whose bend covers `0..width` along its root, from
/// `from` to `to` along it before the keeps `cuts` (each the half-plane in
/// the wall's chart and its label): the bend's port on the root the curve
/// `53`, the free root past the bend's ends `55` and `56`, the sides `50`
/// and `52`, the far end `51` and the corners' lines `58` and `59` (each
/// plus `base`). A refusal paints `places` (the wall's corners).
fn wall_polygon(
    feature: FeatureUid,
    base: u32,
    length: f64,
    width: f64,
    (from, to): (f64, f64),
    cuts: &[([f64; 3], u32)],
    places: &[NoticePlace],
) -> Result<MaterialRegion, String> {
    let mut polygon: Vec<Vertex> = vec![
        ([0.0, from], 50),
        ([length, from], 51),
        ([length, to], 52),
        ([0.0, to], ROOT),
    ];
    for (line, label) in cuts {
        polygon = clip(&polygon, *line, *label);
        if polygon.len() < 3 {
            return Err(at_corner(
                "At a corner the flange's wall would be cut away whole; make the Corner Gap \
                 smaller or the wall longer.",
                places,
            ));
        }
    }
    // The root's ends where the bend's ends are, within the tolerance.
    for (p, _) in &mut polygon {
        if p[0].abs() <= TOLERANCE {
            p[0] = 0.0;
            for at in [0.0, width] {
                if (p[1] - at).abs() <= 1e-6 {
                    p[1] = at;
                }
            }
        }
    }
    let line = |slot: u32, start: [f64; 2], end: [f64; 2]| SheetSegment {
        id: SheetCurveId::Generated {
            feature,
            slot: base + slot,
        },
        geometry: SegmentGeometry::Line { start, end },
    };
    let mut segments = Vec::new();
    let mut port = false;
    for (i, &(p, label)) in polygon.iter().enumerate() {
        let q = polygon[(i + 1) % polygon.len()].0;
        if label != ROOT {
            segments.push(line(label, p, q));
            continue;
        }
        // The root runs from the wall's end at `to` back to `from`.
        let (top, bottom) = (p[1], q[1]);
        if top < width || bottom > 0.0 {
            return Err(at_corner(
                "At a corner the flange's wall would not cover the whole of its bend; make the \
                 Corner Gap smaller.",
                places,
            ));
        }
        if top > width + TOLERANCE {
            segments.push(line(56, [0.0, top], [0.0, width]));
        }
        segments.push(line(53, [0.0, width], [0.0, 0.0]));
        if bottom < -TOLERANCE {
            segments.push(line(55, [0.0, 0.0], [0.0, bottom]));
        }
        port = true;
    }
    if !port {
        return Err(at_corner(
            "At a corner the flange's wall would be cut off its bend; make the Corner Gap smaller.",
            places,
        ));
    }
    MaterialRegion::from_legacy(&SheetRegion {
        loops: vec![SheetLoop { segments }],
    })
}

/// The flange on one edge `k` (its bend and wall slot `k`, curves from
/// `100 k`): cut back, bend, wall, as the corners `ends` (by the input
/// sheet's start and end of the edge) let it.
#[allow(clippy::too_many_arguments)]
fn edge_flange<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    state: &mut SheetState,
    reference: &BoundaryRef,
    k: usize,
    metrics: &BendMetrics,
    tangent: f64,
    length: f64,
    gap: f64,
    rules: &[EndRule; 2],
) -> Result<(), String> {
    let thickness = metrics.thickness;
    let frame: PortFrame = port_frame(state, reference)?;
    let w = frame.width;
    // The input's ends of the edge as the edge now runs (an earlier edge's
    // corner may have shortened it).
    let ends = [frame.origin, add(frame.origin, scale(frame.e, w))];
    let straight = norm(sub(rules[0].at, ends[0])) + norm(sub(rules[1].at, ends[1]));
    let crossed = norm(sub(rules[0].at, ends[1])) + norm(sub(rules[1].at, ends[0]));
    let (first, last) = if straight <= crossed {
        (&rules[0], &rules[1])
    } else {
        (&rules[1], &rules[0])
    };
    // Where a refusal about both ends paints (mitcad#1877).
    let both: Vec<NoticePlace> = first.places.iter().chain(&last.places).cloned().collect();
    // The bend's span along the edge: all of it as far as the corners let.
    let at_start = construction(&frame, tangent, 0.0, metrics);
    let (mut lo, mut hi) = (0.0_f64, w);
    for rule in [first, last] {
        for keep in &rule.bend {
            match bend_limit(&at_start.cylinder, thickness, keep) {
                Limit::Lower(v) => lo = lo.max(v),
                Limit::Upper(v) => hi = hi.min(v),
                Limit::Free => {}
                Limit::Never => {
                    return Err(at_corner(
                        "At a corner the flange's bend would lie wholly past the corner; make the \
                         Corner Gap smaller.",
                        &rule.places,
                    ));
                }
            }
        }
    }
    if hi - lo <= TOLERANCE {
        return Err(at_corner(
            "At a corner the Corner Gap leaves no room for the bend on one of the flange's \
             edges; make the gap smaller or the edges longer.",
            &both,
        ));
    }
    // A free end follows the panel's side on to the setback line
    // (mitcad#1576), as a single flange's whole edge does.
    let reach = outline_overreach(state, reference, &frame, tangent)?;
    let cut_lo = if first.mitred { lo } else { lo - reach.0 };
    let cut_hi = if last.mitred { hi } else { hi + reach.1 };
    let slot = 100 * k as u32;
    let tangent_piece = cut_back(
        ctx,
        state,
        thickness,
        reference,
        &frame,
        tangent,
        (cut_lo, w - cut_hi),
        |_| Ok(Vec::new()),
        false,
        None,
        "the flange's trim",
        10 + slot,
    )?;
    let port = coverage(&side(
        chart(state, reference.panel)?,
        &tangent_piece,
        frame.to_operation,
    )?);
    let start = if first.mitred { lo } else { port[0] };
    let end = if last.mitred { w - hi } else { w - port[1] };
    // The panel beside a bend that a corner shortened, cut back to the
    // tangent line up to the edge's end.
    if first.mitred && start > TOLERANCE {
        cut_back(
            ctx,
            state,
            thickness,
            reference,
            &frame,
            tangent,
            (0.0, w - start),
            |_| Ok(Vec::new()),
            false,
            None,
            "the flange's corner",
            70 + slot,
        )?;
    }
    if last.mitred && end > TOLERANCE {
        cut_back(
            ctx,
            state,
            thickness,
            reference,
            &frame,
            tangent,
            (w - end, 0.0),
            |_| Ok(Vec::new()),
            false,
            None,
            "the flange's corner",
            80 + slot,
        )?;
    }
    let width = w - start - end;
    let mut axial = frame.to_operation;
    axial.origin[1] -= start;
    let stationary = side(chart(state, reference.panel)?, &tangent_piece, axial)?;
    let built = construction(&frame, tangent, start, metrics);
    let SheetEmbedding::Plane { front_frame, .. } = &built.moving else {
        return Err("a flange's wall is not planar".into());
    };
    // The wall's chart: `x` along the wall, `y` along the edge from the
    // bend's start; its back a thickness along `-(x × y)`. A keep holds on
    // both surfaces: a half-plane of the chart.
    let (o, x, y) = (front_frame.origin, front_frame.x_axis, front_frame.y_axis);
    let normal = cross(x, y);
    let mut cuts = Vec::new();
    for (rule, label) in [(first, 58), (last, 59)] {
        for keep in &rule.wall {
            let n = keep.normal;
            let (a, b) = (dot(n, x), dot(n, y));
            let c = keep.offset - dot(n, o) + (thickness * dot(n, normal)).max(0.0);
            let size = a.hypot(b);
            if size < 1e-12 {
                if c > TOLERANCE {
                    return Err(at_corner(
                        "At a corner the flange's wall would lie wholly past the corner; make the \
                         Corner Gap smaller.",
                        &rule.places,
                    ));
                }
                continue;
            }
            cuts.push(([a / size, b / size, c / size], label));
        }
    }
    // Past a mitred end the wall may run on beyond the edge's end, as far
    // as the corner lets it.
    let past = length + metrics.radius + 2.0 * thickness + gap;
    let from = if first.mitred { -start - past } else { 0.0 };
    let to = if last.mitred {
        width + end + past
    } else {
        width
    };
    let region = wall_polygon(ctx.uid, slot, length, width, (from, to), &cuts, &both)?;
    let moving = SheetPatch {
        domain: SheetDomainRef::Panel(PanelId {
            feature: ctx.uid,
            slot: k as u32,
        }),
        regions: vec![region],
        embedding: built.moving.clone(),
    };
    let moving_side = side(&moving, &generated(ctx.uid, 53 + slot), Isometry2::IDENTITY)?;
    add_bend_strip(
        ctx,
        state,
        metrics,
        built.cylinder.clone(),
        vec![stationary],
        vec![moving_side],
        width,
        None,
        k as u32,
    )?;
    state.charts.push(moving);
    Ok(())
}

/// A flange on its `boundary` and further `boundaries`, mitred where they
/// meet (and a gap short of an existing flange's wall a free end meets).
/// A refusal's places are faces of the flange's body as it was before.
pub(crate) fn flange_edges<K: Kernel>(
    d: &SheetFlangeDef<f64>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FeatureOutput<K::Shape>, String> {
    mitred(d, ctx).map_err(|e| {
        if e.contains(PLACED_TAIL) {
            FeatureError::from_text(&e)
                .in_body(&d.body.to_string())
                .text()
        } else {
            e
        }
    })
}

/// The flange of [`flange_edges`], its refusals' places of no body yet.
fn mitred<K: Kernel>(
    d: &SheetFlangeDef<f64>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let (mut state, rules) = read(ctx, d.body)?;
    let rules = rules.with_fold_allowance(d.allowance.as_ref());
    if d.profile.is_some()
        || d.hem.is_some()
        || d.corner.is_some()
        || d.offsets.is_some()
        || d.width.is_some()
    {
        return Err(
            "A flange on several edges, or with a corner gap, covers each edge whole with a plain \
             wall; it takes no profile, hem, corner joint, offsets or width."
                .into(),
        );
    }
    if d.position == SheetPosition::Edge {
        return Err(
            "A flange on several edges, or with a corner gap, cannot be At Edge: its corners need \
             the face cut back beside the bends. Choose another Position."
                .into(),
        );
    }
    let gap = d.corner_gap.unwrap_or(rules.thickness);
    if !gap.is_finite() || gap < 0.0 {
        return Err("The Corner Gap must not be negative.".into());
    }
    let edges = d.edges();
    for edge in &edges {
        free(&state, edge)?;
    }
    let metrics = BendMetrics::past_half_turn(d.angle, d.radius, &rules)?;
    let steps = flange_steps(d, &rules)?;
    let length = steps[0].wall;
    if steps.len() != 1 || length <= 0.0 {
        return Err(
            "A flange on several edges needs a wall on each edge: a length longer than its \
             setback."
                .into(),
        );
    }
    let tangent = metrics.offset(d.position)?;
    let thickness = rules.thickness;
    let geometry = edges
        .iter()
        .map(|edge| EdgeGeometry::new(&state, edge, tangent, &metrics))
        .collect::<Result<Vec<_>, String>>()?;
    let ends_of: Vec<EdgeEnds> = geometry
        .iter()
        .zip(&edges)
        .map(|(g, edge)| EdgeEnds {
            panel: edge.panel,
            points: [g.point(End::Start), g.point(End::End)],
            e: g.e,
        })
        .collect();
    let found = corners(&state, thickness, &ends_of)?;
    for c in &d.corner_miters {
        let [a, b] = c.edges.map(|e| e as usize);
        if !found.iter().any(|f| f.edges == [a, b] || f.edges == [b, a]) {
            let named: Vec<NoticePlace> = [a, b]
                .iter()
                .filter_map(|k| edges.get(*k).copied())
                .map(edge_place)
                .collect();
            return Err(at_corner(
                "A corner's miter names two edges of the flange that do not meet.",
                &named,
            ));
        }
    }
    let mut ends: Vec<[EndRule; 2]> = geometry
        .iter()
        .map(|g| {
            [End::Start, End::End].map(|end| EndRule {
                at: g.point(end),
                ..EndRule::default()
            })
        })
        .collect();
    corner_rules(d, &edges, &geometry, &ends_of, &found, gap, &mut ends)?;
    // A free end that meets an existing flange of the sheet across a bend
    // (mitcad#1877).
    let is_flange = |uid: FeatureUid| {
        ctx.feature_entry(uid)
            .is_ok_and(|entry| matches!(entry.def, crate::features::FeatureDef::SheetFlange(_)))
    };
    for (k, g) in geometry.iter().enumerate() {
        for end in [End::Start, End::End] {
            if ends[k][end.index()].mitred {
                continue;
            }
            let met = existing_walls(&state, ctx.uid, &is_flange, thickness, g, end, gap);
            let rule = &mut ends[k][end.index()];
            let mut flat = false;
            for (keep, place, coplanar) in met {
                rule.mitred = true;
                rule.bend.push(keep);
                rule.wall.push(keep);
                rule.places.extend([edge_place(edges[k]), place]);
                flat |= coplanar;
            }
            if rule.mitred {
                rule.places.push(NoticePlace::point(rule.at));
            }
            if flat && gap <= TOLERANCE {
                return Err(at_corner(
                    "A wall of the flange that lies in one plane with an existing flange's wall \
                     needs a Corner Gap above 0 where they meet; at 0 they would touch over their \
                     cut faces.",
                    &rule.places,
                ));
            }
        }
    }
    for (k, edge) in edges.iter().enumerate() {
        edge_flange(
            ctx, &mut state, edge, k, &metrics, tangent, length, gap, &ends[k],
        )?;
    }
    // The gap at each corner between the new walls (mitcad#2098), for a
    // later flange on their edges.
    let reach = length + metrics.radius + 2.0 * thickness + gap;
    let uid = ctx.uid;
    state.gaps.extend(
        found
            .iter()
            .filter_map(|c| corner_gap(uid, &geometry, &ends_of, c, (gap, reach))),
    );
    // The rules' corner relief at the corners between the flange's edges
    // and where its bends meet other bends at a corner of their panel
    // (mitcad#2342); a corner across a recorded gap has no junction of
    // the two bends to relieve. A flange with a relief of its own keeps its
    // corners as they are.
    if d.relief.is_none() && rules.relieves_flange_corners() {
        let bend = |k: usize| BendId {
            feature: uid,
            slot: k as u32,
        };
        let mut pairs: Vec<[BendId; 2]> = found
            .iter()
            .filter(|c| !c.gap)
            .map(|c| [bend(c.edges[0]), bend(c.edges[1])])
            .collect();
        for (k, edge) in edges.iter().enumerate() {
            for other in super::relief::corner_bends(&state, edge.panel, bend(k)) {
                if other.feature != uid && !pairs.contains(&[bend(k), other]) {
                    pairs.push([bend(k), other]);
                }
            }
        }
        state = super::relief::flange_corner_reliefs(ctx, state, &rules, &pairs)?;
    }
    // Walls that still meet: the refusal paints them, as the input sheet
    // has them, and their corner (mitcad#1877).
    let uid = ctx.uid;
    let meeting = |walls: &[SheetDomainRef]| -> Vec<NoticePlace> {
        let edge_of = |domain: &SheetDomainRef| {
            let slot = match domain {
                SheetDomainRef::Panel(p) if p.feature == uid => Some(p.slot as usize),
                SheetDomainRef::Bend(b) if b.feature == uid => Some(b.slot as usize),
                _ => None,
            };
            slot.filter(|k| *k < edges.len())
        };
        let mut places: Vec<NoticePlace> = walls
            .iter()
            .map(|domain| match edge_of(domain) {
                Some(k) => edge_place(edges[k]),
                None => domain_place(*domain),
            })
            .collect();
        let named = places.clone();
        for k in walls.iter().filter_map(edge_of) {
            for rule in &ends[k] {
                if named.iter().all(|p| rule.places.contains(p)) {
                    places.extend(rule.places.iter().cloned());
                }
            }
        }
        places
    };
    publish_placed(ctx, state, &rules, None, &meeting).map_err(|e| {
        if e.contains("pass through each other") || e.contains("touch over a face") {
            FeatureError::from_text(&e)
                .suffixed(
                    ". Make the Corner Gap larger, or change the flange's angle, length or \
                     position so that its walls clear each other at the corners.",
                )
                .text()
        } else {
            e
        }
    })
}

#[cfg(test)]
#[path = "flange_edges_tests.rs"]
mod tests;
