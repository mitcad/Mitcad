// SPDX-License-Identifier: MIT
//! Boundary Fill (mitcad#457): solids of the cells that tools enclose.
//!
//! With solid tools only, the tools' faces divide space into cells, each
//! inside some of the tools and outside the others; a point inside a cell
//! picks it, all of it (several solids where that space falls apart).
//!
//! With a surface body or a plane among the tools (mitcad#2149: untrimmed,
//! overlapping surfaces and construction or origin planes), a cell is the
//! closed region around its point that the faces of all the tools and the
//! planes bound ([`Kernel::boundary_regions`]); a plane is cut to the
//! space the other tools span. A point in a region the tools leave open
//! fails in words, the gap it leaks through painted.
//!
//! The cells become new bodies, one per cell in the order of the points,
//! or join, cut or intersect the participant bodies as one tool, as an
//! extrusion's tool does (`bodies::apply_tool`: without participants, all
//! bodies of the component, the tools that stay among them). The tools
//! stay unless removed; removed tools are gone before the operation. Two
//! points in one cell fill it once, with a warning.
//!
//! A cell's faces are pieces of the tools' faces and keep their names
//! (`F2:side(c1[c4,c2])`, pieces of one face numbered `#k`), as a boolean
//! keeps them ([`Kernel::boundary_cells`]); pieces of the i-th plane are
//! `<feature>:plane<i>`. The new bodies are `<feature>.b<i>`. The tools
//! are read, never changed.

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use super::bodies::{BodySet, apply_tool, boolean_op, verb};
use super::extrude::check_participants;
use super::geom_ref::{GeomRef, Want};
use super::mapping::map_params;
use super::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, Operation, References,
    is_false,
};
use crate::datum::{DatumPlane, Vec3};
use crate::exchange::BodyKind;
use crate::ids::BodyUid;
use crate::kernel::{BoundaryPlace, Kernel};
use crate::notice::{FeatureError, NoticePlace};
use crate::parameters::ParamId;

/// Has no values; `P` only matches the other feature types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundaryFillDef<P = ParamId> {
    /// The bodies whose faces bound the cells: solids and surface bodies.
    #[serde(default)]
    pub tools: Vec<BodyUid>,
    /// Planes that bound the cells too (mitcad#2149): origin planes,
    /// construction planes, planar faces (their whole plane) and fixed
    /// planes, written as Split Body's plane tool is.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub planes: Vec<GeomRef>,
    /// A point inside each cell to fill, in the coordinates of the
    /// feature's component.
    pub cells: Vec<Vec3>,
    pub operation: Operation,
    /// The bodies a join, cut or intersection works on; empty for all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<BodyUid>,
    /// Takes the tools out of the design (before the operation).
    #[serde(default, skip_serializing_if = "is_false")]
    pub remove_tools: bool,
    #[serde(skip)]
    pub values: PhantomData<P>,
}

impl<P> BoundaryFillDef<P> {
    pub const TYPE: &'static str = "boundary_fill";
    pub const BASE_NAME: &'static str = "BoundaryFill";

    map_params! {
        BoundaryFillDef {
            tools: clone,
            planes: clone,
            cells: clone,
            operation: copy,
            participants: clone,
            remove_tools: copy,
            values: phantom,
        }
    }
}

/// A point as agents read it: "(10, 5, 2.5)".
fn point_text(p: Vec3) -> String {
    let number = |v: f64| {
        let v = if v.abs() < 1e-9 { 0.0 } else { v };
        format!("{}", (v * 1e6).round() / 1e6)
    };
    format!("({}, {}, {})", number(p[0]), number(p[1]), number(p[2]))
}

