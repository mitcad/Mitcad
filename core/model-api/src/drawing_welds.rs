// SPDX-License-Identifier: MIT
//! Weld symbols proposed from BUILD's joints (mitcad#2350; commands.md,
//! "Weld symbols on drawing views"): the command `propose_drawing_welds`.
//!
//! For a drawing of a BUILD design, each joint (`build_junctions`) of the
//! chosen members whose two parts a view shows gets a weld symbol
//! annotation on the view: a fillet weld round the meeting part's section
//! by default, all around where the joint closes, its arrow on an edge of
//! the joint the view sees. The edge is found from the parts' bodies as
//! the view shows them: the edges of the meeting part that lie on the
//! other part (all their points within its box), of them one the view
//! draws as a visible line, the longest seen; the joint closes when those
//! edges close a loop (a section's outline round its end).
//!
//! Members side by side (mitcad#2379: a `cross`, or any joint whose whole
//! edges do not close round it) are welded where they touch: the runs of
//! each member's edges within the other member's box in that member's own
//! coordinates (the contact's outline: one member's edges along the other's
//! face, the other's across the first), one run where both members' edges
//! coincide. When those runs close a loop round the contact, the fillet is
//! all around and its arrow on a run; else the whole edges stand as
//! above, and a joint whose members touch along no edge at all is skipped
//! with words naming it (its kind and its two members).
//!
//! A proposal keeps its joint's key (`WeldSymbol::joint`) and is an
//! ordinary annotation: moved, edited and deleted as any. Proposing again
//! follows the joints as they are then: a joint with a proposal keeps it
//! as edited (its edge found again when its pick is gone), a new joint
//! gets one, and the proposals of joints no longer there go; all in one
//! undo step.

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::drawing_annotations::{ViewAt, placed_curve};
use super::drawings::Shown;
use super::prelude::*;
use mitcad_model::Document;
use mitcad_model::assembly::inverse;
use mitcad_model::datum::{CurveGeometry, Vec3};
use mitcad_model::features::OccurrencePath;
use mitcad_model::ids::ComponentUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::topo::EdgeName;
use mitcad_model::transform::Transform;
use mitcad_model_build::prelude::*;
use mitcad_model_build::{Junction, JunctionSide};
use mitcad_model_drawing::P2;
use mitcad_model_drawing::annotate::{
    self, AnnotationType, ViewAnnotation, ViewPick, WeldKind, WeldSide, WeldSymbol,
};

/// How far (mm) an edge's points may lie outside the other part's box and
/// still lie on it: the import's geometric tolerance.
const ON_PART: f64 = 0.01;
/// Two edge ends nearer than this (mm) are one corner of a loop.
const SAME_POINT: f64 = 1e-3;
/// A run of an edge on the other member shorter than this (mm) is a touch
/// at a point, no contact to weld (mitcad#2379).
const SHORTEST_RUN: f64 = 0.1;

/// `propose_drawing_welds`: weld symbols for the BUILD joints a view shows.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposeWeldsCommand {
    drawing: String,
    view: u32,
    /// The members (BUILD parts, by their components) whose joints get
    /// welds; every joint the view shows when left out.
    #[serde(default)]
    members: Option<Vec<String>>,
    /// Instead of `members` or with them: picks on the view (as the
    /// annotations' picks), each the member whose body it is of (the
    /// Propose Welds tool's clicks).
    #[serde(default)]
    picks: Option<Vec<ViewPick>>,
    /// The weld each new proposal gets (its sizes, tail); a fillet on the
    /// arrow side when left out. All around is added where the joint
    /// closes.
    #[serde(default)]
    weld: Option<WeldSymbol>,
}

/// An edge of a body a view shows, in the drawing's coordinates.
struct ShownEdge {
    pick: ViewPick,
    /// Points along it (its ends first), the drawing's coordinates.
    points: Vec<Vec3>,
    /// Its middle.
    middle: Vec3,
    /// Its two ends (the same for a whole circle).
    ends: (Vec3, Vec3),
}

