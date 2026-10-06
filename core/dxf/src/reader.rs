// SPDX-License-Identifier: MIT
//! ASCII DXF reader (R12 to R2018).

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use crate::error::Error;
use crate::math::{Affine, bulge_arc, ellipse_from_conjugate};
use crate::text;
use crate::types::{Drawing, Entity, Geometry, Layer, Point2, Spline, Text};
use crate::units::Units;

/// Reads a DXF file.
pub fn read_file(path: impl AsRef<Path>) -> Result<Drawing, Error> {
    read(&std::fs::read(path)?)
}

/// Reads DXF text.
pub fn read_str(text: &str) -> Result<Drawing, Error> {
    read(text.as_bytes())
}

/// Reads the bytes of an ASCII DXF file. Text is UTF-8 (R2007 and newer) or
/// Windows-1252. Binary DXF is not supported.
pub fn read(bytes: &[u8]) -> Result<Drawing, Error> {
    let pairs = tokenize(bytes)?;
    let mut reader = Reader::default();
    let mut drawing = Drawing::default();
    let mut model = Vec::new();
    for section in sections(&pairs) {
        match section.name.as_str() {
            "HEADER" => read_header(section.pairs, &mut drawing),
            "TABLES" => drawing.layers = read_layers(section.pairs),
            "BLOCKS" => reader.read_blocks(section.pairs),
            "ENTITIES" => model = reader.read_items(&records(section.pairs)),
            _ => {}
        }
    }
    let mut entities = Vec::new();
    reader.expand(&model, &Affine::IDENTITY, None, 0, &mut entities);
    drawing.entities = entities;
    drawing.warnings = reader.warnings;
    for (kind, count) in reader.skipped {
        drawing
            .warnings
            .push(format!("skipped {count} {kind} entities"));
    }
    Ok(drawing)
}

/// One group: a code and its value.
#[derive(Debug)]
struct Pair {
    code: i32,
    value: String,
    /// 1-based line of the group code.
    line: usize,
}

impl Pair {
    fn is(&self, code: i32, value: &str) -> bool {
        self.code == code && self.value.trim().eq_ignore_ascii_case(value)
    }

    fn f64(&self) -> Result<f64, Error> {
        let value = self.value.trim();
        value
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| {
                Error::at(
                    self.line + 1,
                    format!("invalid number {value:?} for group {}", self.code),
                )
            })
    }

    fn int(&self) -> Result<i64, Error> {
        let value = self.value.trim();
        value
            .parse::<i64>()
            .ok()
            .or_else(|| {
                value
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.fract() == 0.0)
                    .map(|v| v as i64)
            })
            .ok_or_else(|| {
                Error::at(
                    self.line + 1,
                    format!("invalid integer {value:?} for group {}", self.code),
                )
            })
    }
}

/// A binary DXF file starts with a 22-byte sentinel: a seven-letter word,
/// then ` Binary DXF\r\n\x1a\0` (bytes 7 to 21).
fn is_binary(bytes: &[u8]) -> bool {
    bytes.get(7..22) == Some(b" Binary DXF\r\n\x1a\x00".as_slice())
}

fn tokenize(bytes: &[u8]) -> Result<Vec<Pair>, Error> {
    if is_binary(bytes) {
        return Err(Error::new(
            "binary DXF is not supported; save the file as ASCII DXF",
        ));
    }
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let mut lines = bytes
        .split(|&b| b == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .enumerate();
    let mut pairs = Vec::new();
    while let Some((index, code_line)) = lines.next() {
        let code_text = text::decode_line(code_line);
        let code_text = code_text.trim();
        if code_text.is_empty() {
            // Blank lines at the end of the file.
            if lines
                .clone()
                .all(|(_, l)| l.iter().all(u8::is_ascii_whitespace))
            {
                break;
            }
            return Err(Error::at(index + 1, "empty group code"));
        }
        let code = code_text
            .parse::<i32>()
            .map_err(|_| Error::at(index + 1, format!("invalid group code {code_text:?}")))?;
        let Some((_, value_line)) = lines.next() else {
            return Err(Error::at(index + 1, "group code without a value"));
        };
        let value = text::decode_line(value_line).trim_end().to_owned();
        pairs.push(Pair {
            code,
            value,
            line: index + 1,
        });
    }
    Ok(pairs)
}

struct Section<'a> {
    name: String,
    pairs: &'a [Pair],
}

