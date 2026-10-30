// SPDX-License-Identifier: MIT
//! Command-line inspector for .f3d/.f3z files: container contents,
//! ASM records, the conversion of bodies to the neutral B-rep model, the
//! display meshes saved with a document, and the decoded design streams
//! (timeline, parameters, sketches).

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use mitcad_f3d::asm::AsmFile;
use mitcad_f3d::asm::history::History;
use mitcad_f3d::container::F3dFile;
use mitcad_f3d::convert::Options;

const USAGE: &str = "usage: mitcad-f3d-inspect <command> ...
  list <file>                 entries with sizes, methods and kinds
  info <file>                 segments, body blobs and their ASM headers
  assets <file> [-v]          appearance and material assets of the
                              document's packages (-v: every property)
  appearances <file> [--json] the appearances and materials of the bodies,
                              faces and occurrences (a summary)
  presentations <file>        the design stream's objects that name
                              appearance or material libraries
  blob-bodies <file> <entry>  a blob's bodies in its stored state: record,
                              key and their faces' appearance GUIDs
  face-boundary <file> <entry> <guid>
                              the face carrying an appearance GUID: its
                              edges sampled and the point inside it
  extract <file> <dir>        write every entry to <dir> (names that would
                              leave <dir> are refused)
  dump <file> <entry> [n]     ASM records of a body blob as text (first n)
  bodies <file> [-v]          convert every body; per-body report
  history <file>              ASM history of the .smbh blobs: delta states
                              and the bodies rolled back state by state
  meshes <file>               display meshes saved with the document, per body
  coverage <file>...          totals over many files
  design <file> [--json]      decoded design streams: timeline, parameters,
                              components; --json prints the dump IR
                              (core/import/SCHEMA.md; an array for an
                              .f3z package)
  names <file> <entry> [-c]   the named faces of a history blob (its entry
                              name or the end of it) at each state,
                              newest first, with their surface types; -c adds
                              each cylinder's axis, radius and seam
  objects <file> <class> [n]  the design stream's objects of a class (GUID
                              or its first digits; id:<n> for one object)
                              in hex, the first n
  off-timeline <file>...      features of the components' feature lists
                              that the timeline does not list, and the
                              item each is a part of
  edge-curves <file>          the curves the edge inputs store (classes,
                              how many decode), each edge input with how
                              the edge found lies to its curve, and the
                              inputs of other names that found one edge
  attrs <file> <key> [value]  the design stream's objects whose root part
                              has the attribute (with that value), every
                              object of the stream looked at (also those
                              no sketch or feature lists any more)";

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

