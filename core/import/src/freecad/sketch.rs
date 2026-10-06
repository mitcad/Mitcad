// SPDX-License-Identifier: MIT
//! A FreeCAD sketch's geometry and constraints as the entities,
//! constraints and dimensions of a Mitcad sketch definition (`commands.md`,
//! *sketch*); the plane, the external geometry and the checks are
//! [`super::sketches`]'.
//!
//! - Points: FreeCAD's curves own their end and centre points, which
//!   Coincident constraints join; Mitcad's curves share point entities. The
//!   points (geometry, position) that coincide are merged (union-find) into
//!   one Mitcad point each, and the Coincident constraints are left out.
//!   Ends joined by end-to-end tangency or perpendicularity merge too. A
//!   merge that would join two points of one line, arc or ellipse is left
//!   out instead.
//! - Geometry: points, lines, circles, arcs, ellipses and their arcs as
//!   they are (arcs counter-clockwise, as the sketcher names their ends);
//!   arcs of hyperbolas and parabolas as exact rational quadratic splines;
//!   B-splines with their poles as control points (a periodic or unclamped
//!   one as the same curve clamped, with new control points). Construction
//!   and blocked geometry keep their flags.
//! - Internal geometry: a B-spline's control point circles are its control
//!   points (constraints on their centres go to them; their radii and
//!   weights are not dimensions in Mitcad), its end knots its ends; an
//!   ellipse's axis lines stay construction lines held on the ellipse
//!   (an end at the major point, the centre at their middles, the minor
//!   one at right angles with its end on the ellipse), so that their
//!   constraints still apply. Foci, interior knots and the parts of
//!   hyperbolas and parabolas are left out.
//! - Constraints and dimensions by type; what Mitcad cannot express is
//!   left out and listed (the sketch is then partial). Values: FreeCAD's
//!   signed horizontal and vertical distances become distances between the
//!   points in the order that makes them positive, angles the angle between
//!   the lines' directions (0 to 180 degrees).

use std::collections::{BTreeSet, HashMap};

use mitcad_freecad::sketch::{
    self as fc, ConstraintType as T, Curve, FIRST_EXTERNAL, GeoRef, H_AXIS, PointPos, V_AXIS,
    internal,
};
use mitcad_model::sketch::geometry::Nurbs;
use serde_json::{Value, json};

use super::report::DimensionSource;

/// A Mitcad curve's kind, for what the constraints accept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Line,
    Circle,
    Arc,
    Ellipse,
    EllipticalArc,
    Spline,
}

impl Kind {
    fn is_round(self) -> bool {
        matches!(self, Kind::Circle | Kind::Arc)
    }

    /// The kinds Mitcad's tangency joins freely (splines only at a shared
    /// end or along their inside; ellipses not at all).
    fn simple(self) -> bool {
        matches!(self, Kind::Line | Kind::Circle | Kind::Arc)
    }
}

/// External geometry already made (by projection or as fixed reference
/// entities): its curve and its points by FreeCAD's position.
#[derive(Debug, Clone, Default)]
pub(super) struct ExternalMap {
    pub curve: Option<(u32, Kind)>,
    pub points: Vec<(PointPos, u32)>,
}

/// A sketch's translation.
#[derive(Debug, Default)]
pub(super) struct Translated {
    pub entities: Vec<Value>,
    pub constraints: Vec<Value>,
    /// Dimensions as driving ones (with values; reference dimensions
    /// driven).
    pub dimensions: Vec<Value>,
    /// The same dimensions, all driven.
    pub driven_dimensions: Vec<Value>,
    /// Where each point should be after solving, and each circle's radius
    /// and each ellipse's minor radius.
    pub positions: Vec<(u32, [f64; 2])>,
    pub radii: Vec<(u32, f64)>,
    /// What was left out or changed: each makes the sketch partial.
    pub dropped: Vec<String>,
    /// Remarks that do not.
    pub notes: Vec<String>,
    pub sources: Vec<DimensionSource>,
    /// The Mitcad curve of each FreeCAD geometry made (by geometry index),
    /// and the point of each point geometry: what the history's features
    /// refer to (revolve axes, holes, paths).
    pub curves: Vec<(i32, u32)>,
    pub points: Vec<(i32, u32)>,
    /// The H and V axis lines, when made.
    pub axes: [Option<u32>; 2],
}

/// A point of a FreeCAD geometry: at a position, a B-spline's pole, an
/// ellipse's major point (Mitcad's), the root point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Vertex {
    At(i32, PointPos),
    Pole(i32, usize),
    Major(i32),
    Root,
}

/// What a geometry of the sketch is to the translation.
#[derive(Debug, Clone, PartialEq)]
enum Role {
    Normal,
    /// A B-spline's control point circle: its centre is the pole.
    Control {
        spline: i32,
        index: usize,
    },
    /// A knot point at a clamped B-spline's end.
    KnotEnd {
        spline: i32,
        end: PointPos,
    },
    /// An ellipse's major or minor axis line.
    Axis {
        ellipse: i32,
        major: bool,
    },
    Dropped(&'static str),
}

/// A curve's lookup: made, internal geometry whose constraints need not
/// carry over (control point circles), or missing with the reason.
enum Found {
    Curve(u32, Kind),
    Internal,
    Missing(String),
}

struct Translator<'a> {
    sketch: &'a fc::Sketch,
    external: &'a [Option<ExternalMap>],
    roles: Vec<Role>,
    keys: Vec<Vertex>,
    index: HashMap<Vertex, usize>,
    parent: Vec<usize>,
    members: Vec<Vec<usize>>,
    /// Coincident constraints whose merge was refused.
    refused: BTreeSet<usize>,
    class_point: HashMap<usize, u32>,
    curves: HashMap<i32, (u32, Kind)>,
    /// Entity index in `out.entities` by id.
    entity_at: HashMap<u32, usize>,
    next: u32,
    root: Option<u32>,
    axes: [Option<u32>; 2],
    out: Translated,
}

