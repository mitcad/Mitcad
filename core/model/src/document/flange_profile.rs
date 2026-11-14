// SPDX-License-Identifier: MIT
//! A sheet-metal flange's profile sketch (mitcad#1648): the sketch whose
//! closed region on the bend line is the wall's outline
//! (`SheetFlangeDef::profile`). `sheet_flange_profile` makes it from a
//! flange's definition where the flange puts its wall: a flange-wall
//! construction plane (hidden, mitcad#1776) and a sketch on it with the
//! rectangle the flange's length and width give now, its side on the bend
//! line fixed there; both go in at the marker, before the flange, as one
//! undo step. The plane follows the flange's wall at every recompute once
//! the flange names the sketch (`PlaneDefinition::FlangeWall`). For a
//! definition that already has a profile, the profile's plane is made a
//! flange-wall plane (a fixed plane of an earlier design) and its stored
//! frame put where the definition puts the wall. A design opened with a
//! profile on a fixed plane of its own gets the flange-wall plane
//! ([`DocState::settle_flange_profiles`]).

use std::sync::Arc;

use super::{DocState, Document, ModelError, invalid};
use crate::features::construction::PlaneDefinition;
use crate::features::{
    ConstructionPlaneDef, FeatureDef, FeatureEntry, SketchDef, SketchPlane, ValueInput,
};
use crate::ids::FeatureUid;
use crate::kernel::Kernel;
use crate::profile::SketchFrame;
use crate::sketch::edit::SketchEdit;
use crate::sketch::{ConstraintKind, DimensionKind, Entity, EntityKind};

/// What `sheet_flange_profile` made or moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlangeProfile {
    /// The profile sketch.
    pub sketch: FeatureUid,
    /// The construction plane it lies on.
    pub plane: FeatureUid,
    /// The features it added before the flange: 2 for a new profile (the
    /// plane and the sketch), 0 when it moved an existing one.
    pub added: usize,
}

/// The flange-wall plane of a profile sketch, at `frame` until a flange
/// names the sketch (mitcad#1776).
fn on_wall<P>(frame: SketchFrame) -> FeatureDef<P> {
    FeatureDef::ConstructionPlane(ConstructionPlaneDef::new(PlaneDefinition::FlangeWall {
        origin: frame.origin,
        x_axis: frame.x_axis,
        y_axis: frame.y_axis,
    }))
}

/// Whether two frames are the same within the modelling tolerance.
fn same_frame(a: &SketchFrame, origin: [f64; 3], x_axis: [f64; 3], y_axis: [f64; 3]) -> bool {
    let near = |p: [f64; 3], q: [f64; 3], tolerance: f64| {
        p.iter().zip(q).all(|(u, v)| (u - v).abs() <= tolerance)
    };
    near(a.origin, origin, 1e-7) && near(a.x_axis, x_axis, 1e-9) && near(a.y_axis, y_axis, 1e-9)
}

