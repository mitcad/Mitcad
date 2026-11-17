// SPDX-License-Identifier: MIT
//! Flex (mitcad#1713): a flexible item (a wire, a cable, a hose, a flat
//! cable, a strip, a belt) laid through the model. The definition stores
//! the item ([`FlexType`]: its section, wall, bend radii), its nodes in
//! order ([`FlexNode`]: free in space, on a face, an edge or a point of a
//! body, or on the end port of a pipe or of another Flex item) and its
//! options. The route between the nodes is derived on every evaluation by
//! the geometry kernel ([`Kernel::flex_route`]): the shortest one that
//! passes through no body and stays within the bend radius; the solid is
//! [`Kernel::flex_solid`] along it, of any section.
//!
//! A route that is not clean does not fail the feature: the item is built
//! on the best route found and the feature warns, each warning with the
//! places it is about ([`FlexProblemKind::message`]: plain words, no
//! names, ids or coordinates). A node whose face, edge, point or port is
//! gone stays at its last place (`at`) and warns too.
//!
//! What the evaluation found is kept with the result ([`FlexOutput`],
//! `Output::flex`), also when the solid then failed: the nodes where they
//! are, the route's curve and samples, its length, the faces it runs on
//! and its problems. The `flex` query and the handles in the view read
//! it. `commands.md`, "Flex", is the contract; the node edits of the
//! `flex.*` commands are the methods of [`FlexDef`] here.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::extrude::{ProfileRef, check_profiles, profile_references, profile_regions};
use super::extrude_scope::assembly_key;
use super::geom_ref::{GeomRef, PathRef, Resolver, Want};
use super::mapping::map_params;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureDef, FeatureInfo, FeatureOutput,
    References,
};
use crate::datum::{PathParameter, Vec3};
use crate::expr::{ParamKind, Unit};
use crate::flex::{
    EndPort, FlexRoute, FlexRouteSpec, FlexSection, FlexSolidSpec, FlexType, RouteFace, RouteNode,
    RouteProblemKind, RouteSample,
};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::links::OccurrenceLink;
use crate::notice::NoticePlace;
use crate::parameters::{ParamId, Parameters};
use crate::recompute::Read;
use crate::sweeps::PathCurve;
use crate::topo::FaceName;
use crate::transform::Transform;

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

/// A node of a Flex item: a point its route passes through.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlexNode {
    /// `n<k>`, unique in the item.
    pub id: String,
    /// In the feature's component: a free node's place; for an attached
    /// one where it was last put (the node is the point of what it is
    /// attached to nearest to this), and where it stays when that is gone.
    pub at: Vec3,
    /// What it is attached to: a face (the node lies on it and the item
    /// rests on it there), an edge (its nearest point), a vertex, a sketch
    /// point, a construction point or the origin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<GeomRef>,
    /// The end port it is on: the cap face of a pipe or of another Flex
    /// item. Only the first or the last node; never with `on`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<GeomRef>,
    /// Radians the section is turned about the route here, right hand
    /// about the route's direction.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub twist: f64,
}

impl FlexNode {
    /// A free node at `at`; its id is given when it is added.
    pub fn free(at: Vec3) -> Self {
        Self {
            id: String::new(),
            at,
            on: None,
            port: None,
            twist: 0.0,
        }
    }
}

/// What a node is attached to, in a command.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeSupport {
    /// Nothing: free in space.
    Free,
    /// A face, an edge or a point ([`FlexNode::on`]).
    On(GeomRef),
    /// An end port ([`FlexNode::port`]).
    Port(GeomRef),
}

