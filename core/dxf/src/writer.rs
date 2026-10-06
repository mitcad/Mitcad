// SPDX-License-Identifier: MIT
//! DXF writer (R12 or R2000).

use std::f64::consts::TAU;
use std::fmt::Write as _;
use std::path::Path;

use crate::error::Error;
use crate::text;
use crate::types::{Drawing, Entity, Geometry, Layer, Point2, Spline};

/// The DXF version to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DxfVersion {
    /// DXF R12 (`AC1009`): read by nearly every program, but it has no
    /// ellipse or spline entity, so those are written as polylines within
    /// [`WriteOptions::tolerance`].
    R12,
    /// DXF R2000 (`AC1015`) with exact ellipses and splines.
    #[default]
    R2000,
}

/// Options for [`write_string`] and [`write_file`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WriteOptions {
    pub version: DxfVersion,
    /// Largest distance, in drawing units, between an R12 polyline and the
    /// ellipse or spline it approximates.
    pub tolerance: f64,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            version: DxfVersion::R2000,
            tolerance: 0.01,
        }
    }
}

/// Writes the drawing to a file.
pub fn write_file(
    drawing: &Drawing,
    path: impl AsRef<Path>,
    options: &WriteOptions,
) -> Result<(), Error> {
    std::fs::write(path, write_string(drawing, options)?)?;
    Ok(())
}

/// The drawing as DXF text. Units go to `$INSUNITS`; non-ASCII text is
/// written as `\U+XXXX` escapes.
pub fn write_string(drawing: &Drawing, options: &WriteOptions) -> Result<String, Error> {
    if options.tolerance.is_nan() || options.tolerance <= 0.0 {
        return Err(Error::new("tolerance must be greater than zero"));
    }
    for (index, entity) in drawing.entities.iter().enumerate() {
        check(entity).map_err(|message| Error::new(format!("entity {index}: {message}")))?;
    }
    let mut writer = Writer {
        out: String::new(),
        version: options.version,
        tolerance: options.tolerance,
        next_handle: 0x20,
        model_space: 0,
    };
    Ok(writer.document(drawing))
}

fn finite(values: &[f64]) -> bool {
    values.iter().all(|v| v.is_finite())
}

fn check(entity: &Entity) -> Result<(), String> {
    let ok = match &entity.geometry {
        Geometry::Point(p) => finite(&[p.x, p.y]),
        Geometry::Line { start, end } => finite(&[start.x, start.y, end.x, end.y]),
        Geometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => finite(&[center.x, center.y, *radius, *start_angle, *end_angle]) && *radius > 0.0,
        Geometry::Circle { center, radius } => {
            finite(&[center.x, center.y, *radius]) && *radius > 0.0
        }
        Geometry::Ellipse {
            center,
            major_axis,
            ratio,
            start_param,
            end_param,
        } => {
            finite(&[
                center.x,
                center.y,
                major_axis.x,
                major_axis.y,
                *ratio,
                *start_param,
                *end_param,
            ]) && major_axis.length() > 0.0
                && *ratio > 0.0
                && *ratio <= 1.0
        }
        Geometry::Spline(spline) => {
            let points_ok = spline
                .control_points
                .iter()
                .chain(&spline.fit_points)
                .all(|p| finite(&[p.x, p.y]));
            let polygon_ok = (spline.control_points.is_empty() && spline.knots.is_empty())
                || spline.domain().is_some();
            let weights_ok =
                spline.weights.is_empty() || spline.weights.len() == spline.control_points.len();
            points_ok
                && polygon_ok
                && weights_ok
                && finite(&spline.knots)
                && finite(&spline.weights)
                && (spline.control_points.len() > spline.degree as usize
                    || spline.fit_points.len() >= 2)
        }
        Geometry::Text(t) => finite(&[t.position.x, t.position.y, t.height, t.rotation]),
    };
    if ok {
        Ok(())
    } else {
        Err("invalid geometry".into())
    }
}

/// A real number with a decimal point and enough digits to read back exactly.
fn real(value: f64) -> String {
    let text = format!("{value}");
    if text.contains('.') || text.contains('e') || text.contains("inf") || text.contains("NaN") {
        text
    } else {
        text + ".0"
    }
}

struct Writer {
    out: String,
    version: DxfVersion,
    tolerance: f64,
    next_handle: u64,
    /// Handle of the *Model_Space block record (R2000), the owner of entities.
    model_space: u64,
}

