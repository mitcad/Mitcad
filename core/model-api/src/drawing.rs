// SPDX-License-Identifier: MIT
//! The `export_drawing` command (mitcad#471, commands.md): a drawing on
//! sheets written as PDF, SVG, DXF or DWG. With `drawing` it writes a drawing
//! kept in the design (mitcad#707); without, it lays out a temporary one
//! from its options as File › Export Drawing's dialog gives them (the
//! bodies at the marker in a grid of views at a scale that fits) and
//! writes that. Both are drawn the same way (`drawings.rs`): the kernel
//! projects the views and finds the round holes; the `threads` query gives
//! the holes' threads and the sketches their dimensions; `mitcad_model_drawing`
//! lays the sheet out and writes it.

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::{Map, Value, json};

use mitcad_vec3::sub;

use super::ApiError;
use super::drawings::{SheetDrawn, Shown};
use super::prelude::*;
use mitcad_model::Document;
use mitcad_model::FeatureStatus;
use mitcad_model::datum::Vec3;
use mitcad_model::features::FeatureDef;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid};
use mitcad_model::kernel::{Kernel, KernelError};
use mitcad_model::sketch::DimensionKind;
use mitcad_model::sketch::geometry::Curve2;
use mitcad_model::transform::Transform;
use mitcad_model_drawing::compose::{
    self, Composed, HoleInput, SketchDimension, ViewInput, shown_views,
};
use mitcad_model_drawing::convention::ThreadInput;
use mitcad_model_drawing::def::{
    Annotations, Drawing, DrawingSheet, DrawingUid, DrawingView, StandardView, Subject,
    SubjectBody, TitleBlock, ViewDirection, ViewKind, ViewStyle,
};
use mitcad_model_drawing::{
    DrawingOptions, RoundHole, SectionChoice, Units, ViewName, pdf, svg, today,
};
use mitcad_vec3::{cross, dot, unit_div as unit};

/// `export_drawing`: the file, its format, and either a kept drawing (and
/// its sheet) or the bodies and the options of a temporary one (the other
/// fields, [`DrawingOptions`]).
#[derive(Debug)]
pub(super) struct ExportDrawingCommand {
    path: String,
    format: Option<String>,
    bodies: Vec<String>,
    drawing: Option<String>,
    sheet: Option<usize>,
    /// A kept drawing's title where it has none of its own.
    title: Option<String>,
    options: DrawingOptions,
}

impl<'de> Deserialize<'de> for ExportDrawingCommand {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let mut map = Map::<String, Value>::deserialize(deserializer)?;
        let path = match map.remove("path") {
            Some(Value::String(path)) => path,
            Some(_) => return Err(D::Error::custom("\"path\" must be a string")),
            None => return Err(D::Error::missing_field("path")),
        };
        let format = match map.remove("format") {
            None | Some(Value::Null) => None,
            Some(Value::String(format)) => Some(format),
            Some(_) => return Err(D::Error::custom("\"format\" must be a string")),
        };
        let bodies = match map.remove("bodies") {
            None | Some(Value::Null) => Vec::new(),
            Some(value) => serde_json::from_value(value).map_err(D::Error::custom)?,
        };
        let drawing = match map.remove("drawing") {
            None | Some(Value::Null) => None,
            Some(Value::String(drawing)) => Some(drawing),
            Some(_) => return Err(D::Error::custom("\"drawing\" must be a string")),
        };
        let sheet = match map.remove("sheet") {
            None | Some(Value::Null) => None,
            Some(Value::Number(n)) if drawing.is_some() => Some(
                n.as_u64()
                    .ok_or_else(|| D::Error::custom("\"sheet\" is a sheet's index"))?
                    as usize,
            ),
            // An export's sheet size (A4, ...) when no drawing is named.
            Some(value) => {
                map.insert("sheet".to_owned(), value);
                None
            }
        };
        let mut title = None;
        if drawing.is_some() {
            title = match map.remove("title") {
                None | Some(Value::Null) => None,
                Some(Value::String(title)) => Some(title),
                Some(_) => return Err(D::Error::custom("\"title\" must be a string")),
            };
            if let Some(field) = map.keys().next() {
                return Err(D::Error::custom(format!(
                    "a kept drawing is exported as it is: \"{field}\" is its own, edit the drawing"
                )));
            }
            if !bodies.is_empty() {
                return Err(D::Error::custom(
                    "a kept drawing is exported as it is: its views have their subjects",
                ));
            }
        }
        let options = serde_json::from_value(Value::Object(map)).map_err(D::Error::custom)?;
        Ok(Self {
            path,
            format,
            bodies,
            drawing,
            sheet,
            title,
            options,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Pdf,
    Svg,
    Dxf,
    /// R2000 `.dwg` (mitcad#728), the DXF export's content.
    Dwg,
}

impl Format {
    fn of(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "pdf" => Some(Self::Pdf),
            "svg" => Some(Self::Svg),
            "dxf" => Some(Self::Dxf),
            "dwg" => Some(Self::Dwg),
            _ => None,
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Svg => "svg",
            Self::Dxf => "dxf",
            Self::Dwg => "dwg",
        }
    }
}

/// A thread of the `threads` query, for its hole.
pub(super) struct ThreadOf {
    body: BodyUid,
    face: String,
    designation: String,
    internal: bool,
    modeled: bool,
    major: f64,
    minor: f64,
    /// A tapering taper pipe thread's taper on the diameter (mitcad#733).
    taper: f64,
    start: Vec3,
    end: Vec3,
}

fn vec3(value: &Value) -> Option<Vec3> {
    let a = value.as_array()?;
    Some([
        a.first()?.as_f64()?,
        a.get(1)?.as_f64()?,
        a.get(2)?.as_f64()?,
    ])
}

pub(super) fn threads_of(threads: &Value) -> Vec<ThreadOf> {
    threads
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| {
            Some(ThreadOf {
                body: t.get("body")?.as_str()?.parse().ok()?,
                face: t.get("face")?.as_str()?.to_owned(),
                designation: t.get("designation")?.as_str()?.to_owned(),
                internal: t.get("internal").and_then(Value::as_bool).unwrap_or(false),
                modeled: t.get("modeled").and_then(Value::as_bool).unwrap_or(false),
                major: t
                    .get("major_diameter")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
                minor: t
                    .get("minor_diameter")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
                taper: t.get("taper").and_then(Value::as_f64).unwrap_or(0.0),
                start: t.get("start").and_then(vec3)?,
                end: t.get("end").and_then(vec3)?,
            })
        })
        .collect()
}

