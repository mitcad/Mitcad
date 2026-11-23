// SPDX-License-Identifier: MIT
//! Cell Grid (mitcad#2238; commands.md "Cell Grid"): the closed profiles of
//! a sketch filled with cells of one shape (hexagons, squares, triangles,
//! circles or regular polygons) of a size and a web between them, kept
//! out of the keep-outs (loops inside the field), and cut out of bodies in
//! one operation: vents, grilles, lightening patterns.
//!
//! The cells are planned in plain Rust ([`plan`], a pure function of the
//! field's loops and the grid's values), then swept along the sketch
//! normal as one extrusion of all their outlines, the tool, which cuts the
//! target bodies once. Trimmed edge cells (`clip: free`) are the cells'
//! tool intersected with the field's own prism first. The bodies keep their
//! ids; the input shapes are not changed.
//!
//! A cell's side faces are named after its lattice index, so that a
//! fillet on a cell's edge follows edits that keep the cell: side `k` of
//! cell `(i, j)` is swept from curve `c<n>` with
//! `n = 2^30 + k + 128 (2048 I + J)`, `I` and `J` being `i + 1024` and
//! `j + 1024` modulo 2048 (`k` 100 for the line a snapped piece is cut
//! along; a circle has the one side 0). Its faces are
//! `<feature>:side(c<n>[c<before>,c<after>])` (a circle's
//! `<feature>:side(c<n>)`), its floor `<feature>:end(r{…})`.

mod plan;

pub use plan::{
    CHORD, CellClip, CellRows, CellShape, GridSpec, MAX_CELLS, MAX_SIDES, Outline, Piece, Plan,
    PlannedCell, field_regions, plan,
};

use serde::{Deserialize, Serialize};

use super::base::apply_tool;
use super::extrude::{ProfileRef, check_profiles, profile_references, profile_regions};
use super::face_refs::kernel_error;
use super::geom_ref::{GeomRef, Want};
use super::mapping::map_params;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    is_false,
};
use crate::ids::{BodyUid, EntityUid};
use crate::kernel::{BooleanOp, ExtrudeFeatureSpec, ExtrudeSide, ExtrudeStart, Kernel, SideEnd};
use crate::parameters::ParamId;
use crate::profile::{ProfileLoop, ProfileRegion, ProfileSegment, SegmentGeometry, SketchFrame};
use crate::topo::{RegionKey, SegmentKey};

/// A polygon cell's sides when none are given.
pub const DEFAULT_SIDES: u32 = 8;

/// Which way the cells go from the sketch plane.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellDirection {
    /// Along the sketch normal.
    Positive,
    /// Against the sketch normal: into the body a sketch on its face lies
    /// on.
    #[default]
    Negative,
    /// Both ways, the depth to each side.
    Symmetric,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct CellGridDef<P = ParamId> {
    /// The field: regions of one sketch. Their distinct loops nest: an
    /// outer loop is a field, a loop inside it a keep-out, a loop inside
    /// that a field again.
    #[serde(deserialize_with = "super::extrude::profile_set")]
    pub profiles: Vec<ProfileRef>,
    #[serde(default, skip_serializing_if = "is_hexagon")]
    pub shape: CellShape,
    /// A polygon's sides, a whole number from 3 to 64; default 8.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub sides: Option<P>,
    /// How circles and polygons are laid out.
    #[serde(default, skip_serializing_if = "is_staggered")]
    pub rows: CellRows,
    /// A cell's inscribed diameter.
    pub size: P,
    /// The wall between neighbouring cells.
    pub web: P,
    /// How deep the cells go; not with `through_all`.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub depth: Option<P>,
    /// Through every target instead of a depth.
    #[serde(default, skip_serializing_if = "is_false")]
    pub through_all: bool,
    #[serde(default, skip_serializing_if = "is_negative")]
    pub direction: CellDirection,
    /// The grid's turn about the anchor; default 0.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub angle: Option<P>,
    /// A point a cell's centre sits on, projected onto the sketch plane;
    /// default the sketch's origin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<GeomRef>,
    /// What happens to the cells the field's edges cross.
    #[serde(default, skip_serializing_if = "is_whole")]
    pub clip: CellClip,
    /// The least clearance from a cell to the edges; default 0.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub margin: Option<P>,
    /// The bodies cut; none: every body the cells reach.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<BodyUid>,
}

