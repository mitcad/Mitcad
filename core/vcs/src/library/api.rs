// SPDX-License-Identifier: MIT
//! The libraries' JSON commands (`core/model-api/src/commands.md`,
//! "Component libraries"). They need no project: the application and
//! `mitcad-cli` run them with the cache of fetched libraries. A fetch's
//! failure (the network, signing in, a URL that is no library) is an
//! answer with `error` (`class`, `message`, `detail`) and `log`, as the
//! remote commands give; other failures are errors.

use std::collections::{BTreeMap, BTreeSet};

use gix::ObjectId;
use serde_json::{Value, json};

use super::build::{self, Fit, Picker, System};
use super::manifest::{
    ACCEPTED_LICENSES, IndexEntry, Manifest, ManifestComponent, license_note, matches, search_words,
};
use super::{LibraryCache, LibraryRepo, LibraryVersion, RepoKind, publish};
use crate::VcsError;
use crate::api::{optional, text};
use crate::remote::commands::answer;
use crate::remote::{Control, GitCli};
use mitcad_model::Configurations;
use mitcad_model::part_options::{self, Naming};

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
        "library_check" => publish::check_command(&command, Some(cache))?,
        "library_init" => publish::init_command(&command)?,
        "library_add" => publish::add_command(&command)?,
        "index_entry" => publish::index_entry_command(&command)?,
        "library_build_candidates" => build_candidates(cache, &command)?,
        "library_configure" => configure(cache, &command)?,
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

/// A component's design at a commit, as JSON.
fn component_design(repo: &LibraryRepo, commit: ObjectId, path: &str) -> Option<Value> {
    let data = repo.read(commit, path).ok()??;
    serde_json::from_slice(&data).ok()
}

/// A design's configuration table, if it has one, in the form of
/// mitcad#798 (an old table's selectors made options, as opening the
/// design makes them). Row fields read apart (a row's `part_number`,
/// mitcad#375) are read with [`build::rows`].
fn design_table(design: &Value) -> Option<Configurations> {
    let mut table = design.get("configurations")?.clone();
    if let Some(rows) = table.get_mut("rows").and_then(Value::as_array_mut) {
        for row in rows.iter_mut().filter_map(Value::as_object_mut) {
            row.retain(|key, _| {
                matches!(key.as_str(), "name" | "select" | "values" | "designation")
            });
        }
    }
    let mut table: Configurations = serde_json::from_value(table).ok()?;
    let names: BTreeSet<String> = design
        .get("parameters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|p| Some(p.get("name")?.as_str()?.to_owned()))
        .collect();
    table.migrate_selectors(&|name| names.contains(name), None);
    Some(table)
}

/// A design's rows' own part numbers, by row.
fn row_part_numbers(design: &Value) -> BTreeMap<String, String> {
    build::rows(design)
        .into_iter()
        .filter_map(|row| Some((row.name, row.part_number?)))
        .collect()
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
        "category": component.home_category(),
        "categories": component.category_ids(),
        "groups": manifest.component_groups(component),
        "standard": component.standard,
        "description": component.description,
        "keywords": component.keywords,
        "license": license,
        "license_note": license.as_deref().and_then(license_note),
        "preview": !component.preview.is_empty(),
        "designation": component.designation,
        "supplier": component.supplier,
        "part_number": component.part_number,
        "supplier_url": component.supplier_url,
        "build": component.build,
        // Mount frames of any part (mitcad#525).
        "mounts": component.mounts,
        // The standards and suppliers of its rows (mitcad#949): what the
        // Standard and Supplier filters offer.
        "standards": component.row_standards(&[]),
        "suppliers": component.row_suppliers(&manifest.suppliers),
    });
    if tables
        && let Some(design) = component_design(repo, commit, &component.path)
        && let Some(table) = design_table(&design)
    {
        let own: BTreeMap<String, build::Row> = build::rows(&design)
            .into_iter()
            .map(|row| (row.name.clone(), row))
            .collect();
        // `selectors`, `selector_values` and the rows' `select` are the
        // options' values as text, as pickers read them before mitcad#798.
        value["configurations"] = json!({
            "options": table.options,
            "closed": table.closed,
            "selectors": table.options,
            "selector_values": table.selector_values(),
            "default": table.default_row().map(|r| r.name.clone()),
            "rows": table.rows.iter().map(|r| {
                let mut row = json!({"name": r.name, "select": table.row_option_texts(r)});
                if let Some(number) = own.get(&r.name).and_then(|o| o.part_number.as_ref()) {
                    row["part_number"] = json!(number);
                }
                if let Some(url) = own.get(&r.name).and_then(|o| o.supplier_url.as_ref()) {
                    row["supplier_url"] = json!(url);
                }
                if let Some(designation) = &r.designation {
                    row["designation"] = json!(designation);
                }
                // A picture of its own (mitcad#484): library_preview with `config`.
                if component.row_previews.contains_key(&r.name) {
                    row["preview"] = json!(true);
                }
                // Its standards and suppliers' articles (mitcad#949).
                if let Some(block) = component.rows.get(&r.name) {
                    row["standards"] = json!(block.standards);
                    row["supplier_parts"] =
                        json!(manifest.named_supplier_parts(component, &r.name));
                }
                row
            }).collect::<Vec<_>>(),
        });
        let order: Vec<String> = table.rows.iter().map(|r| r.name.clone()).collect();
        value["standards"] = json!(component.row_standards(&order));
        // Each option with its domain and allowed values in the default
        // row, for a chooser per option (mitcad#801).
        if !table.options.is_empty() {
            value["options"] = component_state(repo, commit, &component.path)
                .and_then(|state| {
                    part_options::configure_state(
                        &state,
                        &component.name,
                        None,
                        &[],
                        &BTreeSet::new(),
                    )
                    .ok()
                })
                .map_or_else(
                    || json!([]),
                    |c| part_options::options_json(&c.state, &BTreeMap::new()),
                );
        }
    }
    value
}

