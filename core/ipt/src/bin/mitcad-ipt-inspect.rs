// SPDX-License-Identifier: MIT
//! Command-line inspector for `.ipt` files: the compound file's entries,
//! the property sets, the segments with their record tables, and the
//! B-rep records with the bodies converted to the neutral B-rep model.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use mitcad_ipt::cfb::{EntryKind, guid_text};
use mitcad_ipt::{IptFile, read_bodies};

const USAGE: &str = "usage: mitcad-ipt-inspect <command> ...
  list <file>                   entries of the compound file with sizes
  properties <file>             property sets with their values
  segments <file>               segments: records, types, compression
  records <file> <segment>      a segment's records: index, type, size
  record <file> <segment> <n>   one record's bytes (hex) and its trailer
  data <file> <segment> <from> <len>
                                the block table, and the decompressed data
                                stream from <from> (hex, 0x...)
  bodies <file> [-v|-f]         the B-rep records and their bodies; -f
                                each face: surface, box (mm), loops
  result <file>                 the result segment's body list (shown,
                                solid, range) and the top-level bodies
  asm <file> <segment> <n> [m] the ASM data of a record as text (its
                                first m records), from its 14th byte
  design <file> [--json]        the timeline (features, history states)
                                and parameter summaries; --json the dump
  slots <file> <kind>           the property slots of the timeline's
                                features of an object type (or *)
  profile <file> <n>            a profile selection's wire body
  parameters <file>             the parameters: expressions, units, values
  typed <file> <type>           the definitions records whose type starts
                                with <type> (hex): header | body bytes
  show <file> <n>               a definitions record's bytes and the
                                references in it with their types
  refs <file> <n>               the definitions records that refer to
                                record <n>
  expr <file> <n>               a parameter's expression records as a tree
  owners <file>                 the table's parameters with the types of
                                the records that refer to them
  planedefs <file>              the work planes and the records that
                                define them
  constraints <file> <type>     the sketch constraints of a type: their
                                parameter and entities with geometry
  prefix <file>                 checks the records' prefix after the header
  flags <file>                  sketch lines on the selected profiles'
                                boundaries, by their entity flags
  history <file>                the ASM history of the B-rep records: its
                                states and the bodies rolled back
  write-test <out.ipt>          write the test part of mitcad_ipt::testdata
  assembly <file>               an assembly's referenced files and its
                                occurrences: keys, flags, files,
                                placements, range boxes
  stream <file> <path> [--raw]  a stream's bytes (hex, or raw)
  record <file> <seg> <n> --raw a record's bytes, raw
  dump <file> <segment>         every record of a segment (hex)
  labels <file> <segment>       the browser labels of a segment's records";

fn open(path: &str) -> Result<IptFile, String> {
    IptFile::open(Path::new(path)).map_err(|e| format!("{path}: {e}"))
}

fn printable(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_control() {
                format!("\\x{:02x}", c as u32)
            } else {
                c.to_string()
            }
        })
        .collect()
}

fn cmd_list(path: &str) -> Result<(), String> {
    let f = open(path)?;
    println!(
        "version {}, root class {}{}",
        f.container.version,
        guid_text(&f.container.root().clsid),
        if f.is_part() { " (part)" } else { "" }
    );
    for (p, e) in f.container.entries() {
        let kind = match e.kind {
            EntryKind::Root => "root",
            EntryKind::Storage => "storage",
            EntryKind::Stream => "stream",
        };
        println!(
            "{:>10} {:<8} {}",
            e.size,
            kind,
            printable(if p.is_empty() { "/" } else { p })
        );
    }
    Ok(())
}

fn cmd_properties(path: &str) -> Result<(), String> {
    let f = open(path)?;
    for (stream, sets) in &f.properties {
        for s in sets {
            println!(
                "{} {{{}}} code page {:?}",
                printable(stream),
                guid_text(&s.fmtid),
                s.code_page
            );
            for (id, v) in &s.values {
                let name = s.names.get(id).map(|n| format!(" {n}")).unwrap_or_default();
                println!("  {id:>10}{name}: {v}");
            }
        }
    }
    println!("document: {:?}", f.document());
    for w in &f.warnings {
        println!("warning: {w}");
    }
    Ok(())
}

fn segment<'a>(f: &'a IptFile, name: &str) -> Result<&'a mitcad_ipt::Segment, String> {
    f.segments
        .iter()
        .find(|s| s.name == name || s.stream == name)
        .ok_or_else(|| format!("no segment {name}"))
}

fn cmd_segments(path: &str) -> Result<(), String> {
    let f = open(path)?;
    for s in &f.segments {
        let content = f.segment_data(s).map_err(|e| e.to_string())?;
        let records = match &content.records {
            Ok(r) => format!("{} records", r.len()),
            Err(e) => format!("records not split: {e}"),
        };
        println!(
            "{:<20} {{{}}} {} major {} stamps {:?} kind {}, {} types, {} bytes ({}), {}",
            s.name,
            guid_text(&s.id),
            s.meta.tag,
            s.meta.major(),
            s.meta.stamps,
            content.tables.kind,
            content.tables.types.len(),
            content.data.len(),
            content.compression,
            records
        );
    }
    for w in &f.warnings {
        println!("warning: {w}");
    }
    Ok(())
}