/// Where a joint's members touch: a whole edge on the other member, or the
/// run of an edge within it (members side by side, mitcad#2379).
struct Contact<'e> {
    edge: &'e ShownEdge,
    /// Points along it (its ends first and last), the drawing's
    /// coordinates.
    points: Vec<Vec3>,
    /// Its middle (half way along it).
    middle: Vec3,
    ends: (Vec3, Vec3),
}

impl<'e> Contact<'e> {
    fn whole(edge: &'e ShownEdge) -> Self {
        Contact {
            edge,
            points: edge.points.clone(),
            middle: edge.middle,
            ends: edge.ends,
        }
    }
}

/// Why a joint gets no weld symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Miss {
    /// Its members touch along no edge.
    NoContact,
    /// The view shows none of the edges where they touch.
    NotSeen,
}

/// The edge a joint's weld symbol points at and whether the joint closes.
struct JointEdge {
    pick: ViewPick,
    /// Where its arrow goes, view mm.
    at: P2,
    closed: bool,
}

pub(super) trait DrawingWeldsOps<K: Kernel> {
    /// `propose_drawing_welds`: weld symbols proposed for the BUILD joints
    /// a view shows (mitcad#2350).
    fn propose_drawing_welds(&mut self, c: ProposeWeldsCommand) -> Result<Value, ApiError>;
}

