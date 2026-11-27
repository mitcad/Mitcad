// SPDX-License-Identifier: MIT
//! The `curvature_comb` query (mitcad#2520): the curvature combs of sketch
//! curves and edges, with their inflection points, scaled together
//! (`mitcad_model::curvature_comb` makes each comb). Inspect > Curvature
//! Comb draws them over the view, and sketch mode over the sketch it
//! edits (`in_sketch`: sketch curves in their sketch's coordinates).

use serde::Deserialize;
use serde_json::{Value, json};

use super::analysis::{component_placement, picked_edge_curves};
use super::{ApiError, parse};
use mitcad_model::Document;
use mitcad_model::curvature_comb::{
    Comb, DEFAULT_TEETH, MAX_TEETH, MIN_TEETH, auto_scale, curve3_comb, sketch_curve_comb,
};
use mitcad_model::features::FeatureDef;
use mitcad_model::ids::{EntityUid, FeatureUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::sketch::geometry::Curve2;
use mitcad_model::transform::Transform;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CombQuery {
    /// Sketch curves (`{"sketch": "F1", "curve": "c3"}`) and edges (as
    /// `measure` takes them: `{"body", "edge"}`), each with the
    /// `occurrence` that places it when its component is placed more than
    /// once; or a curve's geometry as the `sketch` query gives it
    /// (`{"geometry": {...}}`), combed in those coordinates.
    items: Vec<Value>,
    /// Teeth per curve, equally far apart along it.
    #[serde(default = "default_teeth")]
    teeth: usize,
    /// A tooth's length per curvature (mm per 1/mm); left out, the longest
    /// tooth of all the combs is `size` times a quarter of the diagonal of
    /// the box round their curves.
    #[serde(default)]
    scale: Option<f64>,
    #[serde(default = "default_size")]
    size: f64,
    /// Sketch curves in their sketch's own coordinates (z = 0; sketch
    /// mode), not placed in the design.
    #[serde(default)]
    in_sketch: bool,
}

fn default_teeth() -> usize {
    DEFAULT_TEETH
}

fn default_size() -> f64 {
    1.0
}

/// A sketch curve picked by its sketch and curve id.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SketchCurveItem {
    sketch: String,
    curve: String,
    #[serde(default)]
    occurrence: Option<String>,
}

/// The combs of one item: a curve's, or an edge name's curves'.
struct ItemCombs {
    kind: &'static str,
    combs: Vec<Comb>,
}

pub(super) fn query<K: Kernel>(doc: &Document<K>, query: &CombQuery) -> Result<Value, ApiError> {
    if !(MIN_TEETH..=MAX_TEETH).contains(&query.teeth) {
        return Err(ApiError(format!(
            "teeth must be from {MIN_TEETH} to {MAX_TEETH}, not {}",
            query.teeth
        )));
    }
    if query.scale.is_some_and(|s| !s.is_finite() || s < 0.0)
        || !(query.size.is_finite() && query.size > 0.0)
    {
        return Err(ApiError(
            "scale and size must be positive numbers".to_owned(),
        ));
    }
    let made: Vec<Result<ItemCombs, ApiError>> = query
        .items
        .iter()
        .map(|item| item_combs(doc, item, query.teeth, query.in_sketch))
        .collect();
    let scale_by = query.scale.unwrap_or_else(|| {
        auto_scale(
            made.iter().flatten().flat_map(|item| item.combs.iter()),
            query.size,
        )
    });
    let mut inflections = 0;
    let mut combs = Vec::new();
    for (index, item) in made.into_iter().enumerate() {
        match item {
            Ok(item) => {
                for comb in &item.combs {
                    inflections += comb.inflections.len();
                    let mut value = comb_json(comb, scale_by);
                    value["item"] = json!(index);
                    value["kind"] = json!(item.kind);
                    combs.push(value);
                }
            }
            Err(e) => combs.push(json!({"item": index, "error": e.0})),
        }
    }
    Ok(json!({
        "scale": scale_by,
        "teeth": query.teeth,
        "combs": combs,
        "inflections": inflections,
    }))
}

