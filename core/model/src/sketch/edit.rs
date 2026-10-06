// SPDX-License-Identifier: MIT
//! Editing a sketch: the operations behind the `sketch.*` commands. An
//! edit works on a copy of the definition with the document's parameters
//! (new dimensions get parameters owned by the sketch); when it is done,
//! the sketch is solved from the edited positions with the current values
//! and the solution is stored. A sketch that no longer solves rejects the
//! edit, as an over-constraining constraint is refused.

use super::solve::{SketchStatus, Solved, measure};
use super::{
    Constraint, ConstraintKind, ConstraintUid, Dimension, DimensionKind, Entity, EntityKind, Ref,
};
use crate::document::resolve_value;
use crate::features::ValueInput;
use crate::features::sketch::SketchDef;
use crate::ids::{EntityUid, FeatureUid};
use crate::parameters::{ParamId, Parameters};

/// What an edit created and removed, and the sketch's status after it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EditReport {
    pub entities: Vec<Ref>,
    pub texts: Vec<EntityUid>,
    pub constraints: Vec<ConstraintUid>,
    pub dimensions: Vec<ConstraintUid>,
    /// Parameters created for new dimensions.
    pub parameters: Vec<String>,
    /// Removed entities, constraints and dimensions.
    pub removed: Vec<String>,
    pub status: SketchStatus,
}

/// A point of a command: an existing point or a new one at a position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointInput {
    At([f64; 2]),
    Existing(EntityUid),
}

/// An edit in progress.
pub struct SketchEdit<'a> {
    pub def: SketchDef,
    pub(crate) params: &'a mut Parameters,
    owner: FeatureUid,
    owner_name: String,
    next_entity: u32,
    /// Ids to give out again before new ones (a remade offset keeps its
    /// curves' ids).
    pub(crate) reuse: Vec<EntityUid>,
    next_constraint: u32,
    pub report: EditReport,
    /// Solve and store when finishing (false for edits that set positions
    /// themselves, like a drag).
    pub solve: bool,
}

impl<'a> SketchEdit<'a> {
    pub fn new(
        def: SketchDef,
        params: &'a mut Parameters,
        owner: FeatureUid,
        owner_name: &str,
    ) -> Self {
        let next_entity = def.next_entity().0;
        let next_constraint = def.next_constraint().0;
        Self {
            def,
            params,
            owner,
            owner_name: owner_name.to_owned(),
            next_entity,
            reuse: Vec::new(),
            next_constraint,
            report: EditReport::default(),
            solve: true,
        }
    }

    pub fn owner(&self) -> FeatureUid {
        self.owner
    }

    /// The sketch's name, for parameter comments.
    pub fn owner_name(&self) -> &str {
        &self.owner_name
    }

    /// `count` new entity ids in order.
    pub fn ids(&mut self, count: usize) -> Vec<EntityUid> {
        (0..count)
            .map(|_| {
                if !self.reuse.is_empty() {
                    return self.reuse.remove(0);
                }
                self.next_entity += 1;
                EntityUid(self.next_entity - 1)
            })
            .collect()
    }

    pub fn id(&mut self) -> EntityUid {
        self.ids(1)[0]
    }

    /// Adds an entity whose id came from [`SketchEdit::ids`].
    pub fn push(&mut self, entity: Entity) -> EntityUid {
        let id = entity.id;
        self.report.entities.push(entity.as_ref());
        self.def.entities.push(entity);
        id
    }

    pub fn add_point(&mut self, at: [f64; 2]) -> EntityUid {
        let id = self.id();
        self.push(Entity::point(id, at))
    }

    /// An existing point, or a new one.
    pub fn point(&mut self, input: PointInput) -> Result<EntityUid, String> {
        match input {
            PointInput::At(at) => {
                if !at.iter().all(|v| v.is_finite()) {
                    return Err(format!("the point must be finite, got {at:?}"));
                }
                Ok(self.add_point(at))
            }
            PointInput::Existing(id) => match self.def.entity(id) {
                Some(e) if e.is_point() => Ok(id),
                Some(e) => Err(format!("{} is not a point", e.as_ref())),
                None => Err(format!("point p{} does not exist", id.0)),
            },
        }
    }

