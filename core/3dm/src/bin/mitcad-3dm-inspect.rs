// SPDX-License-Identifier: MIT
//! `mitcad-3dm-inspect <file.3dm>`: the file's summary as JSON (its
//! version, units, layers, objects by kind, B-reps) and, with `--convert`,
//! each B-rep's and extrusion's conversion to the neutral model with its
//! check (no OCCT); with `--faces`, each B-rep's faces as the file keeps
//! them: surface, orientation, loops and trims (mitcad#1738).

use mitcad_3dm::{File3dm, convert, geom, summary};
use serde_json::json;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.iter().find(|a| !a.starts_with("--")) else {
        eprintln!("usage: mitcad-3dm-inspect [--convert] [--faces] <file.3dm>");
        std::process::exit(2);
    };
    let file = match File3dm::open(std::path::Path::new(path)) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{path}: {e}");
            std::process::exit(1);
        }
    };
    let mut out = summary(&file);
    if args.iter().any(|a| a == "--convert") {
        let s = file.units.mm;
        let mut bodies = Vec::new();
        for (i, o) in file.objects.iter().enumerate() {
            let converted = match &o.geometry {
                geom::Geometry::Brep(b) => Ok(convert::brep_body(b, s)),
                geom::Geometry::Extrusion(e) => convert::extrusion_body(e, s),
                _ => continue,
            };
            bodies.push(match converted {
                Ok(c) => {
                    let check = c.body.check(file.tolerance * s);
                    json!({
                        "object": i, "faces": c.body.faces.len(), "edges": c.body.edges.len(),
                        "lumps": c.body.lumps.len(), "solid": c.body.is_solid(),
                        "skipped_faces": c.skipped_faces, "unknown_edges": c.unknown_edges,
                        "degenerate_edges": c.degenerate_edges,
                        "free_edges": check.free_edges, "unpaired_coedges": check.unpaired_coedges,
                        "open_loops": check.open_loops, "vertex_mismatches": check.vertex_mismatches,
                        "max_vertex_gap": check.max_vertex_gap,
                        "off_surface_vertices": check.off_surface_vertices,
                        "max_surface_gap": check.max_surface_gap,
                        "issues": c.issues.iter().take(20).collect::<Vec<_>>(),
                    })
                }
                Err(e) => json!({"object": i, "error": e}),
            });
        }
        out["converted"] = json!(bodies);
    }
    if args.iter().any(|a| a == "--faces") {
        out["faces"] = json!(
            file.objects
                .iter()
                .enumerate()
                .filter_map(|(i, o)| face_dump(i, &o.geometry))
                .collect::<Vec<_>>()
        );
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&out).expect("the summary serializes")
    );
}

/// A surface's form for `--faces`: a plane surface's frame, domain and
/// extents, the kind of the others.
fn surface_dump(s: Option<&geom::Surface>) -> serde_json::Value {
    match s {
        Some(geom::Surface::Plane {
            plane,
            domain,
            extents,
        }) => json!({
            "plane": {"origin": plane.origin, "x": plane.x, "y": plane.y, "z": plane.z},
            "domain": domain, "extents": extents,
        }),
        Some(geom::Surface::Nurbs(n)) => json!({"nurbs": {"order": n.order, "count": n.count}}),
        Some(geom::Surface::Revolution { transposed, .. }) => {
            json!({"revolution": {"transposed": transposed}})
        }
        Some(geom::Surface::Sum { .. }) => json!("sum"),
        Some(geom::Surface::Other(name)) => json!(name),
        None => json!(null),
    }
}

/// Each face of a B-rep with its surface, orientation and loops (`--faces`):
/// a loop's trims (edge, against the edge, kind, start and end points) and
/// the loop's normal by its vertices (Newell's method), to compare with the
/// surface's.
fn face_dump(index: usize, g: &geom::Geometry) -> Option<serde_json::Value> {
    let geom::Geometry::Brep(b) = g else {
        return match g {
            geom::Geometry::Surface(s) => {
                Some(json!({"object": index, "surface": surface_dump(Some(s))}))
            }
            _ => None,
        };
    };
    let point = |v: i32| {
        usize::try_from(v)
            .ok()
            .and_then(|v| b.vertices.get(v))
            .map(|v| v.point)
    };
    let faces: Vec<serde_json::Value> =
        b.faces
            .iter()
            .map(|f| {
                let surface = usize::try_from(f.surface)
                    .ok()
                    .and_then(|s| b.surfaces.get(s))
                    .and_then(|s| s.as_deref());
                let loops: Vec<serde_json::Value> =
                    f.loops
                        .iter()
                        .filter_map(|&l| usize::try_from(l).ok().and_then(|l| b.loops.get(l)))
                        .map(|lp| {
                            let mut newell = [0.0f64; 3];
                            let mut trims = Vec::new();
                            for t in lp.trims.iter().filter_map(|&t| {
                                usize::try_from(t).ok().and_then(|t| b.trims.get(t))
                            }) {
                                let (p, q) = (point(t.vertices[0]), point(t.vertices[1]));
                                if let (Some(p), Some(q)) = (p, q) {
                                    newell[0] += (p[1] - q[1]) * (p[2] + q[2]);
                                    newell[1] += (p[2] - q[2]) * (p[0] + q[0]);
                                    newell[2] += (p[0] - q[0]) * (p[1] + q[1]);
                                }
                                trims.push(json!({
                                    "edge": t.edge, "reversed_3d": t.reversed_3d,
                                    "kind": format!("{:?}", t.kind), "from": p, "to": q,
                                }));
                            }
                            json!({"kind": format!("{:?}", lp.kind), "normal_by_vertices": newell,
                           "trims": trims})
                        })
                        .collect();
                json!({"reversed": f.reversed, "surface": surface_dump(surface), "loops": loops})
            })
            .collect();
    Some(json!({"object": index, "is_solid": b.is_solid, "faces": faces}))
}