impl Writer {
    fn pair(&mut self, code: i32, value: &str) {
        let _ = writeln!(self.out, "{code:>3}\n{value}");
    }

    fn int(&mut self, code: i32, value: i64) {
        let _ = writeln!(self.out, "{code:>3}\n{value:>6}");
    }

    fn real(&mut self, code: i32, value: f64) {
        let _ = writeln!(self.out, "{code:>3}\n{}", real(value));
    }

    fn point(&mut self, code: i32, p: Point2) {
        self.real(code, p.x);
        self.real(code + 10, p.y);
        self.real(code + 20, 0.0);
    }

    fn handle(&mut self) -> u64 {
        let handle = self.next_handle;
        self.next_handle += 1;
        handle
    }

    fn handle_pair(&mut self, code: i32, handle: u64) {
        let _ = writeln!(self.out, "{code:>3}\n{handle:X}");
    }

    fn r2000(&self) -> bool {
        self.version == DxfVersion::R2000
    }

    fn document(&mut self, drawing: &Drawing) -> String {
        let layers = layers(drawing);
        // The body first: $HANDSEED in the header needs the last handle.
        self.model_space = 0x11;
        self.section("TABLES");
        self.tables(&layers);
        self.pair(0, "ENDSEC");
        self.section("BLOCKS");
        if self.r2000() {
            self.layout_block(0x11, "*Model_Space", false);
            self.layout_block(0x12, "*Paper_Space", true);
        }
        self.pair(0, "ENDSEC");
        self.section("ENTITIES");
        for entity in &drawing.entities {
            self.entity(entity);
        }
        self.pair(0, "ENDSEC");
        if self.r2000() {
            self.section("OBJECTS");
            // Root dictionary C with the group dictionary D.
            self.pair(0, "DICTIONARY");
            self.handle_pair(5, 0xC);
            self.handle_pair(330, 0);
            self.pair(100, "AcDbDictionary");
            self.int(281, 1);
            self.pair(3, "ACAD_GROUP");
            self.handle_pair(350, 0xD);
            self.pair(0, "DICTIONARY");
            self.handle_pair(5, 0xD);
            self.handle_pair(330, 0xC);
            self.pair(100, "AcDbDictionary");
            self.int(281, 1);
            self.pair(0, "ENDSEC");
        }
        self.pair(0, "EOF");
        let body = std::mem::take(&mut self.out);

        self.section("HEADER");
        self.pair(9, "$ACADVER");
        self.pair(1, if self.r2000() { "AC1015" } else { "AC1009" });
        if self.r2000() {
            self.pair(9, "$DWGCODEPAGE");
            self.pair(3, "ANSI_1252");
            self.pair(9, "$HANDSEED");
            let seed = self.next_handle;
            self.handle_pair(5, seed);
        }
        self.pair(9, "$INSUNITS");
        self.int(70, i64::from(drawing.units.code()));
        self.pair(9, "$MEASUREMENT");
        self.int(70, i64::from(drawing.units.is_metric()));
        self.pair(0, "ENDSEC");
        if self.r2000() {
            self.section("CLASSES");
            self.pair(0, "ENDSEC");
        }
        let mut out = std::mem::take(&mut self.out);
        out.push_str(&body);
        out
    }

    fn section(&mut self, name: &str) {
        self.pair(0, "SECTION");
        self.pair(2, name);
    }

    /// Starts a table; `handle` is its R2000 handle.
    fn table(&mut self, name: &str, handle: u64, count: usize) {
        self.pair(0, "TABLE");
        self.pair(2, name);
        if self.r2000() {
            self.handle_pair(5, handle);
            self.handle_pair(330, 0);
            self.pair(100, "AcDbSymbolTable");
        }
        self.int(70, count as i64);
    }

    /// Starts a table entry with its R2000 handle, owner and subclass.
    fn entry(&mut self, kind: &str, table: u64, subclass: &str) {
        self.pair(0, kind);
        if self.r2000() {
            let handle = self.handle();
            self.handle_pair(if kind == "DIMSTYLE" { 105 } else { 5 }, handle);
            self.handle_pair(330, table);
            self.pair(100, "AcDbSymbolTableRecord");
            self.pair(100, subclass);
        }
    }

