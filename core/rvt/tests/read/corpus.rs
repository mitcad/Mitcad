// SPDX-License-Identifier: MIT
//! The reader on the `.rvt`/`.rfa` corpus (`MITCAD_RVT_CORPUS`, skipped
//! when unset): every file opens with its metadata, preview, schema (no
//! field encoding outside the known ones), element table and element
//! records; the 2024 and 2025 projects' elements are compared with the
//! public reference reader's outputs kept beside the corpus
//! (`reference/<file>/`): the declared ids, categories, classes, levels,
//! families, types, boxes, transforms and materials. Files are named by
//! the first 8 hex digits of their SHA-256 (the corpus index's).
//!
//! The reference reader finds some values by heuristics where the file
//! stores them elsewhere; where it disagrees with a value this reader
//! reads from the element's own records, the case is listed below
//! ([`KNOWN_DIFFERENCES`]) or covered by a rule ([`level_explained`]),
//! and every other disagreement fails the test.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;

use mitcad_rvt::RvtFile;
use mitcad_rvt::elements::{Element, Model};

fn corpus_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("MITCAD_RVT_CORPUS")?;
    let dir = PathBuf::from(dir);
    dir.join("index.tsv").is_file().then_some(dir)
}

/// The corpus files present: (path relative to the corpus, content id,
/// release, kind).
fn files(dir: &Path) -> Vec<(String, String, Option<u32>, String)> {
    let index = std::fs::read_to_string(dir.join("index.tsv")).expect("the corpus index");
    let mut header: Vec<&str> = Vec::new();
    let mut out = Vec::new();
    for line in index.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if header.is_empty() {
            header = cols;
            continue;
        }
        let col = |name: &str| {
            header
                .iter()
                .position(|h| *h == name)
                .and_then(|i| cols.get(i))
                .copied()
                .unwrap_or_default()
        };
        let path = col("path");
        let lower = path.to_lowercase();
        if ![".rvt", ".rfa", ".rte", ".rft"]
            .iter()
            .any(|e| lower.ends_with(e))
            || !dir.join(path).is_file()
        {
            continue;
        }
        out.push((
            path.to_string(),
            col("sha256").chars().take(8).collect(),
            col("release").parse().ok(),
            col("kind").to_string(),
        ));
    }
    out
}

/// A gzip file of the reference, inflated (the reader's own gzip code).
fn gunzip(path: &Path) -> Option<Vec<u8>> {
    let data = std::fs::read(path).ok()?;
    let members = mitcad_rvt::gzip::members_from(&data, 0).ok()?;
    Some(members.into_iter().flat_map(|m| m.data).collect())
}

fn reference_json(dir: &Path, file: &str, name: &str) -> Option<Value> {
    let data = gunzip(&dir.join("reference").join(file).join(name))?;
    serde_json::from_slice(&data).ok()
}

/// Disagreements with the reference that are known and explained, by
/// content id: (element, field, why).
const KNOWN_DIFFERENCES: &[(&str, &[i64], &str, &str)] = &[
    (
        "33271010",
        &[1557297],
        "material name",
        "the material's own m_name; the reference reads another name block",
    ),
    (
        "33271010",
        &[
            1240813, 1240814, 1240920, 1240921, 1240924, 1240959, 1240960,
        ],
        "type",
        "the stair support's own m_typeId; the reference reads another stair type's support",
    ),
];

fn known(id8: &str, element: i64, field: &str) -> bool {
    KNOWN_DIFFERENCES
        .iter()
        .any(|(f, ids, what, _)| *f == id8 && *what == field && ids.contains(&element))
}

/// A level disagreement explained: the reference binds by one of its
/// geometric or indirect rules (nearest level below the box, a host of
/// the same class, a constraint read by position), and this reader's
/// level is not the element's own `m_assocLevelId` either but a link its
/// records hold (a work plane's, a host's, a stair's, the first of the
/// two levels a sloped beam names).
fn level_explained(source: Option<&str>, ours: &Element) -> bool {
    const HEURISTIC: [&str; 6] = [
        "partition_element_record_base_elevation",
        "partition_element_record_same_class_host",
        "partition_element_record_base_constraint",
        "partition_element_record_level_object",
        "partition_element_record_plumbing_level_object",
        "partition_element_record_host",
    ];
    source.is_some_and(|s| HEURISTIC.contains(&s))
        && ours.level_field.as_deref() != Some("m_assocLevelId")
}

