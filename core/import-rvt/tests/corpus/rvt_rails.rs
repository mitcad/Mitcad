// SPDX-License-Identifier: MIT
//! Rails and stairs of the corpus's 2024 projects (by content id under
//! `MITCAD_RVT_CORPUS`; skipped without it, mitcad#391):
//!
//! - the copies a multistory stair places on further storeys
//!   (`Model::multistory_copies`) are exactly where the geometry the file
//!   keeps places the stairs' runs, landings and supports, the railings'
//!   balusters and the rails' supports and symbols again;
//! - the twisting corner of `33271010`'s rail `1270819`, rounded in plan
//!   along a helix: every node of the stored Hermite faces there lies on
//!   the rail's profile swept along that helix;
//! - (mitcad#395) the landings whose type takes its treads and risers from
//!   the stair's run type: their extents those of the treads the file
//!   keeps, as thick; the rails up stairs whose corners are not filleted:
//!   every plane face the file keeps of them (their mitres and ends) lies
//!   in a plane that cuts or ends a piece of the rail;
//! - (mitcad#397) runs of treads and risers (slanted risers on their
//!   treads, risers behind them, nosing profiles at the treads' fronts):
//!   the volume and box of the geometry the file keeps.

use std::collections::HashMap;
use std::path::PathBuf;

use mitcad_ifc::geometry::{Item, Seg};
use mitcad_ifc::math;
use mitcad_import_rvt::plan::plan;
use mitcad_rvt::RvtFile;
use mitcad_rvt::brep::Surface;
use mitcad_rvt::elements::{FOOT_MM, Model};

fn corpus_model(id: &str) -> Option<Model> {
    let dir = PathBuf::from(std::env::var_os("MITCAD_RVT_CORPUS")?);
    let index = std::fs::read_to_string(dir.join("index.tsv")).ok()?;
    let path = index.lines().find_map(|line| {
        let mut cols = line.split('\t');
        let path = cols.next()?;
        cols.next()?.starts_with(id).then(|| path.to_string())
    })?;
    let p = dir.join(path);
    if !p.is_file() {
        return None;
    }
    let file = RvtFile::open(&p).unwrap();
    let schema = file.schema().unwrap();
    Some(Model::read(&file, &schema).unwrap())
}

#[test]
fn a_multistory_stair_s_copies_are_where_the_file_places_them() {
    // (Content id, stairs repeated, elements copied.)
    for (id, stairs, copied) in [("33271010", 3, 425), ("ff791052", 5, 64)] {
        let Some(model) = corpus_model(id) else {
            eprintln!("MITCAD_RVT_CORPUS has no {id}: skipped");
            continue;
        };
        assert_eq!(model.building.multistory.len(), stairs, "{id}");
        let elements: HashMap<i64, &mitcad_rvt::elements::Element> =
            model.elements.iter().map(|e| (e.id, e)).collect();
        // How far up the geometry the file keeps places each element (by
        // other elements' geometry: a stair's, a railing's, a rail's), and
        // a rail or a railing its own symbol.
        let mut placed: HashMap<i64, Vec<f64>> = HashMap::new();
        for (owner, g) in &model.geometry {
            for p in &g.instances {
                let Some(s) = p.symbol else { continue };
                let Some(e) = elements.get(&s) else { continue };
                let own_symbol = matches!(
                    e.class.as_deref(),
                    Some("ContinuousRailSymbol" | "BaseRailingSym")
                );
                if e.is_type && !own_symbol {
                    continue;
                }
                let at = if own_symbol { *owner } else { s };
                placed.entry(at).or_default().push(p.transform[11]);
            }
        }
        let mut seen = 0;
        for e in &model.elements {
            let Some((stair, copies)) = model.multistory_copies(e.id) else {
                continue;
            };
            if stair == e.id || copies.is_empty() {
                continue;
            }
            // (A rail's support the rail no longer lists: nothing places it.)
            let rail = model
                .building
                .owners
                .get(&e.id)
                .and_then(|o| model.building.sweeps.get(o));
            if rail.is_some_and(|r| !r.supports.contains(&e.id)) {
                assert!(!placed.contains_key(&e.id), "{id}: {}", e.id);
                continue;
            }
            seen += 1;
            // (A support is placed by its rail and its railing, a railing
            // places each of its rails' symbols: each height once.)
            let mut ups: Vec<f64> = Vec::new();
            for z in placed.get(&e.id).into_iter().flatten() {
                if z.abs() > 1e-6 && !ups.iter().any(|u| (u - z).abs() < 1e-6) {
                    ups.push(*z);
                }
            }
            assert_eq!(ups.len(), copies.len(), "{id}: {} at {ups:?}", e.id);
            for ((_, dz), up) in copies.iter().zip(&ups) {
                assert!((dz - up).abs() < 1e-6, "{id}: {} {dz} {up}", e.id);
            }
        }
        assert_eq!(seen, copied, "{id}");
    }
}