/// Draws the profile's first wall into a new sketch: a rectangle from
/// `min` (on the bend line) to `max`, its two corners on the bend line
/// fixed there, its sides vertical, its far side horizontal, and its length
/// along the wall dimensioned (a new parameter).
fn draw_wall(
    state: &mut DocState,
    sketch: FeatureUid,
    min: [f64; 2],
    max: [f64; 2],
) -> Result<(), ModelError> {
    let position = state.require(sketch)?;
    let entry = state.features[position].clone();
    let FeatureDef::Sketch(def) = &entry.def else {
        return Err(invalid(format!("{} is not a sketch", entry.name)));
    };
    let (def, _) = {
        let mut edit = SketchEdit::new(def.clone(), &mut state.parameters, sketch, &entry.name);
        let lines = edit.ids(4);
        let points = edit.ids(4);
        let corners = [min, [max[0], min[1]], max, [min[0], max[1]]];
        for i in 0..4 {
            edit.push(Entity::new(
                lines[i],
                EntityKind::Line {
                    start: points[i],
                    end: points[(i + 1) % 4],
                    centerline: false,
                },
            ));
        }
        for (i, corner) in corners.into_iter().enumerate() {
            let mut point = Entity::point(points[i], corner);
            // The side on the bend line stays there.
            point.fixed = i < 2;
            edit.push(point);
        }
        edit.add_constraint(ConstraintKind::Vertical { line: lines[1] });
        edit.add_constraint(ConstraintKind::Horizontal { line: lines[2] });
        edit.add_constraint(ConstraintKind::Vertical { line: lines[3] });
        edit.add_dimension(
            DimensionKind::Length { line: lines[1] },
            Some(&ValueInput::Number(max[1] - min[1])),
            false,
            Some("wall length"),
        )
        .map_err(invalid)?;
        edit.finish().map_err(invalid)?
    };
    let edited = FeatureEntry {
        def: FeatureDef::Sketch(def),
        ..(*entry).clone()
    };
    state
        .check_feature(position, &edited)
        .map_err(ModelError::Invalid)?;
    state.features[position] = Arc::new(edited);
    Ok(())
}

impl<K: Kernel> Document<K> {
    /// The `sheet_flange_profile` command (mitcad#1648): a profile sketch
    /// for the flange `def` (a `sheet_flange` definition, as `add_feature`
    /// takes it), made at the marker on the wall where the flange puts it,
    /// or, when `def` names a profile already, that profile's plane moved
    /// onto the wall. The flange is read against the sheet at the marker:
    /// while a Flange is edited, the marker stands just before it.
    pub fn sheet_flange_profile(
        &mut self,
        def: &FeatureDef<ValueInput>,
    ) -> Result<FlangeProfile, ModelError> {
        if !matches!(def, FeatureDef::SheetFlange(_)) {
            return Err(invalid(
                "sheet_flange_profile takes a sheet_flange definition",
            ));
        }
        // The definition's values as numbers: it is added to a copy.
        let mut scratch = self.state.clone();
        let added = scratch.add_feature(def, None)?;
        let position = scratch.require(added.uid)?;
        let FeatureDef::SheetFlange(resolved) = &scratch.features[position].def else {
            return Err(invalid(
                "sheet_flange_profile takes a sheet_flange definition",
            ));
        };
        let d = resolved.map_params(&mut |slot, id| {
            scratch
                .parameters
                .value(*id)
                .ok_or_else(|| invalid(format!("the flange's {slot} has no value")))
        })?;
        let result = self.last_result();
        let sheet = result.sheets.get(&d.body).ok_or_else(|| {
            invalid("The flange's body is not a sheet-metal body where the flange is made.")
        })?;
        let rules = result.sheet_rules.get(&sheet.value.rules).ok_or_else(|| {
            invalid("The sheet's rules are not available where the flange is made.")
        })?;
        let layout = crate::sheet::folded::flange_profile_layout(&sheet.value, &rules.value, &d)
            .map_err(invalid)?;
        let frame = layout.frame;
        if let Some(sketch) = d.profile {
            return self.move_flange_profile(sketch, frame);
        }
        let (min, max) = (layout.min, layout.max);
        self.apply(|state| {
            let plane = state.add_feature(&on_wall(frame), None)?;
            // The plane only carries the sketch: hidden in the view.
            state.feature_visibility.insert(plane.uid, false);
            let sketch = state.add_feature(
                &FeatureDef::Sketch(SketchDef::new(SketchPlane::Construction(plane.uid))),
                None,
            )?;
            draw_wall(state, sketch.uid, min, max)?;
            Ok((
                format!("Add {} as a flange's profile", sketch.name),
                FlangeProfile {
                    sketch: sketch.uid,
                    plane: plane.uid,
                    added: 2,
                },
            ))
        })
    }

