// SPDX-License-Identifier: MIT
//! The libraries' JSON commands (`core/model/src/api/commands.md`,
//! "Component libraries"). They need no project: the application and
//! `mitcad-cli` run them with the cache of fetched libraries. A fetch's
//! failure (the network, signing in, a URL that is no library) is an
//! answer with `error` (`class`, `message`, `detail`) and `log`, as the
//! remote commands give; other failures are errors.

use std::collections::BTreeSet;

use gix::ObjectId;
use serde_json::{Value, json};

use super::manifest::{
    ACCEPTED_LICENSES, IndexEntry, Manifest, ManifestComponent, license_note, matches, search_words,
};
use super::{LibraryCache, LibraryRepo, LibraryVersion, RepoKind, publish};
use crate::VcsError;
use crate::api::{optional, text};
use crate::remote::commands::answer;
use crate::remote::{Control, GitCli};
use mitcad_model::Configurations;

fn invalid(message: impl Into<String>) -> VcsError {
    VcsError::Command(message.into())
}

/// Runs a library command; the answer as JSON.
pub fn command(
    json: &str,
    cache: &LibraryCache,
    git: Option<&GitCli>,
    control: Option<&Control>,
) -> Result<String, VcsError> {
    let command: Value =
        serde_json::from_str(json).map_err(|e| invalid(format!("not valid JSON: {e}")))?;
    let name = text(&command, "cmd")?;
    let answer = match name {
        "licenses" => json!({"licenses": ACCEPTED_LICENSES.iter()
            .map(|(id, note)| json!({"id": id, "note": note})).collect::<Vec<_>>()}),
        "library_fetch" => {
            let url = text(&command, "url")?;
            let mut log = Vec::new();
            let git = match git {
                Some(git) => Ok(git.clone()),
                None => GitCli::find(),
            };
            let result = git
                .map_err(VcsError::from)
                .and_then(|git| cache.fetch(url, &git, control, &mut log));
            let (mut value, _) = answer(
                result,
                |_| json!({"url": crate::remote::redact(url), "kind": null, "id": null}),
            )?;
            value["log"] = json!(log);
            if let Some(list) = value.get("versions").and_then(Value::as_array).cloned() {
                let versions: Vec<LibraryVersion> =
                    serde_json::from_value(Value::Array(list)).unwrap_or_default();
                value["versions"] = versions_json(&versions);
            }
            value
        }
        "library_list" => list(cache),
        "library_show" => show(cache, &command)?,
        "library_search" => search(cache, &command)?,
        "library_preview" => preview(cache, &command)?,
        "library_diff" => diff(cache, &command)?,
        "library_check" => publish::check_command(&command)?,
        "library_init" => publish::init_command(&command)?,
        "library_add" => publish::add_command(&command)?,
        "index_entry" => publish::index_entry_command(&command)?,
        other => return Err(invalid(format!("unknown library command '{other}'"))),
    };
    Ok(answer.to_string())
}

/// The repository a command names by `url` (else `id`).
fn repo_of(cache: &LibraryCache, command: &Value) -> Result<LibraryRepo, VcsError> {
    if let Some(url) = optional(command, "url")
        && let Some(repo) = cache.open_url(url)
    {
        return Ok(repo);
    }
    if let Some(id) = optional(command, "id")
        && let Some(repo) = cache.open_id(id)
    {
        return Ok(repo);
    }
    Err(VcsError::NotFound(format!(
        "the library {} has not been fetched here",
        optional(command, "url")
            .or_else(|| optional(command, "id"))
            .unwrap_or("(none given)")
    )))
}

/// The commit a command asks for (`rev`), else the newest version.
fn commit_of(repo: &LibraryRepo, command: &Value) -> Result<ObjectId, VcsError> {
    match optional(command, "rev") {
        Some(rev) => repo.resolve(rev),
        None => match repo.newest()? {
            Some(version) => repo.resolve(&version.rev),
            None => Err(VcsError::NotFound("the library has no versions".to_owned())),
        },
    }
}

fn versions_json(versions: &[LibraryVersion]) -> Value {
    Value::Array(
        versions
            .iter()
            .map(|v| {
                let mut value = json!(v);
                value["text"] = json!(v.text());
                value
            })
            .collect(),
    )
}