/// A component's design at a commit, read as a design (for its options).
fn component_state(
    repo: &LibraryRepo,
    commit: ObjectId,
    path: &str,
) -> Option<mitcad_model::DocState> {
    let data = repo.read(commit, path).ok()??;
    let text = String::from_utf8(data).ok()?;
    mitcad_model::file::load_state(&text)
        .ok()
        .map(|(state, _)| state)
}

/// The pattern a library component's designation follows, with its rows'
/// standards (mitcad#949) and the standard asked for (`chosen`; empty:
/// each row's first).
fn naming<'a>(
    component: &'a ManifestComponent,
    rows: &'a BTreeMap<String, mitcad_model::library::LibraryRow>,
    chosen: &'a str,
) -> Naming<'a> {
    Naming {
        pattern: &component.designation,
        standard: &component.standard,
        name: &component.name,
        fallback: if component.standard.trim().is_empty() {
            "{name} {config}"
        } else {
            "{standard} {config}"
        },
        rows: Some(rows),
        chosen,
    }
}

/// `library_configure` (mitcad#801): the `configure` query's answer
/// (`options` with their allowed values and `restricted_by`, `row`,
/// `designation_values`, `designation`, `moves`, `refused`, `problems`) on
/// a library component's design in the row `config` (its default row
/// when left out) with `values` tried ({option: a text option's text, a
/// number in mm or rad, or a constant expression}): what Insert from
/// Library and the BUILD pickers show before the part is inserted. The
/// values tried are not moved; one that breaks a rule is `refused`, and
/// the later options move.
fn configure(cache: &LibraryCache, command: &Value) -> Result<Value, VcsError> {
    let repo = repo_of(cache, command)?;
    let commit = commit_of(&repo, command)?;
    let manifest = repo.manifest(commit)?;
    let id = text(command, "component")?;
    let component = manifest
        .component(id)
        .ok_or_else(|| VcsError::NotFound(format!("{} has no component '{id}'", manifest.id)))?;
    let state = component_state(&repo, commit, &component.path)
        .ok_or_else(|| invalid(format!("the design of {id} cannot be read")))?;
    let mut values = Vec::new();
    if let Some(given) = command.get("values").and_then(Value::as_object) {
        for (name, value) in given {
            let value = part_options::command_value(&state.parameters, name, value)
                .map_err(|e| invalid(format!("{id}: {e}")))?;
            values.push((name.clone(), value));
        }
    }
    let pinned: BTreeSet<String> = values.iter().map(|(name, _)| name.clone()).collect();
    // The standard the Standard filter asks for (mitcad#949).
    let chosen = optional(command, "standard").unwrap_or("");
    let rows = manifest.named_rows(component);
    let mut answer = part_options::configure_json(
        &state,
        &component.name,
        optional(command, "config"),
        &values,
        &pinned,
        &BTreeMap::new(),
        Some(&naming(component, &rows, chosen)),
    )
    .map_err(|e| invalid(format!("{id}: {e}")))?;
    answer["component"] = json!(component.id);
    answer["library"] = json!(manifest.id);
    answer["rev"] = json!(commit.to_string());
    // The rows with their option values, standards and suppliers'
    // articles (mitcad#949): what the Standard and Supplier filters narrow
    // the choosers to; the combination's row's standard and articles.
    let table = &state.configurations;
    answer["rows"] = Value::Array(
        table
            .rows
            .iter()
            .map(|r| {
                let block = rows.get(&r.name);
                json!({
                    "name": r.name,
                    "select": table.row_option_texts(r),
                    "standards": block.map(|b| b.standards.clone()).unwrap_or_default(),
                    "supplier_parts": block.map(|b| b.supplier_parts.clone()).unwrap_or_default(),
                })
            })
            .collect(),
    );
    let row = answer["row"].as_str().map(str::to_owned);
    let block = row.as_deref().and_then(|name| rows.get(name));
    answer["standard"] = json!(block.and_then(|b| b.named_standard(chosen)));
    answer["supplier_parts"] = json!(block.map(|b| b.supplier_parts.clone()).unwrap_or_default());
    Ok(answer)
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

/// The libraries a command limits itself to (`libraries`: ids or URLs).
fn libraries_filter(command: &Value) -> Option<BTreeSet<String>> {
    command.get("libraries").and_then(Value::as_array).map(|a| {
        a.iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    })
}

/// What a command asks of BUILD parts: `role`, `system` (`{"type",
/// "slot", "grid"}` or `B/8/30`), a linear part's `kind` and `size`.
fn picker_fields(command: &Value) -> Result<Picker<'_>, VcsError> {
    Ok(Picker {
        role: optional(command, "role"),
        system: command
            .get("system")
            .filter(|v| !v.is_null())
            .map(System::from_value)
            .transpose()
            .map_err(invalid)?,
        kind: optional(command, "kind"),
        size: command.get("size").filter(|v| !v.is_null()),
        mount: optional(command, "mount"),
    })
}