impl FeatureInfo for BoundaryFillDef {
    fn references(&self) -> References {
        let mut references = References::default();
        for body in self.tools.iter().chain(&self.participants) {
            references.body(*body);
        }
        for plane in &self.planes {
            plane.add_to(&mut references);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if self.tools.is_empty() && self.planes.is_empty() {
            return Err("no tool bodies or planes selected".to_owned());
        }
        for (i, tool) in self.tools.iter().enumerate() {
            if self.tools[..i].contains(tool) {
                return Err(format!("body {tool} is listed more than once"));
            }
            ctx.body(*tool)?;
        }
        for (i, plane) in self.planes.iter().enumerate() {
            if self.planes[..i].contains(plane) {
                return Err(format!("plane {} is listed more than once", i + 1));
            }
            plane
                .check(ctx, Want::Plane)
                .map_err(|e| format!("plane {}: {e}", i + 1))?;
        }
        if self.tools.is_empty() && self.planes.len() < 4 {
            return Err("planes alone close off a region only with four or more".to_owned());
        }
        if self.cells.is_empty() {
            return Err("no cells picked: a point inside each cell to fill".to_owned());
        }
        if self.cells.iter().flatten().any(|v| !v.is_finite()) {
            return Err("the points of the cells must be finite".to_owned());
        }
        check_participants(ctx, self.operation, &self.participants)?;
        if self.remove_tools
            && let Some(body) = self.participants.iter().find(|b| self.tools.contains(b))
        {
            return Err(format!(
                "body {body} is a tool the fill removes; it cannot be a participant"
            ));
        }
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        // The cells' new bodies, and the pieces of bodies a cut splits.
        true
    }

    fn new_component(&self) -> bool {
        self.operation == Operation::NewComponent
    }
}

impl BoundaryFillDef {
    /// The planes, resolved in the feature's coordinates.
    fn resolved_planes<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<Vec<DatumPlane>, String> {
        self.planes
            .iter()
            .enumerate()
            .map(|(i, plane)| {
                ctx.plane(plane)
                    .map_err(|e| format!("plane {}: {e}", i + 1))
            })
            .collect()
    }

    /// Whether the cells are the regions the tools' faces bound
    /// (mitcad#2149): with a plane or a surface body among the tools.
    /// Solid tools only keep the cells of [`Kernel::boundary_cells`].
    fn by_faces<K: Kernel>(&self, ctx: &EvalContext<'_, K>, tools: &[K::Shape]) -> bool {
        !self.planes.is_empty()
            || tools
                .iter()
                .any(|tool| matches!(ctx.kernel.body_kind(tool), Ok(BodyKind::Sheet)))
    }

    /// The cells the points pick among solid tools, one per distinct cell
    /// in the order of the points (a second point in a cell warns).
    fn solid_cells<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        tools: &[K::Shape],
    ) -> Result<Vec<K::Shape>, String> {
        let refs: Vec<&K::Shape> = tools.iter().collect();
        let cells = ctx
            .kernel
            .boundary_cells(&refs, &self.cells)
            .map_err(|e| e.to_string())?;
        if cells.len() != self.cells.len() {
            return Err(format!(
                "the geometry kernel gave {} cells for {} points",
                cells.len(),
                self.cells.len()
            ));
        }
        // A cell is the same cell as another when the same tools hold its
        // point.
        let mut filled: Vec<K::Shape> = Vec::new();
        for (i, cell) in cells.iter().enumerate() {
            if let Some(j) = cells[..i].iter().position(|c| c.holders == cell.holders) {
                ctx.warn(format!(
                    "points {} and {} pick the same cell; it is filled once",
                    j + 1,
                    i + 1
                ));
                continue;
            }
            filled.push(cell.shape.clone());
        }
        Ok(filled)
    }