    /// The position of a point.
    pub fn at(&self, id: EntityUid) -> Result<[f64; 2], String> {
        match self.def.entity(id).map(|e| &e.kind) {
            Some(EntityKind::Point { at }) => Ok(*at),
            _ => Err(format!("point p{} does not exist", id.0)),
        }
    }

    pub fn add_curve(&mut self, kind: EntityKind) -> EntityUid {
        let id = self.id();
        self.push(Entity::new(id, kind))
    }

    /// A curve of the sketch.
    pub fn curve(&self, id: EntityUid) -> Result<&Entity, String> {
        match self.def.entity(id) {
            Some(e) if !e.is_point() => Ok(e),
            Some(_) => Err(format!("p{} is a point, not a curve", id.0)),
            None => Err(format!("curve c{} does not exist", id.0)),
        }
    }

    pub fn constraint_id(&mut self) -> ConstraintUid {
        self.next_constraint += 1;
        ConstraintUid(self.next_constraint - 1)
    }

    pub fn add_constraint(&mut self, kind: ConstraintKind) -> ConstraintUid {
        let id = self.constraint_id();
        self.def.constraints.push(Constraint { id, kind });
        self.report.constraints.push(id);
        id
    }

    /// The current geometry solved with the current values.
    pub fn solved(&self) -> Result<Solved, String> {
        let params = &*self.params;
        self.def
            .solve_with(
                &mut |id| {
                    params
                        .value(id)
                        .ok_or_else(|| format!("parameter {} does not exist", params.name(id)))
                },
                &|id| params.name(id),
            )
            .map_err(|e| e.message)
    }

    /// Adds a dimension: driven, or driving with a value (a number, a
    /// parameter name or an expression), or driving with the measured
    /// value when `value` is None. `comment` names the new parameter
    /// (`Sketch1 width`); by default it is the dimension type.
    pub fn add_dimension(
        &mut self,
        kind: DimensionKind,
        value: Option<&ValueInput>,
        driven: bool,
        comment: Option<&str>,
    ) -> Result<ConstraintUid, String> {
        let id = self.constraint_id();
        let mut dimension: Dimension<ParamId> = Dimension {
            id,
            kind,
            value: None,
            text: None,
        };
        if !driven {
            let measured;
            let value = match value {
                Some(value) => value,
                None => {
                    // Measure on the geometry as it is now.
                    let solved = self.solved()?;
                    measured = ValueInput::Number(
                        measure(&dimension.kind, &solved)
                            .ok_or("the dimension cannot be measured")?,
                    );
                    &measured
                }
            };
            let slot = dimension.slot();
            let label = dimension.kind.type_name().to_owned();
            let comment = format!("{} {}", self.owner_name, comment.unwrap_or(&label));
            let (param, created) =
                resolve_value(self.params, &slot, value, self.owner, &comment, None)
                    .map_err(|e| e.to_string())?;
            self.report.parameters.extend(created);
            dimension.value = Some(param);
        }
        self.def.dimensions.push(dimension);
        self.report.dimensions.push(id);
        Ok(id)
    }

    /// The number of redundant constraints now. A constraint or driving
    /// dimension that raises it is implied by the others; the commands that
    /// add one refuse it, as they refuse one that over-constrains the
    /// sketch (a dimension can be added as driven instead).
    pub fn redundancy(&self) -> Result<usize, String> {
        Ok(self.solved()?.status.redundant.len())
    }

    pub fn require_needed(&self, before: usize, what: &str) -> Result<(), String> {
        if self.redundancy()? > before {
            return Err(format!(
                "{what} would over-constrain the sketch: the others already fix what it \
                 constrains (a dimension can be added as driven)"
            ));
        }
        Ok(())
    }

