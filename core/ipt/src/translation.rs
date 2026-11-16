// SPDX-License-Identifier: MIT
//! The translation report of a file converted from another format
//! (mitcad#1913). A conversion keeps its report in the document it made
//! as an embedded object: a stream `\x01Ole10Native` (an OLE 1.0 package)
//! whose data is an HTML page, in UTF-16 with a byte order mark. The page
//! names the source file and the translator, and lists the documents the
//! conversion made, each with its status and messages (*Converted files
//! and their translation reports* in `README.md`); `testassembly` writes
//! them for tests.
//!
//! The import reads it to say why a part came in without geometry: an
//! assembly converted from a building model whose parts got no bodies
//! keeps "No Bodies Read From File." for each of them.

use crate::IptFile;
use crate::cfb::EntryKind;

/// The name of the stream of an embedded OLE 1.0 package.
pub const PACKAGE_STREAM: &str = "\u{1}Ole10Native";

/// The namespace that marks the page as a translation report.
pub const REPORT_NAMESPACE: &str = "xmlns:TranslationData";

/// A message of the report about a document it made.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Message {
    /// "Warning", "Error", ...
    pub severity: String,
    pub text: String,
}

/// A document the conversion made: its file name, status and messages.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Component {
    pub name: String,
    /// "Successful", "Warning", ...
    pub status: String,
    pub messages: Vec<Message>,
}

/// A translation report.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    /// The source file as the report names it (a path of the machine that
    /// converted it).
    pub source: Option<String>,
    /// The translator's name, as the report gives it.
    pub translator: Option<String>,
    pub components: Vec<Component>,
}

impl Report {
    /// The source file's extension, lower case with its dot (`.rvt`).
    pub fn source_extension(&self) -> Option<String> {
        let source = self.source.as_deref()?;
        let name = source.rsplit(['\\', '/']).next()?;
        let dot = name.rfind('.')?;
        (dot + 1 < name.len()).then(|| name[dot..].to_lowercase())
    }

    /// The document the report lists by this file name (case-insensitive;
    /// the page's text has its runs of white space collapsed, as a name
    /// with two spaces in a row comes out of it with one).
    pub fn component(&self, file_name: &str) -> Option<&Component> {
        let name = file_name.split_whitespace().collect::<Vec<_>>().join(" ");
        self.components
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(&name))
    }
}

/// The translation report the file keeps, if any: the first embedded
/// package whose data is one.
pub fn read(file: &IptFile) -> Option<Report> {
    file.container
        .entries()
        .filter(|(path, entry)| {
            entry.kind == EntryKind::Stream && path.rsplit('/').next() == Some(PACKAGE_STREAM)
        })
        .filter_map(|(_, entry)| file.container.read(entry).ok())
        .find_map(|stream| parse_html(&decode(package_data(&stream)?)))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    let bytes = data.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

/// The end of the zero-terminated text at `at`, past its zero.
fn past_text(data: &[u8], at: usize) -> Option<usize> {
    let len = data.get(at..)?.iter().position(|&b| b == 0)?;
    Some(at + len + 1)
}

/// The data of an OLE 1.0 package stream: a u32 size, a u16 (2), the label
/// and the source path (zero-terminated), a u32 (0x0003_0000), the
/// temporary path (u32 length, then the path with its zero) and the data
/// (u32 size, then the bytes).
pub fn package_data(stream: &[u8]) -> Option<&[u8]> {
    let total = u32_at(stream, 0)? as usize;
    let stream = stream.get(..total.checked_add(4)?.min(stream.len()))?;
    let at = past_text(stream, 6)?;
    let at = past_text(stream, at)? + 4;
    let temp = u32_at(stream, at)? as usize;
    let at = at.checked_add(4 + temp)?;
    let size = u32_at(stream, at)? as usize;
    stream.get(at + 4..(at + 4).checked_add(size)?)
}

/// The page's text: UTF-16 after a byte order mark, else UTF-8.
fn decode(data: &[u8]) -> String {
    if let Some(rest) = data.strip_prefix(&[0xFF, 0xFE]) {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        let data = data.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(data);
        String::from_utf8_lossy(data).into_owned()
    }
}

/// The value of an attribute in a start tag's text.
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let at = tag.find(&format!("{name}=\""))? + name.len() + 2;
    let len = tag[at..].find('"')?;
    Some(&tag[at..at + len])
}

