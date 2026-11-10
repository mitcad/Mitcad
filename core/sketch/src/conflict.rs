// SPDX-License-Identifier: MIT
//! Conflicting constraints in words (mitcad#1566, `docs/ui-conventions.md`
//! "Messages and severity"): a sketch that does not solve because its
//! constraints and dimensions contradict each other says so by what the
//! user sees, the constraint's type and the entities it is on, a
//! dimension's value, never by the ids (`k5`, `p15`), which the user sees
//! nowhere. Fixed entities are summarised as "projected geometry" (or
//! "fixed geometry" when none of them is projected), never listed one by
//! one.
//!
//! - [`sentence`]: a sketch whose solve fails, every user item of the
//!   conflict ("conflicting constraints: the Equal between two lines, the
//!   2.00 mm distance and 3 more can't all hold with projected geometry");
//!   the solve's error.
//! - [`blocked`]: an edit refused for a conflict, naming the constraints
//!   and dimensions that block it ("the Equal between two lines can't hold
//!   with the fillet"); `SketchEdit` finds them.
//! - [`trailer`] and [`split`]: the blocking items' ids for agents, the log
//!   and the application, on a last line of an edit's refusal
//!   (`[conflicts: k5, k7]`, `[conflicts: none]`), which the application
//!   takes off the message it shows and paints those items in the view.

use std::collections::BTreeSet;

use super::pattern::{Binding, BindingKind};
use super::solve::{DimensionInput, SolverItem};
use super::{Constraint, ConstraintKind, ConstraintUid, Entity, EntityKind, Ref};

/// What the words are made of: the sketch's entities, constraints, the
/// dimensions solved (with their values) and the bindings of its patterns
/// and offsets.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    pub entities: &'a [Entity],
    pub constraints: &'a [Constraint],
    pub dimensions: &'a [DimensionInput<'a>],
    pub bindings: &'a [Binding],
}

/// The start of the last line that lists an edit's blocking items.
pub const TRAILER: &str = "\n[conflicts: ";

/// How many items a message names before it counts the rest ("and 3
/// more").
const NAMED: usize = 3;

/// The ids of the blocking items as the last line of a refusal:
/// `\n[conflicts: k5, k7]`, `\n[conflicts: none]` when only fixed geometry
/// and the edit's own constraints conflict.
pub fn trailer(ids: &[ConstraintUid]) -> String {
    let listed: Vec<String> = ids.iter().map(ToString::to_string).collect();
    let listed = if listed.is_empty() {
        "none".to_owned()
    } else {
        listed.join(", ")
    };
    format!("{TRAILER}{listed}]")
}

/// A refusal's message without its [`trailer`], and the ids the trailer
/// lists (empty for `none`); None when the text has no trailer (no
/// conflict).
pub fn split(text: &str) -> (&str, Option<Vec<String>>) {
    let Some(at) = text.rfind(TRAILER) else {
        return (text, None);
    };
    let Some(listed) = text[at + TRAILER.len()..].strip_suffix(']') else {
        return (text, None);
    };
    let ids = if listed == "none" {
        Vec::new()
    } else {
        listed.split(", ").map(str::to_owned).collect()
    };
    (&text[..at], Some(ids))
}

/// The entity's noun, singular and plural: "a projected line", "projected
/// lines".
fn noun(e: Option<&Entity>) -> (String, String) {
    let Some(e) = e else {
        return ("an entity".to_owned(), "entities".to_owned());
    };
    let (one, many) = if e.axis.is_some() {
        ("axis", "axes")
    } else {
        match &e.kind {
            EntityKind::Point { .. } => ("point", "points"),
            EntityKind::Line { .. } => ("line", "lines"),
            EntityKind::Circle { .. } => ("circle", "circles"),
            EntityKind::Arc { .. } => ("arc", "arcs"),
            EntityKind::Ellipse { .. } => ("ellipse", "ellipses"),
            EntityKind::EllipticalArc { .. } => ("elliptical arc", "elliptical arcs"),
            EntityKind::Spline { .. } | EntityKind::FittedSpline { .. } => ("spline", "splines"),
        }
    };
    let (one, many) = if e.reference {
        (format!("projected {one}"), format!("projected {many}"))
    } else {
        (one.to_owned(), many.to_owned())
    };
    let article = if one.starts_with(|c: char| "aeiou".contains(c)) {
        "an"
    } else {
        "a"
    };
    (format!("{article} {one}"), many)
}