fn cmd_records(path: &str, name: &str) -> Result<(), String> {
    let f = open(path)?;
    let content = f
        .segment_data(segment(&f, name)?)
        .map_err(|e| e.to_string())?;
    for (i, t) in content.tables.types.iter().enumerate() {
        println!("type {i:>3} {} {:?}", hex(&t.id), t.fields);
    }
    let records = content
        .records
        .map_err(|e| format!("records not split: {e}"))?;
    let mut per_type: BTreeMap<usize, usize> = BTreeMap::new();
    for r in &records {
        *per_type.entry(r.type_index()).or_default() += 1;
        println!(
            "record {:>6} type {:>3} selector {:#06x} {:>9} bytes",
            r.index,
            r.type_index(),
            r.selector,
            r.range.len()
        );
    }
    println!("records per type: {per_type:?}");
    Ok(())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn cmd_record(path: &str, name: &str, index: &str) -> Result<(), String> {
    let f = open(path)?;
    let content = f
        .segment_data(segment(&f, name)?)
        .map_err(|e| e.to_string())?;
    let records = content
        .records
        .map_err(|e| format!("records not split: {e}"))?;
    let i: usize = index
        .parse()
        .map_err(|_| format!("bad record number {index}"))?;
    let r = records.get(i).ok_or_else(|| format!("no record {i}"))?;
    dump(&content.data[r.range.clone()], 0);
    if !r.trailer.is_empty() {
        println!("extended trailer:");
        dump(&content.data[r.trailer.clone()], r.trailer.start);
    }
    Ok(())
}

/// A segment's decompressed data stream (hex) from `from`, `len` bytes,
/// after its block table.
fn cmd_data(path: &str, name: &str, from: usize, len: usize) -> Result<(), String> {
    let f = open(path)?;
    let content = f
        .segment_data(segment(&f, name)?)
        .map_err(|e| e.to_string())?;
    let blocks: Vec<String> = content
        .tables
        .blocks
        .iter()
        .map(|b| format!("{}{}", b & 0x7FFF_FFFF, if b >> 31 == 0 { "*" } else { "" }))
        .collect();
    println!(
        "slots {}, kind {}, {} blocks: {}; {} bytes",
        content.tables.slots,
        content.tables.kind,
        blocks.len(),
        blocks.join(" "),
        content.data.len()
    );
    let end = from.saturating_add(len).min(content.data.len());
    dump(&content.data[from.min(end)..end], from);
    Ok(())
}

fn dump(data: &[u8], base: usize) {
    for (k, chunk) in data.chunks(16).enumerate() {
        let text: String = chunk
            .iter()
            .map(|&c| {
                if (32..127).contains(&c) {
                    c as char
                } else {
                    '.'
                }
            })
            .collect();
        println!(
            "{:08x} {:<48} {text}",
            base + 16 * k,
            chunk
                .iter()
                .map(|x| format!("{x:02x} "))
                .collect::<String>()
        );
    }
}

fn dump_hex(data: &[u8]) {
    for (k, chunk) in data.chunks(16).enumerate() {
        let text: String = chunk
            .iter()
            .map(|&c| {
                if (32..127).contains(&c) {
                    c as char
                } else {
                    '.'
                }
            })
            .collect();
        println!(
            "{:08x} {:<48} {text}",
            16 * k,
            chunk
                .iter()
                .map(|x| format!("{x:02x} "))
                .collect::<String>()
        );
    }
}

fn cmd_dump(path: &str, name: &str) -> Result<(), String> {
    let f = open(path)?;
    let content = f
        .segment_data(segment(&f, name)?)
        .map_err(|e| e.to_string())?;
    let records = content
        .records
        .map_err(|e| format!("records not split: {e}"))?;
    for r in &records {
        let t = &content.tables.types[r.type_index()];
        println!(
            "== record {} type {} {} {} bytes",
            r.index,
            r.type_index(),
            hex(&t.id),
            r.range.len()
        );
        dump_hex(&content.data[r.range.clone()]);
    }
    Ok(())
}

fn cmd_assembly(path: &str) -> Result<(), String> {
    use mitcad_ipt::assembly;
    let f = open(path)?;
    if !f.is_assembly() {
        println!(
            "warning: the root class is {}, not an assembly's",
            guid_text(&f.container.root().clsid)
        );
    }
    let a = assembly::read(&f).map_err(|e| e.to_string())?;
    println!(
        "saved as {}; {:?}",
        a.saved_path.as_deref().unwrap_or("?"),
        assembly::summary(&a)
    );
    for r in &a.references {
        println!(
            "file {:>4} {}{}{}",
            r.id,
            r.path,
            if r.library.is_empty() {
                String::new()
            } else {
                format!(" [{}: {}]", r.library, r.member)
            },
            r.variant
                .as_ref()
                .map(|v| format!(" variant {v}"))
                .unwrap_or_default()
        );
    }
    for o in &a.occurrences {
        let reference = o.reference.and_then(|id| a.reference(id));
        let t = o.transform.map(|m| {
            format!(
                "[{:.4} {:.4} {:.4} | {:.4} {:.4} {:.4} | {:.4} {:.4} {:.4}] + ({:.4}, {:.4}, {:.4})",
                m[0][0], m[0][1], m[0][2], m[1][0], m[1][1], m[1][2], m[2][0], m[2][1], m[2][2],
                m[0][3], m[1][3], m[2][3]
            )
        });
        println!(
            "occ {:>6} key {:>5} {:<32} flags {:#010x}{}{}{} file {:?} {} model state {:?} range {:?} center {:?} identity {}",
            o.record,
            o.key.map_or("-".to_owned(), |k| k.to_string()),
            o.name(reference),
            o.flags,
            if o.suppressed() { " suppressed" } else { "" },
            if o.hidden() { " hidden" } else { "" },
            if o.grounded() { " grounded" } else { "" },
            o.reference,
            t.unwrap_or_else(|| "no placement".to_owned()),
            o.model_state,
            o.range,
            o.center,
            o.identity_checked
        );
    }
    for w in &a.warnings {
        println!("warning: {w}");
    }
    Ok(())
}

fn cmd_labels(path: &str, name: &str) -> Result<(), String> {
    let f = open(path)?;
    let s = segment(&f, name)?;
    let content = f.segment_data(s).map_err(|e| e.to_string())?;
    let records = content
        .records
        .map_err(|e| format!("records not split: {e}"))?;
    let dc =
        mitcad_ipt::dc::Definitions::new(s.meta.major(), content.data, &content.tables, &records);
    let mut labels: Vec<_> = dc.labels().into_iter().collect();
    labels.sort_by_key(|(owner, _)| *owner);
    for (owner, l) in labels {
        println!(
            "{owner:>6} {} {:?} label {} class {} children {:?}",
            dc.type_of(owner).map(|t| hex(t)).unwrap_or_default(),
            l.name,
            l.record,
            hex(&l.class),
            l.children
        );
    }
    Ok(())
}

fn cmd_stream(path: &str, stream: &str, raw: bool) -> Result<(), String> {
    use std::io::Write;
    let f = open(path)?;
    let stream = if stream.starts_with('/') {
        stream.to_string()
    } else {
        format!("/{stream}")
    };
    let data = f.container.read_path(&stream).map_err(|e| e.to_string())?;
    if raw {
        std::io::stdout()
            .write_all(&data)
            .map_err(|e| e.to_string())?;
    } else {
        dump_hex(&data);
    }
    Ok(())
}

fn cmd_raw_record(path: &str, name: &str, index: &str) -> Result<(), String> {
    use std::io::Write;
    let f = open(path)?;
    let content = f
        .segment_data(segment(&f, name)?)
        .map_err(|e| e.to_string())?;
    let records = content
        .records
        .map_err(|e| format!("records not split: {e}"))?;
    let i: usize = index
        .parse()
        .map_err(|_| format!("bad record number {index}"))?;
    let r = records.get(i).ok_or_else(|| format!("no record {i}"))?;
    std::io::stdout()
        .write_all(&content.data[r.range.clone()])
        .map_err(|e| e.to_string())
}

fn cmd_bodies(path: &str, verbose: bool, faces: bool) -> Result<(), String> {
    let f = open(path)?;
    let records = f.brep_records().map_err(|e| e.to_string())?;
    for (record, result) in records.iter().zip(read_bodies(&records)) {
        let blob = result?;
        let h = &blob.header;
        println!(
            "{}: {} bytes, {} (save version {}, flags {}, units {} mm), {} bodies{}",
            record.place(),
            record.asm.len(),
            h.asm_version,
            h.version,
            h.flags,
            h.units,
            blob.bodies.len(),
            blob.truncated
                .as_ref()
                .map(|t| format!(", truncated: {t}"))
                .unwrap_or_default()
        );
        if !blob.coverage.unknown_types.is_empty() {
            println!("  unknown record types: {:?}", blob.coverage.unknown_types);
        }
        for b in &blob.bodies {
            let body = &b.body;
            println!(
                "  body #{}{}: {} faces, {} edges, {} vertices; issues {}, skipped faces {}, check clean {}",
                b.record,
                if b.top_level { "" } else { " (owner)" },
                body.faces.len(),
                body.edges.len(),
                body.vertices.len(),
                b.issues.len(),
                b.skipped_faces,
                b.check.is_clean()
            );
            if faces {
                for (i, face) in body.faces.iter().enumerate() {
                    let mut lo = [f64::INFINITY; 3];
                    let mut hi = [f64::NEG_INFINITY; 3];
                    let mut loops = Vec::new();
                    for lp in &face.loops {
                        let mut kinds = Vec::new();
                        for c in lp {
                            let e = &body.edges[c.edge];
                            for &v in &e.vertices {
                                let p = body.vertices[v].point;
                                for k in 0..3 {
                                    lo[k] = lo[k].min(p[k]);
                                    hi[k] = hi[k].max(p[k]);
                                }
                            }
                            kinds.push(format!(
                                "e{}{}:{}[{:.4},{:.4}]v{}-{}({:.3},{:.3},{:.3})",
                                c.edge,
                                if c.forward { "+" } else { "-" },
                                body.curves[e.curve].kind(),
                                e.t[0],
                                e.t[1],
                                e.vertices[0],
                                e.vertices[1],
                                body.vertices[e.vertices[0]].point[0],
                                body.vertices[e.vertices[0]].point[1],
                                body.vertices[e.vertices[0]].point[2]
                            ));
                        }
                        for c in lp {
                            let curve = &body.curves[body.edges[c.edge].curve];
                            if matches!(curve, mitcad_f3d::brep::Curve::Ellipse { .. }) {
                                kinds.push(format!(
                                    "
        e{} {curve:?}",
                                    c.edge
                                ));
                            }
                        }
                        loops.push(kinds.join(" "));
                    }
                    println!(
                        "    face {i} {}{} box [{:.3} {:.3} {:.3}] [{:.3} {:.3} {:.3}]",
                        body.surfaces[face.surface].kind(),
                        if face.reversed { " reversed" } else { "" },
                        lo[0],
                        lo[1],
                        lo[2],
                        hi[0],
                        hi[1],
                        hi[2]
                    );
                    for l in loops {
                        println!("      loop {l}");
                    }
                }
            }
            if verbose {
                let points: usize = body.faces.iter().map(|f| f.point_loops.len()).sum();
                println!("    point loops: {points}");
                for i in &b.issues {
                    println!("    issue: {i}");
                }
                for m in &b.check.messages {
                    println!("    check: {m}");
                }
            }
        }
    }
    Ok(())
}

/// The result segment's body list and the B-rep record's top-level bodies
/// (faces, solid or sheet), one line each.
fn cmd_result(path: &str) -> Result<(), String> {
    let f = open(path)?;
    match f.result_bodies().map_err(|e| e.to_string())? {
        None => println!("no result body list"),
        Some(list) => {
            println!("result list: {} entries", list.len());
            for b in &list {
                println!(
                    "  node {:>6} {:<7} {:<8} kind {} members {:?} features {:?} range [{:.4} {:.4} {:.4}] [{:.4} {:.4} {:.4}]",
                    b.node,
                    if b.visible { "shown" } else { "hidden" },
                    if b.solid { "solid" } else { "surfaces" },
                    b.kind,
                    b.members,
                    b.features,
                    b.range[0][0],
                    b.range[0][1],
                    b.range[0][2],
                    b.range[1][0],
                    b.range[1][1],
                    b.range[1][2],
                );
            }
        }
    }
    let records = f.brep_records().map_err(|e| e.to_string())?;
    for (record, result) in records.iter().zip(read_bodies(&records)) {
        let blob = result?;
        for b in blob.bodies.iter().filter(|b| b.top_level) {
            println!(
                "  {}/{}: {} faces, {} lumps, {}",
                record.place(),
                b.record,
                b.body.faces.len(),
                b.body.lumps.len(),
                if b.body.faces.is_empty() {
                    "no faces"
                } else if b.body.is_solid() {
                    "solid"
                } else {
                    "sheet"
                }
            );
        }
    }
    Ok(())
}

fn cmd_history(path: &str) -> Result<(), String> {
    use mitcad_f3d::asm::AsmFile;
    use mitcad_f3d::asm::history::History;
    let f = open(path)?;
    let options = mitcad_ipt::convert_options();
    for record in f.brep_records().map_err(|e| e.to_string())? {
        let file = AsmFile::parse(&record.asm).map_err(|e| e.to_string())?;
        let history = match History::parse(&file) {
            Ok(Some(h)) => h,
            Ok(None) => {
                println!("{}: no history", record.place());
                continue;
            }
            Err(e) => {
                println!("{}: history not read: {e}", record.place());
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
            "{}: {} states: {}",
            record.place(),
            states.len(),
            states.join(" ")
        );
        let deleted = mitcad_ipt::deleted_bodies(&history, &file);
        let bodies = mitcad_f3d::convert::body_records(&file)
            .into_iter()
            .filter(|(_, top)| *top)
            .map(|(b, _)| (b, None))
            .chain(deleted.iter().map(|&(c, j)| (c, Some(j))));
        for (body, deleted_by) in bodies {
            if let Some(j) = deleted_by {
                println!("  body {body} deleted by state {}", history.states[j].id);
            }
            for steps in 0..=history.states.len() {
                let view = history.view(steps);
                if deleted_by.is_some_and(|j| steps <= j) || view.get(&body) == Some(&None) {
                    println!("  body {body} back {steps}: absent");
                    continue;
                }
                let b = mitcad_f3d::convert::convert_body_at(&file, body, &options, Some(&view));
                let state = history
                    .states
                    .get(steps)
                    .map_or("-".to_string(), |s| s.id.to_string());
                println!(
                    "  body {body} back {steps} (state {state}): {} {} faces {} edges, clean {}, issues {}",
                    if b.body.is_solid() { "solid" } else { "sheet" },
                    b.body.faces.len(),
                    b.body.edges.len(),
                    b.check.is_clean(),
                    b.issues.len()
                );
            }
        }
    }
    Ok(())
}

fn cmd_asm(path: &str, name: &str, index: &str, max: Option<usize>) -> Result<(), String> {
    use mitcad_f3d::asm::AsmFile;
    let f = open(path)?;
    let content = f
        .segment_data(segment(&f, name)?)
        .map_err(|e| e.to_string())?;
    let records = content
        .records
        .map_err(|e| format!("records not split: {e}"))?;
    let i: usize = index
        .parse()
        .map_err(|_| format!("bad record number {index}"))?;
    let r = records.get(i).ok_or_else(|| format!("no record {i}"))?;
    let data = content.data[r.range.clone()]
        .get(mitcad_ipt::BREP_RECORD_HEAD..)
        .ok_or("a record too short for ASM data")?;
    let file = AsmFile::parse(data).map_err(|e| e.to_string())?;
    println!("{:?}", file.header);
    let n = max.unwrap_or(usize::MAX).min(file.records.len());
    for k in 0..n {
        println!("{}", file.record_text(k, 4000));
    }
    if let Some(t) = &file.truncated {
        println!("stopped: {t}");
    }
    Ok(())
}

fn cmd_design(path: &str, json: bool) -> Result<(), String> {
    let f = open(path)?;
    let label = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path);
    let design = mitcad_ipt::design::read(&f, label)
        .map_err(|e| e.to_string())?
        .ok_or("no definitions segment")?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&design.dump).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    println!(
        "{} items, {} with a history state; parameters {}; expressions {}",
        design.items.len(),
        design.features(),
        serde_json::to_string(&design.parameters).map_err(|e| e.to_string())?,
        serde_json::to_string(&design.expressions).map_err(|e| e.to_string())?
    );
    for i in &design.items {
        println!(
            "{:>4} {:<28} {:<32} record {:>6} state {}",
            i.index,
            i.name,
            i.object_type,
            i.record,
            i.state.map_or("-".to_string(), |s| s.to_string())
        );
    }
    for l in &design.left_out {
        println!("left out: {l}");
    }
    for n in &design.notes {
        println!("note: {n}");
    }
    Ok(())
}

