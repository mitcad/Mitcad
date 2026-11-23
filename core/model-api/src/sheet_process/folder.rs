// SPDX-License-Identifier: MIT
//! The blanks of the FEFCO 04 folder boxes (mitcad#2226; commands.md
//! "Standard boxes"): a base panel with walls on its sides, double walls
//! turned over into the box with locking tabs in slots, dust and corner
//! flaps, and a hinged lid with its tuck or front flap and ears.
//!
//! A blank is a tree of panels on the base. Each panel is a rectangle in a
//! frame of its own: `u` across from its crease (0) to its free or far edge
//! (`depth`), `v` along the crease over its `range`. A child's crease lies
//! half a crease strip (`h`, half the allowance) out from its parent's edge,
//! on a tongue of the parent as wide as the crease, and the child is as
//! wide as its crease: past a crease's ends no material lies within its
//! strip, and the parent's edge beside the tongue becomes the seam's line
//! when it folds (as the slots of the slotted boxes reach half a strip into
//! their band). A crease spans exactly its panel's root, so that along the
//! fold's stationary tangent line the parent's material is the tongue's
//! cut alone (a parent edge on that line within the crease would make more
//! stretches there than the strip has). A panel with side children is half
//! a seam gap wider at its root on each such side, out to just past its
//! strip: the seams of those children, which lie on its body's sides, then
//! fall inside the band that folds with it rather than along the band's
//! edge, where the fold's cut would meet them edge on and lose them. A
//! child on the far edge runs on in its parent's frame; one
//! on a side edge (`Low` at the range's start, `High` at its end) turns a
//! quarter, its range then measured along its parent's `u`.
//!
//! Where a crease lies follows from where its panel lies in the folded box,
//! as in [`super::blank`]: a point `d` past a crease lies `d - delta` from
//! the other panel's inside face (`delta` = `h - r`), so two creases with an
//! inside span `s` between them lie `s + 2 delta` apart. Sheets that lie
//! against each other keep a seam gap `g` ([`seam_gap`]); a double wall's
//! layers lie a thickness and two gaps apart, the flaps between them.
//!
//! The folds run leaves first, so that each crease lies on the flat
//! remainder when it folds, in an order that keeps every earlier seam behind
//! the fold's near tangent or beside its line's extent, and every other flat
//! panel out of its strip, its moving side and the lines through its ends
//! (bends.rs `fold_panel`).

use super::{Blank, BlankFold, BoxSizes, BoxStyle, QUARTER, dedup, seam_gap};

/// A frame of the blank's plane: a point at `u`, `v` is `o + u u + v v`.
#[derive(Debug, Clone, Copy)]
struct Frame {
    o: [f64; 2],
    u: [f64; 2],
    v: [f64; 2],
}

impl Frame {
    fn at(&self, u: f64, v: f64) -> [f64; 2] {
        [
            self.o[0] + u * self.u[0] + v * self.v[0],
            self.o[1] + u * self.u[1] + v * self.v[1],
        ]
    }

    /// A point of the plane in this frame: (u, v).
    fn local(&self, p: [f64; 2]) -> [f64; 2] {
        let d = [p[0] - self.o[0], p[1] - self.o[1]];
        [
            d[0] * self.u[0] + d[1] * self.u[1],
            d[0] * self.v[0] + d[1] * self.v[1],
        ]
    }

    /// The rectangle `u0..u1` by `v0..v1` of this frame in the plane:
    /// [x0, x1, y0, y1].
    fn rect(&self, u0: f64, u1: f64, v0: f64, v1: f64) -> [f64; 4] {
        let (a, b) = (self.at(u0, v0), self.at(u1, v1));
        [
            a[0].min(b[0]),
            a[0].max(b[0]),
            a[1].min(b[1]),
            a[1].max(b[1]),
        ]
    }

    /// The `u` and `v` ranges of a rectangle of the plane in this frame.
    fn span(&self, r: [f64; 4]) -> ([f64; 2], [f64; 2]) {
        let (a, b) = (self.local([r[0], r[2]]), self.local([r[1], r[3]]));
        (
            [a[0].min(b[0]), a[0].max(b[0])],
            [a[1].min(b[1]), a[1].max(b[1])],
        )
    }
}

/// Where a child panel hangs on its parent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    /// On the far edge, along the same `v`.
    Far,
    /// On the side edge at the start of the parent's range.
    Low,
    /// On the side edge at its end.
    High,
}

/// A panel of a folder blank: its depth across from its crease, its range
/// along it (a child on a side edge: along its parent's `u`; on a far edge:
/// along its parent's `v`; on the base: along the base's edge, in the
/// blank's x or y), the chamfer of its far corners, its locking tabs on the
/// far edge (`v` ranges) and how far they reach, its holes (`u0, u1, v0,
/// v1`) and its children.
#[derive(Debug, Clone)]
struct Panel {
    name: &'static str,
    depth: f64,
    range: [f64; 2],
    chamfer: f64,
    tabs: Vec<[f64; 2]>,
    tab_depth: f64,
    holes: Vec<[f64; 4]>,
    children: Vec<(Side, Panel)>,
}

impl Panel {
    fn new(name: &'static str, depth: f64, range: [f64; 2]) -> Self {
        Self {
            name,
            depth,
            range,
            chamfer: 0.0,
            tabs: Vec::new(),
            tab_depth: 0.0,
            holes: Vec::new(),
            children: Vec::new(),
        }
    }

    fn with(mut self, side: Side, child: Panel) -> Self {
        self.children.push((side, child));
        self
    }

    fn chamfered(mut self, chamfer: f64) -> Self {
        self.chamfer = chamfer;
        self
    }
}

/// The base's edges, counter-clockwise from the bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    Bottom,
    Right,
    Top,
    Left,
}

/// The base: its rectangle (inside its walls' tongues), its slots
/// ([x0, x1, y0, y1]) and its walls on each edge (ranges along the edge,
/// in the blank's x or y).
#[derive(Debug, Clone)]
struct Base {
    x: [f64; 2],
    y: [f64; 2],
    holes: Vec<[f64; 4]>,
    walls: Vec<(Edge, Panel)>,
}