fn sections(pairs: &[Pair]) -> Vec<Section<'_>> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < pairs.len() {
        if pairs[i].is(0, "EOF") {
            break;
        }
        if pairs[i].is(0, "SECTION") && i + 1 < pairs.len() && pairs[i + 1].code == 2 {
            let name = pairs[i + 1].value.trim().to_ascii_uppercase();
            let start = i + 2;
            let mut end = start;
            while end < pairs.len() && !pairs[end].is(0, "ENDSEC") && !pairs[end].is(0, "EOF") {
                end += 1;
            }
            result.push(Section {
                name,
                pairs: &pairs[start..end],
            });
            i = end + 1;
        } else {
            i += 1;
        }
    }
    result
}

/// A record: a code-0 group (entity type, table entry, ...) with its groups.
struct Record<'a> {
    kind: String,
    pairs: &'a [Pair],
    line: usize,
}

fn records(pairs: &[Pair]) -> Vec<Record<'_>> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < pairs.len() {
        if pairs[i].code != 0 {
            i += 1;
            continue;
        }
        let start = i + 1;
        let mut end = start;
        while end < pairs.len() && pairs[end].code != 0 {
            end += 1;
        }
        result.push(Record {
            kind: pairs[i].value.trim().to_ascii_uppercase(),
            pairs: &pairs[start..end],
            line: pairs[i].line,
        });
        i = end;
    }
    result
}

impl Record<'_> {
    fn first(&self, code: i32) -> Option<&Pair> {
        self.pairs.iter().find(|p| p.code == code)
    }

    fn string(&self, code: i32) -> Option<&str> {
        self.first(code).map(|p| p.value.as_str())
    }

    fn f64_or(&self, code: i32, default: f64) -> Result<f64, Error> {
        self.first(code).map_or(Ok(default), Pair::f64)
    }

    fn int_or(&self, code: i32, default: i64) -> Result<i64, Error> {
        self.first(code).map_or(Ok(default), Pair::int)
    }

    fn point(&self, code: i32) -> Result<[f64; 3], Error> {
        Ok([
            self.f64_or(code, 0.0)?,
            self.f64_or(code + 10, 0.0)?,
            self.f64_or(code + 20, 0.0)?,
        ])
    }

    fn layer(&self) -> String {
        self.string(8)
            .map_or_else(|| "0".to_owned(), |l| l.trim().to_owned())
    }

    fn extrusion(&self) -> Result<[f64; 3], Error> {
        Ok([
            self.f64_or(210, 0.0)?,
            self.f64_or(220, 0.0)?,
            self.f64_or(230, 1.0)?,
        ])
    }
}

fn read_header(pairs: &[Pair], drawing: &mut Drawing) {
    let mut i = 0;
    while i < pairs.len() {
        if pairs[i].code != 9 {
            i += 1;
            continue;
        }
        let name = pairs[i].value.trim().to_ascii_uppercase();
        let value = pairs.get(i + 1).filter(|p| p.code != 9);
        match (name.as_str(), value) {
            ("$ACADVER", Some(v)) => drawing.version = Some(v.value.trim().to_owned()),
            ("$INSUNITS", Some(v)) => match v.int() {
                Ok(code) => match i32::try_from(code).ok().and_then(Units::from_code) {
                    Some(units) => drawing.units = units,
                    None => drawing
                        .warnings
                        .push(format!("unknown $INSUNITS {code}; drawing is unitless")),
                },
                Err(error) => drawing.warnings.push(error.to_string()),
            },
            _ => {}
        }
        i += 1;
    }
}

