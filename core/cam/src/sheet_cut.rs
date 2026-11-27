// SPDX-License-Identifier: MIT
//! Cutting a nested sheet with a plasma torch on a grbl 1.1 controller
//! (mitcad#2553; docs/cam.md, "Sheet cutting").
//!
//! The model gives the beam's paths on the sheet, each a closed loop of
//! one part, the material on its left (an outer boundary counter-clockwise,
//! a hole clockwise) and already moved half the kerf away from the
//! material; this module adds nothing to them. A loop's edges are lines
//! and arcs (mitcad#2570): the arcs and circles of the parts stay arcs
//! about their centres and are cut as G2 and G3; other curves come as the
//! lines of their points and are cut as G1. [`plan`] puts them in the
//! order they are cut and gives each a pierce point and a lead-in;
//! [`moves`] gives the torch's moves and [`post_grbl_plasma`] writes them
//! as grbl 1.1's G-code.
//!
//! - **Order:** a loop is cut only after every loop that lies inside it,
//!   so a part's holes go before its outer boundary and a part lying in
//!   another part's hole is cut before that hole (once a loop is cut, what
//!   it holds may drop or move). Of the loops that may be cut, the next is
//!   the one nearest the torch, a loop of the part being cut first.
//! - **Direction:** each loop is cut with the part on the torch's right
//!   (an outer boundary clockwise, a hole counter-clockwise), the side a
//!   plasma arc's usual clockwise swirl cuts square.
//! - **Lead-in:** the torch pierces in the scrap, off the middle of the
//!   loop's edge nearest to it (an arc's middle, a circle's point opposite
//!   its start), along the edge's normal, `lead_in` away or
//!   half the way to the next path the normal meets or to the sheet's edge
//!   when that is nearer (a small hole is pierced in its middle), and runs
//!   straight onto the loop.
//! - **Heights:** across at the travel height, down to the pierce height,
//!   the torch on (`M3`), the pierce delay (`G4 P`), down to the cut height,
//!   the lead-in and the loop at the cut feed, the torch off (`M5`), up to
//!   the travel height. The work zero is the sheet's lower left corner on
//!   its top surface.
//! - **Arcs:** an arc edge is written as `G2` or `G3` with its centre as
//!   `I` and `J` from its start (a whole circle as two halves, split where
//!   the torch comes on to it); an arc shorter than the program can tell
//!   from a point is written as the line between its ends.

use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use crate::CamError;
use crate::post::{CommentStyle, EXPERIMENTAL_NOTICE, Output, PostSettings, Units, number};
use crate::toolpath::{Move, P3, arc_sweep, cut_as_arc};

/// A point on the sheet (mm).
pub type P2 = [f64; 2];

/// The spindle speed word that switches the torch on: grbl's spindle
/// enable output drives the torch's relay, and its default top speed
/// (`$30`) is 1000, so a speed of 1000 sets the PWM output full on too.
pub const TORCH_ON: &str = "M3 S1000";

/// The torch's settings of a cut sheet (kept with it in the design).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PlasmaSettings {
    /// The cut feed (mm/min).
    pub feed: f64,
    /// The torch's height above the sheet while it pierces (mm).
    pub pierce_height: f64,
    /// How long the torch waits at the pierce height after it fires (s).
    pub pierce_delay: f64,
    /// The torch's height above the sheet while it cuts (mm).
    pub cut_height: f64,
    /// The height the torch moves across at (mm).
    pub travel_height: f64,
    /// The longest lead-in (mm).
    pub lead_in: f64,
    /// The program's units.
    pub units: Units,
    /// N words on every line.
    pub line_numbers: bool,
}

impl Default for PlasmaSettings {
    fn default() -> Self {
        PlasmaSettings {
            feed: 2000.0,
            pierce_height: 3.8,
            pierce_delay: 0.5,
            cut_height: 1.5,
            travel_height: 10.0,
            lead_in: 4.0,
            units: Units::Millimetres,
            line_numbers: false,
        }
    }
}

impl PlasmaSettings {
    /// Why the settings cannot cut, in words, if they cannot.
    pub fn check(&self) -> Result<(), CamError> {
        let finite = [
            self.feed,
            self.pierce_height,
            self.pierce_delay,
            self.cut_height,
            self.travel_height,
            self.lead_in,
        ]
        .iter()
        .all(|v| v.is_finite());
        if !finite {
            return Err(CamError::new("Every cutting setting needs a number."));
        }
        if self.feed <= 0.0 || self.feed > 100_000.0 {
            return Err(CamError::new(
                "The cut feed must be more than 0 and at most 100000 mm/min.",
            ));
        }
        if self.cut_height <= 0.0 {
            return Err(CamError::new(
                "The cut height must be above the sheet: more than 0 mm.",
            ));
        }
        if self.pierce_height < self.cut_height {
            return Err(CamError::new(
                "The pierce height must not be below the cut height.",
            ));
        }
        if self.travel_height <= self.pierce_height {
            return Err(CamError::new(
                "The travel height must be above the pierce height.",
            ));
        }
        if self.travel_height > 1000.0 {
            return Err(CamError::new("The travel height must be at most 1000 mm."));
        }
        if !(0.0..=60.0).contains(&self.pierce_delay) {
            return Err(CamError::new(
                "The pierce delay must be from 0 to 60 seconds.",
            ));
        }
        if !(0.0..=100.0).contains(&self.lead_in) {
            return Err(CamError::new("The lead-in must be from 0 to 100 mm."));
        }
        Ok(())
    }
}