/// `library_list`: every fetched library and index.
fn list(cache: &LibraryCache) -> Value {
    let mut libraries = Vec::new();
    for dir in cache.repos() {
        let Ok(repo) = LibraryRepo::open(&dir) else {
            continue;
        };
        let Ok(Some(head)) = repo.head() else {
            continue;
        };
        let versions = repo.versions().unwrap_or_default();
        let mut value = json!({
            "dir": dir.to_string_lossy(),
            "url": repo.url(),
            "kind": repo.kind(head),
            "versions": versions_json(&versions),
        });
        match repo.kind(head) {
            RepoKind::Library => {
                let newest = versions
                    .first()
                    .and_then(|v| repo.resolve(&v.rev).ok())
                    .unwrap_or(head);
                if let Ok(manifest) = repo.manifest(newest) {
                    value["id"] = json!(manifest.id);
                    value["name"] = json!(manifest.name);
                    value["description"] = json!(manifest.description);
                    value["license"] = json!(manifest.license);
                    value["authors"] = json!(manifest.authors);
                    value["components"] = json!(manifest.components.len());
                    value["problems"] = json!(manifest.problems());
                }
            }
            RepoKind::Index => {
                if let Ok((index, entries, problems)) = repo.index(head) {
                    value["name"] = json!(index.name);
                    value["description"] = json!(index.description);
                    value["entries"] = json!(entries.len());
                    value["problems"] = json!({"errors": [], "warnings": problems});
                }
            }
            RepoKind::Unknown => {}
        }
        libraries.push(value);
    }
    json!({"root": cache.root.to_string_lossy(), "libraries": libraries})
}

/// A component's design's configuration table at a commit, if it has one.
fn component_table(repo: &LibraryRepo, commit: ObjectId, path: &str) -> Option<Configurations> {
    let data = repo.read(commit, path).ok()??;
    let value: Value = serde_json::from_slice(&data).ok()?;
    serde_json::from_value(value.get("configurations")?.clone()).ok()
}

fn component_json(
    repo: &LibraryRepo,
    commit: ObjectId,
    manifest: &Manifest,
    component: &ManifestComponent,
    tables: bool,
) -> Value {
    let license = manifest.license_of(component);
    let mut value = json!({
        "id": component.id,
        "name": component.name,
        "path": component.path,
        "category": component.category,
        "standard": component.standard,
        "description": component.description,
        "keywords": component.keywords,
        "license": license,
        "license_note": license.as_deref().and_then(license_note),
        "preview": !component.preview.is_empty(),
        "designation": component.designation,
    });
    if tables && let Some(table) = component_table(repo, commit, &component.path) {
        value["configurations"] = json!({
            "selectors": table.selectors,
            "selector_values": table.selector_values(),
            "default": table.default_row().map(|r| r.name.clone()),
            "rows": table.rows.iter().map(|r| json!({"name": r.name, "select": r.select}))
                .collect::<Vec<_>>(),
        });
    }
    value
}

/// `library_show`: a library at a version (its manifest, components with
/// their tables, versions), or an index's entries.
fn show(cache: &LibraryCache, command: &Value) -> Result<Value, VcsError> {
    let repo = repo_of(cache, command)?;
    let commit = commit_of(&repo, command)?;
    let versions = repo.versions()?;
    let base = json!({
        "dir": repo.dir().to_string_lossy(),
        "url": repo.url(),
        "rev": commit.to_string(),
        "labels": repo.labels(commit)?,
        "versions": versions_json(&versions),
    });
    let mut value = base;
    match repo.kind(commit) {
        RepoKind::Library => {
            let manifest = repo.manifest(commit)?;
            value["kind"] = json!("library");
            value["problems"] = json!(manifest.problems());
            value["components"] = Value::Array(
                manifest
                    .components
                    .iter()
                    .map(|c| component_json(&repo, commit, &manifest, c, true))
                    .collect(),
            );
            value["manifest"] = json!(manifest);
        }
        RepoKind::Index => {
            let (index, entries, problems) = repo.index(commit)?;
            value["kind"] = json!("index");
            value["name"] = json!(index.name);
            value["description"] = json!(index.description);
            value["entries"] = json!(entries);
            value["problems"] = json!({"errors": [], "warnings": problems});
        }
        RepoKind::Unknown => {
            return Err(VcsError::File(
                "the repository is neither a library nor an index at that version".to_owned(),
            ));
        }
    }
    Ok(value)
}