fn read_layers(pairs: &[Pair]) -> Vec<Layer> {
    let mut layers = Vec::new();
    for record in records(pairs).iter().filter(|r| r.kind == "LAYER") {
        let Some(name) = record.string(2) else {
            continue;
        };
        let mut layer = Layer::new(name.trim());
        let color = record.int_or(62, 7).unwrap_or(7);
        layer.visible = color >= 0;
        layer.color = i16::try_from(color.abs()).unwrap_or(7);
        layer.frozen = record.int_or(70, 0).unwrap_or(0) & 1 != 0;
        if let Some(linetype) = record.string(6) {
            layer.linetype = linetype.trim().to_owned();
        }
        layers.push(layer);
    }
    layers
}

/// The object coordinate system of an entity with extrusion direction `n`
/// (the DXF arbitrary axis algorithm), as the affine map from OCS x, y at
/// elevation `z` to world x, y. `None` when the OCS plane is perpendicular
/// to the XY plane.
fn ocs_to_xy(n: [f64; 3], z: f64) -> Option<Affine> {
    let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if length == 0.0 {
        return Some(Affine::translation(0.0, 0.0));
    }
    let n = [n[0] / length, n[1] / length, n[2] / length];
    let world = if n[0].abs() < 1.0 / 64.0 && n[1].abs() < 1.0 / 64.0 {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let ax = normalize(cross(world, n));
    let ay = normalize(cross(n, ax));
    let affine = Affine {
        a: ax[0],
        b: ay[0],
        c: ax[1],
        d: ay[1],
        tx: z * n[0],
        ty: z * n[1],
    };
    (affine.determinant().abs() > 1e-9).then_some(affine)
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(v: [f64; 3]) -> [f64; 3] {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / length, v[1] / length, v[2] / length]
}

fn xy(p: [f64; 3]) -> Point2 {
    Point2::new(p[0], p[1])
}

/// Model space or block content before block references are expanded.
enum Item {
    Entity(Entity),
    Insert(Insert),
}

struct Insert {
    block: String,
    layer: String,
    /// One transform per array element (one for a plain INSERT).
    transforms: Vec<Affine>,
    line: usize,
}

struct Block {
    items: Vec<Item>,
}

#[derive(Default)]
struct Reader {
    blocks: HashMap<String, Block>,
    warnings: Vec<String>,
    skipped: BTreeMap<String, usize>,
}

/// Block nesting deeper than this is taken as a reference cycle.
const MAX_BLOCK_DEPTH: usize = 32;

impl Reader {
    fn read_blocks(&mut self, pairs: &[Pair]) {
        let records = records(pairs);
        let mut i = 0;
        while i < records.len() {
            if records[i].kind != "BLOCK" {
                i += 1;
                continue;
            }
            let header = &records[i];
            let mut end = i + 1;
            while end < records.len() && records[end].kind != "ENDBLK" {
                end += 1;
            }
            let name = header.string(2).unwrap_or_default().trim().to_owned();
            let flags = header.int_or(70, 0).unwrap_or(0);
            let base = header.point(10).unwrap_or([0.0; 3]);
            let mut items = Vec::new();
            if flags & 4 != 0 {
                self.warnings
                    .push(format!("external reference {name} is not loaded"));
            } else {
                items = self.read_items(&records[i + 1..end]);
            }
            if base[0] != 0.0 || base[1] != 0.0 {
                let shift = Affine::translation(-base[0], -base[1]);
                items = items
                    .into_iter()
                    .map(|item| shifted(item, &shift))
                    .collect();
            }
            self.blocks
                .insert(name.to_ascii_uppercase(), Block { items });
            i = end + 1;
        }
    }

    fn skip(&mut self, kind: &str) {
        *self.skipped.entry(kind.to_owned()).or_default() += 1;
    }

    fn read_items(&mut self, records: &[Record<'_>]) -> Vec<Item> {
        let mut items = Vec::new();
        let mut i = 0;
        while i < records.len() {
            let record = &records[i];
            i += 1;
            if record.int_or(67, 0).unwrap_or(0) == 1 {
                self.skip("paper space");
                continue;
            }
            let result = match record.kind.as_str() {
                "POLYLINE" => {
                    let start = i;
                    while i < records.len() && records[i].kind == "VERTEX" {
                        i += 1;
                    }
                    let vertices = &records[start..i];
                    if i < records.len() && records[i].kind == "SEQEND" {
                        i += 1;
                    }
                    self.polyline(record, vertices, &mut items)
                }
                "LWPOLYLINE" => self.lwpolyline(record, &mut items),
                "INSERT" => match insert(record) {
                    Ok(Some(insert)) => {
                        items.push(Item::Insert(insert));
                        Ok(())
                    }
                    Ok(None) => {
                        self.warnings.push(format!(
                            "line {}: INSERT perpendicular to the XY plane skipped",
                            record.line
                        ));
                        Ok(())
                    }
                    Err(error) => Err(error),
                },
                "SEQEND" | "VERTEX" | "ATTDEF" => Ok(()),
                _ => match geometry(record) {
                    Ok(Converted::Geometry(geometry)) => {
                        items.push(Item::Entity(Entity::new(record.layer(), geometry)));
                        Ok(())
                    }
                    Ok(Converted::Nothing) => Ok(()),
                    Ok(Converted::Perpendicular) => {
                        self.warnings.push(format!(
                            "line {}: {} perpendicular to the XY plane skipped",
                            record.line, record.kind
                        ));
                        Ok(())
                    }
                    Ok(Converted::Unsupported) => {
                        self.skip(&record.kind);
                        Ok(())
                    }
                    Err(error) => Err(error),
                },
            };
            if let Err(error) = result {
                self.warnings
                    .push(format!("{} skipped: {error}", record.kind));
            }
        }
        items
    }

    fn lwpolyline(&mut self, record: &Record<'_>, items: &mut Vec<Item>) -> Result<(), Error> {
        let mut vertices: Vec<(Point2, f64)> = Vec::new();
        for pair in record.pairs {
            match pair.code {
                10 => vertices.push((Point2::new(pair.f64()?, 0.0), 0.0)),
                20 => {
                    if let Some(v) = vertices.last_mut() {
                        v.0.y = pair.f64()?;
                    }
                }
                42 => {
                    if let Some(v) = vertices.last_mut() {
                        v.1 = pair.f64()?;
                    }
                }
                _ => {}
            }
        }
        let closed = record.int_or(70, 0)? & 1 != 0;
        let elevation = record.f64_or(38, 0.0)?;
        let Some(ocs) = ocs_to_xy(record.extrusion()?, elevation) else {
            self.warnings.push(format!(
                "line {}: LWPOLYLINE perpendicular to the XY plane skipped",
                record.line
            ));
            return Ok(());
        };
        push_polyline(&vertices, closed, &ocs, &record.layer(), items);
        Ok(())
    }

    fn polyline(
        &mut self,
        record: &Record<'_>,
        vertices: &[Record<'_>],
        items: &mut Vec<Item>,
    ) -> Result<(), Error> {
        let flags = record.int_or(70, 0)?;
        if flags & (16 | 64) != 0 {
            self.skip("POLYLINE mesh");
            return Ok(());
        }
        let is_3d = flags & 8 != 0;
        let mut points = Vec::new();
        for vertex in vertices {
            // Spline frame control points are not on the curve.
            if vertex.int_or(70, 0)? & 16 != 0 {
                continue;
            }
            let p = vertex.point(10)?;
            let bulge = if is_3d { 0.0 } else { vertex.f64_or(42, 0.0)? };
            points.push((xy(p), bulge));
        }
        let closed = flags & 1 != 0;
        let ocs = if is_3d {
            Some(Affine::IDENTITY)
        } else {
            ocs_to_xy(record.extrusion()?, record.f64_or(30, 0.0)?)
        };
        let Some(ocs) = ocs else {
            self.warnings.push(format!(
                "line {}: POLYLINE perpendicular to the XY plane skipped",
                record.line
            ));
            return Ok(());
        };
        push_polyline(&points, closed, &ocs, &record.layer(), items);
        Ok(())
    }

    /// Expands block references into `out`. Entities on layer 0 inside a
    /// block take the layer of the reference (the DXF rule for layer 0).
    fn expand(
        &mut self,
        items: &[Item],
        transform: &Affine,
        layer: Option<&str>,
        depth: usize,
        out: &mut Vec<Entity>,
    ) {
        let identity = *transform == Affine::IDENTITY;
        for item in items {
            match item {
                Item::Entity(entity) => {
                    let layer_name = match layer {
                        Some(layer) if entity.layer == "0" => layer.to_owned(),
                        _ => entity.layer.clone(),
                    };
                    let geometry = if identity {
                        entity.geometry.clone()
                    } else {
                        entity.geometry.transformed(transform)
                    };
                    out.push(Entity::new(layer_name, geometry));
                }
                Item::Insert(insert) => {
                    if depth >= MAX_BLOCK_DEPTH {
                        self.warnings.push(format!(
                            "line {}: block {} nested too deeply (reference cycle?)",
                            insert.line, insert.block
                        ));
                        continue;
                    }
                    let key = insert.block.to_ascii_uppercase();
                    // Take the block out while expanding it, so a cycle finds it missing.
                    let Some(block) = self.blocks.remove(&key) else {
                        self.warnings.push(format!(
                            "line {}: block {} is missing or refers to itself",
                            insert.line, insert.block
                        ));
                        continue;
                    };
                    let insert_layer = match layer {
                        Some(layer) if insert.layer == "0" => layer.to_owned(),
                        _ => insert.layer.clone(),
                    };
                    for element in &insert.transforms {
                        self.expand(
                            &block.items,
                            &transform.then_after(element),
                            Some(&insert_layer),
                            depth + 1,
                            out,
                        );
                    }
                    self.blocks.insert(key, block);
                }
            }
        }
    }
}

fn shifted(item: Item, shift: &Affine) -> Item {
    match item {
        Item::Entity(entity) => Item::Entity(Entity::new(
            entity.layer,
            entity.geometry.transformed(shift),
        )),
        Item::Insert(mut insert) => {
            for t in &mut insert.transforms {
                *t = shift.then_after(t);
            }
            Item::Insert(insert)
        }
    }
}

/// Lines and arcs of a polyline with bulges, mapped by `ocs`.
fn push_polyline(
    vertices: &[(Point2, f64)],
    closed: bool,
    ocs: &Affine,
    layer: &str,
    items: &mut Vec<Item>,
) {
    let n = vertices.len();
    if n < 2 {
        return;
    }
    let segments = if closed { n } else { n - 1 };
    for i in 0..segments {
        let (start, bulge) = vertices[i];
        let end = vertices[(i + 1) % n].0;
        if start == end {
            continue;
        }
        let geometry = bulge_arc(start, end, bulge).transformed(ocs);
        items.push(Item::Entity(Entity::new(layer, geometry)));
    }
}

fn insert(record: &Record<'_>) -> Result<Option<Insert>, Error> {
    let block = record.string(2).unwrap_or_default().trim().to_owned();
    let position = record.point(10)?;
    let sx = record.f64_or(41, 1.0)?;
    let sy = record.f64_or(42, 1.0)?;
    let rotation = record.f64_or(50, 0.0)?.to_radians();
    let columns = record.int_or(70, 1)?.clamp(1, 10_000);
    let rows = record.int_or(71, 1)?.clamp(1, 10_000);
    let column_spacing = record.f64_or(44, 0.0)?;
    let row_spacing = record.f64_or(45, 0.0)?;
    let Some(ocs) = ocs_to_xy(record.extrusion()?, position[2]) else {
        return Ok(None);
    };
    let placement = ocs
        .then_after(&Affine::translation(position[0], position[1]))
        .then_after(&Affine::rotation(rotation));
    let mut transforms = Vec::new();
    for row in 0..rows {
        for column in 0..columns {
            let offset =
                Affine::translation(column as f64 * column_spacing, row as f64 * row_spacing);
            transforms.push(
                placement
                    .then_after(&offset)
                    .then_after(&Affine::scale(sx, sy)),
            );
        }
    }
    Ok(Some(Insert {
        block,
        layer: record.layer(),
        transforms,
        line: record.line,
    }))
}

enum Converted {
    Geometry(Geometry),
    /// A known entity that carries no 2D geometry (invisible attribute).
    Nothing,
    /// The OCS plane is perpendicular to XY.
    Perpendicular,
    Unsupported,
}

/// Geometry of a single entity, in the coordinates of its block.
fn geometry(record: &Record<'_>) -> Result<Converted, Error> {
    let ocs = |z: f64| -> Result<Option<Affine>, Error> { Ok(ocs_to_xy(record.extrusion()?, z)) };
    let in_ocs = |geometry: Geometry, z: f64| -> Result<Converted, Error> {
        Ok(match ocs(z)? {
            Some(transform) if transform == Affine::IDENTITY => Converted::Geometry(geometry),
            Some(transform) => Converted::Geometry(geometry.transformed(&transform)),
            None => Converted::Perpendicular,
        })
    };
    match record.kind.as_str() {
        "POINT" => Ok(Converted::Geometry(Geometry::Point(xy(record.point(10)?)))),
        "LINE" => Ok(Converted::Geometry(Geometry::Line {
            start: xy(record.point(10)?),
            end: xy(record.point(11)?),
        })),
        "CIRCLE" => {
            let center = record.point(10)?;
            let radius = record.f64_or(40, 0.0)?;
            in_ocs(
                Geometry::Circle {
                    center: xy(center),
                    radius,
                },
                center[2],
            )
        }
        "ARC" => {
            let center = record.point(10)?;
            let arc = Geometry::arc(
                xy(center),
                record.f64_or(40, 0.0)?,
                record.f64_or(50, 0.0)?.to_radians(),
                record.f64_or(51, 360.0)?.to_radians(),
            );
            in_ocs(arc, center[2])
        }
        "ELLIPSE" => {
            // World coordinates; the parameter runs counter-clockwise about
            // the extrusion direction n, so the minor axis is n x major.
            let center = record.point(10)?;
            let major = record.point(11)?;
            let ratio = record.f64_or(40, 1.0)?;
            let n = record.extrusion()?;
            let minor = cross(n, major);
            let minor_length =
                (minor[0] * minor[0] + minor[1] * minor[1] + minor[2] * minor[2]).sqrt();
            let major_length =
                (major[0] * major[0] + major[1] * major[1] + major[2] * major[2]).sqrt();
            if minor_length == 0.0 || major_length == 0.0 {
                return Err(Error::at(record.line, "degenerate ellipse"));
            }
            let scale = ratio * major_length / minor_length;
            let minor = Point2::new(minor[0] * scale, minor[1] * scale);
            let major = xy(major);
            if major.cross(minor).abs() < 1e-12 * major_length * major_length {
                return Ok(Converted::Perpendicular);
            }
            let start = record.f64_or(41, 0.0)?;
            let end = record.f64_or(42, std::f64::consts::TAU)?;
            let (start, end) = crate::types::normalize_range(start, end);
            Ok(Converted::Geometry(ellipse_from_conjugate(
                xy(center),
                major,
                minor,
                start,
                end,
            )))
        }
        "SPLINE" => spline(record).map(Converted::Geometry),
        "TEXT" | "ATTRIB" => {
            if record.kind == "ATTRIB" && record.int_or(70, 0)? & 1 != 0 {
                return Ok(Converted::Nothing);
            }
            let position = record.point(10)?;
            let geometry = Geometry::Text(Text {
                position: xy(position),
                height: record.f64_or(40, 0.0)?,
                rotation: record.f64_or(50, 0.0)?.to_radians(),
                content: text::single_line(record.string(1).unwrap_or_default()),
            });
            in_ocs(geometry, position[2])
        }
        "MTEXT" => {
            let mut content = String::new();
            for pair in record.pairs.iter().filter(|p| p.code == 3) {
                content.push_str(&pair.value);
            }
            content.push_str(record.string(1).unwrap_or_default());
            // The x direction vector, when present, overrides the rotation.
            let rotation = match record.first(11) {
                Some(_) => {
                    let direction = record.point(11)?;
                    direction[1].atan2(direction[0])
                }
                None => record.f64_or(50, 0.0)?.to_radians(),
            };
            Ok(Converted::Geometry(Geometry::Text(Text {
                position: xy(record.point(10)?),
                height: record.f64_or(40, 0.0)?,
                rotation: rotation.rem_euclid(std::f64::consts::TAU),
                content: text::multi_line(&content),
            })))
        }
        _ => Ok(Converted::Unsupported),
    }
}

fn spline(record: &Record<'_>) -> Result<Geometry, Error> {
    let flags = record.int_or(70, 0)?;
    let degree = record.int_or(71, 3)?;
    let degree = u32::try_from(degree)
        .ok()
        .filter(|d| (1..=25).contains(d))
        .ok_or_else(|| Error::at(record.line, format!("invalid spline degree {degree}")))?;
    let mut spline = Spline {
        degree,
        closed: flags & 1 != 0,
        periodic: flags & 2 != 0,
        ..Spline::default()
    };
    for pair in record.pairs {
        match pair.code {
            40 => spline.knots.push(pair.f64()?),
            41 => spline.weights.push(pair.f64()?),
            10 => spline.control_points.push(Point2::new(pair.f64()?, 0.0)),
            20 => {
                if let Some(p) = spline.control_points.last_mut() {
                    p.y = pair.f64()?;
                }
            }
            11 => spline.fit_points.push(Point2::new(pair.f64()?, 0.0)),
            21 => {
                if let Some(p) = spline.fit_points.last_mut() {
                    p.y = pair.f64()?;
                }
            }
            _ => {}
        }
    }
    if !spline.weights.is_empty() && spline.weights.len() != spline.control_points.len() {
        return Err(Error::at(
            record.line,
            "spline weights do not match its control points",
        ));
    }
    if spline.weights.iter().all(|&w| w == 1.0) {
        spline.weights.clear();
    }
    let valid = !spline.control_points.is_empty()
        && spline.knots.len() == spline.control_points.len() + degree as usize + 1;
    if !valid {
        if spline.fit_points.len() >= 2 {
            spline.control_points.clear();
            spline.knots.clear();
            spline.weights.clear();
        } else {
            return Err(Error::at(
                record.line,
                "spline has neither a valid control polygon nor fit points",
            ));
        }
    }
    Ok(Geometry::Spline(spline))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arbitrary_axis() {
        assert_eq!(ocs_to_xy([0.0, 0.0, 1.0], 0.0), Some(Affine::IDENTITY));
        let flipped = ocs_to_xy([0.0, 0.0, -1.0], 5.0).unwrap();
        assert_eq!(flipped.apply(Point2::new(2.0, 3.0)), Point2::new(-2.0, 3.0));
        assert_eq!(ocs_to_xy([1.0, 0.0, 0.0], 0.0), None);
    }

    #[test]
    fn rejects_binary_and_bad_codes() {
        assert!(read(b"Example Binary DXF\r\n\x1a\x00").is_err());
        let error = read_str("0\nSECTION\nxx\nENTITIES\n").unwrap_err();
        assert_eq!(error.line, Some(3));
    }
}
