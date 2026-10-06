// SPDX-License-Identifier: MIT
//! Command-line inspector for .f3d/.f3z files: container contents,
//! ASM records, the conversion of bodies to the neutral B-rep model, the
//! display meshes saved with a document, and the decoded design streams
//! (timeline, parameters, sketches).

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use mitcad_f3d::asm::AsmFile;
use mitcad_f3d::asm::history::History;
use mitcad_f3d::container::F3dFile;
use mitcad_f3d::convert::Options;

const USAGE: &str = "usage: mitcad-f3d-inspect <command> ...
  list <file>                 entries with sizes, methods and kinds
  info <file>                 segments, body blobs and their ASM headers
  extract <file> <dir>        write every entry to <dir>
  dump <file> <entry> [n]     ASM records of a body blob as text (first n)
  bodies <file> [-v]          convert every body; per-body report
  history <file>              ASM history of the .smbh blobs: delta states
                              and the bodies rolled back state by state
  meshes <file>               display meshes saved with the document, per body
  coverage <file>...          totals over many files
  design <file> [--json]      decoded design streams: timeline, parameters,
                              components; --json prints the dump IR
                              (core/import/SCHEMA.md; an array for an
                              .f3z package)";

fn open(path: &str) -> Result<F3dFile, String> {
    F3dFile::open(Path::new(path)).map_err(|e| format!("{path}: {e}"))
}

/// The documents to look at: the file itself, or the documents of an .f3z.
fn documents(path: &str) -> Result<Vec<(String, F3dFile)>, String> {
    let f = open(path)?;
    let nested = f.documents();
    if nested.is_empty() {
        return Ok(vec![(String::new(), f)]);
    }
    let mut out = Vec::new();
    for name in nested {
        let doc = f
            .open_document(&name)
            .map_err(|e| format!("{path}/{name}: {e}"))?;
        out.push((name, doc));
    }
    Ok(out)
}

fn cmd_list(path: &str) -> Result<(), String> {
    for e in open(path)?.entries() {
        println!(
            "{:>10} {:>10} {:<8} {:<22} {}",
            e.size,
            e.compressed_size,
            e.method.to_string(),
            format!("{:?}", e.kind)
                .split([' ', '{'])
                .next()
                .unwrap_or(""),
            e.name
        );
    }
    Ok(())
}

fn cmd_info(path: &str) -> Result<(), String> {
    for (name, doc) in documents(path)? {
        if !name.is_empty() {
            println!("== document {name}");
        }
        if let Some(p) = doc.properties() {
            println!("properties: {p}");
        }
        for s in doc.segments() {
            println!(
                "segment {} ({}){}",
                s.name,
                s.kind,
                if s.is_design() { " design" } else { "" }
            );
        }
        let refs = doc.blob_references();
        for b in doc.body_blobs() {
            let data = doc.read(&b.entry).map_err(|e| e.to_string())?;
            let mine: Vec<_> = refs.iter().filter(|r| r.blob == b.entry).collect();
            let n = mine.len();
            let context = mine
                .first()
                .map(|r| r.context.join(" / "))
                .unwrap_or_default();
            match AsmFile::parse(&data) {
                Ok(f) => {
                    let h = &f.header;
                    let bodies = f
                        .top_level()
                        .filter(|&i| f.records[i].type_name == "body")
                        .count();
                    println!(
                        "blob {} {}: BinaryFile{} v{} {:?} units {} flags {} top-level {} bodies {} records {}{} design refs {} after [{}]",
                        b.guid,
                        if b.history { "smbh" } else { "smb" },
                        h.int_size,
                        h.version,
                        h.asm_version,
                        h.units,
                        h.flags,
                        h.entity_count,
                        bodies,
                        f.records.len(),
                        if f.complete { "" } else { " (incomplete)" },
                        n,
                        context
                    );
                }
                Err(e) => println!("blob {}: {e}", b.guid),
            }
        }
    }
    Ok(())
}

