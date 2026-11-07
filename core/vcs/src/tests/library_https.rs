// SPDX-License-Identifier: MIT
//! Libraries without an installed git (mitcad#1224): a library made and
//! recorded with gix only, served by a small git server of the smart HTTP
//! protocol (version 2) on `http://127.0.0.1` in the test, fetched and then
//! updated by MitCad's built-in transfer while the git program given is one
//! that does not exist, so nothing here needs git. An address that cannot
//! be reached and one with no repository give plain errors; an SSH address
//! says it needs the installed git; the preference Use the installed git
//! program (`use_git`) still takes the git program, and with the system's
//! git (skipped without it) fetches from the same server.
//!
//! The server is plain HTTP on the loopback address: the built-in client's
//! TLS (rustls with the system's root certificates) trusts no certificate a
//! test could make, and HTTPS differs from HTTP only below the protocol
//! this tests. Its packs hold every object reachable from the branches and
//! tags, uncompressed and without deltas, whatever the client already has.

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use gix::ObjectId;
use gix::refs::transaction::PreviousValue;
use serde_json::{Value, json};

use super::remote::{error_class, slashes, system_git};
use super::*;
use crate::library::{LibraryCache, api};
use crate::remote::GitCli;

/// A PNG file's signature and a little more: enough to be taken for one.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n preview";

fn library_command(cache: &LibraryCache, git: Option<&GitCli>, command: Value) -> Value {
    serde_json::from_str(&api::command(&command.to_string(), cache, git, None).unwrap()).unwrap()
}

/// A git program that is not there: any use of git fails.
fn no_git(scratch: &Scratch) -> GitCli {
    GitCli::at(scratch.join("no-git-here").join("git"))
}

/// A square pin `l` long, as a design's text.
fn pin(l: &str) -> String {
    let mut doc = Document::new(MockKernel::default());
    for command in [
        json!({"cmd": "add_parameter", "name": "d", "expression": "4 mm"}),
        json!({"cmd": "add_parameter", "name": "L", "expression": l}),
        json!({"cmd": "sketch.create"}),
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": "d", "height": "d"}),
        json!({"cmd": "add_feature", "def": {"type": "extrude",
            "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
            "extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}}),
    ] {
        doc.command(&command.to_string()).unwrap();
    }
    doc.to_json()
}

/// Adds the pin `l` long to the library in `dir` and records the files
/// `written` and the component's as a version with gix (no git program).
fn record_pin(cache: &LibraryCache, dir: &Path, l: &str, mut written: Vec<String>, message: &str) {
    let added = library_command(
        cache,
        None,
        json!({"cmd": "library_add", "dir": dir, "text": pin(l), "id": "pin",
               "name": "Square pin", "category": "pins",
               "preview": mitcad_model::library::encode_base64(PNG)}),
    );
    assert_eq!(added["errors"], json!([]), "{added}");
    for file in added["written"].as_array().unwrap() {
        written.push(file.as_str().unwrap().to_owned());
    }
    let files: Vec<PathBuf> = written.iter().map(|file| dir.join(file)).collect();
    let repo = ProjectRepo::open(dir).unwrap();
    repo.commit(&files, message, &author()).unwrap();
}

/// Tags HEAD of the repository in `dir` (a lightweight tag), with gix.
fn tag_head(dir: &Path, tag: &str) {
    let repo = gix::open(dir).unwrap();
    let head = repo.head_id().unwrap().detach();
    repo.tag_reference(tag, head, PreviousValue::MustNotExist)
        .unwrap();
}

/// A library in `scratch/pins` made with the publishing commands and
/// recorded with gix only: `v1.0.0` with a 16 mm pin, `v1.1.0` with 18 mm.
fn make_library(scratch: &Scratch, cache: &LibraryCache) -> PathBuf {
    let dir = scratch.join("pins");
    let made = library_command(
        cache,
        None,
        json!({"cmd": "library_init", "dir": dir, "id": "http-pins", "name": "Pins over HTTP",
               "description": "Square pins for tests.", "license": "CC0-1.0"}),
    );
    assert_eq!(made["id"], "http-pins", "{made}");
    let written: Vec<String> = made["written"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file.as_str().unwrap().to_owned())
        .collect();
    record_pin(cache, &dir, "16 mm", written, "Pins 1.0");
    tag_head(&dir, "v1.0.0");
    record_pin(cache, &dir, "18 mm", Vec::new(), "Pins 1.1");
    tag_head(&dir, "v1.1.0");
    dir
}

/// Serves the repository in `repo` at `http://127.0.0.1:<port>/<name>`
/// until the test ends; the URL.
fn serve(repo: &Path, name: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let repo = repo.to_path_buf();
    let prefix = format!("/{name}");
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else {
                continue;
            };
            let repo = repo.clone();
            let prefix = prefix.clone();
            std::thread::spawn(move || {
                let _ = answer(stream, &repo, &prefix);
            });
        }
    });
    format!("http://127.0.0.1:{port}/{name}")
}

