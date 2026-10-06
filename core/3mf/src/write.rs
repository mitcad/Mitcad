// SPDX-License-Identifier: MIT
//! Writing a [`Model`] as a 3MF package.

use std::fmt::Write as _;

use mitcad_zip::{Method, Writer};

use crate::{CORE_NAMESPACE, Error, MODEL_CONTENT_TYPE, MODEL_RELATIONSHIP, Model, Placement};

/// The package's parts.
const CONTENT_TYPES: &str = "[Content_Types].xml";
const RELATIONSHIPS: &str = "_rels/.rels";
const MODEL_PART: &str = "3D/3dmodel.model";

/// The 3MF package of a model: one build item, an object named after the
/// model whose components place a mesh object per part (each placement of
/// a part a component of its own), in millimetres. Parts with a colour
/// refer to it in one `basematerials` group, named after them. Every part
/// needs a mesh with triangles and at least one placement.
pub fn write(model: &Model) -> Result<Vec<u8>, Error> {
    if model.parts.is_empty() {
        return Err(Error("nothing to write: no parts".to_owned()));
    }
    for part in &model.parts {
        if part.mesh.triangles.is_empty() {
            return Err(Error(format!("{} has no triangles", part.name)));
        }
        if part.placements.is_empty() {
            return Err(Error(format!("{} is not placed", part.name)));
        }
    }
    let mut zip = Writer::new();
    let failed = |e: mitcad_zip::ZipError| Error(e.to_string());
    zip.add(CONTENT_TYPES, content_types().as_bytes(), Method::Deflate)
        .map_err(failed)?;
    zip.add(RELATIONSHIPS, relationships().as_bytes(), Method::Deflate)
        .map_err(failed)?;
    zip.add(MODEL_PART, model_xml(model).as_bytes(), Method::Deflate)
        .map_err(failed)?;
    zip.finish().map_err(failed)
}

fn content_types() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\n \
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\n \
         <Default Extension=\"model\" ContentType=\"{MODEL_CONTENT_TYPE}\"/>\n\
         </Types>\n"
    )
}

fn relationships() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\n \
         <Relationship Target=\"/{MODEL_PART}\" Id=\"rel0\" Type=\"{MODEL_RELATIONSHIP}\"/>\n\
         </Relationships>\n"
    )
}

/// The model part. Object ids: the material group 1, the parts' mesh
/// objects from 2 in order, the assembly last.
fn model_xml(model: &Model) -> String {
    let triangles: usize = model.parts.iter().map(|p| p.mesh.triangles.len()).sum();
    let vertices: usize = model.parts.iter().map(|p| p.mesh.vertices.len()).sum();
    let mut xml = String::with_capacity(1024 + vertices * 64 + triangles * 48);
    let _ = write!(
        xml,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <model unit=\"millimeter\" xml:lang=\"en-US\" xmlns=\"{CORE_NAMESPACE}\">\n"
    );
    if !model.application.is_empty() {
        let _ = writeln!(
            xml,
            " <metadata name=\"Application\">{}</metadata>",
            escape(&model.application)
        );
    }
    if !model.name.is_empty() {
        let _ = writeln!(
            xml,
            " <metadata name=\"Title\">{}</metadata>",
            escape(&model.name)
        );
    }
    xml.push_str(" <resources>\n");
    // The colours, one base material per coloured part.
    let mut material = vec![None; model.parts.len()];
    let coloured: Vec<_> = model
        .parts
        .iter()
        .enumerate()
        .filter(|(_, p)| p.color.is_some())
        .collect();
    if !coloured.is_empty() {
        xml.push_str("  <basematerials id=\"1\">\n");
        for (index, (i, part)) in coloured.into_iter().enumerate() {
            let [r, g, b] = part.color.unwrap_or_default().map(channel);
            let _ = writeln!(
                xml,
                "   <base name=\"{}\" displaycolor=\"#{r:02X}{g:02X}{b:02X}FF\"/>",
                escape(&part.name)
            );
            material[i] = Some(index);
        }
        xml.push_str("  </basematerials>\n");
    }
    for (i, part) in model.parts.iter().enumerate() {
        let _ = write!(
            xml,
            "  <object id=\"{}\" type=\"model\" name=\"{}\"",
            i + 2,
            escape(&part.name)
        );
        if let Some(index) = material[i] {
            let _ = write!(xml, " pid=\"1\" pindex=\"{index}\"");
        }
        xml.push_str(">\n   <mesh>\n    <vertices>\n");
        for v in &part.mesh.vertices {
            let _ = writeln!(
                xml,
                "     <vertex x=\"{}\" y=\"{}\" z=\"{}\"/>",
                number(v[0]),
                number(v[1]),
                number(v[2])
            );
        }
        xml.push_str("    </vertices>\n    <triangles>\n");
        for t in &part.mesh.triangles {
            let _ = writeln!(
                xml,
                "     <triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>",
                t[0], t[1], t[2]
            );
        }
        xml.push_str("    </triangles>\n   </mesh>\n  </object>\n");
    }
    let assembly = model.parts.len() + 2;
    let _ = writeln!(
        xml,
        "  <object id=\"{assembly}\" type=\"model\" name=\"{}\">\n   <components>",
        escape(&model.name)
    );
    for (i, part) in model.parts.iter().enumerate() {
        for placement in &part.placements {
            let _ = write!(xml, "    <component objectid=\"{}\"", i + 2);
            if *placement != Placement::IDENTITY {
                let numbers: Vec<String> = placement.to_3mf().iter().map(|&m| number(m)).collect();
                let _ = write!(xml, " transform=\"{}\"", numbers.join(" "));
            }
            xml.push_str("/>\n");
        }
    }
    xml.push_str("   </components>\n  </object>\n </resources>\n <build>\n");
    let _ = writeln!(xml, "  <item objectid=\"{assembly}\"/>");
    xml.push_str(" </build>\n</model>\n");
    xml
}

/// A colour component in [0, 1] as a byte.
fn channel(c: f64) -> u8 {
    (c.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A number with at most 6 decimals (a nanometre in millimetres), without
/// trailing zeros or an exponent: `12.5`, `-0.000125`, `0`.
fn number(value: f64) -> String {
    let rounded = (value * 1e6).round() / 1e6;
    if rounded == 0.0 || !rounded.is_finite() {
        return "0".to_owned();
    }
    let mut text = format!("{rounded:.6}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

/// Text for XML attributes and elements.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // XML 1.0 has no other control characters.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
pub(crate) fn number_for_tests(value: f64) -> String {
    number(value)
}
