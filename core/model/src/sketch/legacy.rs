// SPDX-License-Identifier: MIT
//! The dimension-driven rectangles and circles of V0 (version 1 project
//! files) and of the transitional sketch of version 2 files (`"shapes"`),
//! as entities, constraints and dimensions.
//!
//! A rectangle is four lines `c<k>`..`c<k+3>` (line i from corner i to
//! corner i + 1, corners counter-clockwise from the anchor) sharing their
//! corner points, horizontal and vertical constraints, a length dimension on
//! the first line (the width) and on the second (the height), and the
//! anchor corner fixed. A circle is a circle with its centre fixed and a
//! diameter dimension. Curves keep their ids, so segment and region keys
//! are those the shapes had (`c1[c4,c2]`, `r{c5}`); points get the ids
//! after the largest curve id.

use serde::Deserialize;

use super::solve::Solved;
use super::{
    Constraint, ConstraintKind, ConstraintUid, Dimension, DimensionKind, Entity, EntityKind,
};
use crate::features::sketch::SketchDef;
use crate::ids::{EntityUid, curve_serde};
use crate::parameters::Parameters;
use crate::topo::{RegionKey, SegmentKey};

/// A shape of a version 1 or an early version 2 file.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LegacyShape<P> {
    Rectangle {
        corner: [f64; 2],
        width: P,
        height: P,
        #[serde(with = "four_curves")]
        curves: [EntityUid; 4],
    },
    Circle {
        center: [f64; 2],
        diameter: P,
        #[serde(with = "curve_serde")]
        curve: EntityUid,
    },
}

impl<P> LegacyShape<P> {
    pub fn curves(&self) -> Vec<EntityUid> {
        match self {
            Self::Rectangle { curves, .. } => curves.to_vec(),
            Self::Circle { curve, .. } => vec![*curve],
        }
    }

    pub fn anchor(&self) -> [f64; 2] {
        match self {
            Self::Rectangle { corner, .. } => *corner,
            Self::Circle { center, .. } => *center,
        }
    }

    /// The segments of the shape's boundary on its own, in order.
    pub fn segments(&self) -> Vec<SegmentKey> {
        match self {
            Self::Rectangle { curves: c, .. } => (0..4)
                .map(|i| SegmentKey::between(c[i], c[(i + 3) % 4], c[(i + 1) % 4]))
                .collect(),
            Self::Circle { curve, .. } => vec![SegmentKey::closed(*curve)],
        }
    }

    /// The key of the shape's region on its own.
    pub fn region(&self) -> RegionKey {
        RegionKey::new(self.segments()).expect("a shape has segments")
    }

    /// Whether the points lie in the shape (on its boundary counts), with
    /// the dimension values `size`.
    pub fn contains(&self, points: &[[f64; 2]], size: &dyn Fn(&P) -> Option<f64>) -> bool {
        let tol = 1e-6;
        match self {
            Self::Rectangle {
                corner,
                width,
                height,
                ..
            } => {
                let (Some(w), Some(h)) = (size(width), size(height)) else {
                    return false;
                };
                points.iter().all(|p| {
                    p[0] >= corner[0] - tol
                        && p[0] <= corner[0] + w + tol
                        && p[1] >= corner[1] - tol
                        && p[1] <= corner[1] + h + tol
                })
            }
            Self::Circle {
                center, diameter, ..
            } => {
                let Some(d) = size(diameter) else {
                    return false;
                };
                points
                    .iter()
                    .all(|p| (p[0] - center[0]).hypot(p[1] - center[1]) <= d / 2.0 + tol)
            }
        }
    }
}

/// Solves a sketch with the parameters' values and stores the solved
/// positions (converted shapes start from approximate geometry).
pub(crate) fn settle(def: &mut SketchDef, params: &Parameters) -> Result<Solved, String> {
    let solved = def
        .solve_with(
            &mut |id| {
                params
                    .value(id)
                    .ok_or_else(|| format!("parameter {} does not exist", params.name(id)))
            },
            &|id| params.name(id),
        )
        .map_err(|e| e.message)?;
    solved.store(&mut def.entities);
    Ok(solved)
}