fn is_hexagon(shape: &CellShape) -> bool {
    *shape == CellShape::Hexagon
}

fn is_staggered(rows: &CellRows) -> bool {
    *rows == CellRows::Staggered
}

fn is_negative(direction: &CellDirection) -> bool {
    *direction == CellDirection::Negative
}

fn is_whole(clip: &CellClip) -> bool {
    *clip == CellClip::Whole
}

impl<P> CellGridDef<P> {
    pub const TYPE: &'static str = "cell_grid";
    pub const BASE_NAME: &'static str = "CellGrid";

    map_params! {
        CellGridDef {
            profiles: clone,
            shape: copy,
            sides: option,
            rows: copy,
            size: param,
            web: param,
            depth: option,
            through_all: copy,
            direction: copy,
            angle: option,
            anchor: clone,
            clip: copy,
            margin: option,
            targets: clone,
        }
    }
}

/// The grid's values of a definition, checked: the spec with the anchor at
/// the sketch's origin, and the depth (None: through all).
pub(crate) fn values(
    def: &CellGridDef,
    value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
) -> Result<(GridSpec, Option<f64>), String> {
    let sides = match def.sides {
        Some(id) => {
            let n = value(id)?;
            if !n.is_finite()
                || (n - n.round()).abs() > 1e-9
                || !(3.0..=f64::from(MAX_SIDES)).contains(&n.round())
            {
                return Err(mitcad_model_base::internal::parameters::count_error(
                    format!("a polygon cell has 3 to {MAX_SIDES} sides, a whole number, got {n}"),
                    n,
                ));
            }
            n.round() as u32
        }
        None => DEFAULT_SIDES,
    };
    let spec = GridSpec {
        shape: def.shape,
        sides,
        rows: def.rows,
        size: value(def.size)?,
        web: value(def.web)?,
        angle: def.angle.map(&mut *value).transpose()?.unwrap_or(0.0),
        anchor: [0.0, 0.0],
        clip: def.clip,
        margin: def.margin.map(&mut *value).transpose()?.unwrap_or(0.0),
    };
    plan::check(&spec)?;
    let depth = match (def.through_all, def.depth) {
        (true, _) => None,
        (false, Some(id)) => {
            let depth = value(id)?;
            if !(depth.is_finite() && depth > 0.0) {
                return Err(format!(
                    "the cells' depth must be greater than zero, got {depth}"
                ));
            }
            Some(depth)
        }
        (false, None) => {
            return Err("a cell grid needs a depth, or through all".to_owned());
        }
    };
    Ok((spec, depth))
}