/// The texts a component's search words match: its name, id, standard,
/// description, categories, supplier, part number, keywords and library.
fn component_texts<'a>(manifest: &'a Manifest, c: &'a ManifestComponent) -> Vec<&'a str> {
    let mut texts = vec![
        c.name.as_str(),
        c.id.as_str(),
        c.standard.as_str(),
        c.description.as_str(),
        c.supplier.as_str(),
        c.part_number.as_str(),
        manifest.name.as_str(),
        manifest.id.as_str(),
    ];
    texts.extend(c.category_ids());
    texts.extend(c.keywords.iter().map(String::as_str));
    texts
}

/// A found component with its library's fields; with `tables`, its
/// configuration table (`configurations`) as `library_show` gives it.
fn found_json(
    repo: &LibraryRepo,
    commit: ObjectId,
    manifest: &Manifest,
    c: &ManifestComponent,
    url: &str,
    version: &LibraryVersion,
    tables: bool,
) -> Value {
    let mut item = component_json(repo, commit, manifest, c, tables);
    item["library"] = json!(manifest.id);
    item["library_name"] = json!(manifest.name);
    item["url"] = json!(url);
    item["rev"] = json!(version.rev);
    item["version"] = json!(version.text());
    item["authors"] = json!(manifest.authors);
    item
}

