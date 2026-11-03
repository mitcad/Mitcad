// SPDX-License-Identifier: MIT
//! Which method runs a JSON command (mitcad#585). The enums of commands
//! and queries are declared with the name of each variant
//! (`named_enum!`), and the families of commands are one table of those
//! names and the method that runs them (`Document::command_families`): a
//! new family is one row there, and a command its enum does not list
//! cannot be sent elsewhere.

use serde_json::Value;

use super::prelude::*;
use super::{ApiError, parse};
use mitcad_model::Document;
use mitcad_model::kernel::Kernel;

/// Declares an internally tagged enum of commands or queries with the
/// name of each variant on the wire written before it:
///
/// ```text
/// named_enum! {
///     #[derive(Debug, Deserialize)]
///     #[serde(tag = "cmd", deny_unknown_fields)]
///     pub(crate) enum ExampleCommand {
///         "do_this" => DoThis { value: f64 },
///         "do_that" => DoThat(Box<That>),
///         "stop" => Stop,
///     }
/// }
/// ```
///
/// Each variant gets `#[serde(rename = "<name>")]`, and the enum's `NAMES`
/// lists the names in the variants' order (the order serde's error for an
/// unknown name lists them in). The enum's other attributes and the
/// variants' own (documentation, the fields and their attributes) are kept
/// as they are; a variant must not have a `rename` of its own.
macro_rules! named_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $wire:literal => $variant:ident
                $({ $($fields:tt)* })?
                $(( $($tuple:tt)* ))?
            ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis enum $name {
            $(
                $(#[$variant_meta])*
                #[serde(rename = $wire)]
                $variant $({ $($fields)* })? $(( $($tuple)* ))?,
            )*
        }

        impl $name {
            /// The variants' names on the wire (the `cmd` or `query` of the
            /// JSON), in the variants' order.
            // The document's own commands and the queries are listed only
            // by the tests.
            #[allow(dead_code)]
            pub(crate) const NAMES: &'static [&'static str] = &[$($wire),*];
        }
    };
}
pub(crate) use named_enum;

/// A family of commands and the method that runs them.
pub(super) struct CommandFamily<K: Kernel> {
    /// Its commands' names (its enum's `NAMES`).
    pub(super) names: &'static [&'static str],
    /// Also every other name with this prefix, so that the family's own
    /// error lists its commands for an unknown one.
    pub(super) prefix: Option<&'static str>,
    /// Reads the command and runs it.
    pub(super) run: fn(&mut Document<K>, Value) -> Result<Value, ApiError>,
}