/// The licences a search lets through: None for any.
fn license_filter(command: &Value) -> Option<BTreeSet<String>> {
    let list = command.get("licenses")?.as_array()?;
    Some(
        list.iter()
            .filter_map(Value::as_str)
            .map(|l| l.trim().to_ascii_lowercase())
            .collect(),
    )
}

fn license_passes(
    license: Option<&str>,
    filter: &Option<BTreeSet<String>>,
    unlicensed: bool,
) -> bool {
    match license.map(str::trim).filter(|l| !l.is_empty()) {
        None => unlicensed,
        Some(license) => filter
            .as_ref()
            .is_none_or(|set| set.contains(&license.to_ascii_lowercase())),
    }
}

/// `library_search`: the components of every fetched library (at its
/// newest version) and the libraries the fetched indexes list, by words,
/// licence and category. Items without a licence only with `unlicensed`.
fn search(cache: &LibraryCache, command: &Value) -> Result<Value, VcsError> {
    let words = search_words(optional(command, "text").unwrap_or(""));
    let filter = license_filter(command);
    let unlicensed = command
        .get("unlicensed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let category = optional(command, "category");
    let only: Option<BTreeSet<String>> =
        command.get("libraries").and_then(Value::as_array).map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        });
    let mut components = Vec::new();
    let mut libraries = Vec::new();
    let mut fetched_ids = BTreeSet::new();
    let mut hidden = 0;
    let mut indexes = Vec::new();
    for dir in cache.repos() {
        let Ok(repo) = LibraryRepo::open(&dir) else {
            continue;
        };
        let Ok(Some(head)) = repo.head() else {
            continue;
        };
        let url = repo.url().unwrap_or_default();
        match repo.kind(head) {
            RepoKind::Library => {
                let Some(version) = repo.newest()? else {
                    continue;
                };
                let commit = repo.resolve(&version.rev)?;
                let Ok(manifest) = repo.manifest(commit) else {
                    continue;
                };
                fetched_ids.insert(manifest.id.clone());
                if only
                    .as_ref()
                    .is_some_and(|set| !set.contains(&manifest.id) && !set.contains(&url))
                {
                    continue;
                }
                for c in &manifest.components {
                    let license = manifest.license_of(c);
                    if category.is_some_and(|wanted| wanted != c.category) {
                        continue;
                    }
                    let keywords: Vec<&str> = c.keywords.iter().map(String::as_str).collect();
                    let mut texts = vec![
                        c.name.as_str(),
                        c.id.as_str(),
                        c.standard.as_str(),
                        c.description.as_str(),
                        c.category.as_str(),
                        manifest.name.as_str(),
                        manifest.id.as_str(),
                    ];
                    texts.extend(keywords);
                    if !matches(&words, &texts) {
                        continue;
                    }
                    if !license_passes(license.as_deref(), &filter, unlicensed) {
                        hidden += 1;
                        continue;
                    }
                    let mut item = component_json(&repo, commit, &manifest, c, false);
                    item["library"] = json!(manifest.id);
                    item["library_name"] = json!(manifest.name);
                    item["url"] = json!(url);
                    item["rev"] = json!(version.rev);
                    item["version"] = json!(version.text());
                    item["authors"] = json!(manifest.authors);
                    components.push(item);
                }
            }
            RepoKind::Index => indexes.push((repo, head, url)),
            RepoKind::Unknown => {}
        }
    }
    for (repo, head, index_url) in indexes {
        let Ok((index, entries, _)) = repo.index(head) else {
            continue;
        };
        for entry in entries {
            if !entry_matches(&entry, &words) {
                continue;
            }
            if !license_passes(entry.license.as_deref(), &filter, unlicensed) {
                hidden += 1;
                continue;
            }
            let mut item = json!(entry);
            item["index"] = json!(index.name);
            item["index_url"] = json!(index_url);
            item["fetched"] = json!(fetched_ids.contains(&entry.id));
            item["license_note"] = json!(entry.license.as_deref().and_then(license_note));
            libraries.push(item);
        }
    }
    Ok(json!({"components": components, "libraries": libraries, "hidden": hidden}))
}

