// SPDX-License-Identifier: MIT
//! The `.dwg` import (mitcad#453): `mitcad_dwg` reads the drawing and
//! plans its components (`mitcad_dwg::scene`); the B-rep records of its
//! solids are converted by the `.f3d` reader's ASM code and built by OCCT
//! (`build_brep_body`), its meshes become mesh bodies. Bodies only: a
//! drawing keeps no feature history.
//!
//! - Each named block with 3D content is a component, placed by an
//!   occurrence for each rigid insert of it; nested blocks keep their
//!   nesting. Anonymous blocks and blocks inserted with a scale or a
//!   mirror are copied into the component that inserts them.
//! - The bodies of a component on one layer are one base feature named
//!   after the layer (the layer is the feature's source); the entities'
//!   colours (their own, their layer's, or the inserting block's) are the
//!   bodies' colours.
//! - The drawing's units become the document's (an empty document only),
//!   the bodies are millimetres either way.
//!
//! The report accounts for every entity: taken (bodies, meshes, inserts)
//! or left out with its type, count and why. The `import_dwg` document
//! command and `mitcad-cli import-dwg`.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, json};

use mitcad_dwg::DwgFile;
use mitcad_dwg::scene::{Affine, ItemKind, Plan, Scene};
use mitcad_dwg::solid;
use mitcad_import::outcomes::EntityOutcome;
use mitcad_model::notice::Notice;
use mitcad_model::{BaseInput, ComponentUid, ImportBody, Kernel, Transform};
use mitcad_model_api::ApiError;

use crate::Document;
use crate::brep_import::{Source, to_ffi};
use crate::kernel::exchange::ffi::{build_brep_body, catch_occt_crashes};
use crate::kernel::shape::ffi::new_shape_list;
use crate::kernel::{OcctKernel, Shape};

use cxx::SharedPtr;

/// The options of `import_dwg` (JSON).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    /// A readable report instead of JSON.
    #[serde(default)]
    text: bool,
    /// Also write the JSON report to this file.
    #[serde(default)]
    report_path: Option<String>,
    /// Values the drawing is checked against (a product's data sheet).
    #[serde(default)]
    expect: Expect,
}

/// The data sheet values of a product the import is checked against
/// (`checks` in the report); each is optional.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Expect {
    /// The overall length: the largest side of the bodies' box.
    length_mm: Option<f64>,
    /// The chord tubes' centre distance: with the tube diameter, the
    /// second largest side of the box.
    chord_distance_mm: Option<f64>,
    /// A tube's outer diameter: a cylindrical face of that diameter.
    tube_diameter_mm: Option<f64>,
    /// Its wall: a cylindrical face of the inner diameter too.
    tube_wall_mm: Option<f64>,
    /// The mass: the solids' volume times the density.
    mass_kg: Option<f64>,
    /// kg/m³; aluminium (2700) when not given.
    density_kg_m3: Option<f64>,
    /// How many solids and components.
    solids: Option<usize>,
    components: Option<usize>,
    /// The solids' volume in all their placements (mm³; within 1 %), e.g.
    /// of a reference model of the same product.
    volume_mm3: Option<f64>,
}

/// One check of the report.
fn check(name: &str, expected: Value, found: Value, pass: bool) -> Value {
    json!({"check": name, "expected": expected, "found": found, "pass": pass})
}

/// Within `abs` mm or `rel` of the expected value.
fn near(found: f64, expected: f64, abs: f64, rel: f64) -> bool {
    (found - expected).abs() <= abs.max(rel * expected.abs())
}

fn file_name(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
}

/// A built body of a block's item.
struct Built {
    shape: SharedPtr<Shape>,
    solid: bool,
    mesh: bool,
    valid: bool,
    volume: f64,
    /// The entity type, for the body's name.
    kind: &'static str,
}

fn kind_label(entity: &str) -> &'static str {
    match entity {
        "3DSOLID" => "Solid",
        "BODY" => "Body",
        "REGION" => "Region",
        e if e.ends_with("SURFACE") => "Surface",
        _ => "Mesh",
    }
}