/// Translates a sketch; entity ids start at `first_id` (the external
/// geometry's come before). `axes` asks for the H and V axis lines even
/// when no constraint uses them (features of the history refer to them).
pub(super) fn translate(
    sketch: &fc::Sketch,
    external: &[Option<ExternalMap>],
    first_id: u32,
    axes: [bool; 2],
) -> Translated {
    let mut t = Translator {
        sketch,
        external,
        roles: vec![Role::Normal; sketch.geometry.len()],
        keys: Vec::new(),
        index: HashMap::new(),
        parent: Vec::new(),
        members: Vec::new(),
        refused: BTreeSet::new(),
        class_point: HashMap::new(),
        curves: HashMap::new(),
        entity_at: HashMap::new(),
        next: first_id,
        root: None,
        axes: [None, None],
        out: Translated::default(),
    };
    t.roles();
    t.merge();
    t.points();
    t.curves();
    for (i, c) in sketch.constraints.iter().enumerate() {
        t.constraint(i, c);
    }
    t.block();
    for (which, wanted) in axes.iter().enumerate() {
        if *wanted {
            t.axis(which);
        }
    }
    t.out.axes = t.axes;
    let mut curves: Vec<(i32, u32)> = t.curves.iter().map(|(g, (id, _))| (*g, *id)).collect();
    curves.sort_unstable();
    t.out.curves = curves;
    for (g, geo) in sketch.geometry.iter().enumerate() {
        if matches!(geo.curve, Curve::Point { .. })
            && let Some(id) = t
                .vertex(GeoRef::new(g as i32, PointPos::Start))
                .and_then(|v| t.point_of(v))
        {
            t.out.points.push((g as i32, id));
        }
    }
    // Constraint and dimension ids share one space.
    let first_dimension = t.out.constraints.len();
    for (i, item) in t.out.constraints.iter_mut().enumerate() {
        item["id"] = json!(format!("k{}", i + 1));
    }
    for (i, (driving, driven)) in t
        .out
        .dimensions
        .iter_mut()
        .zip(t.out.driven_dimensions.iter_mut())
        .enumerate()
    {
        let id = json!(format!("k{}", first_dimension + i + 1));
        driving["id"] = id.clone();
        driven["id"] = id;
    }
    // The sources were numbered by their dimension's index.
    for s in &mut t.out.sources {
        let i: usize = s.dimension.parse().unwrap_or(0);
        s.dimension = format!("k{}", first_dimension + i + 1);
    }
    t.out
}

fn p(id: u32) -> String {
    format!("p{id}")
}

fn c(id: u32) -> String {
    format!("c{id}")
}

/// An angle in (−π, π].
fn wrap(a: f64) -> f64 {
    let t = a.rem_euclid(std::f64::consts::TAU);
    if t > std::f64::consts::PI {
        t - std::f64::consts::TAU
    } else {
        t
    }
}