    /// The closed regions the points pick among the faces of the tools and
    /// the planes (mitcad#2149), one per distinct region in the order of
    /// the points (a second point in a region warns).
    fn region_cells<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        tools: &[K::Shape],
    ) -> Result<Vec<K::Shape>, String> {
        let planes = self.resolved_planes(ctx)?;
        let refs: Vec<&K::Shape> = tools.iter().collect();
        let found = ctx
            .kernel
            .boundary_regions(ctx.uid, &refs, &planes, &self.cells)
            .map_err(|e| e.to_string())?;
        if found.places.len() != self.cells.len() {
            return Err(format!(
                "the geometry kernel placed {} of {} points",
                found.places.len(),
                self.cells.len()
            ));
        }
        let mut picked: Vec<(usize, usize)> = Vec::new(); // (region, point)
        for (i, place) in found.places.iter().enumerate() {
            let point = self.cells[i];
            match place {
                BoundaryPlace::Region(region) => {
                    if let Some((_, j)) = picked.iter().find(|(r, _)| r == region) {
                        ctx.warn(format!(
                            "points {} and {} pick the same cell; it is filled once",
                            j + 1,
                            i + 1
                        ));
                        continue;
                    }
                    if *region >= found.regions.len() {
                        return Err(format!(
                            "the geometry kernel placed point {} in region {} of {}",
                            i + 1,
                            region + 1,
                            found.regions.len()
                        ));
                    }
                    picked.push((*region, i));
                }
                BoundaryPlace::OnFace => {
                    return Err(FeatureError::new(
                        "a point of the cells lies on a face of the tools: pick it inside the \
                         region to fill",
                    )
                    .with_detail(format!(
                        "point {} {} lies on a face of the tools or a plane",
                        i + 1,
                        point_text(point)
                    ))
                    .at(NoticePlace::point(point))
                    .text());
                }
                BoundaryPlace::Open { gap } => {
                    return Err(FeatureError::new(
                        "the tools do not close off the region of a cell: it is open where the \
                         surfaces end; extend them or add a surface or a plane across the gap",
                    )
                    .with_detail(format!(
                        "point {} {} is in a region the tools and planes leave open; the gap \
                         runs along {} painted points",
                        i + 1,
                        point_text(point),
                        gap.len()
                    ))
                    .at(NoticePlace::point(point))
                    .with_places(gap.iter().map(|p| NoticePlace::point(*p)))
                    .text());
                }
            }
        }
        Ok(picked
            .into_iter()
            .map(|(region, _)| found.regions[region].shape.clone())
            .collect())
    }
}

impl<K: Kernel> Evaluate<K> for BoundaryFillDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        // Mesh bodies are refused (mitcad#399).
        let tools = self
            .tools
            .iter()
            .map(|uid| ctx.surface_body(*uid))
            .collect::<Result<Vec<_>, _>>()?;
        let filled = if self.by_faces(ctx, &tools) {
            self.region_cells(ctx, &tools)?
        } else {
            self.solid_cells(ctx, &tools)?
        };
        let mut bodies = BodySet::new(if self.operation.makes_bodies() {
            self.tools
                .iter()
                .copied()
                .zip(tools.iter().cloned())
                .collect()
        } else {
            ctx.bodies()
        });
        if self.remove_tools {
            for uid in &self.tools {
                bodies.remove(*uid);
            }
        }
        let mut tool = None;
        if self.operation.makes_bodies() {
            for cell in filled {
                let uid = ctx.new_body();
                bodies.set(uid, cell);
            }
        } else {
            let united = if filled.len() == 1 {
                filled.into_iter().next().expect("one cell")
            } else {
                let refs: Vec<&K::Shape> = filled.iter().collect();
                ctx.kernel.unite(&refs).map_err(|e| e.to_string())?
            };
            if !apply_tool(
                ctx,
                &mut bodies,
                self.operation,
                &self.participants,
                &united,
            )? {
                let op = boolean_op(self.operation).expect("a join, cut or intersection");
                return Err(format!(
                    "the filled cells do not {} any participant body",
                    match verb(op) {
                        "join" => "touch",
                        "cut" => "cut into",
                        other => other,
                    }
                ));
            }
            tool = Some(united);
        }
        Ok(FeatureOutput {
            changes: bodies.changes(),
            tool,
            ..FeatureOutput::default()
        })
    }

    /// With surface or plane tools (mitcad#2149), every closed region the
    /// tools bound, for the open command's preview only: see-through
    /// beside the filled cells, so that the regions not picked yet show
    /// too.
    fn preview_tool(&self, ctx: &mut EvalContext<'_, K>) -> Option<K::Shape> {
        let tools = self
            .tools
            .iter()
            .map(|uid| ctx.surface_body(*uid))
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        if !self.by_faces(ctx, &tools) {
            return None;
        }
        let planes = self.resolved_planes(ctx).ok()?;
        let refs: Vec<&K::Shape> = tools.iter().collect();
        let found = ctx
            .kernel
            .boundary_regions(ctx.uid, &refs, &planes, &[])
            .ok()?;
        let shapes: Vec<K::Shape> = found.regions.into_iter().map(|r| r.shape).collect();
        if shapes.is_empty() {
            return None;
        }
        ctx.kernel.compound(&shapes).ok()
    }
}