fn cmd_extract(path: &str, out: &str) -> Result<(), String> {
    let f = open(path)?;
    let mut failed = false;
    for e in f.archive().entries() {
        if e.is_dir() {
            continue;
        }
        match f.archive().read(e) {
            Ok(data) => {
                let p = Path::new(out).join(&e.name);
                if let Some(parent) = p.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                std::fs::write(&p, data).map_err(|err| format!("{}: {err}", p.display()))?;
            }
            Err(err) => {
                eprintln!("{}: {err}", e.name);
                failed = true;
            }
        }
    }
    if failed {
        Err("some entries failed".into())
    } else {
        Ok(())
    }
}

fn cmd_dump(path: &str, entry: &str, max: Option<usize>) -> Result<(), String> {
    let data = open(path)?.read(entry).map_err(|e| e.to_string())?;
    let f = AsmFile::parse(&data).map_err(|e| e.to_string())?;
    println!("{:?}", f.header);
    let n = max.unwrap_or(usize::MAX).min(f.records.len());
    for i in 0..n {
        println!("{}", f.record_text(i, 4000));
    }
    if let Some(t) = &f.truncated {
        println!("stopped: {t}");
    }
    Ok(())
}

fn cmd_meshes(path: &str) -> Result<(), String> {
    for (name, doc) in documents(path)? {
        let label = if name.is_empty() {
            "display scene".to_string()
        } else {
            format!("{name}: display scene")
        };
        let scene = match mitcad_f3d::ogs::display_scene(&doc) {
            Ok(Some(s)) => s,
            Ok(None) => {
                println!("{label}: none");
                continue;
            }
            Err(e) => {
                println!("{label}: {e}");
                continue;
            }
        };
        println!("{label}: {} bodies", scene.bodies.len());
        for (i, b) in scene.bodies.iter().enumerate() {
            let s = b.stats();
            let shape = if s.triangles == 0 {
                "no mesh".to_string()
            } else if s.closed {
                "closed".to_string()
            } else {
                format!(
                    "open ({} boundary, {} non-manifold edges)",
                    s.boundary_edges, s.nonmanifold_edges
                )
            };
            let bb = s.bbox_mm;
            println!(
                "  body {i} node {} key {}{}: faces {} (without mesh {}) triangles {} {shape} volume {:.3} mm3 area {:.3} mm2 bbox [{:.3} {:.3} {:.3}] [{:.3} {:.3} {:.3}] mm",
                b.node.as_deref().unwrap_or("-"),
                b.key,
                b.appearance
                    .as_deref()
                    .map(|a| format!(" appearance {a}"))
                    .unwrap_or_default(),
                b.faces.len(),
                b.faces_without_mesh(),
                s.triangles,
                s.volume_mm3,
                s.area_mm2,
                bb[0],
                bb[1],
                bb[2],
                bb[3],
                bb[4],
                bb[5]
            );
            for (what, n) in [
                ("misoriented edges", s.misoriented_edges),
                ("degenerate triangles", s.degenerate_triangles),
                ("vertices outside their face box", b.off_bbox_vertices),
            ] {
                if n > 0 {
                    println!("    {what}: {n}");
                }
            }
        }
        for issue in &scene.issues {
            println!("  issue: {issue}");
        }
    }
    Ok(())
}

#[derive(Default)]
struct Totals {
    files: usize,
    files_failed: usize,
    blobs: usize,
    blobs_failed: usize,
    blobs_truncated: usize,
    bodies: usize,
    bodies_clean: usize,
    solids: usize,
    bodies_with_issues: usize,
    faces: usize,
    faces_skipped: usize,
    degenerate_edges: usize,
    unknown_types: BTreeMap<String, usize>,
    subtypes: BTreeMap<String, usize>,
    issues: BTreeMap<String, usize>,
    surface_kinds: BTreeMap<String, usize>,
    curve_kinds: BTreeMap<String, usize>,
}