/// Reads one request and answers it; the connection is closed after it.
fn answer(mut stream: TcpStream, repo: &Path, prefix: &str) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or_default().to_owned();
    let mut length = 0;
    let mut chunked = false;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            let value = value.trim();
            if name.eq_ignore_ascii_case("content-length") {
                length = value.parse().unwrap_or(0);
            } else if name.eq_ignore_ascii_case("transfer-encoding") {
                chunked = value.eq_ignore_ascii_case("chunked");
            }
        }
    }
    let body = if chunked {
        read_chunked(&mut reader)?
    } else {
        let mut body = vec![0; length];
        reader.read_exact(&mut body)?;
        body
    };
    let (status, content_type, payload) = respond(repo, prefix, &method, &target, &body);
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
         Cache-Control: no-cache\r\nConnection: close\r\n\r\n",
        payload.len()
    )?;
    stream.write_all(&payload)?;
    stream.flush()
}

/// A body sent in chunks (`Transfer-Encoding: chunked`).
fn read_chunked(reader: &mut impl BufRead) -> std::io::Result<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let size = line.trim().split(';').next().unwrap_or_default();
        let size = usize::from_str_radix(size, 16).unwrap_or(0);
        if size == 0 {
            // The trailer, up to the empty line.
            loop {
                line.clear();
                if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
                    return Ok(body);
                }
            }
        }
        let start = body.len();
        body.resize(start + size, 0);
        reader.read_exact(&mut body[start..])?;
        line.clear();
        reader.read_line(&mut line)?;
    }
}

/// The status, content type and body of the answer to a request.
fn respond(
    repo: &Path,
    prefix: &str,
    method: &str,
    target: &str,
    body: &[u8],
) -> (&'static str, &'static str, Vec<u8>) {
    let not_found = ("404 Not Found", "text/plain", b"not found\n".to_vec());
    let Some(rest) = target.strip_prefix(prefix) else {
        return not_found;
    };
    match (method, rest) {
        ("GET", "/info/refs?service=git-upload-pack") => (
            "200 OK",
            "application/x-git-upload-pack-advertisement",
            advertisement(),
        ),
        ("POST", "/git-upload-pack") => (
            "200 OK",
            "application/x-git-upload-pack-result",
            upload_pack(repo, body),
        ),
        _ => not_found,
    }
}

/// Appends a packet line with `data`.
fn packet(out: &mut Vec<u8>, data: &[u8]) {
    out.extend_from_slice(format!("{:04x}", data.len() + 4).as_bytes());
    out.extend_from_slice(data);
}

const FLUSH: &[u8] = b"0000";
const DELIMITER: &[u8] = b"0001";

/// The capabilities of protocol version 2.
fn advertisement() -> Vec<u8> {
    let mut out = Vec::new();
    for line in [
        "version 2\n",
        "agent=mitcad-test-server\n",
        "ls-refs\n",
        "fetch\n",
        "object-format=sha1\n",
    ] {
        packet(&mut out, line.as_bytes());
    }
    out.extend_from_slice(FLUSH);
    out
}

/// The data lines of a request, as text without the line end.
fn request_lines(body: &[u8]) -> Vec<String> {
    let mut lines = Vec::new();
    let mut at = 0;
    while at + 4 <= body.len() {
        let length = std::str::from_utf8(&body[at..at + 4])
            .ok()
            .and_then(|hex| usize::from_str_radix(hex, 16).ok())
            .unwrap_or(0);
        if length < 4 {
            // A flush, delimiter or end of response.
            at += 4;
            continue;
        }
        let end = (at + length).min(body.len());
        lines.push(
            String::from_utf8_lossy(&body[at + 4..end])
                .trim_end()
                .to_owned(),
        );
        at = end;
    }
    lines
}

