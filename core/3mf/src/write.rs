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
/// needs a mesh with triangles and at least one placement. With
/// [`Model::objects`] (mitcad#856) the build has an item per object, each
/// of the placements it names.
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
    for object in &model.objects {
        if object.components.is_empty() {
            return Err(Error(format!("object {} has no components", object.name)));
        }
        let placed = |&(part, placement): &(usize, usize)| {
            model
                .parts
                .get(part)
                .is_some_and(|p| placement < p.placements.len())
        };
        if !object.components.iter().all(placed) {
            return Err(Error(format!(
                "object {} refers to a placement of a part that is not there",
                object.name
            )));
        }
    }
    for (path, _) in &model.files {
        let reserved = [CONTENT_TYPES, RELATIONSHIPS, MODEL_PART];
        if path.is_empty()
            || path.starts_with('/')
            || path.contains('\\')
            || path
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == "..")
            || reserved.contains(&path.as_str())
        {
            return Err(Error(format!(
                "{path} is no path of another file of the package"
            )));
        }
    }
    let mut zip = Writer::new();
    let failed = |e: mitcad_zip::ZipError| Error(e.to_string());
    zip.add(
        CONTENT_TYPES,
        content_types(&model.files).as_bytes(),
        Method::Deflate,
    )
    .map_err(failed)?;
    zip.add(RELATIONSHIPS, relationships().as_bytes(), Method::Deflate)
        .map_err(failed)?;
    zip.add(MODEL_PART, model_xml(model).as_bytes(), Method::Deflate)
        .map_err(failed)?;
    for (path, data) in &model.files {
        zip.add(path, data, Method::Deflate).map_err(failed)?;
    }
    zip.finish().map_err(failed)
}

/// The content types: the relationships, the model, and a generic one for
/// each other extension of the package's files.
fn content_types(files: &[(String, Vec<u8>)]) -> String {
    let mut extensions: Vec<String> = files
        .iter()
        .filter_map(|(path, _)| {
            let name = path.rsplit('/').next()?;
            let (_, extension) = name.rsplit_once('.')?;
            Some(extension.to_ascii_lowercase())
        })
        .filter(|e| !e.is_empty() && e != "rels" && e != "model")
        .collect();
    extensions.sort();
    extensions.dedup();
    let mut more = String::new();
    for extension in extensions {
        let _ = writeln!(
            more,
            " <Default Extension=\"{}\" ContentType=\"application/octet-stream\"/>",
            escape(&extension)
        );
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\n \
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\n \
         <Default Extension=\"model\" ContentType=\"{MODEL_CONTENT_TYPE}\"/>\n\
         {more}\
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
/// objects from 2 in order, the build's objects last.
fn model_xml(model: &Model) -> String {
    let triangles: usize = model.parts.iter().map(|p| p.mesh.triangles.len()).sum();
    let vertices: usize = model.parts.iter().map(|p| p.mesh.vertices.len()).sum();
    let mut xml = String::with_capacity(1024 + vertices * 64 + triangles * 48);
    let mut namespaces = String::new();
    for (prefix, uri) in &model.namespaces {
        let _ = write!(namespaces, " xmlns:{}=\"{}\"", escape(prefix), escape(uri));
    }
    let unit = model.unit.as_str();
    let decimals = model.unit.decimals();
    let _ = write!(
        xml,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <model unit=\"{unit}\" xml:lang=\"en-US\" xmlns=\"{CORE_NAMESPACE}\"{namespaces}>\n"
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
    for (name, value) in &model.metadata {
        let _ = writeln!(
            xml,
            " <metadata name=\"{}\">{}</metadata>",
            escape(name),
            escape(value)
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
        xml.push_str(">\n");
        // The object's own metadata: a slicer's per-object settings.
        if !part.metadata.is_empty() {
            xml.push_str("   <metadatagroup>\n");
            for (name, value) in &part.metadata {
                let _ = writeln!(
                    xml,
                    "    <metadata name=\"{}\" preserve=\"true\" type=\"xs:string\">{}</metadata>",
                    escape(name),
                    escape(value)
                );
            }
            xml.push_str("   </metadatagroup>\n");
        }
        xml.push_str("   <mesh>\n    <vertices>\n");
        for v in &part.mesh.vertices {
            let _ = writeln!(
                xml,
                "     <vertex x=\"{}\" y=\"{}\" z=\"{}\"/>",
                number(v[0], decimals),
                number(v[1], decimals),
                number(v[2], decimals)
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
    // The build's objects: one of every placement by default, else those
    // of the model (mitcad#856).
    let objects: Vec<(&str, Vec<(usize, usize)>)> = if model.objects.is_empty() {
        let every = model
            .parts
            .iter()
            .enumerate()
            .flat_map(|(i, part)| (0..part.placements.len()).map(move |k| (i, k)))
            .collect();
        vec![(model.name.as_str(), every)]
    } else {
        model
            .objects
            .iter()
            .map(|o| (o.name.as_str(), o.components.clone()))
            .collect()
    };
    for (index, (name, components)) in objects.iter().enumerate() {
        let _ = writeln!(
            xml,
            "  <object id=\"{}\" type=\"model\" name=\"{}\">\n   <components>",
            crate::build_object_id(model.parts.len(), index),
            escape(name)
        );
        for &(part, k) in components {
            let placement = &model.parts[part].placements[k];
            let _ = write!(xml, "    <component objectid=\"{}\"", part + 2);
            if *placement != Placement::IDENTITY {
                let numbers: Vec<String> = placement
                    .to_3mf()
                    .iter()
                    .map(|&m| number(m, decimals))
                    .collect();
                let _ = write!(xml, " transform=\"{}\"", numbers.join(" "));
            }
            xml.push_str("/>\n");
        }
        xml.push_str("   </components>\n  </object>\n");
    }
    xml.push_str(" </resources>\n <build>\n");
    for index in 0..objects.len() {
        let _ = writeln!(
            xml,
            "  <item objectid=\"{}\"/>",
            crate::build_object_id(model.parts.len(), index)
        );
    }
    xml.push_str(" </build>\n</model>\n");
    xml
}

/// A colour component in [0, 1] as a byte.
fn channel(c: f64) -> u8 {
    (c.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A number with at most `decimals` decimals (6: a nanometre in
/// millimetres), without trailing zeros or an exponent: `12.5`,
/// `-0.000125`, `0`.
fn number(value: f64, decimals: usize) -> String {
    let scale = 10f64.powi(decimals as i32);
    let rounded = (value * scale).round() / scale;
    if rounded == 0.0 || !rounded.is_finite() {
        return "0".to_owned();
    }
    let mut text = format!("{rounded:.decimals$}");
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
pub(crate) fn number_for_tests(value: f64, decimals: usize) -> String {
    number(value, decimals)
}
