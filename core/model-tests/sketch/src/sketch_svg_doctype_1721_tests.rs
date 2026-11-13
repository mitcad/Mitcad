// SPDX-License-Identifier: MIT
//! An SVG drawing with a DOCTYPE (mitcad#1721): the SVG 1.1 declaration
//! and a small internal subset read as without them; external entities
//! and expansion bombs are refused quickly, in words, the parser's own
//! text only in the detail.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::notice::{FeatureError, PLACED_TAIL};
use mitcad_model::sketch::svg::{
    ENTITY_TEXT_LIMIT, SvgDrawing, SvgOptions, entity_expansion, read,
};
use mitcad_model::testing::{MockKernel, unique_temp_dir};
use mitcad_model_api::DocumentApi;

const SVG11: &str = r#"<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">"#;

/// A drawing of a square with a hole, a circle and a stroked line, after
/// `prolog` (an XML declaration, a DOCTYPE or nothing).
fn drawing(prolog: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
{prolog}
<svg xmlns="http://www.w3.org/2000/svg" version="1.1" viewBox="0 0 100 60" width="100mm" height="60mm">
  <path fill-rule="evenodd" d="M0 0H40V40H0Z M10 10H30V30H10Z"/>
  <circle cx="70" cy="20" r="10"/>
  <line x1="50" y1="50" x2="90" y2="55" stroke="black"/>
</svg>
"#
    )
}

/// What a person reads of an error: the text before its detail line.
fn user_part(error: &str) -> &str {
    error.find(PLACED_TAIL).map_or(error, |at| &error[..at])
}

fn read_ok(text: &str) -> SvgDrawing {
    read(text, &SvgOptions::default()).unwrap_or_else(|e| panic!("{e}\n{text}"))
}

fn read_err(text: &str) -> String {
    match read(text, &SvgOptions::default()) {
        Ok(drawing) => panic!("must be refused, read {drawing:?}"),
        Err(e) => e,
    }
}

#[test]
fn the_svg_11_doctype_reads_as_without_it() {
    let plain = read_ok(&drawing(""));
    assert_eq!(plain.shapes.len(), 3, "{plain:?}");
    let with_doctype = read_ok(&drawing(SVG11));
    assert_eq!(with_doctype, plain);
    // The same with SYSTEM only, and with the declaration on its own lines.
    let system = read_ok(&drawing(
        "<!DOCTYPE svg SYSTEM \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\">",
    ));
    assert_eq!(system, plain);
    let split = read_ok(&drawing(
        "<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\"\n  \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\">",
    ));
    assert_eq!(split, plain);
}

#[test]
fn a_small_internal_subset_with_entities_reads_as_without_it() {
    let plain = read_ok(&drawing(""));
    // The kind drawing programs write: the namespace and a path's data as
    // entities, used in attributes.
    let text = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd" [
  <!ENTITY ns_svg "http://www.w3.org/2000/svg">
  <!-- a comment in the subset -->
  <!ENTITY outer "M0 0H40V40H0Z">
  <!ENTITY square '&outer; M10 10H30V30H10Z'>
]>
<svg xmlns="&ns_svg;" version="1.1" viewBox="0 0 100 60" width="100mm" height="60mm">
  <path fill-rule="evenodd" d="&square;"/>
  <circle cx="70" cy="20" r="10"/>
  <line x1="50" y1="50" x2="90" y2="55" stroke="black"/>
</svg>
"#,
    );
    assert_eq!(read_ok(&text), plain);
    // Each use counts its value with the entities in it: the namespace
    // (26 bytes), the square (its 24 and the outline's 13), and the
    // outline's use inside the square's declaration once more (13): uses
    // are counted anywhere in the text, never fewer than the parser makes.
    assert_eq!(entity_expansion(&text), 26 + (24 + 13) + 13);
    assert_eq!(entity_expansion(&drawing(SVG11)), 0);
}