fn comb_json(comb: &Comb, scale_by: f64) -> Value {
    let teeth: Vec<Value> = comb
        .teeth
        .iter()
        .map(|tooth| {
            json!({"t": tooth.t, "point": tooth.point, "tip": tooth.tip(scale_by),
                   "curvature": tooth.curvature})
        })
        .collect();
    let inflections: Vec<Value> = comb
        .inflections
        .iter()
        .map(|inflection| json!({"t": inflection.t, "point": inflection.point}))
        .collect();
    json!({
        "closed": comb.closed,
        "straight": comb.is_straight(),
        "length": comb.length,
        "max_curvature": comb.max_curvature,
        "min_radius": (comb.max_curvature > 0.0).then(|| 1.0 / comb.max_curvature),
        "teeth": teeth,
        "inflections": inflections,
    })
}

fn item_combs<K: Kernel>(
    doc: &Document<K>,
    item: &Value,
    teeth: usize,
    in_sketch: bool,
) -> Result<ItemCombs, ApiError> {
    if let Some(geometry) = item.get("geometry") {
        // A curve as the `sketch` query gives its geometry, combed where it
        // is (sketch mode's curves while they are dragged: nothing solved
        // again).
        let curve = curve_of(geometry)
            .ok_or_else(|| ApiError(format!("{geometry} is not a sketch curve's geometry")))?;
        return Ok(ItemCombs {
            kind: "geometry",
            combs: vec![sketch_curve_comb(&curve, teeth)],
        });
    }
    if item.get("sketch").is_some() {
        let picked: SketchCurveItem = parse(item.clone(), "a sketch curve")?;
        let comb = sketch_comb(doc, &picked, teeth, in_sketch)?;
        return Ok(ItemCombs {
            kind: "sketch_curve",
            combs: vec![comb],
        });
    }
    let (curves, placed) = picked_edge_curves(doc, item)?;
    if curves.is_empty() {
        return Err(ApiError(format!("{item} has no curve")));
    }
    let combs = curves
        .iter()
        .filter_map(|curve| curve3_comb(curve, teeth))
        .map(|comb| moved(&comb, &placed))
        .collect();
    Ok(ItemCombs {
        kind: "edge",
        combs,
    })
}

/// A sketch curve from its geometry as the `sketch` query writes it
/// (`curve_json` in `sketch.rs`): a line's `start` and `end`, a circle's
/// `center` and `radius` (an arc's also `start_angle` and `end_angle`), an
/// ellipse's `major_radius`, `minor_radius` and `rotation`, a spline's
/// `degree`, `control`, `weights` and `knots`. None for anything else.
fn curve_of(geometry: &Value) -> Option<Curve2> {
    let number = |key: &str| geometry.get(key)?.as_f64();
    let point = |value: &Value| -> Option<[f64; 2]> {
        Some([value.get(0)?.as_f64()?, value.get(1)?.as_f64()?])
    };
    let at = |key: &str| point(geometry.get(key)?);
    if let Some(control) = geometry.get("control").and_then(Value::as_array) {
        let degree = geometry.get("degree")?.as_u64()? as usize;
        let control: Vec<[f64; 2]> = control.iter().map(point).collect::<Option<_>>()?;
        let numbers = |key: &str| -> Option<Vec<f64>> {
            match geometry.get(key) {
                None | Some(Value::Null) => Some(Vec::new()),
                Some(list) => list.as_array()?.iter().map(Value::as_f64).collect(),
            }
        };
        let weights = numbers("weights")?;
        let knots = numbers("knots")?;
        let valid = degree >= 1
            && control.len() > degree
            && knots.len() == control.len() + degree + 1
            && (weights.is_empty() || weights.len() == control.len());
        return valid.then_some(Curve2::Nurbs(mitcad_model::sketch::geometry::Nurbs {
            degree,
            control,
            weights,
            knots,
        }));
    }
    let center = at("center");
    let angles = number("start_angle").zip(number("end_angle"));
    if let (Some(center), Some(major), Some(minor)) =
        (center, number("major_radius"), number("minor_radius"))
    {
        let rotation = number("rotation").unwrap_or(0.0);
        return Some(match angles {
            Some((start, end)) => Curve2::EllipticalArc {
                center,
                major,
                minor,
                rotation,
                start,
                end,
            },
            None => Curve2::Ellipse {
                center,
                major,
                minor,
                rotation,
            },
        });
    }
    if let (Some(center), Some(radius)) = (center, number("radius")) {
        return Some(match angles {
            Some((start, end)) => Curve2::Arc {
                center,
                radius,
                start,
                end,
            },
            None => Curve2::Circle {
                center,
                radius,
                start: 0.0,
            },
        });
    }
    let (a, b) = (at("start")?, at("end")?);
    Some(Curve2::Line { a, b })
}

