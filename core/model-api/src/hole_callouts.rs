// SPDX-License-Identifier: MIT
//! Hole callouts on the model (mitcad#902; `annotations.rs`, commands.md
//! "Hole callouts"): an annotation of the kind `hole_callout` on a hole's
//! round face or edge, whose text comes from the feature that made the
//! hole, written by the exported drawing's engine
//! (`mitcad_model_drawing::compose::hole_callout`) in ISO 129-1's symbols:
//! the count of the feature's holes (`4x`), the thread of a tapped hole or
//! `Ø` and the drill's diameter, `THRU` or `↧` and the depth, then the
//! counterbore (`⌴ Ø11 ↧ 6.8`) or countersink (`⌵ Ø12 × 90°`). It is worked
//! out whenever the annotations are asked for, so it follows the hole's
//! size, depth and thread. A round face of a thread feature gives the
//! thread; any other round face or edge its diameter.

use mitcad_vec3::{add_scaled, dot, sub, unit};

use super::analysis::MeasuredPicks;
use super::measurements::across;
use super::prelude::*;
use mitcad_model::datum::Vec3;
use mitcad_model::features::{
    FeatureDef, HoleDef, HoleExtent, HoleKind, HoleThread, ThreadData, ThreadSize,
};
use mitcad_model::ids::FeatureUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::parameters::ParamId;
use mitcad_model::topo::{FaceName, RoleKey};
use mitcad_model::{Document, FeatureStatus, MeasurePick};
use mitcad_model_drawing::RoundHole;
use mitcad_model_drawing::Units;
use mitcad_model_drawing::internal::{CalloutStyle, HoleInput, hole_callout};

/// The faces a pick names: its face, or the two an edge lies between.
fn picked_faces(pick: &MeasurePick) -> Vec<FaceName> {
    match (&pick.face, &pick.edge) {
        (Some(face), _) => vec![face.clone()],
        (None, Some(edge)) => edge.faces().to_vec(),
        _ => Vec::new(),
    }
}

/// The hole a face is part of: the feature that made it and the hole's
/// index (`F5:hole2.wall`, `F5:hole2.cbore_wall`), also inside the name of
/// a face made from it (a pattern's copy, a thread cut into the wall).
fn hole_of(face: &FaceName) -> Option<(FeatureUid, usize)> {
    let index = face
        .role
        .strip_prefix("hole")
        .and_then(|rest| rest.split_once('.'))
        .and_then(|(digits, _)| digits.parse::<usize>().ok());
    match (index, &face.key) {
        (Some(index), _) => Some((face.feature, index)),
        (None, Some(RoleKey::Face(inner))) => hole_of(inner),
        _ => None,
    }
}

/// A thread's designation as a callout writes it: `M6x1 - 6H`, a
/// left-handed one's `LH` after it, a multi-start one's starts
/// (`10x3 2 STARTS`).
fn thread_words(size: &ThreadSize, data: &ThreadData, starts: u32) -> String {
    let mut words = data.designation.clone();
    if let Some(class) = size
        .class
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        words.push_str(&format!(" - {class}"));
    }
    if !size.right_handed {
        words.push_str(" LH");
    }
    if starts > 1 {
        words.push_str(&format!(" {starts} STARTS"));
    }
    words
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777).
pub(super) trait HoleCalloutsOps<K: Kernel> {
    /// A hole callout's lines for its pick (`picks` as measured), in
    /// `units`; why there are none when the pick is no round face or edge.
    fn hole_callout_lines(
        &self,
        pick: &MeasurePick,
        picks: &MeasuredPicks,
        units: Units,
    ) -> Result<Vec<String>, String>;

    /// Where a hole callout's arrow points (mitcad#902): on the rim of the
    /// round face picked at the end of it towards `toward` (the side the
    /// annotation is seen from; the higher end along the axis without
    /// one), at the point of the rim nearest `at` (where the hole was
    /// clicked); none when the pick is no round face the kernel knows.
    fn hole_callout_anchor(
        &self,
        pick: &MeasurePick,
        at: Option<Vec3>,
        toward: Option<Vec3>,
    ) -> Option<Vec3>;
}

/// The document's methods used only in this module.
trait HoleCalloutsLocal<K: Kernel> {
    fn param_value(&self, id: ParamId) -> Option<f64>;

    /// The size and starts of a thread: a custom one's of its values, else
    /// the table's.
    fn thread_data(
        &self,
        size: &ThreadSize,
        custom: Option<(ParamId, ParamId)>,
        starts: Option<ParamId>,
    ) -> Option<(ThreadData, u32)>;