impl<K: Kernel> DrawingWeldsOps<K> for Document<K> {
    fn propose_drawing_welds(&mut self, c: ProposeWeldsCommand) -> Result<Value, ApiError> {
        let old = self.find_drawing(&c.drawing)?.clone();
        let (si, _) = old
            .find_view(c.view)
            .ok_or_else(|| ApiError(format!("{} has no view {}", old.name, c.view)))?;
        if let Some(weld) = &c.weld {
            weld.check().map_err(ApiError)?;
        }
        let mut members = match &c.members {
            Some(list) if list.is_empty() => {
                return Err(ApiError(
                    "give the members whose joints are welded, or leave `members` out for all"
                        .to_owned(),
                ));
            }
            Some(list) => Some(
                list.iter()
                    .map(|m| self.component_ref(m))
                    .collect::<Result<Vec<ComponentUid>, _>>()?,
            ),
            None => None,
        };
        // A member picked on the view: the component its body is of.
        if let Some(picks) = &c.picks {
            if picks.is_empty() {
                return Err(ApiError(
                    "pick the members whose joints are welded, or leave `picks` out for all"
                        .to_owned(),
                ));
            }
            let list = members.get_or_insert_with(Vec::new);
            for pick in picks {
                let component = self.body_component(pick.body).ok_or_else(|| {
                    ApiError(format!("the body {} is no member of the design", pick.body))
                })?;
                if !list.contains(&component) {
                    list.push(component);
                }
            }
        }
        let junctions = self.build_junctions(members.as_deref());
        let evaluated = self.evaluate_views(&old, si);
        let view = self
            .view_at(&old, si, &evaluated, c.view)
            .map_err(ApiError)?;
        let vi = old.sheets[si]
            .view_index(c.view)
            .ok_or_else(|| ApiError(format!("{} has no view {}", old.name, c.view)))?;
        let mut changed = old.clone();
        let mut next = old.next_annotation_id();
        let (mut added, mut updated, mut skipped) = (Vec::new(), Vec::new(), Vec::new());
        let mut joints_here: Vec<String> = Vec::new();
        for junction in &junctions {
            let (Some(a), Some(b)) = (shown_of(&view, &junction.a), shown_of(&view, &junction.b))
            else {
                // A joint of parts the view does not show.
                continue;
            };
            joints_here.push(junction.id.clone());
            let existing = changed.sheets[si].views[vi]
                .annotations
                .iter()
                .position(|x| proposal_of(x) == Some(junction.id.as_str()));
            if let Some(k) = existing {
                // Kept as edited; its edge found again when its pick is gone.
                let annotation = &changed.sheets[si].views[vi].annotations[k];
                if self.annotation_seen(&view, annotation).is_ok() {
                    continue;
                }
                match self.joint_edge(&view, a, b) {
                    Ok(edge) => {
                        let annotation = &mut changed.sheets[si].views[vi].annotations[k];
                        annotation.picks = vec![edge.pick];
                        annotation.at = Some(edge.at);
                        updated.push(annotation.id);
                    }
                    Err(miss) => skipped.push(json!({"joint": junction.id,
                        "why": self.miss_words(junction, miss)})),
                }
                continue;
            }
            let edge = match self.joint_edge(&view, a, b) {
                Ok(edge) => edge,
                Err(miss) => {
                    skipped.push(json!({"joint": junction.id,
                        "why": self.miss_words(junction, miss)}));
                    continue;
                }
            };
            let mut weld = c
                .weld
                .clone()
                .unwrap_or_else(|| WeldSymbol::arrow(WeldSide::new(WeldKind::Fillet)));
            weld.all_around |= edge.closed;
            weld.joint = Some(junction.id.clone());
            let mut annotation = ViewAnnotation::new(next, AnnotationType::Weld, vec![edge.pick]);
            annotation.weld = Some(weld);
            annotation.at = Some(edge.at);
            annotation.check().map_err(ApiError)?;
            if let Err(why) = self.annotation_seen(&view, &annotation) {
                skipped.push(json!({"joint": junction.id, "why": why}));
                continue;
            }
            next += 1;
            added.push(annotation.id);
            changed.sheets[si].views[vi].annotations.push(annotation);
        }
        // The proposals of joints no longer there (of the members asked
        // for, when they are given) go.
        let mut removed = Vec::new();
        changed.sheets[si].views[vi].annotations.retain(|x| {
            let Some(joint) = proposal_of(x) else {
                return true;
            };
            let asked = members.as_ref().is_none_or(|list| {
                list.iter().any(|m| {
                    joint
                        .split('|')
                        .any(|end| end.starts_with(&format!("{m}:")))
                })
            });
            let gone = asked && !joints_here.iter().any(|j| j == joint);
            if gone {
                removed.push(x.id);
            }
            !gone
        });
        if added.is_empty() && updated.is_empty() && removed.is_empty() {
            return Ok(json!({"drawing": old.uid, "annotations": [], "updated": [],
                             "removed": [], "skipped": skipped}));
        }
        let label = match (added.len(), updated.len() + removed.len()) {
            (1, 0) => format!("Propose weld symbol {} in {}", added[0], old.name),
            (n, 0) => format!("Propose {n} weld symbols in {}", old.name),
            _ => format!("Update the proposed weld symbols of {}", old.name),
        };
        self.set_drawing(old.uid, label, changed)?;
        Ok(
            json!({"drawing": old.uid, "annotations": added, "updated": updated,
                  "removed": removed, "skipped": skipped}),
        )
    }
}

/// The joint key of a weld symbol proposed for a BUILD joint.
fn proposal_of(annotation: &ViewAnnotation) -> Option<&str> {
    if annotation.kind != AnnotationType::Weld {
        return None;
    }
    annotation.weld.as_ref()?.joint.as_deref()
}

/// The body a view shows of a joint's part: where its occurrence (the path
/// from the BUILD host) places it under the drawing's component.
fn shown_of<'v>(view: &'v ViewAt<'_>, side: &JunctionSide) -> Option<&'v Shown> {
    if side.occurrence.is_empty() {
        return None;
    }
    view.shown
        .iter()
        .find(|s| !s.flat && s.path.ends_with(side.occurrence.as_slice()))
}

trait DrawingWeldsLocal<K: Kernel> {
    /// A body's edges as the view shows it, in the drawing's coordinates.
    fn shown_edges(&self, s: &Shown) -> Vec<ShownEdge>;