/// Where a new node goes among the item's nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodePlace {
    /// Before the first node.
    Start,
    /// After the last node: the item grows.
    End,
    /// Between this node and the next.
    After(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlexDef<P = ParamId> {
    /// The item, whole: a copy of a Flex type or one written in the design.
    pub item: FlexType,
    /// A closed region of a sketch as the section instead of the item's,
    /// in the sketch's own coordinates as section coordinates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<ProfileRef>,
    /// This item's own bend radius in place of its type's.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub bend_radius: Option<P>,
    /// Likewise over the section's edge.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub bend_radius_edge: Option<P>,
    /// A fixed length, whose slack will hang; today only reported against
    /// the routed length ([`FlexOutput::slack`]).
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub fixed_length: Option<P>,
    /// The item's own reference parameter, set to the route's length on
    /// every evaluation: later features and expressions use it. Given
    /// to the item when it is made ([`settle_length`]).
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub length: Option<P>,
    /// Two or more, from the item's start to its end.
    pub nodes: Vec<FlexNode>,
    /// Bodies of other components that nodes are attached to, as a
    /// construction plane's `links`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub links: BTreeMap<BodyUid, OccurrenceLink>,
    /// Construction features and sketches of other components that nodes
    /// are attached to, as a construction plane's `datum_links`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub datum_links: BTreeMap<FeatureUid, OccurrenceLink>,
    /// For previews: only the route is worked out, no solid is built
    /// and the item has no body. The `flex` query's `preview` reads the
    /// route; a command that would store it is refused.
    #[serde(default, skip_serializing_if = "is_false")]
    pub route_only: bool,
}

/// The person's words and the agent's detail of a node that is not there.
fn no_node(id: &str) -> String {
    crate::notice::said(
        "the item has no such node",
        format!("the item has no node {id}"),
    )
}

/// The comment of an item's length parameter: `Flex1 length (driven)`, as
/// a driven dimension's.
fn length_comment(owner_name: &str) -> String {
    format!("{owner_name} length (driven)")
}

/// Gives a Flex item its length parameter (mitcad#1713): a reference
/// parameter of its own, which every evaluation sets to the route's
/// length, so that expressions and later features can use it. The one the
/// definition names stays (a number given as `length` makes it, as a bound
/// length's first value does); else the one the item had (`old`); else a
/// new one with the next free name. The names of the parameters made.
pub(crate) fn settle_length(
    def: &mut FlexDef,
    params: &mut Parameters,
    owner: FeatureUid,
    owner_name: &str,
    old: Option<ParamId>,
) -> Result<Vec<String>, String> {
    let own = |params: &Parameters, id: ParamId| {
        params.get(id).is_some() && params.owner(id) == Some(owner)
    };
    if let Some(id) = def.length
        && !own(params, id)
    {
        return Err(format!(
            "length: a flex item's length is measured into a parameter of its own; \
             leave it out, not “{}”",
            params.name(id)
        ));
    }
    let mut created = Vec::new();
    let id = match def.length.or(old.filter(|id| own(params, *id))) {
        Some(id) => {
            params.set_kind(id, ParamKind::Reference);
            id
        }
        None => {
            let unit = Unit::of_length(params.context().default_length_unit);
            let id = params
                .add_reference(0.0, unit, &length_comment(owner_name), owner)
                .map_err(|e| format!("length: {e}"))?;
            created.push(params.name(id));
            id
        }
    };
    def.length = Some(id);
    Ok(created)
}

impl<P> FlexDef<P> {
    pub const TYPE: &'static str = "flex";
    pub const BASE_NAME: &'static str = "Flex";

    map_params! {
        FlexDef {
            item: clone,
            profile: clone,
            bend_radius: option,
            bend_radius_edge: option,
            fixed_length: option,
            length: option,
            nodes: clone,
            links: clone,
            datum_links: clone,
            route_only: clone,
        }
    }

    /// An item through `nodes`, with nothing of its own beside the item.
    pub fn new(item: FlexType, nodes: Vec<FlexNode>) -> Self {
        Self {
            item,
            profile: None,
            bend_radius: None,
            bend_radius_edge: None,
            fixed_length: None,
            length: None,
            nodes,
            links: BTreeMap::new(),
            datum_links: BTreeMap::new(),
            route_only: false,
        }
    }

    /// Where the node `id` is among the nodes.
    pub fn node_index(&self, id: &str) -> Result<usize, String> {
        self.nodes
            .iter()
            .position(|node| node.id == id)
            .ok_or_else(|| no_node(id))
    }

    /// The id a new node gets: `n<k>`, one past the largest in use.
    pub fn next_node_id(&self) -> String {
        let largest = self
            .nodes
            .iter()
            .filter_map(|node| node.id.strip_prefix('n')?.parse::<u32>().ok())
            .max()
            .unwrap_or(0);
        format!("n{}", largest + 1)
    }

    /// What the nodes must be, whatever they refer to: two or more, each
    /// with an id of its own, finite, attached to one thing at most, and a
    /// port only at an end.
    pub fn check_nodes(&self) -> Result<(), String> {
        if self.nodes.len() < 2 {
            return Err("the item needs at least two nodes".to_owned());
        }
        let last = self.nodes.len() - 1;
        for (i, node) in self.nodes.iter().enumerate() {
            let valid = node
                .id
                .strip_prefix('n')
                .is_some_and(|k| !k.is_empty() && k.bytes().all(|b| b.is_ascii_digit()));
            if !valid {
                return Err(crate::notice::said(
                    "a node's id is not valid",
                    format!("the node id {:?} is not n<number>", node.id),
                ));
            }
            if self.nodes[..i].iter().any(|other| other.id == node.id) {
                return Err(crate::notice::said(
                    "two nodes have the same id",
                    format!("the node id {} is used twice", node.id),
                ));
            }
            if !(node.at.iter().all(|v| v.is_finite()) && node.twist.is_finite()) {
                return Err("a node's place and twist must be finite numbers".to_owned());
            }
            if node.on.is_some() && node.port.is_some() {
                return Err(
                    "a node is either on a face, an edge or a point, or on an end port, not both"
                        .to_owned(),
                );
            }
            if node.port.is_some() && i != 0 && i != last {
                return Err("only the item's first or last node can be on an end port".to_owned());
            }
        }
        Ok(())
    }

    /// Adds `node` (its id is given here) at `place`; the new id. Nothing
    /// changes when the nodes would not be valid then.
    pub fn add_node(&mut self, mut node: FlexNode, place: &NodePlace) -> Result<String, String> {
        let index = match place {
            NodePlace::Start => 0,
            NodePlace::End => self.nodes.len(),
            NodePlace::After(id) => self.node_index(id)? + 1,
        };
        let id = self.next_node_id();
        node.id.clone_from(&id);
        self.nodes.insert(index, node);
        if let Err(error) = self.check_nodes() {
            self.nodes.remove(index);
            return Err(error);
        }
        Ok(id)
    }

    /// Puts the node at `at`: a free node there, an attached one at the
    /// point of what it is attached to nearest to it.
    pub fn move_node(&mut self, id: &str, at: Vec3) -> Result<(), String> {
        if !at.iter().all(|v| v.is_finite()) {
            return Err("a node's place must be finite numbers".to_owned());
        }
        let index = self.node_index(id)?;
        self.nodes[index].at = at;
        Ok(())
    }

    /// Attaches the node to `support` instead of what it was attached to,
    /// at `at` when given.
    pub fn attach_node(
        &mut self,
        id: &str,
        support: NodeSupport,
        at: Option<Vec3>,
    ) -> Result<(), String> {
        let index = self.node_index(id)?;
        let before = self.nodes[index].clone();
        let node = &mut self.nodes[index];
        (node.on, node.port) = match support {
            NodeSupport::Free => (None, None),
            NodeSupport::On(on) => (Some(on), None),
            NodeSupport::Port(port) => (None, Some(port)),
        };
        if let Some(at) = at {
            node.at = at;
        }
        if let Err(error) = self.check_nodes() {
            self.nodes[index] = before;
            return Err(error);
        }
        Ok(())
    }

    /// Takes the node away: the route goes from the node before it to the
    /// node after it.
    pub fn delete_node(&mut self, id: &str) -> Result<(), String> {
        let index = self.node_index(id)?;
        if self.nodes.len() <= 2 {
            return Err("the item needs at least two nodes; delete the item instead".to_owned());
        }
        self.nodes.remove(index);
        Ok(())
    }

    /// Records where the geometry of a node's reference is when it is of
    /// another component: `link` for the reference's body (`links`) or its
    /// construction feature or sketch (`datum_links`). None (the item's
    /// own component) takes a link of that geometry away.
    pub fn set_link(&mut self, reference: &GeomRef, link: Option<OccurrenceLink>) {
        match reference {
            GeomRef::Face { body, .. }
            | GeomRef::Edge { body, .. }
            | GeomRef::Vertex { body, .. }
            | GeomRef::Body(body) => match link {
                Some(link) => {
                    self.links.insert(*body, link);
                }
                None => {
                    self.links.remove(body);
                }
            },
            GeomRef::Datum(feature)
            | GeomRef::SketchPoint {
                sketch: feature, ..
            } => match link {
                Some(link) => {
                    self.datum_links.insert(*feature, link);
                }
                None => {
                    self.datum_links.remove(feature);
                }
            },
            _ => {}
        }
    }

    /// Drops the links of geometry that no node is attached to any more.
    pub fn prune_links(&mut self) {
        let mut bodies = Vec::new();
        let mut features = Vec::new();
        for node in &self.nodes {
            for reference in [&node.on, &node.port].into_iter().flatten() {
                match reference {
                    GeomRef::Face { body, .. }
                    | GeomRef::Edge { body, .. }
                    | GeomRef::Vertex { body, .. }
                    | GeomRef::Body(body) => bodies.push(*body),
                    GeomRef::Datum(feature)
                    | GeomRef::SketchPoint {
                        sketch: feature, ..
                    } => features.push(*feature),
                    _ => {}
                }
            }
        }
        self.links.retain(|body, _| bodies.contains(body));
        self.datum_links
            .retain(|feature, _| features.contains(feature));
    }

    /// Turns the section about the route at the node.
    pub fn set_twist(&mut self, id: &str, twist: f64) -> Result<(), String> {
        if !twist.is_finite() {
            return Err("a node's twist must be a finite number".to_owned());
        }
        let index = self.node_index(id)?;
        self.nodes[index].twist = twist;
        Ok(())
    }
}

/// How a node is held, as the `flex` query says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlexSupport {
    Free,
    Face,
    Edge,
    Point,
    Port,
}

