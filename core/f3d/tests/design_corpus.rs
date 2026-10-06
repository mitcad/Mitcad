// SPDX-License-Identifier: MIT
//! Design stream decoder on the local .f3d corpus (`MITCAD_F3D_CORPUS`,
//! default `~/f3d-corpus`; skipped when missing).
//!
//! - Every design segment parses: the whole MetaStream, every object tag is
//!   256 + its class index.
//! - The dump IR equals, field by field, the reference decoder's JSON (one
//!   file per corpus design, written by the separate Python decoder of the
//!   format study, which is not part of this repository), read from
//!   `MITCAD_F3D_DESIGN_REF` (default `~/f3d-design-ref`; this part is
//!   skipped when the directory is missing). The light bulbs, which only
//!   this decoder reads, are left out ([`DECODER_ONLY`]).
//! - Every parameter expression evaluated with `mitcad_model::expr::f3d`
//!   equals the stored value.
//!
//! Run with `--nocapture` for the agreement report.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use mitcad_f3d::design::{self, Design, ir};
use mitcad_model::expr::{LengthUnit, Quantity, f3d};
use serde_json::Value;

fn home_dir(var: &str, default: &str) -> Option<PathBuf> {
    let dir = match std::env::var_os(var) {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(std::env::var_os("HOME")?).join(default),
    };
    dir.is_dir().then_some(dir)
}

