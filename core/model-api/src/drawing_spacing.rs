// SPDX-License-Identifier: MIT
//! The `drawing_snap` query (mitcad#1173, commands.md "Dimension
//! spacing"): where a dimension goes when its placing click or its drag
//! snaps to the spacing of the dimension placed before it in its drawing,
//! and to multiples of it (`mitcad_model_drawing::spacing`). The
//! application asks it with the very command the click or the drag would
//! run, shows the answer while the dimension is placed or dragged, and
//! runs the command with the snapped place; the commands themselves place
//! exactly where they are told. A note's, a datum label's or a frame's
//! drag snaps its leader to multiples of 45 degrees instead (mitcad#1294,
//! `mitcad_model_drawing::annotate::snap_leader`).

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::ApiError;
use super::annotations::{Units as WordUnits, frame_words};
use super::drawing_annotations::{AddAnnotationCommand, Asked, EditAnnotationCommand};
use super::drawings::Evaluated;
use super::prelude::*;
use mitcad_model::Document;
use mitcad_model::kernel::Kernel;
use mitcad_model_drawing::annotate::{self, AnnotationType, Frame, Seen, ViewAnnotation};
use mitcad_model_drawing::def::Drawing;
use mitcad_model_drawing::spacing::{self, Family, Gauge, Snap};
use mitcad_model_drawing::{P2, Units};

/// `drawing_snap`: the command a placing click or a drag would run, and
/// whether it is placed freely (the application's Alt; Shift or Alt for a
/// drag).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SnapQuery {
    /// An `add_drawing_annotation` with its `at`, or an
    /// `edit_drawing_annotation` with its `move`.
    command: Value,
    #[serde(default)]
    free: bool,
}

/// The kinds of annotation that snap: the dimensions.
fn spaced(kind: AnnotationType) -> bool {
    matches!(
        kind,
        AnnotationType::Linear
            | AnnotationType::Ordinate
            | AnnotationType::Angle
            | AnnotationType::Radius
            | AnnotationType::Diameter
    )
}

/// The kinds of annotation whose drag snaps their leader to multiples of
/// 45 degrees (mitcad#1294): a note's text, a datum's square, a frame.
fn led(kind: AnnotationType) -> bool {
    matches!(
        kind,
        AnnotationType::Note | AnnotationType::Datum | AnnotationType::Frame
    )
}

/// A feature control frame's cells as the sheet draws them (their widths
/// decide where its leader leaves it); none for the others.
fn frame_cells<K: Kernel>(
    doc: &Document<K>,
    drawing: &Drawing,
    si: usize,
    annotation: &ViewAnnotation,
) -> Vec<String> {
    let Some(tolerance) = annotation
        .frame
        .as_ref()
        .filter(|_| annotation.kind == AnnotationType::Frame)
    else {
        return Vec::new();
    };
    let units = drawing.sheets[si]
        .units
        .as_deref()
        .and_then(Units::by_symbol)
        .unwrap_or_else(|| Units::of(doc.parameters().context().default_length_unit));
    let letters = doc.all_datum_letters();
    let datums: Vec<&str> = letters.iter().map(String::as_str).collect();
    frame_words(tolerance, WordUnits::new(units.symbol, units.mm), &datums).0
}

/// Whether a dimension's place was given (clicked, dragged or set): a
/// dimension placed for itself has no spacing of its own.
fn placed_by_hand(annotation: &ViewAnnotation) -> bool {
    match annotation.kind {
        AnnotationType::Linear | AnnotationType::Ordinate | AnnotationType::Angle => {
            annotation.line_offset.is_some()
        }
        AnnotationType::Radius | AnnotationType::Diameter => {
            annotation.line_offset.is_some() || annotation.text_offset.is_some()
        }
        _ => false,
    }
}

pub(super) trait DrawingSpacingOps<K: Kernel> {
    /// `drawing_snap`: the place (`at`) or the drag (`move`) the command
    /// gets, snapped or as given, with where that puts the dimension.
    fn drawing_snap_json(&self, query: &SnapQuery) -> Result<Value, ApiError>;

    /// The spacing of the dimension of a family placed last in a drawing
    /// (the highest id, `except` left out), and its id; None when there is
    /// none.
    fn dimension_spacing(
        &self,
        drawing: &Drawing,
        family: Family,
        except: Option<u32>,
    ) -> Option<(f64, u32)>;
}