/// What is wrong with an item that was still built: the route's problems
/// and the nodes whose attachment is gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlexProblemKind {
    PassesThrough,
    BendTooTight,
    TwistOverLimit,
    NoCleanRoute,
    FaceLost,
    ReferenceLost,
    PortLost,
    /// A section that is not round bends over its edge more tightly than
    /// its type allows (`BendTooTight` is then about its flat side).
    EdgeBendTooTight,
    /// The item's fixed length is shorter than its route (mitcad#1713,
    /// Part B).
    TooShort,
}

impl FlexProblemKind {
    /// The warning a person reads: what is wrong in plain words, without
    /// names, ids or coordinates (mitcad#1612); where it is goes into the
    /// warning's places.
    pub fn message(self) -> &'static str {
        match self {
            Self::PassesThrough => "the item passes through a body",
            Self::BendTooTight => "the item bends more tightly than its bend radius allows",
            Self::TwistOverLimit => "the item twists more than its type allows",
            Self::NoCleanRoute => {
                "no route was found that passes through nothing within the bend radius; \
                 the item is drawn on the nearest one"
            }
            Self::FaceLost => "a face a node was on no longer exists; the node stays where it was",
            Self::ReferenceLost => {
                "what a node was attached to no longer exists; the node stays where it was"
            }
            Self::PortLost => {
                "the end the item was attached to no longer exists; the node stays where it was"
            }
            Self::EdgeBendTooTight => {
                "the item bends over its edge more tightly than its type allows"
            }
            Self::TooShort => "the item's fixed length is too short to reach between its ends",
        }
    }
}

impl From<RouteProblemKind> for FlexProblemKind {
    fn from(kind: RouteProblemKind) -> Self {
        match kind {
            RouteProblemKind::PassesThrough => Self::PassesThrough,
            RouteProblemKind::BendTooTight => Self::BendTooTight,
            RouteProblemKind::TwistOverLimit => Self::TwistOverLimit,
            RouteProblemKind::NoCleanRoute => Self::NoCleanRoute,
            RouteProblemKind::EdgeBendTooTight => Self::EdgeBendTooTight,
        }
    }
}

/// A node as the evaluation placed it.
#[derive(Debug, Clone, PartialEq)]
pub struct FlexNodeOutput {
    pub id: String,
    /// Where it is, in the feature's component.
    pub point: Vec3,
    /// The outward normal of the face it lies on, there.
    pub normal: Option<Vec3>,
    /// Its length along the route.
    pub along: f64,
    pub support: FlexSupport,
    /// What it was attached to is gone: it is at its last place.
    pub lost: bool,
}

/// A stretch of the route that lies on a face.
#[derive(Debug, Clone, PartialEq)]
pub struct FlexRun {
    pub from: f64,
    pub to: f64,
    pub body: BodyUid,
    pub face: FaceName,
}

/// A problem of an item with what agents need of it.
#[derive(Debug, Clone, PartialEq)]
pub struct FlexProblem {
    pub kind: FlexProblemKind,
    /// The stretch of the route, lengths along it (both 0 for a lost node
    /// before the route is known).
    pub from: f64,
    pub to: f64,
    /// The lost node.
    pub node: Option<String>,
    /// The body passed through.
    pub body: Option<BodyUid>,
    pub places: Vec<NoticePlace>,
}

/// What a Flex item's evaluation found (see the module).
#[derive(Debug, Clone, PartialEq)]
pub struct FlexOutput {
    /// The section the item was built with: its profile's region as an
    /// outline, else the item's own.
    pub section: FlexSection,
    pub nodes: Vec<FlexNodeOutput>,
    /// The route's centre line, its pieces named `F7:route1`, ... in order;
    /// empty when no route was found.
    pub curve: Vec<PathCurve>,
    pub samples: Vec<RouteSample>,
    pub runs: Vec<FlexRun>,
    pub problems: Vec<FlexProblem>,
    /// The route's length.
    pub length: f64,
    pub fixed_length: Option<f64>,
    /// The bend radii the route was held to.
    pub bend_radius: Option<f64>,
    pub bend_radius_edge: Option<f64>,
}

impl FlexOutput {
    /// With a fixed length, that length less the routed one (negative:
    /// the fixed length is too short).
    pub fn slack(&self) -> Option<f64> {
        self.fixed_length.map(|fixed| fixed - self.length)
    }

    /// The route's centre line as points, to draw it.
    pub fn polyline(&self) -> Vec<Vec3> {
        self.samples.iter().map(|sample| sample.point).collect()
    }

    /// The tightest bend of the route as its samples show it: the radius
    /// of the circle through three neighbouring samples that is the
    /// smallest, with the middle one's length along the route and its
    /// point. None for a straight route. An estimate, as close as the
    /// samples are: what the route may not go below is the bend radius
    /// it was held to ([`Self::bend_radius`]). For a section that is not
    /// round its bends over the flat side (about its `x`) alone, against
    /// `bend_radius`; those over its edge are
    /// [`Self::smallest_bend_edge`]'s (mitcad#1713: a flat cable's
    /// warning about its edge stood beside the tightest bend of all).
    pub fn smallest_bend(&self) -> Option<(f64, f64, Vec3)> {
        let way = if self.section.is_round() {
            BendWay::Any
        } else {
            BendWay::Flat
        };
        self.tightest(way)
    }

    /// The tightest bend over the edge (about the section's `y`) of a
    /// section that is not round, as [`Self::smallest_bend`]; against
    /// `bend_radius_edge`. None for a round section, or a route that does
    /// not bend that way.
    pub fn smallest_bend_edge(&self) -> Option<(f64, f64, Vec3)> {
        if self.section.is_round() {
            return None;
        }
        self.tightest(BendWay::Edge)
    }