fn entry_matches(entry: &IndexEntry, words: &[String]) -> bool {
    let mut texts: Vec<&str> = vec![&entry.name, &entry.id, &entry.description];
    texts.extend(entry.tags.iter().map(String::as_str));
    for c in &entry.components {
        texts.push(&c.name);
        texts.push(&c.id);
        texts.push(&c.standard);
        texts.extend(c.tags.iter().map(String::as_str));
    }
    matches(words, &texts)
}

/// `library_preview`: a component's preview image (PNG, base64), or null.
fn preview(cache: &LibraryCache, command: &Value) -> Result<Value, VcsError> {
    let repo = repo_of(cache, command)?;
    let commit = commit_of(&repo, command)?;
    let manifest = repo.manifest(commit)?;
    let id = text(command, "component")?;
    let component = manifest
        .component(id)
        .ok_or_else(|| VcsError::NotFound(format!("{} has no component '{id}'", manifest.id)))?;
    if component.preview.is_empty() {
        return Ok(json!({"png": null}));
    }
    let data = repo.read(commit, &component.preview)?;
    Ok(match data {
        Some(data)
            if data.len() <= super::manifest::MAX_PREVIEW && data.starts_with(b"\x89PNG") =>
        {
            json!({"png": mitcad_model::library::encode_base64(&data)})
        }
        _ => json!({"png": null}),
    })
}