    /// An existing profile sketch's plane put on the wall (mitcad#1776): a
    /// fixed plane (an earlier design's) becomes a flange-wall plane, and a
    /// flange-wall plane's stored frame, used until a flange names the
    /// sketch, is moved to `frame` (no undo step when it is there
    /// already). The sketch's curves stay where they are in its own
    /// coordinates, the wall's.
    fn move_flange_profile(
        &mut self,
        sketch: FeatureUid,
        frame: SketchFrame,
    ) -> Result<FlangeProfile, ModelError> {
        let refused = || {
            invalid(
                "The flange's profile sketch does not lie on a fixed plane of its own, so it \
                 cannot be moved onto the wall.",
            )
        };
        let entry = self
            .state
            .entry(sketch)
            .ok_or(ModelError::NoSuchFeature(sketch))?;
        let FeatureDef::Sketch(def) = &entry.def else {
            return Err(refused());
        };
        let SketchPlane::Construction(plane) = &def.plane else {
            return Err(refused());
        };
        let plane = *plane;
        let plane_entry = self.state.entry(plane).ok_or_else(refused)?;
        let FeatureDef::ConstructionPlane(plane_def) = &plane_entry.def else {
            return Err(refused());
        };
        let made = FlangeProfile {
            sketch,
            plane,
            added: 0,
        };
        match &plane_def.definition {
            PlaneDefinition::FlangeWall {
                origin,
                x_axis,
                y_axis,
            } if same_frame(&frame, *origin, *x_axis, *y_axis) => return Ok(made),
            PlaneDefinition::Fixed { .. } | PlaneDefinition::FlangeWall { .. } => {}
            _ => return Err(refused()),
        }
        let name = plane_entry.name.clone();
        self.apply(|state| {
            state.edit_feature(plane, &on_wall(frame))?;
            Ok((format!("Move {name} onto the flange's wall"), made))
        })
    }
}

impl DocState {
    /// Makes the fixed plane of each flange's profile sketch a flange-wall
    /// plane (mitcad#1776), as a design opened from an earlier version
    /// needs: a sketch named by a flange's `profile` that lies on a fixed
    /// construction plane of its own (no other feature refers to the plane,
    /// the plane links nothing, is not flipped, and the sketch is not on
    /// another component's plane). The stored frame stays the fixed one, so
    /// nothing moves until the plane is evaluated on the flange's wall; the
    /// sketch's curves stay in the wall's coordinates. A sketch on any other
    /// plane is left where the user put it (the flange warns when it is not
    /// on its wall).
    pub(crate) fn settle_flange_profiles(&mut self) {
        let mut planes = Vec::new();
        for entry in &self.features {
            let FeatureDef::SheetFlange(flange) = &entry.def else {
                continue;
            };
            let Some(sketch) = flange.profile else {
                continue;
            };
            let Some(FeatureDef::Sketch(def)) = self.entry(sketch).map(|e| &e.def) else {
                continue;
            };
            let SketchPlane::Construction(plane) = def.plane else {
                continue;
            };
            if def.plane_link.is_some() || planes.contains(&plane) {
                continue;
            }
            let Some(FeatureDef::ConstructionPlane(plane_def)) = self.entry(plane).map(|e| &e.def)
            else {
                continue;
            };
            let own = matches!(plane_def.definition, PlaneDefinition::Fixed { .. })
                && !plane_def.flip
                && plane_def.links.is_empty()
                && plane_def.datum_links.is_empty();
            let shared = self.features.iter().any(|other| {
                other.uid != sketch && other.def.references().features.contains(&plane)
            });
            if own && !shared {
                planes.push(plane);
            }
        }
        for plane in planes {
            let Some(position) = self.features.iter().position(|f| f.uid == plane) else {
                continue;
            };
            let entry = Arc::make_mut(&mut self.features[position]);
            if let FeatureDef::ConstructionPlane(plane_def) = &mut entry.def
                && let PlaneDefinition::Fixed {
                    origin,
                    x_axis,
                    y_axis,
                } = plane_def.definition
            {
                plane_def.definition = PlaneDefinition::FlangeWall {
                    origin,
                    x_axis,
                    y_axis,
                };
            }
        }
    }
}
