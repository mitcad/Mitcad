// SPDX-License-Identifier: MIT
//! In-app tutorials in git repositories (mitcad#2012): a tutorials
//! repository fetched as a library is, its tutorials listed with the ones
//! whose manifest is wrong left out and said, one installed into a folder
//! with its record, installed again at a newer version, and a symbolic link
//! or a path outside it refused.

use serde_json::{Value, json};

use super::remote::{slashes, system_git};
use super::*;
use crate::library::{LibraryCache, api};
use crate::remote::GitCli;

fn command(cache: &LibraryCache, git: Option<&GitCli>, command: Value) -> Value {
    serde_json::from_str(&api::command(&command.to_string(), cache, git, None).unwrap()).unwrap()
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// A tutorials repository with two tutorials, one of them with a manifest
/// whose id is not its folder's.
fn make_tutorials(scratch: &Scratch) -> PathBuf {
    let dir = scratch.join("course");
    write(
        &dir.join("mitcad-tutorials.json"),
        r#"{"format": "mitcad-tutorials", "version": 1, "name": "Test course"}"#,
    );
    write(
        &dir.join("tutorials/two-steps/tutorial.json"),
        r#"{"id": "two-steps", "title": "Two steps", "language": "en", "version": "1.0",
            "min_mitcad": "0.1.0", "license": "CC0-1.0", "start": "index.html",
            "designs": ["designs/start.mitcad"]}"#,
    );
    write(
        &dir.join("tutorials/two-steps/index.html"),
        "<p>Step 1</p>\n",
    );
    write(&dir.join("tutorials/two-steps/images/a.svg"), "<svg/>\n");
    write(
        &dir.join("tutorials/two-steps/designs/start.mitcad"),
        "{}\n",
    );
    write(
        &dir.join("tutorials/wrong/tutorial.json"),
        r#"{"id": "other", "title": "Wrong", "language": "en", "version": "1",
            "min_mitcad": "0.1.0", "license": "CC0-1.0"}"#,
    );
    git(&dir, &["init", "-q"]).unwrap();
    git(&dir, &["add", "-A"]).unwrap();
    git(&dir, &["commit", "-q", "-m", "Course 1.0"]).unwrap();
    dir
}

