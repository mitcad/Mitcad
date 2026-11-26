// SPDX-License-Identifier: MIT
//! A linked projection's elliptical arcs that followed their source
//! (mitcad#2408, GitHub #622: a cone's section and a slot's round end on a
//! face whose draft turned about another plane): the sketch is solved as
//! built where it was stored and then moved to where its projections are
//! now, and the arcs' end parameters follow their moved ends. Before, they
//! stayed where the arcs were stored, the arcs' fixes held them there, and
//! the sketch failed: "the projected geometry contradicts itself".

use super::*;
use crate::def::SketchDef;
use crate::geometry::{P2, dist};
use crate::parameters::ParamId;
use crate::project::solve_followed;

/// An ellipse: its centre, the vector from it to its major point and its
/// minor radius.
struct Ellipse {
    center: P2,
    major: P2,
    minor: f64,
}

impl Ellipse {
    /// Its point at parameter `t`, from the major point towards the major
    /// axis turned a quarter turn counter-clockwise.
    fn at(&self, t: f64) -> P2 {
        let big = self.major[0].hypot(self.major[1]);
        let u = [self.major[0] / big, self.major[1] / big];
        let v = [-u[1], u[0]];
        let (a, b) = (big * t.cos(), self.minor * t.sin());
        [
            self.center[0] + a * u[0] + b * v[0],
            self.center[1] + a * u[1] + b * v[1],
        ]
    }
}

/// A fixed, projected entity.
fn projected(id: u32, kind: EntityKind) -> Entity {
    let mut entity = Entity::new(EntityUid(id), kind);
    entity.fixed = true;
    entity.reference = true;
    entity
}

/// A sketch of one projected elliptical arc of `e` from parameter `from`
/// to `to` and a projected line from its end 30 along y, as a face's
/// outline has them: the arc's centre p1, major point p2, ends p3 and p4,
/// the line's end p5, the arc c6 and the line c7.
fn outline(e: &Ellipse, from: f64, to: f64) -> SketchDef {
    let end = e.at(to);
    let point = |id: u32, at: P2| projected(id, EntityKind::Point { at });
    SketchDef {
        entities: vec![
            point(1, e.center),
            point(2, [e.center[0] + e.major[0], e.center[1] + e.major[1]]),
            point(3, e.at(from)),
            point(4, end),
            point(5, [end[0], end[1] + 30.0]),
            projected(
                6,
                EntityKind::EllipticalArc {
                    center: EntityUid(1),
                    major: EntityUid(2),
                    minor_radius: e.minor,
                    start: EntityUid(3),
                    end: EntityUid(4),
                },
            ),
            projected(
                7,
                EntityKind::Line {
                    start: EntityUid(4),
                    end: EntityUid(5),
                    centerline: false,
                },
            ),
        ],
        ..SketchDef::default()
    }
}

/// `followed` solved as it followed from `stored`: every point stays where
/// the follow put it.
fn solves_where_it_is(stored: &SketchDef, followed: &SketchDef) {
    let mut value = |_: ParamId| -> Result<f64, String> { Err("no parameters".to_owned()) };
    let solved = solve_followed(stored, followed, &mut value, &|_| String::new())
        .unwrap_or_else(|e| panic!("{}", e.message));
    for entity in &followed.entities {
        if let EntityKind::Point { at } = entity.kind {
            let now = solved.points[&entity.id];
            assert!(
                dist(now, at) < 1e-7,
                "p{}: {now:?} stays at {at:?}",
                entity.id.0
            );
        }
    }
}

#[test]
fn projected_elliptical_arcs_that_followed_their_source_solve_where_they_are() {
    // The cone's section as stored: semi-axes 46.58 along y and 46.38
    // about (0, -0.61); followed: 46.09 and 45.91 about (0, -0.48), its
    // ends moved along it.
    let stored = Ellipse {
        center: [0.0, -0.6118],
        major: [0.0, 46.5846],
        minor: 46.3838,
    };
    let followed = Ellipse {
        center: [0.0, -0.4822],
        major: [0.0, 46.0947],
        minor: 45.9059,
    };
    solves_where_it_is(&outline(&stored, 0.4, 2.0), &outline(&followed, 0.42, 2.03));
}

#[test]
fn a_projected_half_ellipse_whose_major_point_turned_over_solves_where_it_is() {
    // The slot's round end: half an ellipse between the ends of its minor
    // axis (semi-axes 20.08 along y and 20), its major point given at the
    // other end of the major axis after the follow.
    let half = std::f64::consts::PI;
    let stored = Ellipse {
        center: [0.0, 0.8716],
        major: [0.0, 20.0763],
        minor: 20.0,
    };
    let followed = Ellipse {
        center: [0.0, 0.9859],
        major: [0.0, -20.0763],
        minor: 20.0,
    };
    // The same ends, (20, y) and (-20, y): from a quarter turn on the
    // turned-over ellipse.
    solves_where_it_is(
        &outline(&stored, -0.5 * half, 0.5 * half),
        &outline(&followed, 0.5 * half, 1.5 * half),
    );
}