/// The property slots of the timeline's features of a kind (an object
/// type such as `RevolveFeature`, or `*`): per slot its record, type and
/// what it holds, as far as it reads as an enumeration, a boolean, a
/// parameter or a direction; else its bytes after the header.
fn cmd_slots(path: &str, kind: &str) -> Result<(), String> {
    use mitcad_ipt::features::{BOOLEAN, DIRECTION};
    let f = open(path)?;
    let dc = f
        .definitions()
        .map_err(|e| e.to_string())?
        .ok_or("no definitions segment")?;
    let design = mitcad_ipt::design::read(&f, path)
        .map_err(|e| e.to_string())?
        .ok_or("no definitions segment")?;
    let params = mitcad_ipt::params::Parameters::read(&dc);
    for item in design
        .items
        .iter()
        .filter(|i| kind == "*" || i.object_type == kind)
    {
        println!(
            "{} {} (record {}, {})",
            item.index,
            item.name,
            item.record,
            dc.type_of(item.record)
                .map(mitcad_ipt::dc::type_text)
                .unwrap_or_default()
        );
        let mut r = dc.body(item.record);
        let slots = (|| -> mitcad_ipt::dc::Result<Vec<u32>> {
            r.i32()?;
            r.u32()?;
            Ok(r.list()?.1)
        })();
        let Ok(slots) = slots else {
            println!("  no slots");
            continue;
        };
        for (k, v) in slots.iter().enumerate() {
            let Some(s) = mitcad_ipt::dc::reference(*v).filter(|&s| s < dc.len()) else {
                continue;
            };
            let t = dc
                .type_of(s)
                .map(mitcad_ipt::dc::type_text)
                .unwrap_or_default();
            let b = dc.bytes(s);
            let what = if let Some(p) = params.by_record(s) {
                format!("parameter {} = {}", p.name, p.value)
            } else if dc.is(s, &BOOLEAN) {
                format!("boolean {}", b.last().copied().unwrap_or(9))
            } else if dc.is(s, &DIRECTION) {
                let mut rd = mitcad_ipt::dc::Reader::at(b, b.len().saturating_sub(24));
                let v: Vec<f64> = (0..3).filter_map(|_| rd.f64().ok()).collect();
                format!("direction {v:?}")
            } else if b.len() == dc.header_len() + dc.prefix_len() + 4 {
                let mut rd = dc.fields(s);
                format!(
                    "enumeration {} {}",
                    rd.u16().unwrap_or(0),
                    rd.u16().unwrap_or(0)
                )
            } else {
                hex(&b[(dc.header_len() + dc.prefix_len()).min(b.len())..]
                    [..(b.len().saturating_sub(dc.header_len() + dc.prefix_len())).min(96)])
            };
            println!("  [{k:>2}] {s:>6} {} {what}", &t[..8]);
        }
    }
    Ok(())
}