/// A panel laid out: its parent (None: the base) and children, its frame
/// and range there, its depth and its flat material (body, tabs and the
/// parent's tongue that folds away into its strip).
#[derive(Debug, Clone)]
struct Placed {
    name: &'static str,
    parent: Option<usize>,
    children: Vec<usize>,
    frame: Frame,
    range: [f64; 2],
    depth: f64,
    material: Vec<[f64; 4]>,
}

/// The blank laid out: its panels, every rectangle of flat material with
/// its panel (None: the base) and its holes.
#[derive(Debug, Default)]
struct Layout {
    h: f64,
    /// How much wider a panel with side children is at its root on each
    /// such side (half a seam gap), and how far past its crease that
    /// shoulder reaches (just past its strip, short of its side children):
    /// its crease then spans its whole root, ending at its material as
    /// every crease does, and its side children's seams lie inside the
    /// band that folds with it, not along the band's edge, where the fold's
    /// cut would meet them edge on and lose them.
    shoulder: f64,
    shoulder_to: f64,
    placed: Vec<Placed>,
    rects: Vec<(Option<usize>, &'static str, [f64; 4])>,
    holes: Vec<Vec<[f64; 2]>>,
}

impl Layout {
    fn hole(&mut self, f: &Frame, r: [f64; 4]) {
        let [u0, u1, v0, v1] = r;
        self.holes
            .push(vec![f.at(u0, v0), f.at(u1, v0), f.at(u1, v1), f.at(u0, v1)]);
    }

    /// A tongue of `owner` (None: the base) for the child placed next.
    fn tongue(&mut self, owner: Option<usize>, name: &'static str, r: [f64; 4]) -> [f64; 4] {
        self.rects.push((owner, name, r));
        r
    }

    /// How much wider `p`'s root is than its body at the start and end of
    /// its own range: a shoulder on each side with side children.
    fn widen(&self, p: &Panel) -> [f64; 2] {
        let on = |side: Side| {
            if p.children.iter().any(|(s, _)| *s == side) {
                self.shoulder
            } else {
                0.0
            }
        };
        [on(Side::Low), on(Side::High)]
    }

    /// Lays out `p` in `f` with its body over `range` (along its own `v`),
    /// hung on `parent` with the parent's `tongue`, and returns its outline
    /// from its crease's start round to its end. Its crease spans its root:
    /// the body and its shoulders.
    fn walk(
        &mut self,
        p: &Panel,
        f: Frame,
        range: [f64; 2],
        parent: Option<usize>,
        tongue: [f64; 4],
    ) -> Vec<[f64; 2]> {
        let h = self.h;
        let [lo, hi] = range;
        let [wide_lo, wide_hi] = self.widen(p);
        let depth = p.depth;
        let me = self.placed.len();
        let mut material = vec![f.rect(0.0, depth, lo, hi)];
        if wide_lo > 0.0 {
            material.push(f.rect(0.0, self.shoulder_to, lo - wide_lo, lo));
        }
        if wide_hi > 0.0 {
            material.push(f.rect(0.0, self.shoulder_to, hi, hi + wide_hi));
        }
        for t in &p.tabs {
            material.push(f.rect(depth, depth + p.tab_depth, t[0], t[1]));
        }
        for r in &material {
            self.rects.push((Some(me), p.name, *r));
        }
        material.push(tongue);
        self.placed.push(Placed {
            name: p.name,
            parent,
            children: Vec::new(),
            frame: f,
            // The crease: the root, body and shoulders.
            range: [lo - wide_lo, hi + wide_hi],
            depth,
            material,
        });
        if let Some(parent) = parent {
            self.placed[parent].children.push(me);
        }
        for r in &p.holes {
            self.hole(&f, *r);
        }
        let of = |side: Side| {
            p.children
                .iter()
                .filter(move |(s, _)| *s == side)
                .map(|(_, c)| c)
        };
        let mut low: Vec<&Panel> = of(Side::Low).collect();
        low.sort_by(|a, b| a.range[0].total_cmp(&b.range[0]));
        let mut high: Vec<&Panel> = of(Side::High).collect();
        high.sort_by(|a, b| b.range[1].total_cmp(&a.range[1]));
        // The far edge: children and tabs by where they start.
        let mut far: Vec<(f64, f64, Option<&Panel>)> = of(Side::Far)
            .map(|c| (c.range[0], c.range[1], Some(c)))
            .collect();
        far.extend(p.tabs.iter().map(|t| (t[0], t[1], None)));
        far.sort_by(|a, b| a.0.total_cmp(&b.0));
        // A chamfer keeps clear of the panel's own strip.
        let chamfer = p.chamfer.min(depth - h).min((hi - lo) / 3.0).max(0.0);
        let mut pts = vec![f.at(0.0, lo - wide_lo)];
        if wide_lo > 0.0 {
            pts.push(f.at(self.shoulder_to, lo - wide_lo));
            pts.push(f.at(self.shoulder_to, lo));
        }
        for c in low {
            // A child's crease along this side: its root, in this `u`.
            let [w0, w1] = self.widen(c);
            let [u0, u1] = [c.range[0] - w0, c.range[1] + w1];
            pts.push(f.at(u0, lo));
            let tongue = self.tongue(Some(me), p.name, f.rect(u0, u1, lo - h, lo));
            let frame = Frame {
                o: f.at(0.0, lo - h),
                u: [-f.v[0], -f.v[1]],
                v: f.u,
            };
            pts.extend(self.walk(c, frame, c.range, Some(me), tongue));
            pts.push(f.at(u1, lo));
        }
        if chamfer > 0.0 {
            pts.push(f.at(depth - chamfer, lo));
            pts.push(f.at(depth, lo + chamfer));
        } else {
            pts.push(f.at(depth, lo));
        }
        for (v0, v1, child) in far {
            match child {
                Some(c) => {
                    let [w0, w1] = self.widen(c);
                    let [v0, v1] = [v0 - w0, v1 + w1];
                    pts.push(f.at(depth, v0));
                    let tongue = self.tongue(Some(me), p.name, f.rect(depth, depth + h, v0, v1));
                    let frame = Frame {
                        o: f.at(depth + h, 0.0),
                        ..f
                    };
                    pts.extend(self.walk(c, frame, c.range, Some(me), tongue));
                    pts.push(f.at(depth, v1));
                }
                None => {
                    pts.push(f.at(depth, v0));
                    pts.push(f.at(depth + p.tab_depth, v0));
                    pts.push(f.at(depth + p.tab_depth, v1));
                    pts.push(f.at(depth, v1));
                }
            }
        }
        if chamfer > 0.0 {
            pts.push(f.at(depth, hi - chamfer));
            pts.push(f.at(depth - chamfer, hi));
        } else {
            pts.push(f.at(depth, hi));
        }
        for c in high {
            // Its own range runs against this `u`: its start's shoulder at
            // the `u1` end.
            let [w0, w1] = self.widen(c);
            let [u0, u1] = [c.range[0] - w1, c.range[1] + w0];
            pts.push(f.at(u1, hi));
            let tongue = self.tongue(Some(me), p.name, f.rect(u0, u1, hi, hi + h));
            let frame = Frame {
                o: f.at(0.0, hi + h),
                u: f.v,
                v: [-f.u[0], -f.u[1]],
            };
            let body = [-c.range[1], -c.range[0]];
            pts.extend(self.walk(c, frame, body, Some(me), tongue));
            pts.push(f.at(u0, hi));
        }
        if wide_hi > 0.0 {
            pts.push(f.at(self.shoulder_to, hi));
            pts.push(f.at(self.shoulder_to, hi + wide_hi));
        }
        pts.push(f.at(0.0, hi + wide_hi));
        pts
    }