    fn tables(&mut self, layers: &[Layer]) {
        let r2000 = self.r2000();
        // Fixed table handles; entries get handles from 0x20.
        if r2000 {
            self.table("VPORT", 0x8, 0);
            self.pair(0, "ENDTAB");
        }

        let linetypes: &[&str] = if r2000 {
            &["ByBlock", "ByLayer", "Continuous"]
        } else {
            &["CONTINUOUS"]
        };
        self.table("LTYPE", 0x5, linetypes.len());
        for &name in linetypes {
            self.entry("LTYPE", 0x5, "AcDbLinetypeTableRecord");
            self.pair(2, name);
            self.int(70, 0);
            self.pair(
                3,
                if name.eq_ignore_ascii_case("continuous") {
                    "Solid line"
                } else {
                    ""
                },
            );
            self.int(72, 65);
            self.int(73, 0);
            self.real(40, 0.0);
        }
        self.pair(0, "ENDTAB");

        self.table("LAYER", 0x2, layers.len());
        for layer in layers {
            self.entry("LAYER", 0x2, "AcDbLayerTableRecord");
            self.pair(2, &text::encode(&layer.name));
            self.int(70, i64::from(layer.frozen));
            let color = i64::from(layer.color.clamp(1, 255));
            self.int(62, if layer.visible { color } else { -color });
            let linetype = if r2000 && layer.linetype.eq_ignore_ascii_case("continuous") {
                "Continuous".to_owned()
            } else {
                text::encode(&layer.linetype)
            };
            self.pair(6, &linetype);
            if r2000 {
                self.int(370, -3);
            }
        }
        self.pair(0, "ENDTAB");

        self.table("STYLE", 0x3, 1);
        self.entry("STYLE", 0x3, "AcDbTextStyleTableRecord");
        self.pair(2, if r2000 { "Standard" } else { "STANDARD" });
        self.int(70, 0);
        self.real(40, 0.0);
        self.real(41, 1.0);
        self.real(50, 0.0);
        self.int(71, 0);
        self.real(42, 2.5);
        self.pair(3, "txt");
        self.pair(4, "");
        self.pair(0, "ENDTAB");

        if r2000 {
            self.table("VIEW", 0x6, 0);
            self.pair(0, "ENDTAB");
            self.table("UCS", 0x7, 0);
            self.pair(0, "ENDTAB");

            self.table("APPID", 0x9, 1);
            self.entry("APPID", 0x9, "AcDbRegAppTableRecord");
            self.pair(2, "ACAD");
            self.int(70, 0);
            self.pair(0, "ENDTAB");

            self.table("DIMSTYLE", 0xA, 1);
            self.pair(100, "AcDbDimStyleTable");
            self.entry("DIMSTYLE", 0xA, "AcDbDimStyleTableRecord");
            self.pair(2, "Standard");
            self.int(70, 0);
            self.pair(0, "ENDTAB");

            self.table("BLOCK_RECORD", 0x1, 2);
            for (handle, name) in [(0x11, "*Model_Space"), (0x12, "*Paper_Space")] {
                self.pair(0, "BLOCK_RECORD");
                self.handle_pair(5, handle);
                self.handle_pair(330, 0x1);
                self.pair(100, "AcDbSymbolTableRecord");
                self.pair(100, "AcDbBlockTableRecord");
                self.pair(2, name);
            }
            self.pair(0, "ENDTAB");
        }
    }

    fn layout_block(&mut self, record: u64, name: &str, paper: bool) {
        self.pair(0, "BLOCK");
        let handle = self.handle();
        self.handle_pair(5, handle);
        self.handle_pair(330, record);
        self.pair(100, "AcDbEntity");
        if paper {
            self.int(67, 1);
        }
        self.pair(8, "0");
        self.pair(100, "AcDbBlockBegin");
        self.pair(2, name);
        self.int(70, 0);
        self.point(10, Point2::default());
        self.pair(3, name);
        self.pair(1, "");
        self.pair(0, "ENDBLK");
        let handle = self.handle();
        self.handle_pair(5, handle);
        self.handle_pair(330, record);
        self.pair(100, "AcDbEntity");
        if paper {
            self.int(67, 1);
        }
        self.pair(8, "0");
        self.pair(100, "AcDbBlockEnd");
    }