impl<K: Kernel> DrawingSpacingOps<K> for Document<K> {
    fn drawing_snap_json(&self, query: &SnapQuery) -> Result<Value, ApiError> {
        let mut fields: Map<String, Value> =
            query.command.as_object().cloned().ok_or_else(|| {
                ApiError("`command` is the command to snap, an object".to_owned())
            })?;
        let cmd = fields
            .remove("cmd")
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        match cmd.as_str() {
            "add_drawing_annotation" => {
                let c: AddAnnotationCommand = super::parse(Value::Object(fields), "command")?;
                snap_new(self, &c, query.free)
            }
            "edit_drawing_annotation" => {
                let c: EditAnnotationCommand = super::parse(Value::Object(fields), "command")?;
                snap_moved(self, &c, query.free)
            }
            other => Err(ApiError(format!(
                "drawing_snap snaps an add_drawing_annotation or an edit_drawing_annotation, \
                 not {other:?}"
            ))),
        }
    }

    fn dimension_spacing(
        &self,
        drawing: &Drawing,
        family: Family,
        except: Option<u32>,
    ) -> Option<(f64, u32)> {
        // The dimensions placed by hand, the newest first.
        let mut placed: Vec<(u32, usize, u32)> = Vec::new();
        for (si, sheet) in drawing.sheets.iter().enumerate() {
            for view in &sheet.views {
                for a in &view.annotations {
                    if Some(a.id) != except && placed_by_hand(a) {
                        placed.push((a.id, si, view.id));
                    }
                }
            }
        }
        placed.sort_unstable_by_key(|a| std::cmp::Reverse(a.0));
        let mut sheets: HashMap<usize, Evaluated> = HashMap::new();
        for (id, si, view_id) in placed {
            let evaluated = sheets
                .entry(si)
                .or_insert_with(|| self.evaluate_views(drawing, si));
            let Ok(view) = self.view_at(drawing, si, evaluated, view_id) else {
                continue;
            };
            let Some(own) = drawing.sheets[si].view(view_id) else {
                continue;
            };
            let frame = view.frame();
            let gauge_of = |a: &ViewAnnotation| -> Option<Gauge> {
                let seen = self.annotation_seen(&view, a).ok()?;
                spacing::gauge(a, &seen, &frame)
            };
            let Some(g) = own
                .annotations
                .iter()
                .find(|a| a.id == id)
                .and_then(gauge_of)
            else {
                continue;
            };
            if g.family != family {
                continue;
            }
            // Stacked with the others of its view: its step from the one
            // inside it.
            let others: Vec<Gauge> = own
                .annotations
                .iter()
                .filter(|a| a.id != id && Some(a.id) != except && placed_by_hand(a))
                .filter_map(gauge_of)
                .collect();
            return Some((spacing::spacing(&g, &others), id));
        }
        None
    }
}

/// What the answer says of a dimension placed: where it goes and how its
/// spacing measures it; with the snap, its spacing, how many of it and the
/// dimension it came from.
fn answer(
    key: &str,
    to: P2,
    placed: &ViewAnnotation,
    seen: &[Seen],
    frame: &Frame<'_>,
    snap: Option<(Snap, u32)>,
) -> Value {
    let mut value = json!({
        "snapped": snap.is_some(),
        "line_offset": placed.line_offset,
        "text_offset": placed.text_offset,
    });
    value[key] = json!(to);
    if let Some(g) = spacing::gauge(placed, seen, frame) {
        value["family"] = json!(g.family.key());
        value["beyond"] = json!(g.value);
    }
    if let Some((snap, from)) = snap {
        value["spacing"] = json!(snap.spacing);
        value["multiple"] = json!(snap.multiple);
        value["from"] = json!(from);
    }
    value
}

/// A new dimension's placing click.
fn snap_new<K: Kernel>(
    doc: &Document<K>,
    c: &AddAnnotationCommand,
    free: bool,
) -> Result<Value, ApiError> {
    let Some(at) = c.at else {
        return Err(ApiError(
            "drawing_snap snaps a placing click: the command has no `at`".to_owned(),
        ));
    };
    let unsnapped = json!({"at": at, "snapped": false});
    let Some(asked) = Asked::of(&c.kind) else {
        return Ok(unsnapped);
    };
    // A dimension whose picks choose its type (mitcad#1272) snaps as the
    // one they choose.
    let snaps = match asked {
        Asked::Type(kind) => spaced(kind),
        Asked::Dimension => true,
    };
    let Some(view_id) = c.view.filter(|_| snaps) else {
        return Ok(unsnapped);
    };
    let drawing = doc.find_drawing(&c.drawing)?;
    let (si, _) = drawing
        .find_view(view_id)
        .ok_or_else(|| ApiError(format!("{} has no view {view_id}", drawing.name)))?;
    let evaluated = doc.evaluate_views(drawing, si);
    let view = doc
        .view_at(drawing, si, &evaluated, view_id)
        .map_err(ApiError)?;
    let (annotation, seen) =
        doc.placing_annotation(&view, c, asked, drawing.next_annotation_id())?;
    let frame = view.frame();
    let mut from = None;
    let snap = if free {
        None
    } else {
        spacing::snap_place(&annotation, &seen, &frame, at, |family| {
            let (step, id) = doc.dimension_spacing(drawing, family, None)?;
            from = Some(id);
            Some(step)
        })
    };
    let to = snap.map_or(at, |s| s.to);
    let mut placed = annotation;
    annotate::placed_at(&mut placed, &seen, &frame, to);
    let mut value = answer("at", to, &placed, &seen, &frame, snap.zip(from));
    made(&mut value, &placed, &seen, &frame);
    Ok(value)
}

