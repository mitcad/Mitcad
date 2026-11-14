// SPDX-License-Identifier: MIT
//! Contour (profile): the tool runs along outlines, beside them or on
//! them, in passes down to the floor. The tool radius compensation is
//! done here: the toolpath is the tool center's (G40 in the program), so
//! every controller cuts the same and the checks see the real path.

use cavalier_contours::polyline::{PlineSource, Polyline};
use cavalier_contours::shape_algorithms::{Shape, ShapeOffsetOptions};
use serde::{Deserialize, Serialize};

use super::entry::{Entry, EntrySpace, enter};
use super::path::{self, Path};
use super::{Depths, Direction, Heights, check_mill};
use crate::CamError;
use crate::geom::Contour;
use crate::tool::{Cutting, Tool};
use crate::toolpath::{PathBuilder, Toolpath};

/// Which side of the outlines the tool cuts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// Outside the closed outlines: cutting a part out.
    Outside,
    /// Inside a closed outline: a hole or an opening.
    Inside,
    /// The tool center on the line (engraving, open outlines).
    On,
}

/// A contour operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContourOp {
    /// The outlines; open ones only with [`Side::On`]. Closed outlines cut
    /// outside one another must not overlap.
    pub contours: Vec<Contour>,
    pub side: Side,
    #[serde(default)]
    pub direction: Direction,
    pub depths: Depths,
    pub heights: Heights,
    /// Material left on the walls for a finishing pass (mm).
    #[serde(default)]
    pub stock_to_leave: f64,
    pub cutting: Cutting,
    /// How the tool goes down at the start of each pass.
    #[serde(default)]
    pub entry: Entry,
    /// Holding tabs: bridges of material left standing on the outlines,
    /// so that a part cut out stays in the stock.
    #[serde(default)]
    pub tabs: Option<Tabs>,
    /// Arcs into and out of the cut, on the waste side, so that the tool
    /// meets the wall tangentially and leaves no mark where it starts.
    /// Closed outlines cut outside or inside only.
    #[serde(default)]
    pub lead: Option<Lead>,
}

/// Where the tabs go along each outline.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum TabLayout {
    /// This many tabs on each outline, evenly spaced.
    Count { count: u32 },
    /// A tab about every `spacing` mm along each outline, at least one.
    Spacing { spacing: f64 },
}

/// Holding tabs of a contour.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tabs {
    pub layout: TabLayout,
    /// The width of the material left standing, along the outline (mm).
    pub width: f64,
    /// How high the tabs stand above the bottom of the cut (mm).
    pub height: f64,
}

impl Tabs {
    pub fn validate(&self, depths: &Depths) -> Result<(), CamError> {
        match self.layout {
            TabLayout::Count { count: 0 } => {
                return Err(CamError::new("Give at least one tab."));
            }
            TabLayout::Spacing { spacing } if !(spacing.is_finite() && spacing > 0.0) => {
                return Err(CamError::new(
                    "The spacing of the tabs must be more than zero.",
                ));
            }
            _ => {}
        }
        if !(self.width.is_finite() && self.width > 0.0) {
            return Err(CamError::new("The tabs must be wider than zero."));
        }
        if !(self.height.is_finite() && self.height > 0.0) {
            return Err(CamError::new("The tabs must be higher than zero."));
        }
        if self.height >= depths.top - depths.bottom {
            return Err(CamError::new(
                "The tabs must be lower than the cut is deep.",
            ));
        }
        Ok(())
    }
}

/// Lead in and out: a straight line of `length` square to the outline,
/// then a quarter circle of `radius` (the tool center's) into the cut;
/// the same mirrored out of it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lead {
    pub radius: f64,
    #[serde(default)]
    pub length: f64,
}

impl Lead {
    pub fn validate(&self) -> Result<(), CamError> {
        if !(self.radius.is_finite() && self.radius > 0.0) {
            return Err(CamError::new("The lead radius must be more than zero."));
        }
        if !(self.length.is_finite() && self.length >= 0.0) {
            return Err(CamError::new("The lead length must be zero or more."));
        }
        Ok(())
    }
}