    /// The callout of hole `index` of a hole feature.
    fn hole_feature_lines(
        &self,
        uid: FeatureUid,
        def: &HoleDef,
        index: usize,
        pick: &MeasurePick,
        units: Units,
    ) -> Result<Vec<String>, String>;

    /// The length of a hole's wall as the kernel measures it on the
    /// pick's body.
    fn wall_length(&self, pick: &MeasurePick, uid: FeatureUid, index: usize) -> Option<f64>;
}

impl<K: Kernel> HoleCalloutsLocal<K> for Document<K> {
    fn param_value(&self, id: ParamId) -> Option<f64> {
        self.parameters().value(id)
    }

    fn thread_data(
        &self,
        size: &ThreadSize,
        custom: Option<(ParamId, ParamId)>,
        starts: Option<ParamId>,
    ) -> Option<(ThreadData, u32)> {
        let data = match custom {
            Some((major, pitch)) => {
                ThreadData::custom(self.param_value(major)?, self.param_value(pitch)?).ok()?
            }
            None => size.data().ok()?,
        };
        let starts = starts
            .and_then(|id| self.param_value(id))
            .map_or(data.starts, |n| n.round().max(1.0) as u32);
        Some((data, starts))
    }

    fn wall_length(&self, pick: &MeasurePick, uid: FeatureUid, index: usize) -> Option<f64> {
        let shape = self.body_shape(pick.body?)?;
        let wall = mitcad_model::features::hole::hole_face(uid, index, "wall");
        self.kernel()
            .face_cylinder(shape, &wall)
            .ok()
            .map(|c| c.length)
    }

    fn hole_feature_lines(
        &self,
        uid: FeatureUid,
        def: &HoleDef,
        index: usize,
        pick: &MeasurePick,
        units: Units,
    ) -> Result<Vec<String>, String> {
        let missing = || "the hole's size is not known".to_owned();
        // A press-fit hole (mitcad#1517) is not round: its flats or crush
        // ribs are dimensioned as they are.
        if def.press_fit.is_some() {
            return Err(
                "a press-fit hole is not round: dimension its flats or its ribs' tips".to_owned(),
            );
        }
        let thread: Option<&HoleThread> = def.thread.as_ref();
        let sized = thread.and_then(|t| {
            let custom = t.custom.as_ref().map(|c| (c.major, c.pitch));
            self.thread_data(&t.size(), custom, t.starts)
                .map(|(data, starts)| (t, data, starts))
        });
        // A tapped hole's bore is its thread's minor diameter (a taper
        // pipe thread's the hole's own, the tap drill).
        let diameter = match &sized {
            Some((t, data, _)) if !t.tapers() => data.minor,
            _ => self.param_value(def.diameter).ok_or_else(missing)?,
        };
        let (through, depth) = match &def.extent {
            HoleExtent::ThroughAll => (true, 0.0),
            HoleExtent::Distance { depth } => {
                (false, self.param_value(*depth).ok_or_else(missing)?)
            }
            HoleExtent::ToObject { .. } => (
                false,
                self.wall_length(pick, uid, index).ok_or_else(missing)?,
            ),
        };
        let mut hole = RoundHole {
            face: None,
            opening: [0.0; 3],
            direction: [0.0, 0.0, -1.0],
            diameter,
            through,
            depth,
            counterbore: None,
            countersink: None,
        };
        match &def.kind {
            HoleKind::Simple => {}
            HoleKind::Counterbore { diameter, depth }
            | HoleKind::Counterdrill {
                diameter, depth, ..
            } => {
                hole.counterbore = Some((
                    self.param_value(*diameter).ok_or_else(missing)?,
                    self.param_value(*depth).ok_or_else(missing)?,
                ));
            }
            HoleKind::Countersink { diameter, angle } => {
                hole.countersink = Some((
                    self.param_value(*diameter).ok_or_else(missing)?,
                    self.param_value(*angle).ok_or_else(missing)?,
                ));
            }
        }
        // A thread along part of the wall: its length and where it starts.
        let thread_depth = sized.as_ref().and_then(|(t, _, _)| {
            let length = self.param_value(t.length?)?;
            Some(t.offset.and_then(|o| self.param_value(o)).unwrap_or(0.0) + length)
        });
        let input = HoleInput {
            hole,
            thread: sized
                .as_ref()
                .map(|(t, data, starts)| thread_words(&t.size(), data, *starts)),
        };
        Ok(hole_callout(
            &input,
            def.count(),
            thread_depth,
            units,
            CalloutStyle::Symbols,
        ))
    }
}