/// Issue text without numbers, for counting kinds of issues.
fn issue_kind(s: &str) -> String {
    let mut out = String::new();
    let mut last_digit = false;
    for c in s.chars() {
        if c.is_ascii_digit() || c == '.' && last_digit || c == '-' && last_digit {
            if !last_digit {
                out.push('#');
            }
            last_digit = true;
        } else {
            out.push(c);
            last_digit = false;
        }
    }
    out
}

fn process(path: &str, verbose: bool, t: &mut Totals, print: bool) {
    t.files += 1;
    let docs = match documents(path) {
        Ok(d) => d,
        Err(e) => {
            t.files_failed += 1;
            eprintln!("{e}");
            return;
        }
    };
    let opt = Options::default();
    for (name, doc) in docs {
        for blob in mitcad_f3d::read_document_bodies(&doc, &opt) {
            t.blobs += 1;
            let blob = match blob {
                Ok(b) => b,
                Err(e) => {
                    t.blobs_failed += 1;
                    if print {
                        println!("{name} {e}");
                    }
                    continue;
                }
            };
            if blob.truncated.is_some() {
                t.blobs_truncated += 1;
            }
            merge(&mut t.unknown_types, &blob.coverage.unknown_types);
            merge(&mut t.subtypes, &blob.coverage.subtypes);
            let short = blob.entry.rsplit('/').next().unwrap_or(&blob.entry);
            if print {
                println!(
                    "{name}{}{short}: {} {} bodies{}",
                    if name.is_empty() { "" } else { "/" },
                    blob.header.asm_version,
                    blob.bodies.len(),
                    blob.truncated
                        .as_deref()
                        .map(|e| format!(" TRUNCATED {e}"))
                        .unwrap_or_default()
                );
            }
            for b in &blob.bodies {
                t.bodies += 1;
                if b.body.is_solid() {
                    t.solids += 1;
                }
                t.faces += b.body.faces.len();
                t.faces_skipped += b.skipped_faces;
                t.degenerate_edges += b.degenerate_edges;
                if b.check.is_clean() && b.issues.is_empty() && b.skipped_faces == 0 {
                    t.bodies_clean += 1;
                }
                if !b.issues.is_empty() {
                    t.bodies_with_issues += 1;
                }
                for i in &b.issues {
                    *t.issues.entry(issue_kind(i)).or_insert(0) += 1;
                }
                let c = &b.check;
                for (bad, what) in [
                    (c.unpaired_coedges > 0, "bodies with unpaired edges"),
                    (c.open_loops > 0, "bodies with open loops"),
                    (c.vertex_mismatches > 0, "bodies with vertex gaps"),
                    (
                        c.off_surface_vertices > 0,
                        "bodies with vertices off analytic surfaces",
                    ),
                ] {
                    if bad {
                        *t.issues.entry(what.to_string()).or_insert(0) += 1;
                    }
                }
                for m in &c.messages {
                    *t.issues
                        .entry(format!("check: {}", issue_kind(m)))
                        .or_insert(0) += 1;
                }
                for s in &b.body.surfaces {
                    *t.surface_kinds.entry(s.kind().to_string()).or_insert(0) += 1;
                }
                for c in &b.body.curves {
                    *t.curve_kinds.entry(c.kind().to_string()).or_insert(0) += 1;
                }
                if print {
                    let c = &b.check;
                    println!(
                        "  body {} {}: lumps {} faces {} (skipped {}) edges {} vertices {} issues {} | free {} unpaired {} open loops {} vertex gaps {} (max {:.2e}) off-surface {} (max {:.2e})",
                        b.record,
                        if b.body.is_solid() { "solid" } else { "sheet" },
                        b.body.lumps.len(),
                        b.body.faces.len(),
                        b.skipped_faces,
                        b.body.edges.len(),
                        b.body.vertices.len(),
                        b.issues.len(),
                        c.free_edges,
                        c.unpaired_coedges,
                        c.open_loops,
                        c.vertex_mismatches,
                        c.max_vertex_gap,
                        c.off_surface_vertices,
                        c.max_surface_gap
                    );
                    if verbose {
                        for i in b.issues.iter().chain(&c.messages).take(12) {
                            println!("    {i}");
                        }
                    }
                }
            }
        }
    }
}