    fn tightest(&self, way: BendWay) -> Option<(f64, f64, Vec3)> {
        use mitcad_vec3::{cross, distance, dot, scale, sub};
        let mut least: Option<(f64, f64, Vec3)> = None;
        for three in self.samples.windows(3) {
            let (a, b, c) = (three[0].point, three[1].point, three[2].point);
            let ab = distance(a, b);
            let bc = distance(b, c);
            let ca = distance(c, a);
            // Twice the triangle's area.
            let normal = cross(sub(b, a), sub(c, a));
            let area2 = distance(normal, [0.0; 3]);
            let longest = ab.max(bc).max(ca);
            if !(ab > 1e-9 && bc > 1e-9) || area2 <= 1e-12 * longest * longest {
                continue;
            }
            let radius = match way {
                BendWay::Any => ab * bc * ca / (2.0 * area2),
                BendWay::Flat | BendWay::Edge => {
                    // The circle's centre from the middle point; the bend
                    // one way is its share along that way: the radius over
                    // the cosine of the angle between them.
                    let (u, w) = (sub(a, b), sub(c, b));
                    let centre = scale(
                        cross(sub(scale(w, dot(u, u)), scale(u, dot(w, w))), normal),
                        1.0 / (2.0 * area2 * area2),
                    );
                    let middle = &three[1];
                    let axis = match way {
                        BendWay::Flat => middle.normal,
                        _ => cross(middle.normal, middle.tangent),
                    };
                    let along = dot(centre, axis).abs();
                    let squared = dot(centre, centre);
                    if along <= 1e-9 * squared {
                        continue; // it does not bend that way here
                    }
                    squared / along
                }
            };
            if radius.is_finite() && least.is_none_or(|(r, _, _)| radius < r) {
                least = Some((radius, three[1].along, b));
            }
        }
        least
    }
}

/// Which bends `FlexOutput::tightest` measures.
#[derive(Clone, Copy)]
enum BendWay {
    Any,
    Flat,
    Edge,
}

/// A node placed for the route.
struct Placed {
    point: Vec3,
    /// The obstacle's index, the face's name there and its normal.
    face: Option<(usize, FaceName, Vec3)>,
    direction: Option<Vec3>,
    x_axis: Option<Vec3>,
    support: FlexSupport,
    lost: Option<FlexProblemKind>,
}

impl Placed {
    fn at(point: Vec3, support: FlexSupport) -> Self {
        Self {
            point,
            face: None,
            direction: None,
            x_axis: None,
            support,
            lost: None,
        }
    }

    /// Where a node stays whose attachment is gone.
    fn lost(node: &FlexNode, support: FlexSupport, kind: FlexProblemKind) -> Self {
        Self {
            lost: Some(kind),
            ..Self::at(node.at, support)
        }
    }
}

/// The error of a face that is no end of a pipe or of a Flex item.
fn not_a_port(port: &GeomRef) -> String {
    port.error(|what| format!("{what} is not an end of a pipe or of a flex item"))
}

/// Checks a node's port: the cap face of a pipe or of a Flex item before
/// the feature.
fn check_port(ctx: &CheckContext<'_>, port: &GeomRef) -> Result<(), String> {
    let GeomRef::Face { face, .. } = port else {
        return Err(not_a_port(port));
    };
    port.check(ctx, Want::Face)?;
    let maker = ctx.feature(face.feature)?;
    let cap = matches!(face.role.as_str(), "start" | "end");
    if cap && matches!(maker.def, FeatureDef::Pipe(_) | FeatureDef::Flex(_)) {
        Ok(())
    } else {
        Err(not_a_port(port))
    }
}

/// The end port at a node's `port` face.
fn port_at<K: Kernel>(ctx: &mut EvalContext<'_, K>, port: &GeomRef) -> Result<EndPort, String> {
    let GeomRef::Face { body, face } = port else {
        return Err(not_a_port(port));
    };
    let shape = Resolver::shape(ctx, *body)?;
    let name = super::renamed::face_for(ctx, &shape, face)?;
    ctx.kernel
        .end_port(&shape, &name)
        .map_err(|e| e.to_string())
}

/// A node on a face: the face's point nearest to where the node was put,
/// its body among the obstacles (added when it is a linked one).
fn face_at<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    on: &GeomRef,
    body: BodyUid,
    face: &FaceName,
    near: Vec3,
    obstacles: &mut Vec<(BodyUid, K::Shape)>,
) -> Result<Placed, String> {
    let found = ctx.face_point(on, near)?;
    let shape = Resolver::shape(ctx, body)?;
    let name = super::renamed::face_for(ctx, &shape, face)?;
    let obstacle = match obstacles.iter().position(|(uid, _)| *uid == body) {
        Some(index) => index,
        None => {
            obstacles.push((body, shape));
            obstacles.len() - 1
        }
    };
    Ok(Placed {
        face: Some((obstacle, name, found.normal)),
        ..Placed::at(found.point, FlexSupport::Face)
    })
}

/// Where a node is now: on what it is attached to, or at its last place
/// when that is gone.
fn place_node<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    node: &FlexNode,
    obstacles: &mut Vec<(BodyUid, K::Shape)>,
) -> Placed {
    if let Some(port) = &node.port {
        return match port_at(ctx, port) {
            Ok(found) => Placed {
                direction: Some(found.direction),
                x_axis: Some(found.x_axis),
                ..Placed::at(found.position, FlexSupport::Port)
            },
            Err(_) => Placed::lost(node, FlexSupport::Port, FlexProblemKind::PortLost),
        };
    }
    match &node.on {
        None => Placed::at(node.at, FlexSupport::Free),
        Some(on @ GeomRef::Face { body, face }) => {
            face_at(ctx, on, *body, face, node.at, obstacles).unwrap_or_else(|_| {
                Placed::lost(node, FlexSupport::Face, FlexProblemKind::FaceLost)
            })
        }
        Some(GeomRef::Edge { body, edge }) => {
            let path = PathRef::Edges {
                body: *body,
                edges: vec![edge.clone()],
            };
            match ctx.path_point(&path, PathParameter::Near(node.at)) {
                Ok(found) => Placed::at(found.point, FlexSupport::Edge),
                Err(_) => Placed::lost(node, FlexSupport::Edge, FlexProblemKind::ReferenceLost),
            }
        }
        Some(other) => match ctx.point(other) {
            Ok(point) => Placed::at(point, FlexSupport::Point),
            Err(_) => Placed::lost(node, FlexSupport::Point, FlexProblemKind::ReferenceLost),
        },
    }
}

/// The placements along `path` from the root, one after the other: where
/// the path's component is in the design. None when an occurrence on it
/// is not placed at this point of the timeline.
fn placed_along<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    path: &[OccurrenceUid],
) -> Option<Transform> {
    let mut transform = Transform::IDENTITY;
    for uid in path {
        let placed = ctx.placement(*uid);
        if !ctx.env.placements.contains_key(uid) {
            return None;
        }
        transform = transform.after(&placed);
    }
    Some(transform)
}

/// How far (mm) a fixed length may be from the route's before it is too
/// short or has slack to hang.
const LENGTH_TOLERANCE: f64 = 0.01;
/// The points a hanging stretch is given per stretch.
const HANGING_POINTS: usize = 7;
/// Routes made through hanging points at most, each asking for what the
/// one before lacked of the fixed length.
const HANGING_TRIES: usize = 3;