impl FeatureInfo for CellGridDef {
    fn references(&self) -> References {
        let mut references = References::default();
        profile_references(&self.profiles, &mut references);
        if let Some(anchor) = &self.anchor {
            anchor.add_to(&mut references);
        }
        for body in &self.targets {
            references.body(*body);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_profiles(ctx, &self.profiles)?;
        if let Some(anchor) = &self.anchor {
            anchor.check(ctx, Want::Point)?;
        }
        for (k, body) in self.targets.iter().enumerate() {
            if self.targets[..k].contains(body) {
                return Err(format!("body {body} is listed more than once"));
            }
            ctx.body(*body)?;
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        values(self, &mut |id| Ok(value(id))).map(|_| ())
    }

    fn profiles(&self) -> &[ProfileRef] {
        &self.profiles
    }
}

impl<K: Kernel> Evaluate<K> for CellGridDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let (mut spec, depth) = values(self, &mut |id| ctx.param(id))?;
        let (sketch, regions) = profile_regions(ctx, &self.profiles)?;
        let frame = sketch.frame;
        if let Some(anchor) = &self.anchor {
            spec.anchor = in_plane(&frame, ctx.point(anchor)?);
        }
        let loops: Vec<ProfileLoop> = regions.iter().flat_map(|r| r.loops.clone()).collect();
        let plan = plan(&loops, &spec)?;

        let participants = if self.targets.is_empty() {
            ctx.surface_bodies()
        } else {
            self.targets
                .iter()
                .map(|uid| ctx.surface_body(*uid).map(|shape| (*uid, shape)))
                .collect::<Result<Vec<_>, _>>()?
        };
        if participants.is_empty() {
            return Err("there is no body to cut the cells from".to_owned());
        }
        let normal = frame.normal();
        let direction = match self.direction {
            CellDirection::Negative => normal.map(|v| -v),
            CellDirection::Positive | CellDirection::Symmetric => normal,
        };
        let symmetric = self.direction == CellDirection::Symmetric;
        let depth = match depth {
            Some(depth) => depth,
            None => {
                let from = field_corners(&frame, &plan);
                super::extrude::through_all_length(
                    ctx.kernel,
                    &from,
                    direction,
                    &participants,
                    symmetric,
                )?
            }
        };
        let cells: Vec<ProfileRegion> = plan.cells.iter().map(cell_region).collect();
        let feature = ctx.uid;
        let sweep = Sweep {
            feature,
            frame,
            direction,
            depth,
            symmetric,
        };
        let mut tool = ctx
            .kernel
            .extrude_feature(&sweep.spec(&cells))
            .map_err(kernel_error)?;
        if plan.cells.iter().any(|c| c.piece == Piece::Trimmed) {
            // Trimmed to the field as drawn: the cells within its prism.
            let field = ctx
                .kernel
                .extrude_feature(&sweep.spec(&field_regions(&loops)?))
                .map_err(kernel_error)?;
            let trimmed = ctx
                .boolean(BooleanOp::Intersect, &[&field], &tool)
                .map_err(|e| e.to_string())?;
            let pieces: Vec<K::Shape> = trimmed.pieces.into_iter().map(|p| p.shape).collect();
            if pieces.is_empty() {
                return Err("the cells trimmed to the field leave nothing to cut".to_owned());
            }
            tool = super::finger_joint::one_tool(ctx, pieces)?;
        }
        let changes = apply_tool(ctx, BooleanOp::Cut, &participants, &tool, "cell grid")?;
        for body in &self.targets {
            let reached = changes.iter().any(|change| match change {
                BodyChange::Set(uid, _) | BodyChange::Remove(uid) => uid == body,
            });
            if !reached {
                ctx.warn(format!("the cells do not reach body {body}"));
            }
        }
        ctx.remark(summary(&plan), Vec::new());
        Ok(FeatureOutput {
            changes,
            tool: Some(tool),
            ..FeatureOutput::default()
        })
    }
}

/// How the cells are swept from the sketch plane.
struct Sweep {
    feature: crate::ids::FeatureUid,
    frame: SketchFrame,
    direction: [f64; 3],
    depth: f64,
    symmetric: bool,
}

impl Sweep {
    fn spec<'a, S>(&self, regions: &'a [ProfileRegion]) -> ExtrudeFeatureSpec<'a, S> {
        let side = || ExtrudeSide {
            end: SideEnd::Distance(self.depth),
            taper: 0.0,
            thin: None,
        };
        ExtrudeFeatureSpec {
            feature: self.feature,
            frame: self.frame,
            regions,
            direction: self.direction,
            start: ExtrudeStart::Offset(0.0),
            side1: side(),
            side2: self.symmetric.then(side),
        }
    }
}

