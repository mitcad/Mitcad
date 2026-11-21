// SPDX-License-Identifier: MIT
//! Blend Curve (mitcad#2107): a spline that joins the ends of two curves
//! with tangent (G1) or curvature (G2) continuity and stays attached to
//! them. Its end points are the curves' end points (a projected end, which
//! the model moves itself, holds a new end with a point-to-point
//! coincident), and a Tangent or a Smooth constraint at each end keeps the
//! continuity when the curves change.
//!
//! The spline is a single Bézier span: degree 3 for Tangent (the inner
//! control points on the end tangents), degree 5 for Curvature (the second
//! and third control points at each end also set the end's curvature). Its
//! legs are a fraction of the distance between the ends, which keeps the
//! shape between fair: no loop, no cusp back over a curve.

use serde::{Deserialize, Serialize};

use super::edit::SketchEdit;
use super::geometry::{Curve2, P2, add, cross, dist, norm, perp, scale, unit};
use super::{ConstraintKind, ConstraintUid, EntityKind, Ref};
use crate::ids::EntityUid;

/// The continuity a blend holds at both ends.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Continuity {
    /// G1: the blend leaves each curve in its direction (degree 3).
    #[default]
    Tangent,
    /// G2: the direction and the curvature as well (degree 5).
    Curvature,
}

/// A curve end a blend starts from: where it is, the curve's direction
/// outward from the end (the way a smooth continuation leaves it), and the
/// curve's signed curvature when it is run along that direction (positive
/// when it turns left).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlendEnd {
    pub at: P2,
    pub dir: P2,
    pub curvature: f64,
}

/// What a blend made: the spline and its constraints (a Tangent or Smooth
/// at each end, and a coincident for an end held at a projected point).
#[derive(Debug, Clone, PartialEq)]
pub struct Blend {
    pub spline: EntityUid,
    pub constraints: Vec<ConstraintUid>,
}

/// The end of an open solved curve at its start (`at_end` false) or end:
/// see [`BlendEnd`]. None when the curve has no direction there.
pub fn curve_end(curve: &Curve2, at_end: bool) -> Option<BlendEnd> {
    let (lo, hi) = curve.domain();
    let [p, d1, d2] = curve.eval(if at_end { hi } else { lo });
    let speed = norm(d1);
    if speed.is_nan() || speed <= 1e-12 {
        return None;
    }
    let k = cross(d1, d2) / speed.powi(3);
    Some(if at_end {
        BlendEnd {
            at: p,
            dir: unit(d1),
            curvature: k,
        }
    } else {
        // Run the other way: the direction turns round and the curvature's
        // sign with it.
        BlendEnd {
            at: p,
            dir: scale(unit(d1), -1.0),
            curvature: -k,
        }
    })
}

/// The control points of the blend from `a` to `b`. The blend leaves `a`
/// along `a.dir` and arrives at `b` against `b.dir` (running on into the
/// second curve). For Curvature its curvature at `a` is `a.curvature` and
/// at `b` the second curve's run inward, `-b.curvature`: for a Bézier span
/// of degree 5 with the leg `s` along the tangent, the second control point
/// at `2 s` along it and `h` to its left, the curvature at the end is
/// `0.8 h / s²`.
pub fn blend_control(a: &BlendEnd, b: &BlendEnd, continuity: Continuity) -> Vec<P2> {
    let length = dist(a.at, b.at);
    match continuity {
        Continuity::Tangent => {
            let s = length / 3.0;
            vec![
                a.at,
                add(a.at, scale(a.dir, s)),
                add(b.at, scale(b.dir, s)),
                b.at,
            ]
        }
        Continuity::Curvature => {
            let s = length / 5.0;
            // At the start: the left of the way the blend runs, `a.dir`.
            let ha = 1.25 * a.curvature * s * s;
            // At the end the blend runs along -b.dir: its left is
            // perp(-b.dir), and the curvature asked for is -b.curvature.
            let inward = scale(b.dir, -1.0);
            let hb = 1.25 * -b.curvature * s * s;
            vec![
                a.at,
                add(a.at, scale(a.dir, s)),
                add(add(a.at, scale(a.dir, 2.0 * s)), scale(perp(a.dir), ha)),
                add(add(b.at, scale(b.dir, 2.0 * s)), scale(perp(inward), hb)),
                add(b.at, scale(b.dir, s)),
                b.at,
            ]
        }
    }
}