/// The tool center's loops of a contour operation at one depth, in
/// cutting order and direction.
pub fn contour_loops(op: &ContourOp, tool: &Tool) -> Result<Vec<Contour>, CamError> {
    if op.contours.is_empty() {
        return Err(CamError::new("The contour has no outlines."));
    }
    for c in &op.contours {
        c.validate().map_err(CamError::new)?;
        if !c.closed && op.side != Side::On {
            return Err(CamError::new(
                "An open outline can only be cut with the tool on the line.",
            ));
        }
    }
    if !(op.stock_to_leave.is_finite() && op.stock_to_leave > -tool.radius()) {
        return Err(CamError::new(
            "The stock to leave must not be more negative than the tool's radius.",
        ));
    }
    let d = tool.radius() + op.stock_to_leave;
    let mut loops: Vec<Contour> = match op.side {
        Side::On => op.contours.clone(),
        Side::Outside => {
            let shape = Shape::from_plines(op.contours.iter().map(|c| c.oriented(true).to_pline()));
            // The material grows by `d`: the loops run round it the way
            // that leaves it on their right (climb).
            shape_loops(
                &shape.parallel_offset(-d, &ShapeOffsetOptions::default()),
                true,
            )
        }
        Side::Inside => {
            let mut all = Vec::new();
            for c in &op.contours {
                let shape = Shape::from_plines([c.oriented(true).to_pline()]);
                let found = shape_loops(
                    &shape.parallel_offset(d, &ShapeOffsetOptions::default()),
                    false,
                );
                if found.is_empty() {
                    return Err(CamError::new("The tool does not fit inside an outline."));
                }
                all.extend(found);
            }
            all
        }
    };
    if op.direction == Direction::Conventional {
        loops = loops.iter().map(Contour::reversed).collect();
    }
    Ok(loops)
}

/// The outlines of an offset shape, reversed when `reverse`. An outline
/// that shrank to a point is left out; one that shrank to a line is kept
/// (the tool running along it clears the middle).
pub(crate) fn shape_loops(shape: &Shape<f64>, reverse: bool) -> Vec<Contour> {
    shape
        .ccw_plines
        .iter()
        .chain(shape.cw_plines.iter())
        .filter(|p| p.polyline.vertex_count() > 1 && p.polyline.path_length() > 1e-3)
        .map(|p| pline_contour(&p.polyline, reverse))
        .collect()
}

fn pline_contour(p: &Polyline<f64>, reverse: bool) -> Contour {
    let c = Contour::from_pline(p);
    if reverse && p.vertex_count() > 1 {
        c.reversed()
    } else {
        c
    }
}