    /// Starts an entity: type, R2000 handle and owner, layer, subclass.
    fn begin(&mut self, kind: &str, layer: &str, subclass: &str) {
        self.pair(0, kind);
        if self.r2000() {
            let handle = self.handle();
            self.handle_pair(5, handle);
            let owner = self.model_space;
            self.handle_pair(330, owner);
            self.pair(100, "AcDbEntity");
        }
        self.pair(8, &text::encode(layer));
        if self.r2000() {
            self.pair(100, subclass);
        }
    }

    fn entity(&mut self, entity: &Entity) {
        let layer = entity.layer.as_str();
        match &entity.geometry {
            Geometry::Point(p) => {
                self.begin("POINT", layer, "AcDbPoint");
                self.point(10, *p);
            }
            Geometry::Line { start, end } => {
                self.begin("LINE", layer, "AcDbLine");
                self.point(10, *start);
                self.point(11, *end);
            }
            Geometry::Circle { center, radius } => {
                self.begin("CIRCLE", layer, "AcDbCircle");
                self.point(10, *center);
                self.real(40, *radius);
            }
            Geometry::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            } => {
                self.begin("ARC", layer, "AcDbCircle");
                self.point(10, *center);
                self.real(40, *radius);
                if self.r2000() {
                    self.pair(100, "AcDbArc");
                }
                self.real(50, start_angle.to_degrees());
                self.real(51, end_angle.to_degrees());
            }
            Geometry::Ellipse {
                center,
                major_axis,
                ratio,
                start_param,
                end_param,
            } => {
                if self.r2000() {
                    self.begin("ELLIPSE", layer, "AcDbEllipse");
                    self.point(10, *center);
                    self.point(11, *major_axis);
                    self.normal();
                    self.real(40, *ratio);
                    self.real(41, *start_param);
                    self.real(42, *end_param);
                } else {
                    let full = entity.geometry.is_full_ellipse();
                    let points = ellipse_points(
                        *center,
                        *major_axis,
                        *ratio,
                        *start_param,
                        *end_param,
                        self.tolerance,
                    );
                    self.polyline(layer, &points, full);
                }
            }
            Geometry::Spline(spline) => {
                if self.r2000() {
                    self.spline(layer, spline);
                } else {
                    let points = spline_points(spline, self.tolerance);
                    self.polyline(layer, &points, false);
                }
            }
            Geometry::Text(t) => {
                self.begin("TEXT", layer, "AcDbText");
                self.point(10, t.position);
                self.real(40, t.height);
                self.pair(1, &text::encode(&t.content));
                self.real(50, t.rotation.to_degrees());
                if self.r2000() {
                    self.pair(100, "AcDbText");
                }
            }
        }
    }

    fn normal(&mut self) {
        self.real(210, 0.0);
        self.real(220, 0.0);
        self.real(230, 1.0);
    }

    fn spline(&mut self, layer: &str, spline: &Spline) {
        self.begin("SPLINE", layer, "AcDbSpline");
        self.normal();
        let rational = spline.is_rational();
        let flags =
            8 + i64::from(spline.closed) + 2 * i64::from(spline.periodic) + 4 * i64::from(rational);
        self.int(70, flags);
        self.int(71, i64::from(spline.degree));
        self.int(72, spline.knots.len() as i64);
        self.int(73, spline.control_points.len() as i64);
        self.int(74, spline.fit_points.len() as i64);
        self.real(42, 1e-10);
        self.real(43, 1e-10);
        if !spline.fit_points.is_empty() {
            self.real(44, 1e-10);
        }
        for &knot in &spline.knots {
            self.real(40, knot);
        }
        if rational {
            for &weight in &spline.weights {
                self.real(41, weight);
            }
        }
        for &p in &spline.control_points {
            self.point(10, p);
        }
        for &p in &spline.fit_points {
            self.point(11, p);
        }
    }

    /// An R12 polyline through `points`.
    fn polyline(&mut self, layer: &str, points: &[Point2], closed: bool) {
        let layer = text::encode(layer);
        self.pair(0, "POLYLINE");
        self.pair(8, &layer);
        self.int(66, 1);
        self.point(10, Point2::default());
        self.int(70, i64::from(closed));
        for &p in points {
            self.pair(0, "VERTEX");
            self.pair(8, &layer);
            self.point(10, p);
        }
        self.pair(0, "SEQEND");
        self.pair(8, &layer);
    }
}

