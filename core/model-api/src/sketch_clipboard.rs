// SPDX-License-Identifier: MIT
//! Copy, Cut and Paste of sketch geometry (mitcad#788,
//! `mitcad_model::sketch::clipboard`): the `sketch_fragment` query that
//! copies, and the `sketch.paste` command; see `commands.md`, "Copy and
//! paste of sketch geometry". The same query copies a 3D sketch's geometry
//! (mitcad#1661, `mitcad_model::sketch::spatial_clipboard`), which
//! `sketch3d.paste` takes.

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::sketch::report_json;
use mitcad_model::Document;
use mitcad_model::features::{FeatureDef, SpatialSketchDef};
use mitcad_model::ids::FeatureUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::sketch::clipboard::{Fragment, copy, held};
use mitcad_model::sketch::geometry::Curve2;
use mitcad_model::sketch::modify::Item;
use mitcad_model::sketch::spatial_clipboard::{self, SpatialFragment};

/// The `sketch_fragment` query: the selected items of a sketch as a
/// fragment to paste.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FragmentQuery {
    sketch: FeatureUid,
    /// Points, curves and texts (`p3`, `c4`, `t5`).
    items: Vec<String>,
}

/// A curve as a polyline of sketch points.
fn polyline(curve: &Curve2) -> Value {
    let (t0, t1) = curve.domain();
    json!(curve.sample(t0, t1))
}

/// The `sketch_fragment` answer of a 3D sketch (mitcad#1661): `kind` `3d`,
/// the `base` point (local `[x, y, z]`), how many points and curves
/// (`entities`) and texts it holds, the relations and dimensions it `kept`
/// and `left_out`, the linked reference records it keeps (`references`),
/// and the `fragment` that `sketch3d.paste` takes.
fn spatial_fragment_json<K: Kernel>(
    doc: &Document<K>,
    uid: FeatureUid,
    name: &str,
    def: &SpatialSketchDef,
    items: &[String],
) -> Result<Value, ApiError> {
    let copied = spatial_clipboard::copy(def, doc.parameters(), uid, items);
    let fragment = copied.map_err(ApiError)?;
    Ok(json!({
        "sketch": uid,
        "sketch_name": name,
        "kind": "3d",
        "base": fragment.base,
        "entities": fragment.points.len() + fragment.curves.len(),
        "texts": fragment.texts.len(),
        "kept": fragment.constraints.len() + fragment.dimensions.len(),
        "left_out": fragment.left_out,
        "references": fragment.sources().len(),
        "fragment": fragment,
    }))
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777).
pub(super) trait SketchClipboardOps<K: Kernel> {
    /// The `sketch_fragment` query: `fragment` (what `sketch.paste` takes),
    /// its `base`, the `outline` of its curves and texts as polylines and
    /// its lone `points` (sketch coordinates, for the paste's preview), and
    /// how many constraints and dimensions it `kept` and `left_out`, and
    /// `held`: `[x, y]`, the directions its constraints and dimensions hold
    /// it to the origin and axes in, which a paste cannot move it along.
    fn sketch_fragment_json(&self, query: &FragmentQuery) -> Result<Value, ApiError>;

    /// `sketch.paste`: a fragment into a sketch, its base point at `at`
    /// (where it was when left out), one undo step.
    fn paste_fragment(
        &mut self,
        sketch: FeatureUid,
        fragment: Value,
        at: Option<[f64; 2]>,
    ) -> Result<Value, ApiError>;
}