    /// Lays out the base and its walls; returns the outline,
    /// counter-clockwise from the base's bottom left corner.
    fn base(&mut self, b: &Base) -> Vec<[f64; 2]> {
        let h = self.h;
        let ([x0, x1], [y0, y1]) = (b.x, b.y);
        self.rects.push((None, "base", [x0, x1, y0, y1]));
        for r in &b.holes {
            self.holes
                .push(vec![[r[0], r[2]], [r[1], r[2]], [r[1], r[3]], [r[0], r[3]]]);
        }
        let on = |edge: Edge| {
            let mut walls: Vec<&Panel> = b
                .walls
                .iter()
                .filter(|(e, _)| *e == edge)
                .map(|(_, p)| p)
                .collect();
            // Along the walk: up the range on the bottom and right edges,
            // down it on the top and left ones.
            walls.sort_by(|a, b| a.range[0].total_cmp(&b.range[0]));
            if matches!(edge, Edge::Top | Edge::Left) {
                walls.reverse();
            }
            walls
        };
        let mut pts = vec![[x0, y0]];
        // Each wall's crease spans its root: its range and shoulders (on a
        // top or left wall its own range runs against the blank's).
        for p in on(Edge::Bottom) {
            let [w0, w1] = self.widen(p);
            let [lo, hi] = [p.range[0] - w0, p.range[1] + w1];
            pts.push([lo, y0]);
            let tongue = self.tongue(None, "base", [lo, hi, y0 - h, y0]);
            let frame = Frame {
                o: [0.0, y0 - h],
                u: [0.0, -1.0],
                v: [1.0, 0.0],
            };
            pts.extend(self.walk(p, frame, p.range, None, tongue));
            pts.push([hi, y0]);
        }
        pts.push([x1, y0]);
        for p in on(Edge::Right) {
            let [w0, w1] = self.widen(p);
            let [lo, hi] = [p.range[0] - w0, p.range[1] + w1];
            pts.push([x1, lo]);
            let tongue = self.tongue(None, "base", [x1, x1 + h, lo, hi]);
            let frame = Frame {
                o: [x1 + h, 0.0],
                u: [1.0, 0.0],
                v: [0.0, 1.0],
            };
            pts.extend(self.walk(p, frame, p.range, None, tongue));
            pts.push([x1, hi]);
        }
        pts.push([x1, y1]);
        for p in on(Edge::Top) {
            let [w0, w1] = self.widen(p);
            let [lo, hi] = [p.range[0] - w1, p.range[1] + w0];
            pts.push([hi, y1]);
            let tongue = self.tongue(None, "base", [lo, hi, y1, y1 + h]);
            let frame = Frame {
                o: [0.0, y1 + h],
                u: [0.0, 1.0],
                v: [-1.0, 0.0],
            };
            let body = [-p.range[1], -p.range[0]];
            pts.extend(self.walk(p, frame, body, None, tongue));
            pts.push([lo, y1]);
        }
        pts.push([x0, y1]);
        for p in on(Edge::Left) {
            let [w0, w1] = self.widen(p);
            let [lo, hi] = [p.range[0] - w1, p.range[1] + w0];
            pts.push([x0, hi]);
            let tongue = self.tongue(None, "base", [x0 - h, x0, lo, hi]);
            let frame = Frame {
                o: [x0 - h, 0.0],
                u: [-1.0, 0.0],
                v: [0.0, -1.0],
            };
            let body = [-p.range[1], -p.range[0]];
            pts.extend(self.walk(p, frame, body, None, tongue));
            pts.push([x0, lo]);
        }
        pts
    }

    /// Whether `j` hangs, through its parents, on `k`.
    fn below(&self, j: usize, k: usize) -> bool {
        let mut at = self.placed[j].parent;
        while let Some(p) = at {
            if p == k {
                return true;
            }
            at = self.placed[p].parent;
        }
        false
    }