/// Layer table: the drawing's layers, then layers that only entities name,
/// with layer 0 always present.
fn layers(drawing: &Drawing) -> Vec<Layer> {
    let mut layers: Vec<Layer> = Vec::new();
    let mut add = |layer: Layer| {
        if !layers
            .iter()
            .any(|l| l.name.eq_ignore_ascii_case(&layer.name))
        {
            layers.push(layer);
        }
    };
    add(drawing
        .layer("0")
        .cloned()
        .unwrap_or_else(|| Layer::new("0")));
    for layer in &drawing.layers {
        add(layer.clone());
    }
    for entity in &drawing.entities {
        add(Layer::new(entity.layer.clone()));
    }
    layers
}

/// Points of an elliptical arc, at most `tolerance` from the curve. A full
/// ellipse leaves out the closing point (the polyline is closed).
fn ellipse_points(
    center: Point2,
    major: Point2,
    ratio: f64,
    start: f64,
    end: f64,
    tolerance: f64,
) -> Vec<Point2> {
    let a = major.length();
    let step = if tolerance < a {
        2.0 * (1.0 - tolerance / a).acos()
    } else {
        TAU / 8.0
    };
    let sweep = end - start;
    let full = (sweep - TAU).abs() < 1e-12;
    let count = ((sweep / step).ceil() as usize).clamp(if full { 8 } else { 2 }, 100_000);
    let last = if full { count - 1 } else { count };
    (0..=last)
        .map(|i| {
            let t = start + sweep * i as f64 / count as f64;
            center + major * t.cos() + major.perp() * (ratio * t.sin())
        })
        .collect()
}

/// Points of a spline, at most about `tolerance` from the curve; the fit
/// points when it has no control polygon.
fn spline_points(spline: &Spline, tolerance: f64) -> Vec<Point2> {
    let Some((start, end)) = spline.domain() else {
        return spline.fit_points.clone();
    };
    // Distinct knots inside the domain split the curve into polynomial pieces.
    let mut breaks: Vec<f64> = spline
        .knots
        .iter()
        .copied()
        .filter(|&k| k > start && k < end)
        .collect();
    breaks.insert(0, start);
    breaks.push(end);
    breaks.dedup();
    let at = |t: f64| spline.point_at(t).unwrap_or_default();
    let mut points = vec![at(start)];
    for pair in breaks.windows(2) {
        let pieces = 4;
        for i in 0..pieces {
            let t0 = pair[0] + (pair[1] - pair[0]) * i as f64 / pieces as f64;
            let t1 = pair[0] + (pair[1] - pair[0]) * (i + 1) as f64 / pieces as f64;
            refine(&at, t0, t1, tolerance, 12, &mut points);
        }
    }
    points
}

/// Appends points after `t0` up to `t1`, splitting while the curve's
/// midpoint is farther than `tolerance` from the chord.
fn refine(
    at: &impl Fn(f64) -> Point2,
    t0: f64,
    t1: f64,
    tolerance: f64,
    depth: u32,
    points: &mut Vec<Point2>,
) {
    let (p0, p1) = (at(t0), at(t1));
    let tm = 0.5 * (t0 + t1);
    let pm = at(tm);
    let chord = p1 - p0;
    let deviation = if chord.length() > 0.0 {
        chord.cross(pm - p0).abs() / chord.length()
    } else {
        pm.distance(p0)
    };
    if depth == 0 || deviation <= tolerance {
        points.push(p1);
    } else {
        refine(at, t0, tm, tolerance, depth - 1, points);
        refine(at, tm, t1, tolerance, depth - 1, points);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reals_keep_a_decimal_point() {
        assert_eq!(real(10.0), "10.0");
        assert_eq!(real(-0.5), "-0.5");
        assert_eq!(real(0.1 + 0.2), "0.30000000000000004");
    }

    #[test]
    fn ellipse_polyline_within_tolerance() {
        let points = ellipse_points(
            Point2::default(),
            Point2::new(10.0, 0.0),
            0.5,
            0.0,
            TAU,
            0.01,
        );
        assert!(points.len() >= 8);
        for p in &points {
            let value = (p.x / 10.0).powi(2) + (p.y / 5.0).powi(2);
            assert!((value - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn rejects_invalid_geometry() {
        let mut drawing = Drawing::default();
        drawing.push(
            "0",
            Geometry::Circle {
                center: Point2::default(),
                radius: f64::NAN,
            },
        );
        assert!(write_string(&drawing, &WriteOptions::default()).is_err());
    }
}
