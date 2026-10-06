// SPDX-License-Identifier: MIT
//! Command-line inspector for FreeCAD .FCStd files: the archive's entries,
//! the document's objects with their types, states, placements and links,
//! and the parsed document as JSON.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use mitcad_freecad::{FcstdFile, Value, version};

const USAGE: &str = "usage: mitcad-fcstd-inspect <command> <file.FCStd> ...
  list <file>        the archive's entries with sizes and methods
  info <file>        version, objects by type, warnings
  objects <file>     every object: name, type, label, state, visibility,
                     placement, stored shape and references
  json <file>        the parsed document (and view data) as JSON
  enums <file>       every enumeration property with its value as text";

fn open(path: &str) -> Result<FcstdFile, String> {
    FcstdFile::open(Path::new(path)).map_err(|e| format!("{path}: {e}"))
}

fn cmd_list(path: &str) -> Result<(), String> {
    for e in open(path)?.entries() {
        println!(
            "{:>10} {:>10} {:<8} {}",
            e.size,
            e.compressed_size,
            e.method.to_string(),
            e.name
        );
    }
    Ok(())
}

fn cmd_info(path: &str) -> Result<(), String> {
    let file = open(path)?;
    let doc = &file.document;
    println!(
        "{}: FreeCAD {} (enumerations of {}), schema {}, {} objects{}",
        doc.label().unwrap_or("?"),
        doc.version(),
        version::table_release(&doc.version()).unwrap_or("none"),
        doc.schema_version,
        doc.objects.len(),
        if file.gui.is_some() {
            ", view data"
        } else {
            ", no view data"
        }
    );
    let mut types: BTreeMap<&str, usize> = BTreeMap::new();
    for o in &doc.objects {
        *types.entry(&o.type_name).or_default() += 1;
    }
    for (t, n) in types {
        println!("  {n:>4} {t}");
    }
    let stale: Vec<&str> = doc
        .objects
        .iter()
        .filter(|o| o.state.is_stale())
        .map(|o| o.name.as_str())
        .collect();
    if !stale.is_empty() {
        println!("  touched, invalid or failed: {}", stale.join(", "));
    }
    for w in &file.warnings {
        println!("  warning: {w}");
    }
    Ok(())
}

fn cmd_objects(path: &str) -> Result<(), String> {
    let file = open(path)?;
    for o in &file.document.objects {
        let mut line = format!("{} [{}] \"{}\"", o.name, o.type_name, o.label());
        if o.state.is_stale() {
            line += &format!(" {:?}", o.state);
        }
        if !file.visible(o) {
            line += " hidden";
        }
        if let Some(p) = o.placement().filter(|p| !p.is_identity(0.0)) {
            line += &format!(" at {:?} {:?}", p.position, p.rotation);
        }
        if let Some(shape) = o.shape_file() {
            line += &format!(" shape {shape}");
        }
        println!("{line}");
        for (property, link) in o.references() {
            let subs: Vec<&str> = link.subs.iter().map(|s| s.name.as_str()).collect();
            println!(
                "    {property} -> {}{}{}",
                link.file
                    .as_deref()
                    .map_or(String::new(), |f| format!("{f}#")),
                link.object,
                if subs.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", subs.join(", "))
                }
            );
        }
    }
    Ok(())
}

fn cmd_json(path: &str) -> Result<(), String> {
    let file = open(path)?;
    let json = serde_json::json!({
        "document": file.document,
        "gui": file.gui,
        "warnings": file.warnings,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn cmd_enums(path: &str) -> Result<(), String> {
    let file = open(path)?;
    let doc = &file.document;
    for o in &doc.objects {
        for p in &o.properties {
            if let Value::Enumeration(e) = &p.value {
                println!(
                    "{}.{} = {} ({})",
                    o.name,
                    p.name,
                    e.index,
                    doc.enum_text(o, &p.name)
                        .unwrap_or_else(|| "unknown".to_owned())
                );
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [cmd, file] => match cmd.as_str() {
            "list" => cmd_list(file),
            "info" => cmd_info(file),
            "objects" => cmd_objects(file),
            "json" => cmd_json(file),
            "enums" => cmd_enums(file),
            _ => Err(USAGE.to_owned()),
        },
        _ => Err(USAGE.to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