/// What the panel shows of a plan: `61 cells, open area 3380.96 mm² (56.3%
/// of the field)`, with the snapped and trimmed cells counted apart.
pub fn summary(plan: &Plan) -> String {
    let count = |piece: Piece| plan.cells.iter().filter(|c| c.piece == piece).count();
    let whole = count(Piece::Whole);
    let mut parts = Vec::new();
    let snapped = plan.cells.len() - whole - count(Piece::Trimmed);
    if snapped > 0 {
        parts.push(format!("{snapped} snapped"));
    }
    let trimmed = count(Piece::Trimmed);
    if trimmed > 0 {
        parts.push(format!("{trimmed} trimmed"));
    }
    let detail = if parts.is_empty() {
        String::new()
    } else {
        format!(" ({whole} whole, {})", parts.join(", "))
    };
    format!(
        "{} cells{detail}, open area {:.2} mm² ({:.1}% of the field)",
        plan.cells.len(),
        plan.open_area,
        100.0 * plan.open_fraction()
    )
}

/// A point's coordinates in the sketch plane (projected onto it).
fn in_plane(frame: &SketchFrame, point: [f64; 3]) -> [f64; 2] {
    let d: [f64; 3] = std::array::from_fn(|i| point[i] - frame.origin[i]);
    [
        mitcad_vec3::dot(d, frame.x_axis),
        mitcad_vec3::dot(d, frame.y_axis),
    ]
}

/// The corners of the cells' box in the sketch plane, in 3D.
fn field_corners(frame: &SketchFrame, plan: &Plan) -> Vec<[f64; 3]> {
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for cell in &plan.cells {
        let (low, high) = match &cell.outline {
            Outline::Polygon { points, .. } => points.iter().fold(
                ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]),
                |(low, high), p| {
                    (
                        [low[0].min(p[0]), low[1].min(p[1])],
                        [high[0].max(p[0]), high[1].max(p[1])],
                    )
                },
            ),
            Outline::Circle { center, radius } => (
                [center[0] - radius, center[1] - radius],
                [center[0] + radius, center[1] + radius],
            ),
        };
        min = [min[0].min(low[0]), min[1].min(low[1])];
        max = [max[0].max(high[0]), max[1].max(high[1])];
    }
    [
        [min[0], min[1]],
        [max[0], min[1]],
        [max[0], max[1]],
        [min[0], max[1]],
    ]
    .map(|p| frame.point(p))
    .to_vec()
}

/// The curve side `side` of cell `index` is swept from (see the module).
pub fn cell_curve(index: [i64; 2], side: u32) -> EntityUid {
    let wrap = |v: i64| (v + 1024).rem_euclid(2048) as u32;
    EntityUid((1 << 30) + side + 128 * (2048 * wrap(index[0]) + wrap(index[1])))
}

/// A planned cell as a profile region whose segments carry its lattice
/// index.
pub fn cell_region(cell: &PlannedCell) -> ProfileRegion {
    let id = |side: u32| cell_curve(cell.index, side);
    let segments = match &cell.outline {
        Outline::Circle { center, radius } => vec![ProfileSegment {
            key: SegmentKey::closed(id(0)),
            geometry: SegmentGeometry::Circle {
                center: *center,
                radius: *radius,
                start: 0.0,
            },
        }],
        Outline::Polygon { points, sides } => {
            let n = points.len();
            (0..n)
                .map(|k| ProfileSegment {
                    key: SegmentKey::between(
                        id(sides[k]),
                        id(sides[(k + n - 1) % n]),
                        id(sides[(k + 1) % n]),
                    ),
                    geometry: SegmentGeometry::Line {
                        start: points[k],
                        end: points[(k + 1) % n],
                    },
                })
                .collect()
        }
    };
    ProfileRegion {
        key: RegionKey::new(segments.iter().map(|s| s.key.clone())).expect("a cell has sides"),
        loops: vec![ProfileLoop { segments }],
    }
}
