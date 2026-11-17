// SPDX-License-Identifier: MIT
//! Flex drape (mitcad#1713): a Flex item let hang. The kernel's inputs and
//! results for the chain of short segments its centre line becomes while it
//! hangs ([`DrapeStepSpec`], [`crate::Kernel::flex_drape_step`]: it falls
//! toward gravity, lies on what is below, wraps round curved faces and
//! edges and passes through no body, held where it is pinned and where the
//! pointer pulls it) and for the nodes that keep its shape when it is
//! locked ([`DrapeNodesSpec`], [`crate::Kernel::flex_drape_nodes`]).
//! Millimetres and seconds, the coordinates of the item's component. The
//! model keeps the chain between steps (`Document::flex_hang`);
//! `commands.md`, "Hanging cables", is the contract; the geometry is
//! `geometry/include/mitcad/geometry/flex_drape.hpp`.

use crate::datum::Vec3;
use crate::topo::FaceName;

/// What a point of the chain lies on.
#[derive(Debug, Clone, PartialEq)]
pub struct DrapeContact {
    /// The obstacle, by its index.
    pub obstacle: usize,
    /// The face's name there; None for a face without one.
    pub face: Option<FaceName>,
    /// The surface's unit outward normal.
    pub normal: Vec3,
}

/// Input of [`crate::Kernel::flex_drape_step`]: simulate the chain for
/// `seconds` (and what the step before left of a substep, `carry`) in substeps of
/// 1/360 s, the rest of a substep carried to the next step; it stops early
/// once it rests.
#[derive(Debug, Clone, Copy)]
pub struct DrapeStepSpec<'a, S> {
    /// The bodies the chain may not pass through, in the item's
    /// component's coordinates. They are left as they are.
    pub obstacles: &'a [&'a S],
    /// The chain's points in order (two or more) and their velocities
    /// (mm/s; empty: at rest).
    pub points: &'a [Vec3],
    pub velocities: &'a [Vec3],
    /// Each point: it stays where it is.
    pub pinned: &'a [bool],
    /// The length of each segment.
    pub segment: f64,
    /// How far the centre line keeps off a surface it lies on (the
    /// section's resting distance).
    pub resting: f64,
    /// The smallest radius the chain bends with, and how stiff it is (a
    /// thick hose bends more gently than a thin wire); None: limp.
    pub bend_radius: Option<f64>,
    /// The unit direction of gravity.
    pub gravity: Vec3,
    pub seconds: f64,
    /// What the step before left of a substep (its [`DrapeStep::carry`]),
    /// seconds.
    pub carry: f64,
    /// A point held by the pointer: its index and where it is pulled.
    pub grab: Option<(usize, Vec3)>,
    /// How long the chain has been still before this step, seconds.
    pub still: f64,
    /// How strongly the air holds the chain back, 0 to 1
    /// ([`DRAPE_DEFAULT_DAMPING`] unless the session was given another):
    /// 0 none (it swings on), 1 about twice the default.
    pub damping: f64,
}

/// Result of [`crate::Kernel::flex_drape_step`].
#[derive(Debug, Clone, PartialEq)]
pub struct DrapeStep {
    pub points: Vec<Vec3>,
    pub velocities: Vec<Vec3>,
    /// The section's `y` at each point: the surface's normal where it lies
    /// on one, turned evenly between (a flat section keeps its flat side
    /// against what it lies on).
    pub normals: Vec<Vec3>,
    /// What each point lies on, None in the air.
    pub contacts: Vec<Option<DrapeContact>>,
    /// How long the chain has now been still, seconds, and whether it
    /// rests (still for 0.3 s; its velocities then zero).
    pub still: f64,
    pub at_rest: bool,
    /// The seconds simulated.
    pub time: f64,
    /// What is left of a substep for the next step, seconds (0 once it
    /// rests).
    pub carry: f64,
}

/// An end of the chain that is a port: the cap's outward normal and the
/// section's `x` there (as a route's node has them).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrapePort {
    pub direction: Vec3,
    pub x_axis: Option<Vec3>,
}

/// Input of [`crate::Kernel::flex_drape_nodes`]: the chain where it rests
/// and the item as a route takes it.
#[derive(Debug, Clone, Copy)]
pub struct DrapeNodesSpec<'a, S> {
    pub obstacles: &'a [&'a S],
    pub points: &'a [Vec3],
    pub contacts: &'a [Option<DrapeContact>],
    /// The points that must be nodes (pinned nodes); the first and the
    /// last are in any case.
    pub keep: &'a [usize],
    /// The first and the last node's port, when they are on one.
    pub start_port: Option<DrapePort>,
    pub end_port: Option<DrapePort>,
    pub resting: f64,
    pub half_width: f64,
    pub reach: f64,
    pub bend_radius: Option<f64>,
    pub bend_radius_edge: Option<f64>,
    pub max_twist: Option<f64>,
    /// How far the route through the nodes may be from the chain.
    pub tolerance: f64,
}

/// Result of [`crate::Kernel::flex_drape_nodes`].
#[derive(Debug, Clone, PartialEq)]
pub struct DrapeNodes {
    /// The points that are nodes, by their index in the chain, in order.
    /// A node at a point that lies on a face is on that face.
    pub nodes: Vec<usize>,
    /// The farthest a point of the chain is from the route through them;
    /// None when no route could be made through them.
    pub deviation: Option<f64>,
}

/// The longest frame a step simulates at once, as the geometry has it (for
/// stand-ins of the kernel).
pub const DRAPE_FRAME: f64 = 1.0 / 30.0;

/// The damping of a hanging item when none is given (the geometry's
/// `kDrapeDefaultDamping`): a slight swing as it settles.
pub const DRAPE_DEFAULT_DAMPING: f64 = 0.5;

/// The length of each segment of an item's chain: twice its resting
/// distance (a round section's diameter), at most half its bend radius, and
/// no more than 400 segments over its length (the geometry's
/// `flex_drape_segment`).
pub fn drape_segment(length: f64, resting: f64, bend_radius: Option<f64>) -> f64 {
    let mut segment = 2.0 * resting;
    if let Some(radius) = bend_radius.filter(|r| *r > 0.0) {
        segment = segment.min(0.5 * radius);
    }
    if length > 0.0 {
        segment = segment.max(length / 400.0);
    }
    segment
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_are_a_fraction_of_the_section_and_the_bend_radius() {
        assert_eq!(drape_segment(300.0, 5.0, Some(30.0)), 10.0);
        assert_eq!(drape_segment(300.0, 5.0, Some(12.0)), 6.0);
        assert_eq!(drape_segment(300.0, 0.5, None), 1.0);
        // No more than 400 of them.
        assert_eq!(drape_segment(4000.0, 0.5, Some(5.0)), 10.0);
    }
}
