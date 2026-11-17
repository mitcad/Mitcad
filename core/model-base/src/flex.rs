// SPDX-License-Identifier: MIT
//! Flex (mitcad#1713): flexible items (wires, cables, hoses, flat cables,
//! strips, belts) laid through the model. Here are the plain data of an
//! item ([`FlexType`], [`FlexSection`]: what a design stores and a type
//! library keeps) and the kernel's inputs and results for it: the route
//! between the item's nodes ([`FlexRouteSpec`], [`FlexRoute`],
//! [`crate::Kernel::flex_route`]), the solid of a section that is not
//! round along the route ([`FlexSolidSpec`], [`crate::Kernel::flex_solid`])
//! and the end ports of pipes and Flex items ([`EndPort`],
//! [`crate::Kernel::end_port`]). Millimetres and radians, the coordinates
//! of the feature's component. The feature is
//! `mitcad_model::features::flex`; `commands.md`, "Flex", is the contract.
//!
//! **Section coordinates.** A section is drawn in two coordinates: the
//! origin on the route, `x` the section's width (along the surface the
//! item rests on), `y` its height, away from that surface. The route's
//! direction is `x × y`. Along the route the section's `y` direction is
//! each sample's [`RouteSample::normal`].

use serde::{Deserialize, Serialize};

use crate::datum::Vec3;
use crate::ids::FeatureUid;
use crate::kernel::Curve3;
use crate::notice::NoticePlace;
use crate::profile::SegmentGeometry;
use crate::sweeps::PathCurve;
use crate::topo::FaceName;

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

/// The section of a Flex item, in section coordinates (see the module).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FlexSection {
    /// A circle about the route.
    Round { diameter: f64 },
    /// A rectangle centred on the route, `width` along `x` and `height`
    /// along `y`, its corners rounded by `corner_radius` (0: sharp).
    Rectangular {
        width: f64,
        height: f64,
        #[serde(default, skip_serializing_if = "is_zero")]
        corner_radius: f64,
    },
    /// Closed loops of curves where they are drawn: the first the
    /// outside, the others holes. Each loop's segments join end to end.
    Outline { loops: Vec<Vec<SegmentGeometry>> },
}

impl FlexSection {
    pub fn is_round(&self) -> bool {
        matches!(self, Self::Round { .. })
    }

    /// The section's box in section coordinates (minimum, maximum); an
    /// outline's from its sampled curves.
    pub fn bounds(&self) -> ([f64; 2], [f64; 2]) {
        match self {
            Self::Round { diameter } => {
                let r = diameter / 2.0;
                ([-r, -r], [r, r])
            }
            Self::Rectangular { width, height, .. } => {
                ([-width / 2.0, -height / 2.0], [width / 2.0, height / 2.0])
            }
            Self::Outline { loops } => {
                let mut min = [f64::INFINITY; 2];
                let mut max = [f64::NEG_INFINITY; 2];
                for segment in loops.iter().flatten() {
                    for p in segment.sample() {
                        for i in 0..2 {
                            min[i] = min[i].min(p[i]);
                            max[i] = max[i].max(p[i]);
                        }
                    }
                }
                if min[0] > max[0] {
                    return ([0.0; 2], [0.0; 2]);
                }
                (min, max)
            }
        }
    }

    /// How far the route is held from the surface the item rests on: the
    /// distance from the route to the section's resting side (its least
    /// `y`). A round section's radius, half a rectangle's height.
    pub fn resting(&self) -> f64 {
        (-self.bounds().0[1]).max(0.0)
    }

    /// How far the section reaches sideways from the route (the larger of
    /// its reaches along `x` and against it).
    pub fn half_width(&self) -> f64 {
        let (min, max) = self.bounds();
        max[0].max(-min[0]).max(0.0)
    }

    /// How far the section reaches from the route any way: the radius of
    /// the circle about the route that holds its box.
    pub fn reach(&self) -> f64 {
        match self {
            Self::Round { diameter } => diameter / 2.0,
            _ => {
                let (min, max) = self.bounds();
                let x = max[0].max(-min[0]);
                let y = max[1].max(-min[1]);
                x.hypot(y)
            }
        }
    }

