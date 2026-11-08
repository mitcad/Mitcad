// SPDX-License-Identifier: MIT
//! model_render with `quality: "raytrace"` on the headless server
//! (mitcad#1489): the camera and the bodies the fast mode draws
//! (`Document::preview_camera`, mitcad#1486), the picture from the Cycles
//! render worker through `mitcad-cli`'s raytracer
//! (tools/cli/mcp-raytrace.cpp), which makes the same scene and job as
//! `mitcad-cli render`. A missing or failing worker is a refusal with its
//! error code, never a blank image.

use std::time::Instant;

use mitcad_mcp::ToolResult;
use mitcad_model::preview::{Background, RenderArgs};
use serde_json::{Value, json};

use super::{CadBackend, boolean, path_text, require_extension, save_bytes, string};

/// The samples per pixel and the seconds the worker may take when the call
/// does not say; it denoises unless `denoise` is false.
const DEFAULT_SAMPLES: u64 = 64;
const DEFAULT_TIME_LIMIT_S: f64 = 30.0;

/// What a refusal suggests instead.
const FAST_INSTEAD: &str = "Render with quality \"fast\" (the default) instead.";

/// Whether a model_render call asks for the ray-traced picture.
pub(super) fn wanted(arguments: &Value) -> bool {
    arguments.get("quality").and_then(Value::as_str) == Some("raytrace")
}

/// A refusal with its error code and the fast mode suggested; `more`'s
/// other fields (render_too_large's smallest size) go with it.
fn refused(document: &str, code: &str, error: &str, more: Option<&Value>) -> ToolResult {
    let mut value = json!({"document": document, "error": error, "error_code": code,
        "suggestion": FAST_INSTEAD});
    for (key, field) in more.and_then(Value::as_object).into_iter().flatten() {
        if value.get(key).is_none() {
            value[key] = field.clone();
        }
    }
    ToolResult {
        value,
        is_error: true,
    }
}

impl CadBackend {
    pub(super) fn raytrace(&mut self, arguments: &Value) -> Result<ToolResult, String> {
        let started = Instant::now();
        let id = string(arguments, "document")?;
        let args = RenderArgs::from_json(arguments)?;
        // The inspection options, views (mitcad#1487) and the overlay
        // (mitcad#1488) are the fast mode's.
        let mut fast_only = args.inspect.given();
        if args.overlay.is_some() {
            fast_only.push("overlay");
        }
        if arguments.get("views").is_some() {
            fast_only.push("views");
        }
        if !fast_only.is_empty() {
            return Err(format!(
                "{} {} drawn by quality fast only: leave {} out of a ray-traced picture",
                fast_only.join(", "),
                if fast_only.len() == 1 { "is" } else { "are" },
                if fast_only.len() == 1 { "it" } else { "them" }
            ));
        }
        // The PNG also into the workspace, as the fast mode writes it.
        let path = match &args.path {
            Some(path) => {
                self.require_writable()
                    .map_err(|e| format!("{e}: model_render cannot write path"))?;
                let overwrite = boolean(arguments, "overwrite", false)?;
                let path = self.workspace.write_path(path, overwrite).map_err(|e| {
                    e.replace(
                        "document_save requires overwrite",
                        "model_render requires overwrite",
                    )
                })?;
                require_extension(&path, "png")?;
                Some(path)
            }
            None => None,
        };
        let budget = args.max_bytes.unwrap_or(mitcad_mcp::IMAGE_INLINE_BYTES);
        let opened = self.document(id)?;
        // The fast mode's camera and bodies for the same arguments.
        let plan = opened
            .document
            .0
            .preview_camera(&args)
            .map_err(|e| e.to_string())?;
        let camera = plan.camera.to_json();
        let Some(raytracer) = self.raytracer.as_ref() else {
            return Ok(refused(
                id,
                "render_worker_missing",
                "This server has no render worker for ray-traced pictures.",
                None,
            ));
        };
        let instances: Vec<Value> = plan
            .instances
            .iter()
            .map(|i| json!({"body": i.body.to_string(), "occurrence": i.path_name}))
            .collect();
        // Textures and an environment image relative to the design's folder.
        let folder = match opened.path.as_deref().and_then(|p| p.parent()) {
            Some(folder) => Some(path_text(folder)?.to_owned()),
            None => None,
        };
        let background = match args.background {
            Background::Theme => json!("theme"),
            Background::Transparent => json!("transparent"),
            Background::Color(rgb) => json!(rgb),
        };
        let denoise = arguments
            .get("denoise")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let request = json!({
            "camera": camera,
            "size": args.size,
            "instances": instances,
            "samples": arguments.get("samples").and_then(Value::as_u64).unwrap_or(DEFAULT_SAMPLES),
            "time_limit_s": arguments.get("time_limit_s").and_then(Value::as_f64).unwrap_or(DEFAULT_TIME_LIMIT_S),
            "denoise": denoise,
            "background": background,
            "max_bytes": budget,
            "folder": folder,
            "full": path.is_some(),
        });
        let answer = raytracer.render(&opened.document, &request.to_string());
        let mut answer: Value = serde_json::from_str(&answer)
            .map_err(|e| format!("the raytracer's answer is not JSON: {e}"))?;
        if let Some(error) = answer.get("error").and_then(Value::as_str) {
            let code = answer
                .get("error_code")
                .and_then(Value::as_str)
                .unwrap_or("render_worker_failed");
            return Ok(refused(id, code, error, Some(&answer)));
        }
        // At full size into the workspace.
        if let Some(path) = &path {
            let written = answer["full_base64"]
                .as_str()
                .ok_or_else(|| "the raytracer gave no full-size picture".to_owned())
                .and_then(mitcad_model::library::decode_base64)
                .and_then(|png| save_bytes(path, &png).map_err(|e| e.to_string()));
            if let Err(e) = written {
                return Ok(refused(
                    id,
                    "render_write_failed",
                    &format!("the PNG could not be written to {}: {e}", path.display()),
                    None,
                ));
            }
        }
        let mut warnings = plan.warnings;
        if let Some(more) = answer.get("warnings").and_then(Value::as_array) {
            warnings.extend(more.iter().filter_map(Value::as_str).map(str::to_owned));
        }
        // Each body drawn once, in the order drawn.
        let mut bodies: Vec<String> = Vec::new();
        for instance in &plan.instances {
            let body = instance.body.to_string();
            if !bodies.contains(&body) {
                bodies.push(body);
            }
        }
        let total = started.elapsed().as_secs_f64() * 1e3;
        let mut value = json!({
            "document": id,
            "quality": "raytrace",
            "format": "png",
            "mime_type": "image/png",
            "full_width": args.size[0],
            "full_height": args.size[1],
            "max_bytes": budget,
            "camera": camera,
            "bodies": bodies,
            "timing_ms": {"render": answer["render_ms"].clone(), "total": (total * 1e3).round() / 1e3},
            "warnings": warnings,
            "width": answer["width"].clone(),
            "height": answer["height"].clone(),
            "bytes": answer["bytes"].clone(),
            "downscaled_for_bytes": answer["downscaled_for_bytes"].as_bool().unwrap_or(false),
            "samples": answer["samples"].clone(),
            "denoise": denoise,
            "device": answer["device"].clone(),
        });
        if let Some(path) = &path {
            value["path"] = json!(path_text(path)?);
        }
        if let Some(image) = answer.get_mut("image_base64").map(Value::take) {
            value["image_base64"] = image;
        }
        Ok(ToolResult {
            value,
            is_error: false,
        })
    }
}