/// The toolpath of a contour operation.
pub fn contour(op: &ContourOp, tool: &Tool) -> Result<Toolpath, CamError> {
    check_mill(tool, &op.cutting, &op.depths)?;
    op.heights.validate(op.depths.top)?;
    let levels = op.depths.levels()?;
    let loops = contour_loops(op, tool)?;
    op.entry.validate()?;
    if let Some(t) = &op.tabs {
        t.validate(&op.depths)?;
    }
    if let Some(l) = &op.lead {
        l.validate()?;
    }
    let d = tool.radius() + op.stock_to_leave;
    let feed = op.cutting.feed;
    let plunge = op.cutting.plunge_feed;
    let fits = |c: [f64; 2], radius: f64| clear_of_outlines(op, d, c, radius);
    let space = EntrySpace {
        helix_fits: &fits,
        min_ramp: tool.radius(),
    };
    let tab_top = op.tabs.map(|t| op.depths.bottom + t.height);
    let mut b = PathBuilder::new();
    let mut remaining: Vec<Contour> = loops;
    let mut at = [0.0, 0.0];
    while !remaining.is_empty() {
        // The nearest outline next, started where it is nearest.
        let (i, _) = remaining
            .iter()
            .enumerate()
            .map(|(i, c)| (i, distance_to(c, at)))
            .fold(
                (0, f64::INFINITY),
                |best, x| if x.1 < best.1 { x } else { best },
            );
        let c = remaining.remove(i);
        let c = if c.closed {
            c.started_near(at)
        } else {
            nearer_end_first(c, at)
        };
        let (c, lead) = match op.lead {
            Some(l) if c.closed && op.side != Side::On => {
                // Where the tool is nearest, or else at the middle of the
                // longest side where the leads fit (not into a corner).
                let step = (d * 0.05).clamp(0.005, 0.1);
                let clear = |p: &Path| p.samples(step).iter().all(|&q| fits(q, 0.0));
                let whole = Path::of(&c);
                let mut middles: Vec<(f64, [f64; 2])> = Vec::new();
                let mut along = 0.0;
                for &len in &whole.lens {
                    if let Some(m) = whole.point_at(along + len / 2.0) {
                        middles.push((len, m));
                    }
                    along += len;
                }
                middles.sort_by(|a, b| b.0.total_cmp(&a.0));
                let found = std::iter::once(c.clone())
                    .chain(middles.iter().map(|(_, m)| c.started_near(*m)))
                    .find_map(|cand| {
                        let (lead_in, lead_out) = leads(
                            &Path::of(&cand),
                            &l,
                            op.direction == Direction::Conventional,
                        );
                        (clear(&lead_in) && clear(&lead_out)).then_some((cand, lead_in, lead_out))
                    });
                match found {
                    Some((cand, lead_in, lead_out)) => (cand, Some((lead_in, lead_out))),
                    None => {
                        return Err(CamError::new(
                            "A lead in or out runs into the part; make its radius or length smaller.",
                        ));
                    }
                }
            }
            _ => (c, None),
        };
        let mut cut = Path::of(&c);
        let spans = match &op.tabs {
            Some(t) => tab_spans(&cut, t, tool.diameter)?,
            None => Vec::new(),
        };
        let start = match &lead {
            Some((lead_in, _)) => lead_in.start().unwrap_or(c.vertices[0].pos()),
            None => c.vertices[0].pos(),
        };
        b.retract_to(start, op.heights.clearance);
        b.rapid([start[0], start[1], op.heights.retract]);
        let mut top = op.depths.top;
        for (k, &z) in levels.iter().enumerate() {
            if k > 0 && lead.is_some() {
                // Back to the start of the lead in through the air.
                b.retract_to(start, op.heights.retract);
            }
            let with_tabs = tab_top.is_some_and(|t| z < t - 1e-9) && !spans.is_empty();
            // A ramp stops short of the first tab.
            let ramp_along = if with_tabs {
                cut.slice(0.0, spans[0].0)
            } else {
                cut.clone()
            };
            let entry_path = match &lead {
                Some((lead_in, _)) => lead_in.joined(&ramp_along),
                None => ramp_along,
            };
            enter(&mut b, &op.entry, &entry_path, top, z, plunge, &space);
            if let Some((lead_in, _)) = &lead {
                lead_in.cut(&mut b, z, z, feed);
            }
            match tab_top {
                Some(t) if with_tabs => cut_with_tabs(&mut b, &cut, &spans, z, t, feed, plunge),
                _ => cut.cut(&mut b, z, z, feed),
            }
            if let Some((_, lead_out)) = &lead {
                lead_out.cut(&mut b, z, z, feed);
            }
            if !c.closed {
                // An open outline is cut back and forth (its tabs lie
                // the same both ways).
                cut = cut.reversed();
            }
            top = z;
        }
        let end = b.at().expect("the tool is placed");
        b.rapid([end[0], end[1], op.heights.clearance]);
        at = [end[0], end[1]];
    }
    Ok(b.finish())
}