    /// The edge a joint's weld points at: of the meeting part's edges on
    /// the other part (else the other way round), or of the runs where
    /// members side by side touch when they close round the contact
    /// (mitcad#2379), one the view draws as a visible line, the longest
    /// seen; and whether those edges close.
    fn joint_edge(&self, view: &ViewAt<'_>, a: &Shown, b: &Shown) -> Result<JointEdge, Miss>;

    /// Why a joint gets no weld, in words naming it: its kind and its
    /// members (mitcad#2379).
    fn miss_words(&self, junction: &Junction, miss: Miss) -> String;
}

impl<K: Kernel> DrawingWeldsLocal<K> for Document<K> {
    fn shown_edges(&self, s: &Shown) -> Vec<ShownEdge> {
        let Ok(geometry) = self.shown_geometry(s) else {
            return Vec::new();
        };
        let occurrence = (!s.path.is_empty()).then(|| OccurrencePath(s.path.clone()));
        geometry
            .edges
            .iter()
            .map(|(name, curve)| {
                let curve = placed_curve(curve, &s.transform);
                let points = curve_points(&curve);
                ShownEdge {
                    pick: edge_pick(s, occurrence.clone(), name),
                    middle: points[points.len() / 2],
                    ends: curve_ends(&curve),
                    points,
                }
            })
            .collect()
    }

    fn joint_edge(&self, view: &ViewAt<'_>, a: &Shown, b: &Shown) -> Result<JointEdge, Miss> {
        let (edges_a, edges_b) = (self.shown_edges(a), self.shown_edges(b));
        let whole = on_part(&edges_a, &edges_b);
        let whole = if whole.is_empty() {
            on_part(&edges_b, &edges_a)
        } else {
            whole
        };
        let mut contact: Vec<Contact<'_>> = whole.into_iter().map(Contact::whole).collect();
        if !closes(&contact) {
            // Members side by side (mitcad#2379): where each one's edges
            // run on the other, taken when they close round the contact.
            let mut runs = runs_on(&edges_a, &edges_b, &b.transform);
            for run in runs_on(&edges_b, &edges_a, &a.transform) {
                let again = runs.iter().any(|r| same_run(r, &run));
                if !again {
                    runs.push(run);
                }
            }
            if contact.is_empty() || closes(&runs) {
                contact = runs;
            }
        }
        if contact.is_empty() {
            return Err(Miss::NoContact);
        }
        let closed = closes(&contact);
        let spec = &view.placed.spec;
        let best = contact
            .iter()
            .filter(|c| view.kept(c.middle) && view.sees(spec.project(c.middle)))
            .map(|c| {
                let middle = spec.project(c.middle);
                let seen: f64 = c
                    .points
                    .windows(2)
                    .map(|w| {
                        let (p, q) = (spec.project(w[0]), spec.project(w[1]));
                        (q[0] - p[0]).hypot(q[1] - p[1])
                    })
                    .sum();
                (view.on_visible(middle), seen, middle, c)
            })
            .max_by(|x, y| x.0.cmp(&y.0).then(x.1.total_cmp(&y.1)))
            .ok_or(Miss::NotSeen)?;
        let (_, _, at, c) = best;
        Ok(JointEdge {
            pick: c.edge.pick.clone(),
            at,
            closed,
        })
    }