/// The `threads` query's threads, kept until the next recompute
/// (mitcad#1112: every drawing view draws its threads).
pub(super) fn drawing_threads<K: Kernel>(doc: &Document<K>) -> Vec<ThreadOf> {
    let recompute = doc.recompute_count();
    let cache = doc.drawing_cache();
    let value = cache.threads(recompute).unwrap_or_else(|| {
        let value = std::sync::Arc::new(doc.threads_json());
        cache.keep_threads(recompute, std::sync::Arc::clone(&value));
        value
    });
    threads_of(&value)
}

/// The threads of the bodies a view shows, placed in the drawing's
/// coordinates, to draw by ISO 6410 (`convention::with_threads`), each
/// with its body's index among those projected (the edges' `source`: the
/// view's bodies are projected in this order).
pub(super) fn placed_threads(threads: &[ThreadOf], shown: &[Shown]) -> Vec<ThreadInput> {
    let mut out = Vec::new();
    for (index, s) in shown.iter().enumerate() {
        for t in threads.iter().filter(|t| t.body == s.body) {
            if !(t.major > t.minor && t.minor > 0.0) {
                continue;
            }
            out.push(ThreadInput {
                start: s.transform.apply_point(t.start),
                end: s.transform.apply_point(t.end),
                major: t.major,
                minor: t.minor,
                internal: t.internal,
                taper: t.taper,
                source: Some(index),
            });
        }
    }
    out
}

/// Whether a hole lies on a thread's axis (both in the design's
/// coordinates).
fn on_axis(hole: &RoundHole, start: Vec3, end: Vec3) -> bool {
    let Some(axis) = unit(sub(end, start)) else {
        return false;
    };
    let c = cross(axis, hole.direction);
    if dot(c, c).sqrt() > 1e-6 {
        return false;
    }
    let off = sub(hole.opening, start);
    let across = cross(off, axis);
    dot(across, across).sqrt() < 1e-4
}

/// A temporary drawing laid out from an export's options, with what it
/// draws.
pub(super) struct AutoDrawing {
    pub(super) drawing: Drawing,
    /// The bodies drawn, in order, with their names.
    pub(super) drawn: Vec<(BodyUid, String)>,
    pub(super) warnings: Vec<String>,
}

/// The standard view of an export's view name.
fn standard(name: ViewName) -> StandardView {
    match name {
        ViewName::Front => StandardView::Front,
        ViewName::Top => StandardView::Top,
        ViewName::Right => StandardView::Right,
        ViewName::Iso => StandardView::Iso,
    }
}