impl<K: Kernel> CommandFamily<K> {
    /// Whether the family runs the command of this name.
    pub(super) fn runs(&self, cmd: &str) -> bool {
        self.names.contains(&cmd) || self.prefix.is_some_and(|prefix| cmd.starts_with(prefix))
    }
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait DispatchOps<K: Kernel> {
    /// The families of commands besides the document's own (`Command`,
    /// which takes every other name, so that its error lists its commands
    /// for an unknown one). A new family is one row here.
    fn command_families() -> [CommandFamily<K>; 27];
}

impl<K: Kernel> DispatchOps<K> for Document<K> {
    fn command_families() -> [CommandFamily<K>; 27] {
        [
            // Gears: the motion link of a mesh (mitcad#472, `gears.rs`),
            // run as the `add_motion_link` worked out from the gears. It has
            // no enum: `link_gears_json` reads its fields.
            CommandFamily {
                names: &["link_gears"],
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    let link = doc.link_gears_json(&value)?;
                    doc.run_joint_command(parse(link, "command")?)
                },
            },
            // The general sketch (S1, `sketch.rs`).
            CommandFamily {
                names: super::sketch::SketchCommand::NAMES,
                prefix: Some("sketch."),
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    let command = super::sketch::parse_sketch_command(value)?;
                    doc.run_sketch_command(command)
                },
            },
            // Components and occurrences (F6, `components.rs`).
            CommandFamily {
                names: super::components::ComponentCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_component_command(parse(value, "command")?)
                },
            },
            // Joints between occurrences (mitcad#55, `joints.rs`).
            CommandFamily {
                names: super::joints::JointCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_joint_command(parse(value, "command")?)
                },
            },
            // Configuration tables and library parts (mitcad#64,
            // `libraries.rs`).
            CommandFamily {
                names: super::libraries::LibraryCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_library_command(parse(value, "command")?)
                },
            },
            // Move Features into a Component (mitcad#171, `move_features.rs`).
            CommandFamily {
                names: super::move_features::MoveCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_move_command(parse(value, "command")?)
                },
            },
            // Design policy (mitcad#220, `policy.rs`).
            CommandFamily {
                names: super::policy::PolicyCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_policy_command(parse(value, "command")?)
                },
            },
            // The BUILD tab (mitcad#375, `build.rs`).
            CommandFamily {
                names: super::build::BuildCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_build_command(parse(value, "command")?)
                },
            },
            // Motion studies and contact sets (mitcad#454, `motion.rs`).
            CommandFamily {
                names: super::motion::MotionCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_motion_command(parse(value, "command")?)
                },
            },
            // Typed property sets (mitcad#496, `properties.rs`).
            CommandFamily {
                names: super::properties::PropertyCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_property_command(parse(value, "command")?)
                },
            },
            // Reorient Design (mitcad#502, `reorient.rs`).
            CommandFamily {
                names: super::reorient::ReorientCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_reorient_command(parse(value, "command")?)
                },
            },
            // Materials and components' default materials (mitcad#640,
            // `materials.rs`).
            CommandFamily {
                names: super::materials::MaterialCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_material_command(parse(value, "command")?)
                },
            },
            // Print profiles, the print libraries and slicer files
            // (mitcad#640, `print_profiles.rs`).
            CommandFamily {
                names: super::print_profiles::PrintProfileCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_print_profile_command(parse(value, "command")?)
                },
            },
            // Gear pairs and belt drives on two placements (mitcad#656,
            // `drives.rs`).
            CommandFamily {
                names: super::drives::DriveCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_drive_command(parse(value, "command")?)
                },
            },
            // Measurement objects (mitcad#649, `measurements.rs`).
            CommandFamily {
                names: super::measurements::MeasurementCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_measurement_command(parse(value, "command")?)
                },
            },
            // The parameters as CSV (mitcad#666, `parameters_csv.rs`).
            CommandFamily {
                names: super::parameters_csv::ParametersCsvCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_parameters_csv_command(parse(value, "command")?)
                },
            },
            // Purge volumes on filament changes (mitcad#705,
            // `purge_volumes.rs`).
            CommandFamily {
                names: super::purge_volumes::PurgeCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_purge_command(parse(value, "command")?)
                },
            },
            // Drawings kept in the design (mitcad#707, `drawings.rs`).
            CommandFamily {
                names: super::drawings::DrawingCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_drawing_command(parse(value, "command")?)
                },
            },
            // Image references (mitcad#782, `image_references.rs`).
            CommandFamily {
                names: super::image_references::ImageReferenceCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_image_reference_command(parse(value, "command")?)
                },
            },
            // Tables of data that expressions look values up in (mitcad#667,
            // `tables.rs`).
            CommandFamily {
                names: super::tables::TableCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_table_command(parse(value, "command")?)
                },
            },
            // A table as CSV (mitcad#804, `tables_csv.rs`).
            CommandFamily {
                names: super::tables_csv::TableCsvCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_table_csv_command(parse(value, "command")?)
                },
            },
            // Cut sheets (mitcad#853, `cut_sheets.rs`).
            CommandFamily {
                names: super::cut_sheets::CutSheetCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_cut_sheet_command(parse(value, "command")?)
                },
            },
            // A configuration table's rows as CSV (mitcad#1044,
            // `configuration_rows_csv.rs`).
            CommandFamily {
                names: super::configuration_rows_csv::RowsCsvCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_rows_csv_command(parse(value, "command")?)
                },
            },
            // The design's summary (mitcad#1088, `design_properties.rs`).
            CommandFamily {
                names: super::design_properties::DesignPropertiesCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_design_properties_command(parse(value, "command")?)
                },
            },
            // Comments pinned to the model for an AI agent (mitcad#1119,
            // `comments.rs`).
            CommandFamily {
                names: super::comments::CommentCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_comment_command(parse(value, "command")?)
                },
            },
            // Print plates (mitcad#1050, `print_plates.rs`).
            CommandFamily {
                names: super::print_plates::PlateCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    doc.run_plate_command(parse(value, "command")?)
                },
            },
            // Auto Fix of references a warning found again (mitcad#1223,
            // `auto_fix.rs`).
            CommandFamily {
                names: super::auto_fix::AutoFixCommand::NAMES,
                prefix: None,
                run: |doc: &mut Self, value: Value| -> Result<Value, ApiError> {
                    super::auto_fix::run_auto_fix_command(doc, parse(value, "command")?)
                },
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde::Deserialize;
    use serde_json::{Value, json};

    use super::DispatchOps;
    use crate::{Command, DocumentApi};
    use mitcad_model::Document;
    use mitcad_model::testing::MockKernel;

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Inner {
        name: String,
    }

    named_enum! {
        /// Every kind of variant the macro takes.
        #[derive(Debug, Deserialize)]
        #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
        enum Example {
            /// A unit variant, with a name its own would not give.
            "first.one" => First,
            // A struct variant whose fields have attributes of their own.
            "second" => Second {
                value: f64,
                #[serde(default)]
                flag: bool,
            },
            "third_one" => Third(Inner),
        }
    }

    fn example(value: Value) -> Result<Example, String> {
        serde_json::from_value(value).map_err(|e| e.to_string())
    }

    #[test]
    fn named_enum_lists_the_names_in_order() {
        assert_eq!(Example::NAMES, ["first.one", "second", "third_one"]);
    }

    #[test]
    fn named_enum_variants_take_their_names() {
        assert!(matches!(
            example(json!({"cmd": "first.one"})),
            Ok(Example::First)
        ));
        match example(json!({"cmd": "second", "value": 2.5})) {
            Ok(Example::Second { value, flag }) => assert_eq!((value, flag), (2.5, false)),
            other => panic!("{other:?}"),
        }
        match example(json!({"cmd": "third_one", "name": "x"})) {
            Ok(Example::Third(inner)) => assert_eq!(inner.name, "x"),
            other => panic!("{other:?}"),
        }
        // The fields' attributes are kept.
        let error = example(json!({"cmd": "second", "value": 1, "other": 0})).unwrap_err();
        assert!(error.contains("unknown field `other`"), "{error}");
        // The variants' own names (and `rename_all`'s) are not names on the
        // wire, and an unknown name's error lists the names in order.
        for name in ["First", "first", "third"] {
            let error = example(json!({"cmd": name})).unwrap_err();
            assert!(
                error.contains(&format!(
                    "unknown variant `{name}`, expected one of `first.one`, `second`, `third_one`"
                )),
                "{error}"
            );
        }
    }

    /// Every command of every family's enum reaches the family's method,
    /// each of the document's own (`Command`) reaches
    /// `run_document_command`, and no name is in two lists.
    #[test]
    fn every_command_reaches_its_family() {
        let families = Document::<MockKernel>::command_families();
        let family_of = |cmd: &str| families.iter().position(|family| family.runs(cmd));
        let mut all: Vec<&str> = Command::NAMES.to_vec();
        for (index, family) in families.iter().enumerate() {
            assert!(!family.names.is_empty(), "family {index} has no commands");
            for &name in family.names {
                assert_eq!(family_of(name), Some(index), "{name}");
                if let Some(prefix) = family.prefix {
                    assert!(name.starts_with(prefix), "{name} is not {prefix}…");
                }
            }
            all.extend(family.names);
        }
        for &name in Command::NAMES {
            assert_eq!(family_of(name), None, "{name} is taken by a family");
        }
        let unique: BTreeSet<&str> = all.iter().copied().collect();
        assert_eq!(unique.len(), all.len(), "a command name in two lists");
        // Unknown names: the sketch's own error, else the document's.
        assert_eq!(family_of("sketch.no_such_command"), Some(1));
        assert_eq!(family_of("no_such_command"), None);
    }

    /// Every family's command, sent through `Document::command`, is read by
    /// its own enum: with a field no command has, each is refused for that
    /// field (or, `link_gears`, for its missing gear), never as an unknown
    /// command, and nothing runs.
    #[test]
    fn every_family_command_is_read_by_its_own_enum() {
        let mut doc = Document::new(MockKernel::default());
        for family in Document::<MockKernel>::command_families() {
            for &name in family.names {
                let command = json!({"cmd": name, "no_such_field": 0});
                let error = doc
                    .command(&command.to_string())
                    .expect_err(name)
                    .to_string();
                assert!(!error.contains("unknown variant"), "{name}: {error}");
                assert!(
                    error.contains("no_such_field") || error == "link_gears needs a gear",
                    "{name}: {error}"
                );
            }
        }
        // An unknown command is refused as before, with the document's
        // commands; an unknown sketch command with the sketch's.
        let error = doc.command(r#"{"cmd": "fly"}"#).unwrap_err().to_string();
        assert!(
            error.starts_with(
                "invalid command: unknown variant `fly`, expected one of `begin_command`, "
            ),
            "{error}"
        );
        assert!(error.ends_with("`set_print_bed_face`"), "{error}");
        let error = doc
            .command(r#"{"cmd": "sketch.fly"}"#)
            .unwrap_err()
            .to_string();
        assert!(
            error.starts_with(
                "invalid sketch command: unknown variant `sketch.fly`, expected one of \
                 `sketch.create`, "
            ),
            "{error}"
        );
    }
}