/// `library_diff`: what changes for parts of a library between two
/// versions: the design's changes (parameters, features, the
/// configuration table), the row's values, the designation and the
/// licence; a component or row the newer version lacks.
fn diff(cache: &LibraryCache, command: &Value) -> Result<Value, VcsError> {
    let id = text(command, "id")?;
    let url = optional(command, "url").unwrap_or("");
    let from = text(command, "from")?;
    let to = text(command, "to")?;
    let (repo, from_commit) = cache.find(id, url, from)?;
    let to_commit = repo.resolve(to)?;
    let parts = command
        .get("parts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let from_manifest = repo.manifest(from_commit)?;
    let to_manifest = repo.manifest(to_commit)?;
    let mut out = Vec::new();
    for part in parts {
        let component = part.get("component").and_then(Value::as_str).unwrap_or("");
        let config = part.get("config").and_then(Value::as_str);
        let mut value = json!({"component": component, "config": config});
        let (Ok(old), Ok(new)) = (
            repo.source(from_commit, component, url),
            repo.source(to_commit, component, url),
        ) else {
            value["missing"] = json!(true);
            value["changed"] = json!(true);
            value["text"] = json!(format!(
                "{component} is not in {}: the part cannot follow that version",
                short(&to_commit)
            ));
            out.push(value);
            continue;
        };
        let mut lines = Vec::new();
        // The design without its configuration table: the row's values
        // are compared on their own, other rows do not concern the part.
        let without_table = |text: &str| -> String {
            match serde_json::from_str::<Value>(text) {
                Ok(mut value) => {
                    if let Some(object) = value.as_object_mut() {
                        object.remove("configurations");
                    }
                    value.to_string()
                }
                Err(_) => text.to_owned(),
            }
        };
        let design =
            mitcad_model::diff::diff_files(&without_table(&old.text), &without_table(&new.text))
                .map_err(|e| VcsError::File(e.to_string()))?;
        if let Some(config) = config {
            let row = |text: &str| {
                let value: Value = serde_json::from_str(text).ok()?;
                let table: Configurations =
                    serde_json::from_value(value.get("configurations")?.clone()).ok()?;
                table.row(config).cloned()
            };
            match (row(&old.text), row(&new.text)) {
                (_, None) => {
                    value["missing"] = json!(true);
                    lines.push(format!(
                        "{config} is not a configuration of {} in {}",
                        new.name,
                        short(&to_commit)
                    ));
                }
                (Some(a), Some(b)) if a != b => {
                    let names: BTreeSet<&String> = a.values.keys().chain(b.values.keys()).collect();
                    for name in names {
                        let (x, y) = (a.values.get(name), b.values.get(name));
                        if x != y {
                            lines.push(format!(
                                "{config}: {name} {} -> {}",
                                x.map_or("-", String::as_str),
                                y.map_or("-", String::as_str)
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        if !design.identical {
            lines.push(format!("design: {}", design.summary));
        }
        if old.license != new.license {
            lines.push(format!("licence {} -> {}", old.license, new.license));
        }
        if old.path != new.path {
            lines.push(format!("file {} -> {}", old.path, new.path));
        }
        value["changed"] = json!(!lines.is_empty());
        value["missing"] = value.get("missing").cloned().unwrap_or(json!(false));
        value["lines"] = json!(lines);
        value["text"] = json!(if lines.is_empty() {
            "no changes".to_owned()
        } else {
            lines.join("; ")
        });
        value["design"] = json!(design);
        out.push(value);
    }
    let mut library = Vec::new();
    if from_manifest.license != to_manifest.license {
        library.push(format!(
            "the library's licence {} -> {}",
            from_manifest.license.as_deref().unwrap_or("none"),
            to_manifest.license.as_deref().unwrap_or("none")
        ));
    }
    if from_manifest.library_version != to_manifest.library_version {
        library.push(format!(
            "library version {} -> {}",
            from_manifest.library_version, to_manifest.library_version
        ));
    }
    Ok(json!({
        "id": id,
        "from": from_commit.to_string(),
        "to": to_commit.to_string(),
        "from_labels": repo.labels(from_commit)?,
        "to_labels": repo.labels(to_commit)?,
        "library": library,
        "parts": out,
    }))
}

fn short(id: &ObjectId) -> String {
    id.to_hex_with_len(7).to_string()
}

/// A library command's JSON answer as text for people (`mitcad-cli
/// library`); a fetch's failure is an error, as is a check that found
/// errors.
pub fn describe(command: &str, answer: &str) -> Result<String, VcsError> {
    use std::fmt::Write;
    let command: Value =
        serde_json::from_str(command).map_err(|e| invalid(format!("not valid JSON: {e}")))?;
    let answer: Value =
        serde_json::from_str(answer).map_err(|e| invalid(format!("not valid JSON: {e}")))?;
    let s = |v: &Value| v.as_str().unwrap_or("").to_owned();
    let versions = |v: &Value| -> String {
        v.as_array()
            .map(|a| {
                a.iter()
                    .map(|x| s(&x["text"]))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    };
    let mut out = String::new();
    match command.get("cmd").and_then(Value::as_str).unwrap_or("") {
        "library_fetch" => {
            if !answer["error"].is_null() {
                let mut message = s(&answer["error"]["message"]);
                let detail = s(&answer["error"]["detail"]);
                if !detail.is_empty() {
                    message.push_str(&format!("\n{detail}"));
                }
                return Err(VcsError::Command(message));
            }
            let what = if answer["kind"] == "index" {
                "index"
            } else {
                "library"
            };
            let _ = writeln!(
                out,
                "{} {what} {}{} from {}",
                if answer["cloned"] == true {
                    "Fetched"
                } else {
                    "Updated"
                },
                s(&answer["name"]),
                if s(&answer["id"]).is_empty() {
                    String::new()
                } else {
                    format!(" ({})", s(&answer["id"]))
                },
                s(&answer["url"])
            );
            let _ = writeln!(out, "Versions: {}", versions(&answer["versions"]));
            for warning in answer["problems"]["warnings"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let _ = writeln!(out, "Warning: {}", s(warning));
            }
            for error in answer["problems"]["errors"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let _ = writeln!(out, "Error: {}", s(error));
            }
        }
        "library_list" => {
            let _ = writeln!(out, "Libraries in {}:", s(&answer["root"]));
            for l in answer["libraries"].as_array().into_iter().flatten() {
                if l["kind"] == "index" {
                    let _ = writeln!(
                        out,
                        "  index {}: {} entries, {}",
                        s(&l["name"]),
                        l["entries"],
                        s(&l["url"])
                    );
                } else {
                    let _ = writeln!(
                        out,
                        "  {} {}: {} components, {}, {}; versions {}",
                        s(&l["id"]),
                        s(&l["name"]),
                        l["components"],
                        l["license"].as_str().unwrap_or("no licence"),
                        s(&l["url"]),
                        versions(&l["versions"])
                    );
                }
            }
        }
        "library_show" => {
            if answer["kind"] == "index" {
                let _ = writeln!(
                    out,
                    "Index {}: {}",
                    s(&answer["name"]),
                    s(&answer["description"])
                );
                for e in answer["entries"].as_array().into_iter().flatten() {
                    let _ = writeln!(
                        out,
                        "  {} {} ({}): {}",
                        s(&e["id"]),
                        s(&e["name"]),
                        e["license"].as_str().unwrap_or("no licence"),
                        s(&e["url"])
                    );
                }
            } else {
                let m = &answer["manifest"];
                let _ = writeln!(
                    out,
                    "{} ({}) at {} {}, licence {}",
                    s(&m["name"]),
                    s(&m["id"]),
                    s(&answer["rev"]).chars().take(7).collect::<String>(),
                    answer["labels"]
                        .as_array()
                        .map(|a| a.iter().map(&s).collect::<Vec<_>>().join(", "))
                        .unwrap_or_default(),
                    m["license"].as_str().unwrap_or("none")
                );
                let _ = writeln!(out, "Versions: {}", versions(&answer["versions"]));
                for c in answer["components"].as_array().into_iter().flatten() {
                    let rows = c["configurations"]["rows"].as_array().map_or(0, Vec::len);
                    let _ = writeln!(
                        out,
                        "  {} {}{}: {}{}",
                        s(&c["id"]),
                        s(&c["name"]),
                        if s(&c["standard"]).is_empty() {
                            String::new()
                        } else {
                            format!(" ({})", s(&c["standard"]))
                        },
                        s(&c["path"]),
                        if rows > 0 {
                            format!(
                                ", {rows} sizes (default {})",
                                s(&c["configurations"]["default"])
                            )
                        } else {
                            String::new()
                        }
                    );
                }
            }
        }
        "library_search" => {
            for c in answer["components"].as_array().into_iter().flatten() {
                let _ = writeln!(
                    out,
                    "{}/{}  {}{}  {}  {}",
                    s(&c["library"]),
                    s(&c["id"]),
                    s(&c["name"]),
                    if s(&c["standard"]).is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", s(&c["standard"]))
                    },
                    c["license"].as_str().unwrap_or("no licence"),
                    s(&c["version"])
                );
            }
            for l in answer["libraries"].as_array().into_iter().flatten() {
                let _ = writeln!(
                    out,
                    "library {}  {}  {}  {}{}",
                    s(&l["id"]),
                    s(&l["name"]),
                    l["license"].as_str().unwrap_or("no licence"),
                    s(&l["url"]),
                    if l["fetched"] == true {
                        " (fetched)"
                    } else {
                        ""
                    }
                );
            }
            if answer["hidden"].as_u64().unwrap_or(0) > 0 {
                let _ = writeln!(out, "{} more without an accepted licence", answer["hidden"]);
            }
        }
        "library_diff" => {
            let label = |key: &str, labels: &str| {
                let short: String = s(&answer[key]).chars().take(7).collect();
                match answer[labels].get(0).and_then(Value::as_str) {
                    Some(l) => format!("{l} ({short})"),
                    None => short,
                }
            };
            let _ = writeln!(
                out,
                "Changes in {} from {} to {}:",
                s(&answer["id"]),
                label("from", "from_labels"),
                label("to", "to_labels")
            );
            for line in answer["library"].as_array().into_iter().flatten() {
                let _ = writeln!(out, "  {}", s(line));
            }
            for p in answer["parts"].as_array().into_iter().flatten() {
                let config = p["config"]
                    .as_str()
                    .map(|c| format!(" {c}"))
                    .unwrap_or_default();
                let _ = writeln!(out, "  {}{config}: {}", s(&p["component"]), s(&p["text"]));
            }
        }
        "library_check" => {
            for e in answer["errors"].as_array().into_iter().flatten() {
                let _ = writeln!(out, "Error: {}", s(e));
            }
            for w in answer["warnings"].as_array().into_iter().flatten() {
                let _ = writeln!(out, "Warning: {}", s(w));
            }
            if answer["ok"] != true {
                return Err(VcsError::Command(out.trim_end().to_owned()));
            }
            let _ = writeln!(
                out,
                "{}: {} components, no errors",
                s(&answer["id"]),
                answer["components"]
            );
        }
        "index_entry" => out = s(&answer["text"]),
        _ => {
            out = serde_json::to_string_pretty(&answer).expect("JSON");
            out.push('\n');
        }
    }
    Ok(out)
}