fn cmd_profile(path: &str, n: &str) -> Result<(), String> {
    let f = open(path)?;
    let dc = f
        .definitions()
        .map_err(|e| e.to_string())?
        .ok_or("no definitions segment")?;
    let record: usize = n.parse().map_err(|_| format!("bad record {n}"))?;
    let mut profiles = mitcad_ipt::profile::Profiles::default();
    match profiles.loops(&dc, record) {
        Some(loops) => {
            for l in &loops {
                // The vector area (mm²) and the centroid of the points.
                let mut a = [0.0f64; 3];
                for (i, p) in l.iter().enumerate() {
                    let q = l[(i + 1) % l.len()];
                    a[0] += p[1] * q[2] - p[2] * q[1];
                    a[1] += p[2] * q[0] - p[0] * q[2];
                    a[2] += p[0] * q[1] - p[1] * q[0];
                }
                let area = 0.5 * (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
                let n = l.len() as f64;
                let c: Vec<f64> = (0..3)
                    .map(|k| l.iter().map(|p| p[k]).sum::<f64>() / n)
                    .collect();
                println!(
                    "loop of {} points from {:?}, area {area:.3} mm², points' mean {c:.3?}",
                    l.len(),
                    l[0]
                );
            }
        }
        None => println!("no loops"),
    }
    Ok(())
}

fn cmd_parameters(path: &str) -> Result<(), String> {
    use mitcad_ipt::params::{DisplayUnit, Parameters};
    let f = open(path)?;
    let dc = f
        .definitions()
        .map_err(|e| e.to_string())?
        .ok_or("no definitions segment")?;
    let length = f
        .document()
        .length_unit
        .and_then(DisplayUnit::length)
        .unwrap_or(DisplayUnit::Mm);
    let params = Parameters::read(&dc);
    println!(
        "segment major {}, table {:?}, {} parameters ({} in the table), {} unread",
        dc.major,
        params.table,
        params.list.len(),
        params.table().count(),
        params.unread.len()
    );
    for p in &params.list {
        let text = params.text(p, length).unwrap_or_else(|e| format!("<{e}>"));
        let check = match params.evaluate(p) {
            Ok(v) if (v - p.value).abs() <= 1e-9 * p.value.abs().max(1.0) => "=".to_string(),
            Ok(v) => format!("!= {v}"),
            Err(e) => format!("? {e}"),
        };
        println!(
            "{}{:>6} {:<16} {:?} {:?} flags {:#010x}{} value {} {check}: {text}",
            if p.in_table { " " } else { "x" },
            p.record,
            p.name,
            p.kind,
            p.quantity,
            p.flags,
            if p.is_computed() { " computed" } else { "" },
            p.value
        );
    }
    for (r, e) in &params.unread {
        println!("unread {r}: {e}");
    }
    Ok(())
}

/// The parameters of the part's table with the types of the records that
/// refer to them (not the collections), and whether their expressions give
/// their stored values: `name agree|differ|unread types...`.
fn cmd_owners(path: &str) -> Result<(), String> {
    use mitcad_ipt::params::Parameters;
    let f = open(path)?;
    let dc = f
        .definitions()
        .map_err(|e| e.to_string())?
        .ok_or("no definitions segment")?;
    let params = Parameters::read(&dc);
    let mut by: std::collections::HashMap<usize, Vec<String>> = Default::default();
    for i in 0..dc.len() {
        if dc.is(i, &mitcad_ipt::dc::COLLECTION) {
            continue;
        }
        let b = dc.bytes(i);
        let mut at = 0;
        while at + 4 <= b.len() {
            let v = u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
            if let Some(r) = mitcad_ipt::dc::reference(v)
                && v & 0x8000_0000 != 0
                && r != i
                && params.by_record(r).is_some()
            {
                let t = dc
                    .type_of(i)
                    .map(mitcad_ipt::dc::type_text)
                    .unwrap_or_default();
                by.entry(r).or_default().push(t[..8].to_owned());
                at += 4;
            } else {
                at += 1;
            }
        }
    }
    for p in params.table() {
        let status = match params.evaluate(p) {
            Ok(v) if (v - p.value).abs() <= 1e-9 * p.value.abs().max(1e-9) => "agree",
            Ok(_) => "differ",
            Err(_) => "unread",
        };
        let mut owners = by.get(&p.record).cloned().unwrap_or_default();
        owners.sort();
        owners.dedup();
        println!("{} {status} {:#010x} {}", p.name, p.flags, owners.join(","));
    }
    Ok(())
}

/// The work planes with labels and the records that define them (the
/// header, an i32, the plane, then references): each definition's type
/// and the records after the plane with their types, then its bytes.
fn cmd_planedefs(path: &str) -> Result<(), String> {
    let f = open(path)?;
    let dc = f
        .definitions()
        .map_err(|e| e.to_string())?
        .ok_or("no definitions segment")?;
    let labels = dc.labels();
    let plane_t = mitcad_ipt::features::WORK_PLANE;
    for p in dc.of_type(&plane_t) {
        let name = labels.get(&p).map(|l| l.name.clone()).unwrap_or_default();
        let geometry = mitcad_ipt::features::plane(&dc, p);
        println!("plane {p} {name:?} {geometry:?}");
        for d in 0..dc.len() {
            if d == p {
                continue;
            }
            let mut r = dc.body(d);
            if r.i32().is_err() || r.reference().ok().flatten() != Some(p) {
                continue;
            }
            let t = dc
                .type_of(d)
                .map(mitcad_ipt::dc::type_text)
                .unwrap_or_default();
            let b = dc.bytes(d);
            let mut refs = Vec::new();
            let mut at = r.pos;
            while at + 4 <= b.len() {
                let v = u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
                match mitcad_ipt::dc::reference(v) {
                    Some(x) if v & 0x8000_0000 != 0 && x < dc.len() => {
                        let xt = dc
                            .type_of(x)
                            .map(mitcad_ipt::dc::type_text)
                            .unwrap_or_default();
                        let xn = labels.get(&x).map(|l| l.name.clone()).unwrap_or_default();
                        refs.push(format!("@{at} {x} {} {xn:?}", &xt[..8]));
                        at += 4;
                    }
                    _ => at += 1,
                }
            }
            println!("  def {d} {t}: {}", refs.join(", "));
            println!("    {}", hex(&b[dc.header_len()..]));
        }
    }
    Ok(())
}

/// A parameter's expression as a tree of its records: each node's type,
/// bytes and the records it refers to (flagged or not) that are
/// expression nodes, units or parameters (named, not followed).
fn cmd_expr(path: &str, n: &str) -> Result<(), String> {
    let f = open(path)?;
    let dc = f
        .definitions()
        .map_err(|e| e.to_string())?
        .ok_or("no definitions segment")?;
    let i: usize = n.parse().map_err(|_| format!("bad record {n}"))?;
    let params = mitcad_ipt::params::Parameters::read(&dc);
    fn walk(
        dc: &mitcad_ipt::dc::Definitions,
        params: &mitcad_ipt::params::Parameters,
        r: usize,
        depth: usize,
    ) {
        let pad = "  ".repeat(depth);
        let t = dc
            .type_of(r)
            .map(mitcad_ipt::dc::type_text)
            .unwrap_or_default();
        if let Some(p) = params.by_record(r)
            && depth > 0
        {
            println!("{pad}{r} parameter {} = {}", p.name, p.value);
            return;
        }
        let b = dc.bytes(r);
        println!("{pad}{r} {t} {}", hex(b));
        if depth > 12 {
            return;
        }
        let known = |x: usize| {
            dc.type_of(x).is_some_and(|t| {
                t[1..] == *b"\x7a\xa7\xf8\xd2\x11\x8f\x09\xc0\x00\x5a\x9a\x23\x78\xd0\x4f"
                    || t[1..] == *b"\x79\xa7\xf8\xd2\x11\x8f\x09\xc0\x00\x5a\x9a\x23\x78\xd0\x4f"
                    || t[1..] == *b"\x00\x9d\x5f\xd2\x11\x8e\x09\xc0\x00\x5a\x9a\x23\x78\xd0\x4f"
                    || t[2..] == *b"\x30\x5c\xd2\x11\x3f\x0d\x60\x00\x6a\xb7\x60\xfe\xc3\xb0"
                    || t[2..] == *b"\x41\x62\xd2\x11\x9b\x0b\x60\x00\x6a\xb7\x60\xfe\xc3\xb0"
            }) || params.by_record(x).is_some()
        };
        let start = if depth == 0 { 22 } else { 6 };
        let mut at = start;
        while at + 4 <= b.len() {
            let v = u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
            if let Some(x) = mitcad_ipt::dc::reference(v)
                && x != r
                && x < dc.len()
                && known(x)
            {
                println!("{pad} @{at}:");
                walk(dc, params, x, depth + 1);
            }
            at += 2;
        }
    }
    walk(&dc, &params, i, 0);
    Ok(())
}

/// The definitions records whose type starts with `prefix` (hex), one per
/// line: index, size, the bytes after the header (hex).
fn cmd_typed(path: &str, prefix: &str) -> Result<(), String> {
    let f = open(path)?;
    let dc = f
        .definitions()
        .map_err(|e| e.to_string())?
        .ok_or("no definitions segment")?;
    println!("segment major {}, {} records", dc.major, dc.len());
    for i in 0..dc.len() {
        let t = dc.type_of(i).map(mitcad_ipt::dc::type_text);
        if !t.as_deref().is_some_and(|t| t.starts_with(prefix)) {
            continue;
        }
        let b = dc.bytes(i);
        let body = b.get(dc.header_len()..).unwrap_or(&[]);
        println!(
            "{i:>6} {:>5} {} | {}",
            b.len(),
            hex(&b[..b.len().min(dc.header_len())]),
            hex(body)
        );
    }
    Ok(())
}

/// A definitions record: its type, bytes, and the u32 at each offset that
/// look like flagged references, with their records' types.
fn cmd_show(path: &str, n: &str) -> Result<(), String> {
    let f = open(path)?;
    let dc = f
        .definitions()
        .map_err(|e| e.to_string())?
        .ok_or("no definitions segment")?;
    let i: usize = n.parse().map_err(|_| format!("bad record {n}"))?;
    let t = dc
        .type_of(i)
        .map(mitcad_ipt::dc::type_text)
        .unwrap_or_default();
    let b = dc.bytes(i);
    println!("record {i} type {t}, {} bytes, major {}", b.len(), dc.major);
    println!("{}", hex(b));
    let mut at = 0;
    while at + 4 <= b.len() {
        let v = u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
        match mitcad_ipt::dc::reference(v) {
            Some(r) if v & 0x8000_0000 != 0 && r < dc.len() => {
                let rt = dc
                    .type_of(r)
                    .map(mitcad_ipt::dc::type_text)
                    .unwrap_or_default();
                println!("  @{at:>4} -> {r:>6} {rt}");
                at += 4;
            }
            _ => at += 1,
        }
    }
    Ok(())
}

/// The definitions records that refer to record `n` (flagged references),
/// with their types and where in them.
fn cmd_refs(path: &str, n: &str) -> Result<(), String> {
    let f = open(path)?;
    let dc = f
        .definitions()
        .map_err(|e| e.to_string())?
        .ok_or("no definitions segment")?;
    let target: usize = n.parse().map_err(|_| format!("bad record {n}"))?;
    for i in 0..dc.len() {
        let b = dc.bytes(i);
        let at: Vec<usize> = (0..b.len().saturating_sub(3))
            .filter(|&k| {
                let v = u32::from_le_bytes([b[k], b[k + 1], b[k + 2], b[k + 3]]);
                v & 0x8000_0000 != 0 && mitcad_ipt::dc::reference(v) == Some(target)
            })
            .collect();
        if !at.is_empty() {
            let t = dc
                .type_of(i)
                .map(mitcad_ipt::dc::type_text)
                .unwrap_or_default();
            println!("{i:>6} {t} at {at:?}");
        }
    }
    Ok(())
}

/// Checks the prefix (`Definitions::prefix_len`) of the records that have
/// one: an i32 −1 first from major 23, and from major 25 a second i32;
/// before major 23 no −1 there. Prints the counts and the records that
/// differ.
fn cmd_prefix(path: &str) -> Result<(), String> {
    use mitcad_ipt::{features, groups, params, profile, sketch};
    let f = open(path)?;
    let dc = f
        .definitions()
        .map_err(|e| e.to_string())?
        .ok_or("no definitions segment")?;
    let types = [
        params::PARAMETER,
        params::INTEGER,
        sketch::POINT,
        sketch::LINE,
        sketch::CIRCLE,
        sketch::TRANSFORM,
        features::ENUM_OPERATION,
        features::ENUM_EXTENT,
        features::BOOLEAN,
        features::DIRECTION,
        features::BOUNDARY_PATCH,
        features::FILLET_SET,
        profile::SELECTION,
        groups::GROUP,
    ];
    let (mut ok, mut bad) = (0, Vec::new());
    let mut stamps: BTreeMap<i32, usize> = BTreeMap::new();
    for i in 0..dc.len() {
        if !types.iter().any(|t| dc.is(i, t)) {
            continue;
        }
        let b = dc.bytes(i);
        let at = |k: usize| {
            b.get(k..k + 4)
                .map(|x| i32::from_le_bytes([x[0], x[1], x[2], x[3]]))
        };
        let h = dc.header_len();
        // −1, or the index of a fillet edge set's parameter or boolean.
        let first = at(h).is_some_and(|v| v == -1 || (0..256).contains(&v));
        let fits = match dc.prefix_len() {
            0 => at(h) != Some(-1),
            4 => first,
            _ => {
                if let Some(s) = at(h + 4) {
                    *stamps.entry(s).or_insert(0) += 1;
                }
                first
            }
        };
        if fits {
            ok += 1;
        } else {
            bad.push(format!(
                "{i} {}",
                dc.type_of(i)
                    .map(mitcad_ipt::dc::type_text)
                    .unwrap_or_default()
            ));
        }
    }
    println!(
        "major {} prefix ok {ok} differ {} stamps {stamps:?}",
        dc.major,
        bad.len()
    );
    for b in bad.iter().take(10) {
        println!("  {b}");
    }
    Ok(())
}

/// For the sketch lines of the features' selected profiles: how many lie
/// on a measured profile's boundary, by their entity flags.
fn cmd_flags(path: &str, verbose: bool) -> Result<(), String> {
    use mitcad_ipt::{design, features, sketch};
    let f = open(path)?;
    let dc = f
        .definitions()
        .map_err(|e| e.to_string())?
        .ok_or("no definitions segment")?;
    let labels = dc.labels();
    let mut profiles = mitcad_ipt::profile::Profiles::default();
    let mut counts: BTreeMap<(u32, bool), usize> = BTreeMap::new();
    for (&record, label) in &labels {
        if !dc.is(record, &design::FEATURE) {
            continue;
        }
        let Some(&sk) = label.children.iter().find(|&&c| dc.is(c, &design::SKETCH)) else {
            continue;
        };
        let Ok(m) = sketch::sketch_matrix(&dc, sk) else {
            continue;
        };
        let mut r = dc.body(record);
        let slots = (|| -> mitcad_ipt::dc::Result<Vec<u32>> {
            r.i32()?;
            r.u32()?;
            Ok(r.list()?.1)
        })()
        .unwrap_or_default();
        let Some(patch) = slots.get(1).and_then(|&v| mitcad_ipt::dc::reference(v)) else {
            continue;
        };
        if !dc.is(patch, &features::BOUNDARY_PATCH) {
            continue;
        }
        let mut pr = dc.fields(patch);
        let selections = pr.references().unwrap_or_default();
        let outlines: Vec<Vec<[f64; 2]>> = selections
            .iter()
            .filter_map(|&s| profiles.measure(&dc, s, &m))
            .map(|m| m.outer)
            .collect();
        if outlines.is_empty() {
            continue;
        }
        let mut sr = dc.body(sk);
        let entities = (|| -> mitcad_ipt::dc::Result<Vec<usize>> {
            sr.i32()?;
            sr.u32()?;
            sr.references()
        })()
        .unwrap_or_default();
        let near = |p: [f64; 2]| {
            outlines.iter().any(|o| {
                (0..o.len()).any(|i| {
                    let (a, b) = (o[i], o[(i + 1) % o.len()]);
                    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                    let l2 = dx * dx + dy * dy;
                    let t = if l2 > 0.0 {
                        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let (x, y) = (a[0] + t * dx - p[0], a[1] + t * dy - p[1]);
                    (x * x + y * y).sqrt() < 1e-5
                })
            })
        };
        for &e in &entities {
            if !dc.is(e, &sketch::LINE) {
                continue;
            }
            let Ok((flags, mut er)) = sketch::entity(&dc, e) else {
                continue;
            };
            let ends = er.references().unwrap_or_default();
            let (Some(a), Some(b)) = (
                ends.first().and_then(|&p| sketch::point(&dc, p)),
                ends.get(1).and_then(|&p| sketch::point(&dc, p)),
            ) else {
                continue;
            };
            let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
            let on = near(a) && near(b) && near(mid);
            *counts.entry((flags, on)).or_insert(0) += 1;
            if verbose {
                let bytes = dc.bytes(e);
                println!(
                    "line {e} flags {flags:#x} on {on} header {} rest {}",
                    hex(&bytes[..dc.header_len().min(bytes.len())]),
                    hex(&bytes[er.pos.min(bytes.len())..])
                );
            }
        }
    }
    for ((flags, on), n) in counts {
        println!("flags {flags:#010x} on a profile's boundary {on}: {n}");
    }
    Ok(())
}

/// A sketch entity in a line: its kind and geometry (sketch cm).
fn entity_text(dc: &mitcad_ipt::dc::Definitions, e: usize) -> String {
    use mitcad_ipt::sketch::{CIRCLE, LINE, POINT, entity, point};
    if dc.is(e, &POINT) {
        return point(dc, e).map_or("point ?".into(), |p| {
            format!("point {:.6} {:.6}", p[0], p[1])
        });
    }
    if dc.is(e, &LINE) {
        let ends = entity(dc, e)
            .ok()
            .and_then(|(_, mut r)| r.references().ok());
        let ps: Vec<String> = ends
            .unwrap_or_default()
            .iter()
            .map(|&p| point(dc, p).map_or("?".into(), |p| format!("({:.6} {:.6})", p[0], p[1])))
            .collect();
        return format!("line {}", ps.join(" "));
    }
    if dc.is(e, &CIRCLE) {
        let b = dc.bytes(e);
        let mut tail = mitcad_ipt::dc::Reader::at(b, b.len().saturating_sub(13));
        let c = tail.reference().ok().flatten();
        let r = tail.f64().unwrap_or(f64::NAN);
        let full = tail.u8().unwrap_or(9);
        let ends = entity(dc, e)
            .ok()
            .and_then(|(_, mut r)| r.references().ok());
        let at = c.and_then(|c| point(dc, c));
        return format!(
            "circle centre {at:?} r {r:.6} full {full} ends {:?}",
            ends.unwrap_or_default()
        );
    }
    dc.type_of(e)
        .map(mitcad_ipt::dc::type_text)
        .unwrap_or_default()
}

/// The constraints of a type: their parameter and entities.
fn cmd_constraints(path: &str, prefix: &str) -> Result<(), String> {
    let f = open(path)?;
    let dc = f
        .definitions()
        .map_err(|e| e.to_string())?
        .ok_or("no definitions segment")?;
    for i in 0..dc.len() {
        let t = dc.type_of(i).map(mitcad_ipt::dc::type_text);
        if !t.as_deref().is_some_and(|t| t.starts_with(prefix)) {
            continue;
        }
        match mitcad_ipt::sketch::constraint_body(&dc, i) {
            Ok((p, rest)) => {
                println!("{i}: parameter {p:?}, {} u32", rest.len());
                for (k, v) in rest.iter().enumerate() {
                    match mitcad_ipt::dc::reference(*v)
                        .filter(|&r| v & 0x8000_0000 != 0 && r < dc.len())
                    {
                        Some(r) => println!("  [{k}] {r}: {}", entity_text(&dc, r)),
                        None => println!("  [{k}] {v:#x}"),
                    }
                }
            }
            Err(e) => println!("{i}: {e}"),
        }
    }
    Ok(())
}

/// A decimal or `0x` hexadecimal number.
fn parse_number(text: &str) -> Result<usize, String> {
    match text.strip_prefix("0x") {
        Some(h) => usize::from_str_radix(h, 16),
        None => text.parse(),
    }
    .map_err(|_| format!("bad number {text}"))
}

fn run(args: &[String]) -> Result<(), String> {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    match a.as_slice() {
        ["list", file] => cmd_list(file),
        ["properties", file] => cmd_properties(file),
        ["segments", file] => cmd_segments(file),
        ["records", file, segment] => cmd_records(file, segment),
        ["record", file, segment, n] => cmd_record(file, segment, n),
        ["data", file, segment, from, len] => {
            cmd_data(file, segment, parse_number(from)?, parse_number(len)?)
        }
        ["stream", file, stream] => cmd_stream(file, stream, false),
        ["stream", file, stream, "--raw"] => cmd_stream(file, stream, true),
        ["record", file, segment, n, "--raw"] => cmd_raw_record(file, segment, n),
        ["dump", file, segment] => cmd_dump(file, segment),
        ["labels", file, segment] => cmd_labels(file, segment),
        ["assembly", file] => cmd_assembly(file),
        ["bodies", file] => cmd_bodies(file, false, false),
        ["bodies", file, "-f"] => cmd_bodies(file, false, true),
        ["bodies", file, "-v"] => cmd_bodies(file, true, false),
        ["result", file] => cmd_result(file),
        ["history", file] => cmd_history(file),
        ["parameters", file] => cmd_parameters(file),
        ["typed", file, prefix] => cmd_typed(file, prefix),
        ["show", file, n] => cmd_show(file, n),
        ["prefix", file] => cmd_prefix(file),
        ["flags", file] => cmd_flags(file, false),
        ["flags", file, "-v"] => cmd_flags(file, true),
        ["constraints", file, prefix] => cmd_constraints(file, prefix),
        ["design", file] => cmd_design(file, false),
        ["profile", file, n] => cmd_profile(file, n),
        ["design", file, "--json"] => cmd_design(file, true),
        ["slots", file, kind] => cmd_slots(file, kind),
        ["refs", file, n] => cmd_refs(file, n),
        ["expr", file, n] => cmd_expr(file, n),
        ["owners", file] => cmd_owners(file),
        ["planedefs", file] => cmd_planedefs(file),
        ["asm", file, segment, n] => cmd_asm(file, segment, n, None),
        ["asm", file, segment, n, m] => cmd_asm(
            file,
            segment,
            n,
            Some(m.parse().map_err(|_| format!("bad count {m}"))?),
        ),
        ["write-test", out] => std::fs::write(out, mitcad_ipt::testdata::test_part())
            .map_err(|e| format!("{out}: {e}")),
        _ => Err(USAGE.to_string()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