/// The design's direction of gravity in the feature's component's
/// coordinates (its first occurrence's placement turned back), None
/// without one.
fn gravity_here<K: Kernel>(ctx: &mut EvalContext<'_, K>) -> Option<Vec3> {
    let down = ctx.gravity()?;
    let own = ctx.component();
    if own == ComponentUid::ROOT {
        return Some(down);
    }
    let path = crate::links::first_path(ctx.env.assembly, own, None).ok()?;
    let place = placed_along(ctx, &path)?;
    crate::gravity::unit(crate::assembly::inverse(&place).apply_vector(down))
}

/// The route's nodes with the points of hanging curves added in its
/// stretches through the air, so that the item is about `fixed` long, and
/// each node's index among the item's (None for an added one). A stretch
/// is through the air when most of its length lies on no face; the slack
/// is shared among them by their lengths. A hanging point rests on what is
/// below it: no deeper than the first body its way down meets, less the
/// section's reach. None when no stretch can hang.
#[allow(clippy::too_many_arguments)]
fn hang_slack<'a, K: Kernel>(
    kernel: &K,
    route: &FlexRoute,
    nodes: &[RouteNode<'a>],
    shapes: &[&K::Shape],
    down: Vec3,
    fixed: f64,
    reach: f64,
    bend_radius: Option<f64>,
) -> Option<(Vec<RouteNode<'a>>, Vec<Option<usize>>)> {
    use mitcad_vec3::{add, distance, scale, sub};
    let samples = &route.samples;
    let at: Vec<Option<usize>> = (0..nodes.len())
        .map(|k| samples.iter().position(|s| s.node == Some(k)))
        .collect();
    // The stretches through the air and their lengths along the route.
    let mut free: Vec<(usize, f64)> = Vec::new();
    for k in 0..nodes.len().saturating_sub(1) {
        let (Some(s0), Some(s1)) = (at[k], at[k + 1]) else {
            continue;
        };
        if s1 <= s0 {
            continue;
        }
        let length = samples[s1].along - samples[s0].along;
        let in_air: f64 = samples[s0..=s1]
            .windows(2)
            .filter(|w| w[0].on.is_none() && w[1].on.is_none())
            .map(|w| w[1].along - w[0].along)
            .sum();
        if length > 0.0 && in_air > 0.5 * length {
            free.push((k, length));
        }
    }
    let shared: f64 = free.iter().map(|(_, l)| l).sum();
    if shared <= 0.0 {
        return None;
    }
    let slack = fixed - route.length();
    let mut out: Vec<RouteNode<'a>> = Vec::new();
    let mut of: Vec<Option<usize>> = Vec::new();
    let mut hung_any = false;
    for (k, node) in nodes.iter().enumerate() {
        out.push(*node);
        of.push(Some(k));
        let Some((_, length)) = free.iter().find(|(f, _)| *f == k) else {
            continue;
        };
        let next = &nodes[k + 1];
        let wanted = length + slack * length / shared;
        let Some(points) = crate::gravity::hanging(
            node.point,
            next.point,
            down,
            wanted,
            HANGING_POINTS,
            bend_radius,
        ) else {
            continue;
        };
        let chord = sub(next.point, node.point);
        let span = mitcad_vec3::dot(chord, chord);
        for (i, mut p) in points.into_iter().enumerate() {
            let t = (i + 1) as f64 / (HANGING_POINTS + 1) as f64;
            // Resting on what is below: from the point of the chord above it
            // down to it and the section's reach beyond.
            let along = (mitcad_vec3::dot(sub(p, node.point), chord) / span).clamp(0.0, 1.0);
            let from = add(node.point, scale(chord, along));
            let depth = distance(from, p);
            if depth > 1e-9 {
                let way = scale(sub(p, from), 1.0 / depth);
                let to = add(p, scale(way, reach));
                let first = shapes
                    .iter()
                    .filter_map(|s| kernel.segment_crossings(s, from, to).ok())
                    .flat_map(|crossings| {
                        crossings
                            .into_iter()
                            .filter(|(_, entering)| *entering)
                            .map(|(f, _)| f)
                            .next()
                    })
                    .fold(f64::INFINITY, f64::min);
                if first.is_finite() {
                    let reached = first * (depth + reach) - reach;
                    p = add(from, scale(way, reached.max(0.0)));
                }
            }
            out.push(RouteNode {
                point: p,
                face: None,
                direction: None,
                x_axis: None,
                twist: node.twist + (next.twist - node.twist) * t,
            });
            of.push(None);
            hung_any = true;
        }
    }
    hung_any.then_some((out, of))
}

/// Whether two transforms put a body at the same place.
fn same_place(a: &Transform, b: &Transform) -> bool {
    let near = |x: f64, y: f64| (x - y).abs() <= 1e-9 * (1.0 + x.abs().max(y.abs()));
    (0..3).all(|r| {
        near(a.translation[r], b.translation[r])
            && (0..3).all(|c| near(a.linear[r][c], b.linear[r][c]))
    })
}

/// Adds the solids of the rest of the design to the obstacles: every body
/// of every other component at this point of the timeline, once for each
/// place an occurrence puts it at, moved into the coordinates of the
/// feature's component (as its first occurrence places it). A linked body
/// a node is on is among the obstacles already, and is not added again
/// where its link has it. Shown or hidden makes no difference: the route
/// is the same whatever is on display.
fn add_other_components<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    links: &BTreeMap<BodyUid, OccurrenceLink>,
    obstacles: &mut Vec<(BodyUid, K::Shape)>,
) {
    // The occurrences there are and what they place: a new or a removed
    // one routes the item again.
    let key = assembly_key(&ctx.env);
    ctx.reads.push(Read::Assembly(key));
    let assembly = ctx.env.assembly;
    let own = ctx.component();
    let Ok(own_path) = crate::links::first_path(assembly, own, None) else {
        return;
    };
    let Some(own_place) = placed_along(ctx, &own_path) else {
        return;
    };
    let into_own = crate::assembly::inverse(&own_place);
    let mut linked: Vec<(BodyUid, Transform)> = Vec::new();
    for (body, link) in links {
        if obstacles.iter().any(|(uid, _)| uid == body)
            && let Ok((_, transform)) = ctx.linked(link)
        {
            linked.push((*body, transform));
        }
    }
    let components: Vec<ComponentUid> = std::iter::once(ComponentUid::ROOT)
        .chain(assembly.components.iter().map(|c| c.uid))
        .filter(|component| *component != own)
        .collect();
    for component in components {
        let shapes = ctx.bodies_in(component);
        if shapes.is_empty() {
            continue;
        }
        for path in assembly.paths_to(component) {
            let Some(place) = placed_along(ctx, &path) else {
                continue;
            };
            let transform = into_own.after(&place);
            for (uid, shape) in &shapes {
                let there = linked
                    .iter()
                    .any(|(body, at)| body == uid && same_place(at, &transform));
                if there {
                    continue;
                }
                if let Ok(moved) = ctx.kernel.transform_shape(shape, &transform, None) {
                    obstacles.push((*uid, moved));
                }
            }
        }
    }
}