impl Translator<'_> {
    fn geometry(&self, geo: i32) -> Option<&fc::Geometry> {
        self.sketch.geometry(geo)
    }

    fn type_of(&self, geo: i32) -> String {
        match geo {
            H_AXIS => "the H axis".to_owned(),
            V_AXIS => "the V axis".to_owned(),
            g => self.geometry(g).map_or_else(
                || format!("geometry {g}"),
                |x| {
                    x.curve
                        .type_name()
                        .trim_start_matches("Part::Geom")
                        .to_owned()
                },
            ),
        }
    }

    fn drop_constraint(&mut self, i: usize, kind: T, why: impl Into<String>) {
        self.out
            .dropped
            .push(format!("constraint {i} ({}): {}", kind.name(), why.into()));
    }

    // Internal geometry.

    fn roles(&mut self) {
        for c in &self.sketch.constraints {
            let Some((kind, index)) = c.alignment else {
                continue;
            };
            let (child, parent) = (c.first.geo, c.second.geo);
            let (Ok(child), Some(parent_geometry)) =
                (usize::try_from(child), self.geometry(parent))
            else {
                continue;
            };
            if child >= self.roles.len() {
                continue;
            }
            let role = match (kind, &parent_geometry.curve) {
                (
                    internal::ELLIPSE_MAJOR_DIAMETER | internal::ELLIPSE_MINOR_DIAMETER,
                    Curve::Ellipse { .. } | Curve::ArcOfEllipse { .. },
                ) => Role::Axis {
                    ellipse: parent,
                    major: kind == internal::ELLIPSE_MAJOR_DIAMETER,
                },
                (internal::ELLIPSE_FOCUS1 | internal::ELLIPSE_FOCUS2, _) => {
                    Role::Dropped("an ellipse's focus")
                }
                (
                    internal::HYPERBOLA_MAJOR
                    | internal::HYPERBOLA_MINOR
                    | internal::HYPERBOLA_FOCUS,
                    _,
                ) => Role::Dropped("internal geometry of a hyperbola"),
                (internal::PARABOLA_FOCUS | internal::PARABOLA_FOCAL_AXIS, _) => {
                    Role::Dropped("internal geometry of a parabola")
                }
                (internal::BSPLINE_CONTROL_POINT, Curve::BSpline(b)) => {
                    match usize::try_from(index) {
                        Ok(i) if b.is_clamped() && i < b.poles.len() => Role::Control {
                            spline: parent,
                            index: i,
                        },
                        _ => Role::Dropped("a control point of a B-spline Mitcad clamps"),
                    }
                }
                (internal::BSPLINE_KNOT_POINT, Curve::BSpline(b)) => {
                    let last = b.knots.len().saturating_sub(1) as i64;
                    match index {
                        0 if b.is_clamped() => Role::KnotEnd {
                            spline: parent,
                            end: PointPos::Start,
                        },
                        i if i == last && b.is_clamped() => Role::KnotEnd {
                            spline: parent,
                            end: PointPos::End,
                        },
                        _ => Role::Dropped("an interior knot point of a B-spline"),
                    }
                }
                _ => continue,
            };
            self.roles[child] = role;
        }
        let sketch = self.sketch;
        for (g, geometry) in sketch.geometry.iter().enumerate() {
            if self.roles[g] == Role::Normal && matches!(geometry.curve, Curve::Other { .. }) {
                self.roles[g] = Role::Dropped("a kind of curve Mitcad has not");
            }
            let Role::Dropped(why) = self.roles[g] else {
                continue;
            };
            let line = format!(
                "geometry {g} ({}): {why}",
                geometry.curve.type_name().trim_start_matches("Part::Geom")
            );
            // Internal geometry nothing else refers to is no loss.
            let used = sketch.constraints.iter().any(|c| {
                c.kind != T::InternalAlignment
                    && c.active
                    && c.refs().iter().any(|r| r.geo == g as i32)
            });
            if used || geometry.internal == internal::NONE {
                self.out.dropped.push(line);
            } else {
                self.out.notes.push(line);
            }
        }
    }

    // Points.

    /// The key of a referenced point, None for a whole curve or nothing.
    fn vertex(&self, r: GeoRef) -> Option<Vertex> {
        if !r.is_point() {
            return None;
        }
        if r.geo == H_AXIS && r.pos == PointPos::Start {
            return Some(Vertex::Root);
        }
        // A point is one vertex, whatever position names it.
        if self
            .geometry(r.geo)
            .is_some_and(|g| matches!(g.curve, Curve::Point { .. }))
        {
            return Some(Vertex::At(r.geo, PointPos::Start));
        }
        Some(Vertex::At(r.geo, r.pos))
    }

    /// Where a vertex is.
    fn position(&self, v: Vertex) -> Option<[f64; 2]> {
        match v {
            Vertex::Root => Some([0.0, 0.0]),
            Vertex::At(g, pos) => self.geometry(g)?.curve.point(pos),
            Vertex::Pole(g, i) => match &self.geometry(g)?.curve {
                Curve::BSpline(b) => b.poles.get(i).copied(),
                _ => None,
            },
            Vertex::Major(g) => match &self.geometry(g)?.curve {
                Curve::Ellipse {
                    center,
                    major,
                    frame,
                    ..
                }
                | Curve::ArcOfEllipse {
                    center,
                    major,
                    frame,
                    ..
                } => Some(frame.place(*center, *major, 0.0)),
                _ => None,
            },
        }
    }

    fn key(&mut self, v: Vertex) -> usize {
        if let Some(&i) = self.index.get(&v) {
            return i;
        }
        let i = self.keys.len();
        self.keys.push(v);
        self.index.insert(v, i);
        self.parent.push(i);
        self.members.push(vec![i]);
        i
    }

    fn find(&mut self, i: usize) -> usize {
        let mut root = i;
        while self.parent[root] != root {
            root = self.parent[root];
        }
        let mut at = i;
        while self.parent[at] != root {
            let next = self.parent[at];
            self.parent[at] = root;
            at = next;
        }
        root
    }

    /// The geometry whose distinct points a vertex is (lines, arcs,
    /// ellipses, external curves); None for points, circles and splines,
    /// whose points may coincide.
    fn distinct_owner(&self, v: Vertex) -> Option<i32> {
        let g = match v {
            Vertex::At(g, _) | Vertex::Major(g) => g,
            Vertex::Pole(..) | Vertex::Root => return None,
        };
        match &self.geometry(g)?.curve {
            Curve::Point { .. }
            | Curve::Circle { .. }
            | Curve::BSpline(_)
            | Curve::Bezier { .. } => None,
            _ => Some(g),
        }
    }

    /// Merges two vertices; false when that would join two points of one
    /// curve that must stay apart.
    fn union(&mut self, a: Vertex, b: Vertex) -> bool {
        let (ka, kb) = (self.key(a), self.key(b));
        let (ra, rb) = (self.find(ka), self.find(kb));
        if ra == rb {
            return true;
        }
        let owners = |t: &Self, r: usize| -> BTreeSet<i32> {
            t.members[r]
                .iter()
                .filter_map(|&k| t.distinct_owner(t.keys[k]))
                .collect()
        };
        if !owners(self, ra).is_disjoint(&owners(self, rb)) {
            return false;
        }
        let moved = std::mem::take(&mut self.members[rb]);
        self.members[ra].extend(moved);
        self.parent[rb] = ra;
        true
    }

    /// The vertices of the geometry the sketch keeps, and their structural
    /// merges; then the merges of the constraints.
    fn merge(&mut self) {
        let sketch = self.sketch;
        for (g, geo) in sketch.geometry.iter().enumerate() {
            let g = g as i32;
            let role = self.roles[g as usize].clone();
            if matches!(role, Role::Dropped(_)) {
                continue;
            }
            for pos in [PointPos::Start, PointPos::End, PointPos::Mid] {
                let v = self.vertex(GeoRef::new(g, pos));
                if let Some(v) = v
                    && self.position(v).is_some()
                {
                    self.key(v);
                }
            }
            match (&role, &geo.curve) {
                (Role::Control { spline, index }, _) => {
                    self.union(Vertex::At(g, PointPos::Mid), Vertex::Pole(*spline, *index));
                }
                (Role::KnotEnd { spline, end }, _) => {
                    self.union(Vertex::At(g, PointPos::Start), Vertex::At(*spline, *end));
                }
                (
                    Role::Axis {
                        ellipse,
                        major: true,
                    },
                    Curve::Line { start, end },
                ) => {
                    if let Some(m) = self.position(Vertex::Major(*ellipse)) {
                        let d = |q: [f64; 2]| (q[0] - m[0]).hypot(q[1] - m[1]);
                        let at = if d(*start) <= d(*end) {
                            PointPos::Start
                        } else {
                            PointPos::End
                        };
                        self.union(Vertex::At(g, at), Vertex::Major(*ellipse));
                    }
                }
                (Role::Normal, Curve::BSpline(b)) => {
                    if b.is_clamped() {
                        for i in 0..b.poles.len() {
                            self.key(Vertex::Pole(g, i));
                        }
                        let last = b.poles.len() - 1;
                        self.union(Vertex::At(g, PointPos::Start), Vertex::Pole(g, 0));
                        self.union(Vertex::At(g, PointPos::End), Vertex::Pole(g, last));
                    } else if b.periodic {
                        self.union(Vertex::At(g, PointPos::Start), Vertex::At(g, PointPos::End));
                    }
                }
                (Role::Normal, Curve::Ellipse { .. } | Curve::ArcOfEllipse { .. }) => {
                    self.key(Vertex::Major(g));
                }
                _ => {}
            }
        }
        // External points.
        let external = self.external;
        for (k, map) in external.iter().enumerate() {
            if let Some(map) = map {
                for (pos, _) in &map.points {
                    self.key(Vertex::At(FIRST_EXTERNAL - k as i32, *pos));
                }
            }
        }
        for (i, con) in sketch.constraints.iter().enumerate() {
            if !con.active {
                continue;
            }
            let joins = match con.kind {
                T::Coincident => true,
                T::Tangent | T::Perpendicular => {
                    con.first.is_point() && con.second.is_point() && con.third.is_undef()
                }
                _ => false,
            };
            if !joins {
                continue;
            }
            let (Some(a), Some(b)) = (self.vertex(con.first), self.vertex(con.second)) else {
                continue;
            };
            // Only vertices of geometry kept (others are reported by the
            // constraint).
            if !self.index.contains_key(&a) && a != Vertex::Root
                || !self.index.contains_key(&b) && b != Vertex::Root
            {
                continue;
            }
            if !self.union(a, b) {
                self.refused.insert(i);
            }
        }
    }

    fn alloc(&mut self) -> u32 {
        self.next += 1;
        self.next - 1
    }

    fn push_entity(&mut self, id: u32, e: Value) {
        self.entity_at.insert(id, self.out.entities.len());
        self.out.entities.push(e);
    }

    /// A Mitcad point for each class of vertices: an external point when
    /// the class has one, else a new one.
    fn points(&mut self) {
        for k in 0..self.keys.len() {
            let r = self.find(k);
            if self.class_point.contains_key(&r) {
                continue;
            }
            let mut external = None;
            let mut root = false;
            let mut at = None;
            for &m in &self.members[r] {
                match self.keys[m] {
                    Vertex::Root => root = true,
                    Vertex::At(g, pos) if g <= FIRST_EXTERNAL => {
                        let k = (FIRST_EXTERNAL - g) as usize;
                        if external.is_none()
                            && let Some(Some(map)) = self.external.get(k)
                        {
                            external = map
                                .points
                                .iter()
                                .find(|(q, _)| *q == pos)
                                .map(|(_, id)| *id);
                        }
                    }
                    v => {
                        if at.is_none() {
                            at = self.position(v);
                        }
                    }
                }
            }
            let id = match external {
                Some(id) => id,
                None => {
                    let at = if root { Some([0.0, 0.0]) } else { at };
                    let Some(at) = at else { continue };
                    let id = self.alloc();
                    let mut e = json!({"id": p(id), "type": "point", "at": at});
                    if root {
                        e["fixed"] = json!(true);
                        self.root = Some(id);
                    }
                    self.push_entity(id, e);
                    self.out.positions.push((id, at));
                    id
                }
            };
            self.class_point.insert(r, id);
        }
    }

    /// The Mitcad point of a vertex.
    fn point_of(&mut self, v: Vertex) -> Option<u32> {
        if v == Vertex::Root && !self.index.contains_key(&v) {
            return Some(self.root_point());
        }
        let k = *self.index.get(&v)?;
        let r = self.find(k);
        self.class_point.get(&r).copied()
    }

    fn root_point(&mut self) -> u32 {
        if let Some(id) = self.root {
            return id;
        }
        let id = self.alloc();
        self.push_entity(
            id,
            json!({"id": p(id), "type": "point", "at": [0.0, 0.0], "fixed": true}),
        );
        self.out.positions.push((id, [0.0, 0.0]));
        self.root = Some(id);
        id
    }

    /// A construction line along the H (0) or V (1) axis from the root
    /// point, fixed.
    fn axis(&mut self, which: usize) -> u32 {
        if let Some(id) = self.axes[which] {
            return id;
        }
        let root = self.root_point();
        let end = self.alloc();
        let at = if which == 0 { [10.0, 0.0] } else { [0.0, 10.0] };
        self.push_entity(
            end,
            json!({"id": p(end), "type": "point", "at": at, "fixed": true}),
        );
        self.out.positions.push((end, at));
        let id = self.alloc();
        self.push_entity(
            id,
            json!({"id": c(id), "type": "line", "start": p(root), "end": p(end),
                   "construction": true, "fixed": true}),
        );
        self.axes[which] = Some(id);
        id
    }

    // Curves.

    fn curves(&mut self) {
        let sketch = self.sketch;
        // Axis lines are held on their ellipses once all curves are made.
        let mut axes = Vec::new();
        for (g, geo) in sketch.geometry.iter().enumerate() {
            let role = self.roles[g].clone();
            if !matches!(role, Role::Normal | Role::Axis { .. }) {
                continue;
            }
            let g = g as i32;
            let id = match self.curve_entity(g, geo) {
                Ok(Some((id, kind, entity))) => {
                    let mut entity = entity;
                    if geo.construction {
                        entity["construction"] = json!(true);
                    }
                    self.push_entity(id, entity);
                    self.curves.insert(g, (id, kind));
                    id
                }
                Ok(None) => continue,
                Err(why) => {
                    self.out.dropped.push(format!(
                        "geometry {g} ({}): {why}",
                        geo.curve.type_name().trim_start_matches("Part::Geom")
                    ));
                    continue;
                }
            };
            if let Role::Axis { ellipse, major } = role {
                axes.push((g, id, ellipse, major));
            }
        }
        for (g, id, ellipse, major) in axes {
            self.hold_axis(g, id, ellipse, major);
        }
    }

    fn at(&mut self, g: i32, pos: PointPos) -> Result<u32, String> {
        let v = self.vertex(GeoRef::new(g, pos)).ok_or("no such point")?;
        self.point_of(v)
            .ok_or_else(|| "a point is missing".to_owned())
    }

    /// A new free point at `at`.
    fn new_point(&mut self, at: [f64; 2]) -> u32 {
        let id = self.alloc();
        self.push_entity(id, json!({"id": p(id), "type": "point", "at": at}));
        self.out.positions.push((id, at));
        id
    }

    /// The curve entity of a geometry: id, kind and definition; None for a
    /// point.
    fn curve_entity(
        &mut self,
        g: i32,
        geo: &fc::Geometry,
    ) -> Result<Option<(u32, Kind, Value)>, String> {
        use PointPos::{End, Mid, Start};
        let id = match &geo.curve {
            Curve::Point { .. } => return Ok(None),
            _ => self.alloc(),
        };
        Ok(Some(match &geo.curve {
            Curve::Point { .. } | Curve::Other { .. } => unreachable!("left out above"),
            Curve::Line { .. } => {
                let (s, e) = (self.at(g, Start)?, self.at(g, End)?);
                if s == e {
                    return Err("its ends are one point".to_owned());
                }
                (
                    id,
                    Kind::Line,
                    json!({"id": c(id), "type": "line", "start": p(s), "end": p(e)}),
                )
            }
            Curve::Circle { radius, .. } => {
                let center = self.at(g, Mid)?;
                self.out.radii.push((id, *radius));
                (
                    id,
                    Kind::Circle,
                    json!({"id": c(id), "type": "circle", "center": p(center), "radius": radius}),
                )
            }
            Curve::Arc { .. } => {
                let (center, s, e) = (self.at(g, Mid)?, self.at(g, Start)?, self.at(g, End)?);
                if center == s || center == e || s == e {
                    return Err("an arc needs three points".to_owned());
                }
                (
                    id,
                    Kind::Arc,
                    json!({"id": c(id), "type": "arc", "center": p(center), "start": p(s),
                           "end": p(e)}),
                )
            }
            Curve::Ellipse { minor, .. } | Curve::ArcOfEllipse { minor, .. } => {
                let center = self.at(g, Mid)?;
                let major = self
                    .point_of(Vertex::Major(g))
                    .ok_or("its major point is missing")?;
                self.out.radii.push((id, *minor));
                let mut e = json!({"id": c(id), "type": "ellipse", "center": p(center),
                                   "major": p(major), "minor_radius": minor});
                let mut kind = Kind::Ellipse;
                if matches!(geo.curve, Curve::ArcOfEllipse { .. }) {
                    let (s, en) = (self.at(g, Start)?, self.at(g, End)?);
                    e["type"] = json!("elliptical_arc");
                    e["start"] = json!(p(s));
                    e["end"] = json!(p(en));
                    kind = Kind::EllipticalArc;
                }
                (id, kind, e)
            }
            Curve::ArcOfHyperbola { .. } | Curve::ArcOfParabola { .. } => {
                let (middle, weight) = conic_middle(&geo.curve).ok_or("a degenerate conic arc")?;
                let (s, e) = (self.at(g, Start)?, self.at(g, End)?);
                let m = self.new_point(middle);
                self.out.dropped.push(format!(
                    "geometry {g} ({}): a spline of the same shape",
                    geo.curve.type_name().trim_start_matches("Part::Geom")
                ));
                (
                    id,
                    Kind::Spline,
                    json!({"id": c(id), "type": "spline", "degree": 2,
                           "control": [p(s), p(m), p(e)], "weights": [1.0, weight, 1.0],
                           "knots": [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]}),
                )
            }
            Curve::BSpline(b) => {
                let flat = b.flat().ok_or("its knots do not fit its poles")?;
                let weights: Vec<f64> = flat.weights.clone();
                let rational = weights.iter().any(|w| (w - 1.0).abs() > 1e-15);
                if b.is_clamped() {
                    let control = (0..b.poles.len())
                        .map(|i| self.point_of(Vertex::Pole(g, i)).map(p))
                        .collect::<Option<Vec<String>>>()
                        .ok_or("a control point is missing")?;
                    let mut e = json!({"id": c(id), "type": "spline", "degree": b.degree,
                                       "control": control, "knots": flat.knots});
                    if rational {
                        e["weights"] = json!(weights);
                    }
                    (id, Kind::Spline, e)
                } else {
                    // The same curve clamped: new control points between
                    // its ends.
                    let nurbs = Nurbs {
                        degree: flat.degree,
                        control: flat.poles.clone(),
                        weights: if rational { weights } else { Vec::new() },
                        knots: flat.knots.clone(),
                    };
                    let (lo, hi) = nurbs.domain();
                    let clamped = nurbs.piece(lo, hi);
                    let (s, e) = (self.at(g, Start)?, self.at(g, End)?);
                    let last = clamped.control.len() - 1;
                    let mut control = Vec::new();
                    for (i, q) in clamped.control.iter().enumerate() {
                        control.push(p(match i {
                            0 => s,
                            i if i == last => e,
                            _ => self.new_point(*q),
                        }));
                    }
                    let mut entity = json!({"id": c(id), "type": "spline",
                                            "degree": clamped.degree, "control": control,
                                            "knots": clamped.knots});
                    if !clamped.weights.is_empty() {
                        entity["weights"] = json!(clamped.weights);
                    }
                    self.out.notes.push(format!(
                        "geometry {g} (BSplineCurve): {} as a clamped spline",
                        if b.periodic { "periodic" } else { "unclamped" }
                    ));
                    (id, Kind::Spline, entity)
                }
            }
            Curve::Bezier { poles, weights } => {
                if poles.len() < 2 {
                    return Err("a Bézier curve needs two poles".to_owned());
                }
                let (s, e) = (self.at(g, Start)?, self.at(g, End)?);
                let last = poles.len() - 1;
                let mut control = Vec::new();
                for (i, q) in poles.iter().enumerate() {
                    control.push(p(match i {
                        0 => s,
                        i if i == last => e,
                        _ => self.new_point(*q),
                    }));
                }
                let mut entity = json!({"id": c(id), "type": "spline", "degree": last,
                                        "control": control});
                if weights.iter().any(|w| (w - 1.0).abs() > 1e-15) {
                    entity["weights"] = json!(weights);
                }
                (id, Kind::Spline, entity)
            }
        }))
    }

    /// Holds an ellipse's axis line on it: the centre at its middle, the
    /// minor axis at right angles to the major one with an end on the
    /// ellipse.
    fn hold_axis(&mut self, line: i32, id: u32, ellipse: i32, major: bool) {
        let Ok(center) = self.at(ellipse, PointPos::Mid) else {
            return;
        };
        let Some(&(e, _)) = self.curves.get(&ellipse) else {
            return;
        };
        self.out
            .constraints
            .push(json!({"type": "midpoint", "point": p(center), "curve": c(id)}));
        if major {
            return;
        }
        // The major axis: its line, else a construction line to the major
        // point.
        let major_line = self.roles.iter().enumerate().find_map(|(g, r)| match r {
            Role::Axis {
                ellipse: x,
                major: true,
            } if *x == ellipse => self.curves.get(&(g as i32)).map(|(id, _)| *id),
            _ => None,
        });
        let major_line = match major_line {
            Some(l) => l,
            None => {
                let Some(m) = self.point_of(Vertex::Major(ellipse)) else {
                    return;
                };
                let l = self.alloc();
                self.push_entity(
                    l,
                    json!({"id": c(l), "type": "line", "start": p(center), "end": p(m),
                           "construction": true}),
                );
                l
            }
        };
        self.out
            .constraints
            .push(json!({"type": "perpendicular", "a": c(id), "b": c(major_line)}));
        if let Ok(end) = self.at(line, PointPos::Start) {
            self.out
                .constraints
                .push(json!({"type": "coincident", "point": p(end), "entity": c(e)}));
        }
    }

    // Constraints.

    /// The curve a GeoId names.
    fn curve(&mut self, geo: i32) -> Found {
        match geo {
            H_AXIS => return Found::Curve(self.axis(0), Kind::Line),
            V_AXIS => return Found::Curve(self.axis(1), Kind::Line),
            _ => {}
        }
        if geo <= FIRST_EXTERNAL {
            let k = (FIRST_EXTERNAL - geo) as usize;
            return match self.external.get(k) {
                Some(Some(ExternalMap {
                    curve: Some((id, kind)),
                    ..
                })) => Found::Curve(*id, *kind),
                _ => Found::Missing(format!("external geometry {k} was not made")),
            };
        }
        if let Some(&(id, kind)) = self.curves.get(&geo) {
            return Found::Curve(id, kind);
        }
        if geo < 0 {
            return Found::Missing(format!("no geometry {geo}"));
        }
        match self.roles.get(geo as usize) {
            Some(Role::Control { .. }) => Found::Internal,
            Some(Role::Dropped(why)) => Found::Missing(format!("geometry {geo} is {why}")),
            _ => Found::Missing(format!(
                "geometry {geo} ({}) was not made",
                self.type_of(geo)
            )),
        }
    }

    /// The point a reference names.
    fn point(&mut self, r: GeoRef) -> Result<u32, String> {
        let v = self.vertex(r).ok_or("not a point")?;
        if let Some(id) = self.point_of(v) {
            return Ok(id);
        }
        Err(match v {
            Vertex::At(g, _) if g <= FIRST_EXTERNAL => format!(
                "a point of external geometry {} that was not made",
                FIRST_EXTERNAL - g
            ),
            Vertex::At(g, pos) => format!(
                "point {} of geometry {g} ({}), which was left out",
                pos.index(),
                self.type_of(g)
            ),
            _ => "a point that was left out".to_owned(),
        })
    }

    fn push(&mut self, v: Value) {
        self.out.constraints.push(v);
    }

    /// A dimension, driving with `mitcad` as its value unless FreeCAD's is
    /// a reference.
    fn dimension(&mut self, i: usize, con: &fc::Constraint, mut v: Value, mitcad: f64) {
        let mut driven = v.clone();
        driven["driven"] = json!(true);
        if con.driving {
            v["value"] = json!(mitcad);
            self.out.sources.push(DimensionSource {
                index: i,
                // Numbered by the dimension's index until ids are given.
                dimension: self.out.dimensions.len().to_string(),
                name: con.name.clone(),
                expression: self.sketch.expression(i).map(str::to_owned),
                value: con.value,
                mitcad,
                parameter: None,
            });
        } else {
            v = driven.clone();
        }
        self.out.dimensions.push(v);
        self.out.driven_dimensions.push(driven);
    }

    /// The points a curve is drawn through (a line's start and end).
    fn line_ends(&mut self, geo: i32) -> Option<(u32, u32)> {
        Some((
            self.at(geo, PointPos::Start).ok()?,
            self.at(geo, PointPos::End).ok()?,
        ))
    }

    fn constraint(&mut self, i: usize, con: &fc::Constraint) {
        let kind = con.kind;
        if !con.active {
            self.out
                .notes
                .push(format!("constraint {i} ({}): switched off", kind.name()));
            return;
        }
        if let Err(why) = self.translate(i, con) {
            self.drop_constraint(i, kind, why);
        }
    }

    fn translate(&mut self, i: usize, con: &fc::Constraint) -> Result<(), String> {
        let (first, second, third) = (con.first, con.second, con.third);
        let curve = |t: &mut Self, geo: i32| -> Result<(u32, Kind), String> {
            match t.curve(geo) {
                Found::Curve(id, kind) => Ok((id, kind)),
                Found::Internal => Err("internal geometry".to_owned()),
                Found::Missing(why) => Err(why),
            }
        };
        match con.kind {
            T::None | T::InternalAlignment => {}
            T::Coincident => {
                if self.refused.contains(&i) {
                    return Err("it would join two points of one curve".to_owned());
                }
                // Merged; check that both points exist.
                self.point(first)?;
                self.point(second)?;
            }
            T::Weight => {
                if !matches!(self.curve(first.geo), Found::Internal) {
                    return Err("not on a control point".to_owned());
                }
                self.out.notes.push(format!(
                    "constraint {i} (Weight): the weight {} is the spline's",
                    con.value
                ));
            }
            T::Horizontal | T::Vertical => {
                let t = if con.kind == T::Horizontal {
                    "horizontal"
                } else {
                    "vertical"
                };
                if first.is_point() && second.is_point() {
                    let (a, b) = (self.point(first)?, self.point(second)?);
                    if a != b {
                        self.push(json!({"type": format!("{t}_points"), "a": p(a), "b": p(b)}));
                    }
                } else {
                    match curve(self, first.geo)? {
                        (l, Kind::Line) => self.push(json!({"type": t, "line": c(l)})),
                        _ => return Err("not on a line".to_owned()),
                    }
                }
            }
            T::Parallel => match (curve(self, first.geo)?, curve(self, second.geo)?) {
                ((a, Kind::Line), (b, Kind::Line)) => {
                    self.push(json!({"type": "parallel", "a": c(a), "b": c(b)}));
                }
                _ => return Err("not between two lines".to_owned()),
            },
            T::Perpendicular | T::Tangent => {
                if self.refused.contains(&i) {
                    return Err("it would join two points of one curve".to_owned());
                }
                let a = curve(self, first.geo)?;
                let b = curve(self, second.geo)?;
                // Where they meet: merged ends, an end on the other curve,
                // or a third point on both.
                if !third.is_undef() {
                    let q = self.point(third)?;
                    self.on_curve(q, a.0)?;
                    self.on_curve(q, b.0)?;
                } else if first.is_point() && !second.is_point() {
                    let q = self.point(first)?;
                    self.on_curve(q, b.0)?;
                } else if second.is_point() && !first.is_point() {
                    let q = self.point(second)?;
                    self.on_curve(q, a.0)?;
                }
                if con.kind == T::Tangent {
                    self.tangent(a, b)?;
                } else {
                    self.perpendicular(a, b)?;
                }
            }
            T::Distance => self.distance(i, con)?,
            T::DistanceX | T::DistanceY => {
                let (a, b) = if first.is_point() && second.is_point() {
                    (self.point(first)?, self.point(second)?)
                } else if first.is_point() {
                    (self.root_point(), self.point(first)?)
                } else {
                    match curve(self, first.geo)? {
                        (_, Kind::Line) => {
                            self.line_ends(first.geo).ok_or("its ends are missing")?
                        }
                        _ => return Err("not on a line".to_owned()),
                    }
                };
                let t = if con.kind == T::DistanceX {
                    "horizontal_distance"
                } else {
                    "vertical_distance"
                };
                let (a, b, v) = if con.value < 0.0 {
                    (b, a, -con.value)
                } else {
                    (a, b, con.value)
                };
                if a == b {
                    return Err("between a point and itself".to_owned());
                }
                self.dimension(i, con, json!({"type": t, "a": p(a), "b": p(b)}), v);
            }
            T::Angle => self.angle(i, con)?,
            T::Radius | T::Diameter => match self.curve(first.geo) {
                Found::Internal => {}
                Found::Curve(id, kind) if kind.is_round() => {
                    let t = if con.kind == T::Radius {
                        "radius"
                    } else {
                        "diameter"
                    };
                    if con.value <= 0.0 {
                        return Err("not positive".to_owned());
                    }
                    self.dimension(i, con, json!({"type": t, "curve": c(id)}), con.value);
                }
                Found::Curve(..) => return Err("not on a circle or an arc".to_owned()),
                Found::Missing(why) => return Err(why),
            },
            T::Equal => match (self.curve(first.geo), self.curve(second.geo)) {
                (Found::Internal, Found::Internal) => {}
                (Found::Curve(a, ka), Found::Curve(b, kb))
                    if ka == Kind::Line && kb == Kind::Line || ka.is_round() && kb.is_round() =>
                {
                    self.push(json!({"type": "equal", "a": c(a), "b": c(b)}));
                }
                (Found::Missing(why), _) | (_, Found::Missing(why)) => return Err(why),
                _ => return Err("equal sizes of these curves are not in Mitcad".to_owned()),
            },
            T::PointOnObject => {
                let q = self.point(first)?;
                match second.geo {
                    H_AXIS | V_AXIS => {
                        let root = self.root_point();
                        if q != root {
                            let t = if second.geo == H_AXIS {
                                "horizontal_points"
                            } else {
                                "vertical_points"
                            };
                            self.push(json!({"type": t, "a": p(root), "b": p(q)}));
                        }
                    }
                    g => {
                        let (target, _) = curve(self, g)?;
                        self.on_curve(q, target)?;
                    }
                }
            }
            T::Symmetric => {
                let (a, b) = (self.point(first)?, self.point(second)?);
                if a == b {
                    return Err("of a point and itself".to_owned());
                }
                if third.is_point() {
                    // About a point: the middle of a construction line
                    // between them.
                    let m = self.point(third)?;
                    let l = self.alloc();
                    self.push_entity(
                        l,
                        json!({"id": c(l), "type": "line", "start": p(a), "end": p(b),
                               "construction": true}),
                    );
                    self.push(json!({"type": "midpoint", "point": p(m), "curve": c(l)}));
                } else {
                    match curve(self, third.geo)? {
                        (axis, Kind::Line) => self.push(
                            json!({"type": "symmetric", "a": p(a), "b": p(b), "axis": c(axis)}),
                        ),
                        _ => return Err("about a curve that is not a line".to_owned()),
                    }
                }
            }
            T::Block => {}
            T::SnellsLaw => return Err("Mitcad has no refraction constraint".to_owned()),
            T::Group | T::Text | T::Other(_) => {
                return Err("a kind of constraint Mitcad has not".to_owned());
            }
        }
        Ok(())
    }

    /// A point on a curve, unless it is one of the curve's own points.
    fn on_curve(&mut self, q: u32, curve: u32) -> Result<(), String> {
        let own = self
            .entity_at
            .get(&curve)
            .map(|&at| &self.out.entities[at])
            .is_some_and(|e| {
                ["start", "end"]
                    .iter()
                    .any(|k| e[*k].as_str() == Some(p(q).as_str()))
                    || e["control"].as_array().is_some_and(|c| {
                        c.first() == Some(&json!(p(q))) || c.last() == Some(&json!(p(q)))
                    })
            });
        if !own {
            self.push(json!({"type": "coincident", "point": p(q), "entity": c(curve)}));
        }
        Ok(())
    }

    fn tangent(&mut self, a: (u32, Kind), b: (u32, Kind)) -> Result<(), String> {
        let ((ia, ka), (ib, kb)) = (a, b);
        if ka == Kind::Line && kb == Kind::Line {
            self.push(json!({"type": "collinear", "a": c(ia), "b": c(ib)}));
            return Ok(());
        }
        let allowed = (ka.simple() && kb.simple())
            || (ka == Kind::Spline && kb != Kind::Ellipse && kb != Kind::EllipticalArc)
            || (kb == Kind::Spline && ka != Kind::Ellipse && ka != Kind::EllipticalArc);
        if !allowed {
            return Err("Mitcad's tangency does not take ellipses".to_owned());
        }
        self.push(json!({"type": "tangent", "a": c(ia), "b": c(ib)}));
        Ok(())
    }

    fn perpendicular(&mut self, a: (u32, Kind), b: (u32, Kind)) -> Result<(), String> {
        let ((ia, ka), (ib, kb)) = (a, b);
        match (ka, kb) {
            (Kind::Line, Kind::Line) => {
                self.push(json!({"type": "perpendicular", "a": c(ia), "b": c(ib)}));
            }
            // A line at right angles to a circle runs through its centre.
            (Kind::Line, k) | (k, Kind::Line) if k.is_round() => {
                let (line, round) = if ka == Kind::Line { (ia, ib) } else { (ib, ia) };
                let center = self
                    .entity_at
                    .get(&round)
                    .and_then(|&at| self.out.entities[at]["center"].as_str())
                    .and_then(|s| s.strip_prefix('p')?.parse::<u32>().ok())
                    .ok_or("the circle has no centre")?;
                self.on_curve(center, line)?;
            }
            _ => return Err("Mitcad's right angles are between lines".to_owned()),
        }
        Ok(())
    }

    fn distance(&mut self, i: usize, con: &fc::Constraint) -> Result<(), String> {
        let (first, second) = (con.first, con.second);
        if con.value <= 0.0 {
            return Err("not positive".to_owned());
        }
        let found = |t: &mut Self, geo: i32| match t.curve(geo) {
            Found::Curve(id, kind) => Ok((id, kind)),
            Found::Internal => Err("internal geometry".to_owned()),
            Found::Missing(why) => Err(why),
        };
        let v = if first.is_point() && second.is_point() {
            let (a, b) = (self.point(first)?, self.point(second)?);
            if a == b {
                return Err("between a point and itself".to_owned());
            }
            json!({"type": "distance", "a": p(a), "b": p(b)})
        } else if first.is_point() && !second.is_undef() {
            let q = self.point(first)?;
            match found(self, second.geo)? {
                (l, Kind::Line) => {
                    json!({"type": "point_line_distance", "point": p(q), "line": c(l)})
                }
                _ => return Err("a point's distance from a curve other than a line".to_owned()),
            }
        } else if second.is_undef() {
            match found(self, first.geo)? {
                (l, Kind::Line) => json!({"type": "length", "line": c(l)}),
                (a, Kind::Arc) => json!({"type": "arc_length", "arc": c(a)}),
                _ => return Err("the length of a curve other than a line or an arc".to_owned()),
            }
        } else {
            match (found(self, first.geo)?, found(self, second.geo)?) {
                ((a, Kind::Line), (b, Kind::Line)) => {
                    json!({"type": "line_distance", "a": c(a), "b": c(b)})
                }
                _ => return Err("the distance between curves other than lines".to_owned()),
            }
        };
        self.dimension(i, con, v, con.value);
        Ok(())
    }

    fn angle(&mut self, i: usize, con: &fc::Constraint) -> Result<(), String> {
        let (first, second, third) = (con.first, con.second, con.third);
        let found = |t: &mut Self, geo: i32| match t.curve(geo) {
            Found::Curve(id, kind) => Ok((id, kind)),
            Found::Internal => Err("internal geometry".to_owned()),
            Found::Missing(why) => Err(why),
        };
        if second.is_undef() {
            return match found(self, first.geo)? {
                // A line's direction from the sketch's x axis.
                (l, Kind::Line) => {
                    let h = self.axis(0);
                    let v = wrap(con.value).abs();
                    self.dimension(i, con, json!({"type": "angle", "a": c(h), "b": c(l)}), v);
                    Ok(())
                }
                // An arc's sweep: between construction lines from its
                // centre to its ends.
                (_, Kind::Arc) => {
                    let sweep = con.value;
                    if !(0.0..std::f64::consts::PI).contains(&sweep) || sweep == 0.0 {
                        return Err("an arc's angle of 180 degrees or more".to_owned());
                    }
                    let center = self.at(first.geo, PointPos::Mid)?;
                    let start = self.at(first.geo, PointPos::Start)?;
                    let end = self.at(first.geo, PointPos::End)?;
                    let mut lines = [0u32; 2];
                    for (slot, end) in lines.iter_mut().zip([start, end]) {
                        let l = self.alloc();
                        self.push_entity(
                            l,
                            json!({"id": c(l), "type": "line", "start": p(center),
                                   "end": p(end), "construction": true}),
                        );
                        *slot = l;
                    }
                    self.dimension(
                        i,
                        con,
                        json!({"type": "angle", "a": c(lines[0]), "b": c(lines[1])}),
                        sweep,
                    );
                    Ok(())
                }
                _ => Err("the angle of a curve other than a line or an arc".to_owned()),
            };
        }
        let ((a, ka), (b, kb)) = (found(self, first.geo)?, found(self, second.geo)?);
        if ka != Kind::Line || kb != Kind::Line {
            return Err("an angle between curves other than lines".to_owned());
        }
        if a == b {
            return Err("between a line and itself".to_owned());
        }
        // FreeCAD measures from the given ends along the lines; Mitcad
        // between their directions.
        let sign = |r: GeoRef| if r.pos == PointPos::End { -1.0 } else { 1.0 };
        let flip = if third.is_undef() {
            sign(first) * sign(second)
        } else {
            let q = self.point(third)?;
            self.on_curve(q, a)?;
            self.on_curve(q, b)?;
            1.0
        };
        let v = if flip > 0.0 {
            wrap(con.value).abs()
        } else {
            std::f64::consts::PI - wrap(con.value).abs()
        };
        self.dimension(i, con, json!({"type": "angle", "a": c(a), "b": c(b)}), v);
        Ok(())
    }

    /// Blocked geometry: fixed, with its points.
    fn block(&mut self) {
        let mut blocked: BTreeSet<i32> = self
            .sketch
            .constraints
            .iter()
            .filter(|c| c.active && c.kind == T::Block)
            .map(|c| c.first.geo)
            .collect();
        for (g, geo) in self.sketch.geometry.iter().enumerate() {
            if geo.blocked {
                blocked.insert(g as i32);
            }
        }
        for g in blocked {
            let mut ids = Vec::new();
            if let Some(&(id, _)) = self.curves.get(&g) {
                ids.push(id);
            }
            for pos in [PointPos::Start, PointPos::End, PointPos::Mid] {
                if let Some(v) = self.vertex(GeoRef::new(g, pos))
                    && let Some(id) = self.point_of(v)
                {
                    ids.push(id);
                }
            }
            if let Some(&(id, _)) = self.curves.get(&g)
                && let Some(&at) = self.entity_at.get(&id)
            {
                for key in ["control", "major"] {
                    let refs: Vec<String> = match &self.out.entities[at][key] {
                        Value::Array(a) => a
                            .iter()
                            .filter_map(|x| x.as_str().map(str::to_owned))
                            .collect(),
                        Value::String(s) => vec![s.clone()],
                        _ => Vec::new(),
                    };
                    ids.extend(
                        refs.iter()
                            .filter_map(|r| r.strip_prefix('p')?.parse::<u32>().ok()),
                    );
                }
            }
            for id in ids {
                if let Some(&at) = self.entity_at.get(&id) {
                    self.out.entities[at]["fixed"] = json!(true);
                }
            }
        }
    }
}

/// The middle control point and its weight of the rational quadratic
/// Bézier curve that is a hyperbola's or a parabola's arc: where the
/// tangents at its ends meet.
fn conic_middle(curve: &Curve) -> Option<([f64; 2], f64)> {
    match curve {
        Curve::ArcOfHyperbola {
            center,
            major,
            minor,
            frame,
            start,
            end,
        } => {
            let (h, m) = ((end - start) / 2.0, (end + start) / 2.0);
            let w = h.cosh();
            (h.abs() > 1e-12).then(|| {
                (
                    frame.place(*center, major * m.cosh() / w, minor * m.sinh() / w),
                    w,
                )
            })
        }
        Curve::ArcOfParabola {
            vertex,
            focal,
            frame,
            start,
            end,
        } => ((end - start).abs() > 1e-12 && *focal != 0.0).then(|| {
            (
                frame.place(*vertex, start * end / (4.0 * focal), (start + end) / 2.0),
                1.0,
            )
        }),
        _ => None,
    }
}