impl<K: Kernel> SketchClipboardOps<K> for Document<K> {
    fn sketch_fragment_json(&self, query: &FragmentQuery) -> Result<Value, ApiError> {
        let uid = query.sketch;
        let entry = self
            .feature(uid)
            .ok_or_else(|| ApiError(format!("feature {uid} does not exist")))?;
        let FeatureDef::Sketch(def) = &entry.def else {
            // A 3D sketch's geometry with its z, relations and sources
            // (mitcad#1661).
            if let FeatureDef::SpatialSketch(def) = &entry.def {
                return spatial_fragment_json(self, uid, &entry.name, def, &query.items);
            }
            return Err(ApiError(format!("{} ({uid}) is not a sketch", entry.name)));
        };
        // Linked projections where their sources are now.
        let followed = self.followed_sketch(uid, def);
        let def = followed.as_ref().unwrap_or(def);
        let items = query
            .items
            .iter()
            .map(|i| i.parse::<Item>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(ApiError)?;
        let fragment = copy(def, self.parameters(), uid, &items).map_err(ApiError)?;
        // What the paste's preview draws: the curves as the sketch shows
        // them, the texts by their glyph outlines.
        let output = self.sketch_output(uid);
        let mut outline: Vec<Value> = Vec::new();
        let mut points: Vec<Value> = Vec::new();
        let used: std::collections::BTreeSet<_> = fragment
            .entities
            .iter()
            .flat_map(|e| e.kind.points())
            .collect();
        for e in &fragment.entities {
            match &e.kind {
                mitcad_model::sketch::EntityKind::Point { at } => {
                    if !used.contains(&e.id) {
                        points.push(json!(at));
                    }
                }
                _ => {
                    if let Some(curve) = output.and_then(|o| o.solved.curves.get(&e.id)) {
                        outline.push(polyline(curve));
                    }
                }
            }
        }
        if let Some(output) = output {
            for text in output
                .texts
                .iter()
                .filter(|t| fragment.texts.iter().any(|f| f.id == t.id))
            {
                for contour in &text.contours {
                    let mut line: Vec<[f64; 2]> = Vec::new();
                    for part in &contour.parts {
                        let (t0, t1) = part.domain();
                        let mut samples = part.sample(t0, t1);
                        if !line.is_empty() && !samples.is_empty() {
                            samples.remove(0);
                        }
                        line.extend(samples);
                    }
                    outline.push(json!(line));
                }
            }
        }
        let kept = fragment.constraints.len() + fragment.dimensions.len();
        let held = held(&fragment);
        Ok(json!({
            "sketch": uid,
            "sketch_name": entry.name,
            "base": fragment.base,
            "outline": outline,
            "points": points,
            "entities": fragment.entities.len(),
            "texts": fragment.texts.len(),
            "kept": kept,
            "left_out": fragment.left_out,
            "held": held,
            "fragment": fragment,
        }))
    }

    fn paste_fragment(
        &mut self,
        sketch: FeatureUid,
        fragment: Value,
        at: Option<[f64; 2]>,
    ) -> Result<Value, ApiError> {
        // A 3D sketch's copy keeps its z and sources, which a planar sketch
        // has no place for (mitcad#1661).
        if SpatialFragment::is_spatial(&fragment) {
            return Err(ApiError(
                "3D sketch geometry cannot be pasted into a 2D sketch: paste it into a 3D sketch"
                    .to_owned(),
            ));
        }
        let fragment: Fragment =
            serde_json::from_value(fragment).map_err(|e| ApiError(format!("fragment: {e}")))?;
        let base = fragment.base;
        let at = at.unwrap_or(base);
        let by = [at[0] - base[0], at[1] - base[1]];
        let (pasted, report) = self.edit_sketch(sketch, "Paste into {sketch}", |edit| {
            edit.paste(&fragment, by)
        })?;
        let def = match self.feature(sketch).map(|f| &f.def) {
            Some(FeatureDef::Sketch(def)) => Some(def),
            _ => None,
        };
        let mut value = report_json(&report, def);
        value["made"] = json!(pasted.made);
        value["kept"] = json!(pasted.kept);
        value["left_out"] = json!(pasted.left_out);
        let mut warnings: Vec<String> = Vec::new();
        // What the copy left out was said then: only what this paste dropped.
        let dropped = pasted.left_out.saturating_sub(fragment.left_out);
        if dropped > 0 {
            warnings.push(format!(
                "{dropped} constraint(s) and dimension(s) left out: they reference geometry the \
                 copied geometry lacks"
            ));
        }
        if !pasted.values.is_empty() {
            // Counted for a person, the ids for agents (mitcad#1612).
            let counted = match pasted.values.len() {
                1 => "a dimension took its value".to_owned(),
                n => format!("{n} dimensions took their values"),
            };
            warnings.push(
                mitcad_model::notice::FeatureError::new(format!(
                    "{counted}: the parameters their expressions use are not in this design"
                ))
                .with_detail(format!(
                    "{} took their values: the parameters their expressions use are not in this \
                     design",
                    pasted.values.join(", ")
                ))
                .text(),
            );
            value["values"] = json!(pasted.values);
        }
        if !warnings.is_empty() {
            let mut all: Vec<Value> = value["warnings"].as_array().cloned().unwrap_or_default();
            all.extend(warnings.into_iter().map(Value::String));
            value["warnings"] = json!(all);
        }
        Ok(value)
    }
}