/// An edge of a loop that is an arc (mitcad#2570).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct EdgeArc {
    /// Its centre.
    pub center: P2,
    /// Counter-clockwise from the edge's start to its end; a whole turn
    /// when they are one point.
    pub ccw: bool,
}

impl EdgeArc {
    /// The same arc run the other way.
    fn reversed(self) -> Self {
        EdgeArc {
            center: self.center,
            ccw: !self.ccw,
        }
    }
}

/// A loop the beam follows: the beam's path (kerf compensation applied),
/// closed, the material on its left.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SheetLoop {
    /// The part's id on the cut sheet.
    pub part: u32,
    /// An outer boundary, or a hole.
    pub outer: bool,
    /// Its corners: edge `i` runs from `points[i]` to the next point, the
    /// last edge back to the first point (which is not repeated). A circle
    /// is one point and one arc edge.
    pub points: Vec<P2>,
    /// The arc edges: `arcs[i]` for the edge from `points[i]`; an edge
    /// without an entry, or with `None`, is a line.
    pub arcs: Vec<Option<EdgeArc>>,
}

impl SheetLoop {
    /// The arc of edge `i`, if it is one.
    pub fn arc(&self, i: usize) -> Option<EdgeArc> {
        self.arcs.get(i).copied().flatten()
    }

    fn edge(&self, i: usize) -> (P2, P2, Option<EdgeArc>) {
        let n = self.points.len();
        (self.points[i], self.points[(i + 1) % n], self.arc(i))
    }

    /// Its points, the first not repeated at the end, arcs as lines within
    /// [`FLATTEN`] with each arc's middle one of the points.
    pub fn polygon(&self) -> Vec<P2> {
        let mut out = Vec::with_capacity(self.points.len());
        for i in 0..self.points.len() {
            let (a, b, arc) = self.edge(i);
            out.push(a);
            between(a, b, arc, &mut out);
        }
        out
    }
}

/// How far the lines that stand for arcs where the order and the lead-ins
/// are worked out may lie from them (mm).
pub const FLATTEN: f64 = 0.005;

/// An arc edge's sweep (radians, counter-clockwise positive).
fn sweep(a: P2, b: P2, arc: EdgeArc) -> f64 {
    arc_sweep(a, b, arc.center, arc.ccw)
}

/// An edge's length.
fn edge_length(a: P2, b: P2, arc: Option<EdgeArc>) -> f64 {
    match arc {
        None => dist(a, b),
        Some(c) => dist(a, c.center) * sweep(a, b, c).abs(),
    }
}

/// The point a fraction `t` along an edge.
fn edge_point(a: P2, b: P2, arc: Option<EdgeArc>, t: f64) -> P2 {
    match arc {
        None => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t],
        Some(c) => {
            let r = dist(a, c.center);
            let angle = (a[1] - c.center[1]).atan2(a[0] - c.center[0]) + sweep(a, b, c) * t;
            [c.center[0] + r * angle.cos(), c.center[1] + r * angle.sin()]
        }
    }
}

/// The unit direction an edge runs in at its middle.
fn middle_direction(a: P2, b: P2, arc: Option<EdgeArc>) -> P2 {
    match arc {
        None => {
            let l = dist(a, b);
            [(b[0] - a[0]) / l, (b[1] - a[1]) / l]
        }
        Some(c) => {
            let m = edge_point(a, b, arc, 0.5);
            let r = dist(m, c.center);
            let u = [(m[0] - c.center[0]) / r, (m[1] - c.center[1]) / r];
            if c.ccw { [-u[1], u[0]] } else { [u[1], -u[0]] }
        }
    }
}

/// The points of an arc edge between its ends, an even number of steps
/// within [`FLATTEN`] (so that its middle is one of them); none for a
/// line.
fn between(a: P2, b: P2, arc: Option<EdgeArc>, out: &mut Vec<P2>) {
    let Some(c) = arc else {
        return;
    };
    let r = dist(a, c.center);
    let step = if r > FLATTEN {
        2.0 * (1.0 - FLATTEN / r).acos()
    } else {
        PI
    }
    .min(10f64.to_radians());
    let mut n = ((sweep(a, b, c).abs() / step).ceil() as usize).max(2);
    n += n % 2;
    for i in 1..n {
        out.push(edge_point(a, b, arc, i as f64 / n as f64));
    }
}

