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
    writer.document(drawing)
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

    fn document(&mut self, drawing: &Drawing) -> Result<String, Error> {
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
        for (index, entity) in drawing.entities.iter().enumerate() {
            self.entity(entity)
                .map_err(|e| Error::new(format!("entity {index}: {}", e.message)))?;
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
        Ok(out)
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

    fn entity(&mut self, entity: &Entity) -> Result<(), Error> {
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
                    let points = spline_points(spline, self.tolerance)?;
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
        Ok(())
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

const MAX_SPLINE_POINTS: usize = 100_000;
const MAX_SPLINE_DEPTH: u32 = 32;
const MAX_SPLINE_WORK: usize = 10_000_000;

fn spline_limit() -> Error {
    Error::new("R12 spline cannot meet tolerance within the subdivision limit")
}

/// Homogeneous Bezier controls keep rational curves exact during subdivision.
type Homogeneous = [f64; 3];

fn projected([x, y, w]: Homogeneous) -> Result<Point2, Error> {
    let point = Point2::new(x / w, y / w);
    if !finite(&[point.x, point.y]) {
        return Err(Error::new(
            "R12 spline has an undefined or non-finite point",
        ));
    }
    Ok(point)
}

fn blend(a: Homogeneous, b: Homogeneous, t: f64) -> Homogeneous {
    std::array::from_fn(|i| (1.0 - t) * a[i] + t * b[i])
}

/// Distance to the finite segment, including collinear curves that double back.
fn segment_distance(point: Point2, start: Point2, end: Point2) -> f64 {
    let chord = end - start;
    let length = chord.length();
    if length == 0.0 {
        return point.distance(start);
    }
    let direction = chord * (1.0 / length);
    let along = (point - start).dot(direction).clamp(0.0, length);
    point.distance(start + direction * along)
}

struct SplineBudget {
    points: usize,
    work: usize,
}

impl SplineBudget {
    fn spend(&mut self, work: usize) -> Result<(), Error> {
        self.work = self.work.checked_sub(work).ok_or_else(spline_limit)?;
        Ok(())
    }

    fn push(&self, points: &mut Vec<Point2>, point: Point2) -> Result<(), Error> {
        if points.len() >= self.points {
            return Err(spline_limit());
        }
        if points.len() == points.capacity() {
            let capacity = points.capacity().saturating_mul(2).max(1).min(self.points);
            points.reserve_exact(capacity - points.len());
        }
        points.push(point);
        Ok(())
    }
}

/// Bound each Bezier piece by its control polygon's distance to the chord.
/// With weights of one sign the rational curve lies in that convex hull;
/// distance to a segment is convex. This bounds the entire curve, including
/// inflections that a midpoint check misses. Fit-only splines retain the
/// existing polyline through their fit points.
fn spline_points(spline: &Spline, tolerance: f64) -> Result<Vec<Point2>, Error> {
    spline_points_with_limits(spline, tolerance, MAX_SPLINE_POINTS, MAX_SPLINE_DEPTH)
}

fn spline_points_with_limits(
    spline: &Spline,
    tolerance: f64,
    max_points: usize,
    depth: u32,
) -> Result<Vec<Point2>, Error> {
    let mut budget = SplineBudget {
        points: max_points,
        work: MAX_SPLINE_WORK,
    };
    let Some((start, end)) = spline.domain() else {
        if spline.fit_points.len() > max_points {
            return Err(spline_limit());
        }
        return Ok(spline.fit_points.clone());
    };
    let degree = spline.degree as usize;
    let count = spline.control_points.len();
    if degree >= count || start >= end || spline.knots.windows(2).any(|pair| pair[0] > pair[1]) {
        return Err(Error::new("R12 spline has an invalid knot vector"));
    }
    // Normalize weights before forming homogeneous coordinates to avoid
    // overflowing x*w for otherwise ordinary rational curves.
    let scale = (0..count)
        .map(|i| spline.weight(i).abs())
        .fold(0.0, f64::max);
    if scale == 0.0 {
        return Err(Error::new("R12 spline has only zero weights"));
    }
    let order = degree + 1;
    let quadratic_work = order.checked_mul(order).ok_or_else(spline_limit)?;
    let extraction_work = quadratic_work.checked_mul(order).ok_or_else(spline_limit)?;
    let mut points: Vec<Point2> = Vec::new();
    for span in degree..count {
        let left = spline.knots[span];
        let right = spline.knots[span + 1];
        if left == right {
            continue;
        }
        budget.spend(extraction_work)?;
        // The jth Bezier control is the B-spline blossom at degree-j
        // copies of left and j copies of right. Unlike endpoint clamping,
        // this also extracts the correct pieces from periodic knot vectors.
        let mut controls = Vec::with_capacity(order);
        for j in 0..=degree {
            let mut d: Vec<Homogeneous> = (span - degree..=span)
                .map(|i| {
                    let w = spline.weight(i) / scale;
                    let c = spline.control_points[i];
                    [c.x * w, c.y * w, w]
                })
                .collect();
            for r in 1..=degree {
                let t = if r <= degree - j { left } else { right };
                for k in (r..=degree).rev() {
                    let i = span - degree + k;
                    let denominator = spline.knots[i + degree + 1 - r] - spline.knots[i];
                    let alpha = if denominator == 0.0 {
                        0.0
                    } else {
                        (t - spline.knots[i]) / denominator
                    };
                    d[k] = blend(d[k - 1], d[k], alpha);
                }
            }
            controls.push(d[degree]);
        }
        let first = projected(controls[0])?;
        if let Some(&previous) = points.last() {
            if previous.distance(first) > tolerance {
                return Err(Error::new("R12 spline has a discontinuous knot span"));
            }
            if previous != first {
                budget.push(&mut points, first)?;
            }
        } else {
            budget.push(&mut points, first)?;
        }
        refine_bezier(&controls, tolerance, depth, &mut budget, &mut points)?;
    }
    Ok(points)
}

fn refine_bezier(
    controls: &[Homogeneous],
    tolerance: f64,
    depth: u32,
    budget: &mut SplineBudget,
    points: &mut Vec<Point2>,
) -> Result<(), Error> {
    let order = controls.len();
    budget.spend(order.checked_mul(order).ok_or_else(spline_limit)?)?;
    let start = projected(controls[0])?;
    let end = projected(controls[order - 1])?;
    let sign = controls[0][2].signum();
    let same_sign = controls.iter().all(|c| c[2] * sign > 0.0);
    let mut flat = same_sign;
    if same_sign {
        for &control in controls {
            let distance = segment_distance(projected(control)?, start, end);
            flat &= distance.is_finite() && distance <= tolerance;
        }
    }
    if flat {
        return budget.push(points, end);
    }
    if depth == 0 {
        return Err(spline_limit());
    }
    // De Casteljau subdivision preserves both polynomial and rational pieces.
    let mut d = controls.to_vec();
    let mut left = Vec::with_capacity(order);
    let mut right = Vec::with_capacity(order);
    left.push(d[0]);
    right.push(d[order - 1]);
    for remaining in (1..order).rev() {
        for i in 0..remaining {
            d[i] = blend(d[i], d[i + 1], 0.5);
        }
        left.push(d[0]);
        right.push(d[remaining - 1]);
    }
    right.reverse();
    refine_bezier(&left, tolerance, depth - 1, budget, points)?;
    refine_bezier(&right, tolerance, depth - 1, budget, points)
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

    #[test]
    fn spline_limits_reject_inaccurate_chords_and_bound_vertices() {
        let spline = Spline {
            degree: 2,
            control_points: vec![
                Point2::default(),
                Point2::new(1.0, 2.0),
                Point2::new(2.0, 0.0),
            ],
            knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            ..Spline::default()
        };
        let depth_error = spline_points_with_limits(&spline, 0.001, 100, 0).unwrap_err();
        assert!(depth_error.message.contains("subdivision limit"));
        let point_error = spline_points_with_limits(&spline, 0.001, 2, 32).unwrap_err();
        assert!(point_error.message.contains("subdivision limit"));
        // Inspect the destination when subdivision runs out of vertices.
        let mut points = vec![Point2::default()];
        let mut budget = SplineBudget {
            points: 2,
            work: MAX_SPLINE_WORK,
        };
        let controls = [[0.0, 0.0, 1.0], [1.0, 2.0, 1.0], [2.0, 0.0, 1.0]];
        assert!(refine_bezier(&controls, 0.001, 32, &mut budget, &mut points).is_err());
        assert_eq!(points.len(), 2);
        assert!(points.capacity() <= 2);
    }

    #[test]
    fn spline_work_budget_rejects_before_large_degree_extraction() {
        let degree = 1000;
        let spline = Spline {
            degree,
            control_points: vec![Point2::default(); degree as usize + 1],
            knots: std::iter::repeat_n(0.0, degree as usize + 1)
                .chain(std::iter::repeat_n(1.0, degree as usize + 1))
                .collect(),
            ..Spline::default()
        };
        let error = spline_points(&spline, 0.001).unwrap_err();
        assert!(error.message.contains("subdivision limit"));
    }

    #[test]
    fn fit_only_spline_retains_its_polyline() {
        let spline = Spline {
            degree: 3,
            fit_points: vec![
                Point2::default(),
                Point2::new(1.0, 2.0),
                Point2::new(3.0, 0.0),
            ],
            ..Spline::default()
        };
        assert_eq!(spline_points(&spline, 0.001).unwrap(), spline.fit_points);
        assert!(spline_points_with_limits(&spline, 0.001, 2, 32).is_err());
    }
}