fn to_transform(a: &Affine, unit_mm: f64) -> Transform {
    Transform {
        linear: a.m,
        translation: [a.t[0] * unit_mm, a.t[1] * unit_mm, a.t[2] * unit_mm],
    }
}

/// The signed volume a closed mesh bounds (mm³).
fn mesh_volume(vertices: &[[f64; 3]], triangles: &[[u32; 3]]) -> f64 {
    triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| vertices[i as usize]);
            (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0
        })
        .sum()
}

/// The world placements of every node (the root's is the identity),
/// millimetres.
fn world_placements(plan: &Plan) -> Vec<Vec<Transform>> {
    let mut out = vec![Vec::new(); plan.nodes.len()];
    fn walk(plan: &Plan, node: usize, at: Transform, out: &mut Vec<Vec<Transform>>, depth: usize) {
        if depth > 64 {
            return;
        }
        out[node].push(at);
        for (child, t) in &plan.nodes[node].children {
            let t = Transform {
                linear: t.m,
                translation: t.t,
            };
            walk(plan, *child, at.after(&t), out, depth + 1);
        }
    }
    walk(plan, 0, Transform::IDENTITY, &mut out, 0);
    out
}

impl Document {
    /// `pause`: the job's monitor, whose test delay each item of the
    /// drawing waits (`RecomputeMonitor::test_pause`; the import itself
    /// runs without the job).
    fn import_dwg_report(
        &mut self,
        path: &str,
        options: &Options,
        pause: Option<&mitcad_model::RecomputeMonitor>,
    ) -> Result<Value, ApiError> {
        let file = file_name(path);
        let fail = |e: &dyn std::fmt::Display| ApiError(format!("{file}: {e}"));
        let started = std::time::Instant::now();
        let dwg = DwgFile::open(std::path::Path::new(path)).map_err(|e| fail(&e))?;
        let scene = Scene::of(&dwg);
        let plan = scene.plan();
        let read_seconds = started.elapsed().as_secs_f64();
        catch_occt_crashes();
        let mut warnings = scene.account.warnings.clone();
        warnings.extend(plan.warnings.iter().cloned());

        // Each block item once (a component's occurrences share it):
        // the bodies its B-rep record holds, or its mesh.
        let mut built: BTreeMap<(u64, usize), Vec<Built>> = BTreeMap::new();
        let mut not_built = Vec::new();
        let mut records = Vec::new();
        // Cylindrical faces by diameter (in 0.01 mm).
        let mut diameters: BTreeMap<i64, usize> = BTreeMap::new();
        let convert = mitcad_asm::convert::Options {
            scale: scene.unit_mm,
            ..Default::default()
        };
        let wanted: std::collections::BTreeSet<(u64, usize)> = plan
            .nodes
            .iter()
            .flat_map(|n| n.bodies.iter().map(|b| (b.block, b.item)))
            .collect();
        for &(block, index) in &wanted {
            if let Some(monitor) = pause {
                monitor.test_pause();
            }
            let item = &scene.blocks[&block].items[index];
            let place = format!("{} {:X}", entity_name(&item.kind), item.handle);
            let mut out = Vec::new();
            match &item.kind {
                ItemKind::Brep {
                    form,
                    bytes,
                    entity,
                } => {
                    let asm = match solid::parse(*form, bytes) {
                        Ok(a) => a,
                        Err(e) => {
                            not_built.push(
                                json!({"source": place, "error": format!("its B-rep record: {e}")}),
                            );
                            continue;
                        }
                    };
                    records.push(json!({
                        "source": place, "form": form.name(),
                        "asm_version": asm.header.asm_version, "version": asm.header.version,
                        "records": asm.records.len(),
                        "truncated": asm.truncated.as_ref().map(|t| t.to_string()),
                    }));
                    let source = Source {
                        document: "",
                        blob: &place,
                        asm_version: &asm.header.asm_version,
                        history: false,
                        history_step: 0,
                    };
                    let mut list = new_shape_list();
                    for body in mitcad_asm::convert::convert_file(&asm, &convert)
                        .iter()
                        .filter(|b| b.top_level && !b.body.faces.is_empty())
                    {
                        // The diameters of its cylindrical faces (tubes).
                        for f in &body.body.faces {
                            if let Some(mitcad_asm::brep::Surface::Cone {
                                radius, half_angle, ..
                            }) = body.body.surfaces.get(f.surface)
                                && half_angle.abs() < 1e-9
                            {
                                *diameters
                                    .entry((radius * 200.0).round() as i64)
                                    .or_insert(0usize) += 1;
                            }
                        }
                        let data = to_ffi(&source, body);
                        let r = build_brep_body(&data, list.pin_mut());
                        // Faces whose rolling-ball blend is the file's
                        // approximation, too far off it (mitcad#622).
                        for blend in &body.stored_blends {
                            if let Some(why) = blend.reason(&r.messages) {
                                warnings.push(format!(
                                    "{place}/{}: {}",
                                    body.record,
                                    blend.note(&why)
                                ));
                            }
                        }
                        // A body none of whose faces could be built is
                        // not built: an empty body ended the whole import
                        // (mitcad#589).
                        if !r.built || r.faces == 0 {
                            let error = if r.built {
                                "none of its faces could be built".to_owned()
                            } else {
                                r.error.to_string()
                            };
                            not_built.push(json!({"source": format!("{place}/{}", body.record), "error": error}));
                            continue;
                        }
                        out.push(Built {
                            shape: list.at(list.size() - 1),
                            solid: r.solid,
                            mesh: false,
                            valid: r.valid,
                            volume: r.volume,
                            kind: kind_label(entity),
                        });
                    }
                    if out.is_empty()
                        && !not_built
                            .iter()
                            .any(|n| n["source"].as_str().is_some_and(|s| s.starts_with(&place)))
                    {
                        not_built.push(json!({"source": place, "error": "its B-rep record holds no body with faces"}));
                    }
                }
                ItemKind::Mesh {
                    vertices,
                    triangles,
                    ..
                } => {
                    let mm: Vec<[f64; 3]> = vertices
                        .iter()
                        .map(|p| {
                            [
                                p[0] * scene.unit_mm,
                                p[1] * scene.unit_mm,
                                p[2] * scene.unit_mm,
                            ]
                        })
                        .collect();
                    match crate::kernel::mesh::ffi::mesh_from_triangles(
                        mm.as_flattened(),
                        triangles.as_flattened(),
                    ) {
                        Ok(shape) => out.push(Built {
                            shape,
                            solid: false,
                            mesh: true,
                            valid: true,
                            volume: mesh_volume(&mm, triangles).abs(),
                            kind: "Mesh",
                        }),
                        Err(e) => not_built.push(json!({"source": place, "error": e.to_string()})),
                    }
                }
                ItemKind::Insert { .. } => {}
            }
            built.insert((block, index), out);
        }
        let build_seconds = started.elapsed().as_secs_f64() - read_seconds;

        // The components, their occurrences and their bodies: one undo
        // step.
        let empty = self.0.features().next().is_none();
        let depth = self.0.undo_depth();
        let label = format!("Import {file}");
        let mut units = None;
        let symbol = match scene.units {
            mitcad_dwg::Units::Millimeters => Some("mm"),
            mitcad_dwg::Units::Centimeters => Some("cm"),
            mitcad_dwg::Units::Meters => Some("m"),
            mitcad_dwg::Units::Inches => Some("in"),
            mitcad_dwg::Units::Feet => Some("ft"),
            _ => None,
        };
        if let Some(sym) = symbol
            && let Some((length, 1)) = mitcad_model::expr::Unit::parse(sym)
                .ok()
                .and_then(|u| u.length())
        {
            if empty {
                self.0.set_units(length)?;
                units = Some(sym);
            } else {
                warnings.push(format!(
                    "the drawing's length unit ({sym}) is not applied to a document that has features"
                ));
            }
        }
        let mut components: Vec<Option<ComponentUid>> = vec![None; plan.nodes.len()];
        components[0] = Some(ComponentUid::ROOT);
        let mut occurrence_count = vec![0usize; plan.nodes.len()];
        occurrence_count[0] = 1;
        for (i, node) in plan.nodes.iter().enumerate() {
            let Some(parent) = components[i] else {
                continue;
            };
            for (child, t) in &node.children {
                let transform = Transform {
                    linear: t.m,
                    translation: t.t,
                };
                match components[*child] {
                    Some(c) => {
                        self.0.add_occurrence(c, parent, transform)?;
                    }
                    None => {
                        let (c, _) = self.0.add_component(
                            Some(&plan.nodes[*child].name),
                            parent,
                            transform,
                        )?;
                        components[*child] = Some(c);
                    }
                }
                occurrence_count[*child] += 1;
            }
        }
        let mut solids = 0;
        let mut meshes = 0;
        let mut valid = 0;
        let mut bodies_total = 0;
        let mut volume_per_node = vec![0.0; plan.nodes.len()];
        let mut shapes_per_node: Vec<Vec<SharedPtr<Shape>>> = vec![Vec::new(); plan.nodes.len()];
        let mut inputs = Vec::new();
        let mut feature_names: std::collections::BTreeSet<String> =
            self.0.features().map(|f| f.name.clone()).collect();
        for (i, node) in plan.nodes.iter().enumerate() {
            let Some(component) = components[i] else {
                continue;
            };
            // By layer, in the order the layers come.
            let mut by_layer: BTreeMap<String, Vec<ImportBody<SharedPtr<Shape>>>> = BTreeMap::new();
            let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
            let total: usize = node
                .bodies
                .iter()
                .map(|b| built.get(&(b.block, b.item)).map_or(0, Vec::len))
                .sum();
            for b in &node.bodies {
                let Some(list) = built.get(&(b.block, b.item)) else {
                    continue;
                };
                for body in list {
                    let shape = if b.transform.is_identity() {
                        body.shape.clone()
                    } else {
                        match OcctKernel.transform_shape(
                            &body.shape,
                            &to_transform(&b.transform, scene.unit_mm),
                            None,
                        ) {
                            Ok(s) => s,
                            Err(e) => {
                                not_built.push(json!({"source": format!("a copy in {}", node_name(&plan, i)), "error": e.to_string()}));
                                continue;
                            }
                        }
                    };
                    let n = counts.entry(body.kind).or_insert(0);
                    *n += 1;
                    let name = if total == 1 && i > 0 {
                        node.name.clone()
                    } else {
                        format!("{} {}", body.kind, n)
                    };
                    bodies_total += 1;
                    if body.solid {
                        solids += 1;
                    }
                    if body.mesh {
                        meshes += 1;
                    }
                    if body.valid {
                        valid += 1;
                    }
                    if body.solid {
                        // A copy's own volume (an insert may scale it).
                        volume_per_node[i] += if b.transform.is_identity() {
                            body.volume
                        } else {
                            OcctKernel
                                .mass_properties(&shape)
                                .map_or(body.volume, |m| m.volume)
                        };
                    }
                    shapes_per_node[i].push(shape.clone());
                    by_layer
                        .entry(scene.layer_name(b.layer).to_owned())
                        .or_default()
                        .push(ImportBody {
                            name: Some(name),
                            color: b.color.map(|c| c.map(|v| f64::from(v) / 255.0)),
                            shape,
                        });
                }
            }
            for (layer, bodies) in by_layer {
                // A name of its own for each layer's feature (feature names
                // may repeat, mitcad#1107): a component's carry its name.
                let base = if i == 0 {
                    format!("Layer {layer}")
                } else {
                    format!("{} layer {layer}", node.name)
                };
                let name = std::iter::once(base.clone())
                    .chain((2..).map(|n| format!("{base} ({n})")))
                    .find(|n| !feature_names.contains(n))
                    .expect("an unused name exists");
                feature_names.insert(name.clone());
                inputs.push(BaseInput {
                    name: Some(name),
                    source: Some(format!("{file}, layer {layer}")),
                    component: Some(component),
                    ..BaseInput::new(bodies)
                });
            }
        }
        if !inputs.is_empty() {
            self.0.add_base_features(&label, inputs)?;
        }
        self.0.merge_undo(depth, &label);

        // The overall box of every placed body, and the solids' volume.
        let placements = world_placements(&plan);
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        let mut volume = 0.0;
        for (i, shapes) in shapes_per_node.iter().enumerate() {
            volume += volume_per_node[i] * placements[i].len() as f64;
            for shape in shapes {
                let Ok(Some(b)) = OcctKernel.bounding_box(shape) else {
                    continue;
                };
                for at in &placements[i] {
                    for c in b.corners() {
                        let p = at.apply_point(c);
                        for k in 0..3 {
                            lo[k] = lo[k].min(p[k]);
                            hi[k] = hi[k].max(p[k]);
                        }
                    }
                }
            }
        }
        let bounds = lo[0].is_finite().then(|| json!({"min": lo, "max": hi}));
        let extents = scene.extents.map(|(a, b)| {
            let mm = |p: [f64; 3]| p.map(|v| v * scene.unit_mm);
            json!({"min": mm(a), "max": mm(b)})
        });
        let a = &scene.account;
        let left_out: Vec<Value> = a
            .left_out
            .iter()
            .map(|(t, (n, why))| json!({"type": t, "count": n, "why": why}))
            .collect();
        let component_values: Vec<Value> = plan
            .nodes
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, n)| {
                json!({
                    "name": n.name, "bodies": shapes_per_node[i].len(),
                    "occurrences": occurrence_count[i], "placed": placements[i].len(),
                    "component": components[i].map(|c| c.to_string()),
                })
            })
            .collect();

        // The checks: everything built and valid, within the drawing's
        // extents; the data sheet's values where given.
        let mut checks = vec![
            check(
                "every body built",
                json!(0),
                json!(not_built.len()),
                not_built.is_empty(),
            ),
            check(
                "every body valid",
                json!(bodies_total),
                json!(valid),
                valid == bodies_total,
            ),
        ];
        // The 3D entities left out (2D entities are left out by design).
        let missed: usize = scene
            .account
            .left_out
            .iter()
            .filter(|(t, _)| three_d(t))
            .map(|(_, (n, _))| n)
            .sum();
        checks.push(check(
            "every 3D entity taken",
            json!(0),
            json!(missed),
            missed == 0,
        ));
        let entities_3d: usize = scene
            .account
            .entities
            .iter()
            .filter(|(t, _)| three_d(t))
            .map(|(_, n)| n)
            .sum();
        if entities_3d > 0 {
            checks.push(check(
                "bodies imported",
                json!("> 0"),
                json!(bodies_total),
                bodies_total > 0,
            ));
        }
        let size = lo[0]
            .is_finite()
            .then(|| [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]);
        if let (Some((a, b)), Some(size)) = (scene.extents, size) {
            let margin = |k: usize| 1.0 + 0.005 * size[k];
            let inside = (0..3).all(|k| {
                lo[k] >= a[k] * scene.unit_mm - margin(k)
                    && hi[k] <= b[k] * scene.unit_mm + margin(k)
            });
            let tight = (0..3).all(|k| {
                (lo[k] - a[k] * scene.unit_mm).abs() <= margin(k)
                    && (hi[k] - b[k] * scene.unit_mm).abs() <= margin(k)
            });
            checks.push(check(
                "bounds within the drawing's extents",
                json!({"min": a.map(|v| v * scene.unit_mm), "max": b.map(|v| v * scene.unit_mm)}),
                json!({"min": lo, "max": hi, "equal": tight}),
                inside,
            ));
        }
        let e = &options.expect;
        // The box's sides: the length is the side nearest to it (a short
        // segment is shorter than it is wide), the chord distance with the
        // tube diameter the nearest of the others.
        let mut sides: Vec<f64> = size.map_or_else(Vec::new, |s| s.to_vec());
        let mut take_nearest = |value: f64| -> Option<f64> {
            let i = (0..sides.len()).min_by(|&a, &b| {
                (sides[a] - value)
                    .abs()
                    .total_cmp(&(sides[b] - value).abs())
            })?;
            Some(sides.remove(i))
        };
        if let Some(length) = e.length_mm {
            let found = take_nearest(length);
            checks.push(check(
                "length (a side of the bodies' box)",
                json!(length),
                json!(found),
                found.is_some_and(|f| near(f, length, 1.0, 0.005)),
            ));
        }
        if let (Some(chord), Some(d)) = (e.chord_distance_mm, e.tube_diameter_mm) {
            let found = take_nearest(chord + d);
            checks.push(check(
                "chord distance and tube diameter (another side of the box)",
                json!(chord + d),
                json!(found),
                found.is_some_and(|f| near(f, chord + d, 1.0, 0.01)),
            ));
        }
        let has_diameter = |d: f64| {
            diameters
                .keys()
                .any(|&k| (k as f64 / 100.0 - d).abs() <= 0.1)
        };
        if let Some(d) = e.tube_diameter_mm {
            checks.push(check(
                "a cylindrical face of the tube's diameter",
                json!(d),
                json!(has_diameter(d)),
                has_diameter(d),
            ));
            if let Some(wall) = e.tube_wall_mm {
                let inner = d - 2.0 * wall;
                checks.push(check(
                    "a cylindrical face of the tube's inner diameter",
                    json!(inner),
                    json!(has_diameter(inner)),
                    has_diameter(inner),
                ));
            }
        }
        if let Some(mass) = e.mass_kg {
            let density = e.density_kg_m3.unwrap_or(2700.0);
            let found = volume * 1e-9 * density;
            checks.push(check(
                "mass (the solids' volume times the density)",
                json!(mass),
                json!(found),
                near(found, mass, 0.0, 0.05),
            ));
        }
        if let Some(v) = e.volume_mm3 {
            checks.push(check(
                "the solids' volume",
                json!(v),
                json!(volume),
                near(volume, v, 0.0, 0.01),
            ));
        }
        if let Some(n) = e.solids {
            checks.push(check("solids", json!(n), json!(solids), n == solids));
        }
        if let Some(n) = e.components {
            let made = plan.nodes.len() - 1;
            checks.push(check("components", json!(n), json!(made), n == made));
        }
        let pass = checks.iter().all(|c| c["pass"] == true);
        let diameter_list: Vec<Value> = diameters
            .iter()
            .map(|(d, n)| json!({"diameter": *d as f64 / 100.0, "faces": n}))
            .collect();
        let mut report = json!({
            "checks": checks,
            "pass": pass,
            "cylinder_diameters": diameter_list,
            "file": file,
            "format": "dwg",
            "version": dwg.version.to_string(),
            "units": units,
            "drawing_units": format!("{:?}", scene.units),
            "unit_mm": scene.unit_mm,
            "units_assumed": scene.units_assumed,
            "extents": extents,
            "bounds": bounds,
            "entities": a.entities,
            "taken": a.taken,
            "left_out": left_out,
            "paper_space": a.paper_space,
            "objects": dwg.objects.len(),
            "objects_failed": dwg.failed.len(),
            "components": component_values,
            "copied_inserts": plan.flattened,
            "bodies": bodies_total,
            "solids": solids,
            "meshes": meshes,
            "valid": valid,
            "solid_volume": volume,
            "not_built": not_built,
            "records": records,
            "warnings": warnings,
            "seconds": {"read": read_seconds, "build": build_seconds},
        });
        // The levels of its entities' outcomes and of its warnings
        // (mitcad#1051).
        report["notices"] = mitcad_model::notice::to_json(&notices(&report));
        if let Some(path) = &options.report_path {
            let text = serde_json::to_string_pretty(&report).expect("the report serializes");
            std::fs::write(path, format!("{text}\n"))
                .map_err(|e| ApiError(format!("{path}: {e}")))?;
        }
        Ok(report)
    }

    /// The bridge's `import_dwg`: the report as JSON, or text.
    pub(crate) fn import_dwg(&mut self, path: &str, json: &str) -> Result<String, ApiError> {
        let json = if json.trim().is_empty() { "{}" } else { json };
        let options: Options = serde_json::from_str(json)
            .map_err(|e| ApiError(format!("invalid import options: {e}")))?;
        let monitor = self.0.monitor().cloned();
        let report = self.without_job(|document| {
            document.import_dwg_report(path, &options, monitor.as_deref())
        })?;
        Ok(if options.text {
            summary(&report)
        } else {
            serde_json::to_string_pretty(&report).expect("the report serializes")
        })
    }

    /// `{"cmd": "import_dwg", "path": ..., ...}`.
    pub(crate) fn import_dwg_command(&mut self, mut command: Value) -> Result<Value, ApiError> {
        let path = command
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| ApiError("import_dwg needs a \"path\"".to_owned()))?
            .to_owned();
        if let Some(map) = command.as_object_mut() {
            map.remove("cmd");
            map.remove("path");
        }
        let options: Options = serde_json::from_value(command)
            .map_err(|e| ApiError(format!("invalid import options: {e}")))?;
        let monitor = self.0.monitor().cloned();
        let report = self.without_job(|document| {
            document.import_dwg_report(&path, &options, monitor.as_deref())
        })?;
        Ok(json!({
            "file": report["file"],
            "bodies": report["bodies"],
            "components": report["components"].as_array().map_or(0, Vec::len),
            // Its outcomes' and warnings' levels (mitcad#1051).
            "notices": report["notices"],
            "report": report,
        }))
    }
}