impl Context<'_> {
    fn entity(&self, r: Ref) -> Option<&Entity> {
        self.entities.iter().find(|e| e.id == r.uid())
    }

    /// The entities in words: "a line", "two lines", "a point and a
    /// circle", "4 points".
    fn things(&self, refs: &[Ref]) -> String {
        let nouns: Vec<(String, String)> = refs.iter().map(|r| noun(self.entity(*r))).collect();
        match nouns.as_slice() {
            [] => String::new(),
            [(one, _)] => one.clone(),
            [(a, many), (b, _)] if a == b => format!("two {many}"),
            [(a, _), (b, _)] => format!("{a} and {b}"),
            [(_, many), rest @ ..] if rest.iter().all(|(_, m)| m == many) => {
                format!("{} {many}", nouns.len())
            }
            _ => format!("{} entities", nouns.len()),
        }
    }

    /// A constraint in words: "the Equal between two lines", "the
    /// Horizontal on a line", "the Symmetry between two points about a
    /// line".
    fn constraint(&self, kind: &ConstraintKind) -> String {
        use ConstraintKind as K;
        let name = match kind {
            K::Coincident { .. } => "Coincident",
            K::Horizontal { .. } | K::HorizontalPoints { .. } => "Horizontal",
            K::Vertical { .. } | K::VerticalPoints { .. } => "Vertical",
            K::Parallel { .. } => "Parallel",
            K::Perpendicular { .. } => "Perpendicular",
            K::Collinear { .. } => "Collinear",
            K::Tangent { .. } => "Tangent",
            K::Smooth { .. } => "Curvature",
            K::Equal { .. } => "Equal",
            K::Concentric { .. } => "Concentric",
            K::Midpoint { .. } => "Midpoint",
            K::Symmetric { .. } => "Symmetry",
            K::EqualSpacing { .. } => "Equal Spacing",
        };
        let refs: Vec<Ref> = match kind {
            // The curves, not the point a tangent applies at.
            K::Tangent { a, b, .. } | K::Smooth { a, b, .. } => {
                if a == b {
                    vec![Ref::Curve(*a)]
                } else {
                    vec![Ref::Curve(*a), Ref::Curve(*b)]
                }
            }
            K::Symmetric { a, b, axis, .. } => {
                let about = noun(self.entity(Ref::Curve(*axis))).0;
                return format!(
                    "the {name} between {} about {about}",
                    self.things(&[*a, *b])
                );
            }
            K::EqualSpacing { anchors, .. } => anchors.clone(),
            other => other.refs(),
        };
        let things = self.things(&refs);
        match refs.len() {
            0 => format!("the {name}"),
            1 => format!("the {name} on {things}"),
            2 => format!("the {name} between {things}"),
            _ => format!("the {name} of {things}"),
        }
    }

    /// A dimension in words, with its value: "the 2.00 mm distance", "the
    /// 45.00° angle".
    fn dimension(&self, d: &DimensionInput<'_>) -> String {
        let what = match d.kind.type_name() {
            "horizontal_distance" => "horizontal distance",
            "vertical_distance" => "vertical distance",
            "length" => "length",
            "angle" => "angle",
            "radius" => "radius",
            "diameter" | "linear_diameter" => "diameter",
            "arc_length" => "arc length",
            "major_radius" => "major radius",
            "minor_radius" => "minor radius",
            "spacing" => "spacing",
            _ => "distance",
        };
        match d.value {
            Some(v) if what == "angle" => format!("the {:.2}° {what}", v.to_degrees()),
            Some(v) => format!("the {v:.2} mm {what}"),
            None => format!("the {what}"),
        }
    }

    /// A pattern's or an offset's own constraints, by its bindings.
    fn owner(&self, id: ConstraintUid) -> Option<String> {
        let mut bindings = self.bindings.iter().filter(|b| b.owner == id).peekable();
        bindings.peek()?;
        let pattern = bindings.any(|b| {
            matches!(
                b.kind,
                BindingKind::Translated { .. }
                    | BindingKind::Rotated { .. }
                    | BindingKind::TurnedDirection { .. }
            )
        });
        Some(if pattern { "the pattern" } else { "the offset" }.to_owned())
    }

    /// A constraint, dimension, pattern or offset in words, with the order
    /// it is named in: constraints between curves first, then dimensions,
    /// then coincidences and tangencies, then patterns and offsets. None
    /// for an id the sketch does not have.
    fn phrase(&self, id: ConstraintUid) -> Option<(u8, String)> {
        if let Some(c) = self.constraints.iter().find(|c| c.id == id) {
            let incidence = matches!(
                c.kind,
                ConstraintKind::Coincident { .. }
                    | ConstraintKind::Tangent { .. }
                    | ConstraintKind::Smooth { .. }
                    | ConstraintKind::Midpoint { .. }
            );
            return Some((if incidence { 2 } else { 0 }, self.constraint(&c.kind)));
        }
        if let Some(d) = self.dimensions.iter().find(|d| d.id == id) {
            return Some((1, self.dimension(d)));
        }
        self.owner(id).map(|o| (3, o))
    }

    /// The items in words, in the order they are named.
    fn phrases(&self, ids: &[ConstraintUid]) -> Vec<String> {
        let mut found: Vec<(u8, usize, String)> = ids
            .iter()
            .enumerate()
            .filter_map(|(i, id)| self.phrase(*id).map(|(rank, words)| (rank, i, words)))
            .collect();
        found.sort();
        found.into_iter().map(|(_, _, words)| words).collect()
    }
}