/// The stretches of the samples that lie on one face each.
fn runs_of<S>(samples: &[RouteSample], obstacles: &[(BodyUid, S)]) -> Vec<FlexRun> {
    let mut runs: Vec<FlexRun> = Vec::new();
    // Whether the sample before lay on the last run's face.
    let mut open = false;
    for sample in samples {
        let on: Option<(BodyUid, &FaceName)> = sample
            .on
            .as_ref()
            .and_then(|(obstacle, face)| Some((obstacles.get(*obstacle)?.0, face)));
        let same = open
            && on.is_some_and(|(body, face)| {
                runs.last()
                    .is_some_and(|run| run.body == body && run.face == *face)
            });
        match on {
            Some(_) if same => {
                if let Some(run) = runs.last_mut() {
                    run.to = sample.along;
                }
            }
            Some((body, face)) => {
                runs.push(FlexRun {
                    from: sample.along,
                    to: sample.along,
                    body,
                    face: face.clone(),
                });
                open = true;
            }
            None => open = false,
        }
    }
    runs
}

impl FlexDef {
    /// The section the item is built with: its profile's region as an
    /// outline, else the item's own.
    fn section<K: Kernel>(&self, ctx: &mut EvalContext<'_, K>) -> Result<FlexSection, String> {
        let Some(profile) = &self.profile else {
            return Ok(self.item.section.clone());
        };
        let (_, regions) = profile_regions(ctx, std::slice::from_ref(profile))?;
        let region = regions.first().ok_or("the profile has no region")?;
        Ok(FlexSection::Outline {
            loops: region
                .loops
                .iter()
                .map(|l| l.segments.iter().map(|s| s.geometry.clone()).collect())
                .collect(),
        })
    }
}