/// One loop as it is cut.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlannedCut {
    pub part: u32,
    pub outer: bool,
    /// Where the torch pierces.
    pub pierce: P2,
    /// Where the lead-in meets the loop: the cut starts and ends here.
    pub start: P2,
    /// The loop from `start` round to `start` again, in the direction it
    /// is cut (the part on the right): the ends of its edges.
    pub path: Vec<P2>,
    /// The arc edges, one entry for each edge (`path[i]` to
    /// `path[i + 1]`): cut as G2 or G3; `None` is a line (G1).
    pub arcs: Vec<Option<EdgeArc>>,
}

impl PlannedCut {
    /// The lead-in's length (mm).
    pub fn lead_in(&self) -> f64 {
        dist(self.pierce, self.start)
    }

    fn edge(&self, i: usize) -> (P2, P2, Option<EdgeArc>) {
        (
            self.path[i],
            self.path[i + 1],
            self.arcs.get(i).copied().flatten(),
        )
    }

    /// The loop's length (mm), the lead-in not counted.
    pub fn length(&self) -> f64 {
        (0..self.path.len().saturating_sub(1))
            .map(|i| {
                let (a, b, arc) = self.edge(i);
                edge_length(a, b, arc)
            })
            .sum()
    }

    /// How many of its edges are arcs.
    pub fn arc_count(&self) -> usize {
        self.arcs.iter().filter(|a| a.is_some()).count()
    }

    /// Its points from `start` round to `start`, arcs as lines within
    /// [`FLATTEN`] (for drawing it).
    pub fn points(&self) -> Vec<P2> {
        let mut out = Vec::with_capacity(self.path.len());
        for i in 0..self.path.len().saturating_sub(1) {
            let (a, b, arc) = self.edge(i);
            out.push(a);
            between(a, b, arc, &mut out);
        }
        out.extend(self.path.last());
        out
    }
}

/// The loops of a sheet in the order they are cut.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct SheetCutPlan {
    pub cuts: Vec<PlannedCut>,
}

impl SheetCutPlan {
    /// How far the torch cuts (mm): every loop and its lead-in.
    pub fn cut_length(&self) -> f64 {
        self.cuts.iter().map(|c| c.length() + c.lead_in()).sum()
    }

    /// How far the torch moves across with the arc off (mm), from the
    /// work zero to the first pierce, between loops and back to the work
    /// zero.
    pub fn travel_length(&self) -> f64 {
        let mut at = [0.0, 0.0];
        let mut total = 0.0;
        for c in &self.cuts {
            total += dist(at, c.pierce);
            at = c.start;
        }
        total + dist(at, [0.0, 0.0])
    }

    /// The time the arc is on (s): the cuts at the feed and the pierce
    /// delays. The moves across depend on the machine's rapid rate and
    /// are not counted.
    pub fn arc_time(&self, settings: &PlasmaSettings) -> f64 {
        self.cut_length() / settings.feed * 60.0 + settings.pierce_delay * self.cuts.len() as f64
    }
}

fn sub(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}

fn dist(a: P2, b: P2) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn cross(a: P2, b: P2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn signed_area(points: &[P2]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| cross(points[i], points[(i + 1) % n]))
        .sum::<f64>()
        / 2.0
}

fn bounds(points: &[P2]) -> (P2, P2) {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for p in points {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

/// Whether a point lies inside a closed polygon (even-odd).
fn inside(polygon: &[P2], p: P2) -> bool {
    let n = polygon.len();
    let mut inside = false;
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                inside = !inside;
            }
        }
    }
    inside
}

/// How far a ray from `from` along the unit `dir` goes before it meets a
/// loop's edge (beyond a hair's breadth, so not the edge it starts on),
/// the loops as polygons.
fn ray_hit(polygons: &[Vec<P2>], from: P2, dir: P2) -> f64 {
    const HAIR: f64 = 1e-6;
    let mut nearest = f64::INFINITY;
    for polygon in polygons {
        let n = polygon.len();
        for i in 0..n {
            let (a, b) = (polygon[i], polygon[(i + 1) % n]);
            let e = sub(b, a);
            let den = cross(dir, e);
            if den.abs() < 1e-12 {
                continue;
            }
            let w = sub(a, from);
            let t = cross(w, e) / den;
            let u = cross(w, dir) / den;
            if t > HAIR && (-1e-12..=1.0 + 1e-12).contains(&u) {
                nearest = nearest.min(t);
            }
        }
    }
    nearest
}

