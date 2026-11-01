// SPDX-License-Identifier: MIT
//! Local assistant setup plans. Planning never reads or writes configuration files.

use serde_json::{Value, json};
use toml_edit::{Array, DocumentMut, Item, Table, value};

const SERVER: &str = "mitcad-desktop";
const MAX_CONFIG_BYTES: usize = 4 * 1024 * 1024;

fn failure(status: &str, error: &str) -> Value {
    json!({"status": status, "error": error, "backup_required": false})
}

fn absolute_executable(path: &str) -> bool {
    let bytes = path.as_bytes();
    !path.contains('\0')
        && (path.starts_with('/')
            || path.starts_with("\\\\")
            || (bytes.len() > 3
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && matches!(bytes[2], b'/' | b'\\')))
}

/// Adds only the local MitCad entry, preserving other settings and comments.
/// The caller owns path discovery, policy checks, explicit conflict consent,
/// backup creation, concurrent-edit detection and atomic file replacement.
pub fn plan_codex_config(current: &str, bridge: &str, replace_existing: bool) -> Value {
    if current.len() > MAX_CONFIG_BYTES {
        return failure(
            "invalid",
            "The assistant configuration exceeds the supported size.",
        );
    }
    if !absolute_executable(bridge) {
        return failure(
            "invalid",
            "The installed MitCad bridge needs an absolute executable path.",
        );
    }
    let Ok(mut document) = current.parse::<DocumentMut>() else {
        // TOML parser errors may quote existing secrets; never copy them into UI logs.
        return failure(
            "invalid",
            "The assistant configuration is not valid TOML; it was left unchanged.",
        );
    };
    if let Some(servers) = document.get("mcp_servers") {
        let Some(table) = servers.as_table_like() else {
            return failure("invalid", "The mcp_servers setting must be a TOML table.");
        };
        if let Some(existing) = table.get(SERVER) {
            if let Some(entry) = existing.as_table_like() {
                if entry
                    .get("enabled")
                    .is_some_and(|enabled| enabled.as_bool().is_none())
                {
                    return failure(
                        "invalid",
                        "The existing MitCad enabled setting must be true or false.",
                    );
                }
                if entry.get("enabled").and_then(Item::as_bool) == Some(false) {
                    return failure(
                        "conflict",
                        "MitCad is disabled in the assistant settings. Enable it there before setup.",
                    );
                }
                let expected_args =
                    entry
                        .get("args")
                        .and_then(Item::as_array)
                        .is_some_and(|args| {
                            args.len() == 2
                                && args.get(0).and_then(toml_edit::Value::as_str) == Some("mcp")
                                && args.get(1).and_then(toml_edit::Value::as_str)
                                    == Some("--connect")
                        });
                let local = !entry.contains_key("url")
                    && !entry.contains_key("experimental_environment")
                    && !entry.contains_key("transport");
                if local
                    && expected_args
                    && entry.get("command").and_then(Item::as_str) == Some(bridge)
                {
                    return json!({"status": "unchanged", "content": current, "backup_required": false});
                }
            }
            if !replace_existing {
                return failure(
                    "conflict",
                    "The existing MitCad entry uses different settings. Confirm replacement to continue.",
                );
            }
        }
    } else {
        let mut parent = Table::new();
        parent.set_implicit(true);
        document.insert("mcp_servers", Item::Table(parent));
    }

    let mut entry = Table::new();
    if let Some(original) = document
        .get("mcp_servers")
        .and_then(Item::as_table_like)
        .and_then(|servers| servers.get(SERVER))
        .and_then(Item::as_table)
    {
        *entry.decor_mut() = original.decor().clone();
        entry.set_position(original.position());
    }
    entry.insert("command", value(bridge));
    let mut args = Array::new();
    args.push("mcp");
    args.push("--connect");
    entry.insert("args", value(args));
    let parent = document
        .get_mut("mcp_servers")
        .expect("validated or created table");
    let inline = parent.is_inline_table();
    let server = if inline {
        Item::Value(toml_edit::Value::InlineTable(entry.into_inline_table()))
    } else {
        Item::Table(entry)
    };
    parent
        .as_table_like_mut()
        .expect("validated table")
        .insert(SERVER, server);
    let content = document.to_string();
    if content.parse::<DocumentMut>().is_err() {
        return failure(
            "invalid",
            "The assistant configuration could not be updated safely; it was left unchanged.",
        );
    }
    json!({"status": "updated", "content": content, "backup_required": !current.is_empty()})
}