    fn miss_words(&self, junction: &Junction, miss: Miss) -> String {
        let name = |side: &JunctionSide| {
            self.assembly()
                .component(side.component)
                .map_or_else(|| side.component.to_string(), |c| c.name.clone())
        };
        let (kind, a, b) = (junction.kind, name(&junction.a), name(&junction.b));
        match miss {
            Miss::NoContact => format!(
                "the {kind} joint of {a} and {b}: the members touch along no edge, so no weld can be placed"
            ),
            Miss::NotSeen => format!("the view shows no edge of the {kind} joint of {a} and {b}"),
        }
    }
}

/// A whole edge of a body a view shows as a pick.
fn edge_pick(s: &Shown, occurrence: Option<OccurrencePath>, edge: &EdgeName) -> ViewPick {
    ViewPick {
        body: s.body,
        occurrence,
        edge: Some(edge.clone()),
        face: None,
        point: None,
        on_face: None,
        on_cut: None,
        drawn: None,
        side: None,
        outline: None,
        extreme: None,
        bend: None,
    }
}

/// The edges of `edges` that lie on the part `other`'s edges bound: all
/// their points within its box (within [`ON_PART`]).
fn on_part<'e>(edges: &'e [ShownEdge], other: &[ShownEdge]) -> Vec<&'e ShownEdge> {
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for p in other.iter().flat_map(|e| e.points.iter()) {
        for (k, value) in p.iter().enumerate() {
            low[k] = low[k].min(*value);
            high[k] = high[k].max(*value);
        }
    }
    if !low.iter().all(|v| v.is_finite()) {
        return Vec::new();
    }
    let inside = |p: &Vec3| (0..3).all(|k| p[k] >= low[k] - ON_PART && p[k] <= high[k] + ON_PART);
    edges
        .iter()
        .filter(|e| e.points.iter().all(inside))
        .collect()
}

/// The runs of `edges` within the member `other`'s edges bound, in that
/// member's own coordinates (`placed` takes them into the drawing's): of
/// each edge the stretches inside its box, an edge lying along a face of
/// the box (within [`ON_PART`]) kept along it, stretches shorter than
/// [`SHORTEST_RUN`] left out (mitcad#2379).
fn runs_on<'e>(
    edges: &'e [ShownEdge],
    other: &[ShownEdge],
    placed: &Transform,
) -> Vec<Contact<'e>> {
    let into = inverse(placed);
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for p in other.iter().flat_map(|e| e.points.iter()) {
        let p = into.apply_point(*p);
        for (k, value) in p.iter().enumerate() {
            low[k] = low[k].min(*value);
            high[k] = high[k].max(*value);
        }
    }
    if !low.iter().all(|v| v.is_finite()) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for edge in edges {
        let local: Vec<Vec3> = edge.points.iter().map(|p| into.apply_point(*p)).collect();
        let mut run: Vec<Vec3> = Vec::new();
        for (k, w) in local.windows(2).enumerate() {
            let Some((t0, t1)) = clip_to_box(w[0], w[1], low, high) else {
                push_run(&mut out, edge, std::mem::take(&mut run));
                continue;
            };
            let (p, q) = (edge.points[k], edge.points[k + 1]);
            let at = |t: f64| mitcad_vec3::add(p, mitcad_vec3::scale(mitcad_vec3::sub(q, p), t));
            let (from, to) = (at(t0), at(t1));
            let goes_on = run
                .last()
                .is_some_and(|last| mitcad_vec3::distance(*last, from) < SAME_POINT);
            if !goes_on {
                push_run(&mut out, edge, std::mem::take(&mut run));
                run.push(from);
            }
            run.push(to);
        }
        push_run(&mut out, edge, run);
    }
    out
}

/// What of the segment from `p` to `q` lies in the box from `low` to
/// `high`, as the segment's parameters (0 at `p`, 1 at `q`): across a pair
/// of the box's faces cut where it passes them, along them (moving less
/// than [`ON_PART`] across) kept when within them by [`ON_PART`].
fn clip_to_box(p: Vec3, q: Vec3, low: Vec3, high: Vec3) -> Option<(f64, f64)> {
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for k in 0..3 {
        let d = q[k] - p[k];
        if d.abs() <= ON_PART {
            let outside = |v: f64| v < low[k] - ON_PART || v > high[k] + ON_PART;
            if outside(p[k]) || outside(q[k]) {
                return None;
            }
            continue;
        }
        let (a, b) = ((low[k] - p[k]) / d, (high[k] - p[k]) / d);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
    }
    (t1 > t0).then_some((t0, t1))
}