impl SketchEdit<'_> {
    /// Blend Curve (mitcad#2107): a spline from the end of `curve1`
    /// nearest `at1` to the end of `curve2` nearest `at2`, tangent or
    /// curvature-continuous at both, attached by its end points (the
    /// curves' own) and a Tangent or Smooth constraint at each end. Lines,
    /// arcs, elliptical arcs and open splines; both ends may be on one open
    /// curve (a gap). Refused in words for a curve without ends (a circle,
    /// an ellipse, a closed spline), for a curve a derived offset computes
    /// (it takes no constraints), for one end picked twice and for ends
    /// that already meet.
    pub fn blend_curve(
        &mut self,
        curve1: EntityUid,
        at1: P2,
        curve2: EntityUid,
        at2: P2,
        continuity: Continuity,
    ) -> Result<Blend, String> {
        let solved = self.solved()?;
        let (end1, point1, geometry1) = self.blend_end(curve1, at1, &solved)?;
        let (end2, point2, geometry2) = self.blend_end(curve2, at2, &solved)?;
        if curve1 == curve2 && point1 == point2 {
            return Err(crate::said(
                "both picks are at the same end of the curve: pick near the two ends to join",
                format!("{} is picked twice at the same end", Ref::Curve(curve1)),
            ));
        }
        let size = (norm(geometry1) + norm(geometry2)).max(1.0);
        if point1 == point2 || dist(end1.at, end2.at) <= 1e-9 * size {
            return Err(crate::said(
                "the ends already meet: Blend Curve joins ends with a gap between them",
                format!(
                    "the ends of {} and {} already meet",
                    Ref::Curve(curve1),
                    Ref::Curve(curve2)
                ),
            ));
        }
        let control = blend_control(&end1, &end2, continuity);
        let last = control.len() - 1;
        let mut constraints = Vec::new();
        let mut ends = [point1, point2];
        for end in &mut ends {
            if !self.mergeable_point(*end) {
                // A projected end (or a pattern's) is held, not moved: the
                // blend ends at a point of its own on it.
                let at = solved.points.get(end).copied().unwrap_or_default();
                let own = self.add_point(at);
                constraints.push(self.add_constraint(ConstraintKind::Coincident {
                    point: own,
                    entity: Ref::Point(*end),
                }));
                *end = own;
            }
        }
        let mut points = Vec::with_capacity(control.len());
        for (i, c) in control.iter().enumerate() {
            points.push(match i {
                0 => ends[0],
                i if i == last => ends[1],
                _ => self.add_point(*c),
            });
        }
        let degree = match continuity {
            Continuity::Tangent => 3,
            Continuity::Curvature => 5,
        };
        let spline = self.add_curve(EntityKind::Spline {
            degree,
            control: points,
            weights: Vec::new(),
            knots: Vec::new(),
        });
        for (curve, at) in [(curve1, ends[0]), (curve2, ends[1])] {
            let kind = match continuity {
                Continuity::Tangent => ConstraintKind::Tangent {
                    a: spline,
                    b: curve,
                    at: Some(at),
                },
                Continuity::Curvature => ConstraintKind::Smooth {
                    a: spline,
                    b: curve,
                    at: Some(at),
                },
            };
            constraints.push(self.add_constraint(kind));
        }
        Ok(Blend {
            spline,
            constraints,
        })
    }

    /// The end of `curve` nearest `at` that a blend starts from, its point
    /// and the curve's extent (for a tolerance).
    fn blend_end(
        &self,
        curve: EntityUid,
        at: P2,
        solved: &super::solve::Solved,
    ) -> Result<(BlendEnd, EntityUid, P2), String> {
        let kind = &self.curve(curve)?.kind;
        let named = Ref::Curve(curve);
        let Some((start, end)) = kind.ends() else {
            let what = match kind {
                EntityKind::Circle { .. } => "a circle",
                EntityKind::Ellipse { .. } => "an ellipse",
                _ => "a closed spline",
            };
            return Err(crate::said(
                format!("{what} has no end to blend from: pick an open curve"),
                format!("{named} is {what} and has no end"),
            ));
        };
        if self.computing_offset(curve).is_some() {
            return Err(crate::said(
                "an offset computes this curve and it takes no constraints: free the offset first",
                format!("{named} is computed by a derived offset"),
            ));
        }
        let geometry = solved
            .curves
            .get(&curve)
            .ok_or_else(|| format!("curve {named} has no solved geometry"))?;
        let (first, second) = geometry.ends().ok_or_else(|| {
            crate::said(
                "a closed curve has no end to blend from: pick an open curve",
                format!("{named} is closed and has no end"),
            )
        })?;
        let at_end = dist(at, second) < dist(at, first);
        let blend_end = curve_end(geometry, at_end).ok_or_else(|| {
            crate::said(
                "the curve has no direction at its end to blend from",
                format!("{named} has no direction at its end"),
            )
        })?;
        let (lo, hi) = geometry.bounds();
        let extent = [hi[0] - lo[0], hi[1] - lo[1]];
        Ok((blend_end, if at_end { end } else { start }, extent))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Entity;
    use crate::def::SketchDef;
    use crate::geometry::{dot, sub};
    use crate::ids::FeatureUid;
    use crate::parameters::Parameters;

    /// Builds a definition: points from 100 on (shared at one position),
    /// curves from 1 on.
    #[derive(Default)]
    struct Def {
        entities: Vec<Entity>,
        curves: u32,
        points: u32,
    }

    impl Def {
        fn point(&mut self, at: P2) -> EntityUid {
            if let Some(e) = self
                .entities
                .iter()
                .find(|e| matches!(e.kind, EntityKind::Point { at: p } if p == at))
            {
                return e.id;
            }
            self.points += 1;
            let id = EntityUid(100 + self.points);
            self.entities.push(Entity::point(id, at));
            id
        }

        fn curve(&mut self, kind: EntityKind) -> EntityUid {
            self.curves += 1;
            let id = EntityUid(self.curves);
            self.entities.push(Entity::new(id, kind));
            id
        }

        fn line(&mut self, a: P2, b: P2) -> EntityUid {
            let (start, end) = (self.point(a), self.point(b));
            self.curve(EntityKind::Line {
                start,
                end,
                centerline: false,
            })
        }

        fn arc(&mut self, center: P2, start: P2, end: P2) -> EntityUid {
            let (center, start, end) = (self.point(center), self.point(start), self.point(end));
            self.curve(EntityKind::Arc { center, start, end })
        }

        fn def(self) -> SketchDef {
            SketchDef {
                entities: self.entities,
                ..SketchDef::default()
            }
        }
    }

    /// Blends and finishes the edit; the solved blend and the solved
    /// curves.
    fn blended(
        def: SketchDef,
        picks: (EntityUid, P2, EntityUid, P2),
        continuity: Continuity,
    ) -> Result<(Blend, super::super::solve::Solved), String> {
        let mut params = Parameters::default();
        let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
        let blend = edit.blend_curve(picks.0, picks.1, picks.2, picks.3, continuity)?;
        let solved = edit.solved()?;
        Ok((blend, solved))
    }

    fn sine(u: P2, v: P2) -> f64 {
        cross(u, v) / (norm(u) * norm(v))
    }

    /// The blend's ends against the curves': the same point, the blend
    /// running on in the curve's outward direction at its start and into
    /// the second curve at its end, within 1e-6.
    fn check_tangent(
        solved: &super::super::solve::Solved,
        blend: EntityUid,
        a: BlendEnd,
        b: BlendEnd,
    ) {
        let curve = &solved.curves[&blend];
        let start = curve_end(curve, false).unwrap();
        let end = curve_end(curve, true).unwrap();
        assert!(dist(start.at, a.at) < 1e-6, "{start:?} {a:?}");
        assert!(dist(end.at, b.at) < 1e-6, "{end:?} {b:?}");
        // The blend run from its start outward leaves backward into the
        // first curve: its start direction is the opposite of start.dir.
        let leaving = scale(start.dir, -1.0);
        assert!(sine(leaving, a.dir).abs() < 1e-6, "{leaving:?} {a:?}");
        assert!(
            dot(leaving, a.dir) > 0.0,
            "no cusp back over the first curve"
        );
        let arriving = end.dir;
        assert!(sine(arriving, b.dir).abs() < 1e-6, "{arriving:?} {b:?}");
        assert!(
            dot(arriving, b.dir) < 0.0,
            "no cusp back over the second curve"
        );
    }

    #[test]
    fn a_tangent_blend_joins_two_lines() {
        let mut d = Def::default();
        let l1 = d.line([0.0, 0.0], [10.0, 0.0]);
        let l2 = d.line([20.0, 10.0], [20.0, 20.0]);
        let (blend, solved) = blended(
            d.def(),
            (l1, [9.0, 0.0], l2, [20.0, 11.0]),
            Continuity::Tangent,
        )
        .unwrap();
        assert_eq!(blend.constraints.len(), 2);
        let a = curve_end(&solved.curves[&l1], true).unwrap();
        let b = curve_end(&solved.curves[&l2], false).unwrap();
        check_tangent(&solved, blend.spline, a, b);
        assert!(solved.status.conflicts.is_empty());
    }

    #[test]
    fn a_tangent_blend_joins_a_line_and_an_arc() {
        let mut d = Def::default();
        let line = d.line([0.0, 0.0], [10.0, 0.0]);
        // A quarter arc about (30, 10) from (30, 0) to (40, 10).
        let arc = d.arc([30.0, 10.0], [30.0, 0.0], [40.0, 10.0]);
        let (blend, solved) = blended(
            d.def(),
            (line, [9.0, 0.0], arc, [30.5, 0.2]),
            Continuity::Tangent,
        )
        .unwrap();
        let a = curve_end(&solved.curves[&line], true).unwrap();
        let b = curve_end(&solved.curves[&arc], false).unwrap();
        check_tangent(&solved, blend.spline, a, b);
        let spline = solved.curves[&blend.spline].clone();
        let Curve2::Nurbs(n) = spline else {
            panic!("a spline")
        };
        assert_eq!(n.degree, 3);
    }

    #[test]
    fn a_curvature_blend_between_two_arcs_matches_their_curvatures() {
        let mut d = Def::default();
        // An arc of radius 10 about (0, 0) from (10, 0) to (0, 10), left
        // at its end heading -x, and one of radius 5 about (-30, 10) from
        // (-25, 10) to (-30, 15), joined at its start heading +y.
        let a1 = d.arc([0.0, 0.0], [10.0, 0.0], [0.0, 10.0]);
        let a2 = d.arc([-30.0, 10.0], [-25.0, 10.0], [-30.0, 15.0]);
        let (blend, solved) = blended(
            d.def(),
            (a1, [1.0, 9.9], a2, [-25.1, 10.5]),
            Continuity::Curvature,
        )
        .unwrap();
        let a = curve_end(&solved.curves[&a1], true).unwrap();
        let b = curve_end(&solved.curves[&a2], false).unwrap();
        check_tangent(&solved, blend.spline, a, b);
        let curve = &solved.curves[&blend.spline];
        let start = curve_end(curve, false).unwrap();
        let end = curve_end(curve, true).unwrap();
        // The blend run forward from its start: the first arc's curvature
        // run outward; at its end: the second's run inward.
        assert!(
            (-start.curvature - a.curvature).abs() < 1e-6,
            "{start:?} {a:?}"
        );
        assert!((end.curvature + b.curvature).abs() < 1e-6, "{end:?} {b:?}");
        assert!((a.curvature.abs() - 0.1).abs() < 1e-9);
        assert!((b.curvature.abs() - 0.2).abs() < 1e-9);
        let Curve2::Nurbs(n) = curve else {
            panic!("a spline")
        };
        assert_eq!(n.degree, 5);
        assert!(solved.status.conflicts.is_empty());
    }

    /// An elliptical arc about (30, 0), semi-axes 10 along x and 5, from
    /// (40, 0) counter-clockwise to (30, 5).
    fn elliptical_arc(d: &mut Def) -> EntityUid {
        let (center, major) = (d.point([30.0, 0.0]), d.point([40.0, 0.0]));
        // The start is a point of its own beside the major axis' end.
        d.points += 1;
        let start = EntityUid(100 + d.points);
        d.entities.push(Entity::point(start, [40.0, 0.0]));
        let end = d.point([30.0, 5.0]);
        d.curve(EntityKind::EllipticalArc {
            center,
            major,
            minor_radius: 5.0,
            start,
            end,
        })
    }

    #[test]
    fn a_tangent_blend_joins_a_line_and_an_elliptical_arc() {
        let mut d = Def::default();
        let line = d.line([0.0, 0.0], [10.0, 0.0]);
        let arc = elliptical_arc(&mut d);
        // The arc's end (30, 5), where it runs on along -x.
        let (blend, solved) = blended(
            d.def(),
            (line, [9.0, 0.0], arc, [30.5, 5.0]),
            Continuity::Tangent,
        )
        .unwrap();
        let a = curve_end(&solved.curves[&line], true).unwrap();
        let b = curve_end(&solved.curves[&arc], true).unwrap();
        check_tangent(&solved, blend.spline, a, b);
        assert!(solved.status.conflicts.is_empty());
    }

    #[test]
    fn a_curvature_blend_matches_an_elliptical_arcs_curvature() {
        let mut d = Def::default();
        let a1 = d.arc([0.0, 0.0], [10.0, 0.0], [0.0, 10.0]);
        let arc = elliptical_arc(&mut d);
        // From the circular arc's end (0, 10) to the elliptical arc's
        // start (40, 0), where its curvature is |A| b / |E'|^3 = 10 * 5 / 5^3.
        let (blend, solved) = blended(
            d.def(),
            (a1, [1.0, 9.9], arc, [40.0, 0.5]),
            Continuity::Curvature,
        )
        .unwrap();
        let a = curve_end(&solved.curves[&a1], true).unwrap();
        let b = curve_end(&solved.curves[&arc], false).unwrap();
        check_tangent(&solved, blend.spline, a, b);
        assert!(
            (b.curvature.abs() - 10.0 * 5.0 / 125.0).abs() < 1e-9,
            "{b:?}"
        );
        let curve = &solved.curves[&blend.spline];
        let start = curve_end(curve, false).unwrap();
        let end = curve_end(curve, true).unwrap();
        assert!(
            (-start.curvature - a.curvature).abs() < 1e-6,
            "{start:?} {a:?}"
        );
        assert!((end.curvature + b.curvature).abs() < 1e-6, "{end:?} {b:?}");
        assert!(solved.status.conflicts.is_empty());
    }

    #[test]
    fn the_blend_ends_are_the_curves_end_points() {
        let mut d = Def::default();
        let l1 = d.line([0.0, 0.0], [10.0, 0.0]);
        let l2 = d.line([20.0, 10.0], [20.0, 20.0]);
        let (p1, p2) = (d.point([10.0, 0.0]), d.point([20.0, 10.0]));
        let mut params = Parameters::default();
        let mut edit = SketchEdit::new(d.def(), &mut params, FeatureUid(1), "Sketch1");
        let blend = edit
            .blend_curve(l1, [9.0, 0.0], l2, [20.0, 11.0], Continuity::Tangent)
            .unwrap();
        let EntityKind::Spline { control, .. } = &edit.def.entity(blend.spline).unwrap().kind
        else {
            panic!("a spline")
        };
        assert_eq!(control.first(), Some(&p1));
        assert_eq!(control.last(), Some(&p2));
        let kinds: Vec<_> = blend
            .constraints
            .iter()
            .map(|id| {
                edit.def
                    .constraints
                    .iter()
                    .find(|c| c.id == *id)
                    .unwrap()
                    .kind
                    .clone()
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                ConstraintKind::Tangent {
                    a: blend.spline,
                    b: l1,
                    at: Some(p1)
                },
                ConstraintKind::Tangent {
                    a: blend.spline,
                    b: l2,
                    at: Some(p2)
                },
            ]
        );
        edit.finish().unwrap();
    }

    #[test]
    fn a_blend_to_a_projected_curve_holds_its_end() {
        let mut d = Def::default();
        let line = d.line([0.0, 0.0], [10.0, 0.0]);
        let projected = d.line([20.0, 10.0], [20.0, 20.0]);
        for e in &mut d.entities {
            let ours = e.id == projected
                || matches!(e.kind, EntityKind::Point { at } if at == [20.0, 10.0] || at == [20.0, 20.0]);
            if ours {
                e.fixed = true;
                e.reference = true;
            }
        }
        let held = d.point([20.0, 10.0]);
        let mut params = Parameters::default();
        let mut edit = SketchEdit::new(d.def(), &mut params, FeatureUid(1), "Sketch1");
        let blend = edit
            .blend_curve(
                line,
                [9.0, 0.0],
                projected,
                [20.0, 11.0],
                Continuity::Tangent,
            )
            .unwrap();
        assert_eq!(blend.constraints.len(), 3, "a coincident and two tangents");
        let coincident = edit
            .def
            .constraints
            .iter()
            .find(|c| c.id == blend.constraints[0])
            .unwrap();
        assert!(matches!(
            coincident.kind,
            ConstraintKind::Coincident { entity: Ref::Point(p), .. } if p == held
        ));
        let solved = edit.solved().unwrap();
        assert_eq!(
            solved.points[&held],
            [20.0, 10.0],
            "the projected end stays"
        );
        let a = curve_end(&solved.curves[&line], true).unwrap();
        let b = curve_end(&solved.curves[&projected], false).unwrap();
        check_tangent(&solved, blend.spline, a, b);
    }

    #[test]
    fn a_blend_closes_the_gap_of_one_open_spline() {
        let mut d = Def::default();
        // A U of control points, open at the top: the blend closes it.
        let control: Vec<EntityUid> = [[0.0, 20.0], [0.0, 0.0], [20.0, 0.0], [20.0, 20.0]]
            .iter()
            .map(|p| d.point(*p))
            .collect();
        let spline = d.curve(EntityKind::Spline {
            degree: 3,
            control,
            weights: Vec::new(),
            knots: Vec::new(),
        });
        let (blend, solved) = blended(
            d.def(),
            (spline, [0.0, 19.0], spline, [20.0, 19.0]),
            Continuity::Curvature,
        )
        .unwrap();
        let a = curve_end(&solved.curves[&spline], false).unwrap();
        let b = curve_end(&solved.curves[&spline], true).unwrap();
        check_tangent(&solved, blend.spline, a, b);
        // It rises from both ends: the blend bulges up, it does not cross
        // back down into the U.
        let top = solved.curves[&blend.spline].point(0.5);
        assert!(top[1] > 20.0, "{top:?}");
    }

    #[test]
    fn blends_are_refused_in_words() {
        let mut d = Def::default();
        let line = d.line([0.0, 0.0], [10.0, 0.0]);
        let joined = d.line([10.0, 0.0], [10.0, 10.0]);
        let center = d.point([40.0, 0.0]);
        let circle = d.curve(EntityKind::Circle {
            center,
            radius: 5.0,
            start: 0.0,
        });
        let def = d.def();
        let refusal = |picks| blended(def.clone(), picks, Continuity::Tangent).unwrap_err();
        let said = |text: String| mitcad_model_base::notice::FeatureError::from_text(&text).message;
        assert_eq!(
            said(refusal((circle, [45.0, 0.0], line, [0.0, 0.0]))),
            "a circle has no end to blend from: pick an open curve"
        );
        assert_eq!(
            said(refusal((line, [1.0, 0.0], line, [2.0, 0.0]))),
            "both picks are at the same end of the curve: pick near the two ends to join"
        );
        assert_eq!(
            said(refusal((line, [9.0, 0.0], joined, [10.0, 1.0]))),
            "the ends already meet: Blend Curve joins ends with a gap between them"
        );
        // A refusal names no ids for a person; its detail does for agents.
        let text = refusal((circle, [45.0, 0.0], line, [0.0, 0.0]));
        assert!(!said(text.clone()).contains("c3"));
        assert!(text.contains("c3"), "{text}");
    }

    #[test]
    fn the_control_points_give_the_end_curvatures_asked_for() {
        // Not through the solver: the formula alone.
        let a = BlendEnd {
            at: [0.0, 0.0],
            dir: [1.0, 0.0],
            curvature: 0.05,
        };
        let b = BlendEnd {
            at: [30.0, 10.0],
            dir: unit([0.0, 1.0]),
            curvature: -0.2,
        };
        let control = blend_control(&a, &b, Continuity::Curvature);
        let n = crate::geometry::Nurbs {
            degree: 5,
            control,
            weights: Vec::new(),
            knots: crate::geometry::clamped_uniform_knots(6, 5),
        };
        let curve = Curve2::Nurbs(n);
        let start = curve_end(&curve, false).unwrap();
        let end = curve_end(&curve, true).unwrap();
        assert!((-start.curvature - a.curvature).abs() < 1e-9, "{start:?}");
        assert!((end.curvature + b.curvature).abs() < 1e-9, "{end:?}");
        assert!(norm(sub(start.at, a.at)) < 1e-12);
    }
}