/// The appearance and material assets of the document's packages
/// (mitcad#228); `-v` lists every property.
fn cmd_assets(path: &str, all: bool) -> Result<(), String> {
    for (name, doc) in documents(path)? {
        if !name.is_empty() {
            println!("== document {name}");
        }
        let package = mitcad_f3d::protein::document_assets(&doc);
        for a in &package.assets {
            println!(
                "{} {} name {:?} library {} ui {:?} category {:?} description {:?}",
                a.schema, a.guid, a.name, a.library, a.ui_name, a.category, a.description
            );
            for (id, p) in &a.properties {
                let interesting = all
                    || !matches!(
                        &p.value,
                        mitcad_f3d::protein::Value::Text(s) if s.is_empty()
                    ) && !id.ends_with("_colorspace");
                if interesting {
                    let links = if p.connections.is_empty() {
                        String::new()
                    } else {
                        format!(" -> {}", p.connections.join(", "))
                    };
                    println!("    {id} = {:?}{links}", p.value);
                }
            }
        }
        for e in &package.errors {
            println!("error: {e}");
        }
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
            let data = doc.read_large(&b.entry).map_err(|e| e.to_string())?;
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
        let Some(p) = entry_path(Path::new(out), &e.name) else {
            eprintln!("{}: not a relative path inside the folder, skipped", e.name);
            failed = true;
            continue;
        };
        match f
            .archive()
            .read_with_limit(e, mitcad_f3d::zip::LARGE_ENTRY_LIMIT)
        {
            Ok(data) => {
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

/// Where `extract` writes an archive entry: its name below `out`, or None
/// for a name that could leave the folder (mitcad#934): an absolute or
/// rooted name, one with a drive or other prefix, or one with a `..`
/// component (with either separator, as an archive may use `\`).
fn entry_path(out: &Path, name: &str) -> Option<PathBuf> {
    if name.is_empty() || name.split(['/', '\\']).any(|c| c == "..") {
        return None;
    }
    let relative = Path::new(name);
    relative
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
        .then(|| out.join(relative))
}

fn cmd_dump(path: &str, entry: &str, max: Option<usize>) -> Result<(), String> {
    let data = open(path)?.read_large(entry).map_err(|e| e.to_string())?;
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

/// The faces of a history blob at each state with their names (mitcad#161:
/// where a feature input's names are): one line per face, `state k`
/// headers from the stored state (0) back.
fn cmd_names(path: &str, entry: &str, cylinders: bool) -> Result<(), String> {
    use mitcad_f3d::names::NamedState;
    for (name, doc) in documents(path)? {
        for blob in doc
            .body_blobs()
            .into_iter()
            .filter(|b| b.entry.ends_with(entry))
        {
            let data = doc.read_large(&blob.entry).map_err(|e| e.to_string())?;
            let file = AsmFile::parse(&data).map_err(|e| e.to_string())?;
            let history = History::parse(&file).map_err(|e| e.to_string())?;
            let steps = history.as_ref().map_or(0, |h| h.states.len());
            println!("{name}{}", blob.entry);
            for k in 0..=steps {
                let view = history.as_ref().map(|h| h.view(k));
                let state = NamedState::new(&file, view);
                let id = history
                    .as_ref()
                    .and_then(|h| h.states.get(k))
                    .map_or("-".to_owned(), |s| s.id.to_string());
                println!(
                    "state {k} (before {id}): {} bodies, {} faces, {} edges",
                    state.bodies.len(),
                    state.faces.len(),
                    state.edges.len()
                );
                for (i, f) in state.faces.iter().enumerate() {
                    let names: Vec<String> = f
                        .names
                        .iter()
                        .map(|n| format!("{}:{}{:?}", n.kind_type, n.tag, n.ops))
                        .collect();
                    println!(
                        "  face {i} (entity {}, body {}) {}: {}",
                        f.entity,
                        f.body,
                        state.surface_kind(i),
                        names.join(" ")
                    );
                    if cylinders && let Ok(c) = state.cylinder(i) {
                        let v = |p: [f64; 3]| format!("({:.4}, {:.4}, {:.4})", p[0], p[1], p[2]);
                        println!(
                            "    cylinder origin {} axis {} r {:.4} seam {} {}{}",
                            v(c.origin),
                            v(c.axis),
                            c.radius,
                            v(c.ref_dir),
                            if c.full { "full" } else { "part" },
                            if c.internal { " internal" } else { "" }
                        );
                    }
                }
                // The closed edges: where each circle's seam vertex lies.
                for i in 0..state.edges.len() {
                    if cylinders
                        && let Ok(g) = state.edge(i)
                        && g.length > 0.0
                        && mitcad_f3d::brep::dist(g.start, g.end) < 1e-6
                    {
                        let v = |p: [f64; 3]| format!("({:.4}, {:.4}, {:.4})", p[0], p[1], p[2]);
                        println!(
                            "  closed edge {i} (entity {}): start {} mid {} length {:.4}",
                            state.edges[i].entity,
                            v(g.start),
                            v(g.mid),
                            g.length
                        );
                    }
                }
            }
        }
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
            let data = doc.read_large(&blob.entry).map_err(|e| e.to_string())?;
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

/// The features of the components' feature lists that the timeline does
/// not list (mitcad#169), one line each: file, object id, type, result
/// number, and the timeline item it is a part of by the design's table of
/// item parts and that item's type (`-` for none).
fn cmd_off_timeline(files: &[String]) -> Result<(), String> {
    use mitcad_f3d::design::{self, classes, decode};
    use std::collections::HashMap;
    for path in files {
        for (label, doc) in design::documents(Path::new(path))? {
            let Some(s) = design::find_design_streams(&doc).map_err(|e| e.to_string())? else {
                continue;
            };
            let seg = design::Segment::parse(&s.meta, s.bulk).map_err(|e| e.to_string())?;
            let dec = decode::decode(&seg);
            let timeline: HashMap<u64, usize> =
                dec.timeline.iter().map(|t| (t.id, t.pos)).collect();
            let part_of: HashMap<u64, u64> = design::build::item_parts(&seg)
                .into_iter()
                .flat_map(|(item, features)| features.into_iter().map(move |f| (f, item)))
                .collect();
            for list in seg.objects_of(classes::FEATURE_LIST) {
                for r in seg.ref_ids(seg.data(list)) {
                    if timeline.contains_key(&r) {
                        continue;
                    }
                    let Some(o) = seg.object(r) else { continue };
                    let Some(name) = seg.guid(o).and_then(classes::known_name) else {
                        continue;
                    };
                    let Some(tail) = decode::feature_tail(&seg, o) else {
                        continue;
                    };
                    let item = part_of.get(&r).and_then(|i| timeline.get(i));
                    let item_type = item.and_then(|&p| dec.timeline[p].type_name).unwrap_or("-");
                    let item = item.map_or("-".to_owned(), ToString::to_string);
                    println!(
                        "{path}{label}\t{r}\t{name}\t{}\t{item}\t{item_type}",
                        tail.result_no
                    );
                }
            }
        }
    }
    Ok(())
}

/// The design stream's objects of one class in hex, for studying a record
/// layout: id, version, length, references (offset and target class) and
/// the bytes, 32 a line.
/// The appearances and materials the design gives its bodies, faces and
/// occurrences (mitcad#228): a summary, or with `--json` the decoded data.
fn cmd_appearances(path: &str, json: bool) -> Result<(), String> {
    use mitcad_f3d::design;
    for (label, doc) in design::documents(Path::new(path))? {
        let d = design::decode_document(&doc, &label)?;
        let Some(a) = d.dump.and_then(|d| d.f3d).and_then(|f| f.appearance) else {
            println!("{label}: no design");
            continue;
        };
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&a).map_err(|e| e.to_string())?
            );
            continue;
        }
        let bodies = a.bodies.as_deref().unwrap_or(&[]);
        let faces: Vec<_> = bodies
            .iter()
            .flat_map(|b| b.faces.iter().flatten())
            .collect();
        let count = |f: &dyn Fn(&mitcad_f3d::design::ir::BodyLook) -> bool| {
            bodies.iter().filter(|b| f(b)).count()
        };
        println!(
            "{label}: bodies {} (in a blob {}, record {}), appearance {} (asset {}), material {} \
             (asset {}); faces {} (found {}, asset {}); face references not found {}; \
             occurrence overrides {}; assets {}; warnings {}",
            bodies.len(),
            count(&|b| b.blob.is_some()),
            count(&|b| b.record.is_some()),
            count(&|b| b.appearance.is_some()),
            count(&|b| b.appearance.as_ref().is_some_and(|x| x.asset.is_some())),
            count(&|b| b.material.is_some()),
            count(&|b| b.material.as_ref().is_some_and(|x| x.asset.is_some())),
            faces.len(),
            faces.iter().filter(|f| f.faces.is_some()).count(),
            faces
                .iter()
                .filter(|f| f.appearance.as_ref().is_some_and(|x| x.asset.is_some()))
                .count(),
            a.faces.as_ref().map_or(0, Vec::len),
            a.occurrences.as_ref().map_or(0, Vec::len),
            a.assets.as_ref().map_or(0, Vec::len),
            a.warnings.as_ref().map_or(0, Vec::len),
        );
        for w in a.warnings.iter().flatten() {
            println!("  warning: {w}");
        }
        for f in a.faces.iter().flatten() {
            println!(
                "  face reference {} {}: {}",
                f.object_id.unwrap_or(0),
                f.guid.as_deref().unwrap_or(""),
                f.why.as_deref().unwrap_or("")
            );
        }
        for b in bodies {
            for f in b.faces.iter().flatten().filter(|f| f.faces.is_none()) {
                println!(
                    "  face {} of body {}: {}",
                    f.guid.as_deref().unwrap_or(""),
                    b.entity.unwrap_or(0),
                    f.why.as_deref().unwrap_or("")
                );
            }
            if b.appearance.as_ref().is_some_and(|x| x.asset.is_none()) {
                println!(
                    "  body {}: appearance {:?} not in the assets",
                    b.entity.unwrap_or(0),
                    b.appearance
                );
            }
        }
    }
    Ok(())
}

/// The bodies of a blob in its stored state: record, key (the design's
/// body map names a body by it) and the appearance GUIDs of its faces.
fn cmd_blob_bodies(path: &str, entry: &str) -> Result<(), String> {
    let f = open(path)?;
    let name = f
        .body_blobs()
        .into_iter()
        .find(|b| b.entry.ends_with(entry))
        .ok_or_else(|| format!("no blob {entry}"))?
        .entry;
    let data = f.read_large(&name).map_err(|e| e.to_string())?;
    let file = AsmFile::parse(&data).map_err(|e| e.to_string())?;
    let state = mitcad_f3d::names::NamedState::new(&file, None);
    let tops = mitcad_f3d::convert::body_records(&file);
    for (k, b) in state.bodies.iter().enumerate() {
        let top = tops
            .iter()
            .find(|(r, _)| *r == b.record)
            .is_some_and(|t| t.1);
        let names: Vec<String> = b
            .names
            .iter()
            .map(|n| format!("{}:{}{:?}", n.kind_type, n.tag, n.ops))
            .collect();
        println!(
            "  top {top} type {} names {}",
            file.records[b.record].type_name,
            names.join(" ")
        );
        let guids: Vec<String> = (0..state.faces.len())
            .filter(|&i| state.faces[i].body == k)
            .filter_map(|i| state.face_appearance_guid(i))
            .collect();
        println!(
            "body {k} record {} key {:?} faces {} appearance GUIDs {}",
            b.record,
            state.body_key(k),
            state.faces.iter().filter(|x| x.body == k).count(),
            guids.join(" ")
        );
    }
    Ok(())
}

/// The face of a blob's stored state whose appearance GUID is `guid`: its
/// surface, the boundary sampled edge by edge (first and last point of
/// each) and the point inside it the import looks for it by.
fn cmd_face_boundary(path: &str, entry: &str, guid: &str) -> Result<(), String> {
    let f = open(path)?;
    let name = f
        .body_blobs()
        .into_iter()
        .find(|b| b.entry.ends_with(entry))
        .ok_or_else(|| format!("no blob {entry}"))?
        .entry;
    let data = f.read_large(&name).map_err(|e| e.to_string())?;
    let file = AsmFile::parse(&data).map_err(|e| e.to_string())?;
    let state = mitcad_f3d::names::NamedState::new(&file, None);
    for i in 0..state.faces.len() {
        if state.face_appearance_guid(i).as_deref() != Some(guid) {
            continue;
        }
        let face = &state.faces[i];
        let body = state.bodies[face.body].record;
        let s = mitcad_f3d::convert::face_surface(
            &file,
            body,
            face.entity,
            &mitcad_f3d::convert::Options::default(),
            None,
            16,
        )
        .map_err(|e| e.0)?;
        println!("face {} surface {}", face.entity, s.surface.kind());
        for (k, edge) in s.boundary.chunks(16).enumerate() {
            println!(
                "  edge {k}: {:?} .. {:?} .. {:?}",
                edge.first(),
                edge.get(edge.len() / 2),
                edge.last()
            );
        }
        println!("  plane {:?}", state.plane(i));
    }
    Ok(())
}

/// The design stream's objects that name appearance or material libraries
/// (mitcad#228): their UTF-16 strings and references, in order.
fn cmd_presentations(path: &str) -> Result<(), String> {
    use mitcad_f3d::design;
    const LIBRARIES: [&str; 3] = [
        "BA5EE55E-9982-449B-9D66-9F036540E140",
        "C1EEA57C-3F56-45FC-B8CB-A9EC46A9994C",
        "08861000-1D69-CF2A-C082-CBD98E7E5D7F",
    ];
    for (label, doc) in design::documents(Path::new(path))? {
        let Some(s) = design::find_design_streams(&doc).map_err(|e| e.to_string())? else {
            continue;
        };
        let seg = design::Segment::parse(&s.meta, s.bulk).map_err(|e| e.to_string())?;
        for o in &seg.objects {
            let d = seg.data(o);
            let mut items: Vec<(usize, String)> = Vec::new();
            let mut p = 0;
            while p + 4 < d.len() {
                let n = u32::from_le_bytes([d[p], d[p + 1], d[p + 2], d[p + 3]]) as usize;
                if (1..=200).contains(&n) && p + 4 + 2 * n <= d.len() {
                    let units: Vec<u16> = d[p + 4..p + 4 + 2 * n]
                        .chunks(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect();
                    if let Ok(s) = String::from_utf16(&units)
                        && s.chars().all(|c| (' '..='~').contains(&c))
                    {
                        items.push((p, s));
                        p += 4 + 2 * n;
                        continue;
                    }
                }
                p += 1;
            }
            if !items.iter().any(|(_, s)| LIBRARIES.contains(&s.as_str())) {
                continue;
            }
            println!(
                "== {label} object {} class {} v{} @{} len {} name {:?}",
                o.id,
                seg.guid(o).unwrap_or("?"),
                seg.version(o).unwrap_or(0),
                o.offset,
                d.len(),
                seg.header(o).map(|h| h.name).unwrap_or_default()
            );
            for (p, r) in seg.refs_in(d, 0, d.len()) {
                let t = seg.guid_of(r.id).unwrap_or("?");
                println!("  ref @{p} -> {} {}", r.id, &t[..8.min(t.len())]);
            }
            for (p, s) in items {
                println!("  str @{p} {s}");
            }
        }
    }
    Ok(())
}

/// The curves the edge inputs store (mitcad#306): per curve class how many
/// decode, then per edge input of the timeline (item, input object, curve,
/// how the edge found lies to it, where it was found), then the inputs of
/// one item with other names that found the same edge.
fn cmd_edge_curves(path: &str) -> Result<(), String> {
    use mitcad_f3d::design::{self, classes, curve, recipe};
    use serde_json::Value;
    for (label, doc) in design::documents(Path::new(path))? {
        let Some(s) = design::find_design_streams(&doc).map_err(|e| e.to_string())? else {
            continue;
        };
        let seg = design::Segment::parse(&s.meta, s.bulk).map_err(|e| e.to_string())?;
        let mut classes_seen: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        let mut without = 0;
        for o in seg.objects_of(classes::FACE_REF) {
            if recipe::of_input(&seg, o.id).is_none_or(|r| r.kind != "edge") {
                continue;
            }
            let ids = curve::ids_of_input(&seg, o.id);
            if ids.is_empty() {
                without += 1;
            }
            for id in ids {
                let g = seg.guid_of(id).unwrap_or("?").to_owned();
                let e = classes_seen.entry(g).or_default();
                match curve::decode(&seg, id) {
                    Some(_) => e.0 += 1,
                    None => e.1 += 1,
                }
            }
        }
        println!("== {label}");
        for (g, (ok, bad)) in &classes_seen {
            println!("curve-class {g} decoded {ok} not {bad}");
        }
        println!("edge-inputs-without-curve {without}");
        let fd = design::decode_document(&doc, &label)?;
        let Some(dump) = fd.dump else { continue };
        let items = dump.timeline_items();
        for it in items {
            let value = serde_json::to_value(it).map_err(|e| e.to_string())?;
            let mut edges: Vec<&serde_json::Map<String, Value>> = Vec::new();
            fn walk<'v>(v: &'v Value, out: &mut Vec<&'v serde_json::Map<String, Value>>) {
                match v {
                    Value::Object(m) => {
                        if m.get("objectType").and_then(Value::as_str) == Some("BRepEdge")
                            && m.get("_f3d").and_then(|f| f.get("object_id")).is_some()
                        {
                            out.push(m);
                        }
                        m.values().for_each(|x| walk(x, out));
                    }
                    Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
                    _ => {}
                }
            }
            walk(&value, &mut edges);
            let index = it.index.unwrap_or(-1);
            let name = it.name().unwrap_or("?");
            let text = |v: Option<&Value>| match v {
                Some(Value::String(s)) => s.clone(),
                Some(v) => v.to_string(),
                None => "-".to_owned(),
            };
            let mut by_place: BTreeMap<String, Vec<(String, String, String)>> = BTreeMap::new();
            for m in &edges {
                let f = m.get("_f3d").expect("has _f3d");
                let object = text(f.get("object_id"));
                // Each curve's type (several where the input stores more).
                let curve = f
                    .get("curves")
                    .and_then(Value::as_array)
                    .map(|c| {
                        c.iter()
                            .map(|c| text(c.get("type")))
                            .collect::<Vec<_>>()
                            .join("+")
                    })
                    .unwrap_or_else(|| "none".to_owned());
                let place = match (m.get("mid_point"), m.get("length")) {
                    (Some(Value::Array(p)), Some(l)) => {
                        let r = |v: &Value| format!("{:.5}", v.as_f64().unwrap_or(f64::NAN));
                        Some(format!("{} {} {} l {}", r(&p[0]), r(&p[1]), r(&p[2]), r(l)))
                    }
                    _ => None,
                };
                println!(
                    "input item {index} {name:?} object {object} curve {curve} fit {:?} at {} found {:?}",
                    text(f.get("curve_fit")),
                    place.as_deref().unwrap_or("-"),
                    text(f.get("found")),
                );
                if let Some(place) = place {
                    by_place.entry(place).or_default().push((
                        object,
                        text(f.get("entities")),
                        text(f.get("curves")),
                    ));
                }
            }
            for (place, inputs) in by_place {
                let names: std::collections::BTreeSet<&String> =
                    inputs.iter().map(|i| &i.1).collect();
                if names.len() < 2 {
                    continue;
                }
                let curves: std::collections::BTreeSet<&String> =
                    inputs.iter().map(|i| &i.2).collect();
                let objects: Vec<&str> = inputs.iter().map(|i| i.0.as_str()).collect();
                println!(
                    "same-edge item {index} {name:?} at {place} inputs {} names {} curves {} objects {}",
                    inputs.len(),
                    names.len(),
                    curves.len(),
                    objects.join(",")
                );
            }
        }
    }
    Ok(())
}

/// The objects whose root part has the attribute `key` (equal to `value`
/// when given): a sketch curve's ids (`crv_primary_id`) are found this way
/// also in objects nothing refers to any more.
fn cmd_attrs(path: &str, key: &str, value: Option<&str>) -> Result<(), String> {
    use mitcad_f3d::design;
    use mitcad_f3d::design::stream::AttrValue;
    let mut found = 0;
    for (label, doc) in design::documents(Path::new(path))? {
        let Some(s) = design::find_design_streams(&doc).map_err(|e| e.to_string())? else {
            continue;
        };
        let seg = design::Segment::parse(&s.meta, s.bulk).map_err(|e| e.to_string())?;
        for o in &seg.objects {
            let Some(root) = seg.root_part(seg.data(o)) else {
                continue;
            };
            let Some(a) = root.attrs.iter().find(|a| a.key == key) else {
                continue;
            };
            let text = match &a.value {
                AttrValue::U64(v) => v.to_string(),
                AttrValue::Bool(v) => v.to_string(),
                AttrValue::Text(t) => t.clone(),
            };
            if value.is_some_and(|v| v != text) {
                continue;
            }
            found += 1;
            let attrs: Vec<String> = root
                .attrs
                .iter()
                .map(|a| match &a.value {
                    AttrValue::U64(v) => format!("{}={v}", a.key),
                    AttrValue::Bool(v) => format!("{}={v}", a.key),
                    AttrValue::Text(t) => format!("{}={t:?}", a.key),
                })
                .collect();
            let refs: Vec<String> = root
                .refs
                .iter()
                .map(|r| r.map_or_else(|| "-".to_owned(), |r| r.to_string()))
                .collect();
            println!(
                "{label} object {} class {} refs [{}] {}",
                o.id,
                seg.guid(o).unwrap_or("?"),
                refs.join(","),
                attrs.join(" ")
            );
        }
    }
    println!("{found} objects");
    Ok(())
}

fn cmd_objects(path: &str, class: &str, max: Option<usize>) -> Result<(), String> {
    use mitcad_f3d::design;
    let prefix = class.to_uppercase();
    for (label, doc) in design::documents(Path::new(path))? {
        let Some(s) = design::find_design_streams(&doc).map_err(|e| e.to_string())? else {
            continue;
        };
        let seg = design::Segment::parse(&s.meta, s.bulk).map_err(|e| e.to_string())?;
        let mut shown = 0;
        let id = prefix
            .strip_prefix("ID:")
            .and_then(|s| s.parse::<u64>().ok());
        for o in &seg.objects {
            let Some(g) = seg.guid(o) else { continue };
            if id.map_or(!g.starts_with(&prefix), |id| o.id != id) {
                continue;
            }
            if max.is_some_and(|m| shown >= m) {
                break;
            }
            shown += 1;
            let d = seg.data(o);
            println!(
                "== {label} object {} class {g} v{} len {} sub {}",
                o.id,
                seg.version(o).unwrap_or(0),
                d.len(),
                seg.sub_start(o)
            );
            // The class and its bases with their versions (each its own
            // part of the object's fields).
            let chain: Vec<String> = seg
                .chain(g)
                .iter()
                .map(|c| {
                    let v = seg.class_by_guid(c).map_or(0, |r| r.version);
                    format!("{c} v{v}")
                })
                .collect();
            println!("  chain {}", chain.join(" < "));
            for (p, r) in seg.refs_in(d, 0, d.len()) {
                let t = seg.guid_of(r.id).unwrap_or("?");
                println!("  ref @{p}..{} -> {} {}", r.end, r.id, &t[..8.min(t.len())]);
            }
            // References into other documents: the object's id there, the
            // document's key and the object's class.
            for (p, r) in seg.context_refs_in(d, 0, d.len()) {
                println!(
                    "  other @{p}..{} -> {} in {:016x} {}",
                    r.end,
                    r.id,
                    r.key,
                    &r.guid[..8.min(r.guid.len())]
                );
            }
            for (i, chunk) in d.chunks(32).enumerate() {
                let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
                println!("  {:5} {}", i * 32, hex.join(" "));
            }
        }
    }
    Ok(())
}

fn run(args: &[String]) -> Result<(), String> {
    match args {
        [cmd, file] if cmd == "list" => cmd_list(file),
        [cmd, file] if cmd == "info" => cmd_info(file),
        [cmd, file] if cmd == "assets" => cmd_assets(file, false),
        [cmd, file] if cmd == "presentations" => cmd_presentations(file),
        [cmd, file, entry] if cmd == "blob-bodies" => cmd_blob_bodies(file, entry),
        [cmd, file, entry, guid] if cmd == "face-boundary" => cmd_face_boundary(file, entry, guid),
        [cmd, file] if cmd == "appearances" => cmd_appearances(file, false),
        [cmd, file, flag] if cmd == "appearances" && flag == "--json" => {
            cmd_appearances(file, true)
        }
        [cmd, file, flag] if cmd == "assets" && flag == "-v" => cmd_assets(file, true),
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
        [cmd, file, entry] if cmd == "names" => cmd_names(file, entry, false),
        [cmd, file, entry, flag] if cmd == "names" && flag == "-c" => cmd_names(file, entry, true),
        [cmd, files @ ..] if cmd == "off-timeline" && !files.is_empty() => cmd_off_timeline(files),
        [cmd, file, class] if cmd == "objects" => cmd_objects(file, class, None),
        [cmd, file, class, n] if cmd == "objects" => cmd_objects(file, class, n.parse().ok()),
        [cmd, file] if cmd == "edge-curves" => cmd_edge_curves(file),
        [cmd, file, key] if cmd == "attrs" => cmd_attrs(file, key, None),
        [cmd, file, key, value] if cmd == "attrs" => cmd_attrs(file, key, Some(value)),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// An archive entry named `../escape.txt` is not written beside the
    /// output folder: `extract` fails and writes the other entries
    /// (mitcad#934).
    #[test]
    fn extract_refuses_names_that_leave_the_folder() {
        let dir =
            std::env::temp_dir().join(format!("mitcad-f3d-inspect-extract-{}", std::process::id()));
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let escaped = dir.join("escape.txt");
        let _ = std::fs::remove_file(&escaped);
        let archive = dir.join("escape.f3d");
        let zip = mitcad_f3d::zip::stored_zip(&[
            ("inside.txt", &b"in"[..]),
            ("../escape.txt", &b"out"[..]),
        ]);
        std::fs::write(&archive, zip).unwrap();
        let result = cmd_extract(archive.to_str().unwrap(), out.to_str().unwrap());
        assert!(result.is_err());
        assert!(!escaped.exists());
        assert_eq!(std::fs::read(out.join("inside.txt")).unwrap(), b"in");

        let base = Path::new("out");
        for bad in ["", "../x", "a/../../x", "a\\..\\x", "/etc/x", "\\x"] {
            if bad == "\\x" && cfg!(not(windows)) {
                // A plain file name on Unix; rooted on Windows.
                continue;
            }
            assert_eq!(entry_path(base, bad), None, "{bad}");
        }
        assert_eq!(
            entry_path(base, "a/b.txt"),
            Some(base.join("a").join("b.txt"))
        );
    }
}