/// `.f3d`/`.f3z` files under `dir`, sorted by path text (the reference
/// decoder's order, which numbers its JSON files).
fn corpus_files(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else {
                let name = p.to_string_lossy().to_lowercase();
                if name.ends_with(".f3d") || name.ends_with(".f3z") {
                    out.push(p);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, &mut out);
    out.sort_by_key(|p| p.to_string_lossy().into_owned());
    out
}

/// Field-level comparison of two JSON values.
#[derive(Default)]
struct Agreement {
    equal: usize,
    differ: usize,
    /// In the reference only.
    missing: usize,
    /// In the Rust dump only.
    extra: usize,
    /// Field path (indices as `[]`) -> (equal, not equal).
    by_field: BTreeMap<String, (usize, usize)>,
    examples: Vec<String>,
}

const IGNORED: &[&str] = &[".generator.decoder"];

/// The reference JSON predates the dump format's own names (mitcad#26):
/// its `schema` name and the build key of `_f3d.format` (the one key there
/// ending in `_build`) are given the current names before the comparison.
fn current_names(reference: &Value) -> Value {
    let mut r = reference.clone();
    if let Some(format) = r.pointer_mut("/_f3d/format").and_then(Value::as_object_mut) {
        let old: Vec<String> = format
            .keys()
            .filter(|k| k.ends_with("_build") && k.as_str() != "writer_build")
            .cloned()
            .collect();
        for key in old {
            if let Some(build) = format.remove(&key) {
                format.insert("writer_build".to_owned(), build);
            }
        }
    }
    if r.get("schema").is_some() {
        r["schema"] = Value::from(ir::SCHEMA_NAME);
    }
    r
}

/// Fields this decoder writes beyond the reference decoder: the light
/// bulbs of sketches and construction planes (mitcad#6), checked against
/// the dumps of the reference models instead (`design_models.rs`).
const DECODER_ONLY: &[&str] = &[
    ".timeline.items[].props",
    ".timeline.items[]._f3d.light_bulb",
];

fn leaves(v: &Value) -> usize {
    match v {
        Value::Object(m) => m.values().map(leaves).sum::<usize>().max(1),
        Value::Array(a) => a.iter().map(leaves).sum::<usize>().max(1),
        _ => 1,
    }
}

impl Agreement {
    fn note(&mut self, field: &str, ok: bool, n: usize, what: impl FnOnce() -> String) {
        let e = self.by_field.entry(field.to_string()).or_default();
        if ok {
            e.0 += n;
        } else {
            e.1 += n;
            if self.examples.len() < 40 {
                self.examples.push(what());
            }
        }
    }

    fn compare(&mut self, label: &str, path: &str, field: &str, rust: &Value, reference: &Value) {
        if IGNORED.contains(&field) {
            return;
        }
        match (rust, reference) {
            (Value::Object(a), Value::Object(b)) => {
                let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
                keys.sort();
                keys.dedup();
                for k in keys {
                    let p = format!("{path}.{k}");
                    let f = format!("{field}.{k}");
                    match (a.get(k), b.get(k)) {
                        (Some(x), Some(y)) => self.compare(label, &p, &f, x, y),
                        (None, Some(y)) => {
                            let n = leaves(y);
                            self.missing += n;
                            self.note(&f, false, n, || format!("{label}: {p} missing: {y}"));
                        }
                        (Some(_), None) if DECODER_ONLY.contains(&f.as_str()) => {}
                        (Some(x), None) => {
                            let n = leaves(x);
                            self.extra += n;
                            self.note(&f, false, n, || format!("{label}: {p} extra: {x}"));
                        }
                        (None, None) => {}
                    }
                }
            }
            (Value::Array(a), Value::Array(b)) => {
                let f = format!("{field}[]");
                for i in 0..a.len().max(b.len()) {
                    let p = format!("{path}[{i}]");
                    match (a.get(i), b.get(i)) {
                        (Some(x), Some(y)) => self.compare(label, &p, &f, x, y),
                        (None, Some(y)) => {
                            let n = leaves(y);
                            self.missing += n;
                            self.note(&f, false, n, || format!("{label}: {p} missing: {y}"));
                        }
                        (Some(x), None) => {
                            let n = leaves(x);
                            self.extra += n;
                            self.note(&f, false, n, || format!("{label}: {p} extra: {x}"));
                        }
                        (None, None) => {}
                    }
                }
                if a.is_empty() && b.is_empty() {
                    self.equal += 1;
                    self.note(field, true, 1, String::new);
                }
            }
            (x, y) => {
                let ok = x == y;
                if ok {
                    self.equal += 1;
                } else {
                    self.differ += 1;
                }
                self.note(field, ok, 1, || {
                    format!("{label}: {path}: rust {x} reference {y}")
                });
            }
        }
    }
}

/// Evaluates every parameter with Mitcad's expression engine.
#[derive(Default, Debug)]
struct ExprCheck {
    ok: usize,
    mismatch: usize,
    /// Text parameters and expressions the engine does not evaluate.
    skipped: usize,
    examples: Vec<String>,
}

fn check_expressions(dump: &ir::Dump, label: &str, out: &mut ExprCheck) {
    let params: Vec<&ir::Parameter> = dump.all_parameters().collect();
    let mut values: HashMap<String, Quantity> = HashMap::new();
    for p in &params {
        let (Some(name), Some(unit), Some(value)) = (&p.name, &p.unit, p.value) else {
            continue;
        };
        if let Ok(u) = f3d::parse_unit(unit) {
            values.insert(name.clone(), f3d::internal_quantity(value, u.dims()));
        }
    }
    let ctx = f3d::context(LengthUnit::Millimetre);
    for p in &params {
        let (Some(name), Some(expr), Some(unit), Some(value)) =
            (&p.name, &p.expression, &p.unit, p.value)
        else {
            continue;
        };
        if unit == "Text" {
            out.skipped += 1;
            continue;
        }
        match f3d::check_parameter(expr, unit, value, &values, &ctx, 1e-9) {
            Ok((_, true)) => out.ok += 1,
            Ok((q, false)) => {
                out.mismatch += 1;
                out.examples.push(format!(
                    "{label}: {name} = {expr:?} -> {q}, stored {value} ({unit})"
                ));
            }
            Err(e) => {
                out.skipped += 1;
                if out.examples.len() < 20 {
                    out.examples
                        .push(format!("{label}: {name} = {expr:?}: {e}"));
                }
            }
        }
    }
}

#[test]
fn corpus_designs_match_the_reference_decoder() {
    let Some(corpus) = home_dir("MITCAD_F3D_CORPUS", "f3d-corpus") else {
        eprintln!("corpus not found; skipped");
        return;
    };
    let reference = home_dir("MITCAD_F3D_DESIGN_REF", "f3d-design-ref");
    let ref_files: Vec<PathBuf> = reference
        .as_ref()
        .and_then(|d| std::fs::read_dir(d).ok())
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    let mut agreement = Agreement::default();
    let mut exprs = ExprCheck::default();
    let (mut designs, mut compared, mut segments_parsed) = (0usize, 0usize, 0usize);
    let (mut items, mut named, mut params, mut sketches) = (0usize, 0usize, 0usize, 0usize);
    let (mut points, mut curves, mut constraints, mut dimensions) =
        (0usize, 0usize, 0usize, 0usize);
    let (mut identical, mut objects) = (0usize, 0usize);
    // Sketches whose light bulb decodes (mitcad#6).
    let mut bulbs = 0usize;
    for path in corpus_files(&corpus) {
        for (label, doc) in design::documents(&path).unwrap() {
            let n = designs;
            designs += 1;
            let Some(s) = design::find_design_streams(&doc).unwrap() else {
                continue;
            };
            let d = Design::parse(&s.meta, s.bulk).unwrap_or_else(|e| panic!("{label}: {e}"));
            segments_parsed += 1;
            // Framing: the whole MetaStream; every object header has the
            // index id and the tag 256 + class index.
            assert!(d.segment.meta.tail_ok, "{label}: MetaStream tail");
            for o in &d.segment.objects {
                let h = d.segment.header(o).unwrap();
                assert_eq!(
                    (h.class_index(), h.id),
                    (o.class, o.id),
                    "{label}: {}",
                    o.id
                );
                objects += 1;
            }
            let dump = d.dump(&label, &s.segment_dir);
            check_expressions(&dump, &label, &mut exprs);
            let tl = dump.timeline_items();
            items += tl.len();
            named += tl.iter().filter(|i| i.name().is_some()).count();
            params += dump.all_parameters().count();
            for it in tl {
                if let Some(ir::Detail::Sketch(s)) = &it.detail {
                    sketches += 1;
                    bulbs += usize::from(it.light_bulb().is_some());
                    points += s.points.as_ref().map_or(0, Vec::len);
                    curves += s.curves.as_ref().map_or(0, Vec::len);
                    constraints += s.constraints.as_ref().map_or(0, Vec::len);
                    dimensions += s.dimensions.as_ref().map_or(0, Vec::len);
                }
            }
            // The IR reads its own output back unchanged.
            let v = serde_json::to_value(&dump).unwrap();
            let back: ir::Dump = serde_json::from_value(v.clone()).unwrap();
            assert_eq!(
                serde_json::to_value(&back).unwrap(),
                v,
                "{label}: round trip"
            );

            let prefix = format!("{n:02}_");
            let Some(file) = ref_files.iter().find(|f| {
                f.file_name()
                    .is_some_and(|x| x.to_string_lossy().starts_with(&prefix))
            }) else {
                continue;
            };
            let text = std::fs::read_to_string(file).unwrap();
            let r: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(
                r["source"]["file"],
                Value::from(label.as_str()),
                "{}",
                file.display()
            );
            // The reference reads into the IR as well.
            let rd = ir::Dump::from_json(&text).unwrap();
            assert_eq!(
                serde_json::to_value(&rd).unwrap(),
                r,
                "{label}: reference round trip"
            );
            let before = agreement.differ + agreement.missing + agreement.extra;
            agreement.compare(&label, "", "", &v, &current_names(&r));
            if agreement.differ + agreement.missing + agreement.extra == before {
                identical += 1;
            }
            compared += 1;
        }
    }
    println!(
        "designs {designs}, design segments {segments_parsed} ({objects} objects); \
         timeline items {items} (named {named}), \
         parameters {params}, sketches {sketches} (points {points}, curves {curves}, \
         constraints {constraints}, dimensions {dimensions}; light bulb decoded {bulbs}, \
         unknown {})",
        sketches - bulbs
    );
    println!(
        "parameter expressions (mitcad_model::expr::f3d): ok {}, mismatch {}, skipped {}",
        exprs.ok, exprs.mismatch, exprs.skipped
    );
    for e in &exprs.examples {
        println!("  {e}");
    }
    assert_eq!(
        exprs.mismatch, 0,
        "parameter values differ from their expressions"
    );
    if compared == 0 {
        eprintln!("reference JSON not found; comparison skipped");
        return;
    }
    let total = agreement.equal + agreement.differ + agreement.missing + agreement.extra;
    println!(
        "reference comparison: {compared} designs ({identical} identical); fields {total}: \
         equal {}, different {}, missing {}, extra {} ({:.4} % agree)",
        agreement.equal,
        agreement.differ,
        agreement.missing,
        agreement.extra,
        100.0 * agreement.equal as f64 / total.max(1) as f64
    );
    let mut fields: Vec<_> = agreement.by_field.iter().collect();
    fields.sort_by_key(|(_, (ok, bad))| std::cmp::Reverse(ok + bad));
    println!("fields (equal / total):");
    for (f, (ok, bad)) in fields.iter().take(60) {
        println!("  {:>8} / {:<8} {f}", ok, ok + bad);
    }
    for (f, (ok, bad)) in &fields {
        if *bad > 0 {
            println!("  DIFF {f}: {bad} of {}", ok + bad);
        }
    }
    for e in &agreement.examples {
        println!("  {e}");
    }
    assert_eq!(
        agreement.differ + agreement.missing + agreement.extra,
        0,
        "the dump IR differs from the reference decoder; if the reference decoder \
         changed, regenerate the reference JSON (MITCAD_F3D_DESIGN_REF) with it"
    );
}
