// SPDX-License-Identifier: MIT
//! The `cam_simulate` query (mitcad#1013, phase 3; docs/cam.md "Stock
//! simulation"): a setup's program, computed now, cut out of its stock on
//! a height map (`mitcad_cam::stock`) in work coordinates.
//!
//! - **The stock** is the setup's: its box, or the stock body's mesh seen
//!   from above (its highest surface over each node, on its lowest point).
//! - **The grid** spans the stock's box, an eighth of the smallest tool's
//!   diameter apart (or the `resolution` asked), coarser until it has at
//!   most `max_nodes` nodes.
//! - **Heights** go out as runs to the micrometre (`HeightMap::runs`):
//!   pairs of a height (µm) and a count of nodes, row by row from the
//!   grid's origin; a snapshot (`frames`) as its changes from the one
//!   before, a pair `0, -n` for `n` nodes unchanged.
//! - **The material left** is measured against the setup's bodies (each
//!   meshed and seen from above on the same grid): over them (the
//!   thickness left, its mean, the thickest places, the gouges) and beside
//!   them.

use serde_json::{Value, json};

use super::super::{Document, ModelError, invalid};
use super::SetupFrame;
use crate::cam::SetupRef;
use crate::cam::Stock;
use crate::cam::engine::TriMesh;
use crate::cam::engine::machine::Bounds;
use crate::cam::engine::stock::{
    self as sim, Grid, HeightMap, MAX_NODES, Place, Remaining, finest_step,
};
use crate::cam::simulate::{
    MAX_FRAMES, REMAINING_TOLERANCE, SimulateOptions, SimulatePlace, SimulateResult,
};
use crate::exchange::MeshTolerance;
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::transform::Transform;

impl<K: Kernel> Document<K> {
    /// The bodies' meshes, placed and in work coordinates, as one mesh.
    fn cam_work_mesh(
        &self,
        bodies: &[(BodyUid, Transform)],
        sf: &SetupFrame,
        deviation: f64,
    ) -> Result<TriMesh, String> {
        let tolerance = MeshTolerance {
            deviation,
            angle: 15.0_f64.to_radians(),
            face_triangles: 0,
            max_edge: 0.0,
        };
        let mut out = TriMesh::default();
        for (body, placement) in bodies {
            let Some(shape) = self.body_shape(*body) else {
                continue;
            };
            let mesh = self
                .kernel()
                .triangle_mesh(shape, &tolerance)
                .map_err(|_| {
                    "A body of the setup cannot be meshed for the simulation.".to_owned()
                })?;
            let base = out.vertices.len() as u32;
            out.vertices.extend(
                mesh.vertices
                    .iter()
                    .map(|p| sf.frame.to_work(placement.apply_point(*p))),
            );
            out.triangles
                .extend(mesh.triangles.iter().map(|t| t.map(|i| i + base)));
        }
        Ok(out)
    }