    /// The panels in the order they fold, where nothing below decides in
    /// the order they were placed, each after its children: every panel
    /// after the panels on it; a panel whose seam would lie past another's
    /// near tangent along that one's line after it; a panel whose flat
    /// material lies in another's strip, moving side or the lines through
    /// its ends before it. None where no order does.
    fn order(&self) -> Option<Vec<usize>> {
        let h = self.h;
        let n = self.placed.len();
        let mut post = Vec::with_capacity(n);
        fn visit(l: &Layout, i: usize, post: &mut Vec<usize>) {
            for &c in &l.placed[i].children {
                visit(l, c, post);
            }
            post.push(i);
        }
        for (i, p) in self.placed.iter().enumerate() {
            if p.parent.is_none() {
                visit(self, i, &mut post);
            }
        }
        // before[k]: the panels that fold before k.
        let mut before = vec![Vec::new(); n];
        for k in 0..n {
            let fk = &self.placed[k];
            // The band that folds: the crease's extent, the panel's root.
            let [lo, hi] = fk.range;
            for j in 0..n {
                if j == k || self.below(k, j) {
                    continue;
                }
                if self.below(j, k) {
                    before[k].push(j);
                    continue;
                }
                let fj = &self.placed[j];
                // j's seam, half a strip back from its crease on its parent.
                let a = fk.frame.local(fj.frame.at(-h, fj.range[0]));
                let b = fk.frame.local(fj.frame.at(-h, fj.range[1]));
                let beside = a[1].max(b[1]) < lo - 1e-6 || a[1].min(b[1]) > hi + 1e-6;
                if a[0].max(b[0]) > -h + 1e-6 && !beside {
                    before[j].push(k);
                }
                let in_way = fj.material.iter().any(|r| {
                    let (u, v) = fk.frame.span(*r);
                    u[1] > -h + 1e-6 && v[0] <= hi + 1e-5 && v[1] >= lo - 1e-5
                });
                if in_way {
                    before[k].push(j);
                }
            }
        }
        let rank: Vec<usize> = {
            let mut rank = vec![0; n];
            for (r, &i) in post.iter().enumerate() {
                rank[i] = r;
            }
            rank
        };
        let mut done = vec![false; n];
        let mut out = Vec::with_capacity(n);
        while out.len() < n {
            let next = (0..n)
                .filter(|&i| !done[i] && before[i].iter().all(|&j| done[j]))
                .min_by_key(|&i| rank[i])?;
            done[next] = true;
            out.push(next);
        }
        Some(out)
    }
}

/// The tabs of a wall's free edge over `range`: `n` of them, the range in
/// `2 n + 1` equal parts, the tabs on every second from the first.
fn tabs(range: [f64; 2], n: u32) -> Vec<[f64; 2]> {
    let part = (range[1] - range[0]) / f64::from(2 * n + 1);
    (0..n)
        .map(|k| {
            let k = f64::from(k);
            [
                range[0] + (2.0 * k + 1.0) * part,
                range[0] + (2.0 * k + 2.0) * part,
            ]
        })
        .collect()
}

/// The sizes a folder works with.
#[derive(Debug, Clone, Copy)]
struct Measures {
    l: f64,
    w: f64,
    h_box: f64,
    t: f64,
    r: f64,
    /// Half a crease strip.
    h: f64,
    /// How far a crease lies past the inside face of the panel it folds
    /// against, less the radius: `h - r`.
    d: f64,
    g: f64,
    /// The gap between a double wall's layers.
    s: f64,
    /// The height of a double wall's top strip's inside face.
    zs: f64,
    tuck: f64,
    tab_depth: f64,
}

/// The FEFCO 0421 and 0427 folders: the base W by L, the front and back
/// walls on its long sides with dust flaps at both ends that fold between
/// the layers of the double end walls, the end walls turned over with
/// locking tabs into slots in the base, the lid on the back wall with its
/// tuck; the 0427's lid with ears inside the end walls and its front flap
/// with ears inside those.
fn mailer(m: &Measures, ears: bool, n: u32) -> Base {
    let Measures {
        l,
        w,
        h_box,
        t,
        r,
        h,
        d,
        g,
        s,
        zs,
        tuck,
        tab_depth,
    } = *m;
    // The outer end walls' inside faces 2 t + 2 g out from the inside.
    let outer = 2.0 * t + 2.0 * g;
    let bx = w + 2.0 * d;
    let by = l + 2.0 * outer + 2.0 * d;
    // The walls end at their dust flaps' creases, whose flaps lie between
    // the end walls' layers: t + g out from the inside.
    let side = [t + g + h, by - t - g - h];
    let dust = |name| Panel::new(name, w / 2.0 - t + d, [h + g, zs - r - g + d]);
    let front = Panel::new("front wall", h_box + d, side)
        .with(Side::Low, dust("dust flap"))
        .with(Side::High, dust("dust flap"));
    let lid_depth = w - t - g + 2.0 * d - h;
    let lid_range = if ears {
        // The ears' creases where the ears lie t + g in from the end walls.
        [3.0 * t + 3.0 * g + h, by - 3.0 * t - 3.0 * g - h]
    } else {
        side
    };
    let tuck_range = if ears {
        // The front flap's ears another t + g in.
        [4.0 * t + 4.0 * g + h, by - 4.0 * t - 4.0 * g - h]
    } else {
        // The tuck a gap and a radius in from the end walls.
        [2.0 * t + 3.0 * g + h, by - 2.0 * t - 3.0 * g - h]
    };
    let mut tuck_panel = Panel::new(
        if ears { "front flap" } else { "tuck" },
        tuck + d,
        tuck_range,
    );
    let mut lid = Panel::new("lid", lid_depth, lid_range);
    if ears {
        let ear = || {
            Panel::new("front flap ear", w / 2.0 - t - g + d, [h + g, tuck + d])
                .chamfered((w / 2.0 - t - g + d).min(tuck) / 3.0)
        };
        tuck_panel = tuck_panel.with(Side::Low, ear()).with(Side::High, ear());
        let lid_ear = || {
            Panel::new("lid ear", h_box - r - g + d, [h + g, lid_depth])
                .chamfered((h_box - r - g + d).min(lid_depth - h - g) / 3.0)
        };
        lid = lid.with(Side::Low, lid_ear()).with(Side::High, lid_ear());
    } else {
        let width = tuck_range[1] - tuck_range[0];
        tuck_panel = tuck_panel.chamfered(tuck.min(width / 4.0) / 2.0);
    }
    let back = Panel::new("back wall", h_box + 2.0 * d - h, side)
        .with(Side::Low, dust("dust flap"))
        .with(Side::High, dust("dust flap"))
        .with(Side::Far, lid.with(Side::Far, tuck_panel));
    let mut holes = Vec::new();
    let end = |top: bool, holes: &mut Vec<[f64; 4]>| {
        // The end walls as wide as the inside less a radius and a gap at
        // each side, clear of the walls' bends.
        let range = [h + g, bx - h - g];
        let own = if top { [-range[1], -range[0]] } else { range };
        let mut inner = Panel::new("inner end wall", zs - g + d, own);
        inner.tabs = tabs(own, n);
        inner.tab_depth = tab_depth;
        // The slots: the tabs and a gap round them, through the base under
        // the inner wall (from t + g out to g in from the inside).
        for tab in &inner.tabs {
            let x = if top { [-tab[1], -tab[0]] } else { *tab };
            let y = [t + g + d, 2.0 * t + 3.0 * g + d];
            let y = if top { [by - y[1], by - y[0]] } else { y };
            holes.push([x[0] - g, x[1] + g, y[0], y[1]]);
        }
        let strip = Panel::new("end wall top", s + 2.0 * d - h, own).with(Side::Far, inner);
        Panel::new("end wall", zs + 2.0 * d - h, range).with(Side::Far, strip)
    };
    let bottom = end(false, &mut holes);
    let top = end(true, &mut holes);
    Base {
        x: [h, bx - h],
        y: [h, by - h],
        holes,
        walls: vec![
            (Edge::Left, front),
            (Edge::Right, back),
            (Edge::Bottom, bottom),
            (Edge::Top, top),
        ],
    }
}

/// The FEFCO 0426 folder: the front wall turned over with locking tabs
/// into slots in the base, the end walls with corner flaps that fold
/// between the front wall's layers and inside the back wall, the lid on the
/// back wall with ears inside the end walls and its front flap.
fn front_double(m: &Measures, n: u32) -> Base {
    let Measures {
        l,
        w,
        h_box,
        t,
        r,
        h,
        d,
        g,
        s,
        zs,
        tuck,
        tab_depth,
    } = *m;
    // The outer front wall's inside face 2 t + 2 g out from the inside.
    let outer = 2.0 * t + 2.0 * g;
    let bx = w + outer + 2.0 * d;
    let by = l + 2.0 * d;
    // The inner wall and its top a radius and a gap in from the end walls.
    let inner_range = [-(by - h - g), -(h + g)];
    let mut inner = Panel::new("inner front wall", zs - g + d, inner_range);
    inner.tabs = tabs(inner_range, n);
    inner.tab_depth = tab_depth;
    let holes: Vec<[f64; 4]> = inner
        .tabs
        .iter()
        .map(|tab| [t + g + d, 2.0 * t + 3.0 * g + d, -tab[1] - g, -tab[0] + g])
        .collect();
    let front = Panel::new("front wall", zs + 2.0 * d - h, [h, by - h]).with(
        Side::Far,
        Panel::new("front wall top", s + 2.0 * d - h, inner_range).with(Side::Far, inner),
    );
    let lid_depth = w - t - g + 2.0 * d - h;
    let ear = || {
        Panel::new("lid ear", h_box - r - g + d, [t + 2.0 * g + h, lid_depth])
            .chamfered((h_box - r - g + d).min(lid_depth - t - 2.0 * g - h) / 3.0)
    };
    let tuck_range = [t + 2.0 * g + h, by - t - 2.0 * g - h];
    let flap = Panel::new("front flap", tuck + d, tuck_range)
        .chamfered(tuck.min((tuck_range[1] - tuck_range[0]) / 2.0) / 3.0);
    let lid = Panel::new("lid", lid_depth, [g + t + h, by - g - t - h])
        .with(Side::Low, ear())
        .with(Side::High, ear())
        .with(Side::Far, flap);
    let back = Panel::new("back wall", h_box + 2.0 * d - h, [h, by - h]).with(Side::Far, lid);
    let corner = (h_box.min(l / 2.0 - t)) + d;
    let end = |top: bool| {
        let to_front = Panel::new("corner flap", corner, [h + g, zs - r - g + d]);
        let to_back = Panel::new("corner flap", corner, [h + g, h_box - r - g + d]);
        let (low, high) = if top {
            (to_back, to_front)
        } else {
            (to_front, to_back)
        };
        Panel::new("end wall", h_box - g + d, [t + g + h, bx - t - g - h])
            .with(Side::Low, low)
            .with(Side::High, high)
    };
    Base {
        x: [h, bx - h],
        y: [h, by - h],
        holes,
        walls: vec![
            (Edge::Left, front),
            (Edge::Right, back),
            (Edge::Bottom, end(false)),
            (Edge::Top, end(true)),
        ],
    }
}

/// The FEFCO 0470 and 0471 folders: the end walls on the back wall's ends,
/// each with a floor flap on the base and turned over with a locking tab
/// into a slot in the floor flap, the front wall with end flaps that fold
/// between the end walls' layers, the lid on the back wall with its tuck;
/// the 0471's lid with a front flap with ears inside the end walls.
fn back_ends(m: &Measures, front_flap: bool, n: u32) -> Base {
    let Measures {
        l,
        w,
        h_box,
        t,
        r,
        h,
        d,
        g,
        s,
        zs,
        tuck,
        tab_depth,
    } = *m;
    let bx = w + 2.0 * d;
    // The base under the end walls to their outside faces, 3 t + 2 g out.
    let by = l + 6.0 * t + 4.0 * g;
    // The front wall's end flaps lie between the end walls' layers.
    let flap_crease = 2.0 * t + g - d;
    let end_flap = || Panel::new("end flap", w - r - g + d, [t + 2.0 * g + h, zs - r - g + d]);
    let front = Panel::new(
        "front wall",
        h_box + d,
        [flap_crease + h, by - flap_crease - h],
    )
    .with(Side::Low, end_flap())
    .with(Side::High, end_flap());
    // The end walls' creases on the back wall's ends.
    let end_crease = t - d;
    let back_range = [end_crease + h, by - end_crease - h];
    let span = [h + g, w - r - g + d];
    let end = |top: bool| {
        let own = if top { span } else { [-span[1], -span[0]] };
        let mut inner = Panel::new("inner end wall", zs - t - 2.0 * g + d, own);
        inner.tabs = tabs(own, n);
        inner.tab_depth = tab_depth;
        // The floor flap clear of the base's front bend and the front flap,
        // and in the blank a thickness clear of the base.
        let floor_range = [(h + g).max(2.0 * t - d), w - t - 2.0 * g - r + d];
        let mut floor = Panel::new("floor flap", h_box.min(w).min(l / 2.0) + d, floor_range);
        // Its slots under the inner wall's tabs: the tab and a gap round
        // it, from t + g out to g in from the inside.
        for tab in &inner.tabs {
            // Along the floor flap the other way from the inner wall at
            // either end (the floor flap and the top hang on opposite
            // sides of the end wall).
            let along = [-tab[1] - g, -tab[0] + g];
            floor
                .holes
                .push([t + g + d, 2.0 * t + 3.0 * g + d, along[0], along[1]]);
        }
        let strip = Panel::new("end wall top", s + 2.0 * d - h, span).with(Side::Far, inner);
        let wall = Panel::new("end wall", w + t + d, [t + g + h, zs + 2.0 * d - h]);
        if top {
            wall.with(Side::High, floor).with(Side::Low, strip)
        } else {
            wall.with(Side::Low, floor).with(Side::High, strip)
        }
    };
    let lid_depth = w - t - g + 2.0 * d - h;
    let closing = if front_flap {
        let range = [4.0 * t + 3.0 * g + r, by - 4.0 * t - 3.0 * g - r];
        // The ears clear, in the blank, of the inner end walls and their
        // tabs past the lid's crease.
        let inner_far = -t - g + (s + 2.0 * d) + (zs - t - 2.0 * g + d) + tab_depth;
        let ear_from = (h + g).max(inner_far - (lid_depth + h) + t);
        let ear_to = (h_box - t - 2.0 * g - r + d).min(tuck + d);
        let ear = || {
            Panel::new("front flap ear", w / 2.0 - t - g + d, [ear_from, ear_to])
                .chamfered((w / 2.0 - t - g + d).min(ear_to - ear_from) / 3.0)
        };
        Panel::new("front flap", tuck + d, range)
            .with(Side::Low, ear())
            .with(Side::High, ear())
    } else {
        let range = [3.0 * t + 3.0 * g + r, by - 3.0 * t - 3.0 * g - r];
        Panel::new("tuck", tuck + d, range).chamfered(tuck.min((range[1] - range[0]) / 4.0) / 2.0)
    };
    let back = Panel::new("back wall", h_box + 2.0 * d - h, back_range)
        .with(Side::Low, end(false))
        .with(Side::High, end(true))
        .with(
            Side::Far,
            Panel::new("lid", lid_depth, back_range).with(Side::Far, closing),
        );
    Base {
        x: [h, bx - h],
        y: [0.0, by],
        holes: Vec::new(),
        walls: vec![(Edge::Left, front), (Edge::Right, back)],
    }
}

/// The blank of a FEFCO 04 folder style (mitcad#2226), or why its sizes
/// make none.
pub(super) fn blank(style: BoxStyle, s: &BoxSizes) -> Result<Blank, String> {
    let (t, r) = (s.thickness, s.radius);
    let h = s.allowance / 2.0;
    let g = seam_gap(t);
    let m = Measures {
        l: s.length,
        w: s.width,
        h_box: s.height,
        t,
        r,
        h,
        d: h - r,
        g,
        s: t + 2.0 * g,
        zs: s.height - t - g,
        tuck: s.tuck,
        tab_depth: g + t / 2.0,
    };
    // Each tab at least twice the board's thickness wide, along the turned
    // over wall (the end walls; the 0426's front wall).
    let wall = if style == BoxStyle::Fefco0426 {
        s.length
    } else {
        s.width
    } - 2.0 * r
        - 2.0 * g;
    if s.tabs > 0 && wall / f64::from(2 * s.tabs + 1) < 2.0 * t {
        let most = ((wall / (2.0 * t) - 1.0) / 2.0).floor().max(0.0);
        return Err(format!(
            "the turned over walls take at most {most} locking tabs twice the board's \
             thickness wide, got {}",
            s.tabs
        ));
    }
    let base = match style {
        BoxStyle::Fefco0421 => mailer(&m, false, s.tabs),
        BoxStyle::Fefco0427 => mailer(&m, true, s.tabs),
        BoxStyle::Fefco0426 => front_double(&m, s.tabs),
        BoxStyle::Fefco0470 => back_ends(&m, false, s.tabs),
        BoxStyle::Fefco0471 => back_ends(&m, true, s.tabs),
        _ => return Err(format!("{style:?} is not a folder style")),
    };
    let mut layout = Layout {
        h,
        shoulder: g / 2.0,
        shoulder_to: h + g / 2.0,
        ..Layout::default()
    };
    let mut outline = layout.base(&base);
    // Every panel needs room past its strip and shoulders.
    for p in &layout.placed {
        if p.range[1] - p.range[0] < h || p.depth < layout.shoulder_to + 1e-3 {
            return Err(format!(
                "the box is too small for its board: its {} has no room",
                p.name
            ));
        }
    }
    // No two pieces of the blank may lie over each other.
    for (i, (_, a_name, a)) in layout.rects.iter().enumerate() {
        for (_, b_name, b) in &layout.rects[i + 1..] {
            let eps = 1e-7;
            if a[0] < b[1] - eps && b[0] < a[1] - eps && a[2] < b[3] - eps && b[2] < a[3] - eps {
                return Err(format!(
                    "the box's blank runs onto itself at these sizes (its {a_name} and \
                     {b_name}): make it lower or wider"
                ));
            }
        }
    }
    let order = layout
        .order()
        .ok_or_else(|| "no fold order suits the box's blank at these sizes".to_owned())?;
    dedup(&mut outline);
    let folds = order
        .iter()
        .map(|&i| {
            let p = &layout.placed[i];
            BlankFold {
                a: p.frame.at(0.0, p.range[0]),
                b: p.frame.at(0.0, p.range[1]),
                stationary_left: true,
                angle: QUARTER,
            }
        })
        .collect();
    Ok(Blank {
        outline,
        holes: layout.holes,
        folds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// C flute board (4 mm, deduction 4, radius 0.5: each crease develops
    /// 5, h = 2.5, delta = 2, the seam gap 0.4), inside 300 x 200 x `height`.
    fn sizes(height: f64) -> BoxSizes {
        BoxSizes {
            length: 300.0,
            width: 200.0,
            height,
            thickness: 4.0,
            radius: 0.5,
            allowance: 5.0,
            glue_flap: 30.0,
            tuck: 20.0,
            tabs: 2,
        }
    }

    fn area(points: &[[f64; 2]]) -> f64 {
        let n = points.len();
        (0..n)
            .map(|i| {
                let (a, b) = (points[i], points[(i + 1) % n]);
                a[0] * b[1] - b[0] * a[1]
            })
            .sum::<f64>()
            / 2.0
    }

    fn on_outline(outline: &[[f64; 2]], p: [f64; 2]) -> bool {
        let n = outline.len();
        (0..n).any(|i| {
            let (a, c) = (outline[i], outline[(i + 1) % n]);
            let cross = (c[0] - a[0]) * (p[1] - a[1]) - (c[1] - a[1]) * (p[0] - a[0]);
            let within =
                (p[0] - a[0]) * (p[0] - c[0]) <= 1e-9 && (p[1] - a[1]) * (p[1] - c[1]) <= 1e-9;
            cross.abs() < 1e-6 && within
        })
    }

    /// Whether `p` lies inside the closed outline (even-odd).
    fn inside(outline: &[[f64; 2]], p: [f64; 2]) -> bool {
        let n = outline.len();
        let mut odd = false;
        for i in 0..n {
            let (a, b) = (outline[i], outline[(i + 1) % n]);
            if (a[1] > p[1]) != (b[1] > p[1]) {
                let x = a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
                if x > p[0] {
                    odd = !odd;
                }
            }
        }
        odd
    }

    /// No two edges of the closed outline but neighbours meet.
    fn simple(outline: &[[f64; 2]]) -> bool {
        let n = outline.len();
        let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
            (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
        };
        let meet = |p: [f64; 2], q: [f64; 2], r: [f64; 2], s: [f64; 2]| {
            let boxed = p[0].min(q[0]) <= r[0].max(s[0]) + 1e-9
                && r[0].min(s[0]) <= p[0].max(q[0]) + 1e-9
                && p[1].min(q[1]) <= r[1].max(s[1]) + 1e-9
                && r[1].min(s[1]) <= p[1].max(q[1]) + 1e-9;
            boxed
                && cross(r, s, p) * cross(r, s, q) <= 1e-12
                && cross(p, q, r) * cross(p, q, s) <= 1e-12
        };
        (0..n).all(|i| {
            (i + 2..n).all(|j| {
                (i == 0 && j == n - 1)
                    || !meet(
                        outline[i],
                        outline[(i + 1) % n],
                        outline[j],
                        outline[(j + 1) % n],
                    )
            })
        })
    }

    fn bounds(outline: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
        outline.iter().fold(
            ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]),
            |(lo, hi), p| {
                (
                    [lo[0].min(p[0]), lo[1].min(p[1])],
                    [hi[0].max(p[0]), hi[1].max(p[1])],
                )
            },
        )
    }

    /// A folder blank's shape: a simple counter-clockwise outline, every
    /// crease's ends on it, every slot inside it, square folds.
    fn check(b: &Blank, folds: usize, slots: usize) {
        assert!(area(&b.outline) > 0.0, "counter-clockwise");
        assert!(simple(&b.outline), "{:?}", b.outline);
        assert_eq!(b.folds.len(), folds);
        for fold in &b.folds {
            assert!(
                on_outline(&b.outline, fold.a) && on_outline(&b.outline, fold.b),
                "{fold:?}"
            );
            assert!(fold.stationary_left && fold.angle == QUARTER);
        }
        assert_eq!(b.holes.len(), slots);
        for hole in &b.holes {
            for p in hole {
                assert!(inside(&b.outline, *p), "{hole:?}");
            }
        }
    }

    /// The line of a crease from `a` to `to`, as the blank has it.
    fn crease(b: &Blank, a: [f64; 2], to: [f64; 2]) -> bool {
        b.folds.iter().any(|f| near(f.a, a) && near(f.b, to))
    }

    /// Where in the folding order the crease from `a` comes.
    fn at(b: &Blank, a: [f64; 2]) -> usize {
        b.folds.iter().position(|f| near(f.a, a)).unwrap()
    }

    fn near(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
    }

    /// FEFCO 0421 (mitcad#2226), inside 300 x 200 x 100: the base 200 +
    /// 2 delta = 204 wide between the front and back walls' creases (x = 0,
    /// 204), 300 + 2 (2 t + 2 g) + 2 delta = 321.6 long between the end
    /// walls' (y = 0, 321.6); the back wall H + 2 delta = 104 to the lid's
    /// crease (308), the lid W - t - g + 2 delta = 199.6 to the tuck's
    /// (507.6), the tuck 20 + delta deep; each end wall zs + 2 delta = 99.6
    /// (zs = H - t - g) to its top's crease, the top s + 2 delta = 8.8 (s =
    /// t + 2 g) to the inner wall's, the inner wall zs - g + delta = 97.2 to
    /// its free edge and its tabs g + t / 2 = 2.4 further: 14 folds, two
    /// tabs at each end and their slots.
    #[test]
    fn a_0421_blank_turns_its_end_walls_over() {
        let b = blank(BoxStyle::Fefco0421, &sizes(100.0)).unwrap();
        check(&b, 14, 4);
        assert!(crease(&b, [204.0, 6.7], [204.0, 314.9]), "{:?}", b.folds);
        assert!(crease(&b, [308.0, 6.9], [308.0, 314.7]), "{:?}", b.folds);
        assert!(crease(&b, [507.6, 11.7], [507.6, 309.9]), "{:?}", b.folds);
        assert!(crease(&b, [0.0, 314.9], [0.0, 6.7]), "{:?}", b.folds);
        assert!(crease(&b, [2.9, 0.0], [201.1, 0.0]), "{:?}", b.folds);
        assert!(crease(&b, [2.9, -99.6], [201.1, -99.6]), "{:?}", b.folds);
        assert!(crease(&b, [2.9, -108.4], [201.1, -108.4]), "{:?}", b.folds);
        // The dust flaps' creases t + g = 4.4 in from the end walls'.
        assert!(crease(&b, [206.9, 4.4], [300.7, 4.4]), "{:?}", b.folds);
        let (lo, hi) = bounds(&b.outline);
        assert!(
            near(lo, [-102.0, -208.0]) && near(hi, [529.6, 529.6]),
            "{lo:?} {hi:?}"
        );
        // The leaves fold first: each end wall's inner wall before its top
        // and the top before the end wall; the lid's tuck before the lid,
        // the lid before the back wall.
        assert!(at(&b, [2.9, -108.4]) < at(&b, [2.9, -99.6]));
        assert!(at(&b, [2.9, -99.6]) < at(&b, [2.9, 0.0]));
        assert!(at(&b, [507.6, 11.7]) < at(&b, [308.0, 6.9]));
        assert!(at(&b, [308.0, 6.9]) < at(&b, [204.0, 6.7]));
    }

    /// FEFCO 0427: the 0421 with ears on the lid (their creases 3 t + 3 g
    /// = 13.2 in from the end walls', the ears t + g inside them) and a
    /// front flap H - t = 96 deep with ears another t + g in: 18 folds,
    /// the ears before the panels they hang on.
    #[test]
    fn a_0427_blank_has_ears_on_its_lid_and_front_flap() {
        let s = BoxSizes {
            tuck: 96.0,
            ..sizes(100.0)
        };
        let b = blank(BoxStyle::Fefco0427, &s).unwrap();
        check(&b, 18, 4);
        assert!(crease(&b, [308.0, 15.5], [308.0, 306.1]), "{:?}", b.folds);
        assert!(crease(&b, [310.9, 13.2], [505.1, 13.2]), "{:?}", b.folds);
        assert!(crease(&b, [510.5, 17.6], [605.6, 17.6]), "{:?}", b.folds);
        let (lo, hi) = bounds(&b.outline);
        assert!(
            near(lo, [-102.0, -208.0]) && near(hi, [605.6, 529.6]),
            "{lo:?} {hi:?}"
        );
        assert!(at(&b, [510.5, 17.6]) < at(&b, [507.6, 19.9]));
        assert!(at(&b, [310.9, 13.2]) < at(&b, [308.0, 15.5]));
    }

    /// FEFCO 0426, inside 300 x 200 x 80: the front wall turned over (its
    /// outer layer 2 t + 2 g = 8.8 out, the base 200 + 8.8 + 2 delta =
    /// 212.8 wide), three tabs into slots along the front of the base, the
    /// end walls with corner flaps, the lid's front flap H - t = 76 deep.
    #[test]
    fn a_0426_blank_turns_its_front_wall_over() {
        let s = BoxSizes {
            tuck: 76.0,
            tabs: 3,
            ..sizes(80.0)
        };
        let b = blank(BoxStyle::Fefco0426, &s).unwrap();
        check(&b, 14, 3);
        let (lo, hi) = bounds(&b.outline);
        assert!(
            near(lo, [-168.0, -81.6]) && near(hi, [574.4, 385.6]),
            "{lo:?} {hi:?}"
        );
        // The back wall's crease at x = 212.8, the lid's 84 further.
        assert!(
            b.folds.iter().any(|f| near(f.a, [212.8, 2.5])),
            "{:?}",
            b.folds
        );
        assert!(
            b.folds.iter().any(|f| (f.a[0] - 296.8).abs() < 1e-6),
            "{:?}",
            b.folds
        );
    }

    /// FEFCO 0470 and 0471, inside 300 x 200 x 150: the end walls on the
    /// back wall's ends, W + t + delta = 206 long down from their creases
    /// at y = t - delta = 2, each with a floor flap and a slot in it for the
    /// inner wall's tab; the 0471's front flap H - t = 146 deep with ears.
    #[test]
    fn a_0470_blank_hangs_its_end_walls_on_the_back_wall() {
        let s = BoxSizes {
            tabs: 1,
            ..sizes(150.0)
        };
        let b = blank(BoxStyle::Fefco0470, &s).unwrap();
        check(&b, 14, 2);
        let (lo, hi) = bounds(&b.outline);
        assert!(
            near(lo, [-152.0, -204.0]) && near(hi, [579.6, 529.6]),
            "{lo:?} {hi:?}"
        );
        let s = BoxSizes { tuck: 146.0, ..s };
        let b = blank(BoxStyle::Fefco0471, &s).unwrap();
        check(&b, 16, 2);
        let (lo, hi) = bounds(&b.outline);
        assert!(
            near(lo, [-152.0, -204.0]) && near(hi, [705.6, 529.6]),
            "{lo:?} {hi:?}"
        );
    }

    /// Every style in a range of sizes and boards makes a blank of its
    /// shape, the walls' tabs as many as asked.
    #[test]
    fn folder_blanks_hold_their_shape_over_sizes_and_boards() {
        let boards = [
            (4.0, 0.5, 4.0),
            (4.0, 0.5, 0.4),
            (0.5, 0.0625, 0.5),
            (7.0, 0.875, 7.0),
        ];
        let styles = [
            (BoxStyle::Fefco0421, 14),
            (BoxStyle::Fefco0426, 14),
            (BoxStyle::Fefco0427, 18),
            (BoxStyle::Fefco0470, 14),
            (BoxStyle::Fefco0471, 16),
        ];
        for (t, r, deduction) in boards {
            for (length, width, height) in [
                (60.0, 60.0, 60.0),
                (400.0, 150.0, 60.0),
                (160.0, 90.0, 250.0),
            ] {
                for (style, folds) in styles {
                    let tabs = 1;
                    let s = BoxSizes {
                        length,
                        width,
                        height,
                        thickness: t,
                        radius: r,
                        allowance: 2.0 * (r + t) - deduction,
                        glue_flap: 30.0,
                        tuck: if style.front_flap() {
                            height - t
                        } else {
                            (height / 3.0).min(20.0)
                        },
                        tabs,
                    };
                    let b = blank(style, &s).unwrap_or_else(|e| panic!("{style:?} {s:?}: {e}"));
                    let walls = if style == BoxStyle::Fefco0426 { 1 } else { 2 };
                    check(&b, folds, walls);
                }
            }
        }
    }

    /// More tabs than a wall takes at twice the board's thickness each are
    /// refused in words.
    #[test]
    fn too_many_tabs_are_refused() {
        let s = BoxSizes {
            width: 100.0,
            tabs: 12,
            ..sizes(100.0)
        };
        let error = blank(BoxStyle::Fefco0421, &s).unwrap_err();
        assert!(error.contains("at most 5 locking tabs"), "{error}");
    }
}