/// A document's drawing export with the output checked before it is
/// written. An extension trait (mitcad#777), so that the api can live in a
/// crate above the model.
pub trait DrawingApi<K: Kernel> {
    /// Exports a drawing after the caller has checked every output path.
    /// All paths are checked before the first write, including the numbered
    /// files of a per-body SVG or DXF drawing. Ordinary API exports retain
    /// their existing overwrite behavior.
    fn export_drawing_checked(
        &self,
        request: &str,
        check: impl Fn(&str) -> Result<(), String>,
    ) -> Result<Value, ApiError>;
}

impl<K: Kernel> DrawingApi<K> for Document<K> {
    fn export_drawing_checked(
        &self,
        request: &str,
        check: impl Fn(&str) -> Result<(), String>,
    ) -> Result<Value, ApiError> {
        let command =
            serde_json::from_str::<super::Command>(request).map_err(|e| ApiError(e.to_string()))?;
        let super::Command::ExportDrawing(command) = command else {
            return Err(ApiError("expected export_drawing".to_owned()));
        };
        self.export_drawing_command_checked(command, check)
    }
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait DrawingOps<K: Kernel> {
    fn export_drawing_command(&self, command: ExportDrawingCommand) -> Result<Value, ApiError>;

    /// Lays out a drawing of the root from an export's options: a sheet
    /// (or one per body) with the views asked for in a grid after the
    /// projection method at a scale that fits (or the one given), the
    /// section's view beside them, as `export_drawing` draws them.
    fn auto_drawing(
        &self,
        options: &DrawingOptions,
        bodies: &[String],
    ) -> Result<AutoDrawing, ApiError>;

    /// The round holes of a body where a view shows it, as
    /// [`Self::drawing_holes`] finds them, kept by the body's shape version,
    /// its placement and its threads (mitcad#718: hole tables found them
    /// again on every draw). The warnings of finding them go to `warnings`;
    /// only holes found without one are kept (mitcad#722: a passing kernel
    /// failure was not tried again until the body changed, and a warning
    /// kept a renamed body's old name).
    fn shown_holes(
        &self,
        shown: &Shown,
        threads: &[ThreadOf],
        warnings: &mut Vec<String>,
    ) -> Vec<HoleInput>;

    /// A placed body's round holes in the drawing's coordinates, with their
    /// threads; a modelled thread in a hole that has no cylinder left is a
    /// hole of its major diameter.
    fn drawing_holes(
        &self,
        body: BodyUid,
        placement: &Transform,
        shape: &K::Shape,
        threads: &[ThreadOf],
        warnings: &mut Vec<String>,
    ) -> Vec<HoleInput>;

    /// The dimensions of the sketches at the marker, each in its sketch's
    /// component's coordinates (a drawing places them through the
    /// occurrences, `sketch_placements`): distances, lengths, radii and
    /// diameters, each with its sketch.
    fn sketch_dimensions(&self) -> Vec<(FeatureUid, SketchDimension)>;

    /// The sketches that shape bodies (mitcad#717: a view's sketch
    /// dimensions are those of its own bodies): those of the features that
    /// made them, of the features that name them, and of the features of
    /// their components that neither make nor name a body (a cut through
    /// every body).
    fn sketches_shaping(&self, bodies: &[BodyUid]) -> BTreeSet<FeatureUid>;
}

impl<K: Kernel> DrawingOps<K> for Document<K> {
    fn export_drawing_command(&self, command: ExportDrawingCommand) -> Result<Value, ApiError> {
        self.export_drawing_command_checked(command, |_| Ok(()))
    }

