// SPDX-License-Identifier: MIT
//! `model_render`'s `views` (mitcad#1487): several pictures in one call, a
//! contact sheet such as iso, front and a detail. Each entry is a view (a
//! name, angles or a camera, as `view` takes it) or an object with its own
//! `view`, `projection`, `fit`, `size` and `section`; what it does not give
//! is the call's. Each picture is the one a single call with the merged
//! arguments gives.

use serde_json::{Map, Value};

/// The most pictures in one call.
pub const MAX_VIEWS: usize = 8;

/// The fields a view entry may give for its picture.
pub const VIEW_FIELDS: [&str; 5] = ["view", "projection", "fit", "size", "section"];

/// Each picture's arguments, as a single call takes them (without
/// `views`), when the call has `views`; None without it.
pub fn view_arguments(arguments: &Value) -> Result<Option<Vec<Value>>, String> {
    let Some(object) = arguments.as_object() else {
        return Ok(None);
    };
    let Some(views) = object.get("views") else {
        return Ok(None);
    };
    let entries = views
        .as_array()
        .ok_or("views must be a list of views or view objects")?;
    if entries.is_empty() || entries.len() > MAX_VIEWS {
        return Err(format!("views must hold 1 to {MAX_VIEWS} entries"));
    }
    if object.contains_key("path") {
        return Err(
            "views returns its pictures inline: write a file with path in a call of its own"
                .to_owned(),
        );
    }
    let mut base: Map<String, Value> = object.clone();
    base.remove("views");
    entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let mut merged = base.clone();
            match entry {
                Value::Object(fields) if is_spec(fields) => {
                    for (key, value) in fields {
                        if !VIEW_FIELDS.contains(&key.as_str()) {
                            return Err(format!(
                                "views[{i}] has {key}: a view entry gives only {}",
                                VIEW_FIELDS.join(", ")
                            ));
                        }
                        merged.insert(key.clone(), value.clone());
                    }
                }
                Value::String(_) | Value::Object(_) => {
                    merged.insert("view".to_owned(), entry.clone());
                }
                _ => {
                    return Err(format!(
                        "views[{i}] must be a view (a name, {{azimuth, elevation}} or {{eye, target, up}}) or an object with {}",
                        VIEW_FIELDS.join(", ")
                    ));
                }
            }
            Ok(Value::Object(merged))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

/// An entry with the picture's fields, not a view value itself (a camera
/// or angles).
fn is_spec(fields: &Map<String, Value>) -> bool {
    ![
        "eye",
        "target",
        "up",
        "azimuth",
        "elevation",
        "height",
        "fov",
    ]
    .iter()
    .any(|key| fields.contains_key(*key))
}