#[test]
fn a_file_with_a_doctype_comes_in_through_the_api() {
    let dir = unique_temp_dir("svg-doctype-1721");
    let plain_path = dir.join("plain.svg");
    let doctype_path = dir.join("doctype.svg");
    std::fs::write(&plain_path, drawing("")).unwrap();
    std::fs::write(&doctype_path, drawing(SVG11)).unwrap();
    let mut d = Document::new(MockKernel::default());
    let info = |d: &Document<MockKernel>, path: &std::path::Path| -> Value {
        let text = d
            .query(&json!({"query": "svg_info", "path": path.to_str().unwrap()}).to_string())
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        serde_json::from_str(&text).unwrap()
    };
    let plain = info(&d, &plain_path);
    assert_eq!(info(&d, &doctype_path), plain);
    // The square and its hole, the circle, the line.
    assert_eq!(plain["paths"], json!(4), "{plain}");
    d.command(&json!({"cmd": "sketch.create"}).to_string())
        .unwrap();
    let made: Value = serde_json::from_str(
        &d.command(
            &json!({"cmd": "sketch.import_svg", "sketch": "F1", "path": doctype_path.to_str().unwrap()})
                .to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(made["warnings"], json!([]), "{made}");
}

#[test]
fn a_file_that_is_not_xml_says_so_in_words() {
    let error = read_err("<svg xmlns=\"http://www.w3.org/2000/svg\"><path d=\"M0 0\"></svg>");
    assert_eq!(
        user_part(&error),
        "it is not an SVG drawing: the file is not valid XML"
    );
    // The parser's own text (with its line and column) is the detail.
    let detail = FeatureError::from_text(&error).detail;
    assert!(
        detail.starts_with("it is not an SVG drawing: ") && detail.len() > user_part(&error).len(),
        "{error}"
    );
    // Through the API the file's path stays before the message.
    let dir = unique_temp_dir("svg-not-xml-1721");
    let path = dir.join("broken.svg");
    std::fs::write(&path, "this is not XML").unwrap();
    let d = Document::new(MockKernel::default());
    let error = d
        .query(&json!({"query": "svg_info", "path": path.to_str().unwrap()}).to_string())
        .unwrap_err()
        .to_string();
    assert!(
        user_part(&error).ends_with("it is not an SVG drawing: the file is not valid XML"),
        "{error}"
    );
}

#[test]
fn an_external_entity_is_not_read() {
    // The parser never opens what a SYSTEM entity names; a use of one is
    // an unknown entity, so the file is refused as not valid XML.
    let dir = unique_temp_dir("svg-external-entity-1721");
    let secret = dir.join("secret.txt");
    std::fs::write(&secret, "SECRET-CONTENT").unwrap();
    let text = format!(
        r#"<?xml version="1.0"?>
<!DOCTYPE svg [ <!ENTITY leak SYSTEM "file://{}"> ]>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><text x="1" y="5">&leak;</text></svg>"#,
        secret.display()
    );
    let error = read_err(&text);
    assert_eq!(
        user_part(&error),
        "it is not an SVG drawing: the file is not valid XML"
    );
    assert!(!error.contains("SECRET-CONTENT"), "{error}");
    // Declared but not used, it is no fault.
    let unused = text.replace("&leak;", "plain");
    assert_eq!(read_ok(&unused).texts.len(), 1);
}

#[test]
fn an_entity_expansion_bomb_is_refused_without_hanging() {
    const EXPANDS: &str =
        "it is not an SVG drawing: its entity declarations expand to too much text";
    let start = std::time::Instant::now();
    // Ten levels of ten: 10^9 copies of "lol".
    let mut subset = String::from("<!ENTITY lol0 \"lol\">\n");
    for level in 1..=9 {
        let value = format!("&lol{};", level - 1).repeat(10);
        subset.push_str(&format!("<!ENTITY lol{level} \"{value}\">\n"));
    }
    let laughs = format!(
        "<?xml version=\"1.0\"?>\n<!DOCTYPE svg [\n{subset}]>\n\
         <svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"><text x=\"1\" y=\"5\">&lol9;</text></svg>"
    );
    assert!(entity_expansion(&laughs) > ENTITY_TEXT_LIMIT);
    let error = read_err(&laughs);
    assert_eq!(user_part(&error), EXPANDS);
    assert!(
        FeatureError::from_text(&error).detail.contains("bytes"),
        "{error}"
    );
    // One large entity used many times at the top level, which the
    // parser's own nesting limits do not stop: 64 KiB used 1024 times.
    let big = "x".repeat(64 << 10);
    let uses = "&big;".repeat(1024);
    let quadratic = format!(
        "<?xml version=\"1.0\"?>\n<!DOCTYPE svg [ <!ENTITY big \"{big}\"> ]>\n\
         <svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"><text x=\"1\" y=\"5\">{uses}</text></svg>"
    );
    assert_eq!(entity_expansion(&quadratic), (64 << 10) * 1024);
    assert_eq!(user_part(&read_err(&quadratic)), EXPANDS);
    // Entities that refer to each other.
    let looped = "<?xml version=\"1.0\"?>\n<!DOCTYPE svg [ <!ENTITY a \"&b;\"> <!ENTITY b \"&a;\"> ]>\n\
                  <svg xmlns=\"http://www.w3.org/2000/svg\"><text>&a;</text></svg>";
    assert_eq!(entity_expansion(looped), usize::MAX);
    assert_eq!(user_part(&read_err(looped)), EXPANDS);
    // A long chain of entities, each the one before's only content,
    // declared from the top down so that the count meets the deepest
    // first: deeper than the parser follows, so refused, and the count
    // does not exhaust the stack.
    let mut chain = String::new();
    for i in (1..20_000).rev() {
        chain.push_str(&format!("<!ENTITY e{i} \"&e{};\">\n", i - 1));
    }
    chain.push_str("<!ENTITY e0 \"end\">\n");
    assert_eq!(entity_expansion(&chain), usize::MAX);
    let deep = format!(
        "<?xml version=\"1.0\"?>\n<!DOCTYPE svg [\n{chain}]>\n\
         <svg xmlns=\"http://www.w3.org/2000/svg\"><text>&e19999;</text></svg>"
    );
    assert_eq!(user_part(&read_err(&deep)), EXPANDS);
    // All of it in well under a second's work; a generous bound for a
    // loaded test machine.
    assert!(
        start.elapsed() < std::time::Duration::from_secs(20),
        "{:?}",
        start.elapsed()
    );
}