    fn auto_drawing(
        &self,
        options: &DrawingOptions,
        bodies: &[String],
    ) -> Result<AutoDrawing, ApiError> {
        let units = match &options.units {
            Some(symbol) => Units::by_symbol(symbol)
                .ok_or_else(|| ApiError(format!("unknown length unit “{symbol}”")))?,
            None => Units::of(self.parameters().context().default_length_unit),
        };
        if options.views.is_empty() && options.section.is_none() {
            return Err(ApiError("a drawing needs at least one view".to_owned()));
        }
        // The bodies, without the empty ones.
        let mut drawn: Vec<(BodyUid, String)> = Vec::new();
        for uid in self.drawing_bodies(bodies)? {
            let subject = Subject::Bodies {
                bodies: vec![SubjectBody {
                    body: uid,
                    occurrence: None,
                }],
            };
            let shown = self
                .subject_bodies(ComponentUid::ROOT, &subject)
                .map_err(ApiError)?;
            if !shown.is_empty() {
                drawn.push((uid, self.body_name(uid)));
            }
        }
        if drawn.is_empty() {
            return Err(ApiError("there are no bodies to draw".to_owned()));
        }
        let section = match &options.section {
            None => None,
            Some(SectionChoice::Plane { origin, normal }) => Some(
                compose::cut_plane(*origin, *normal)
                    .ok_or_else(|| ApiError("the section plane's normal is zero".to_owned()))?,
            ),
            Some(SectionChoice::Analysis { analysis }) => {
                let found = self
                    .analyses()
                    .iter()
                    .find(|a| &a.name == analysis)
                    .ok_or_else(|| ApiError(format!("there is no analysis {analysis}")))?;
                let mitcad_model::AnalysisDef::Section(def) = &found.def else {
                    return Err(ApiError(format!("{analysis} is not a section analysis")));
                };
                let plane = self
                    .section_plane(def)
                    .map_err(|e| ApiError(format!("{analysis}: {e}")))?;
                Some(
                    compose::cut_plane(plane.origin, plane.normal)
                        .ok_or_else(|| ApiError(format!("{analysis}: no plane")))?,
                )
            }
        };
        let mut names: Vec<ViewName> = Vec::new();
        for v in &options.views {
            if !names.contains(v) {
                names.push(*v);
            }
        }
        let style = ViewStyle {
            hidden_lines: options.hidden_lines,
            tangent_edges: options.tangent_edges,
            shaded: false,
            polygonal: !options.exact,
        };
        let sheet_bodies: Vec<Vec<BodyUid>> = if options.per_body {
            drawn.iter().map(|(b, _)| vec![*b]).collect()
        } else {
            vec![drawn.iter().map(|(b, _)| *b).collect()]
        };
        let threads = threads_of(&self.threads_json());
        let mut warnings = Vec::new();
        let mut sheets = Vec::new();
        // View ids are the drawing's, one counter over all its sheets.
        let mut next_id = 1u32;
        for (index, bodies) in sheet_bodies.iter().enumerate() {
            let subject = Subject::Bodies {
                bodies: bodies
                    .iter()
                    .map(|b| SubjectBody {
                        body: *b,
                        occurrence: None,
                    })
                    .collect(),
            };
            let mut sheet = DrawingSheet::new(format!("Sheet{}", index + 1));
            sheet.size = options.sheet;
            sheet.orientation = options.orientation;
            sheet.projection = options.projection;
            sheet.units = options.units.clone();
            sheet.annotations = Annotations::of(options);
            sheet.title_block = TitleBlock {
                title: Some(
                    options
                        .title
                        .clone()
                        .filter(|t| !t.trim().is_empty())
                        .unwrap_or_else(|| "Untitled".to_owned()),
                ),
                number: None,
                author: None,
                date: Some(options.date.clone().unwrap_or_else(today)),
            };
            // The views asked for, each projected to lay them out.
            let mut kinds: Vec<(Option<ViewName>, ViewKind)> = names
                .iter()
                .map(|n| {
                    (
                        Some(*n),
                        ViewKind::Base {
                            direction: ViewDirection::Named(standard(*n)),
                        },
                    )
                })
                .collect();
            if let Some(plane) = &section {
                kinds.push((
                    None,
                    ViewKind::Section {
                        parent: None,
                        origin: plane.origin,
                        normal: plane.normal,
                        span: None,
                        letter: "A".to_owned(),
                        hatch: true,
                    },
                ));
            }
            let shown: Vec<Shown> = self
                .subject_bodies(ComponentUid::ROOT, &subject)
                .map_err(ApiError)?;
            let mut inputs: Vec<(ViewInput, DrawingView)> = Vec::new();
            for (i, (name, kind)) in kinds.into_iter().enumerate() {
                let view = DrawingView {
                    id: i as u32 + 1,
                    kind,
                    place: [0.0, 0.0],
                    scale: Some(1.0),
                    style: Some(ViewStyle {
                        // A section shows no hidden edges.
                        hidden_lines: style.hidden_lines && name.is_some(),
                        ..style
                    }),
                    label: None,
                    subject: Some(subject.clone()),
                    breaks: Vec::new(),
                    aligned: false,
                    center_marks: false,
                    retrieved_dimensions: false,
                    annotations: Vec::new(),
                    rotation: 0.0,
                    sketch: Vec::new(),
                };
                sheet.views.push(view.clone());
                let spec = sheet.spec(view.id).map_err(ApiError)?;
                let projection = self.projected(&shown, &spec).map_err(|e| {
                    ApiError(format!(
                        "the {} view: {e}",
                        name.map_or("section", ViewName::as_str)
                    ))
                })?;
                inputs.push((
                    ViewInput {
                        name,
                        spec,
                        projection: (*projection).clone(),
                    },
                    view,
                ));
            }
            // The holes, for the room their callouts and table take (the
            // sheet's drawing says what went wrong with them).
            let mut holes: Vec<HoleInput> = Vec::new();
            if options.dimensions || options.hole_table {
                for s in &shown {
                    if let Ok(shapes) = self.placed_shapes(std::slice::from_ref(s)) {
                        holes.extend(self.drawing_holes(
                            s.body,
                            &s.transform,
                            &shapes[0],
                            &threads,
                            &mut Vec::new(),
                        ));
                    }
                }
            }
            // Only the views with something to show stay.
            let (view_inputs, views): (Vec<ViewInput>, Vec<DrawingView>) =
                inputs.into_iter().unzip();
            let laid_out = shown_views(&view_inputs, &mut warnings);
            let layout = compose::layout(options, units, &laid_out, &holes);
            warnings.extend(layout.warnings.iter().cloned());
            // Each at the layout's place: its anchor (the origin) where the
            // layout puts the middle of its box.
            sheet.views.clear();
            for ((input, _), map) in laid_out.iter().zip(&layout.maps) {
                let Some(map) = map else { continue };
                let Some(index) = view_inputs.iter().position(|v| std::ptr::eq(v, *input)) else {
                    continue;
                };
                let mut view = views[index].clone();
                view.scale = Some(layout.scale);
                view.place = [
                    map.place[0] - layout.scale * map.anchor[0],
                    map.place[1] - layout.scale * map.anchor[1],
                ];
                view.id = next_id;
                next_id += 1;
                sheet.views.push(view);
            }
            sheets.push(sheet);
        }
        let drawing = Drawing {
            uid: DrawingUid(0),
            name: String::new(),
            component: ComponentUid::ROOT,
            sheets,
            revisions: Vec::new(),
            exported: 0,
            defaults: Default::default(),
            cut_sheet: None,
        };
        Ok(AutoDrawing {
            drawing,
            drawn,
            warnings,
        })
    }