#[derive(Default)]
struct Tally {
    ok: BTreeMap<&'static str, usize>,
    explained: BTreeMap<&'static str, usize>,
    failed: Vec<String>,
}

impl Tally {
    fn check(
        &mut self,
        field: &'static str,
        agree: bool,
        explained: bool,
        what: impl FnOnce() -> String,
    ) {
        if agree {
            *self.ok.entry(field).or_default() += 1;
        } else if explained {
            *self.explained.entry(field).or_default() += 1;
        } else {
            self.failed.push(format!("{field}: {}", what()));
        }
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * (1.0 + b.abs())
}

/// The reference's fields of an element: name -> value (or id).
fn fields(element: &Value) -> HashMap<String, Value> {
    element["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| {
            let name = f["name"].as_str()?.to_string();
            let value = if f.get("value").is_some() {
                f["value"].clone()
            } else {
                f["id"].clone()
            };
            Some((name, value))
        })
        .collect()
}

/// The elements and materials of a 2024 or 2025 project against the
/// reference's element list.
fn compare_elements(id8: &str, model: &Model, reference: &Value, tally: &mut Tally) {
    let ours: HashMap<i64, &Element> = model.elements.iter().map(|e| (e.id, e)).collect();
    let materials: HashMap<i64, &mitcad_rvt::elements::Material> =
        model.materials.iter().map(|m| (m.id, m)).collect();
    for element in reference.as_array().into_iter().flatten() {
        let Some(id) = element["id"].as_i64() else {
            continue;
        };
        let f = fields(element);
        if element["class_name"] == "Material" {
            let typed = &element["typed"];
            let transparency = typed["transparency"].as_f64();
            // The reference misreads some records (a denormal or huge
            // transparency and a black colour): not compared.
            if transparency.is_some_and(|t| t != 0.0 && (t.abs() < 1e-6 || t > 100.0)) {
                *tally
                    .explained
                    .entry("material (reference misread)")
                    .or_default() += 1;
                continue;
            }
            let Some(m) = materials.get(&id) else {
                tally.failed.push(format!("material {id} not read"));
                continue;
            };
            tally.check(
                "material name",
                Some(m.name.as_str()) == typed["name"].as_str(),
                known(id8, id, "material name"),
                || format!("{id}: {:?} vs {}", m.name, typed["name"]),
            );
            if let Some(c) = typed["color"].as_u64() {
                let theirs = [
                    (c & 0xff) as u8,
                    ((c >> 8) & 0xff) as u8,
                    ((c >> 16) & 0xff) as u8,
                ];
                tally.check("material color", m.color == Some(theirs), false, || {
                    format!("{id}: {:?} vs {theirs:?}", m.color)
                });
            }
            if let Some(t) = transparency {
                tally.check(
                    "material transparency",
                    m.transparency.is_some_and(|ours| close(ours, t)),
                    false,
                    || format!("{id}: {:?} vs {t}", m.transparency),
                );
            }
            continue;
        }
        let Some(e) = ours.get(&id).filter(|e| e.partition.is_some()) else {
            tally.failed.push(format!(
                "{id} ({}) not listed with records",
                element["class_name"]
            ));
            continue;
        };
        *tally.ok.entry("listed").or_default() += 1;
        if let Some(c) = f.get("m_builtinCategory").and_then(Value::as_i64) {
            tally.check("category", e.category == Some(c), false, || {
                format!("{id}: {:?} vs {c}", e.category)
            });
        }
        if let Some(c) = f.get("m_revit_class").and_then(Value::as_str) {
            tally.check("class", e.class.as_deref() == Some(c), false, || {
                format!("{id}: {:?} vs {c}", e.class)
            });
        }
        if let Some(l) = f.get("m_levelId").and_then(Value::as_i64) {
            let source = f.get("m_levelBindSource").and_then(Value::as_str);
            tally.check(
                "level",
                e.level == Some(l),
                level_explained(source, e),
                || {
                    format!(
                        "{id}: {:?} ({:?}) vs {l} ({source:?})",
                        e.level, e.level_field
                    )
                },
            );
        }
        if let Some(t) = f.get("m_type_id").and_then(Value::as_i64) {
            tally.check("type", e.type_id == Some(t), known(id8, id, "type"), || {
                format!("{id}: {:?} vs {t}", e.type_id)
            });
        }
        if let Some(t) = f.get("m_type_name").and_then(Value::as_str) {
            tally.check(
                "type name",
                e.type_name.as_deref() == Some(t),
                known(id8, id, "type"),
                || format!("{id}: {:?} vs {t:?}", e.type_name),
            );
        }
        if let Some(n) = f.get("m_family_name").and_then(Value::as_str) {
            tally.check(
                "family name",
                e.family_name.as_deref() == Some(n),
                false,
                || format!("{id}: {:?} vs {n:?}", e.family_name),
            );
        }
        // The box as the record stores it: the reference's own where it
        // did not reshape it (walls trimmed at joins, columns cut by walls,
        // profiles of the type).
        let reshaped = f
            .keys()
            .any(|k| k.ends_with("_body_source") || k == "m_type_profile_source");
        if let (Some(b), false) = (e.bbox, reshaped) {
            let ours = [
                (b[0] + b[3]) / 2.0,
                (b[1] + b[4]) / 2.0,
                b[2],
                b[3] - b[0],
                b[4] - b[1],
                b[5] - b[2],
            ];
            let keys = [
                "m_locationX",
                "m_locationY",
                "m_locationZ",
                "m_bboxWidth",
                "m_bboxDepth",
                "m_bboxHeight",
            ];
            if keys.iter().all(|k| f.contains_key(*k)) {
                let theirs: Vec<f64> = keys
                    .iter()
                    .map(|k| f[*k].as_f64().unwrap_or(f64::NAN))
                    .collect();
                tally.check(
                    "box",
                    ours.iter().zip(&theirs).all(|(a, b)| close(*a, *b)),
                    false,
                    || format!("{id}: {ours:?} vs {theirs:?}"),
                );
            }
        }
        // The transform: its origin, and the X axis of an upright one (the
        // reference gives the plan direction), or all three axes.
        if let (Some(x), Some(y)) = (
            f.get("m_instance_origin_x").and_then(Value::as_f64),
            f.get("m_instance_origin_y").and_then(Value::as_f64),
        ) {
            tally.check(
                "transform origin",
                e.transform
                    .is_some_and(|t| close(t[9], x) && close(t[10], y)),
                false,
                || {
                    format!(
                        "{id}: {:?} vs [{x}, {y}]",
                        e.transform.map(|t| [t[9], t[10]])
                    )
                },
            );
        }
        if let (Some(t), Some(ax), Some(ay)) = (
            e.transform,
            f.get("m_instance_x_axis_x").and_then(Value::as_f64),
            f.get("m_instance_x_axis_y").and_then(Value::as_f64),
        ) && close(t[8], 1.0)
        {
            tally.check(
                "transform axis",
                close(t[0], ax) && close(t[3], ay),
                false,
                || format!("{id}: [{}, {}] vs [{ax}, {ay}]", t[0], t[3]),
            );
        }
        if let Some(t) = e.transform {
            let names = ["xx", "xy", "xz", "yx", "yy", "yz", "zx", "zy", "zz"];
            let theirs: Vec<Option<f64>> = names
                .iter()
                .map(|n| {
                    f.get(&format!("m_curtain_axis_{n}"))
                        .and_then(Value::as_f64)
                })
                .collect();
            if theirs.iter().all(Option::is_some) {
                // Ours by rows: the axes are the columns.
                let axes = [t[0], t[3], t[6], t[1], t[4], t[7], t[2], t[5], t[8]];
                tally.check(
                    "transform axes",
                    axes.iter()
                        .zip(&theirs)
                        .all(|(a, b)| close(*a, b.unwrap_or(f64::NAN))),
                    false,
                    || format!("{id}: {axes:?} vs {theirs:?}"),
                );
            }
        }
    }
}