/// `git-upload-pack` of protocol version 2: `ls-refs` and `fetch`.
fn upload_pack(repo: &Path, body: &[u8]) -> Vec<u8> {
    let lines = request_lines(body);
    let repo = gix::open(repo).unwrap();
    if lines.iter().any(|line| line == "command=ls-refs") {
        list_references(&repo)
    } else if lines.iter().any(|line| line == "command=fetch") {
        send_pack(&repo, &lines)
    } else {
        FLUSH.to_vec()
    }
}

/// The branches and tags with the objects they point to.
fn references(repo: &gix::Repository) -> Vec<(String, ObjectId)> {
    let mut out = Vec::new();
    for reference in repo.references().unwrap().all().unwrap() {
        let reference = reference.unwrap();
        let name = reference.name().as_bstr().to_string();
        if (name.starts_with("refs/heads/") || name.starts_with("refs/tags/"))
            && let Some(id) = reference.try_id()
        {
            out.push((name, id.detach()));
        }
    }
    out
}

/// `ls-refs`: HEAD with the branch it names, then the branches and tags.
fn list_references(repo: &gix::Repository) -> Vec<u8> {
    let mut out = Vec::new();
    let head = repo.head().unwrap();
    if let (Some(id), Some(name)) = (head.id(), head.referent_name()) {
        let line = format!("{} HEAD symref-target:{}\n", id.detach(), name.as_bstr());
        packet(&mut out, line.as_bytes());
    }
    for (name, id) in references(repo) {
        packet(&mut out, format!("{id} {name}\n").as_bytes());
    }
    out.extend_from_slice(FLUSH);
    out
}

/// `fetch`: the acknowledgments when the client is still negotiating
/// (the objects it has that are here, and ready), then the pack in side
/// band 1.
fn send_pack(repo: &gix::Repository, lines: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    if !lines.iter().any(|line| line == "done") {
        packet(&mut out, b"acknowledgments\n");
        for have in lines.iter().filter_map(|line| line.strip_prefix("have ")) {
            if let Ok(id) = ObjectId::from_hex(have.as_bytes())
                && repo.has_object(id)
            {
                packet(&mut out, format!("ACK {id}\n").as_bytes());
            }
        }
        packet(&mut out, b"ready\n");
        out.extend_from_slice(DELIMITER);
    }
    packet(&mut out, b"packfile\n");
    let tips: Vec<ObjectId> = references(repo).into_iter().map(|(_, id)| id).collect();
    for chunk in pack(repo, tips).chunks(65515) {
        let mut data = Vec::with_capacity(chunk.len() + 1);
        data.push(1);
        data.extend_from_slice(chunk);
        packet(&mut out, &data);
    }
    out.extend_from_slice(FLUSH);
    out
}

/// Every object reachable from `tips`: its kind and data.
fn reachable(repo: &gix::Repository, tips: Vec<ObjectId>) -> Vec<(gix::object::Kind, Vec<u8>)> {
    use gix::object::Kind;
    let mut seen = HashSet::new();
    let mut pending = tips;
    let mut out = Vec::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let object = repo.find_object(id).unwrap();
        let kind = object.kind;
        let data = object.data.clone();
        drop(object);
        match kind {
            Kind::Commit => {
                let commit = repo.find_commit(id).unwrap();
                pending.push(commit.tree_id().unwrap().detach());
                pending.extend(commit.parent_ids().map(|parent| parent.detach()));
            }
            Kind::Tree => {
                if !id.is_empty_tree() {
                    let tree = repo.find_tree(id).unwrap();
                    for entry in tree.decode().unwrap().entries {
                        if !entry.mode.is_commit() {
                            pending.push(entry.oid.to_owned());
                        }
                    }
                }
            }
            Kind::Tag => {
                let tag = repo.find_tag(id).unwrap();
                pending.push(tag.target_id().unwrap().detach());
            }
            Kind::Blob => {}
        }
        out.push((kind, data));
    }
    out
}

