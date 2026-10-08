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
  record <file> <segment> <n>   one record's bytes (hex)
  bodies <file> [-v]            the B-rep records and their bodies
  asm <file> <segment> <n> [m] the ASM data of a record as text (its
                                first m records), from its 14th byte
  design <file> [--json]        the timeline (features, history states)
                                and parameter summaries; --json the dump
  profile <file> <n>            a profile selection's wire body
  parameters <file>             the parameters: expressions, units, values
  history <file>                the ASM history of the B-rep records: its
                                states and the bodies rolled back
  write-test <out.ipt>          write the test part of mitcad_ipt::testdata";

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
            "{:<20} {{{}}} {} kind {}, {} types, {} bytes ({}), {}",
            s.name,
            guid_text(&s.id),
            s.meta.tag,
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
    for (k, chunk) in content.data[r.range.clone()].chunks(16).enumerate() {
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
    Ok(())
}

fn cmd_bodies(path: &str, verbose: bool) -> Result<(), String> {
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
            if verbose {
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
        for (body, top) in mitcad_f3d::convert::body_records(&file) {
            if !top {
                continue;
            }
            for steps in 0..=history.states.len() {
                let view = history.view(steps);
                if view.get(&body) == Some(&None) {
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
                println!("loop of {} points from {:?}", l.len(), l[0]);
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
            "{}{:>6} {:<16} {:?} {:?} flags {:#010x} value {} {check}: {text}",
            if p.in_table { " " } else { "x" },
            p.record,
            p.name,
            p.kind,
            p.quantity,
            p.flags,
            p.value
        );
    }
    for (r, e) in &params.unread {
        println!("unread {r}: {e}");
    }
    Ok(())
}

fn run(args: &[String]) -> Result<(), String> {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    match a.as_slice() {
        ["list", file] => cmd_list(file),
        ["properties", file] => cmd_properties(file),
        ["segments", file] => cmd_segments(file),
        ["records", file, segment] => cmd_records(file, segment),
        ["record", file, segment, n] => cmd_record(file, segment, n),
        ["bodies", file] => cmd_bodies(file, false),
        ["bodies", file, "-v"] => cmd_bodies(file, true),
        ["history", file] => cmd_history(file),
        ["parameters", file] => cmd_parameters(file),
        ["design", file] => cmd_design(file, false),
        ["profile", file, n] => cmd_profile(file, n),
        ["design", file, "--json"] => cmd_design(file, true),
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