/// What one file's check found: whether it opened, its failures and the
/// lines it prints.
#[derive(Default)]
struct FileCheck {
    opened: bool,
    failures: Vec<String>,
    lines: Vec<String>,
}

/// Checks one corpus file (path relative to the corpus, content id,
/// release, kind).
fn check_file(dir: &Path, path: &str, id8: &str, release: Option<u32>, kind: &str) -> FileCheck {
    let mut check = FileCheck::default();
    let fail = |what: String| format!("{id8}: {what}");
    let file = match RvtFile::open(&dir.join(path)) {
        Ok(f) => f,
        Err(e) => {
            check.failures.push(fail(format!("does not open: {e}")));
            return check;
        }
    };
    check.opened = true;
    // Metadata.
    if file.release() != release {
        check.failures.push(fail(format!(
            "release {:?}, the index says {release:?}",
            file.release()
        )));
    }
    if file.release_by_table() != file.release() {
        check.failures.push(fail(format!(
            "the element table's class names release {:?}",
            file.release_by_table()
        )));
    }
    let family = kind == "family";
    if (file.kind == mitcad_rvt::Kind::Family) != family {
        check
            .failures
            .push(fail(format!("kind {:?}, the index says {kind}", file.kind)));
    }
    if family && file.part_atom().and_then(|a| a.title).is_none() {
        check
            .failures
            .push(fail("a family without its PartAtom title".into()));
    }
    // The preview: the reference's image byte for byte, or none.
    let theirs = std::fs::read(dir.join("reference").join(path).join("preview.png")).ok();
    if file.preview() != theirs {
        check.failures.push(fail(format!(
            "preview of {:?} bytes, the reference's {:?}",
            file.preview().map(|p| p.len()),
            theirs.map(|p| p.len())
        )));
    }
    // The schema: whole, every encoding known, every class the
    // reference finds.
    let schema = match file.schema() {
        Ok(s) => s,
        Err(e) => {
            check.failures.push(fail(format!("schema: {e}")));
            return check;
        }
    };
    if let Some(stopped) = &schema.stopped {
        check
            .failures
            .push(fail(format!("the schema stops: {stopped}")));
    }
    for unknown in schema.unknown_encodings() {
        check
            .failures
            .push(fail(format!("unknown field encoding {unknown}")));
    }
    if let Some(reference) = reference_json(dir, path, "schema.json.gz") {
        let missing: Vec<&str> = reference["classes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c["name"].as_str())
            .filter(|n| schema.tag(n).is_none())
            .collect();
        if !missing.is_empty() {
            check.failures.push(fail(format!(
                "classes the reference finds and the schema lacks: {missing:?}"
            )));
        }
    }
    // The element table and the records.
    let model = match Model::read(&file, &schema) {
        Ok(m) => m,
        Err(e) => {
            check.failures.push(fail(format!("elements: {e}")));
            return check;
        }
    };
    // The geometry the projects of 2023 on keep (mitcad#333): every
    // record walks and reads, and every edge's two sides land within
    // 0.001 ft of each other.
    if !family && release.is_some_and(|r| r >= 2023) {
        if model.geometry.is_empty() || model.geometry_errors > 0 {
            check.failures.push(fail(format!(
                "geometry of {} elements, {} records not walked",
                model.geometry.len(),
                model.geometry_errors
            )));
        }
        for (id, g) in &model.geometry {
            if let Some(note) = g.notes.first() {
                check
                    .failures
                    .push(fail(format!("element {id}'s geometry: {note}")));
            }
            let gap = g.solids.iter().map(|s| s.seam_gap()).fold(0.0, f64::max);
            if gap > 0.001 {
                check.failures.push(fail(format!(
                    "element {id}'s edges' sides lie {gap:.2e} ft apart"
                )));
            }
        }
    }
    let declared: BTreeSet<i64> = model.declared.iter().map(|d| d.id).collect();
    // The reference reads the table from 24 bytes into it, in records
    // of 28 (to 2023) or 40 bytes: it leaves out the first record and
    // (but in projects of 28-byte records) reads one past the last (the
    // table's trailing word, or zeros). Its ids, in order, without that
    // one, are ours without the first;
    // on 40-byte records its second id is the element's (its first is
    // the next record's original id). A table it reads in other strides
    // it did not parse.
    if let Some(reference) = reference_json(dir, path, "elem-table.json.gz") {
        let stride = reference["layout"]["stride"].as_u64();
        let key = if stride == Some(40) {
            "id_secondary"
        } else {
            "id_primary"
        };
        let mut theirs: Vec<i64> = reference["records"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r[key].as_i64())
            .collect();
        let ours: Vec<i64> = model.declared.iter().skip(1).map(|d| d.id).collect();
        if theirs.len() == ours.len() + 1 {
            theirs.pop();
        }
        if !matches!(stride, Some(28 | 40)) {
            check.lines.push(format!(
                "{id8}: the reference reads the element table in records of {stride:?} bytes: not compared"
            ));
        } else if theirs != ours {
            let (a, b): (BTreeSet<i64>, BTreeSet<i64>) = (
                ours.iter().copied().collect(),
                theirs.iter().copied().collect(),
            );
            check.failures.push(fail(format!(
                "declared ids: {} ours, {} the reference's, {} only ours, {} only theirs",
                declared.len(),
                theirs.len(),
                a.difference(&b).count(),
                b.difference(&a).count()
            )));
        }
    }
    if model.elements.len() != declared.len() {
        check
            .failures
            .push(fail("not every declared element is listed".into()));
    }
    let without = model
        .elements
        .iter()
        .filter(|e| e.partition.is_none())
        .count();
    if without > 0 {
        check
            .failures
            .push(fail(format!("{without} declared elements without records")));
    }
    if model.header_errors > 0 || model.object_errors > 0 {
        check.failures.push(fail(format!(
            "{} header and {} object records not walked",
            model.header_errors, model.object_errors
        )));
    }
    // Records read in part stop only at data of extensible storage
    // schemas (and one marshalled element), which the file's class
    // schema does not describe.
    for e in &model.elements {
        if let Some(error) = &e.error
            && !(error.contains("ESEntity") || error.contains("XMarshalledRec"))
        {
            check
                .failures
                .push(fail(format!("element {}: {error}", e.id)));
        }
    }
    let partial = model.objects_partial;
    let mut line = format!(
        "{id8} {} {:?} {:>6} declared, {:>6} objects walked whole, {partial} in part (extensible storage)",
        release.map_or("?".into(), |r| r.to_string()),
        file.kind,
        declared.len(),
        model.objects_walked
    );
    // The 2024 and 2025 projects' elements against the reference.
    if !family
        && matches!(release, Some(2024 | 2025))
        && let Some(reference) = reference_json(dir, path, "elements.json.gz")
    {
        let mut tally = Tally::default();
        compare_elements(id8, &model, &reference, &mut tally);
        line += &format!("; agree {:?}", tally.ok);
        if !tally.explained.is_empty() {
            line += &format!("; explained {:?}", tally.explained);
        }
        for f in tally.failed.iter().take(20) {
            check.failures.push(fail(f.clone()));
        }
        if tally.failed.len() > 20 {
            check
                .failures
                .push(fail(format!("... {} more", tally.failed.len() - 20)));
        }
    }
    check.lines.push(line);
    check
}

/// The files checked at once (mitcad#875): `MITCAD_CORPUS_JOBS` when set
/// (1: one after another), else a quarter of the cores, at most 4 (one
/// file's model took up to about 1 GB).
fn jobs(files: usize) -> usize {
    let given = std::env::var("MITCAD_CORPUS_JOBS")
        .ok()
        .and_then(|text| text.trim().parse::<usize>().ok())
        .filter(|&n| n > 0);
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    given.unwrap_or((cores / 4).clamp(1, 4)).min(files.max(1))
}

#[test]
fn every_file_and_the_reference() {
    let Some(dir) = corpus_dir() else {
        eprintln!("MITCAD_RVT_CORPUS is not set: the corpus test is skipped");
        return;
    };
    // The files several at once, each in a thread of its own, the largest
    // first, their lines in the index's order (mitcad#875: one after
    // another this was the longest crate test of the check).
    let list = files(&dir);
    let mut order: Vec<usize> = (0..list.len()).collect();
    let size = |i: &usize| std::fs::metadata(dir.join(&list[*i].0)).map_or(0, |m| m.len());
    order.sort_by_key(|i| std::cmp::Reverse(size(i)));
    let next = AtomicUsize::new(0);
    let results: Vec<Mutex<Option<FileCheck>>> = list.iter().map(|_| Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for _ in 0..jobs(list.len()) {
            scope.spawn(|| {
                while let Some(&i) = order.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let (path, id8, release, kind) = &list[i];
                    let check = check_file(&dir, path, id8, *release, kind);
                    *results[i].lock().unwrap() = Some(check);
                }
            });
        }
    });
    let mut failures = Vec::new();
    let mut opened = 0;
    for result in results {
        let check = result.into_inner().unwrap().expect("every file checked");
        for line in &check.lines {
            eprintln!("{line}");
        }
        opened += usize::from(check.opened);
        failures.extend(check.failures);
    }
    assert!(opened > 0, "no corpus file found under {}", dir.display());
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