/// How far a ray from a point on the sheet goes before it leaves it;
/// infinite for a point off the sheet (no limit is set there).
fn sheet_exit(sheet: P2, from: P2, dir: P2) -> f64 {
    if !(0.0..=sheet[0]).contains(&from[0]) || !(0.0..=sheet[1]).contains(&from[1]) {
        return f64::INFINITY;
    }
    let mut t = f64::INFINITY;
    for k in 0..2 {
        if dir[k] > 1e-12 {
            t = t.min((sheet[k] - from[k]) / dir[k]);
        } else if dir[k] < -1e-12 {
            t = t.min(-from[k] / dir[k]);
        }
    }
    t
}

/// The loops of a sheet `sheet` (width, height; mm) in the order they are
/// cut, each with its pierce point, lead-in and direction (see the
/// module's notes). Loops of no area are left out; the model refuses them
/// before (`invalid_loops`).
pub fn plan(loops: &[SheetLoop], sheet: P2, settings: &PlasmaSettings) -> SheetCutPlan {
    let (loops, polygons): (Vec<&SheetLoop>, Vec<Vec<P2>>) = loops
        .iter()
        .map(|l| (l, l.polygon()))
        .filter(|(_, p)| p.len() >= 3 && signed_area(p).abs() > 1e-9)
        .unzip();
    let n = loops.len();
    let boxes: Vec<(P2, P2)> = polygons.iter().map(|p| bounds(p)).collect();
    let areas: Vec<f64> = polygons.iter().map(|p| signed_area(p).abs()).collect();
    // encloses[i]: the loops lying inside loop i.
    let encloses: Vec<Vec<usize>> = (0..n)
        .map(|i| {
            (0..n)
                .filter(|&j| {
                    j != i
                        && areas[j] < areas[i]
                        && boxes[j].0[0] >= boxes[i].0[0]
                        && boxes[j].0[1] >= boxes[i].0[1]
                        && boxes[j].1[0] <= boxes[i].1[0]
                        && boxes[j].1[1] <= boxes[i].1[1]
                        && polygons[j].iter().any(|p| inside(&polygons[i], *p))
                })
                .collect()
        })
        .collect();
    let mut done = vec![false; n];
    let mut at: P2 = [0.0, 0.0];
    let mut part: Option<u32> = None;
    let mut cuts = Vec::with_capacity(n);
    for _ in 0..n {
        // The nearest edge middle of each loop that may be cut now.
        let mut best: Option<(bool, f64, usize, usize)> = None;
        for i in (0..n).filter(|&i| !done[i] && encloses[i].iter().all(|&j| done[j])) {
            for e in 0..loops[i].points.len() {
                let (a, b, arc) = loops[i].edge(e);
                if edge_length(a, b, arc) <= 1e-9 {
                    continue;
                }
                let middle = edge_point(a, b, arc, 0.5);
                let d = dist(at, middle);
                // A loop of the part being cut goes first.
                let other = part.is_some_and(|p| p != loops[i].part);
                let better = match best {
                    None => true,
                    Some((o, bd, _, _)) => (other, d) < (o, bd),
                };
                if better {
                    best = Some((other, d, i, e));
                }
            }
        }
        let Some((_, _, i, e)) = best else {
            break;
        };
        done[i] = true;
        let l = loops[i];
        let points = &l.points;
        let m = points.len();
        let (a, b, arc) = l.edge(e);
        let start = edge_point(a, b, arc, 0.5);
        // The scrap lies right of the loop's own direction.
        let ahead = middle_direction(a, b, arc);
        let scrap = [ahead[1], -ahead[0]];
        let free = ray_hit(&polygons, start, scrap).min(sheet_exit(sheet, start, scrap));
        let lead = settings.lead_in.min(free / 2.0).max(0.0);
        let pierce = [start[0] + scrap[0] * lead, start[1] + scrap[1] * lead];
        // Against the loop's direction: start, back along the edge's first
        // half to a, the edges before it round to b, and the edge's second
        // half back to the start; each arc run the other way.
        let back = |arc: Option<EdgeArc>| arc.map(EdgeArc::reversed);
        let mut path = Vec::with_capacity(m + 2);
        let mut arcs = Vec::with_capacity(m + 1);
        path.push(start);
        path.push(a);
        arcs.push(back(arc));
        for k in 1..m {
            let j = (e + m - k) % m;
            path.push(points[j]);
            arcs.push(back(l.arc(j)));
        }
        path.push(start);
        arcs.push(back(arc));
        // An arc too short to tell from a point in the program is cut as
        // the line between its ends (at most half its chord off it).
        for (k, arc) in arcs.iter_mut().enumerate() {
            if let Some(c) = *arc
                && !cut_as_arc(path[k], path[k + 1], c.center, c.ccw)
            {
                *arc = None;
            }
        }
        cuts.push(PlannedCut {
            part: l.part,
            outer: l.outer,
            pierce,
            start,
            path,
            arcs,
        });
        at = start;
        part = Some(l.part);
    }
    SheetCutPlan { cuts }
}