/// A run of `edge` as a contact, when it is long enough.
fn push_run<'e>(out: &mut Vec<Contact<'e>>, edge: &'e ShownEdge, run: Vec<Vec3>) {
    let lengths: Vec<f64> = run
        .windows(2)
        .map(|w| mitcad_vec3::distance(w[0], w[1]))
        .collect();
    let total: f64 = lengths.iter().sum();
    if run.len() < 2 || total < SHORTEST_RUN {
        return;
    }
    // Half way along it.
    let mut left = total / 2.0;
    let mut middle = run[run.len() - 1];
    for (w, length) in run.windows(2).zip(&lengths) {
        if left <= *length {
            let t = if *length > 0.0 { left / length } else { 0.0 };
            middle = mitcad_vec3::add(w[0], mitcad_vec3::scale(mitcad_vec3::sub(w[1], w[0]), t));
            break;
        }
        left -= length;
    }
    out.push(Contact {
        edge,
        ends: (run[0], run[run.len() - 1]),
        middle,
        points: run,
    });
}

/// Whether two runs are one stretch (both members' edges along one
/// line): their ends the same, either way round, and their middles.
fn same_run(a: &Contact<'_>, b: &Contact<'_>) -> bool {
    let near = |p: Vec3, q: Vec3| mitcad_vec3::distance(p, q) < SAME_POINT;
    let ends = (near(a.ends.0, b.ends.0) && near(a.ends.1, b.ends.1))
        || (near(a.ends.0, b.ends.1) && near(a.ends.1, b.ends.0));
    ends && near(a.middle, b.middle)
}

/// Whether contacts close loops: each end met by another's (a whole circle
/// by its own other end).
fn closes(edges: &[Contact<'_>]) -> bool {
    let ends: Vec<Vec3> = edges.iter().flat_map(|e| [e.ends.0, e.ends.1]).collect();
    !ends.is_empty()
        && ends.iter().enumerate().all(|(i, p)| {
            let met = ends
                .iter()
                .enumerate()
                .filter(|(j, q)| *j != i && mitcad_vec3::distance(*p, **q) < SAME_POINT)
                .count();
            met % 2 == 1
        })
}

/// A curve's two ends.
fn curve_ends(curve: &CurveGeometry) -> (Vec3, Vec3) {
    match curve {
        CurveGeometry::Line { start, end }
        | CurveGeometry::Circle { start, end, .. }
        | CurveGeometry::Other { start, end, .. } => (*start, *end),
    }
}

/// Points along a curve from its start to its end: a line's ends and
/// middle, an arc or a circle every 15 degrees, another curve's ends and
/// the middle between them.
fn curve_points(curve: &CurveGeometry) -> Vec<Vec3> {
    match curve {
        CurveGeometry::Line { start, end } | CurveGeometry::Other { start, end, .. } => {
            vec![
                *start,
                mitcad_vec3::scale(mitcad_vec3::add(*start, *end), 0.5),
                *end,
            ]
        }
        CurveGeometry::Circle {
            center,
            normal,
            radius,
            start,
            end,
        } => {
            let sweep = annotate::arc_sweep(*center, *normal, *radius, *start, *end);
            let n = mitcad_vec3::unit(*normal).unwrap_or([0.0, 0.0, 1.0]);
            let u = mitcad_vec3::unit(mitcad_vec3::sub(*start, *center)).unwrap_or([1.0, 0.0, 0.0]);
            let v = mitcad_vec3::cross(n, u);
            let steps = ((sweep / 15f64.to_radians()).ceil() as usize).max(2);
            (0..=steps)
                .map(|i| {
                    let t = sweep * i as f64 / steps as f64;
                    mitcad_vec3::add(
                        *center,
                        mitcad_vec3::add(
                            mitcad_vec3::scale(u, radius * t.cos()),
                            mitcad_vec3::scale(v, radius * t.sin()),
                        ),
                    )
                })
                .collect()
        }
    }
}