/// What a placing click makes, for its preview (mitcad#1272): its `type`
/// and `along`; a linear dimension's `ends` (where its extension lines
/// start, on the sheet) and `direction` (what it measures along, a unit
/// vector on the sheet).
fn made(value: &mut Value, placed: &ViewAnnotation, seen: &[Seen], frame: &Frame<'_>) {
    value["type"] = json!(placed.kind.key());
    if let Some(along) = placed.along {
        value["along"] = json!(along);
    }
    if placed.kind != AnnotationType::Linear || seen.len() < 2 {
        return;
    }
    let Ok((_, d)) = annotate::linear_axes(placed.along, placed.turn, seen) else {
        return;
    };
    let (a, b) = (seen[0].point(), seen[1].point());
    let (sa, sb) = (frame.map.at(a), frame.map.at(b));
    // The view's map scales and moves: its directions stay.
    value["ends"] = json!([sa, sb]);
    value["direction"] = json!(d);
}

/// A placed dimension's drag.
fn snap_moved<K: Kernel>(
    doc: &Document<K>,
    c: &EditAnnotationCommand,
    free: bool,
) -> Result<Value, ApiError> {
    let Some(delta) = c.shift else {
        return Err(ApiError(
            "drawing_snap snaps a drag: the command has no `move`".to_owned(),
        ));
    };
    let unsnapped = json!({"move": delta, "snapped": false});
    let drawing = doc.find_drawing(&c.drawing)?;
    let id = c.annotation;
    // A text note, a line or an arc drawn outside the model: as dragged.
    let Some((si, Some(view_id))) = drawing.find_annotation(id) else {
        return Ok(unsnapped);
    };
    let Some(annotation) = drawing.sheets[si]
        .view(view_id)
        .and_then(|v| v.annotations.iter().find(|a| a.id == id))
        .filter(|a| spaced(a.kind) || led(a.kind))
    else {
        return Ok(unsnapped);
    };
    let evaluated = doc.evaluate_views(drawing, si);
    let Ok(view) = doc.view_at(drawing, si, &evaluated, view_id) else {
        return Ok(unsnapped);
    };
    // A broken one moves as dragged.
    let Ok(seen) = doc.annotation_seen(&view, annotation) else {
        return Ok(unsnapped);
    };
    if led(annotation.kind) {
        // Laid out with its own letters' height, as it is drawn.
        let frame = view.frame_with(drawing.defaults.text(annotation.height));
        let cells = frame_cells(doc, drawing, si, annotation);
        let snap = if free {
            None
        } else {
            annotate::snap_leader(annotation, &seen, &frame, &cells, delta)
        };
        let to = snap.map_or(delta, |s| s.to);
        let mut placed = annotation.clone();
        annotate::moved(&mut placed, &seen, &frame, to);
        let mut value = json!({
            "move": to,
            "snapped": snap.is_some(),
            "text_offset": placed.text_offset,
        });
        if let Some(snap) = snap {
            value["angle"] = json!(snap.angle);
        }
        return Ok(value);
    }
    let frame = view.frame();
    let mut from = None;
    let snap = if free {
        None
    } else {
        spacing::snap_move(annotation, &seen, &frame, delta, |family| {
            let (step, other) = doc.dimension_spacing(drawing, family, Some(id))?;
            from = Some(other);
            Some(step)
        })
    };
    let to = snap.map_or(delta, |s| s.to);
    let mut placed = annotation.clone();
    annotate::moved(&mut placed, &seen, &frame, to);
    Ok(answer("move", to, &placed, &seen, &frame, snap.zip(from)))
}