/// What a cut does besides its moves, in the order they come.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TorchStep {
    /// The plan's cut of this index begins (with the move to its pierce
    /// point).
    Loop(usize),
    Move(Move),
    /// The torch fires (`M3`).
    On,
    /// The pierce delay (s).
    Dwell(f64),
    /// The torch goes out (`M5`).
    Off,
}

/// The torch's steps along the plan: from the travel height above the
/// work zero to each pierce point, the cut, and back to the work zero.
pub fn steps(plan: &SheetCutPlan, settings: &PlasmaSettings) -> Vec<TorchStep> {
    let s = settings;
    let mut out = Vec::new();
    let high = |p: P2| -> P3 { [p[0], p[1], s.travel_height] };
    for (index, c) in plan.cuts.iter().enumerate() {
        out.push(TorchStep::Loop(index));
        out.push(TorchStep::Move(Move::Rapid { to: high(c.pierce) }));
        out.push(TorchStep::Move(Move::Rapid {
            to: [c.pierce[0], c.pierce[1], s.pierce_height],
        }));
        out.push(TorchStep::On);
        if s.pierce_delay > 0.0 {
            out.push(TorchStep::Dwell(s.pierce_delay));
        }
        let feed = |p: P2, arc: Option<EdgeArc>| {
            let to = [p[0], p[1], s.cut_height];
            TorchStep::Move(match arc {
                Some(arc) => Move::Arc {
                    to,
                    center: arc.center,
                    ccw: arc.ccw,
                    feed: s.feed,
                },
                None => Move::Line { to, feed: s.feed },
            })
        };
        out.push(feed(c.pierce, None));
        if c.lead_in() > 0.0 {
            out.push(feed(c.start, None));
        }
        for (k, p) in c.path.iter().enumerate().skip(1) {
            out.push(feed(*p, c.arcs.get(k - 1).copied().flatten()));
        }
        out.push(TorchStep::Off);
        out.push(TorchStep::Move(Move::Rapid { to: high(c.start) }));
    }
    out.push(TorchStep::Move(Move::Rapid {
        to: [0.0, 0.0, s.travel_height],
    }));
    out
}

/// The torch's moves along the plan ([`steps`] without the torch's
/// switching), from the travel height above the work zero.
pub fn moves(plan: &SheetCutPlan, settings: &PlasmaSettings) -> Vec<Move> {
    steps(plan, settings)
        .into_iter()
        .filter_map(|s| match s {
            TorchStep::Move(m) => Some(m),
            _ => None,
        })
        .collect()
}