/// Whether the tool center at `p` with a circle of `radius` round it
/// keeps `d` from the outlines on the side the operation cuts.
fn clear_of_outlines(op: &ContourOp, d: f64, p: [f64; 2], radius: f64) -> bool {
    let inside = op.contours.iter().any(|c| c.closed && c.contains(p));
    let side_ok = match op.side {
        Side::On => return false,
        Side::Outside => !inside,
        Side::Inside => inside,
    };
    side_ok && path::distance_to(&op.contours, p) >= d + radius - 1e-6
}

/// The lead in (ending at the start of `cut`) and the lead out (starting
/// at its end), on the side away from the part: the left of the travel
/// for climb milling, the right for conventional.
fn leads(cut: &Path, lead: &Lead, conventional: bool) -> (Path, Path) {
    let waste = |t: [f64; 2]| {
        if conventional {
            [t[1], -t[0]]
        } else {
            [-t[1], t[0]]
        }
    };
    let r = lead.radius;
    let start = cut.start().unwrap_or([0.0, 0.0]);
    let t = cut.tangent_at(0.0);
    let w = waste(t);
    // The arc turns about a center on the waste side; it runs
    // counter-clockwise when the waste is on the left.
    let ccw = !conventional;
    let c = [start[0] + r * w[0], start[1] + r * w[1]];
    let a = [c[0] - r * t[0], c[1] - r * t[1]];
    let p0 = [a[0] + lead.length * w[0], a[1] + lead.length * w[1]];
    let mut lead_in = Path::default();
    lead_in.line(p0, a);
    lead_in.arc(a, start, c, ccw);
    let end = cut.end().unwrap_or(start);
    let te = cut.tangent_at(cut.length());
    let we = waste(te);
    let ce = [end[0] + r * we[0], end[1] + r * we[1]];
    let bq = [ce[0] + r * te[0], ce[1] + r * te[1]];
    let p1 = [bq[0] + lead.length * we[0], bq[1] + lead.length * we[1]];
    let mut lead_out = Path::default();
    lead_out.arc(end, bq, ce, ccw);
    lead_out.line(bq, p1);
    (lead_in, lead_out)
}

/// Where the tool runs over the tabs along `cut`: `(from, to)` lengths,
/// each the tab's width plus the tool's diameter, centered on the tabs.
pub(crate) fn tab_spans(
    cut: &Path,
    tabs: &Tabs,
    diameter: f64,
) -> Result<Vec<(f64, f64)>, CamError> {
    let total = cut.length();
    let n = match tabs.layout {
        TabLayout::Count { count } => count as usize,
        TabLayout::Spacing { spacing } => ((total / spacing).round() as usize).max(1),
    };
    let span = tabs.width + diameter;
    if n as f64 * span >= total - 1e-9 {
        return Err(CamError::new(
            "The tabs do not fit along an outline; use fewer or narrower tabs.",
        ));
    }
    Ok((0..n)
        .map(|k| {
            let mid = (k as f64 + 0.5) * total / n as f64;
            (mid - span / 2.0, mid + span / 2.0)
        })
        .collect())
}

/// Cuts along `cut` at `z`, up to `tab_top` over the tabs: straight up at
/// the feed before each, straight down at the plunge feed after it.
fn cut_with_tabs(
    b: &mut PathBuilder,
    cut: &Path,
    spans: &[(f64, f64)],
    z: f64,
    tab_top: f64,
    feed: f64,
    plunge: f64,
) {
    let mut from = 0.0;
    for &(a, e) in spans {
        cut.slice(from, a).cut(b, z, z, feed);
        let p = cut.point_at(a).expect("a point on the cut");
        b.line([p[0], p[1], tab_top], feed);
        cut.slice(a, e).cut(b, tab_top, tab_top, feed);
        let q = cut.point_at(e).expect("a point on the cut");
        b.line([q[0], q[1], z], plunge);
        from = e;
    }
    cut.slice(from, cut.length()).cut(b, z, z, feed);
}