fn moved(comb: &Comb, placed: &Transform) -> Comb {
    if placed.is_identity() {
        return comb.clone();
    }
    comb.moved(|p| placed.apply_point(p), |v| placed.apply_vector(v))
}

/// A sketch curve's comb: in its sketch's coordinates, or placed in the
/// design by the sketch's frame and its component's placement.
fn sketch_comb<K: Kernel>(
    doc: &Document<K>,
    picked: &SketchCurveItem,
    teeth: usize,
    in_sketch: bool,
) -> Result<Comb, ApiError> {
    let uid: FeatureUid = picked
        .sketch
        .trim()
        .parse()
        .map_err(|e| ApiError(format!("sketch {}: {e}", picked.sketch)))?;
    let id = EntityUid::parse_curve(picked.curve.trim()).map_err(|e| ApiError(e.to_string()))?;
    let entry = doc
        .feature(uid)
        .ok_or_else(|| ApiError(format!("feature {uid} does not exist")))?;
    let curve = solved_curve(doc, uid, id)?;
    let comb = sketch_curve_comb(&curve, teeth);
    if in_sketch {
        return Ok(comb);
    }
    let output = doc
        .sketch_output(uid)
        .ok_or_else(|| ApiError(format!("{} has not been computed", entry.name)))?;
    if output.developed.is_some() {
        return Err(ApiError(format!(
            "{} lies on a curved face: its combs show in the sketch",
            entry.name
        )));
    }
    let frame = output.frame;
    let normal = frame.normal();
    let in_component = comb.moved(
        |p| {
            let q = frame.point([p[0], p[1]]);
            std::array::from_fn(|i| q[i] + p[2] * normal[i])
        },
        |v| {
            std::array::from_fn(|i| {
                v[0] * frame.x_axis[i] + v[1] * frame.y_axis[i] + v[2] * normal[i]
            })
        },
    );
    let placed = component_placement(
        doc,
        entry.component,
        picked.occurrence.as_deref(),
        &entry.name,
    )?;
    Ok(moved(&in_component, &placed))
}

/// A sketch curve as its sketch solves now: the evaluation's solve when it
/// is of the definition and values there are now, else solved here, as the
/// `sketch` query does.
fn solved_curve<K: Kernel>(
    doc: &Document<K>,
    uid: FeatureUid,
    id: EntityUid,
) -> Result<Curve2, ApiError> {
    let entry = doc
        .feature(uid)
        .ok_or_else(|| ApiError(format!("feature {uid} does not exist")))?;
    let FeatureDef::Sketch(def) = &entry.def else {
        return Err(ApiError(format!("{} ({uid}) is not a sketch", entry.name)));
    };
    let solved = match doc.evaluated_solve(uid) {
        Some(solved) => solved,
        None => {
            let params = doc.parameters();
            let followed = doc.followed_sketch(uid, def);
            let mut def = followed.unwrap_or_else(|| def.clone());
            if !def.equations.is_empty() {
                let _ = mitcad_model::sketch::equation::follow_formulas(&mut def, params.table());
            }
            let solved = def
                .solve_with(
                    &mut |p| {
                        params
                            .value(p)
                            .ok_or_else(|| format!("parameter {} does not exist", params.name(p)))
                    },
                    &|p| params.name(p),
                )
                .map_err(|e| ApiError(format!("{} does not solve: {e}", entry.name)))?;
            std::sync::Arc::new(solved)
        }
    };
    solved
        .curves
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError(format!("{} has no curve {}", entry.name, id.curve_name())))
}