impl FeatureInfo for FlexDef {
    fn references(&self) -> References {
        let mut references = References::default();
        if let Some(profile) = &self.profile {
            profile_references(std::slice::from_ref(profile), &mut references);
        }
        for node in &self.nodes {
            for reference in [&node.on, &node.port].into_iter().flatten() {
                reference.add_to(&mut references);
            }
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        self.check_nodes()?;
        self.item.check()?;
        if let Some(profile) = &self.profile {
            check_profiles(ctx, std::slice::from_ref(profile))?;
        }
        for node in &self.nodes {
            if let Some(on) = &node.on {
                let want = if matches!(on, GeomRef::Face { .. }) {
                    Want::Face
                } else {
                    Want::Point
                };
                on.check(ctx, want)?;
            }
            if let Some(port) = &node.port {
                check_port(ctx, port)?;
            }
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        for (what, id) in [
            ("the bend radius", self.bend_radius),
            ("the bend radius over the edge", self.bend_radius_edge),
            ("the fixed length", self.fixed_length),
        ] {
            if let Some(id) = id {
                let v = value(id);
                if !(v > 0.0 && v.is_finite()) {
                    return Err(format!("{what} must be greater than zero, got {v}"));
                }
            }
        }
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        !self.route_only
    }
}

impl<K: Kernel> Evaluate<K> for FlexDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        ctx.geom_links.clone_from(&self.links);
        ctx.datum_links.clone_from(&self.datum_links);
        self.check_nodes()?;
        let section = self.section(ctx)?;
        section.check(self.item.wall)?;
        let bend_radius = match self.bend_radius {
            Some(id) => Some(ctx.positive(id)?),
            None => self.item.bend_radius,
        };
        let bend_radius_edge = if section.is_round() {
            // One radius, whichever way a round section bends.
            bend_radius
        } else {
            match self.bend_radius_edge {
                Some(id) => Some(ctx.positive(id)?),
                None => self.item.bend_radius_edge,
            }
        };
        if section.is_round() && bend_radius.is_none() {
            return Err("a round section needs a bend radius".to_owned());
        }
        let fixed_length = self.fixed_length.map(|id| ctx.positive(id)).transpose()?;

        // The obstacles: the bodies of the feature's component at this
        // point of the timeline, the linked bodies its nodes are on, and
        // every other body of the design where its occurrences place it.
        let mut obstacles: Vec<(BodyUid, K::Shape)> = ctx.bodies();
        let placed: Vec<Placed> = self
            .nodes
            .iter()
            .map(|node| place_node(ctx, node, &mut obstacles))
            .collect();
        add_other_components(ctx, &self.links, &mut obstacles);

        let mut output = FlexOutput {
            section: section.clone(),
            nodes: self
                .nodes
                .iter()
                .zip(&placed)
                .map(|(node, at)| FlexNodeOutput {
                    id: node.id.clone(),
                    point: at.point,
                    normal: at.face.as_ref().map(|(_, _, normal)| *normal),
                    along: 0.0,
                    support: at.support,
                    lost: at.lost.is_some(),
                })
                .collect(),
            curve: Vec::new(),
            samples: Vec::new(),
            runs: Vec::new(),
            problems: Vec::new(),
            length: 0.0,
            fixed_length,
            bend_radius,
            bend_radius_edge,
        };
        // A node whose attachment is gone stays where it was, and warns.
        for (node, at) in self.nodes.iter().zip(&placed) {
            if let Some(kind) = at.lost {
                let places = vec![NoticePlace::point(at.point)];
                ctx.warn_at(kind.message(), places.clone());
                output.problems.push(FlexProblem {
                    kind,
                    from: 0.0,
                    to: 0.0,
                    node: Some(node.id.clone()),
                    body: None,
                    places,
                });
            }
        }
        let first = placed[0].point;
        if !placed
            .iter()
            .any(|at| mitcad_vec3::distance(at.point, first) > 1e-9)
        {
            ctx.flex = Some(output);
            return Err("the item's nodes are all at one point".to_owned());
        }

        let shapes: Vec<&K::Shape> = obstacles.iter().map(|(_, shape)| shape).collect();
        let nodes: Vec<RouteNode<'_>> = self
            .nodes
            .iter()
            .zip(&placed)
            .map(|(node, at)| RouteNode {
                point: at.point,
                face: at.face.as_ref().map(|(obstacle, face, normal)| RouteFace {
                    obstacle: *obstacle,
                    face,
                    normal: *normal,
                }),
                direction: at.direction,
                x_axis: at.x_axis,
                twist: node.twist,
            })
            .collect();
        let spec = FlexRouteSpec {
            feature: ctx.uid,
            obstacles: &shapes,
            nodes: &nodes,
            resting: section.resting(),
            half_width: section.half_width(),
            reach: section.reach(),
            bend_radius,
            bend_radius_edge,
            max_twist: if section.is_round() {
                None
            } else {
                self.item.max_twist
            },
            // The route alone under the pointer (mitcad#1713): quick.
            quick: self.route_only,
        };
        let mut route = match ctx.kernel.flex_route(&spec) {
            Ok(route) => route,
            Err(error) => {
                ctx.flex = Some(output);
                return Err(format!("no route could be made: {error}"));
            }
        };

        // A fixed length (mitcad#1713, Part B): shorter than the route, a
        // warning; longer, its slack hangs toward gravity in the stretches
        // through the air, through points of a hanging curve that the
        // route then passes, resting on what is below them.
        if let Some(fixed) = fixed_length {
            let length = route.length();
            let ends = |route: &FlexRoute| -> Vec<NoticePlace> {
                route
                    .samples
                    .first()
                    .into_iter()
                    .chain(route.samples.last())
                    .map(|s| NoticePlace::point(s.point))
                    .collect()
            };
            if fixed < length - LENGTH_TOLERANCE {
                let places = ends(&route);
                ctx.warn_at(FlexProblemKind::TooShort.message(), places.clone());
                output.problems.push(FlexProblem {
                    kind: FlexProblemKind::TooShort,
                    from: 0.0,
                    to: length,
                    node: None,
                    body: None,
                    places,
                });
            } else if fixed > length + LENGTH_TOLERANCE
                && let Some(down) = gravity_here(ctx)
            {
                // The route through the hanging points is a little shorter
                // than the curves they are on: asked again for what it
                // lacked, until it is long enough or no longer grows (the
                // slack resting on what is below).
                let straight = route.clone();
                let mut wanted = fixed;
                let mut got = length;
                for _ in 0..HANGING_TRIES {
                    let Some((hung, of)) = hang_slack(
                        ctx.kernel,
                        &straight,
                        &nodes,
                        &shapes,
                        down,
                        wanted,
                        section.reach(),
                        bend_radius,
                    ) else {
                        break;
                    };
                    let again = FlexRouteSpec {
                        nodes: &hung,
                        ..spec
                    };
                    let Ok(mut hanging) = ctx.kernel.flex_route(&again) else {
                        break;
                    };
                    // The samples' nodes are the item's again.
                    for sample in &mut hanging.samples {
                        sample.node = sample.node.and_then(|i| of.get(i).copied().flatten());
                    }
                    let longer = hanging.length();
                    route = hanging;
                    if fixed - longer <= LENGTH_TOLERANCE.max(fixed * 1e-3)
                        || longer <= got + LENGTH_TOLERANCE
                    {
                        break;
                    }
                    wanted += fixed - longer;
                    got = longer;
                }
            }
        }

        // What the route found: kept with the result also when the solid
        // then fails, so that the nodes can still be moved.
        let uid = ctx.uid;
        let path: Vec<PathCurve> = route
            .curve
            .iter()
            .enumerate()
            .map(|(i, curve)| PathCurve {
                name: format!("{uid}:route{}", i + 1),
                curve: curve.clone(),
            })
            .collect();
        for sample in &route.samples {
            if let Some(node) = sample.node.and_then(|i| output.nodes.get_mut(i)) {
                node.along = sample.along;
            }
        }
        output.length = route.length();
        output.runs = runs_of(&route.samples, &obstacles);
        for problem in &route.problems {
            let kind = FlexProblemKind::from(problem.kind);
            // A route that is not clean is a warning with its places, never
            // a failure.
            ctx.warn_at(kind.message(), problem.places.clone());
            output.problems.push(FlexProblem {
                kind,
                from: problem.from,
                to: problem.to,
                node: None,
                body: problem
                    .obstacle
                    .and_then(|i| obstacles.get(i))
                    .map(|(body, _)| *body),
                places: problem.places.clone(),
            });
        }
        output.curve.clone_from(&path);
        output.samples.clone_from(&route.samples);
        let length = output.length;
        ctx.flex = Some(output);
        if path.is_empty() || !(length > 0.0 && length.is_finite()) {
            return Err("the route has no length".to_owned());
        }

        // A preview of the route alone builds nothing.
        if self.route_only {
            return Ok(FeatureOutput::default());
        }
        let built = ctx.kernel.flex_solid(&FlexSolidSpec {
            feature: uid,
            path: &path,
            samples: &route.samples,
            section: &section,
            wall: self.item.wall,
        });
        let tool = built.map_err(|e| e.to_string())?;
        ctx.warn_notes(&tool);
        let body = ctx.new_body();
        Ok(FeatureOutput {
            changes: vec![BodyChange::Set(body, tool.clone())],
            tool: Some(tool),
            // The route's length into the item's reference parameter.
            measured: self.length.map(|id| (id, length)).into_iter().collect(),
            ..FeatureOutput::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hose() -> FlexType {
        serde_json::from_str(
            r#"{"name": "Hose 10", "section": {"type": "round", "diameter": 10},
                "wall": 1.5, "bend_radius": 30}"#,
        )
        .unwrap()
    }

    fn node(id: &str, at: Vec3) -> FlexNode {
        FlexNode {
            id: id.to_owned(),
            ..FlexNode::free(at)
        }
    }

    fn item() -> FlexDef<String> {
        FlexDef::new(
            hose(),
            vec![
                node("n1", [0.0, 0.0, 0.0]),
                node("n2", [50.0, 0.0, 0.0]),
                node("n3", [50.0, 40.0, 0.0]),
            ],
        )
    }

    fn ids(def: &FlexDef<String>) -> Vec<&str> {
        def.nodes.iter().map(|n| n.id.as_str()).collect()
    }

    #[test]
    fn the_definition_reads_and_writes_as_the_reference_gives_it() {
        let text = r#"{"type": "flex",
            "item": {"name": "Hose 10", "section": {"type": "round", "diameter": 10},
                     "wall": 1.5, "bend_radius": 30},
            "bend_radius": "d4",
            "nodes": [
              {"id": "n1", "at": [0, 0, 40], "port": {"body": "F2.b0", "face": "F2:end"}},
              {"id": "n2", "at": [30, 10, 20], "on": {"body": "F3.b0", "face": "F3:top"}},
              {"id": "n3", "at": [90, 40, 0], "twist": 1.5}]}"#;
        let def: FeatureDef<String> = serde_json::from_str(text).unwrap();
        let FeatureDef::Flex(flex) = &def else {
            panic!("a flex");
        };
        assert_eq!(flex.bend_radius.as_deref(), Some("d4"));
        assert_eq!(flex.nodes.len(), 3);
        assert!(matches!(flex.nodes[0].port, Some(GeomRef::Face { .. })));
        assert!(matches!(flex.nodes[1].on, Some(GeomRef::Face { .. })));
        assert_eq!(flex.nodes[2].twist, 1.5);
        assert!(flex.check_nodes().is_ok());
        let written = serde_json::to_value(&def).unwrap();
        assert_eq!(written["type"], "flex");
        // What is not set is left out.
        assert!(written.get("fixed_length").is_none(), "{written}");
        assert!(written.get("links").is_none(), "{written}");
        assert!(written["nodes"][0].get("twist").is_none(), "{written}");
        assert!(written["nodes"][2].get("on").is_none(), "{written}");
        assert_eq!(
            serde_json::from_value::<FeatureDef<String>>(written).unwrap(),
            def
        );
        // The bend radius is the definition's one value.
        let mut slots = Vec::new();
        def.map_params(&mut |slot, value: &String| {
            slots.push((slot.to_owned(), value.clone()));
            Ok::<_, ()>(())
        })
        .unwrap();
        assert_eq!(slots, [("bend_radius".to_owned(), "d4".to_owned())]);
    }

    #[test]
    fn nodes_are_added_at_the_ends_and_between() {
        let mut def = item();
        let end = def
            .add_node(FlexNode::free([90.0, 40.0, 0.0]), &NodePlace::End)
            .unwrap();
        assert_eq!(end, "n4");
        let start = def
            .add_node(FlexNode::free([-10.0, 0.0, 0.0]), &NodePlace::Start)
            .unwrap();
        assert_eq!(start, "n5");
        let between = def
            .add_node(
                FlexNode::free([25.0, 5.0, 0.0]),
                &NodePlace::After("n1".to_owned()),
            )
            .unwrap();
        assert_eq!(between, "n6");
        assert_eq!(ids(&def), ["n5", "n1", "n6", "n2", "n3", "n4"]);
        // After the last node is the end.
        def.add_node(
            FlexNode::free([120.0, 40.0, 0.0]),
            &NodePlace::After("n4".to_owned()),
        )
        .unwrap();
        assert_eq!(ids(&def).last(), Some(&"n7"));
        let error = def
            .add_node(
                FlexNode::free([0.0; 3]),
                &NodePlace::After("n40".to_owned()),
            )
            .unwrap_err();
        assert!(error.starts_with("the item has no such node"), "{error}");
        assert_eq!(def.nodes.len(), 7);
    }

    #[test]
    fn a_node_moves_twists_and_goes() {
        let mut def = item();
        def.move_node("n2", [55.0, 5.0, 2.0]).unwrap();
        assert_eq!(def.nodes[1].at, [55.0, 5.0, 2.0]);
        def.set_twist("n2", std::f64::consts::PI).unwrap();
        assert_eq!(def.nodes[1].twist, std::f64::consts::PI);
        assert!(def.move_node("n2", [f64::NAN, 0.0, 0.0]).is_err());
        assert!(def.set_twist("n9", 1.0).is_err());
        def.delete_node("n2").unwrap();
        assert_eq!(ids(&def), ["n1", "n3"]);
        // Two nodes are the least: the item itself is deleted instead.
        let error = def.delete_node("n1").unwrap_err();
        assert!(error.contains("at least two nodes"), "{error}");
        assert_eq!(ids(&def), ["n1", "n3"]);
        // A deleted node's id is not handed out again while a larger one
        // is in use.
        assert_eq!(def.next_node_id(), "n4");
    }

    #[test]
    fn a_node_is_attached_again_or_set_free() {
        let face: GeomRef = serde_json::from_str(r#"{"body": "F3.b0", "face": "F3:top"}"#).unwrap();
        let cap: GeomRef = serde_json::from_str(r#"{"body": "F2.b0", "face": "F2:end"}"#).unwrap();
        let mut def = item();
        def.attach_node("n2", NodeSupport::On(face.clone()), Some([50.0, 0.0, 10.0]))
            .unwrap();
        assert_eq!(def.nodes[1].on.as_ref(), Some(&face));
        assert_eq!(def.nodes[1].at, [50.0, 0.0, 10.0]);
        // A port only at an end: refused between, with nothing changed.
        let error = def
            .attach_node("n2", NodeSupport::Port(cap.clone()), None)
            .unwrap_err();
        assert!(error.contains("first or last node"), "{error}");
        assert_eq!(def.nodes[1].on.as_ref(), Some(&face));
        def.attach_node("n1", NodeSupport::Port(cap.clone()), None)
            .unwrap();
        assert_eq!(def.nodes[0].port.as_ref(), Some(&cap));
        // A node before a port's would leave the port between: refused.
        let error = def
            .add_node(FlexNode::free([-5.0, 0.0, 0.0]), &NodePlace::Start)
            .unwrap_err();
        assert!(error.contains("first or last node"), "{error}");
        assert_eq!(def.nodes.len(), 3);
        def.attach_node("n2", NodeSupport::Free, None).unwrap();
        assert_eq!((&def.nodes[1].on, &def.nodes[1].port), (&None, &None));
        assert_eq!(def.nodes[1].at, [50.0, 0.0, 10.0]);
    }

    #[test]
    fn nodes_that_cannot_be_are_refused() {
        let mut def = item();
        def.nodes.truncate(1);
        assert!(def.check_nodes().unwrap_err().contains("at least two"));
        let mut def = item();
        def.nodes[2].id = "n1".to_owned();
        let error = def.check_nodes().unwrap_err();
        assert!(error.starts_with("two nodes have the same id"), "{error}");
        let mut def = item();
        def.nodes[1].id = "node".to_owned();
        assert!(def.check_nodes().is_err());
        let face: GeomRef = serde_json::from_str(r#"{"body": "F3.b0", "face": "F3:top"}"#).unwrap();
        let mut def = item();
        def.nodes[0].on = Some(face.clone());
        def.nodes[0].port = Some(face);
        assert!(def.check_nodes().unwrap_err().contains("not both"));
    }

    #[test]
    fn warnings_say_what_is_wrong_in_plain_words() {
        for kind in [
            FlexProblemKind::PassesThrough,
            FlexProblemKind::BendTooTight,
            FlexProblemKind::TwistOverLimit,
            FlexProblemKind::NoCleanRoute,
            FlexProblemKind::FaceLost,
            FlexProblemKind::ReferenceLost,
            FlexProblemKind::PortLost,
            FlexProblemKind::EdgeBendTooTight,
        ] {
            let message = kind.message();
            assert_eq!(crate::notice::user_text_leaks(message), None, "{message}");
        }
    }

    #[test]
    fn runs_are_the_stretches_on_one_face() {
        let face = |role: &str| FaceName::new("F3".parse().unwrap(), role, None);
        let body = BodyUid::new("F3".parse().unwrap(), 0);
        let sample = |along: f64, on: Option<&str>| RouteSample {
            along,
            point: [along, 0.0, 0.0],
            tangent: [1.0, 0.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            node: None,
            on: on.map(|role| (0, face(role))),
        };
        let samples = [
            sample(0.0, None),
            sample(10.0, Some("top")),
            sample(20.0, Some("top")),
            sample(25.0, None),
            sample(30.0, Some("side1")),
            sample(40.0, Some("side1")),
            sample(50.0, Some("top")),
        ];
        let runs = runs_of(&samples, &[(body, ())]);
        let spans: Vec<(f64, f64, &str)> = runs
            .iter()
            .map(|run| (run.from, run.to, run.face.role.as_str()))
            .collect();
        assert_eq!(
            spans,
            [
                (10.0, 20.0, "top"),
                (30.0, 40.0, "side1"),
                (50.0, 50.0, "top")
            ]
        );
    }
}