    /// Why the section's sizes cannot be built, with the `wall` that
    /// makes it hollow.
    pub fn check(&self, wall: Option<f64>) -> Result<(), String> {
        let positive = |what: &str, value: f64| {
            if value > 0.0 && value.is_finite() {
                Ok(())
            } else {
                Err(format!("{what} must be greater than zero, got {value}"))
            }
        };
        let room = match self {
            Self::Round { diameter } => {
                positive("the section's diameter", *diameter)?;
                diameter / 2.0
            }
            Self::Rectangular {
                width,
                height,
                corner_radius,
            } => {
                positive("the section's width", *width)?;
                positive("the section's height", *height)?;
                let least = width.min(*height) / 2.0;
                if !(*corner_radius >= 0.0 && corner_radius.is_finite()) {
                    return Err(format!(
                        "the section's corner radius must not be negative, got {corner_radius}"
                    ));
                }
                if *corner_radius > least {
                    return Err(format!(
                        "the section's corner radius {corner_radius} is more than half its \
                         smaller side; it must be at most {least}"
                    ));
                }
                least
            }
            Self::Outline { loops } => {
                if loops.first().is_none_or(Vec::is_empty) {
                    return Err("the section's outline has no curves".to_owned());
                }
                if wall.is_some() {
                    return Err(
                        "an outline is hollow by its own inner loops; it takes no wall".to_owned(),
                    );
                }
                return Ok(());
            }
        };
        if let Some(wall) = wall {
            positive("the wall", wall)?;
            if wall >= room {
                return Err(format!(
                    "the wall {wall} fills the section; it must be less than {room}"
                ));
            }
        }
        Ok(())
    }
}

/// Where an item's copy of a type came from: the library (`built_in`,
/// `user` or `project`), the type's id there and its version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlexTypeOrigin {
    pub library: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
}

/// A Flex item, independent of any route: what a type library keeps and
/// what a Flex feature stores whole as its `item` (`commands.md`, "Flex").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlexType {
    pub name: String,
    pub section: FlexSection,
    /// Makes a round or a rectangular section hollow, the wall inside it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall: Option<f64>,
    /// The smallest radius of the route's centre line: a round section's
    /// one radius; of another section, bending over its flat side (about
    /// the section's `x`). None: it does not bend that way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bend_radius: Option<f64>,
    /// Of a section that is not round, bending over its edge (about the
    /// section's `y`). None: it does not bend that way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bend_radius_edge: Option<f64>,
    /// The most the section may turn about the route, radians per
    /// millimetre; None: no limit. Only of a section that is not round.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_twist: Option<f64>,
    /// The appearance the item's body gets: an id of the design's or its
    /// libraries' appearances.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<String>,
    /// The material the item's body gets, likewise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    /// Grams per metre of length.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mass_per_length: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<FlexTypeOrigin>,
}

impl FlexType {
    /// Why the item cannot be built: its section's sizes, its wall, its
    /// bend radii and its limits.
    pub fn check(&self) -> Result<(), String> {
        self.section.check(self.wall)?;
        let positive = |what: &str, value: Option<f64>| match value {
            Some(v) if !(v > 0.0 && v.is_finite()) => {
                Err(format!("{what} must be greater than zero, got {v}"))
            }
            _ => Ok(()),
        };
        positive("the bend radius", self.bend_radius)?;
        positive("the bend radius over the edge", self.bend_radius_edge)?;
        positive("the twist limit", self.max_twist)?;
        positive("the mass per length", self.mass_per_length)?;
        if self.section.is_round() && self.bend_radius.is_none() {
            return Err("a round section needs a bend radius".to_owned());
        }
        Ok(())
    }
}

/// The face a node of a route lies on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RouteFace<'a> {
    /// The body, by its index in [`FlexRouteSpec::obstacles`].
    pub obstacle: usize,
    /// The face's name in that body.
    pub face: &'a FaceName,
    /// The face's unit outward normal at the node.
    pub normal: Vec3,
}

/// A node of a route: a point the route passes through.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RouteNode<'a> {
    pub point: Vec3,
    /// The face it lies on; None for a node in space (also one on an edge
    /// or at a point of a body: only `point` then).
    pub face: Option<RouteFace<'a>>,
    /// A port's direction (a unit vector): the cap's outward normal. The
    /// route leaves the first node along it and reaches the last node
    /// against it.
    pub direction: Option<Vec3>,
    /// A port's section orientation: the section's `x` here (a unit
    /// vector, square to `direction`).
    pub x_axis: Option<Vec3>,
    /// Radians the section is turned about the route here, right hand
    /// about the route's direction, beyond what the route gives it.
    pub twist: f64,
}