/// The phrases joined, the first [`NAMED`] of them and the count of the
/// rest: "a, b, c and 4 more".
fn listed(phrases: &[String]) -> String {
    let mut parts: Vec<String> = phrases.iter().take(NAMED).cloned().collect();
    if phrases.len() > NAMED {
        parts.push(format!("{} more", phrases.len() - NAMED));
    }
    match parts.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The constraints, dimensions, patterns and offsets of a conflict, each
/// once, in its order.
pub fn user_items(items: &[SolverItem]) -> Vec<ConstraintUid> {
    let mut seen = BTreeSet::new();
    items
        .iter()
        .filter_map(|item| match item {
            SolverItem::Constraint(id) if seen.insert(*id) => Some(*id),
            _ => None,
        })
        .collect()
}

/// The fixed entities of a conflict in a word: "projected geometry" when
/// one of them is projected, else "fixed geometry"; None without any.
pub fn fixed_words(ctx: &Context<'_>, items: &[SolverItem]) -> Option<&'static str> {
    let fixed: Vec<Option<&Entity>> = items
        .iter()
        .filter_map(|item| match item {
            SolverItem::Fixed(r) => Some(ctx.entity(*r)),
            SolverItem::Constraint(_) => None,
        })
        .collect();
    if fixed.is_empty() {
        None
    } else if fixed.iter().any(|e| e.is_some_and(|e| e.reference)) {
        Some("projected geometry")
    } else {
        Some("fixed geometry")
    }
}

/// A sketch whose constraints contradict each other, in words: every
/// constraint, dimension, pattern and offset of the conflict (the first
/// few named, the rest counted) and the fixed geometry they cannot hold
/// with.
pub fn sentence(ctx: &Context<'_>, items: &[SolverItem]) -> String {
    let phrases = ctx.phrases(&user_items(items));
    let fixed = fixed_words(ctx, items);
    let list = listed(&phrases);
    let verb = if phrases.len() == 1 {
        "can't hold"
    } else {
        "can't all hold"
    };
    match (phrases.is_empty(), fixed) {
        (true, Some(fixed)) => format!("conflicting constraints: the {fixed} contradicts itself"),
        (true, None) => "conflicting constraints".to_owned(),
        (false, Some(fixed)) => format!("conflicting constraints: {list} {verb} with {fixed}"),
        (false, None) => format!("conflicting constraints: {list} {verb}"),
    }
}

/// An edit refused for a conflict, in words: the constraints and
/// dimensions that block `change` (the first few named, the rest counted);
/// without any, the fixed geometry `change` cannot hold with.
pub fn blocked(
    ctx: &Context<'_>,
    blockers: &[ConstraintUid],
    change: &str,
    fixed: Option<&str>,
) -> String {
    let phrases = ctx.phrases(blockers);
    if phrases.is_empty() {
        return match fixed {
            Some(fixed) => format!("{change} can't hold with {fixed}"),
            None => format!("{change} can't hold"),
        };
    }
    format!("{} can't hold with {change}", listed(&phrases))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::EntityUid;

    fn line(id: u32, start: u32, end: u32) -> Entity {
        Entity::new(
            EntityUid(id),
            EntityKind::Line {
                start: EntityUid(start),
                end: EntityUid(end),
                centerline: false,
            },
        )
    }

    /// A solve that fails tells the conflict in words; its ids are the
    /// status' `conflicts`.
    #[test]
    fn a_failed_solve_says_what_cannot_hold_without_ids() {
        let mut a = Entity::point(EntityUid(1), [0.0, 0.0]);
        let mut b = Entity::point(EntityUid(2), [10.0, 0.0]);
        a.fixed = true;
        b.fixed = true;
        let entities = vec![a, b, line(3, 1, 2)];
        let constraints = vec![Constraint::new(
            ConstraintUid(1),
            ConstraintKind::Vertical { line: EntityUid(3) },
        )];
        let error = crate::solve::solve(&entities, &constraints, &[])
            .expect_err("a vertical line between two fixed points side by side");
        assert_eq!(
            error.message,
            "conflicting constraints: the Vertical on a line can't hold with fixed geometry"
        );
        let items: Vec<SolverItem> = error.status.conflicts.concat();
        assert!(
            items.contains(&SolverItem::Constraint(ConstraintUid(1))),
            "{items:?}"
        );
    }

    #[test]
    fn a_trailer_is_split_off_its_message() {
        let text = format!(
            "the Equal between two lines can't hold with the fillet{}",
            trailer(&[ConstraintUid(5), ConstraintUid(7)])
        );
        let (message, ids) = split(&text);
        assert_eq!(
            message,
            "the Equal between two lines can't hold with the fillet"
        );
        assert_eq!(ids, Some(vec!["k5".to_owned(), "k7".to_owned()]));
        assert_eq!(
            split(&format!("x{}", trailer(&[]))),
            ("x", Some(Vec::new()))
        );
        assert_eq!(split("no conflict"), ("no conflict", None));
    }

    #[test]
    fn constraints_and_dimensions_are_named_by_what_the_user_sees() {
        let mut projected = line(9, 7, 8);
        projected.reference = true;
        projected.fixed = true;
        let entities = vec![
            Entity::point(EntityUid(1), [0.0, 0.0]),
            Entity::point(EntityUid(2), [10.0, 0.0]),
            line(3, 1, 2),
            Entity::point(EntityUid(4), [0.0, 5.0]),
            Entity::point(EntityUid(5), [10.0, 5.0]),
            line(6, 4, 5),
            Entity::point(EntityUid(7), [0.0, -5.0]),
            Entity::point(EntityUid(8), [10.0, -5.0]),
            projected,
        ];
        let constraints = vec![
            Constraint::new(
                ConstraintUid(1),
                ConstraintKind::Equal {
                    a: EntityUid(3),
                    b: EntityUid(6),
                },
            ),
            Constraint::new(
                ConstraintUid(2),
                ConstraintKind::Horizontal { line: EntityUid(3) },
            ),
        ];
        let kind = super::super::DimensionKind::LineDistance {
            a: EntityUid(6),
            b: EntityUid(9),
        };
        let dimensions = [DimensionInput {
            id: ConstraintUid(3),
            kind: &kind,
            value: Some(2.0),
        }];
        let ctx = Context {
            entities: &entities,
            constraints: &constraints,
            dimensions: &dimensions,
            bindings: &[],
        };
        let items = [
            SolverItem::Fixed(Ref::Curve(EntityUid(9))),
            SolverItem::Constraint(ConstraintUid(3)),
            SolverItem::Constraint(ConstraintUid(1)),
            SolverItem::Constraint(ConstraintUid(2)),
        ];
        let text = sentence(&ctx, &items);
        assert_eq!(
            text,
            "conflicting constraints: the Equal between two lines, the Horizontal on a line \
             and the 2.00 mm distance can't all hold with projected geometry"
        );
        let refused = blocked(
            &ctx,
            &[ConstraintUid(1)],
            "the fillet",
            fixed_words(&ctx, &items),
        );
        assert_eq!(
            refused,
            "the Equal between two lines can't hold with the fillet"
        );
        assert_eq!(
            blocked(&ctx, &[], "the new dimension", Some("projected geometry")),
            "the new dimension can't hold with projected geometry"
        );
    }
}