/// Check a supported client's configuration write before reporting setup success.
/// Differences inside its new MitCad entry are allowed; unrelated text is not.
pub fn verify_codex_config(before: &str, after: &str, bridge: &str) -> Value {
    let target = plan_codex_config(after, bridge, false);
    if target["status"] != "unchanged" {
        return json!({"valid": false, "error": "The assistant did not retain the expected local MitCad configuration."});
    }
    let unrelated = |source: &str| -> Option<String> {
        if source.len() > MAX_CONFIG_BYTES {
            return None;
        }
        let mut document = source.parse::<DocumentMut>().ok()?;
        let has_comment = |decor: &toml_edit::Decor| {
            [decor.prefix(), decor.suffix()]
                .into_iter()
                .flatten()
                .filter_map(|text| text.as_str())
                .any(|text| text.contains('#'))
        };
        // An empty pre-existing parent can carry policy notes. Do not discard
        // those while normalizing the container created for a new entry.
        let parent_comments =
            document.as_table().key("mcp_servers").is_some_and(|key| {
                has_comment(key.leaf_decor()) || has_comment(key.dotted_decor())
            }) || document.get("mcp_servers").is_some_and(|parent| {
                parent
                    .as_table()
                    .is_some_and(|table| has_comment(table.decor()))
                    || parent
                        .as_inline_table()
                        .is_some_and(|table| has_comment(table.decor()))
            });
        if let Some(servers) = document.get_mut("mcp_servers") {
            let table = servers.as_table_like_mut()?;
            table.remove(SERVER);
            if table.is_empty() && !parent_comments {
                document.remove("mcp_servers");
            }
        }
        Some(document.to_string().trim().to_owned())
    };
    match (unrelated(before), unrelated(after)) {
        (Some(before), Some(after)) if before == after => json!({"valid": true}),
        _ => {
            json!({"valid": false, "error": "The assistant changed unrelated settings or comments. Restore the saved backup before retrying."})
        }
    }
}