/// Input of [`crate::Kernel::flex_route`]: the route of a Flex item
/// through its nodes in order, the shortest one that passes through no
/// obstacle and stays within the bend radii.
///
/// Between neighbouring nodes that both lie on faces of obstacles that
/// touch (or of one obstacle) the route runs along the surfaces, held
/// `resting` away from them, crossing from face to face; over a sharp
/// edge it rounds with the bend radius and leaves the surface there. Any
/// other stretch is a smooth curve around the obstacles in its way. The
/// route's own earlier run is an obstacle too; touching is allowed.
///
/// The section's `y` direction ([`RouteSample::normal`]) is the surface's
/// outward normal where the route lies on a face; through the air it goes
/// from the orientation at one end of the stretch to that at the other
/// with the least twist; each node's `twist` is then added from that node
/// on; at a node with an `x_axis` the orientation is that.
///
/// The result is never an error because no clean route exists: the best
/// route found is given with its [`RouteProblem`]s. Errors are for input
/// that gives no route at all (fewer than two distinct points).
#[derive(Debug, Clone, Copy)]
pub struct FlexRouteSpec<'a, S> {
    pub feature: FeatureUid,
    /// The bodies the route may not pass through, in the coordinates of
    /// the feature's component. They are left as they are.
    pub obstacles: &'a [&'a S],
    /// Two or more, in order from the item's start to its end.
    pub nodes: &'a [RouteNode<'a>],
    /// How far the route is held from the surface it lies on
    /// ([`FlexSection::resting`]).
    pub resting: f64,
    /// How far the section reaches sideways ([`FlexSection::half_width`]).
    pub half_width: f64,
    /// How far the section reaches any way ([`FlexSection::reach`]): the
    /// clearance of a stretch through the air.
    pub reach: f64,
    /// The smallest radius of bending about the section's `x` (the one
    /// radius of a round section); None: it does not bend that way.
    pub bend_radius: Option<f64>,
    /// The smallest radius of bending about the section's `y`; for a
    /// round section the same as `bend_radius`.
    pub bend_radius_edge: Option<f64>,
    /// The most the section may turn about the route, radians per
    /// millimetre; None: no limit.
    pub max_twist: Option<f64>,
    /// A quicker, rougher route, for the route alone under the pointer
    /// (a preview with `route_only`): the finest steps are left out.
    pub quick: bool,
}

/// A point of a route.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteSample {
    /// Its length along the route from the start.
    pub along: f64,
    pub point: Vec3,
    /// The route's unit direction here.
    pub tangent: Vec3,
    /// The section's unit `y` direction here, square to the tangent: the
    /// surface's outward normal where the route lies on a face, with the
    /// nodes' twists.
    pub normal: Vec3,
    /// The node here, by its index in [`FlexRouteSpec::nodes`].
    pub node: Option<usize>,
    /// The face the route lies on here: the obstacle's index and the
    /// face's name.
    pub on: Option<(usize, FaceName)>,
}

/// What is wrong with a stretch of a route.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteProblemKind {
    /// The item passes through an obstacle (or through its own run).
    PassesThrough,
    /// The route bends with a smaller radius than allowed.
    BendTooTight,
    /// The section turns more per length than allowed.
    TwistOverLimit,
    /// No route passes through nothing within the bend radius: the route
    /// given is the nearest found.
    NoCleanRoute,
    /// A section that is not round bends over its edge (about its `y`)
    /// with a smaller radius than `bend_radius_edge` allows, or at all
    /// without one; [`Self::BendTooTight`] is then its bends about `x`.
    EdgeBendTooTight,
}

/// A stretch of a route with something wrong.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteProblem {
    pub kind: RouteProblemKind,
    /// The stretch, lengths along the route.
    pub from: f64,
    pub to: f64,
    /// The obstacle passed through, by its index.
    pub obstacle: Option<usize>,
    /// Where it is, to paint: points and boxes in the feature's
    /// coordinates (no names of the route's own body: it is not built yet).
    pub places: Vec<NoticePlace>,
}

/// Result of [`crate::Kernel::flex_route`].
#[derive(Debug, Clone, PartialEq)]
pub struct FlexRoute {
    /// The route's centre line: curves end to end in order, each running
    /// along it, from the first node to the last; tangent to each other
    /// where they meet.
    pub curve: Vec<Curve3>,
    /// Points along it in order, the first at the first node (`along` 0)
    /// and the last at the last node (`along` the route's length), one at
    /// every node (`node`), and between them close enough that the
    /// polyline through them stays within a hundredth of the section's
    /// reach of the curve.
    pub samples: Vec<RouteSample>,
    pub problems: Vec<RouteProblem>,
}

impl FlexRoute {
    /// The route's length: the last sample's.
    pub fn length(&self) -> f64 {
        self.samples.last().map_or(0.0, |s| s.along)
    }
}