#[test]
fn tutorials_are_fetched_listed_and_installed() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("tutorials");
    let cache = LibraryCache::new(scratch.join("cache"));
    let course = make_tutorials(&scratch);
    let url = slashes(&course);
    let fetched = command(
        &cache,
        Some(&cli),
        json!({"cmd": "library_fetch", "url": url}),
    );
    assert_eq!(fetched["kind"], "tutorials", "{fetched}");
    assert_eq!(fetched["name"], "Test course", "{fetched}");

    let listed = command(&cache, None, json!({"cmd": "tutorial_list", "urls": [url]}));
    let source = &listed["sources"][0];
    assert_eq!(source["name"], "Test course", "{listed}");
    let tutorials = source["tutorials"].as_array().unwrap();
    assert_eq!(tutorials.len(), 1, "{listed}");
    assert_eq!(tutorials[0]["id"], "two-steps");
    assert_eq!(tutorials[0]["min_mitcad"], "0.1.0");
    let problems = source["problems"].to_string();
    assert!(problems.contains("tutorials/wrong"), "{problems}");
    assert!(problems.contains("its folder's name"), "{problems}");
    // Another source's filter lists nothing; the library list names it.
    let none = command(
        &cache,
        None,
        json!({"cmd": "tutorial_list", "urls": ["https://example.org/other.git"]}),
    );
    assert_eq!(none["sources"], json!([]));
    let libraries = command(&cache, None, json!({"cmd": "library_list"}));
    assert_eq!(
        libraries["libraries"][0]["kind"], "tutorials",
        "{libraries}"
    );

    let installed_root = scratch.join("installed");
    let installed = command(
        &cache,
        None,
        json!({"cmd": "tutorial_install", "url": url, "id": "two-steps", "folder": installed_root}),
    );
    let folder = installed_root.join("two-steps");
    assert_eq!(installed["files"], 4, "{installed}");
    assert!(folder.join("index.html").is_file());
    assert!(folder.join("images/a.svg").is_file());
    assert!(folder.join("designs/start.mitcad").is_file());
    assert!(installed["start"].as_str().unwrap().ends_with("index.html"));
    let mark: Value = serde_json::from_str(
        &fs::read_to_string(folder.join(".mitcad-tutorial-source.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(mark["url"], url);
    assert_eq!(mark["version"], "1.0");

    // A newer version: fetched, installed again in place; a page gone from
    // it is gone from the installed folder too.
    fs::remove_file(course.join("tutorials/two-steps/images/a.svg")).unwrap();
    write(
        &course.join("tutorials/two-steps/step2.html"),
        "<p>Step 2</p>\n",
    );
    git(&course, &["add", "-A"]).unwrap();
    git(&course, &["commit", "-q", "-m", "Course 1.1"]).unwrap();
    command(
        &cache,
        Some(&cli),
        json!({"cmd": "library_fetch", "url": url}),
    );
    let again = command(
        &cache,
        None,
        json!({"cmd": "tutorial_install", "url": url, "id": "two-steps", "folder": installed_root}),
    );
    assert_ne!(again["rev"], installed["rev"], "{again}");
    assert!(folder.join("step2.html").is_file());
    assert!(!folder.join("images/a.svg").exists());

    // An id that is none, a tutorial that is not there.
    for id in ["../two-steps", "missing"] {
        let refused = api::command(
            &json!({"cmd": "tutorial_install", "url": url, "id": id, "folder": installed_root})
                .to_string(),
            &cache,
            None,
            None,
        );
        assert!(refused.is_err(), "{id}");
    }
}

/// The Start page's cards (mitcad#2346): a tutorial's duration, level,
/// picture and place in the recommended order are listed when it gives
/// them and left out when it does not; the order puts the first part first
/// whatever the titles, those without an order after; a wrong level, a
/// picture that is no PNG or JPEG and an order of 0 leave the tutorial out
/// with the reason; the picture is written for a card before the tutorial
/// is installed.
#[test]
fn tutorials_give_their_cards_details_and_order() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("tutorials-cards");
    let cache = LibraryCache::new(scratch.join("cache"));
    let dir = scratch.join("course");
    write(
        &dir.join("mitcad-tutorials.json"),
        r#"{"format": "mitcad-tutorials", "version": 1, "name": "Ordered course"}"#,
    );
    let tutorial = |id: &str, title: &str, extra: &str| {
        write(
            &dir.join(format!("tutorials/{id}/tutorial.json")),
            &format!(
                r#"{{"id": "{id}", "title": "{title}", "language": "en", "version": "1.0",
                    "min_mitcad": "0.1.0", "license": "CC0-1.0"{extra}}}"#
            ),
        );
        write(
            &dir.join(format!("tutorials/{id}/index.html")),
            "<p>1</p>\n",
        );
    };
    tutorial(
        "part-two",
        "A second part",
        r#", "order": 2, "duration": 15, "level": "intermediate""#,
    );
    tutorial(
        "part-one",
        "Zero to a block",
        r#", "order": 1, "duration": 10, "level": "beginner", "picture": "images/result.png",
            "description": "A sketch and an extrude.""#,
    );
    tutorial("extra", "An extra", "");
    tutorial("bad-level", "Bad level", r#", "level": "expert""#);
    tutorial(
        "bad-picture",
        "Bad picture",
        r#", "picture": "images/result.gif""#,
    );
    tutorial("bad-order", "Bad order", r#", "order": 0"#);
    write(
        &dir.join("tutorials/part-one/images/result.png"),
        "not really a PNG\n",
    );
    git(&dir, &["init", "-q"]).unwrap();
    git(&dir, &["add", "-A"]).unwrap();
    git(&dir, &["commit", "-q", "-m", "Ordered course"]).unwrap();
    let url = slashes(&dir);
    command(
        &cache,
        Some(&cli),
        json!({"cmd": "library_fetch", "url": url}),
    );

    let listed = command(&cache, None, json!({"cmd": "tutorial_list", "urls": [url]}));
    let source = &listed["sources"][0];
    let ids: Vec<&str> = source["tutorials"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["part-one", "part-two", "extra"], "{listed}");
    let first = &source["tutorials"][0];
    assert_eq!(first["duration"], 10, "{first}");
    assert_eq!(first["level"], "beginner", "{first}");
    assert_eq!(first["picture"], "images/result.png", "{first}");
    assert_eq!(first["order"], 1, "{first}");
    assert_eq!(first["description"], "A sketch and an extrude.", "{first}");
    // A tutorial without them has none of the fields.
    let extra = &source["tutorials"][2];
    for field in ["duration", "level", "picture", "order"] {
        assert!(extra.get(field).is_none(), "{field}: {extra}");
    }
    let problems = source["problems"].to_string();
    assert!(
        problems.contains("level \\\"expert\\\" is not one of"),
        "{problems}"
    );
    assert!(problems.contains("is not a PNG or JPEG file"), "{problems}");
    assert!(problems.contains("order starts at 1"), "{problems}");

    // The picture for the card, before any install.
    let file = scratch.join("pictures/part-one.png");
    let written = command(
        &cache,
        None,
        json!({"cmd": "tutorial_picture", "url": url, "id": "part-one", "file": file}),
    );
    assert_eq!(written["bytes"], 17, "{written}");
    assert_eq!(fs::read_to_string(&file).unwrap(), "not really a PNG\n");
    // A tutorial without a picture, and one that is not there.
    for id in ["part-two", "missing"] {
        let refused = api::command(
            &json!({"cmd": "tutorial_picture", "url": url, "id": id, "file": scratch.join("pictures/x.png")})
                .to_string(),
            &cache,
            None,
            None,
        );
        assert!(refused.is_err(), "{id}");
    }
}

#[cfg(unix)]
#[test]
fn a_tutorial_with_a_symbolic_link_is_not_installed() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("tutorials-link");
    let cache = LibraryCache::new(scratch.join("cache"));
    let course = make_tutorials(&scratch);
    std::os::unix::fs::symlink("/etc/passwd", course.join("tutorials/two-steps/secret")).unwrap();
    git(&course, &["add", "-A"]).unwrap();
    git(&course, &["commit", "-q", "-m", "A link"]).unwrap();
    let url = slashes(&course);
    command(
        &cache,
        Some(&cli),
        json!({"cmd": "library_fetch", "url": url}),
    );
    let installed_root = scratch.join("installed");
    let refused = api::command(
        &json!({"cmd": "tutorial_install", "url": url, "id": "two-steps", "folder": installed_root})
            .to_string(),
        &cache,
        None,
        None,
    );
    assert!(refused.is_err());
    assert!(!installed_root.join("two-steps").exists());
}