/// Constructs the bundle from a manifest and the application's own launcher.
/// The installed bridge path belongs in the manifest's argument array, never
/// in a shell command. Executable metadata survives standard ZIP extraction.
pub fn mcp_bundle(manifest: &str, launcher: &[u8]) -> Result<Vec<u8>, String> {
    let manifest_value: Value = serde_json::from_str(manifest)
        .map_err(|_| "The assistant bundle manifest is not valid JSON.".to_owned())?;
    let entry = if cfg!(windows) {
        "server/mitcad-mcp-launcher.exe"
    } else {
        "server/mitcad-mcp-launcher"
    };
    if manifest_value["manifest_version"] != "0.3"
        || manifest_value["server"]["type"] != "binary"
        || manifest_value["server"]["entry_point"] != entry
    {
        return Err("The assistant bundle must reference its packaged native launcher.".to_owned());
    }
    let config = &manifest_value["server"]["mcp_config"];
    if config["command"] != format!("${{__dirname}}/{entry}")
        || !config["args"].as_array().is_some_and(|args| {
            args.len() == 3
                && args[0].as_str().is_some_and(absolute_executable)
                && args[1] == "mcp"
                && args[2] == "--connect"
        })
    {
        return Err(
            "The assistant bundle must launch the installed local bridge directly.".to_owned(),
        );
    }
    if launcher.is_empty() || launcher.len() > 32 * 1024 * 1024 {
        return Err(
            "The assistant bundle launcher is missing or exceeds the supported size.".to_owned(),
        );
    }
    let mut writer = mitcad_zip::Writer::new();
    writer
        .add(
            "manifest.json",
            manifest.as_bytes(),
            mitcad_zip::Method::Deflate,
        )
        .map_err(|error| error.to_string())?;
    writer
        .add_executable(entry, launcher, mitcad_zip::Method::Deflate)
        .map_err(|error| error.to_string())?;
    writer
        .add(
            "LICENSE",
            include_bytes!("../../../LICENSE"),
            mitcad_zip::Method::Deflate,
        )
        .map_err(|error| error.to_string())?;
    writer.finish().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_unrelated_settings_comments_and_idempotence() {
        let original = "# personal preferences\nmodel = 'example' # keep this\n[mcp_servers.other]\ncommand = 'other-tool'\nargs = ['--stdio'] # keep arguments\n";
        let plan = plan_codex_config(original, "/Applications/Design Tool/bridge", false);
        assert_eq!(plan["status"], "updated");
        assert_eq!(plan["backup_required"], true);
        let content = plan["content"].as_str().unwrap();
        assert!(content.starts_with(original));
        let parsed = content.parse::<DocumentMut>().unwrap();
        assert_eq!(
            parsed["mcp_servers"][SERVER]["command"].as_str(),
            Some("/Applications/Design Tool/bridge")
        );
        let again = plan_codex_config(content, "/Applications/Design Tool/bridge", false);
        assert_eq!(again["status"], "unchanged");
        assert_eq!(again["content"], content);
        assert_eq!(again["backup_required"], false);
    }

    #[test]
    fn supports_inline_tables_dotted_keys_and_escaped_paths() {
        for original in [
            "mcp_servers = { other = { command = 'other' } } # preserve inline\n",
            "mcp_servers.other.command = 'other' # preserve dotted\n",
            "[mcp_servers]\n# preserve empty parent\n",
            "",
        ] {
            let bridge = "C:\\Program Files\\Design Tool\\bridge.exe";
            let plan = plan_codex_config(original, bridge, false);
            assert_eq!(plan["status"], "updated", "{plan}");
            let content = plan["content"].as_str().unwrap();
            let parsed = content.parse::<DocumentMut>().unwrap();
            assert_eq!(
                parsed["mcp_servers"][SERVER]["command"].as_str(),
                Some(bridge)
            );
            if original.contains("other") {
                assert_eq!(
                    parsed["mcp_servers"]["other"]["command"].as_str(),
                    Some("other")
                );
            }
            assert_eq!(
                plan_codex_config(content, bridge, false)["status"],
                "unchanged"
            );
        }
        let path = "/Applications/Quotes \"and\" spaces/bridge";
        let plan = plan_codex_config("", path, false);
        let parsed = plan["content"]
            .as_str()
            .unwrap()
            .parse::<DocumentMut>()
            .unwrap();
        assert_eq!(
            parsed["mcp_servers"][SERVER]["command"].as_str(),
            Some(path)
        );
    }

    #[test]
    fn refuses_conflicts_until_confirmed_and_never_enables_disabled_entries() {
        let original = "# other entry\n[mcp_servers.other]\ncommand = 'keep'\n[mcp_servers.mitcad-desktop]\nurl = 'https://example.invalid/mcp'\n";
        assert_eq!(
            plan_codex_config(original, "/opt/tools/bridge", false)["status"],
            "conflict"
        );
        let plan = plan_codex_config(original, "/opt/tools/bridge", true);
        assert_eq!(plan["status"], "updated");
        let parsed = plan["content"]
            .as_str()
            .unwrap()
            .parse::<DocumentMut>()
            .unwrap();
        assert!(
            !parsed["mcp_servers"][SERVER]
                .as_table_like()
                .unwrap()
                .contains_key("url")
        );
        assert_eq!(
            parsed["mcp_servers"]["other"]["command"].as_str(),
            Some("keep")
        );
        for replace in [false, true] {
            let disabled = "[mcp_servers.mitcad-desktop]\nenabled = false\ncommand = '/opt/tools/bridge'\nargs = ['mcp', '--connect']\n";
            assert_eq!(
                plan_codex_config(disabled, "/opt/tools/bridge", replace)["status"],
                "conflict"
            );
        }
        let malformed = "mcp_servers.mitcad-desktop = 'old invalid entry'\n";
        assert_eq!(
            plan_codex_config(malformed, "/opt/tools/bridge", false)["status"],
            "conflict"
        );
        assert_eq!(
            plan_codex_config(malformed, "/opt/tools/bridge", true)["status"],
            "updated"
        );
        let invalid_enabled = "[mcp_servers.mitcad-desktop]\nenabled = 'false'\n";
        assert_eq!(
            plan_codex_config(invalid_enabled, "/opt/tools/bridge", true)["status"],
            "invalid"
        );
    }

    #[test]
    fn verifies_client_writes_without_ignoring_unrelated_changes() {
        let before = "# keep this note\nmodel = 'example'\n[mcp_servers.other]\ncommand = 'keep'\n";
        let after = plan_codex_config(before, "/opt/tools/bridge", false)["content"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            verify_codex_config(before, &after, "/opt/tools/bridge")["valid"],
            true
        );
        assert_eq!(
            verify_codex_config(
                before,
                &after.replace("'example'", "'changed'"),
                "/opt/tools/bridge"
            )["valid"],
            false
        );
        assert_eq!(
            verify_codex_config(
                before,
                &after.replace("# keep this note", "# removed original note"),
                "/opt/tools/bridge"
            )["valid"],
            false
        );
        assert_eq!(
            verify_codex_config(
                before,
                &after.replace("'keep'", "'different'"),
                "/opt/tools/bridge"
            )["valid"],
            false
        );
        assert_eq!(
            verify_codex_config(before, &after, "/opt/other/bridge")["valid"],
            false
        );
        let fresh = plan_codex_config("", "/opt/tools/bridge", false)["content"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            verify_codex_config("", &fresh, "/opt/tools/bridge")["valid"],
            true
        );
        let reformatted_target = fresh.replace("\"/opt/tools/bridge\"", "'/opt/tools/bridge'");
        assert_eq!(
            verify_codex_config("", &reformatted_target, "/opt/tools/bridge")["valid"],
            true
        );
        let parent = "model = 'x'\n# keep policy note\n[mcp_servers] # keep parent note\n";
        let added = plan_codex_config(parent, "/opt/tools/bridge", false)["content"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            verify_codex_config(parent, &added, "/opt/tools/bridge")["valid"],
            true
        );
        let lost = added
            .replace("# keep policy note\n", "")
            .replace(" # keep parent note", "");
        assert_eq!(
            verify_codex_config(parent, &lost, "/opt/tools/bridge")["valid"],
            false
        );
    }

    #[test]
    fn invalid_inputs_are_non_mutating_and_errors_do_not_quote_contents() {
        for original in [
            "broken = [",
            "duplicate = 1\nduplicate = 2",
            "mcp_servers = 7",
        ] {
            let plan = plan_codex_config(original, "/opt/tools/bridge", true);
            assert_eq!(plan["status"], "invalid");
            assert!(plan.get("content").is_none());
            assert!(!plan["error"].as_str().unwrap().contains(original));
        }
        assert_eq!(
            plan_codex_config("", "relative/bridge", false)["status"],
            "invalid"
        );
        assert_eq!(
            plan_codex_config("", "/bad\0path", false)["status"],
            "invalid"
        );
    }

    #[test]
    fn bundle_round_trip_contains_manifest_and_executable() {
        let entry = if cfg!(windows) {
            "server/mitcad-mcp-launcher.exe"
        } else {
            "server/mitcad-mcp-launcher"
        };
        let manifest = json!({"manifest_version":"0.3", "server":{"type":"binary", "entry_point":entry,
            "mcp_config":{"command":format!("${{__dirname}}/{entry}"),"args":["/opt/tools/bridge","mcp","--connect"]}}}).to_string();
        let bytes = mcp_bundle(&manifest, b"synthetic native executable").unwrap();
        let archive = mitcad_zip::Archive::new(bytes).unwrap();
        assert_eq!(archive.entries().len(), 3);
        assert_eq!(
            archive.read_by_name("LICENSE").unwrap(),
            include_bytes!("../../../LICENSE")
        );
        assert_eq!(
            archive.read_by_name("manifest.json").unwrap(),
            manifest.as_bytes()
        );
        assert_eq!(
            archive.read_by_name(entry).unwrap(),
            b"synthetic native executable"
        );
        assert!(mcp_bundle("{}", b"launcher").is_err());
        assert!(mcp_bundle(&manifest, b"").is_err());
    }
}