/// Input of [`crate::Kernel::flex_solid`]: the solid of a section swept
/// along a route. At every sample the section's origin
/// is the sample's point, its `y` the sample's `normal` and its `x`
/// `normal × tangent`; between samples it turns evenly.
///
/// Faces: caps `start` and `end`. Of a round section `side0` (and
/// `inner0` of a hollow one). Of a rectangle `side0` (the resting
/// side, at `-y`), `side1` (at `+x`), `side2`, `side3`, counter-clockwise
/// about the route's direction, and `corner0` to `corner3` after each
/// when the corners are rounded; `inner<i>` likewise for a hollow one. Of
/// an outline `side<i>` in the order of the outer loop's segments and
/// `inner<i>` in the order of the inner loops' segments.
#[derive(Debug, Clone, Copy)]
pub struct FlexSolidSpec<'a> {
    pub feature: FeatureUid,
    /// The route's centre line ([`FlexRoute::curve`]), named.
    pub path: &'a [PathCurve],
    /// The route's samples ([`FlexRoute::samples`]).
    pub samples: &'a [RouteSample],
    /// A circle, a rectangle or an outline.
    pub section: &'a FlexSection,
    /// Makes the section hollow, the wall inside it.
    pub wall: Option<f64>,
}

/// An end of a pipe or of a Flex item where another item begins or ends
/// ([`crate::Kernel::end_port`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EndPort {
    /// The middle of the cap's outer boundary.
    pub position: Vec3,
    /// The cap's unit outward normal: an item that begins here leaves
    /// along it.
    pub direction: Vec3,
    /// The section's `x` at the cap, a unit vector square to `direction`:
    /// the direction of the cap's width (of its longest straight edge;
    /// for a round cap any such direction, the same on every evaluation).
    pub x_axis: Vec3,
    /// The cap's extent along `x_axis` and across it.
    pub size: [f64; 2],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_reach_from_the_route() {
        let round = FlexSection::Round { diameter: 10.0 };
        assert_eq!(round.resting(), 5.0);
        assert_eq!(round.half_width(), 5.0);
        assert_eq!(round.reach(), 5.0);
        let flat = FlexSection::Rectangular {
            width: 12.0,
            height: 2.0,
            corner_radius: 0.0,
        };
        assert_eq!(flat.resting(), 1.0);
        assert_eq!(flat.half_width(), 6.0);
        assert!((flat.reach() - 37.0_f64.sqrt()).abs() < 1e-12);
        // An outline rests on its lowest point where it is drawn.
        let strip = FlexSection::Outline {
            loops: vec![vec![
                SegmentGeometry::Line {
                    start: [-4.0, -1.0],
                    end: [4.0, -1.0],
                },
                SegmentGeometry::Line {
                    start: [4.0, -1.0],
                    end: [0.0, 3.0],
                },
                SegmentGeometry::Line {
                    start: [0.0, 3.0],
                    end: [-4.0, -1.0],
                },
            ]],
        };
        assert_eq!(strip.resting(), 1.0);
        assert_eq!(strip.half_width(), 4.0);
        assert_eq!(strip.reach(), 5.0);
    }

    #[test]
    fn sizes_that_cannot_be_built_are_refused() {
        let round = FlexSection::Round { diameter: 10.0 };
        assert!(round.check(Some(1.5)).is_ok());
        let error = round.check(Some(5.0)).unwrap_err();
        assert!(error.contains("fills the section"), "{error}");
        let error = FlexSection::Round { diameter: 0.0 }
            .check(None)
            .unwrap_err();
        assert!(error.contains("greater than zero"), "{error}");
        let flat = FlexSection::Rectangular {
            width: 12.0,
            height: 2.0,
            corner_radius: 1.5,
        };
        let error = flat.check(None).unwrap_err();
        assert!(error.contains("corner radius"), "{error}");
        let outline = FlexSection::Outline { loops: Vec::new() };
        assert!(outline.check(None).is_err());
    }

    #[test]
    fn a_type_reads_and_writes_as_the_reference_gives_it() {
        let text = r#"{"name": "Flat cable 12", "section": {"type": "rectangular", "width": 12, "height": 2},
                       "bend_radius": 6, "max_twist": 0.05, "mass_per_length": 40,
                       "origin": {"library": "built_in", "id": "flat-cable-12"}}"#;
        let item: FlexType = serde_json::from_str(text).unwrap();
        assert_eq!(item.bend_radius, Some(6.0));
        assert_eq!(item.bend_radius_edge, None);
        assert!(item.check().is_ok());
        let written = serde_json::to_value(&item).unwrap();
        assert!(written.get("bend_radius_edge").is_none(), "{written}");
        assert!(
            written["section"].get("corner_radius").is_none(),
            "{written}"
        );
        assert_eq!(serde_json::from_value::<FlexType>(written).unwrap(), item);
        // A round section must say how tightly it bends.
        let round: FlexType = serde_json::from_str(
            r#"{"name": "Wire", "section": {"type": "round", "diameter": 2}}"#,
        )
        .unwrap();
        let error = round.check().unwrap_err();
        assert!(error.contains("needs a bend radius"), "{error}");
        // An unknown field is refused.
        assert!(
            serde_json::from_str::<FlexType>(
                r#"{"name": "Wire", "section": {"type": "round", "diameter": 2}, "radius": 3}"#
            )
            .is_err()
        );
    }
}