    /// The `cam_simulate` query (the module's documentation): `{setup,
    /// work, grid, until, rapid_cuts, removed, lowest}` with `initial`,
    /// `heights`, `frames` and `part` (runs) for `result: heights`, and
    /// `remaining` when the setup has bodies. Refused while an operation
    /// fails.
    pub fn cam_simulate_json(
        &self,
        setup: &SetupRef,
        options: &SimulateOptions,
    ) -> Result<Value, ModelError> {
        let index = self.cam_setup_index(setup)?;
        let setup = &self.state.cam.setups[index];
        let run = self.compute_cam_setup(index, None);
        let program = Self::cam_program(setup, &run).map_err(invalid)?;
        let sf = run.frame.as_ref().map_err(|e| invalid(e.clone()))?;
        // The program's operations are the computed ones, in order.
        let ids: Vec<u32> = run
            .operations
            .iter()
            .filter(|c| matches!(c.result, Some(Ok(_))))
            .map(|c| c.id)
            .collect();
        let place = |p: &SimulatePlace| -> Result<Place, ModelError> {
            let operation = ids.iter().position(|id| *id == p.operation).ok_or_else(|| {
                invalid("That operation is not in the setup's program: it is suppressed or in another setup.")
            })?;
            let moves = program.operations[operation].toolpath.moves.len();
            Ok(Place {
                operation,
                move_index: p.move_index.min(moves.saturating_sub(1)),
            })
        };
        let until = options.until.as_ref().map(place).transpose()?;
        if options.frames.len() > MAX_FRAMES {
            return Err(invalid(format!(
                "Ask for at most {MAX_FRAMES} snapshots of the stock."
            )));
        }
        let mut frames: Vec<Place> = options.frames.iter().map(place).collect::<Result<_, _>>()?;
        frames.sort();
        frames.dedup();
        let finest = options
            .resolution
            .filter(|r| r.is_finite() && *r > 0.0)
            .unwrap_or_else(|| finest_step(&program));
        let max_nodes = options.max_nodes.unwrap_or(MAX_NODES).max(4);
        let deviation = |grid: &Grid| (grid.step[0].min(grid.step[1]) / 4.0).clamp(0.001, 0.05);

        // The stock before the cut.
        let initial = match &setup.stock {
            Stock::AroundBodies { .. } => HeightMap::of_box(
                &Bounds {
                    min: sf.stock_min,
                    max: sf.stock_max,
                },
                finest,
                max_nodes,
            ),
            Stock::Body { body, occurrence } => {
                let grid = Grid::for_stock(
                    [sf.stock_min[0], sf.stock_min[1]],
                    [sf.stock_max[0], sf.stock_max[1]],
                    finest,
                    max_nodes,
                );
                let placed = self
                    .cam_body_placed(&crate::cam::CamBody {
                        body: *body,
                        occurrence: occurrence.clone(),
                    })
                    .map_err(invalid)?;
                let mesh = self
                    .cam_work_mesh(&[placed], sf, deviation(&grid))
                    .map_err(invalid)?;
                HeightMap::top_of_mesh(grid, sf.stock_min[2], &mesh)
            }
        };
        let grid = initial.grid;

        // The bodies machined, seen from above on the same grid.
        let bodies = self.cam_setup_bodies(setup).map_err(invalid)?;
        let part = if bodies.is_empty() {
            None
        } else {
            let mesh = self
                .cam_work_mesh(&bodies, sf, deviation(&grid))
                .map_err(invalid)?;
            Some(HeightMap::top_of_mesh(grid, initial.bottom, &mesh))
        };

        // The cut, with its snapshots.
        let heights = options.result == SimulateResult::Heights;
        let mut last = initial.clone();
        let mut snapshots: Vec<Value> = Vec::new();
        let mut next = 0usize;
        let simulated = sim::simulate(&program, initial.clone(), until, |at, map| {
            while heights && next < frames.len() && frames[next] <= at {
                let f = frames[next];
                snapshots.push(json!({
                    "operation": ids[f.operation],
                    "move": f.move_index,
                    "runs": map.runs_since(&last),
                }));
                last.z.copy_from_slice(&map.z);
                next += 1;
            }
        });
        let place_json = |p: Place| json!({"operation": ids[p.operation], "move": p.move_index});
        let rapid_cuts: Vec<Value> = simulated
            .rapid_cuts
            .iter()
            .map(|c| {
                json!({
                    "message": c.message,
                    "operation": ids[c.operation],
                    "move": c.move_index,
                    "leg": c.leg,
                    "depth": c.depth,
                    "at": c.at,
                })
            })
            .collect();
        let mut out = json!({
            "setup": setup.id,
            "work": Self::cam_frame_json(&run.frame),
            "grid": {
                "origin": grid.origin,
                "step": grid.step,
                "nx": grid.nx,
                "ny": grid.ny,
                "bottom": initial.bottom,
                "top": initial.top(),
            },
            "until": simulated.last.map(place_json),
            "rapid_cuts": rapid_cuts,
            "removed": simulated.removed,
            "lowest": simulated.stock.lowest(),
        });
        if heights {
            out["initial"] = json!(initial.runs());
            out["heights"] = json!(simulated.stock.runs());
            out["frames"] = json!(snapshots);
        }
        if let Some(part) = &part {
            if heights {
                out["part"] = json!(part.runs());
            }
            let tolerance = options
                .tolerance
                .filter(|t| t.is_finite() && *t >= 0.0)
                .unwrap_or(REMAINING_TOLERANCE);
            let spacing = program
                .operations
                .iter()
                .map(|op| op.tool.diameter)
                .fold(f64::INFINITY, f64::min);
            let spacing = if spacing.is_finite() { spacing } else { 1.0 };
            let left = sim::remaining(&simulated.stock, part, tolerance, spacing);
            out["remaining"] = Self::cam_remaining_json(&left, sf);
        }
        Ok(out)
    }

    /// The material left as JSON, each place in work coordinates (`at`)
    /// and in the model (`model`).
    fn cam_remaining_json(left: &Remaining, sf: &SetupFrame) -> Value {
        let places = |found: &[sim::Thickness]| -> Vec<Value> {
            found
                .iter()
                .map(|t| {
                    json!({
                        "at": t.at,
                        "model": sf.frame.to_model(t.at),
                        "thickness": t.thickness,
                    })
                })
                .collect()
        };
        json!({
            "tolerance": left.tolerance,
            "max": left.max,
            "mean": left.mean,
            "volume": left.volume,
            "area": left.area,
            "thickest": places(&left.thickest),
            "gouge": left.gouge,
            "gouges": places(&left.gouges),
            "beside": left.beside,
            "beside_volume": left.beside_volume,
            "part_nodes": left.part_nodes,
            "beside_nodes": left.beside_nodes,
        })
    }
}