/// Whether an entity type is 3D content, which the import takes (2D
/// entities are left out by design).
fn three_d(entity: &str) -> bool {
    matches!(entity, "3DSOLID" | "BODY" | "REGION" | "3DFACE" | "MESH")
        || entity.ends_with("SURFACE")
        || entity.ends_with("mesh)")
}

/// The import's notices from its report (mitcad#1051): its entities'
/// outcomes ([`EntityOutcome`]) at their levels, with their counts, the
/// worst first, then its warnings.
fn notices(report: &Value) -> Vec<Notice> {
    let n = |v: &Value| usize::try_from(v.as_u64().unwrap_or(0)).unwrap_or(usize::MAX);
    let mut counts: BTreeMap<EntityOutcome, usize> = BTreeMap::new();
    let taken = report["taken"].as_object();
    counts.insert(
        EntityOutcome::Taken,
        taken.map_or(0, |t| t.values().map(n).sum()),
    );
    for left in report["left_out"].as_array().into_iter().flatten() {
        let outcome = if three_d(left["type"].as_str().unwrap_or_default()) {
            EntityOutcome::LeftOut
        } else {
            EntityOutcome::Drawing
        };
        *counts.entry(outcome).or_default() += n(&left["count"]);
    }
    counts.insert(
        EntityOutcome::NotValid,
        n(&report["bodies"]).saturating_sub(n(&report["valid"])),
    );
    counts.insert(
        EntityOutcome::NotBuilt,
        report["not_built"].as_array().map_or(0, Vec::len),
    );
    let warnings: Vec<String> = report["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    EntityOutcome::notices(counts, &warnings)
}

fn entity_name(kind: &ItemKind) -> &str {
    match kind {
        ItemKind::Brep { entity, .. }
        | ItemKind::Mesh { entity, .. }
        | ItemKind::Insert { entity, .. } => entity,
    }
}

fn node_name(plan: &Plan, i: usize) -> String {
    if i == 0 {
        "the model space".into()
    } else {
        plan.nodes[i].name.clone()
    }
}

/// The import report as text.
fn summary(report: &Value) -> String {
    let n = |v: &Value| v.as_u64().unwrap_or(0);
    let mut text = format!(
        "Imported {} ({} {}): {} bodies ({} solids, {} meshes; {} valid), {} components\n",
        report["file"].as_str().unwrap_or_default(),
        report["format"].as_str().unwrap_or_default(),
        report["version"].as_str().unwrap_or_default(),
        n(&report["bodies"]),
        n(&report["solids"]),
        n(&report["meshes"]),
        n(&report["valid"]),
        report["components"].as_array().map_or(0, Vec::len),
    );
    text.push_str(&format!(
        "Units: {} ({} mm per drawing unit{})\n",
        report["drawing_units"].as_str().unwrap_or_default(),
        report["unit_mm"],
        if report["units_assumed"] == true {
            ", assumed"
        } else {
            ""
        }
    ));
    for c in report["components"].as_array().into_iter().flatten() {
        text.push_str(&format!(
            "Component {}: {} bodies, {} occurrences ({} placements)\n",
            c["name"].as_str().unwrap_or_default(),
            n(&c["bodies"]),
            n(&c["occurrences"]),
            n(&c["placed"])
        ));
    }
    let taken: Vec<String> = report["taken"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, v)| format!("{k} {v}"))
        .collect();
    text.push_str(&format!("Entities taken: {}\n", taken.join(", ")));
    for l in report["left_out"].as_array().into_iter().flatten() {
        text.push_str(&format!(
            "Left out: {} {} ({})\n",
            n(&l["count"]),
            l["type"].as_str().unwrap_or_default(),
            l["why"].as_str().unwrap_or_default()
        ));
    }
    if n(&report["copied_inserts"]) > 0 {
        text.push_str(&format!(
            "Inserts copied in place: {}\n",
            n(&report["copied_inserts"])
        ));
    }
    if let (Some(b), Some(e)) = (report["bounds"].as_object(), report["extents"].as_object()) {
        text.push_str(&format!(
            "Bounds {} {} (the drawing's extents {} {})\n",
            b["min"], b["max"], e["min"], e["max"]
        ));
    }
    text.push_str(&format!(
        "Volume of the solids: {:.1} mm3\n",
        report["solid_volume"].as_f64().unwrap_or(0.0)
    ));
    for b in report["not_built"].as_array().into_iter().flatten() {
        text.push_str(&format!(
            "Not built: {}: {}\n",
            b["source"].as_str().unwrap_or_default(),
            b["error"].as_str().unwrap_or_default()
        ));
    }
    for w in report["warnings"].as_array().into_iter().flatten() {
        text.push_str(&format!("Warning: {}\n", w.as_str().unwrap_or_default()));
    }
    let checks = report["checks"].as_array().map_or(&[][..], Vec::as_slice);
    text.push_str(&format!(
        "Checks: {} of {} pass\n",
        checks.iter().filter(|c| c["pass"] == true).count(),
        checks.len()
    ));
    for c in checks {
        text.push_str(&format!(
            "Check {}: {} (expected {}, found {})\n",
            c["check"].as_str().unwrap_or_default(),
            if c["pass"] == true { "pass" } else { "FAIL" },
            c["expected"],
            c["found"]
        ));
    }
    text
}