    /// Checks the values, solves from the edited positions and stores the
    /// solution.
    pub fn finish(mut self) -> Result<(SketchDef, EditReport), String> {
        // Patterns follow structural edits of their originals.
        if !self.def.patterns.is_empty() {
            self.sync_patterns()?;
        }
        self.def.check_definition()?;
        let params = &*self.params;
        crate::features::FeatureInfo::check_values(&self.def, &|id| {
            params.value(id).unwrap_or(f64::NAN)
        })?;
        if self.solve {
            self.refit_offsets()?;
            let solved = self.solved()?;
            solved.store(&mut self.def.entities);
            // Texts in frames keep their frame's corner and direction.
            for t in &mut self.def.texts {
                if let Some((at, angle)) = super::text::frame_anchor(t, &solved.points) {
                    t.at = at;
                    t.angle = angle;
                }
            }
            self.report.status = solved.status;
        }
        Ok((self.def, self.report))
    }

    // V0's tools, used by the current application.

    /// A rectangle dimensioned by width and height from a fixed corner:
    /// lines `c<k>`..`c<k+3>` (corners counter-clockwise from `corner`),
    /// then the corner points, horizontal and vertical constraints, and
    /// length dimensions on the first two lines (see `legacy.rs`).
    pub fn dimensioned_rectangle(
        &mut self,
        corner: [f64; 2],
        width: &ValueInput,
        height: &ValueInput,
    ) -> Result<Vec<EntityUid>, String> {
        if !corner.iter().all(|v| v.is_finite()) {
            return Err(format!("the point must be finite, got {corner:?}"));
        }
        let lines = self.ids(4);
        let points = self.ids(4);
        // Start from the given size when it is a number; the dimensions
        // set it anyway.
        let size = |v: &ValueInput| match v {
            ValueInput::Number(n) if n.is_finite() && *n > 0.0 => *n,
            _ => 10.0,
        };
        let (w, h) = (size(width), size(height));
        let [x, y] = corner;
        let corners = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
        for i in 0..4 {
            self.push(Entity::new(
                lines[i],
                EntityKind::Line {
                    start: points[i],
                    end: points[(i + 1) % 4],
                    centerline: false,
                },
            ));
        }
        for i in 0..4 {
            let mut point = Entity::point(points[i], corners[i]);
            point.fixed = i == 0;
            self.push(point);
        }
        for (i, line) in lines.iter().enumerate() {
            self.add_constraint(if i.is_multiple_of(2) {
                ConstraintKind::Horizontal { line: *line }
            } else {
                ConstraintKind::Vertical { line: *line }
            });
        }
        self.add_dimension(
            DimensionKind::Length { line: lines[0] },
            Some(width),
            false,
            Some("width"),
        )?;
        self.add_dimension(
            DimensionKind::Length { line: lines[1] },
            Some(height),
            false,
            Some("height"),
        )?;
        Ok(lines)
    }

    /// A circle with a fixed centre and a diameter dimension.
    pub fn dimensioned_circle(
        &mut self,
        center: [f64; 2],
        diameter: &ValueInput,
    ) -> Result<EntityUid, String> {
        if !center.iter().all(|v| v.is_finite()) {
            return Err(format!("the point must be finite, got {center:?}"));
        }
        let ids = self.ids(2);
        let d = match diameter {
            ValueInput::Number(n) if n.is_finite() && *n > 0.0 => *n,
            _ => 10.0,
        };
        self.push(Entity::new(
            ids[0],
            EntityKind::Circle {
                center: ids[1],
                radius: d / 2.0,
            },
        ));
        let mut point = Entity::point(ids[1], center);
        point.fixed = true;
        self.push(point);
        self.add_dimension(
            DimensionKind::Diameter { curve: ids[0] },
            Some(diameter),
            false,
            Some("diameter"),
        )?;
        Ok(ids[0])
    }
}