fn cmd_history(path: &str) -> Result<(), String> {
    let opt = Options::default();
    for (name, doc) in documents(path)? {
        for blob in doc.body_blobs().into_iter().filter(|b| b.history) {
            let data = doc.read(&blob.entry).map_err(|e| e.to_string())?;
            let file = AsmFile::parse(&data).map_err(|e| e.to_string())?;
            let short = blob.entry.rsplit('/').next().unwrap_or(&blob.entry);
            let history = match History::parse(&file) {
                Ok(Some(h)) => h,
                Ok(None) => {
                    println!("{name}{short}: no history");
                    continue;
                }
                Err(e) => {
                    println!("{name}{short}: history not read: {e}");
                    continue;
                }
            };
            let states: Vec<String> = history
                .states
                .iter()
                .map(|s| {
                    let (c, m, d) = s.counts();
                    format!("{}(+{c} ~{m} -{d})", s.id)
                })
                .collect();
            println!(
                "{name}{short}: {} states: {}",
                states.len(),
                states.join(" ")
            );
            for (record, top) in mitcad_f3d::convert::body_records(&file) {
                if !top {
                    continue;
                }
                for steps in 0..=history.states.len() {
                    let view = history.view(steps);
                    let b = mitcad_f3d::convert::convert_body_at(&file, record, &opt, Some(&view));
                    let state = history
                        .states
                        .get(steps)
                        .map_or("-".to_string(), |s| s.id.to_string());
                    let c = &b.check;
                    println!(
                        "  body {record} back {steps} (state {state}): {} faces {} edges {} clean {} \
                         (free {} unpaired {} open {} mismatched {} gap {:.1e}; skipped faces {}) issues {}{}",
                        if b.body.is_solid() { "solid" } else { "sheet" },
                        b.body.faces.len(),
                        b.body.edges.len(),
                        c.is_clean(),
                        c.free_edges,
                        c.unpaired_coedges,
                        c.open_loops,
                        c.vertex_mismatches,
                        c.max_vertex_gap,
                        b.skipped_faces,
                        b.issues.len(),
                        b.issues
                            .first()
                            .map(|i| format!(" ({i})"))
                            .unwrap_or_default()
                    );
                    for m in c.messages.iter().take(4) {
                        println!("    {m}");
                    }
                }
            }
        }
    }
    Ok(())
}

fn merge(into: &mut BTreeMap<String, usize>, from: &BTreeMap<String, usize>) {
    for (k, v) in from {
        *into.entry(k.clone()).or_insert(0) += v;
    }
}

fn print_totals(t: &Totals) {
    println!(
        "files {} (failed {}), blobs {} (failed {}, truncated {}), bodies {} (solids {}, clean {}, with issues {}), faces {} (skipped {}), degenerate edges {}",
        t.files,
        t.files_failed,
        t.blobs,
        t.blobs_failed,
        t.blobs_truncated,
        t.bodies,
        t.solids,
        t.bodies_clean,
        t.bodies_with_issues,
        t.faces,
        t.faces_skipped,
        t.degenerate_edges
    );
    let show = |title: &str, m: &BTreeMap<String, usize>| {
        if m.is_empty() {
            return;
        }
        println!("{title}:");
        let mut v: Vec<_> = m.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1));
        for (k, n) in v {
            println!("  {n:>8} {k}");
        }
    };
    show("unknown record types", &t.unknown_types);
    show("subtypes", &t.subtypes);
    show("surfaces", &t.surface_kinds);
    show("curves", &t.curve_kinds);
    show("issues", &t.issues);
}