fn distance_to(c: &Contour, at: [f64; 2]) -> f64 {
    if c.closed {
        let s = c.started_near(at).vertices[0].pos();
        (s[0] - at[0]).hypot(s[1] - at[1])
    } else {
        let a = c.vertices[0].pos();
        let b = c.vertices[c.vertices.len() - 1].pos();
        (a[0] - at[0])
            .hypot(a[1] - at[1])
            .min((b[0] - at[0]).hypot(b[1] - at[1]))
    }
}

fn nearer_end_first(c: Contour, at: [f64; 2]) -> Contour {
    let a = c.vertices[0].pos();
    let b = c.vertices[c.vertices.len() - 1].pos();
    if (b[0] - at[0]).hypot(b[1] - at[1]) < (a[0] - at[0]).hypot(a[1] - at[1]) {
        c.reversed()
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolpath::Move;

    fn op(contours: Vec<Contour>, side: Side) -> ContourOp {
        ContourOp {
            contours,
            side,
            direction: Direction::Climb,
            depths: Depths {
                top: 0.0,
                bottom: -6.0,
                step_down: 3.0,
            },
            heights: Heights {
                clearance: 10.0,
                retract: 2.0,
            },
            stock_to_leave: 0.0,
            cutting: Cutting {
                spindle_rpm: 18000.0,
                feed: 1000.0,
                plunge_feed: 300.0,
            },
            entry: Entry::Plunge,
            tabs: None,
            lead: None,
        }
    }

    /// Points along every move below `z_above`, arcs and lines sampled
    /// finely.
    fn cut_points(p: &Toolpath, z_above: f64) -> Vec<[f64; 3]> {
        let mut out = Vec::new();
        let mut at = [0.0, 0.0, 100.0];
        for m in &p.moves {
            match m {
                Move::Rapid { .. } => {}
                // Points on the arc itself (a chord between them would
                // stray inside it).
                Move::Arc { .. } => out.extend(
                    crate::toolpath::move_points(at, m, 1e-7)
                        .into_iter()
                        .filter(|q| q[2] < z_above),
                ),
                Move::Line { to, .. } => {
                    let n = ((to[0] - at[0]).hypot(to[1] - at[1]) / 0.05)
                        .ceil()
                        .max(1.0) as usize;
                    for k in 0..=n {
                        let t = k as f64 / n as f64;
                        let s = [
                            at[0] + (to[0] - at[0]) * t,
                            at[1] + (to[1] - at[1]) * t,
                            at[2] + (to[2] - at[2]) * t,
                        ];
                        if s[2] < z_above {
                            out.push(s);
                        }
                    }
                }
            }
            at = m.to();
        }
        out
    }

    /// No point of the cut below the top comes closer to the outlines
    /// than the tool's radius (1e-6 mm allowed), on the waste side.
    fn assert_no_gouge(o: &ContourOp, tool: &Tool, p: &Toolpath) {
        let pts = cut_points(p, o.depths.top);
        assert!(!pts.is_empty());
        for q in pts {
            let d = path::distance_to(&o.contours, [q[0], q[1]]);
            assert!(
                d >= tool.radius() + o.stock_to_leave - 1e-6,
                "the tool comes {d} from the outline at {q:?}"
            );
            let inside = o.contours.iter().any(|c| c.contains([q[0], q[1]]));
            assert_eq!(inside, o.side == Side::Inside, "on the wrong side at {q:?}");
        }
    }

    #[test]
    fn tabs_stand_where_asked_and_the_passes_rise_over_them() {
        let tool = Tool::flat_end_mill(1, 6.0);
        let mut o = op(
            vec![Contour::rectangle([0.0, 0.0], [40.0, 20.0])],
            Side::Outside,
        );
        o.depths = Depths {
            top: 0.0,
            bottom: -6.0,
            step_down: 2.0,
        };
        o.tabs = Some(Tabs {
            layout: TabLayout::Count { count: 4 },
            width: 4.0,
            height: 1.5,
        });
        let p = contour(&o, &tool).unwrap();
        assert_no_gouge(&o, &tool, &p);
        // The loop: 40x20 grown by 3 with round corners.
        let total = 120.0 + std::f64::consts::TAU * 3.0;
        let lp = Path::of(&contour_loops(&o, &tool).unwrap()[0]);
        assert!((lp.length() - total).abs() < 1e-9);
        // The passes at -2 and -4 are above the tabs' top (-4.5), only the
        // last rises: four times to -4.5.
        let tops: Vec<[f64; 3]> = p
            .moves
            .iter()
            .filter_map(|m| match m {
                Move::Line { to, feed } if *feed == 1000.0 && (to[2] + 4.5).abs() < 1e-12 => {
                    Some(*to)
                }
                _ => None,
            })
            .collect();
        assert!(tops.len() >= 4, "{tops:?}");
        // Over each tab the tool stays at the tab's top for the tab's width
        // plus the diameter, centered at a quarter of the loop apart.
        let mut at: [f64; 3] = [0.0; 3];
        let mut over = 0.0;
        let mut rises = 0;
        for m in &p.moves {
            let to = m.to();
            if (at[2] + 4.5).abs() < 1e-12 && (to[2] + 4.5).abs() < 1e-12 {
                over += crate::toolpath::move_length(at, m);
            }
            if (at[2] + 6.0).abs() < 1e-12 && (to[2] + 4.5).abs() < 1e-12 {
                rises += 1;
            }
            at = to;
        }
        assert_eq!(rises, 4);
        assert!((over - 4.0 * 10.0).abs() < 1e-5, "{over}");
        // Nothing of the last pass runs at the bottom inside a tab's span.
        let spans = tab_spans(&lp, o.tabs.as_ref().unwrap(), 6.0).unwrap();
        for (a, e) in &spans {
            assert!((e - a - 10.0).abs() < 1e-12);
        }
        assert!((spans[1].0 + 5.0 - 1.5 * total / 4.0).abs() < 1e-9);
    }

    #[test]
    fn tabs_by_spacing_and_tabs_that_do_not_fit() {
        let tool = Tool::flat_end_mill(1, 6.0);
        let lp = Path::of(&Contour::rectangle([0.0, 0.0], [100.0, 50.0]));
        let t = Tabs {
            layout: TabLayout::Spacing { spacing: 70.0 },
            width: 5.0,
            height: 1.0,
        };
        assert_eq!(tab_spans(&lp, &t, tool.diameter).unwrap().len(), 4);
        let crowded = Tabs {
            layout: TabLayout::Count { count: 30 },
            ..t
        };
        assert!(tab_spans(&lp, &crowded, tool.diameter).is_err());
        let d = Depths {
            top: 0.0,
            bottom: -3.0,
            step_down: 1.0,
        };
        assert!(Tabs { height: 3.0, ..t }.validate(&d).is_err());
        assert!(t.validate(&d).is_ok());
    }

    #[test]
    fn lead_arcs_are_tangent_to_the_cut_and_on_the_waste_side() {
        let tool = Tool::flat_end_mill(1, 6.0);
        for side in [Side::Outside, Side::Inside] {
            for direction in [Direction::Climb, Direction::Conventional] {
                let mut o = op(vec![Contour::rectangle([0.0, 0.0], [40.0, 30.0])], side);
                o.direction = direction;
                o.lead = Some(Lead {
                    radius: 2.0,
                    length: 1.0,
                });
                let loops = contour_loops(&o, &tool).unwrap();
                let cut = Path::of(&loops[0].started_near([20.0, -10.0]));
                let (lead_in, lead_out) = leads(
                    &cut,
                    o.lead.as_ref().unwrap(),
                    direction == Direction::Conventional,
                );
                // The arcs end and start on the cut, going its way there.
                assert_eq!(lead_in.end(), cut.start());
                assert_eq!(lead_out.start(), cut.end());
                let t_in = lead_in.tangent_at(lead_in.length());
                let t_cut = cut.tangent_at(0.0);
                assert!((t_in[0] - t_cut[0]).abs() < 1e-9 && (t_in[1] - t_cut[1]).abs() < 1e-9);
                let t_out = lead_out.tangent_at(0.0);
                let t_end = cut.tangent_at(cut.length());
                assert!((t_out[0] - t_end[0]).abs() < 1e-9 && (t_out[1] - t_end[1]).abs() < 1e-9);
                // A quarter circle of the radius and the straight length.
                assert!((lead_in.length() - (std::f64::consts::PI + 1.0)).abs() < 1e-9);
                let p = contour(&o, &tool).unwrap();
                assert_no_gouge(&o, &tool, &p);
            }
        }
    }

    #[test]
    fn a_lead_into_the_part_is_refused() {
        let tool = Tool::flat_end_mill(1, 6.0);
        // The tool center runs round a circle of radius 5: a lead arc of
        // radius 6 cannot stay inside it.
        let mut o = op(vec![Contour::circle([0.0, 0.0], 8.0)], Side::Inside);
        o.lead = Some(Lead {
            radius: 6.0,
            length: 0.0,
        });
        assert!(contour(&o, &tool).unwrap_err().message.contains("lead"));
    }

    #[test]
    fn a_helix_entry_outside_and_a_ramp_on_an_open_line_keep_off_the_part() {
        let tool = Tool::flat_end_mill(1, 6.0);
        let mut o = op(
            vec![Contour::rectangle([0.0, 0.0], [40.0, 20.0])],
            Side::Outside,
        );
        o.entry = Entry::Helix {
            radius: 2.0,
            angle: 5.0,
        };
        let p = contour(&o, &tool).unwrap();
        assert_no_gouge(&o, &tool, &p);
        let helix_turns = p
            .moves
            .iter()
            .filter(|m| matches!(m, Move::Arc { to, center, .. } if ((center[0] - to[0]).hypot(center[1] - to[1]) - 2.0).abs() < 1e-9))
            .count();
        assert!(helix_turns >= 4, "{helix_turns}");
        // No straight plunge below the top.
        assert!(!p.moves.windows(2).any(|w| {
            let (a, b) = (w[0].to(), w[1].to());
            matches!(w[1], Move::Line { .. })
                && a[0] == b[0]
                && a[1] == b[1]
                && b[2] < -1e-9
                && b[2] < a[2]
        }));
        let mut on = op(
            vec![Contour::open(vec![
                crate::geom::Vertex::line(0.0, 0.0),
                crate::geom::Vertex::line(30.0, 0.0),
            ])],
            Side::On,
        );
        on.entry = Entry::Helix {
            radius: 2.0,
            angle: 5.0,
        };
        let q = contour(&on, &tool).unwrap();
        // On the line a helix would cut beside it: the ramp runs along it.
        assert!(q.moves.iter().all(|m| m.to()[1] == 0.0));
        assert!(
            q.moves
                .iter()
                .any(|m| matches!(m, Move::Line { to, feed } if *feed == 300.0 && to[0] > 1.0))
        );
    }

    #[test]
    fn outside_of_a_square_is_offset_by_the_radius_with_round_corners_clockwise() {
        let tool = Tool::flat_end_mill(1, 6.0);
        let loops = contour_loops(
            &op(
                vec![Contour::rectangle([0.0, 0.0], [40.0, 20.0])],
                Side::Outside,
            ),
            &tool,
        )
        .unwrap();
        assert_eq!(loops.len(), 1);
        let l = &loops[0];
        assert!(!l.is_ccw(), "climb outside runs clockwise");
        // The square grown by 3 with quarter circles: 40x20 + perimeter*3 + pi*9.
        let expected = 800.0 + 120.0 * 3.0 + std::f64::consts::PI * 9.0;
        assert!((l.area().abs() - expected).abs() < 1e-6, "{}", l.area());
        assert_eq!(l.segments().iter().filter(|s| s.arc.is_some()).count(), 4);
    }

    #[test]
    fn inside_of_a_circle_shrinks_by_the_radius_counter_clockwise() {
        let tool = Tool::flat_end_mill(1, 6.0);
        let mut o = op(vec![Contour::circle([0.0, 0.0], 10.0)], Side::Inside);
        let loops = contour_loops(&o, &tool).unwrap();
        assert_eq!(loops.len(), 1);
        assert!(loops[0].is_ccw(), "climb inside runs counter-clockwise");
        assert!((loops[0].area() - std::f64::consts::PI * 49.0).abs() < 1e-6);
        o.direction = Direction::Conventional;
        assert!(!contour_loops(&o, &tool).unwrap()[0].is_ccw());
        o.stock_to_leave = 0.5;
        assert!(
            (contour_loops(&o, &tool).unwrap()[0].area().abs() - std::f64::consts::PI * 6.5 * 6.5)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn a_tool_too_big_for_the_hole_is_refused() {
        let tool = Tool::flat_end_mill(1, 6.0);
        let e = contour(
            &op(vec![Contour::circle([0.0, 0.0], 2.0)], Side::Inside),
            &tool,
        )
        .unwrap_err();
        assert_eq!(e.message, "The tool does not fit inside an outline.");
    }

    #[test]
    fn outlines_close_together_do_not_gouge_each_other() {
        // Two squares 2 mm apart, a 6 mm tool outside both: the offsets
        // meet and become one loop round both.
        let tool = Tool::flat_end_mill(1, 6.0);
        let a = Contour::rectangle([0.0, 0.0], [10.0, 10.0]);
        let b = Contour::rectangle([12.0, 0.0], [22.0, 10.0]);
        let loops = contour_loops(&op(vec![a.clone(), b.clone()], Side::Outside), &tool).unwrap();
        assert_eq!(loops.len(), 1);
        for s in loops[0].segments() {
            for c in [&a, &b] {
                assert!(!c.contains(s.start));
            }
        }
    }

    #[test]
    fn passes_go_down_in_steps_and_end_at_the_clearance() {
        let tool = Tool::flat_end_mill(1, 6.0);
        let p = contour(
            &op(
                vec![Contour::rectangle([0.0, 0.0], [40.0, 20.0])],
                Side::Outside,
            ),
            &tool,
        )
        .unwrap();
        assert!(matches!(p.moves[0], Move::Rapid { to } if to[2] == 10.0));
        let plunges: Vec<f64> = p
            .moves
            .iter()
            .filter_map(|m| match m {
                Move::Line { to, feed } if *feed == 300.0 => Some(to[2]),
                _ => None,
            })
            .collect();
        assert_eq!(plunges, vec![-3.0, -6.0]);
        assert_eq!(p.moves.last().unwrap().to()[2], 10.0);
        let (min, max) = p.bounds().unwrap();
        assert!((min[0] + 3.0).abs() < 1e-6 && (max[0] - 43.0).abs() < 1e-6);
        assert_eq!(min[2], -6.0);
    }

    #[test]
    fn an_open_outline_is_cut_on_the_line_back_and_forth() {
        let tool = Tool::flat_end_mill(1, 3.0);
        let line = Contour::open(vec![
            crate::geom::Vertex::line(0.0, 0.0),
            crate::geom::Vertex::line(30.0, 0.0),
        ]);
        let p = contour(&op(vec![line.clone()], Side::On), &tool).unwrap();
        let ends: Vec<[f64; 3]> = p
            .moves
            .iter()
            .filter(|m| matches!(m, Move::Line { feed, .. } if *feed == 1000.0))
            .map(|m| m.to())
            .collect();
        assert_eq!(ends, vec![[30.0, 0.0, -3.0], [0.0, 0.0, -6.0]]);
        assert!(contour(&op(vec![line], Side::Outside), &tool).is_err());
    }

    #[test]
    fn a_ball_end_mill_is_refused() {
        let mut tool = Tool::flat_end_mill(1, 6.0);
        tool.shape = crate::tool::ToolShape::BallEndMill;
        assert!(
            contour(
                &op(
                    vec![Contour::rectangle([0.0, 0.0], [10.0, 10.0])],
                    Side::Outside
                ),
                &tool
            )
            .is_err()
        );
    }
}
