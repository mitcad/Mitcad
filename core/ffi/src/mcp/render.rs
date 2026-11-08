// SPDX-License-Identifier: MIT
//! `model_render`'s fast picture (mitcad#1486), shared by the headless
//! server and the live application (mitcad#1487): one picture, or with
//! `views` several, each with its own camera, the byte budget shared
//! equally among them. The caller checks `path` (the workspace, or the
//! folders the user allows agents); this writes it.

use std::path::Path;
use std::time::Instant;

use mitcad_mcp::ToolResult;
use mitcad_model::preview::{Inline, Preview, RenderArgs, views};
use serde_json::{Value, json};

use super::{path_text, save_bytes};

type Model = mitcad_model::Document<crate::kernel::OcctKernel>;

/// The picture(s) of `document` for a `model_render` call; `path` is the
/// checked file to write the full picture to.
pub(super) fn answer(
    document: &Model,
    id: &str,
    arguments: &Value,
    path: Option<&Path>,
) -> Result<ToolResult, String> {
    // Each quality's own path; the fast rasterizer is the default.
    match arguments.get("quality").and_then(Value::as_str) {
        None | Some("fast") => {}
        Some(other) => return Err(format!("model_render has no quality {other}")),
    }
    let started = Instant::now();
    if let Some(each) = views::view_arguments(arguments)? {
        return several(document, id, &each, started);
    }
    let args = RenderArgs::from_json(arguments)?;
    let budget = args.max_bytes.unwrap_or(mitcad_mcp::IMAGE_INLINE_BYTES);
    let preview = document
        .render_preview(&args, Some(budget))
        .map_err(|e| e.to_string())?;
    if let Some(path) = path
        && let Err(e) = save_bytes(path, &preview.full.png)
    {
        return Ok(ToolResult {
            value: json!({
                "document": id,
                "error": format!("the PNG could not be written to {}: {e}", path.display()),
                "error_code": "render_write_failed",
            }),
            is_error: true,
        });
    }
    let mut value = json!({
        "document": id,
        "quality": "fast",
        "format": "png",
        "mime_type": "image/png",
        "max_bytes": budget,
    });
    picture(&mut value, &args, &preview, started, budget);
    if let Some(path) = path {
        value["path"] = json!(path_text(path)?);
    }
    let is_error = value.get("error").is_some();
    Ok(ToolResult { value, is_error })
}

/// `views`: each picture within its share of the budget.
fn several(
    document: &Model,
    id: &str,
    each: &[Value],
    started: Instant,
) -> Result<ToolResult, String> {
    let all: Vec<RenderArgs> = each
        .iter()
        .enumerate()
        .map(|(i, arguments)| {
            RenderArgs::from_json(arguments).map_err(|e| format!("views[{i}]: {e}"))
        })
        .collect::<Result<_, _>>()?;
    let budget = all
        .first()
        .and_then(|args| args.max_bytes)
        .unwrap_or(mitcad_mcp::IMAGE_INLINE_BYTES);
    let share = budget / all.len() as u64;
    let mut pictures = Vec::with_capacity(all.len());
    for (i, args) in all.iter().enumerate() {
        let one = Instant::now();
        let preview = document
            .render_preview(args, Some(share))
            .map_err(|e| format!("views[{i}]: {e}"))?;
        let mut value = json!({});
        picture(&mut value, args, &preview, one, share);
        if value.get("error").is_some() {
            let mut refused = json!({
                "document": id,
                "view": i,
                "max_bytes": budget,
                "share_bytes": share,
            });
            for (key, field) in value.as_object().into_iter().flatten() {
                if refused.get(key).is_none() && key != "timing_ms" {
                    refused[key] = field.clone();
                }
            }
            refused["error"] = json!(format!(
                "views[{i}]: {} (each of the {} pictures has {share} of the {budget} bytes)",
                value["error"].as_str().unwrap_or_default(),
                all.len()
            ));
            return Ok(ToolResult {
                value: refused,
                is_error: true,
            });
        }
        pictures.push(value);
    }
    let total = started.elapsed().as_secs_f64() * 1e3;
    let bytes: u64 = pictures.iter().filter_map(|p| p["bytes"].as_u64()).sum();
    Ok(ToolResult {
        value: json!({
            "document": id,
            "quality": "fast",
            "format": "png",
            "mime_type": "image/png",
            "max_bytes": budget,
            "share_bytes": share,
            "bytes": bytes,
            "views": pictures,
            "timing_ms": {"total": (total * 1e3).round() / 1e3},
        }),
        is_error: false,
    })
}

/// One picture's fields into `value`: the camera, its size and bytes, the
/// image (`image_base64`), or `render_too_large`'s error.
fn picture(value: &mut Value, args: &RenderArgs, preview: &Preview, started: Instant, budget: u64) {
    let total = started.elapsed().as_secs_f64() * 1e3;
    value["full_width"] = json!(args.size[0]);
    value["full_height"] = json!(args.size[1]);
    value["camera"] = preview.camera.to_json();
    value["bodies"] = json!(
        preview
            .bodies
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    value["timing_ms"] = json!({
        "mesh": preview.mesh_ms,
        "raster": preview.raster_ms,
        "encode": preview.encode_ms,
        "total": (total * 1e3).round() / 1e3,
    });
    value["warnings"] = json!(preview.warnings);
    // What the overlay found (mitcad#1488).
    if let Some(overlay) = &preview.overlay {
        value["overlay"] = overlay.clone();
        value["timing_ms"]["overlay"] = json!(preview.overlay_ms);
    }
    let image = match &preview.inline {
        Inline::Full => &preview.full,
        Inline::Smaller(smaller) => smaller,
        Inline::TooLarge {
            bytes,
            width,
            height,
        } => {
            value["error"] = json!(format!(
                "the picture takes more than max_bytes ({budget}) even at {width} x {height} pixels ({bytes} bytes); allow more bytes, ask for a smaller size, or write it to a file with path"
            ));
            value["error_code"] = json!("render_too_large");
            value["smallest_bytes"] = json!(bytes);
            value["smallest_width"] = json!(width);
            value["smallest_height"] = json!(height);
            return;
        }
    };
    value["width"] = json!(image.width);
    value["height"] = json!(image.height);
    value["bytes"] = json!(image.png.len());
    value["downscaled_for_bytes"] = json!(!matches!(preview.inline, Inline::Full));
    value["triangles"] = json!(image.triangles);
    value["edges"] = json!(image.edges);
    value["image_base64"] = json!(mitcad_model::library::encode_base64(&image.png));
}
