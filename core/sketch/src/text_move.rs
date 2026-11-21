// SPDX-License-Identifier: MIT
//! Texts moved and copied with Move/Copy (`sketch.move`'s `texts`,
//! mitcad#1911, GitHub mitcad/Mitcad#369): a text without a frame or a path
//! moves its anchor (`at`) and turns its `angle` with a rotation; a text in
//! a frame follows its frame's rectangle, which moves with the entities; a
//! text on a path follows its curve and is refused. A copy copies the text
//! (with its frame's rectangle) and the text parameter it follows.

use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::{PI, TAU};

use super::edit::SketchEdit;
use super::geometry::P2;
use super::modify::Map2;
use super::{Entity, EntityKind, Ref, SketchText};
use crate::ids::EntityUid;
use crate::values::ValueInput;

/// An angle in (-π, π].
fn wrapped(angle: f64) -> f64 {
    let mut a = angle;
    while a > PI {
        a -= TAU;
    }
    while a <= -PI {
        a += TAU;
    }
    a
}

/// A line's ends.
fn line_ends(e: &Entity) -> Option<(EntityUid, EntityUid)> {
    match e.kind {
        EntityKind::Line { start, end, .. } => Some((start, end)),
        _ => None,
    }
}

impl SketchEdit<'_> {
    fn text_of(&self, id: EntityUid) -> Result<&SketchText, String> {
        self.def
            .texts
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| format!("text t{} does not exist", id.0))
    }

    /// The entities that carry texts in a Move/Copy (mitcad#1911): the
    /// rectangle of each text in a frame (its corner points and the lines
    /// between them), which the text follows; nothing for a text without a
    /// frame. A text on a path is refused: it follows its curve.
    pub fn text_carriers(&self, texts: &[EntityUid]) -> Result<Vec<Ref>, String> {
        let mut carriers = Vec::new();
        for &id in texts {
            let text = self.text_of(id)?;
            if let Some(path) = &text.path {
                return Err(crate::said(
                    "a text on a path follows its curve: move the curve instead",
                    format!(
                        "t{} runs along {}: move the curve instead",
                        id.0,
                        Ref::Curve(path.curve)
                    ),
                ));
            }
            for r in self.frame_rectangle(&text.frame) {
                if !carriers.contains(&r) {
                    carriers.push(r);
                }
            }
        }
        Ok(carriers)
    }

    /// A text frame's rectangle: its three corner points, the fourth corner
    /// where a line from the x corner and one from the y corner meet, and
    /// the lines between those corners. A frame of three points without
    /// lines is those points alone.
    pub(crate) fn frame_rectangle(&self, frame: &[EntityUid]) -> Vec<Ref> {
        let mut corners: BTreeSet<EntityUid> = frame.iter().copied().collect();
        let neighbours = |p: EntityUid| -> BTreeSet<EntityUid> {
            self.def
                .entities
                .iter()
                .filter_map(line_ends)
                .filter_map(|(a, b)| {
                    if a == p {
                        Some(b)
                    } else if b == p {
                        Some(a)
                    } else {
                        None
                    }
                })
                .collect()
        };
        if let [_, x, y] = frame {
            let fourth: Vec<EntityUid> = neighbours(*x)
                .intersection(&neighbours(*y))
                .copied()
                .filter(|q| !corners.contains(q))
                .collect();
            if let [q] = fourth[..] {
                corners.insert(q);
            }
        }
        let mut refs: Vec<Ref> = frame.iter().map(|p| Ref::Point(*p)).collect();
        refs.extend(
            corners
                .iter()
                .filter(|p| !frame.contains(p))
                .map(|p| Ref::Point(*p)),
        );
        refs.extend(
            self.def
                .entities
                .iter()
                .filter(|e| {
                    line_ends(e).is_some_and(|(a, b)| corners.contains(&a) && corners.contains(&b))
                })
                .map(|e| Ref::Curve(e.id)),
        );
        refs
    }

    /// The anchors of the texts without a frame (mitcad#1911): where
    /// Move/Copy's middle counts them, with the entities' points and the
    /// frames' corners.
    pub fn free_text_anchors(&self, texts: &[EntityUid]) -> Result<Vec<P2>, String> {
        let mut anchors = Vec::new();
        for &id in texts {
            let text = self.text_of(id)?;
            if text.frame.is_empty() && text.path.is_none() {
                anchors.push(text.at);
            }
        }
        Ok(anchors)
    }

    /// The middle of the objects of a Move/Copy (mitcad#1911): the
    /// entities, the rectangles of the texts in frames (`carriers`) and the
    /// anchors of the other texts. Rotate turns about it without a pivot.
    pub fn middle_of_objects(&self, entities: &[Ref], texts: &[EntityUid]) -> Result<P2, String> {
        if entities.is_empty() && texts.is_empty() {
            return Err("give \"entities\" or \"texts\"".to_owned());
        }
        let anchors = self.free_text_anchors(texts)?;
        self.middle_with(entities, &anchors)
    }

    /// Moves texts by a map (mitcad#1911), after their frames' rectangles
    /// moved with the entities: a text without a frame moves its anchor
    /// where the map takes it and turns with a rotation; a text in a frame
    /// takes its frame's corner and direction as its anchor and angle.
    pub fn move_texts(&mut self, texts: &[EntityUid], map: &Map2) -> Result<(), String> {
        let turn = map.turn().unwrap_or(0.0);
        for &id in texts {
            let frame = self.text_of(id)?.frame.clone();
            let corner = if frame.is_empty() {
                None
            } else {
                self.frame_corner(&frame)
            };
            let Some(t) = self.def.texts.iter_mut().find(|t| t.id == id) else {
                continue;
            };
            if t.path.is_some() {
                continue;
            }
            match corner {
                Some((at, angle)) => {
                    t.at = at;
                    t.angle = angle;
                }
                None if frame.is_empty() => {
                    t.at = map.apply(t.at);
                    t.angle = wrapped(t.angle + turn);
                }
                None => {}
            }
        }
        Ok(())
    }

    /// Copies texts by a map (mitcad#1911, Move/Copy's Create Copy):
    /// `copies` gives the copy of each entity the copy of the entities made
    /// (a frame's rectangle among them). A copy follows the text parameter
    /// its text follows, where that is a parameter or an expression of
    /// parameters. Returns the copies, in the order of `texts`.
    pub fn copy_texts(
        &mut self,
        texts: &[EntityUid],
        map: &Map2,
        copies: &BTreeMap<EntityUid, EntityUid>,
    ) -> Result<Vec<EntityUid>, String> {
        let turn = map.turn().unwrap_or(0.0);
        let mut made = Vec::new();
        for &id in texts {
            let mut text = self.text_of(id)?.clone();
            text.id = self.id();
            text.at = map.apply(text.at);
            text.angle = wrapped(text.angle + turn);
            for p in &mut text.frame {
                *p = *copies
                    .get(p)
                    .ok_or_else(|| format!("text t{} has a frame point it lacks", id.0))?;
            }
            if let Some((at, angle)) = self.frame_corner(&text.frame) {
                text.at = at;
                text.angle = angle;
            }
            let copy = text.id;
            self.report.texts.push(copy);
            self.def.texts.push(text);
            // Its text parameter: a constant of its own is the text itself.
            if let Some(param) = self.def.text_value(id) {
                let (expression, _) = super::clipboard::value_of(self.params, self.owner(), param);
                if let Some(expression) = expression {
                    self.set_text_value(copy, Some(&ValueInput::Name(expression)))?;
                }
            }
            made.push(copy);
        }
        Ok(made)
    }
}