fn cmd_design(path: &str, json: bool) -> Result<(), String> {
    use mitcad_f3d::design::{self, ir};
    let designs = design::decode_path(Path::new(path))?;
    if json {
        let dumps: Vec<Option<&ir::Dump>> = designs.iter().map(|d| d.dump.as_ref()).collect();
        let text = if dumps.len() == 1 {
            serde_json::to_string_pretty(&dumps[0])
        } else {
            serde_json::to_string_pretty(&dumps)
        };
        println!("{}", text.map_err(|e| e.to_string())?);
        return Ok(());
    }
    for d in &designs {
        let Some(dump) = &d.dump else {
            println!("== {}: no design segment", d.label);
            continue;
        };
        let f3d = dump.f3d.as_ref();
        let format = f3d.and_then(|f| f.format.as_ref());
        println!(
            "== {} [{}, writer {}, {} objects]",
            d.label,
            d.segment_dir.as_deref().unwrap_or("?"),
            format.and_then(|f| f.writer.as_deref()).unwrap_or("?"),
            format.and_then(|f| f.objects).unwrap_or(0)
        );
        let items = dump.timeline_items();
        println!("timeline: {} items", items.len());
        for it in items {
            let mut extra = String::new();
            if let Some(ex) = it.f3d.as_ref().and_then(|f| f.extrude.as_ref()) {
                extra = format!(
                    " op={} direction={:?}",
                    ex.operation
                        .clone()
                        .flatten()
                        .unwrap_or_else(|| format!("{}", ex.operation_code.unwrap_or(0))),
                    ex.direction.flatten()
                );
            }
            println!(
                "  {:>4} {:<32} {:<26}{}",
                it.index.unwrap_or(-1),
                it.name().unwrap_or("?"),
                it.object_type().unwrap_or("-"),
                extra
            );
        }
        let params: Vec<&ir::Parameter> = dump.all_parameters().collect();
        println!("parameters: {}", params.len());
        for p in params {
            let owner = match p.created_by.as_ref() {
                Some(ir::Reference::Feature(f)) => f.name.clone().flatten(),
                Some(ir::Reference::SketchDimension(s)) => s
                    .sketch
                    .clone()
                    .flatten()
                    .map(|n| format!("{n}/{}", s.id.clone().flatten().unwrap_or_default())),
                _ if p.role.is_none() => Some("user".into()),
                _ => None,
            };
            println!(
                "  {:<12} = {:<24} {:>14.6} {:<4} {:<24} {}",
                p.name.as_deref().unwrap_or("?"),
                p.expression.as_deref().unwrap_or(""),
                p.value.unwrap_or(f64::NAN),
                p.unit.as_deref().unwrap_or(""),
                p.role.as_deref().unwrap_or(""),
                owner.as_deref().unwrap_or("?")
            );
        }
        for c in dump.components.iter().flatten() {
            println!(
                "component {}{}",
                c.name.clone().flatten().unwrap_or_else(|| "?".into()),
                if c.is_root == Some(true) {
                    " (root)"
                } else {
                    ""
                }
            );
        }
    }
    Ok(())
}

fn run(args: &[String]) -> Result<(), String> {
    match args {
        [cmd, file] if cmd == "list" => cmd_list(file),
        [cmd, file] if cmd == "info" => cmd_info(file),
        [cmd, file, out] if cmd == "extract" => cmd_extract(file, out),
        [cmd, file, entry] if cmd == "dump" => cmd_dump(file, entry, None),
        [cmd, file, entry, n] if cmd == "dump" => cmd_dump(file, entry, n.parse().ok()),
        [cmd, file] if cmd == "history" => cmd_history(file),
        [cmd, file] if cmd == "meshes" => cmd_meshes(file),
        [cmd, file, rest @ ..] if cmd == "bodies" => {
            let mut t = Totals::default();
            process(file, rest.iter().any(|a| a == "-v"), &mut t, true);
            print_totals(&t);
            Ok(())
        }
        [cmd, files @ ..] if cmd == "coverage" && !files.is_empty() => {
            let mut t = Totals::default();
            for f in files {
                process(f, false, &mut t, false);
            }
            print_totals(&t);
            Ok(())
        }
        [cmd, file] if cmd == "design" => cmd_design(file, false),
        [cmd, file, flag] if cmd == "design" && flag == "--json" => cmd_design(file, true),
        _ => Err(USAGE.to_string()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(2)
        }
    }
}