/// Whether a component has `number` as its part number, one of its rows'
/// or one of its rows' suppliers' articles (mitcad#949; compared without
/// case).
fn has_part_number(
    repo: &LibraryRepo,
    commit: ObjectId,
    c: &ManifestComponent,
    number: &str,
) -> bool {
    let same = |other: &str| other.trim().eq_ignore_ascii_case(number.trim());
    same(&c.part_number)
        || c.rows
            .values()
            .any(|row| row.supplier_parts.iter().any(|p| same(&p.part_number)))
        || component_design(repo, commit, &c.path)
            .is_some_and(|design| row_part_numbers(&design).values().any(|n| same(n)))
}

/// Whether a component is sold by `supplier`: its own `supplier`, or a
/// supplier with articles on its rows (mitcad#949), by id or name
/// (compared without case).
fn has_supplier(manifest: &Manifest, c: &ManifestComponent, supplier: &str) -> bool {
    let same = |other: &str| other.trim().eq_ignore_ascii_case(supplier.trim());
    same(&c.supplier)
        || c.row_suppliers(&manifest.suppliers)
            .iter()
            .any(|s| same(&s.id) || same(&s.name))
}

/// A URL as a repository's key (mitcad#949): without user information, a
/// trailing slash or `.git`, `/` for a backslash, in lower case.
fn url_key(url: &str) -> String {
    let shown = mitcad_model::library::shown_url(url.trim()).replace('\\', "/");
    let mut key = shown.trim_end_matches('/').to_ascii_lowercase();
    if let Some(stripped) = key.strip_suffix(".git") {
        key = stripped.to_owned();
    }
    key
}

/// A library fetched here, at its newest version.
struct Fetched {
    repo: LibraryRepo,
    url: String,
    version: LibraryVersion,
    commit: ObjectId,
    manifest: Manifest,
}

/// Why a library is not listed (mitcad#949): an index's library, or a
/// clone of one, whose repository an enabled source has (after redirects:
/// the source's clone has its commits) or whose id a fetched library
/// replaces. Its line `<id> left out: same repository as <source>`.
fn left_out_json(id: &str, url: &str, why: &str) -> Value {
    json!({"id": id, "url": url, "reason": why, "text": format!("{id} left out: {why}")})
}

/// What leaves the fetched library `f` out (mitcad#949): another fetched
/// library that replaces its id, or, when it is no enabled source, an
/// enabled source whose clone has its newest commit.
fn fetched_left_out(
    f: &Fetched,
    all: &[Fetched],
    is_source: &dyn Fn(&str) -> bool,
) -> Option<String> {
    for other in all {
        if std::ptr::eq(other, f) || other.manifest.id == f.manifest.id {
            continue;
        }
        if other
            .manifest
            .replaces
            .iter()
            .any(|id| id == &f.manifest.id)
        {
            return Some(format!(
                "superseded by {} ({})",
                other.manifest.id, other.url
            ));
        }
    }
    if is_source(&f.url) {
        return None;
    }
    all.iter()
        .filter(|other| !std::ptr::eq(*other, f) && is_source(&other.url))
        .find(|other| url_key(&other.url) == url_key(&f.url) || other.repo.contains(f.commit))
        .map(|other| format!("same repository as {}", other.url))
}

/// What leaves an index's entry out (mitcad#949): a fetched library that
/// replaces its id, or an enabled source of another id with its
/// repository (the same URL, or a clone that has a version the index
/// reviewed).
fn entry_left_out(
    entry: &IndexEntry,
    all: &[Fetched],
    is_source: &dyn Fn(&str) -> bool,
) -> Option<String> {
    for f in all {
        if f.manifest.id != entry.id && f.manifest.replaces.iter().any(|id| id == &entry.id) {
            return Some(format!("superseded by {} ({})", f.manifest.id, f.url));
        }
    }
    all.iter()
        .filter(|f| is_source(&f.url) && f.manifest.id != entry.id)
        .find(|f| {
            url_key(&f.url) == url_key(&entry.url)
                || entry.reviewed.iter().any(|r| {
                    ObjectId::from_hex(r.rev.trim().as_bytes()).is_ok_and(|id| f.repo.contains(id))
                })
        })
        .map(|f| format!("same repository as {}", f.url))
}