#[test]
fn a_rail_s_twisting_corner_follows_its_stored_hermite_faces() {
    let Some(model) = corpus_model("33271010") else {
        eprintln!("MITCAD_RVT_CORPUS has no 33271010: skipped");
        return;
    };
    let rail = model.element(1270819).unwrap();
    let p = plan(&model, rail).unwrap();
    assert!(
        !p.notes.iter().any(|n| n.contains("mitered")),
        "{:?}",
        p.notes
    );
    // The sweep along the helix (inside the cuts at its mitres).
    fn sweeps<'a>(item: &'a Item, out: &mut Vec<&'a Item>) {
        match item {
            Item::Boolean { first, second, .. } => {
                sweeps(first, out);
                sweeps(second, out);
            }
            Item::Sweep { .. } => out.push(item),
            _ => {}
        }
    }
    let mut found = Vec::new();
    sweeps(&p.solid, &mut found);
    let [
        Item::Sweep {
            profile,
            directrix,
            reference,
            ..
        },
    ] = found.as_slice()
    else {
        panic!("{found:?}");
    };
    let Some(&Seg::Helix {
        frame,
        radius,
        pitch,
        start,
        sweep,
    }) = directrix.segments.first()
    else {
        panic!("{directrix:?}");
    };
    let rise = pitch / std::f64::consts::TAU;
    let (cx, cy, cz) = (
        math::column(&frame, 0),
        math::column(&frame, 1),
        math::column(&frame, 2),
    );
    let o = math::origin(&frame);
    let out_at = |t: f64| math::add(math::scale(cx, t.cos()), math::scale(cy, t.sin()));
    let at = |t: f64| {
        math::add(
            o,
            math::add(math::scale(out_at(t), radius), math::scale(cz, rise * t)),
        )
    };
    let tangent = |t: f64| {
        math::normalize(math::add(
            math::scale(
                math::sub(math::scale(cy, t.cos()), math::scale(cx, t.sin())),
                radius,
            ),
            math::scale(cz, rise),
        ))
        .unwrap()
    };
    // The profile's x, level: towards or away from the axis as at the start.
    let side = math::dot(reference.unwrap(), out_at(start)).signum();
    let outline: Vec<[f64; 3]> = profile.areas[0]
        .outer
        .segments
        .iter()
        .flat_map(|s| s.sample(2000))
        .collect();
    let mut nodes = 0;
    let symbol = model.geometry[&1270819].instances[0].symbol.unwrap();
    for s in &model.geometry[&symbol].solids {
        for f in &s.faces {
            let Surface::Hermite { nodes: n, .. } = &f.surface else {
                continue;
            };
            for node in n {
                let q = node[0].map(|v| v * FOOT_MM);
                // The section through it: where it is square to the helix.
                let along = |t: f64| math::dot(math::sub(q, at(t)), tangent(t));
                let (mut lo, mut hi) = (start - 0.2, start + sweep + 0.2);
                assert!(along(lo) > 0.0 && along(hi) < 0.0);
                for _ in 0..100 {
                    let mid = 0.5 * (lo + hi);
                    if along(mid) > 0.0 {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                let t = 0.5 * (lo + hi);
                let x = math::scale(out_at(t), side);
                let y = math::cross(tangent(t), x);
                let d = math::sub(q, at(t));
                let local = [math::dot(d, x), math::dot(d, y), 0.0];
                let off = outline
                    .iter()
                    .map(|p| math::length(math::sub(*p, local)))
                    .fold(f64::INFINITY, f64::min);
                assert!(off < 0.01, "a node {off} mm off the profile");
                nodes += 1;
            }
        }
    }
    assert_eq!(nodes, 100);
}

#[test]
fn a_landing_with_its_run_s_treads_is_its_stored_tread() {
    // (Content id, landings whose type takes its treads and risers from its
    // stair's run type, mitcad#395: slanted risers standing on the treads
    // in `33271010`, and no risers and a nosing profile along the sides too
    // (`746555`); risers behind the treads and a nosing profile at the
    // front in `ff791052`.)
    for (id, landings) in [
        ("33271010", &[621697, 701097, 882257, 746555][..]),
        ("ff791052", &[354440, 355955, 357442, 358449, 360295][..]),
    ] {
        let Some(model) = corpus_model(id) else {
            eprintln!("MITCAD_RVT_CORPUS has no {id}: skipped");
            continue;
        };
        for &landing in landings {
            let e = model.element(landing).unwrap();
            let p = plan(&model, e).unwrap();
            // The box of what the plan adds (mm): its extrusions, less what
            // it takes away.
            fn added(item: &Item, lo: &mut [f64; 3], hi: &mut [f64; 3], depth: &mut Vec<f64>) {
                match item {
                    Item::Boolean { op, first, second } => {
                        added(first, lo, hi, depth);
                        if *op == mitcad_ifc::geometry::BoolOp::Union {
                            added(second, lo, hi, depth);
                        }
                    }
                    Item::Extrusion {
                        position,
                        profile,
                        direction,
                        depth: d,
                    } => {
                        depth.push(*d);
                        let up = math::scale(math::apply_vector(position, *direction), *d);
                        for s in &profile.areas[0].outer.segments {
                            for q in s.sample(2) {
                                let q = math::apply(position, q);
                                for r in [q, math::add(q, up)] {
                                    for k in 0..3 {
                                        lo[k] = lo[k].min(r[k]);
                                        hi[k] = hi[k].max(r[k]);
                                    }
                                }
                            }
                        }
                    }
                    // (A side nosing swept along lines: the profile at each
                    // line's ends, its x level and square to the line.)
                    Item::Sweep {
                        profile,
                        directrix,
                        reference,
                        ..
                    } => {
                        let vertical = [0.0, 0.0, 1.0];
                        let along = |s: &Seg| {
                            let Seg::Line { start, end } = s else {
                                panic!("{s:?}")
                            };
                            (
                                *start,
                                *end,
                                math::normalize(math::sub(*end, *start)).unwrap(),
                            )
                        };
                        let (_, _, t0) = along(&directrix.segments[0]);
                        let side = math::dot(reference.unwrap(), math::cross(vertical, t0));
                        for s in &directrix.segments {
                            let (a, b, t) = along(s);
                            let x = math::scale(math::cross(vertical, t), side.signum());
                            let y = math::cross(t, x);
                            for q in profile.areas[0]
                                .outer
                                .segments
                                .iter()
                                .flat_map(|s| s.sample(2))
                            {
                                for o in [a, b] {
                                    let r = math::add(
                                        o,
                                        math::add(math::scale(x, q[0]), math::scale(y, q[1])),
                                    );
                                    for k in 0..3 {
                                        lo[k] = lo[k].min(r[k]);
                                        hi[k] = hi[k].max(r[k]);
                                    }
                                }
                            }
                        }
                    }
                    other => panic!("{other:?}"),
                }
            }
            let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
            let mut depths = Vec::new();
            added(&p.solid, &mut lo, &mut hi, &mut depths);
            // The tread its geometry places (its own faces are its sides).
            let symbol = model.geometry[&landing].instances[0].symbol.unwrap();
            let stored = model.geometry[&symbol]
                .solids
                .iter()
                .filter_map(|s| s.bounds())
                .next()
                .unwrap()
                .map(|v| v * FOOT_MM);
            for k in 0..3 {
                assert!(
                    (lo[k] - stored[k]).abs() < 1e-6 && (hi[k] - stored[k + 3]).abs() < 1e-6,
                    "{id} {landing}: {lo:?} {hi:?} against {stored:?} ({:?})",
                    p.notes
                );
            }
            // As thick as its run's tread.
            assert!(
                (depths[0] - (stored[5] - stored[2])).abs() < 1e-6,
                "{id} {landing}"
            );
        }
    }
}

#[test]
fn a_rail_s_corners_up_a_stair_are_mitred_as_stored() {
    let Some(model) = corpus_model("33271010") else {
        eprintln!("MITCAD_RVT_CORPUS has no 33271010: skipped");
        return;
    };
    // The planes (mm: a point and the unit normal) that cut or end the
    // pieces of a plan: its half spaces and its extrusions' ends.
    fn planes(item: &Item, out: &mut Vec<([f64; 3], [f64; 3])>) {
        match item {
            Item::Boolean { first, second, .. } => {
                planes(first, out);
                planes(second, out);
            }
            Item::HalfSpace { plane, .. } => {
                out.push((math::origin(plane), math::column(plane, 2)));
            }
            Item::Extrusion {
                position,
                direction,
                depth,
                ..
            } => {
                let n = math::normalize(math::apply_vector(position, *direction)).unwrap();
                let o = math::origin(position);
                out.push((o, n));
                out.push((math::add(o, math::scale(n, *depth)), n));
            }
            _ => {}
        }
    }
    // Rails along lines up stairs, their corners not filleted.
    for rail in [832432, 832452, 832740, 832772, 2270401] {
        let e = model.element(rail).unwrap();
        let p = plan(&model, e).unwrap();
        let mut cut = Vec::new();
        planes(&p.solid, &mut cut);
        // The rail's solid as the file keeps it (beside its sheet faces).
        let symbol = model.geometry[&rail].instances[0].symbol.unwrap();
        let solid = model.geometry[&symbol]
            .solids
            .iter()
            .max_by_key(|s| s.faces.len())
            .unwrap();
        // Its edges' points on the surfaces they are kept on (the file
        // keeps some mitres' plane faces without their loops, the pieces'
        // cylinders meeting there): each edge that is not straight (round a
        // round profile's end or mitre) in one of the planes, and each end
        // of a straight one (along a piece, or across an end or a mitre of
        // a profile of lines) in one.
        let off = |points: &[[f64; 3]]| {
            cut.iter()
                .map(|(o, n)| {
                    points
                        .iter()
                        .map(|q| math::dot(math::sub(*q, *o), *n).abs())
                        .fold(0.0, f64::max)
                })
                .fold(f64::INFINITY, f64::min)
        };
        let (mut curved, mut ends) = (0, 0);
        for (k, e) in solid.edges.iter().enumerate() {
            let points: Vec<[f64; 3]> = (0..2)
                .flat_map(|side| {
                    let surface = e.faces[side].map(|j| &solid.faces[j].surface);
                    e.points[side].iter().filter_map(move |uv| surface?.at(*uv))
                })
                .map(|q| q.map(|v| v * FOOT_MM))
                .collect();
            let (Some(a), Some(b)) = (points.first(), points.last()) else {
                continue;
            };
            let Some(d) = math::normalize(math::sub(*b, *a)) else {
                continue;
            };
            let straight = points.iter().all(|q| {
                let v = math::sub(*q, *a);
                math::length(math::sub(v, math::scale(d, math::dot(v, d)))) < 1e-6
            });
            if straight {
                for q in [a, b] {
                    let d = off(&[*q]);
                    assert!(
                        d < 1e-4,
                        "{rail} edge {k}'s end: {d} mm off ({:?})",
                        p.notes
                    );
                    ends += 1;
                }
            } else {
                let d = off(&points);
                assert!(d < 1e-4, "{rail} edge {k}: {d} mm off ({:?})", p.notes);
                curved += 1;
            }
        }
        assert!(curved + ends > 0, "{rail}");
    }
}

#[test]
fn a_sloped_elliptical_rail_piece_meets_the_next_in_its_stored_section() {
    // (mitcad#397) The 3 road stripings of `33271010`, of sloped
    // elliptical pieces meeting level arcs at an angle: the plan sweeps the
    // ellipses with the binormal up, and cuts the piece after each such
    // joint in the piece before it's end section, as the file keeps them:
    // every straight edge the file keeps within 0.4 ft of the joint runs
    // along that plane (within 1e-5 rad: the profile's x level, its y
    // square to the sloped piece) and lies in it within the 0.3 mm by which
    // the path's joint (`Path3d`) lies on along it from the stored one.
    let Some(model) = corpus_model("33271010") else {
        eprintln!("MITCAD_RVT_CORPUS has no 33271010: skipped");
        return;
    };
    fn cuts(item: &Item, out: &mut Vec<([f64; 3], [f64; 3])>, binormals: &mut Vec<[f64; 3]>) {
        match item {
            Item::Boolean { first, second, .. } => {
                cuts(first, out, binormals);
                cuts(second, out, binormals);
            }
            Item::HalfSpace { plane, .. } => {
                out.push((math::origin(plane), math::column(plane, 2)));
            }
            Item::Sweep {
                binormal: Some(b), ..
            } => binormals.push(*b),
            _ => {}
        }
    }
    use mitcad_rvt::building::Curve;
    for rail in [2150352, 2153181, 2153936] {
        let e = model.element(rail).unwrap();
        let p = plan(&model, e).unwrap();
        let (mut planes, mut binormals) = (Vec::new(), Vec::new());
        cuts(&p.solid, &mut planes, &mut binormals);
        assert!(
            !binormals.is_empty() && binormals.iter().all(|b| *b == [0.0, 0.0, 1.0]),
            "{rail}: {binormals:?}"
        );
        // The joints of an elliptical piece where the path turns.
        let path = model.building.sweeps[&rail].path3d.unwrap();
        let curves: Vec<&Curve> = model.building.paths3d[&path].iter().flatten().collect();
        let mut joints = Vec::new();
        for (a, b) in curves.iter().zip(curves.iter().skip(1)) {
            if !matches!(a, Curve::Ellipse { .. }) && !matches!(b, Curve::Ellipse { .. }) {
                continue;
            }
            let ta = math::normalize(math::sub(a.end(), a.at(1.0 - 1e-7))).unwrap();
            let tb = math::normalize(math::sub(b.at(1e-7), b.start())).unwrap();
            if math::length(math::sub(a.end(), b.start())) < 1e-4 && math::dot(ta, tb) < 1.0 - 1e-8
            {
                joints.push(a.end().map(|v| v * FOOT_MM));
            }
        }
        assert!(!joints.is_empty(), "{rail}");
        let symbol = model.geometry[&rail].instances[0].symbol.unwrap();
        let solid = model.geometry[&symbol]
            .solids
            .iter()
            .max_by_key(|s| s.faces.len())
            .unwrap();
        let mut checked = 0;
        for joint in &joints {
            for edge in &solid.edges {
                let points: Vec<[f64; 3]> = (0..2)
                    .flat_map(|side| {
                        let surface = edge.faces[side].map(|j| &solid.faces[j].surface);
                        edge.points[side]
                            .iter()
                            .filter_map(move |uv| surface?.at(*uv))
                    })
                    .map(|q| q.map(|v| v * FOOT_MM))
                    .collect();
                let (Some(a), Some(b)) = (points.first(), points.last()) else {
                    continue;
                };
                let Some(d) = math::normalize(math::sub(*b, *a)) else {
                    continue;
                };
                let near = points
                    .iter()
                    .all(|q| math::length(math::sub(*q, *joint)) < 0.4 * FOOT_MM);
                let straight = points.iter().all(|q| {
                    let v = math::sub(*q, *a);
                    math::length(math::sub(v, math::scale(d, math::dot(v, d)))) < 1e-6
                });
                if !near || !straight {
                    continue;
                }
                // (Along the plane within 1e-5 rad; off it by at most the
                // 0.3 mm the path's joint lies from the stored one.)
                let lies = planes.iter().any(|(o, n)| {
                    math::dot(d, *n).abs() < 1e-5
                        && points
                            .iter()
                            .all(|q| math::dot(math::sub(*q, *o), *n).abs() < 0.35)
                });
                assert!(lies, "{rail} at {joint:?}: {points:?} against {planes:?}");
                checked += 1;
            }
        }
        assert!(checked > 0, "{rail}");
    }
}

#[test]
fn a_run_of_treads_and_risers_is_its_stored_run() {
    // (mitcad#397) Runs of `33271010` as one section across their width:
    // slanted risers standing on their treads (`621691`, `621695`), risers
    // behind their treads and the nosing profile at the treads' fronts
    // (`832703`; `1240916`, whose first riser reaches down behind the
    // landing it leaves; `1563804`, with a top tread). Their volume and box
    // are those of the geometry the file keeps (a mesh whose treads and
    // risers meet edge to edge).
    let Some(model) = corpus_model("33271010") else {
        eprintln!("MITCAD_RVT_CORPUS has no 33271010: skipped");
        return;
    };
    for run in [621691, 621695, 832703, 1240916, 1563804] {
        let e = model.element(run).unwrap();
        let p = plan(&model, e).unwrap();
        let Item::Extrusion {
            position,
            profile,
            direction,
            depth,
        } = &p.solid
        else {
            panic!("{run}: {:?}", p.solid)
        };
        assert!(p.more.is_empty(), "{run}");
        let shoelace = |path: &mitcad_ifc::geometry::Path| {
            let q: Vec<[f64; 3]> = path.segments.iter().map(|s| s.sample(1)[0]).collect();
            (0..q.len())
                .map(|k| {
                    let (a, b) = (q[k], q[(k + 1) % q.len()]);
                    a[0] * b[1] - b[0] * a[1]
                })
                .sum::<f64>()
                .abs()
                / 2.0
        };
        let area: f64 = profile
            .areas
            .iter()
            .map(|a| shoelace(&a.outer) - a.holes.iter().map(shoelace).sum::<f64>())
            .sum();
        let (lo, hi) = {
            let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
            let up = math::scale(math::apply_vector(position, *direction), *depth);
            for q in profile
                .areas
                .iter()
                .flat_map(|a| &a.outer.segments)
                .flat_map(|s| s.sample(1))
            {
                let q = math::apply(position, q);
                for r in [q, math::add(q, up)] {
                    for k in 0..3 {
                        lo[k] = lo[k].min(r[k]);
                        hi[k] = hi[k].max(r[k]);
                    }
                }
            }
            (lo, hi)
        };
        let mesh = mitcad_import_rvt::geometry::element_mesh(
            &model,
            run,
            &mut mitcad_import_rvt::geometry::Cache::new(),
            &Default::default(),
        )
        .unwrap()
        .unwrap();
        assert!(
            mitcad_import_ifc::brep::mesh_balanced(&mesh.points, &mesh.triangles),
            "{run}"
        );
        let stored = mitcad_import_ifc::brep::mesh_measures(&mesh.points, &mesh.triangles)
            .0
            .abs();
        let volume = area * depth;
        assert!(
            (volume - stored).abs() < 1e-9 * stored,
            "{run}: {volume} against {stored} mm³ ({:?})",
            p.notes
        );
        let (slo, shi) = mesh.bounds().unwrap();
        for k in 0..3 {
            assert!(
                (lo[k] - slo[k]).abs() < 1e-6 && (hi[k] - shi[k]).abs() < 1e-6,
                "{run}: {lo:?} {hi:?} against {slo:?} {shi:?}"
            );
        }
    }
}