/// The plan as grbl 1.1's G-code for a plasma torch switched by the
/// spindle output ([`TORCH_ON`], `M5`): the notice that CAM is highly
/// experimental, `title` and `notes` as comments, `G90 G94 G17`, the
/// units, `G54`, the travel height, the cuts and the end (`M5`, back to
/// the work zero at the travel height, `M30`). Comments lose grbl's
/// realtime characters as the milling posts' do.
pub fn post_grbl_plasma(
    plan: &SheetCutPlan,
    settings: &PlasmaSettings,
    title: &str,
    notes: &[String],
) -> Result<String, CamError> {
    settings.check()?;
    if plan.cuts.is_empty() {
        return Err(CamError::new("There is nothing to cut."));
    }
    let post_settings = PostSettings {
        units: settings.units,
        arcs: true,
        line_numbers: settings.line_numbers,
        comments: true,
        ..PostSettings::default()
    };
    let mut out = Output::new(&post_settings, CommentStyle::Parentheses);
    out.set_stream_commands("?~!");
    for line in EXPERIMENTAL_NOTICE {
        out.notice(line);
    }
    out.comment(title);
    out.comment("Plasma cutting for grbl 1.1: the torch on M3, off M5");
    for note in notes {
        out.comment(note);
    }
    out.line("G90 G94 G17");
    let units = out.units_code();
    out.line(units);
    out.line("G54");
    out.line("M5");
    out.rapid_z(settings.travel_height);
    for step in steps(plan, settings) {
        match step {
            TorchStep::Loop(index) => {
                let c = &plan.cuts[index];
                out.comment(&format!(
                    "Part {}, {}",
                    c.part,
                    if c.outer { "outer boundary" } else { "hole" }
                ));
            }
            TorchStep::Move(m) => out.motion(&m, true, 0.01)?,
            TorchStep::On => out.line(TORCH_ON),
            TorchStep::Dwell(s) => out.line(format!("G4 P{}", number(s, 3))),
            TorchStep::Off => out.line("M5"),
        }
    }
    out.line("M30");
    Ok(out.finish())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::gcode::{GcodeReader, Motion, known};

    fn rectangle(part: u32, lo: P2, hi: P2) -> SheetLoop {
        SheetLoop {
            part,
            outer: true,
            points: vec![lo, [hi[0], lo[1]], hi, [lo[0], hi[1]]],
            arcs: Vec::new(),
        }
    }

    /// A hole: clockwise, the material on its left.
    fn hole(part: u32, center: P2, r: f64) -> SheetLoop {
        let points = (0..36)
            .map(|k| {
                let a = -(k as f64) * std::f64::consts::TAU / 36.0;
                [center[0] + r * a.cos(), center[1] + r * a.sin()]
            })
            .collect();
        SheetLoop {
            part,
            outer: false,
            points,
            arcs: Vec::new(),
        }
    }

    /// Two plates, each with a hole, and a small part lying in the second
    /// plate's large hole.
    fn sheet() -> Vec<SheetLoop> {
        vec![
            rectangle(1, [10.0, 10.0], [110.0, 60.0]),
            hole(1, [30.0, 35.0], 5.0),
            rectangle(2, [130.0, 10.0], [230.0, 90.0]),
            hole(2, [180.0, 50.0], 30.0),
            rectangle(3, [170.0, 40.0], [190.0, 60.0]),
            hole(3, [180.0, 50.0], 2.0),
        ]
    }

    fn index_of(plan: &SheetCutPlan, part: u32, outer: bool) -> usize {
        plan.cuts
            .iter()
            .position(|c| c.part == part && c.outer == outer)
            .unwrap()
    }

    #[test]
    fn holes_and_parts_in_holes_are_cut_before_what_holds_them() {
        let settings = PlasmaSettings::default();
        let plan = plan(&sheet(), [300.0, 200.0], &settings);
        assert_eq!(plan.cuts.len(), 6);
        // Each part's hole before its outer boundary.
        for part in 1..=3 {
            assert!(index_of(&plan, part, false) < index_of(&plan, part, true));
        }
        // The small part lies in the second plate's hole: it is cut, hole
        // and all, before that hole.
        assert!(index_of(&plan, 3, true) < index_of(&plan, 2, false));
        // The first plate first (nearest the work zero), its loops
        // together.
        assert_eq!(plan.cuts[0].part, 1);
        assert_eq!(plan.cuts[1].part, 1);
    }

    #[test]
    fn each_loop_is_cut_with_the_part_on_the_right_from_a_lead_in_in_the_scrap() {
        let settings = PlasmaSettings::default();
        let loops = sheet();
        let plan = plan(&loops, [300.0, 200.0], &settings);
        for c in &plan.cuts {
            // Closed, from the start round to the start.
            assert_eq!(c.path.first(), Some(&c.start));
            assert_eq!(c.path.last(), Some(&c.start));
            // The part on the right: an outer boundary clockwise, a hole
            // counter-clockwise.
            let area = signed_area(&c.path);
            if c.outer {
                assert!(area < 0.0, "{c:?}");
            } else {
                assert!(area > 0.0, "{c:?}");
            }
            // The pierce point in the scrap: outside an outer boundary,
            // inside a hole.
            let own = loops
                .iter()
                .find(|l| l.part == c.part && l.outer == c.outer)
                .unwrap();
            assert_eq!(inside(&own.points, c.pierce), !c.outer, "{c:?}");
            assert!(c.lead_in() <= settings.lead_in + 1e-9);
        }
        // The plates' lead-ins are the whole 4 mm; the 2 mm hole's half
        // the way across it: its pierce at its middle.
        let small = &plan.cuts[index_of(&plan, 3, false)];
        assert!(dist(small.pierce, [180.0, 50.0]) < 0.01, "{small:?}");
        assert!((plan.cuts[index_of(&plan, 1, true)].lead_in() - 4.0).abs() < 1e-9);
    }

    #[test]
    fn a_lead_in_stays_on_the_sheet_and_off_the_neighbours() {
        let settings = PlasmaSettings::default();
        // A plate 1 mm from the sheet's left edge and one 2 mm right of it.
        let loops = vec![
            rectangle(1, [1.0, 1.0], [50.0, 50.0]),
            rectangle(2, [52.0, 1.0], [100.0, 50.0]),
        ];
        let plan = plan(&loops, [200.0, 100.0], &settings);
        for c in &plan.cuts {
            assert!(c.pierce[0] >= 0.0 && c.pierce[1] >= 0.0, "{c:?}");
            assert!(c.lead_in() <= 1.0 + 1e-9, "{c:?}");
        }
    }

    #[test]
    fn the_program_follows_the_plan_and_switches_the_torch_per_loop() {
        let settings = PlasmaSettings::default();
        let plan = plan(&sheet(), [300.0, 200.0], &settings);
        let text = post_grbl_plasma(&plan, &settings, "Sheet1", &["Kerf 1.5 mm".into()]).unwrap();
        for line in EXPERIMENTAL_NOTICE {
            assert!(text.contains(line), "{text}");
        }
        assert_eq!(text.matches(TORCH_ON).count(), 6);
        assert_eq!(text.lines().filter(|l| *l == "M5").count(), 7);
        assert_eq!(text.matches("G4 P0.5").count(), 6);
        assert!(text.lines().last() == Some("M30"));
        assert!(!text.contains('?') && !text.contains('!') && !text.contains('~'));
        // Read back: every cut at the cut height on the plan's paths, every
        // move across at the travel height.
        let mut reader = GcodeReader::new(
            [Some([0.0; 3]), None, None, None, None, None],
            BTreeMap::new(),
        );
        let read = reader.read(&text).unwrap();
        let wanted = moves(&plan, &settings);
        let ends: Vec<P3> = read.iter().filter_map(|m| known(m.to)).collect();
        let mut k = 0;
        for w in &wanted {
            let to = w.to();
            // The rapid that only rises or sinks may be one line; find it.
            let found = ends[k..]
                .iter()
                .position(|e| (0..3).all(|i| (e[i] - to[i]).abs() <= 0.0005 + 1e-9));
            let Some(found) = found else {
                panic!("{w:?} not in the program");
            };
            k += found;
        }
        for m in &read {
            let Some((from, to)) = m.ends() else { continue };
            match m.motion {
                Motion::Feed => {
                    assert!(to[2] <= settings.pierce_height + 1e-9, "{m:?}");
                    assert_eq!(m.feed, Some(settings.feed));
                }
                Motion::Rapid => {
                    // Across only at the travel height.
                    if (from[0] - to[0]).abs() > 1e-9 || (from[1] - to[1]).abs() > 1e-9 {
                        assert!((to[2] - settings.travel_height).abs() < 1e-9, "{m:?}");
                    }
                }
                Motion::Arc { .. } => panic!("no arcs: {m:?}"),
            }
        }
        // In inches every length is converted.
        let inches = PlasmaSettings {
            units: Units::Inches,
            ..settings.clone()
        };
        let text = post_grbl_plasma(&plan, &inches, "Sheet1", &[]).unwrap();
        assert!(text.contains("G20"));
        assert!(text.contains(&format!("Z{}", number(10.0 / 25.4, 4))));
    }

    #[test]
    fn settings_out_of_range_are_refused_in_words() {
        let bad = PlasmaSettings {
            pierce_height: 1.0,
            ..PlasmaSettings::default()
        };
        assert_eq!(
            bad.check().unwrap_err().message,
            "The pierce height must not be below the cut height."
        );
        let bad = PlasmaSettings {
            feed: 0.0,
            ..PlasmaSettings::default()
        };
        assert!(bad.check().is_err());
        assert!(PlasmaSettings::default().check().is_ok());
        let empty = SheetCutPlan::default();
        assert!(post_grbl_plasma(&empty, &PlasmaSettings::default(), "", &[]).is_err());
    }

    /// A plate with rounded corners, counter-clockwise: its lines and its
    /// corners' quarter arcs about their centres.
    fn rounded(part: u32, lo: P2, hi: P2, r: f64) -> SheetLoop {
        let (x0, y0, x1, y1) = (lo[0], lo[1], hi[0], hi[1]);
        let arc = |center: P2| Some(EdgeArc { center, ccw: true });
        SheetLoop {
            part,
            outer: true,
            points: vec![
                [x0 + r, y0],
                [x1 - r, y0],
                [x1, y0 + r],
                [x1, y1 - r],
                [x1 - r, y1],
                [x0 + r, y1],
                [x0, y1 - r],
                [x0, y0 + r],
            ],
            arcs: vec![
                None,
                arc([x1 - r, y0 + r]),
                None,
                arc([x1 - r, y1 - r]),
                None,
                arc([x0 + r, y1 - r]),
                None,
                arc([x0 + r, y0 + r]),
            ],
        }
    }

    /// A round hole: one point and a clockwise whole turn about its centre.
    fn circle(part: u32, center: P2, r: f64) -> SheetLoop {
        SheetLoop {
            part,
            outer: false,
            points: vec![[center[0] + r, center[1]]],
            arcs: vec![Some(EdgeArc { center, ccw: false })],
        }
    }

    /// Arcs and circles are cut as G2 and G3 (mitcad#2570): a plate with
    /// rounded corners and a round hole.
    #[test]
    fn arcs_and_circles_are_cut_as_g2_and_g3() {
        use std::f64::consts::TAU;
        let settings = PlasmaSettings::default();
        let (r, hole_r) = (5.0, 6.0);
        let loops = vec![
            rounded(1, [10.0, 10.0], [70.0, 50.0], r),
            circle(1, [40.0, 30.0], hole_r),
        ];
        let plan = plan(&loops, [100.0, 100.0], &settings);
        assert_eq!(plan.cuts.len(), 2);
        let (hole, plate) = (&plan.cuts[0], &plan.cuts[1]);
        assert!(!hole.outer && plate.outer);
        // The hole: two halves counter-clockwise (the part on the right)
        // from the point opposite its start, the lead-in 4 mm towards its
        // centre.
        let half = Some(EdgeArc {
            center: [40.0, 30.0],
            ccw: true,
        });
        assert_eq!(hole.arcs, vec![half; 2], "{hole:?}");
        assert!(dist(hole.start, [34.0, 30.0]) < 1e-9, "{hole:?}");
        assert!(dist(hole.pierce, [38.0, 30.0]) < 1e-9, "{hole:?}");
        assert!((hole.length() - TAU * hole_r).abs() < 1e-9);
        assert!(signed_area(&hole.points()) > 0.0);
        // The plate clockwise, its corners' arcs clockwise about their
        // centres, pierced outside it.
        assert_eq!(plate.arc_count(), 4, "{plate:?}");
        assert!(plate.arcs.iter().flatten().all(|a| !a.ccw));
        let perimeter = 2.0 * (60.0 + 40.0) - 8.0 * r + TAU * r;
        assert!((plate.length() - perimeter).abs() < 1e-9);
        assert!(signed_area(&plate.points()) < 0.0);
        assert!(!inside(&loops[0].polygon(), plate.pierce));
        assert!((plan.cut_length() - perimeter - TAU * hole_r - 8.0).abs() < 1e-9);

        let text = post_grbl_plasma(&plan, &settings, "Sheet1", &[]).unwrap();
        let count = |code: &str| text.lines().filter(|l| l.starts_with(code)).count();
        assert_eq!(count("G2 "), 4, "{text}");
        assert_eq!(count("G3 "), 2, "{text}");
        // Read back: every arc at the cut height and the feed, about its
        // centre, each of its points on its circle.
        let mut reader = GcodeReader::new(
            [Some([0.0; 3]), None, None, None, None, None],
            BTreeMap::new(),
        );
        let read = reader.read(&text).unwrap();
        let mut arcs = 0;
        for m in &read {
            let Motion::Arc { center, ccw, .. } = m.motion else {
                continue;
            };
            arcs += 1;
            let (from, to) = m.ends().unwrap();
            assert_eq!(m.feed, Some(settings.feed));
            assert!((from[2] - settings.cut_height).abs() < 1e-9, "{m:?}");
            assert!((to[2] - settings.cut_height).abs() < 1e-9, "{m:?}");
            let radius = if ccw { hole_r } else { r };
            for p in m.points(0.001) {
                let off = (p[0] - center[0]).hypot(p[1] - center[1]);
                assert!((off - radius).abs() < 0.002, "{m:?}");
            }
        }
        assert_eq!(arcs, 6);
        // Every move of the plan in the program, in order.
        let ends: Vec<P3> = read.iter().filter_map(|m| known(m.to)).collect();
        let mut k = 0;
        for w in moves(&plan, &settings) {
            let to = w.to();
            let found = ends[k..]
                .iter()
                .position(|e| (0..3).all(|i| (e[i] - to[i]).abs() <= 0.0005 + 1e-9));
            k += found.unwrap_or_else(|| panic!("{w:?} not in the program"));
        }
        // In inches the centre's offsets are converted too.
        let inches = PlasmaSettings {
            units: Units::Inches,
            ..settings.clone()
        };
        let text = post_grbl_plasma(&plan, &inches, "Sheet1", &[]).unwrap();
        assert!(
            text.contains(&format!(" I{} ", number(hole_r / 25.4, 4))),
            "{text}"
        );
    }

    /// An arc shorter than the program can tell from a point is cut as
    /// the line between its ends; a whole circle never is.
    #[test]
    fn a_tiny_arc_is_cut_as_a_line() {
        let settings = PlasmaSettings::default();
        // A square whose corner is rounded by 0.002 mm.
        let r = 0.002;
        let mut plate = rectangle(1, [10.0, 10.0], [30.0, 30.0]);
        plate.points = vec![
            [10.0, 10.0],
            [30.0 - r, 10.0],
            [30.0, 10.0 + r],
            [30.0, 30.0],
            [10.0, 30.0],
        ];
        plate.arcs = vec![
            None,
            Some(EdgeArc {
                center: [30.0 - r, 10.0 + r],
                ccw: true,
            }),
        ];
        let plan = plan(
            &[plate, circle(2, [60.0, 20.0], 0.5)],
            [100.0, 50.0],
            &settings,
        );
        let square = plan.cuts.iter().find(|c| c.part == 1).unwrap();
        assert_eq!(square.arc_count(), 0, "{square:?}");
        let ring = plan.cuts.iter().find(|c| c.part == 2).unwrap();
        assert_eq!(ring.arc_count(), 2, "{ring:?}");
    }
}