/// `library_search`: the components of every fetched library (at its
/// newest version) and the libraries the fetched indexes list, by words,
/// licence and category; BUILD parts also by `role`, `system`, `supplier`
/// and `part_number`. Items without a licence only with `unlicensed`.
/// With `sources` (the enabled sources' URLs), a library an index offers,
/// or a clone of one, that is the same repository as a source's under
/// another id, or whose id a fetched library `replaces`, is left out
/// (`left_out`, mitcad#949).
fn search(cache: &LibraryCache, command: &Value) -> Result<Value, VcsError> {
    let words = search_words(optional(command, "text").unwrap_or(""));
    let filter = license_filter(command);
    let unlicensed = command
        .get("unlicensed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let category = optional(command, "category");
    let only = libraries_filter(command);
    let picker = picker_fields(command)?;
    let supplier = optional(command, "supplier");
    let part_number = optional(command, "part_number");
    let sources: Option<Vec<String>> = command
        .get("sources")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(url_key).collect());
    let is_source = |url: &str| {
        sources
            .as_ref()
            .is_none_or(|list| list.contains(&url_key(url)))
    };
    let build_only = !picker.is_empty();
    let mut fetched: Vec<Fetched> = Vec::new();
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
                fetched.push(Fetched {
                    repo,
                    url,
                    version,
                    commit,
                    manifest,
                });
            }
            RepoKind::Index => indexes.push((repo, head, url)),
            RepoKind::Unknown => {}
        }
    }
    let mut components = Vec::new();
    let mut libraries = Vec::new();
    let mut left_out = Vec::new();
    let mut hidden = 0;
    let fetched_ids: BTreeSet<String> = fetched.iter().map(|f| f.manifest.id.clone()).collect();
    for f in &fetched {
        if let Some(why) = fetched_left_out(f, &fetched, &is_source) {
            left_out.push(left_out_json(&f.manifest.id, &f.url, &why));
            continue;
        }
        let (repo, manifest, url) = (&f.repo, &f.manifest, &f.url);
        if only
            .as_ref()
            .is_some_and(|set| !set.contains(&manifest.id) && !set.contains(url))
        {
            continue;
        }
        for c in &manifest.components {
            let license = manifest.license_of(c);
            if category.is_some_and(|wanted| !c.in_category(wanted)) {
                continue;
            }
            if build_only && !matches!(build::fit_picker(c, &picker), Fit::Fits(_)) {
                continue;
            }
            if supplier.is_some_and(|s| !has_supplier(manifest, c, s)) {
                continue;
            }
            if !matches(&words, &component_texts(manifest, c)) {
                continue;
            }
            if part_number.is_some_and(|n| !has_part_number(repo, f.commit, c, n)) {
                continue;
            }
            if !license_passes(license.as_deref(), &filter, unlicensed) {
                hidden += 1;
                continue;
            }
            components.push(found_json(
                repo, f.commit, manifest, c, url, &f.version, false,
            ));
        }
    }
    for (repo, head, index_url) in indexes {
        let Ok((index, entries, _)) = repo.index(head) else {
            continue;
        };
        for entry in entries {
            if let Some(why) = entry_left_out(&entry, &fetched, &is_source) {
                left_out.push(left_out_json(&entry.id, &entry.url, &why));
                continue;
            }
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
    Ok(
        json!({"components": components, "libraries": libraries, "hidden": hidden,
              "left_out": left_out}),
    )
}

/// `library_build_candidates` (mitcad#375): the BUILD parts of the fetched
/// libraries (at their newest versions) that fit a junction of two
/// profiles (a connector's `junctions`, `systems` and `space` against the
/// junction's), or a picker's `role`, `system`, `kind` and `size`; best first, and apart
/// the parts that nearly fit with the reason, for greyed rows.
fn build_candidates(cache: &LibraryCache, command: &Value) -> Result<Value, VcsError> {
    let junction = command.get("junction").filter(|j| j.is_object());
    let picker = picker_fields(command)?;
    if junction.is_none() && picker.role.is_none() {
        return Err(invalid(
            "library_build_candidates needs a junction, or a role (and a system, kind or size)              for a picker",
        ));
    }
    let words = search_words(optional(command, "text").unwrap_or(""));
    let filter = license_filter(command);
    let unlicensed = command
        .get("unlicensed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let only = libraries_filter(command);
    let mut fits: Vec<(u32, String, Value)> = Vec::new();
    let mut refused = Vec::new();
    let mut hidden = 0;
    for dir in cache.repos() {
        let Ok(repo) = LibraryRepo::open(&dir) else {
            continue;
        };
        let Ok(Some(head)) = repo.head() else {
            continue;
        };
        if repo.kind(head) != RepoKind::Library {
            continue;
        }
        let Some(version) = repo.newest()? else {
            continue;
        };
        let commit = repo.resolve(&version.rev)?;
        let Ok(manifest) = repo.manifest(commit) else {
            continue;
        };
        let url = repo.url().unwrap_or_default();
        if only
            .as_ref()
            .is_some_and(|set| !set.contains(&manifest.id) && !set.contains(&url))
        {
            continue;
        }
        for c in &manifest.components {
            let fit = match junction {
                Some(junction) => build::fit_junction(c, junction),
                None => build::fit_picker(c, &picker),
            };
            if fit == Fit::No || !matches(&words, &component_texts(&manifest, c)) {
                continue;
            }
            if !license_passes(manifest.license_of(c).as_deref(), &filter, unlicensed) {
                hidden += 1;
                continue;
            }
            match fit {
                Fit::Fits(score) => fits.push((
                    score,
                    c.name.to_lowercase(),
                    // With its rows: every row a part of the picker (mitcad#484).
                    found_json(&repo, commit, &manifest, c, &url, &version, true),
                )),
                // A greyed row: which part, and why.
                Fit::Near(reason) => refused.push(json!({
                    "id": c.id,
                    "name": c.name,
                    "library": manifest.id,
                    "reason": reason,
                })),
                Fit::No => {}
            }
        }
    }
    fits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    Ok(json!({
        "components": fits.into_iter().map(|(_, _, item)| item).collect::<Vec<_>>(),
        "refused": refused,
        "hidden": hidden,
    }))
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

/// `library_preview`: a component's preview image (PNG, base64), or null;
/// with `config`, that row's own when it has one (mitcad#484).
fn preview(cache: &LibraryCache, command: &Value) -> Result<Value, VcsError> {
    let repo = repo_of(cache, command)?;
    let commit = commit_of(&repo, command)?;
    let manifest = repo.manifest(commit)?;
    let id = text(command, "component")?;
    let component = manifest
        .component(id)
        .ok_or_else(|| VcsError::NotFound(format!("{} has no component '{id}'", manifest.id)))?;
    let path = optional(command, "config")
        .and_then(|row| component.row_previews.get(row))
        .unwrap_or(&component.preview);
    if path.is_empty() {
        return Ok(json!({"png": null}));
    }
    let data = repo.read(commit, path)?;
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
            mitcad_model_diff::diff_files(&without_table(&old.text), &without_table(&new.text))
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
        "library_search" | "library_build_candidates" => {
            let supplier = |c: &Value| {
                let (who, number) = (s(&c["supplier"]), s(&c["part_number"]));
                match (who.is_empty(), number.is_empty()) {
                    (true, true) => String::new(),
                    (false, true) => format!("  {who}"),
                    (true, false) => format!("  {number}"),
                    (false, false) => format!("  {who} {number}"),
                }
            };
            for c in answer["components"].as_array().into_iter().flatten() {
                let _ = writeln!(
                    out,
                    "{}/{}  {}{}{}  {}  {}",
                    s(&c["library"]),
                    s(&c["id"]),
                    s(&c["name"]),
                    if s(&c["standard"]).is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", s(&c["standard"]))
                    },
                    supplier(c),
                    c["license"].as_str().unwrap_or("no licence"),
                    s(&c["version"])
                );
            }
            for c in answer["refused"].as_array().into_iter().flatten() {
                let _ = writeln!(
                    out,
                    "not {}/{}  {}: {}",
                    s(&c["library"]),
                    s(&c["id"]),
                    s(&c["name"]),
                    s(&c["reason"])
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
            // Libraries an index offers that a source has (mitcad#949).
            for l in answer["left_out"].as_array().into_iter().flatten() {
                let _ = writeln!(out, "not listed: {}", s(&l["text"]));
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