/// Entities, constraints and dimensions of converted shapes.
#[derive(Debug, Clone, PartialEq)]
pub struct Converted<P> {
    pub entities: Vec<Entity>,
    pub constraints: Vec<Constraint>,
    pub dimensions: Vec<Dimension<P>>,
}

/// Converts shapes. `size` gives a dimension's value where it is known
/// (the start geometry; the solver makes the sizes exact anyway).
pub fn convert<P>(shapes: Vec<LegacyShape<P>>, size: &dyn Fn(&P) -> Option<f64>) -> Converted<P> {
    let mut next_point = shapes
        .iter()
        .flat_map(LegacyShape::curves)
        .map(|c| c.0)
        .max()
        .unwrap_or(0)
        + 1;
    let mut next_constraint = 1;
    let mut out = Converted {
        entities: Vec::new(),
        constraints: Vec::new(),
        dimensions: Vec::new(),
    };
    let mut point = |out: &mut Converted<P>, at: [f64; 2], fixed: bool| {
        let id = EntityUid(next_point);
        next_point += 1;
        let mut entity = Entity::point(id, at);
        entity.fixed = fixed;
        out.entities.push(entity);
        id
    };
    let mut constraint_id = || {
        next_constraint += 1;
        ConstraintUid(next_constraint - 1)
    };
    let positive = |v: Option<f64>| v.filter(|v| v.is_finite() && *v > 0.0).unwrap_or(10.0);
    for shape in shapes {
        match shape {
            LegacyShape::Rectangle {
                corner,
                width,
                height,
                curves,
            } => {
                let (w, h) = (positive(size(&width)), positive(size(&height)));
                let [x, y] = corner;
                let corners = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
                // Curves first, so their ids stay the shape's.
                let lines_at = out.entities.len();
                let points: Vec<EntityUid> = corners
                    .iter()
                    .enumerate()
                    .map(|(i, c)| point(&mut out, *c, i == 0))
                    .collect();
                for i in 0..4 {
                    out.entities.insert(
                        lines_at + i,
                        Entity::new(
                            curves[i],
                            EntityKind::Line {
                                start: points[i],
                                end: points[(i + 1) % 4],
                                centerline: false,
                            },
                        ),
                    );
                }
                for (i, line) in curves.iter().enumerate() {
                    let kind = if i.is_multiple_of(2) {
                        ConstraintKind::Horizontal { line: *line }
                    } else {
                        ConstraintKind::Vertical { line: *line }
                    };
                    out.constraints.push(Constraint {
                        id: constraint_id(),
                        kind,
                    });
                }
                for (line, value) in [(curves[0], width), (curves[1], height)] {
                    out.dimensions.push(Dimension {
                        id: constraint_id(),
                        kind: DimensionKind::Length { line },
                        value: Some(value),
                        text: None,
                    });
                }
            }
            LegacyShape::Circle {
                center,
                diameter,
                curve,
            } => {
                let d = positive(size(&diameter));
                let circle_at = out.entities.len();
                let c = point(&mut out, center, true);
                out.entities.insert(
                    circle_at,
                    Entity::new(
                        curve,
                        EntityKind::Circle {
                            center: c,
                            radius: d / 2.0,
                        },
                    ),
                );
                out.dimensions.push(Dimension {
                    id: constraint_id(),
                    kind: DimensionKind::Diameter { curve },
                    value: Some(diameter),
                    text: None,
                });
            }
        }
    }
    out
}

/// Four curve ids as `["c1", "c2", "c3", "c4"]`.
mod four_curves {
    use serde::Deserialize;

    use crate::ids::EntityUid;

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<[EntityUid; 4], D::Error> {
        let names = <[String; 4]>::deserialize(deserializer)?;
        let mut curves = [EntityUid(0); 4];
        for (curve, name) in curves.iter_mut().zip(&names) {
            *curve = EntityUid::parse_curve(name).map_err(serde::de::Error::custom)?;
        }
        Ok(curves)
    }
}