/// The text form of an `import_dwg` report given as JSON (`mitcad-cli
/// import-dwg`, which reads the checks from the JSON).
pub(crate) fn dwg_report_text(json: &str) -> String {
    serde_json::from_str::<Value>(json)
        .map_or_else(|e| format!("not a report: {e}\n"), |r| summary(&r))
}

/// Whether an `import_dwg` report given as JSON passes all its checks.
pub(crate) fn dwg_report_passes(json: &str) -> bool {
    serde_json::from_str::<Value>(json).is_ok_and(|r| r["pass"] == true)
}

/// Writes the reader's test drawing in a version (`mitcad-cli import-dwg
/// --write-test`): R13, R14, R2000, R2004, R2007, R2010, R2013 or R2018;
/// `test_case` (empty for none) adds a case of an import test
/// (`mitcad_dwg::testfile::test_case`).
pub(crate) fn dwg_write_test_file(
    path: &str,
    version: &str,
    test_case: &str,
) -> Result<(), ApiError> {
    let v = mitcad_dwg::Version::ALL
        .into_iter()
        .find(|v| v.name().eq_ignore_ascii_case(version))
        .ok_or_else(|| ApiError(format!("unknown drawing version {version}")))?;
    let drawing = mitcad_dwg::testfile::test_case(test_case)
        .ok_or_else(|| ApiError(format!("unknown test case {test_case}")))?;
    let bytes = mitcad_dwg::testfile::write(&drawing, v);
    std::fs::write(path, bytes).map_err(|e| ApiError(format!("{path}: {e}")))
}
