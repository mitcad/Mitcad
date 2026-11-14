// SPDX-License-Identifier: MIT
//! Commands and queries of CAM (mitcad#1013, `mitcad_model::cam`,
//! `document/cam.rs`): setups, their operations, the machines with their
//! tool libraries kept with the design, the machine library's files, the
//! toolpaths computed through the crate `mitcad-cam` and the G-code of a
//! setup. See `commands.md`, "CAM: setups, operations and machines".

use std::path::Path;

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use mitcad_model::Document;
use mitcad_model::cam::engine::{self, Units};
use mitcad_model::cam::library::CamLibrary;
use mitcad_model::cam::{CamMachine, CamOperationInput, CamSetupInput, CamToolInput, SetupRef};
use mitcad_model::kernel::Kernel;

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(super) enum CamCommand {
        "cam_setup_add" => SetupAdd {
            /// The setup's fields; every one has a default.
            #[serde(default)]
            setup: Box<CamSetupInput>,
        },
        "cam_setup_edit" => SetupEdit {
            /// The setup's id or name.
            setup: SetupRef,
            changes: Box<CamSetupInput>,
        },
        "cam_setup_delete" => SetupDelete {
            setup: SetupRef,
        },
        "cam_setup_move" => SetupMove {
            setup: SetupRef,
            index: usize,
        },
        "cam_operation_add" => OperationAdd {
            setup: SetupRef,
            /// `kind` and `tool` needed.
            operation: Box<CamOperationInput>,
            /// Its place in the setup's order; last when left out.
            #[serde(default)]
            index: Option<usize>,
        },
        "cam_operation_edit" => OperationEdit {
            operation: u32,
            changes: Box<CamOperationInput>,
        },
        "cam_operation_delete" => OperationDelete {
            operation: u32,
        },
        "cam_operation_move" => OperationMove {
            operation: u32,
            /// Another setup to move it into; its own when left out.
            #[serde(default)]
            setup: Option<SetupRef>,
            index: usize,
        },
        "cam_machine_put" => MachinePut {
            machine: Box<CamMachine>,
        },
        "cam_machine_delete" => MachineDelete {
            machine: String,
        },
        "cam_tool_put" => ToolPut {
            machine: String,
            tool: Box<CamToolInput>,
        },
        "cam_tool_delete" => ToolDelete {
            machine: String,
            number: u32,
        },
        "cam_generate" => Generate {
            /// Every setup when left out.
            #[serde(default)]
            setup: Option<SetupRef>,
        },
        "cam_machine_from_library" => FromLibrary {
            /// `user` or `project`.
            library: String,
            id: String,
            /// The project's folder, for the project's library.
            #[serde(default)]
            project: Option<String>,
            /// The id in the design; the library's when left out.
            #[serde(default)]
            as_id: Option<String>,
        },
        "cam_machine_to_library" => ToLibrary {
            machine: String,
            /// `user` or `project`.
            library: String,
            #[serde(default)]
            project: Option<String>,
            /// The file's id in the library; the machine's when left out.
            #[serde(default)]
            as_id: Option<String>,
        },
    }
}

/// The `cam_operations` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OperationsQuery {
    setup: SetupRef,
}

/// The `cam_toolpaths` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ToolpathsQuery {
    setup: SetupRef,
    /// Only these operations (ids); all when left out.
    #[serde(default)]
    operations: Option<Vec<u32>>,
}

/// The `cam_post` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PostQuery {
    setup: SetupRef,
    /// Another dialect than the setup's.
    #[serde(default)]
    dialect: Option<String>,
    /// `millimetres` or `inches` in place of the setup's.
    #[serde(default)]
    units: Option<Units>,
}

/// The `cam_library` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LibraryQuery {
    /// The project's folder, when the design is in one.
    #[serde(default)]
    project: Option<String>,
}

fn library(text: &str) -> Result<CamLibrary, ApiError> {
    CamLibrary::parse(text)
        .ok_or_else(|| ApiError(format!("“{text}” is no machine library: user or project")))
}