/// The text of HTML: the tags left out, the entities the report uses
/// replaced, the white space collapsed.
fn text_of(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        out.push(' ');
        rest = rest[lt..].find('>').map_or("", |gt| &rest[lt + gt + 1..]);
    }
    out.push_str(rest);
    let out = out
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The texts of a row's cells (`<td>`); a cell runs to its `</td>`, or to
/// the end of the row's text when a nested table starts in it.
fn cells(row: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = row;
    while let Some(at) = rest.find("<td") {
        let Some(gt) = rest[at..].find('>') else {
            break;
        };
        let body = &rest[at + gt + 1..];
        let end = body.find("</td>").unwrap_or(body.len());
        out.push(text_of(&body[..end]));
        rest = &body[end..];
    }
    out
}

/// Reads a translation report's page; None when the text is not one.
pub fn parse_html(html: &str) -> Option<Report> {
    if !html.contains(REPORT_NAMESPACE) {
        return None;
    }
    let mut report = Report::default();
    // The document whose messages the rows after its details row give.
    let mut current: Option<usize> = None;
    for row in html.split("<tr").skip(1) {
        let tag = &row[..row.find('>').unwrap_or(row.len())];
        let cells = cells(row);
        match attribute(tag, "id") {
            Some(id) if id.starts_with("component-row") => {
                report.components.push(Component {
                    name: cells.get(1).cloned().unwrap_or_default(),
                    status: if cells.len() > 3 {
                        cells.last().cloned().unwrap_or_default()
                    } else {
                        String::new()
                    },
                    messages: Vec::new(),
                });
                current = None;
            }
            Some(id) if id.starts_with("component-details") => {
                current = report.components.len().checked_sub(1);
            }
            Some(_) => current = None,
            None => match current {
                Some(i) => {
                    if cells.len() >= 3 {
                        report.components[i].messages.push(Message {
                            severity: cells[0].clone(),
                            text: cells[cells.len() - 1].clone(),
                        });
                    }
                    // The details' own table ends: no more of its messages.
                    if row.contains("</table>") {
                        current = None;
                    }
                }
                None => {
                    // The header's labels, `Source File:` and `Translator
                    // Type:`, without their spaces and case.
                    for pair in cells.windows(2) {
                        match pair[0].replace(' ', "").to_lowercase().as_str() {
                            "sourcefile:" => report.source = Some(pair[1].clone()),
                            "translatortype:" => report.translator = Some(pair[1].clone()),
                            _ => {}
                        }
                    }
                }
            },
        }
    }
    Some(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testassembly::{utf16, write_html, write_package};

    fn sample() -> Report {
        let message = |text: &str| Message {
            severity: "Warning".into(),
            text: text.into(),
        };
        Report {
            source: Some(r"D:\models\house.RVT".into()),
            translator: Some("Building".into()),
            components: vec![
                Component {
                    name: "house.iam".into(),
                    status: "Successful".into(),
                    messages: vec![],
                },
                Component {
                    name: "wall.ipt".into(),
                    status: "Warning".into(),
                    messages: vec![message("No Bodies Read From File.")],
                },
                Component {
                    name: "door & frame.ipt".into(),
                    status: "Warning".into(),
                    messages: vec![message("No Bodies Read From File."), message("Second.")],
                },
            ],
        }
    }

    #[test]
    fn reads_a_report_from_its_package() {
        let report = sample();
        let html = write_html(&report).replace("door & frame", "door &amp; frame");
        let stream = write_package("house.htm", &utf16(&html));
        let data = package_data(&stream).expect("the package's data");
        assert_eq!(parse_html(&decode(data)), Some(report.clone()));
        assert_eq!(report.source_extension().as_deref(), Some(".rvt"));
        let wall = report.component("WALL.ipt").expect("listed");
        assert_eq!(wall.messages[0].text, "No Bodies Read From File.");
        assert!(report.component("roof.ipt").is_none());
        // A file name with two spaces in a row, as the page gives it.
        assert_eq!(
            report.component("Door  & frame.ipt"),
            Some(&report.components[2])
        );
    }

    #[test]
    fn reads_the_report_an_assembly_keeps() {
        use crate::testassembly::{assembly_file, converted_file, converted_report, top_assembly};
        let file = IptFile::parse(converted_file()).expect("an assembly");
        assert_eq!(read(&file), Some(converted_report()));
        let plain = IptFile::parse(assembly_file(&top_assembly())).expect("an assembly");
        assert_eq!(read(&plain), None);
    }

    #[test]
    fn other_pages_and_short_packages_are_no_report() {
        assert_eq!(
            parse_html("<html><body><tr><td>a</td></tr></body></html>"),
            None
        );
        let stream = write_package("a.htm", b"<html></html>");
        assert_eq!(package_data(&stream[..stream.len() - 3]), None);
        assert_eq!(package_data(&[1, 0, 0]), None);
    }
}