impl<K: Kernel> HoleCalloutsOps<K> for Document<K> {
    fn hole_callout_lines(
        &self,
        pick: &MeasurePick,
        picks: &MeasuredPicks,
        units: Units,
    ) -> Result<Vec<String>, String> {
        let faces = picked_faces(pick);
        if faces.is_empty() || pick.body.is_none() {
            return Err("a hole callout points at a hole: its round face or edge".to_owned());
        }
        // A hole of a hole feature.
        for face in &faces {
            let Some((uid, index)) = hole_of(face) else {
                continue;
            };
            let Some(entry) = self.feature(uid) else {
                continue;
            };
            if let FeatureDef::Hole(def) = &entry.def {
                if self.status(uid) != Some(&FeatureStatus::Ok) {
                    return Err(format!("{} is not built", entry.name));
                }
                return self.hole_feature_lines(uid, def, index, pick, units);
            }
        }
        // A face of a thread feature.
        for entry in self.features() {
            let FeatureDef::Thread(def) = &entry.def else {
                continue;
            };
            if self.status(entry.uid) != Some(&FeatureStatus::Ok)
                || !def
                    .faces
                    .iter()
                    .any(|f| Some(f.body) == pick.body && faces.contains(&f.face))
            {
                continue;
            }
            let custom = def.custom.as_ref().map(|c| (c.major, c.pitch));
            let Some((data, starts)) = self.thread_data(&def.thread, custom, def.starts) else {
                continue;
            };
            let words = thread_words(&def.thread, &data, starts);
            return Ok(vec![match def.length.and_then(|id| self.param_value(id)) {
                Some(length) => format!("{words} ↧ {}", units.length(length)),
                None => words,
            }]);
        }
        // Any other round face or edge: its diameter.
        let radius = picks
            .single
            .as_ref()
            .and_then(|m| m.circle)
            .map(|c| c.radius)
            .filter(|r| *r > 0.0)
            .or_else(|| {
                let shape = self.body_shape(pick.body?)?;
                let face = pick.face.as_ref()?;
                self.kernel()
                    .face_cylinder(shape, face)
                    .ok()
                    .map(|c| c.radius)
            });
        match radius {
            Some(r) => Ok(vec![format!("Ø{}", units.length(2.0 * r))]),
            None => Err("a hole callout points at a hole: its round face or edge".to_owned()),
        }
    }

    fn hole_callout_anchor(
        &self,
        pick: &MeasurePick,
        at: Option<Vec3>,
        toward: Option<Vec3>,
    ) -> Option<Vec3> {
        let face = pick.face.as_ref()?;
        let shape = self.body_shape(pick.body?)?;
        let cylinder = self.kernel().face_cylinder(shape, face).ok()?;
        let transform = self.pick_transform(pick).ok()?;
        let axis = unit(transform.apply_vector(cylinder.axis.direction))?;
        let low = transform.apply_point(cylinder.axis.origin);
        let high = add_scaled(low, axis, cylinder.length);
        // The end the annotation is seen from: the hole's opening there.
        let up = toward.unwrap_or([0.0, 0.0, 1.0]);
        let rim = if dot(sub(high, low), up) >= 0.0 {
            high
        } else {
            low
        };
        let radial = at
            .and_then(|p| {
                let off = sub(p, rim);
                unit(add_scaled(off, axis, -dot(off, axis)))
            })
            .unwrap_or_else(|| across(axis));
        Some(add_scaled(rim, radial, cylinder.radius))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hole_is_found_in_its_faces_names() {
        let wall = mitcad_model::features::hole::hole_face(FeatureUid(5), 2, "wall");
        assert_eq!(hole_of(&wall), Some((FeatureUid(5), 2)));
        let cbore = mitcad_model::features::hole::hole_face(FeatureUid(7), 0, "cbore_wall");
        assert_eq!(hole_of(&cbore), Some((FeatureUid(7), 0)));
        // A face made from the wall (a pattern's copy) names it.
        let copy = FaceName::new(
            FeatureUid(9),
            "inst1",
            Some(RoleKey::Face(Box::new(wall.clone()))),
        );
        assert_eq!(hole_of(&copy), Some((FeatureUid(5), 2)));
        let side: FaceName = "F2:side(c1)".parse().unwrap();
        assert_eq!(hole_of(&side), None);
    }
}