    fn shown_holes(
        &self,
        shown: &Shown,
        threads: &[ThreadOf],
        warnings: &mut Vec<String>,
    ) -> Vec<HoleInput> {
        let mut fp = mitcad_model::internal::fingerprint::Fingerprint::new("drawing-holes-v1");
        fp.str(&shown.body.to_string())
            .u128(self.body_version(shown.body).unwrap_or(0));
        for row in &shown.transform.linear {
            for v in row {
                fp.f64(*v);
            }
        }
        for v in &shown.transform.translation {
            fp.f64(*v);
        }
        for t in threads.iter().filter(|t| t.body == shown.body) {
            fp.str(&t.face)
                .str(&t.designation)
                .bool(t.internal)
                .bool(t.modeled)
                .f64(t.major);
            for v in t.start.iter().chain(&t.end) {
                fp.f64(*v);
            }
        }
        let key = fp.finish();
        if let Some(holes) = self.drawing_cache().holes(key) {
            return (*holes).clone();
        }
        let mut found = Vec::new();
        let holes = match self.placed_shapes(std::slice::from_ref(shown)) {
            Ok(shapes) => self.drawing_holes(
                shown.body,
                &shown.transform,
                &shapes[0],
                threads,
                &mut found,
            ),
            Err(e) => {
                found.push(format!("the holes of {}: {e}", shown.name));
                Vec::new()
            }
        };
        if found.is_empty() {
            self.drawing_cache()
                .keep_holes(key, std::sync::Arc::new(holes.clone()));
        }
        warnings.extend(found);
        holes
    }

    fn drawing_holes(
        &self,
        body: BodyUid,
        placement: &Transform,
        shape: &K::Shape,
        threads: &[ThreadOf],
        warnings: &mut Vec<String>,
    ) -> Vec<HoleInput> {
        let holes = match self.kernel().round_holes(shape) {
            Ok(holes) => holes,
            Err(KernelError::Unsupported(_)) => return Vec::new(),
            Err(e) => {
                warnings.push(format!("the holes of {}: {e}", self.body_name(body)));
                return Vec::new();
            }
        };
        let mine: Vec<&ThreadOf> = threads.iter().filter(|t| t.body == body).collect();
        let placed = |t: &ThreadOf| (placement.apply_point(t.start), placement.apply_point(t.end));
        let mut used = vec![false; mine.len()];
        let mut out: Vec<HoleInput> = holes
            .into_iter()
            .map(|hole| {
                let by_face = hole.face.as_ref().and_then(|f| {
                    let name = f.to_string();
                    mine.iter().position(|t| t.face == name)
                });
                let found = by_face.or_else(|| {
                    mine.iter().position(|t| {
                        let (s, e) = placed(t);
                        t.internal && on_axis(&hole, s, e)
                    })
                });
                if let Some(i) = found {
                    used[i] = true;
                }
                HoleInput {
                    thread: found.map(|i| mine[i].designation.clone()),
                    hole,
                }
            })
            .collect();
        for (i, t) in mine.iter().enumerate() {
            if used[i] || !t.internal || !t.modeled {
                continue;
            }
            let (s, e) = placed(t);
            let Some(axis) = unit(sub(e, s)) else {
                continue;
            };
            // Opening as a hole open at both ends does: towards +Z, +Y, +X.
            let key = if axis[2].abs() > 1e-9 {
                axis[2]
            } else if axis[1].abs() > 1e-9 {
                axis[1]
            } else {
                axis[0]
            };
            let (opening, direction) = if key > 0.0 {
                (e, axis.map(|v| -v))
            } else {
                (s, axis)
            };
            let depth = dot(sub(e, s), sub(e, s)).sqrt();
            out.push(HoleInput {
                hole: RoundHole {
                    face: t.face.parse().ok(),
                    opening,
                    direction,
                    diameter: t.major,
                    through: false,
                    depth,
                    counterbore: None,
                    countersink: None,
                },
                thread: Some(t.designation.clone()),
            });
        }
        out
    }