/// A pack of the objects reachable from `tips`, each stored whole.
fn pack(repo: &gix::Repository, tips: Vec<ObjectId>) -> Vec<u8> {
    use gix::object::Kind;
    let objects = reachable(repo, tips);
    let mut pack = b"PACK".to_vec();
    pack.extend_from_slice(&2_u32.to_be_bytes());
    pack.extend_from_slice(&u32::try_from(objects.len()).unwrap().to_be_bytes());
    for (kind, data) in &objects {
        let code: u8 = match kind {
            Kind::Commit => 1,
            Kind::Tree => 2,
            Kind::Blob => 3,
            Kind::Tag => 4,
        };
        let mut size = data.len();
        let mut byte = (code << 4) | (size & 0x0f) as u8;
        size >>= 4;
        while size > 0 {
            pack.push(byte | 0x80);
            byte = (size & 0x7f) as u8;
            size >>= 7;
        }
        pack.push(byte);
        pack.extend_from_slice(&stored_zlib(data));
    }
    let mut hasher = gix::hash::hasher(gix::hash::Kind::Sha1);
    hasher.update(&pack);
    let digest = hasher.try_finalize().unwrap();
    pack.extend_from_slice(digest.as_slice());
    pack
}

/// `data` as a zlib stream of stored (uncompressed) blocks.
fn stored_zlib(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    if data.is_empty() {
        out.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
    }
    let mut blocks = data.chunks(65535).peekable();
    while let Some(block) = blocks.next() {
        out.push(u8::from(blocks.peek().is_none()));
        let length = u16::try_from(block.len()).unwrap();
        out.extend_from_slice(&length.to_le_bytes());
        out.extend_from_slice(&(!length).to_le_bytes());
        out.extend_from_slice(block);
    }
    let (mut a, mut b) = (1_u32, 0_u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    out.extend_from_slice(&((b << 16) | a).to_be_bytes());
    out
}

/// The labels of a fetch answer's versions, newest first.
fn labels(answer: &Value) -> Vec<String> {
    answer["versions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["label"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_library_is_fetched_and_updated_over_http_without_git() {
    let scratch = Scratch::new("library-http");
    let cache = LibraryCache::new(scratch.join("cache"));
    let source = make_library(&scratch, &cache);
    let url = serve(&source, "pins.git");
    let missing = no_git(&scratch);

    let fetched = library_command(
        &cache,
        Some(&missing),
        json!({"cmd": "library_fetch", "url": url}),
    );
    assert_eq!(fetched["error"], Value::Null, "{fetched}");
    assert_eq!(
        (fetched["kind"].as_str(), fetched["id"].as_str()),
        (Some("library"), Some("http-pins")),
        "{fetched}"
    );
    assert_eq!(fetched["cloned"], true);
    assert_eq!(fetched["transport"], "built_in", "{fetched}");
    assert_eq!(labels(&fetched), ["v1.1.0", "v1.0.0"], "{fetched}");
    let log = fetched["log"].to_string();
    assert!(log.contains("built-in fetch"), "{fetched}");
    assert!(!log.contains("git clone"), "{fetched}");
    let dir = PathBuf::from(fetched["dir"].as_str().unwrap());
    assert!(dir.starts_with(&cache.root) && dir.extension().unwrap() == "git");
    // No temporary clone is left beside it.
    let leftovers: Vec<String> = fs::read_dir(&cache.root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");

    // The clone is read as one the git program made: its address, HEAD on
    // the default branch, the components at their versions.
    let listed = library_command(&cache, None, json!({"cmd": "library_list"}));
    assert_eq!(listed["libraries"][0]["url"], url.as_str(), "{listed}");
    assert_eq!(listed["libraries"][0]["id"], "http-pins", "{listed}");
    let shown = library_command(
        &cache,
        None,
        json!({"cmd": "library_show", "url": url, "rev": "v1.0.0"}),
    );
    assert_eq!(shown["components"][0]["id"], "pin", "{shown}");
    assert_eq!(shown["labels"], json!(["v1.0.0"]), "{shown}");
    let preview = library_command(
        &cache,
        None,
        json!({"cmd": "library_preview", "id": "http-pins", "component": "pin", "rev": "v1.1.0"}),
    );
    assert_eq!(
        mitcad_model::library::decode_base64(preview["png"].as_str().unwrap()).unwrap(),
        PNG
    );

    // A new version upstream: the update brings it as the unreleased tip,
    // the tagged versions stay.
    record_pin(
        &cache,
        &source,
        "20 mm",
        Vec::new(),
        "Pins 1.2 (unreleased)",
    );
    let again = library_command(
        &cache,
        Some(&missing),
        json!({"cmd": "library_fetch", "url": url}),
    );
    assert_eq!(again["error"], Value::Null, "{again}");
    assert_eq!(again["cloned"], false, "{again}");
    assert_eq!(again["transport"], "built_in", "{again}");
    let versions = again["versions"].as_array().unwrap();
    assert_eq!(versions.len(), 3, "{again}");
    assert_eq!(
        (versions[2]["label"].as_str(), versions[2]["head"].as_bool()),
        (Some(""), Some(true)),
        "{again}"
    );
    assert_eq!(again["dir"], fetched["dir"]);
    // Nothing of the update is left under refs/incoming/.
    let clone = gix::open(&dir).unwrap();
    for reference in clone.references().unwrap().all().unwrap() {
        let name = reference.unwrap().name().as_bstr().to_string();
        assert!(!name.starts_with("refs/incoming/"), "{name}");
    }

    // A third fetch with nothing new keeps everything.
    let unchanged = library_command(
        &cache,
        Some(&missing),
        json!({"cmd": "library_fetch", "url": url}),
    );
    assert_eq!(unchanged["error"], Value::Null, "{unchanged}");
    assert_eq!(unchanged["versions"].as_array().unwrap().len(), 3);
}

#[test]
fn unreachable_and_missing_libraries_give_plain_errors() {
    let scratch = Scratch::new("library-http-errors");
    let cache = LibraryCache::new(scratch.join("cache"));
    let missing = no_git(&scratch);

    // A port nothing listens on any more.
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let unreachable = format!("http://127.0.0.1:{port}/gone.git");
    let answer = library_command(
        &cache,
        Some(&missing),
        json!({"cmd": "library_fetch", "url": unreachable}),
    );
    assert_eq!(error_class(&answer), "network", "{answer}");
    let message = answer["error"]["message"].as_str().unwrap();
    assert!(
        message.starts_with("Fetching http://127.0.0.1:"),
        "{answer}"
    );
    assert!(message.contains("cannot be reached"), "{answer}");
    assert!(cache.repos().is_empty());

    // A server without that repository.
    let source = scratch.join("empty");
    fs::create_dir_all(&source).unwrap();
    gix::init(&source).unwrap();
    let base = serve(&source, "there.git");
    let elsewhere = base.replace("/there.git", "/missing.git");
    let answer = library_command(
        &cache,
        Some(&missing),
        json!({"cmd": "library_fetch", "url": elsewhere}),
    );
    assert_eq!(error_class(&answer), "not_found", "{answer}");
    assert!(
        answer["error"]["message"]
            .as_str()
            .unwrap()
            .contains("no repository at that address"),
        "{answer}"
    );
    assert!(cache.repos().is_empty());

    // An SSH address needs the installed git; nothing is run.
    let answer = library_command(
        &cache,
        Some(&missing),
        json!({"cmd": "library_fetch", "url": "ssh://git@example.invalid/pins.git"}),
    );
    assert_eq!(error_class(&answer), "unsupported", "{answer}");
    let message = answer["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("need the installed git program"),
        "{answer}"
    );
    assert!(
        message.contains("Use the installed git program"),
        "{answer}"
    );
    assert_eq!(answer["log"], json!([]), "{answer}");
    let answer = library_command(
        &cache,
        Some(&missing),
        json!({"cmd": "library_fetch", "url": "git@example.invalid:mitcad/pins.git"}),
    );
    assert_eq!(error_class(&answer), "unsupported", "{answer}");
}

#[test]
fn the_installed_git_program_is_used_when_chosen() {
    let scratch = Scratch::new("library-http-git");
    let cache = LibraryCache::new(scratch.join("cache"));
    let source = make_library(&scratch, &cache);
    let url = serve(&source, "pins.git");

    // With use_git the git program fetches even an http:// address: one
    // that is not there fails as git missing, after a try to run it.
    let answer = library_command(
        &cache,
        Some(&no_git(&scratch)),
        json!({"cmd": "library_fetch", "url": url, "use_git": true}),
    );
    assert_eq!(error_class(&answer), "git_missing", "{answer}");
    assert!(
        answer["log"].to_string().contains("clone --bare"),
        "{answer}"
    );
    assert!(cache.repos().is_empty());

    // The system's git, where it is installed, fetches from the same
    // server; the built-in transfer then updates that clone.
    let Some(cli) = system_git() else {
        return;
    };
    let fetched = library_command(
        &cache,
        Some(&cli),
        json!({"cmd": "library_fetch", "url": url, "use_git": true}),
    );
    assert_eq!(fetched["error"], Value::Null, "{fetched}");
    assert_eq!(fetched["transport"], "git", "{fetched}");
    assert!(
        fetched["log"][0].as_str().unwrap().contains("clone --bare"),
        "{fetched}"
    );
    assert_eq!(labels(&fetched), ["v1.1.0", "v1.0.0"], "{fetched}");
    record_pin(
        &cache,
        &source,
        "20 mm",
        Vec::new(),
        "Pins 1.2 (unreleased)",
    );
    let updated = library_command(
        &cache,
        Some(&no_git(&scratch)),
        json!({"cmd": "library_fetch", "url": url}),
    );
    assert_eq!(updated["error"], Value::Null, "{updated}");
    assert_eq!(
        (updated["cloned"].clone(), updated["transport"].clone()),
        (json!(false), json!("built_in")),
        "{updated}"
    );
    assert_eq!(updated["versions"].as_array().unwrap().len(), 3);

    // A library in a folder still goes through git, the setting on or off.
    let by_path = library_command(
        &cache,
        Some(&cli),
        json!({"cmd": "library_fetch", "url": slashes(&source)}),
    );
    assert_eq!(by_path["error"], Value::Null, "{by_path}");
    assert_eq!(by_path["transport"], "git", "{by_path}");
}

/// An update without the git program brings new tags, and the newest by its
/// numbers is the version read (mitcad#1423, GitHub mitcad/Mitcad#172):
/// v1.9.0 and v1.10.0 recorded after the first fetch come with the update,
/// v1.10.0 first, and the library is shown at v1.10.0.
#[test]
fn an_update_without_git_brings_new_tags_newest_first() {
    let scratch = Scratch::new("library-http-tags");
    let cache = LibraryCache::new(scratch.join("cache"));
    let source = make_library(&scratch, &cache);
    let url = serve(&source, "pins.git");
    let missing = no_git(&scratch);
    let fetched = library_command(
        &cache,
        Some(&missing),
        json!({"cmd": "library_fetch", "url": url}),
    );
    assert_eq!(fetched["error"], Value::Null, "{fetched}");
    assert_eq!(labels(&fetched), ["v1.1.0", "v1.0.0"], "{fetched}");

    record_pin(&cache, &source, "20 mm", Vec::new(), "Pins 1.9");
    tag_head(&source, "v1.9.0");
    record_pin(&cache, &source, "22 mm", Vec::new(), "Pins 1.10");
    tag_head(&source, "v1.10.0");
    let updated = library_command(
        &cache,
        Some(&missing),
        json!({"cmd": "library_fetch", "url": url}),
    );
    assert_eq!(updated["error"], Value::Null, "{updated}");
    assert_eq!(updated["cloned"], false, "{updated}");
    assert_eq!(updated["transport"], "built_in", "{updated}");
    assert_eq!(
        labels(&updated),
        ["v1.10.0", "v1.9.0", "v1.1.0", "v1.0.0"],
        "{updated}"
    );
    let shown = library_command(&cache, None, json!({"cmd": "library_show", "url": url}));
    assert_eq!(shown["labels"], json!(["v1.10.0"]), "{shown}");
    let found = library_command(
        &cache,
        None,
        json!({"cmd": "library_search", "text": "pin"}),
    );
    let version = found["components"][0]["version"].as_str().unwrap_or("");
    assert!(version.starts_with("v1.10.0 ("), "{found}");
}