/// Runs a CAM command.
pub(super) fn run_cam_command<K: Kernel>(
    doc: &mut Document<K>,
    command: CamCommand,
) -> Result<Value, ApiError> {
    Ok(match command {
        CamCommand::SetupAdd { setup } => json!({"setup": doc.add_cam_setup(*setup)?}),
        CamCommand::SetupEdit { setup, changes } => {
            doc.edit_cam_setup(&setup, *changes)?;
            json!({})
        }
        CamCommand::SetupDelete { setup } => {
            doc.delete_cam_setup(&setup)?;
            json!({})
        }
        CamCommand::SetupMove { setup, index } => {
            doc.move_cam_setup(&setup, index)?;
            json!({})
        }
        CamCommand::OperationAdd {
            setup,
            operation,
            index,
        } => json!({"operation": doc.add_cam_operation(&setup, *operation, index)?}),
        CamCommand::OperationEdit { operation, changes } => {
            doc.edit_cam_operation(operation, *changes)?;
            json!({})
        }
        CamCommand::OperationDelete { operation } => {
            doc.delete_cam_operation(operation)?;
            json!({})
        }
        CamCommand::OperationMove {
            operation,
            setup,
            index,
        } => {
            doc.move_cam_operation(operation, setup.as_ref(), index)?;
            json!({})
        }
        CamCommand::MachinePut { machine } => {
            let id = machine.id.clone();
            doc.put_cam_machine(*machine)?;
            json!({"machine": id})
        }
        CamCommand::MachineDelete { machine } => {
            doc.delete_cam_machine(&machine)?;
            json!({})
        }
        CamCommand::ToolPut { machine, tool } => {
            json!({"tool": doc.put_cam_tool(&machine, &tool)?})
        }
        CamCommand::ToolDelete { machine, number } => {
            doc.delete_cam_tool(&machine, number)?;
            json!({})
        }
        CamCommand::Generate { setup } => doc.generate_cam(setup.as_ref())?,
        CamCommand::FromLibrary {
            library: from,
            id,
            project,
            as_id,
        } => {
            let id = doc.copy_cam_machine_from_library(
                library(&from)?,
                &id,
                project.as_deref().map(Path::new),
                as_id.as_deref(),
            )?;
            json!({"machine": id})
        }
        CamCommand::ToLibrary {
            machine,
            library: to,
            project,
            as_id,
        } => {
            let (path, version) = doc.save_cam_machine_to_library(
                &machine,
                library(&to)?,
                project.as_deref().map(Path::new),
                as_id.as_deref(),
            )?;
            json!({"path": path.display().to_string(), "version": version})
        }
    })
}

/// The `cam_operations` query.
pub(super) fn operations<K: Kernel>(
    doc: &Document<K>,
    query: &OperationsQuery,
) -> Result<Value, ApiError> {
    Ok(doc.cam_operations_json(&query.setup)?)
}

/// The `cam_toolpaths` query.
pub(super) fn toolpaths<K: Kernel>(
    doc: &Document<K>,
    query: &ToolpathsQuery,
) -> Result<Value, ApiError> {
    Ok(doc.cam_toolpaths_json(&query.setup, query.operations.as_deref())?)
}

/// The `cam_post` query.
pub(super) fn post<K: Kernel>(doc: &Document<K>, query: &PostQuery) -> Result<Value, ApiError> {
    Ok(doc.cam_post_json(&query.setup, query.dialect.as_deref(), query.units)?)
}

/// The `cam_library` query.
pub(super) fn library_json<K: Kernel>(doc: &Document<K>, query: &LibraryQuery) -> Value {
    doc.cam_library_json(query.project.as_deref().map(Path::new))
}

/// The `cam_dialects` query: the dialects MitCad writes, `{id, label,
/// extension}`.
pub(super) fn dialects() -> Value {
    let dialects: Vec<Value> = engine::dialects()
        .iter()
        .map(|d| json!({"id": d.id(), "label": d.label(), "extension": d.extension()}))
        .collect();
    json!({"dialects": dialects})
}

/// The `cam_simulate` query (mitcad#1013, phase 3): a setup's program cut
/// out of its stock on a height map, up to a move or to the end.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SimulateQuery {
    setup: SetupRef,
    /// `{operation (id), move (index in its toolpath)}`; the end when left
    /// out.
    #[serde(default)]
    until: Option<mitcad_model::cam::simulate::SimulatePlace>,
    /// Snapshots after these moves.
    #[serde(default)]
    frames: Vec<mitcad_model::cam::simulate::SimulatePlace>,
    /// The grid's step (mm).
    #[serde(default)]
    resolution: Option<f64>,
    /// The most nodes of the grid.
    #[serde(default)]
    max_nodes: Option<usize>,
    /// Thicknesses up to this count as none (mm).
    #[serde(default)]
    tolerance: Option<f64>,
    /// `heights` (the default) or `remaining`.
    #[serde(default)]
    result: mitcad_model::cam::simulate::SimulateResult,
}

/// The `cam_simulate` query.
pub(super) fn simulate<K: Kernel>(
    doc: &Document<K>,
    query: SimulateQuery,
) -> Result<Value, ApiError> {
    let options = mitcad_model::cam::simulate::SimulateOptions {
        until: query.until,
        frames: query.frames,
        resolution: query.resolution,
        max_nodes: query.max_nodes,
        tolerance: query.tolerance,
        result: query.result,
    };
    Ok(doc.cam_simulate_json(&query.setup, &options)?)
}