    fn sketch_dimensions(&self) -> Vec<(FeatureUid, SketchDimension)> {
        let mut out = Vec::new();
        for entry in self.features() {
            let FeatureDef::Sketch(def) = &entry.def else {
                continue;
            };
            if self.status(entry.uid) != Some(&FeatureStatus::Ok) {
                continue;
            }
            let Some(output) = self.sketch_output(entry.uid) else {
                continue;
            };
            let frame = output.frame;
            let normal = frame.normal();
            let solved = &output.solved;
            let point = |id| solved.points.get(id).map(|p| frame.point(*p));
            let text = |d: &mitcad_model::sketch::Dimension<_>| d.text.map(|t| frame.point(t));
            // Not the inactive ones (mitcad#721): they hold nothing.
            for dim in def.dimensions.iter().filter(|d| def.dimension_applied(d)) {
                let made = match &dim.kind {
                    DimensionKind::Distance { a, b } => {
                        point(a)
                            .zip(point(b))
                            .map(|(pa, pb)| SketchDimension::Linear {
                                a: pa,
                                b: pb,
                                along: None,
                                text: text(dim),
                                value: dot(sub(pb, pa), sub(pb, pa)).sqrt(),
                                normal,
                            })
                    }
                    DimensionKind::HorizontalDistance { a, b, .. }
                    | DimensionKind::VerticalDistance { a, b, .. } => {
                        let along = if matches!(dim.kind, DimensionKind::HorizontalDistance { .. })
                        {
                            frame.x_axis
                        } else {
                            frame.y_axis
                        };
                        point(a)
                            .zip(point(b))
                            .map(|(pa, pb)| SketchDimension::Linear {
                                a: pa,
                                b: pb,
                                along: Some(along),
                                text: text(dim),
                                value: dot(sub(pb, pa), along).abs(),
                                normal,
                            })
                    }
                    DimensionKind::Length { line } => match solved.curves.get(line) {
                        Some(Curve2::Line { a, b }) => {
                            let (pa, pb) = (frame.point(*a), frame.point(*b));
                            Some(SketchDimension::Linear {
                                a: pa,
                                b: pb,
                                along: None,
                                text: text(dim),
                                value: dot(sub(pb, pa), sub(pb, pa)).sqrt(),
                                normal,
                            })
                        }
                        _ => None,
                    },
                    DimensionKind::Radius { curve } | DimensionKind::Diameter { curve } => {
                        let diameter = matches!(dim.kind, DimensionKind::Diameter { .. });
                        match solved.curves.get(curve) {
                            Some(
                                Curve2::Circle { center, radius, .. }
                                | Curve2::Arc { center, radius, .. },
                            ) => Some(SketchDimension::Radial {
                                center: frame.point(*center),
                                radius: *radius,
                                diameter,
                                text: text(dim),
                                value: if diameter { 2.0 * radius } else { *radius },
                                normal,
                            }),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                out.extend(made.map(|d| (entry.uid, d)));
            }
        }
        out
    }

    fn sketches_shaping(&self, bodies: &[BodyUid]) -> BTreeSet<FeatureUid> {
        let makers: BTreeSet<FeatureUid> = bodies.iter().map(|b| b.feature).collect();
        let components: BTreeSet<ComponentUid> = bodies
            .iter()
            .filter_map(|b| self.body_component(*b))
            .collect();
        let every_maker: BTreeSet<FeatureUid> =
            self.bodies().iter().map(|b| b.uid.feature).collect();
        let sketches: BTreeSet<FeatureUid> = self
            .features()
            .filter(|e| matches!(e.def, FeatureDef::Sketch(_)))
            .map(|e| e.uid)
            .collect();
        let mut out = BTreeSet::new();
        for entry in self.features() {
            if entry.suppressed || matches!(entry.def, FeatureDef::Sketch(_)) {
                continue;
            }
            let named = entry.def.references().features;
            let shapes = makers.contains(&entry.uid)
                || named.iter().any(|f| makers.contains(f))
                || (!every_maker.contains(&entry.uid)
                    && named.iter().all(|f| !every_maker.contains(f))
                    && components.contains(&entry.component));
            if shapes {
                out.extend(named.iter().filter(|f| sketches.contains(f)).copied());
            }
        }
        out
    }
}

/// The document's methods used only in this module, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
trait DrawingLocal<K: Kernel> {
    /// The bodies an export draws: those given (by uid or name), or every
    /// body the design shows; each once.
    fn drawing_bodies(&self, given: &[String]) -> Result<Vec<BodyUid>, ApiError>;

    fn export_drawing_command_checked(
        &self,
        command: ExportDrawingCommand,
        check: impl Fn(&str) -> Result<(), String>,
    ) -> Result<Value, ApiError>;
}

impl<K: Kernel> DrawingLocal<K> for Document<K> {
    fn drawing_bodies(&self, given: &[String]) -> Result<Vec<BodyUid>, ApiError> {
        let all = self.bodies();
        let instances = self.instances();
        if given.is_empty() {
            return Ok(all
                .iter()
                .filter(|b| instances.iter().any(|i| i.body == b.uid && i.visible))
                .map(|b| b.uid)
                .collect());
        }
        let mut chosen = Vec::new();
        for body in given {
            let uid = self.body_by_uid_or_name(body)?;
            if chosen.contains(&uid) {
                return Err(ApiError(format!("body {body} is listed more than once")));
            }
            if !all.iter().any(|b| b.uid == uid) {
                return Err(ApiError(format!(
                    "body {uid} does not exist at the timeline marker"
                )));
            }
            chosen.push(uid);
        }
        Ok(chosen)
    }

    fn export_drawing_command_checked(
        &self,
        command: ExportDrawingCommand,
        check: impl Fn(&str) -> Result<(), String>,
    ) -> Result<Value, ApiError> {
        // A cut sheet (mitcad#853) is written as its own DXF or SVG.
        if let Some(name) = &command.drawing
            && self.find_drawing(name)?.cut_sheet.is_some()
        {
            if command.sheet.is_some_and(|s| s > 0) {
                return Err(ApiError(format!("{name} has one sheet")));
            }
            return self.export_cut_sheet_checked(
                name,
                &command.path,
                command.format.as_deref(),
                false,
                &check,
            );
        }
        let extension = std::path::Path::new(&command.path)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_owned);
        let format = match (&command.format, &extension) {
            (Some(name), _) => Format::of(name).ok_or_else(|| {
                ApiError(format!(
                    "unknown drawing format {name} (expected pdf, svg, dxf or dwg)"
                ))
            })?,
            (None, Some(ext)) => Format::of(ext).ok_or_else(|| {
                ApiError(format!(
                    "{}: drawings are written to .pdf, .svg, .dxf or .dwg files",
                    command.path
                ))
            })?,
            (None, None) => {
                return Err(ApiError(format!(
                    "{}: give a .pdf, .svg, .dxf or .dwg file or \"format\"",
                    command.path
                )));
            }
        };

        // The drawing: kept, or laid out for the export.
        let (drawing, drawn, mut warnings, title) = match &command.drawing {
            Some(name) => {
                let drawing = self.find_drawing(name)?.clone();
                (drawing, None, Vec::new(), command.title.clone())
            }
            None => {
                let auto = self.auto_drawing(&command.options, &command.bodies)?;
                (auto.drawing, Some(auto.drawn), auto.warnings, None)
            }
        };
        let indices: Vec<usize> = match command.sheet {
            Some(i) if i < drawing.sheets.len() => vec![i],
            Some(i) => return Err(ApiError(format!("{} has no sheet {i}", drawing.name))),
            None => (0..drawing.sheets.len()).collect(),
        };
        let mut sheets: Vec<SheetDrawn> = Vec::new();
        for i in &indices {
            sheets.push(self.draw_sheet(&drawing, *i, title.as_deref())?);
        }
        let composed: Vec<&Composed> = sheets.iter().map(|s| &s.composed).collect();
        for c in &composed {
            warnings.extend(c.warnings.iter().cloned());
        }

        // The files: a PDF of every sheet, an SVG or DXF file per sheet
        // (the second and later named <name>-2.svg, …).
        let paths: Vec<String> = (0..if format == Format::Pdf {
            1
        } else {
            composed.len()
        })
            .map(|i| {
                if i == 0 {
                    command.path.clone()
                } else {
                    numbered(&command.path, i + 1)
                }
            })
            .collect();
        for path in &paths {
            check(path).map_err(ApiError)?;
        }
        let mut files = Vec::new();
        let write = |path: &str, bytes: &[u8]| {
            std::fs::write(path, bytes).map_err(|e| ApiError(format!("cannot write {path}: {e}")))
        };
        let document_title = composed
            .first()
            .map(|c| c.sheet.title.clone())
            .unwrap_or_default();
        match format {
            Format::Pdf => {
                let pages: Vec<_> = composed.iter().map(|c| c.sheet.clone()).collect();
                write(&command.path, &pdf::write(&pages, &document_title))?;
                files.push(command.path.clone());
            }
            Format::Svg | Format::Dxf | Format::Dwg => {
                for (i, c) in composed.iter().enumerate() {
                    let path = paths[i].clone();
                    let bytes = if format == Format::Svg {
                        svg::write(&c.sheet).into_bytes()
                    } else if format == Format::Dwg {
                        mitcad_model_drawing::dwg::write(&c.sheet)
                            .map_err(|e| ApiError(format!("cannot write {path}: {e}")))?
                    } else {
                        let options = mitcad_dxf::WriteOptions::default();
                        mitcad_dxf::write_string(
                            &mitcad_model_drawing::dxf::drawing(&c.sheet),
                            &options,
                        )
                        .map_err(|e| ApiError(format!("cannot write {path}: {e}")))?
                        .into_bytes()
                    };
                    write(&path, &bytes)?;
                    files.push(path);
                }
            }
        }
        let first = composed
            .first()
            .ok_or_else(|| ApiError("the drawing has no sheet".to_owned()))?;
        let first_sheet = &drawing.sheets[indices[0]];
        let orientation = match first_sheet.orientation {
            mitcad_model_drawing::Orientation::Landscape => "landscape",
            mitcad_model_drawing::Orientation::Portrait => "portrait",
        };
        let scales: Vec<String> = composed
            .iter()
            .map(|c| mitcad_model_drawing::scale_text(c.scale))
            .collect();
        // The bodies drawn: those laid out, or those the kept drawing's
        // views show.
        let drawn: Vec<(BodyUid, String)> = match drawn {
            Some(drawn) => drawn,
            None => {
                let mut out: Vec<(BodyUid, String)> = Vec::new();
                for s in sheets.iter().flat_map(|s| &s.evaluated.shown) {
                    if !out.iter().any(|(b, _)| *b == s.body) {
                        out.push((s.body, s.name.clone()));
                    }
                }
                out
            }
        };
        let bodies: Vec<BodyUid> = drawn.iter().map(|d| d.0).collect();
        let names: Vec<String> = drawn.iter().map(|d| d.1.clone()).collect();
        let pages = if format == Format::Pdf {
            composed.len()
        } else {
            1
        };
        let units = match &first_sheet.units {
            Some(symbol) => Units::by_symbol(symbol).map_or("mm", |u| u.symbol),
            None => Units::of(self.parameters().context().default_length_unit).symbol,
        };
        let mut result = json!({
            "path": command.path,
            "format": format.as_str(),
            "files": files,
            "sheets": composed.len(),
            "pages": pages,
            "sheet": first_sheet.size.name(),
            "orientation": orientation,
            "scale": mitcad_model_drawing::scale_text(first.scale),
            "scales": scales,
            "views": first.views,
            "bodies": bodies,
            "body_names": names,
            "dimensions": composed.iter().map(|c| c.dimensions).sum::<usize>(),
            "holes": composed.iter().map(|c| c.holes).sum::<usize>(),
            "hole_table_rows": composed.iter().map(|c| c.hole_rows).sum::<usize>(),
            "units": units,
            "polygonal": composed.iter().any(|c| c.polygonal),
            "warnings": warnings,
        });
        if command.drawing.is_some() {
            result["drawing"] = json!(drawing.uid);
        }
        Ok(result)
    }
}

/// `part.svg` as `part-2.svg`.
fn numbered(path: &str, n: usize) -> String {
    let p = std::path::Path::new(path);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("drawing");
    let name = match p.extension().and_then(|e| e.to_str()) {
        Some(ext) => format!("{stem}-{n}.{ext}"),
        None => format!("{stem}-{n}"),
    };
    p.with_file_name(name).to_string_lossy().into_owned()
}
