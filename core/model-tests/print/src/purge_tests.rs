// SPDX-License-Identifier: MIT
//! Purge volumes on filament changes (mitcad#705): the table computed from
//! the materials and the multiplier, the pairs set by hand (kept, marked,
//! reset, saved), a project's table read as those pairs, and what the
//! Bambu family's and PrusaSlicer's files get.

use serde_json::{Map, Value, json};

use crate::print_files_tests::{command, design, design_of, export_to, project};
use mitcad_model::Document;
use mitcad_model::print_files::installed::SlicerFolder;
use mitcad_model::print_keys::Dialect;
use mitcad_model::print_purge::{PurgeValues, colour_defaults, computed};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

/// The `purge_volumes` query of the design's bodies.
fn table(doc: &mut Document<MockKernel>) -> Value {
    query(doc, json!({"query": "purge_volumes"}))
}

/// A query's answer.
fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

/// A slot's purge values as the query gives them.
fn values(slot: &Value) -> PurgeValues {
    PurgeValues {
        into: slot["purge_into"].as_f64().unwrap(),
        out: slot["purge_out"].as_f64().unwrap(),
    }
}

/// A pair's cell: (volume, computed, edited).
fn cell(table: &Value, from: usize, to: usize) -> (f64, f64, bool) {
    let c = &table["volumes"][from][to];
    (
        c["volume"].as_f64().unwrap(),
        c["computed"].as_f64().unwrap(),
        c["edited"].as_bool().unwrap(),
    )
}

/// A command that must be refused.
fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was taken: {result}"),
        Err(e) => e.to_string(),
    }
}

/// The Bambu family's project settings of an export.
fn bambu_settings(data: &[u8]) -> Map<String, Value> {
    let package = mitcad_3mf::read_package(data).unwrap();
    serde_json::from_slice(package.file("Metadata/project_settings.config").unwrap()).unwrap()
}

/// PrusaSlicer's project settings of an export, as text.
fn prusa_settings(data: &[u8]) -> String {
    let package = mitcad_3mf::read_package(data).unwrap();
    std::str::from_utf8(package.file("Metadata/Slic3r_PE.config").unwrap())
        .unwrap()
        .to_owned()
}

#[test]
fn the_table_is_computed_from_the_materials_colours_and_the_multiplier() {
    // A PLA body at the root (a white appearance) and an ABS one (black).
    let mut doc = design();
    let t = table(&mut doc);
    let slots = t["slots"].as_array().unwrap();
    assert_eq!(slots.len(), 2);
    assert_eq!(slots[0]["material"], "pla");
    assert_eq!(slots[1]["material"], "abs");
    assert_eq!(t["multiplier"], json!(1.0));
    let (pla, abs) = (values(&slots[0]), values(&slots[1]));
    // Each colour's defaults.
    let colour = |slot: &Value| {
        mitcad_model::internal::print_files::hex_rgb(slot["color"].as_str().unwrap()).unwrap()
    };
    assert_eq!(pla, colour_defaults(Some(colour(&slots[0]))));
    assert_eq!(abs, colour_defaults(Some(colour(&slots[1]))));
    // Nothing from a filament to itself; dark to light takes more.
    assert_eq!(cell(&t, 0, 0), (0.0, 0.0, false));
    assert_eq!(cell(&t, 0, 1).1, computed(pla, abs, 1.0));
    assert_eq!(cell(&t, 1, 0).1, computed(abs, pla, 1.0));
    assert!(cell(&t, 1, 0).1 > cell(&t, 0, 1).1, "{t}");
    assert_eq!(cell(&t, 0, 1).1, (pla.out + abs.into).round());

    // The process' multiplier (Others) multiplies them.
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Process",
               "key": "flush_multiplier", "value": 1.5}),
    );
    let t = table(&mut doc);
    assert_eq!(t["multiplier"], json!(1.5));
    assert_eq!(cell(&t, 1, 0), {
        let v = computed(abs, pla, 1.5);
        (v, v, false)
    });

    // Of the bodies given, in their order: the ABS one alone is one slot.
    let abs_body = slots[1]["bodies"][0].clone();
    let t = query(
        &doc,
        json!({"query": "purge_volumes", "bodies": [abs_body]}),
    );
    assert_eq!(t["slots"].as_array().unwrap().len(), 1);
    assert_eq!(
        t["volumes"],
        json!([[{"computed": 0.0, "volume": 0.0, "edited": false}]])
    );
}

#[test]
fn a_materials_own_purge_values_win_over_its_colour() {
    let mut doc = design();
    command(
        &mut doc,
        json!({"cmd": "create_material", "id": "dark-petg", "name": "Dark PETG",
               "based_on": "pla", "filament": {"purge_into": 50, "purge_out": 260}}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Arm", "material": "dark-petg"}),
    );
    let t = table(&mut doc);
    assert_eq!(t["slots"][1]["material"], "dark-petg");
    let own = values(&t["slots"][1]);
    assert_eq!(
        own,
        PurgeValues {
            into: 50.0,
            out: 260.0
        }
    );
    let pla = values(&t["slots"][0]);
    assert_eq!(cell(&t, 1, 0).1, (260.0 + pla.into).round());
    assert_eq!(cell(&t, 0, 1).1, (pla.out + 50.0).round());
    // A negative volume is no purge volume.
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_material", "id": "dark-petg", "filament": {"purge_out": -1}}),
    );
    assert!(error.contains("purge_out"), "{error}");
    // A colour's defaults, for the material editor's empty fields.
    assert_eq!(
        query(&doc, json!({"query": "purge_defaults", "color": "#000000"})),
        json!({"purge_into": 70.0, "purge_out": 210.0})
    );
    assert_eq!(
        query(&doc, json!({"query": "purge_defaults"})),
        json!({"purge_into": 140.0, "purge_out": 140.0})
    );
    assert!(
        doc.query(&json!({"query": "purge_defaults", "color": "black"}).to_string())
            .is_err()
    );
}

#[test]
fn a_pair_set_by_hand_is_kept_marked_and_reset() {
    let mut doc = design();
    let before = table(&mut doc);
    let computed_pla_abs = cell(&before, 0, 1).1;
    command(
        &mut doc,
        json!({"cmd": "set_purge_volumes",
               "volumes": [{"from": "pla", "to": "abs", "volume": 333}]}),
    );
    assert!(
        doc.undo_label()
            .unwrap()
            .starts_with("Set Purge Volume PLA"),
        "{:?}",
        doc.undo_label()
    );
    let t = table(&mut doc);
    assert_eq!(cell(&t, 0, 1), (333.0, computed_pla_abs, true));
    // The other way is still computed.
    assert!(!cell(&t, 1, 0).2);
    // The multiplier does not change it; the computed one follows.
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Process",
               "key": "flush_multiplier", "value": 2}),
    );
    let t = table(&mut doc);
    assert_eq!(cell(&t, 0, 1).0, 333.0);
    assert_eq!(cell(&t, 0, 1).1, (computed_pla_abs * 2.0).round());

    // Saved with the design and read back.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["purge_volumes"],
        json!([{"from": "pla", "to": "abs", "volume": 333.0}])
    );
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.purge_overrides(), doc.purge_overrides());
    assert_eq!(loaded.to_json(), saved);

    // Reset: computed again, one undo step; undo brings it back.
    command(
        &mut doc,
        json!({"cmd": "set_purge_volumes", "volumes": [{"from": "pla", "to": "abs"}]}),
    );
    assert!(doc.undo_label().unwrap().starts_with("Reset Purge Volume"));
    let t = table(&mut doc);
    assert!(!cell(&t, 0, 1).2);
    assert_eq!(cell(&t, 0, 1).0, cell(&t, 0, 1).1);
    assert!(doc.purge_overrides().is_empty());
    assert!(!doc.to_json().contains("\"purge_volumes\""));
    doc.undo();
    assert_eq!(cell(&table(&mut doc), 0, 1).0, 333.0);

    // Refused: an unknown material, a material to itself, a negative volume.
    for (volumes, expected) in [
        (
            json!([{"from": "pla", "to": "nothing", "volume": 1}]),
            "unknown material",
        ),
        (json!([{"from": "pla", "to": "pla", "volume": 1}]), "itself"),
        (
            json!([{"from": "pla", "to": "abs", "volume": -1}]),
            "0 mm³ or more",
        ),
        (json!([]), "pairs"),
    ] {
        let error = refused(
            &mut doc,
            json!({"cmd": "set_purge_volumes", "volumes": volumes}),
        );
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn a_deleted_material_s_pairs_do_not_reach_a_new_one_of_its_id() {
    // mitcad#758: a new material takes the first free `custom{n}`, the id
    // of a material deleted before it.
    let mut doc = design();
    let make = json!({"cmd": "create_material", "name": "White PETG",
                      "filament": {"filament_type": "PETG"}});
    assert_eq!(command(&mut doc, make)["id"], "custom1");
    let arm = json!({"cmd": "set_component_material", "component": "Arm",
                     "material": "custom1"});
    command(&mut doc, arm.clone());
    command(
        &mut doc,
        json!({"cmd": "set_purge_volumes",
               "volumes": [{"from": "custom1", "to": "pla", "volume": 50}]}),
    );
    let t = table(&mut doc);
    assert_eq!(t["slots"][1]["material"], "custom1");
    assert_eq!(cell(&t, 1, 0).0, 50.0);
    assert!(cell(&t, 1, 0).2);

    // Deleted: its pair goes in the same step.
    command(&mut doc, json!({"cmd": "delete_material", "id": "custom1"}));
    assert!(doc.purge_overrides().is_empty());
    // A new material gets custom1 again: its cells are computed.
    let make = json!({"cmd": "create_material", "name": "Black TPU",
                      "filament": {"filament_type": "TPU"}});
    assert_eq!(command(&mut doc, make)["id"], "custom1");
    command(&mut doc, arm);
    let t = table(&mut doc);
    assert_eq!(t["slots"][1]["material"], "custom1");
    let (volume, worked_out, edited) = cell(&t, 1, 0);
    assert!(!edited, "{t}");
    assert_eq!(volume, worked_out);
    assert!(!doc.to_json().contains("\"purge_volumes\""));

    // Undoing the delete brings the pair back with the material.
    doc.undo();
    doc.undo();
    doc.undo();
    assert_eq!(doc.purge_overrides().len(), 1);
    assert_eq!(doc.purge_overrides()[0].from, "custom1");
    assert_eq!(doc.purge_overrides()[0].volume, 50.0);
    let t = table(&mut doc);
    assert_eq!(t["slots"][1]["material"], "custom1");
    assert_eq!(cell(&t, 1, 0).0, 50.0);
    assert!(cell(&t, 1, 0).2);
}

#[test]
fn exports_write_the_table_with_the_pairs_set_by_hand() {
    let mut doc = design();
    command(
        &mut doc,
        json!([
            {"cmd": "set_property", "owner": "C0", "set": "Print Process",
             "key": "flush_multiplier", "value": 2},
            {"cmd": "set_purge_volumes",
             "volumes": [{"from": "pla", "to": "abs", "volume": 300}]},
        ]),
    );
    let t = table(&mut doc);
    let (pla, abs) = (values(&t["slots"][0]), values(&t["slots"][1]));
    let back = cell(&t, 1, 0).0;
    assert_eq!(back, computed(abs, pla, 2.0));
    let text = |x: f64| mitcad_model::print_keys::number_text(x);

    // The Bambu family: the table divided by the multiplier, which the
    // slicer applies; each filament's values (out, in), unmultiplied.
    for dialect in ["bambu_studio", "orcaslicer", "qidi_studio"] {
        let (_, data) = export_to(&mut doc, &format!("purge-{dialect}.3mf"), dialect);
        let settings = bambu_settings(&data);
        assert_eq!(
            settings["flush_volumes_matrix"],
            json!(["0", "150", text(back / 2.0), "0"]),
            "{dialect}"
        );
        assert_eq!(settings["flush_multiplier"], json!(["2"]), "{dialect}");
        assert_eq!(
            settings["flush_volumes_vector"],
            json!([text(pla.out), text(pla.into), text(abs.out), text(abs.into)]),
            "{dialect}"
        );
    }

    // PrusaSlicer has no multiplier: the table as it purges, its own table
    // switched on, the filaments' values multiplied.
    let (_, data) = export_to(&mut doc, "purge-prusa.3mf", "prusaslicer");
    let config = prusa_settings(&data);
    let line = |key: &str| {
        config
            .lines()
            .find_map(|l| l.strip_prefix(&format!("; {key} = ")))
            .unwrap_or_else(|| panic!("no {key}: {config}"))
            .to_owned()
    };
    assert_eq!(
        line("wiping_volumes_matrix"),
        format!("0,300,{},0", text(back))
    );
    assert_eq!(line("wiping_volumes_use_custom_matrix"), "1");
    // Its table counts only for one nozzle with several filaments.
    assert_eq!(line("single_extruder_multi_material"), "1");
    assert_eq!(
        line("wiping_volumes_extruders"),
        [pla.out, pla.into, abs.out, abs.into]
            .map(|v| text(v * 2.0))
            .join(",")
    );
    assert!(!config.contains("flush_multiplier"), "{config}");
}

#[test]
fn a_bambu_projects_table_becomes_the_pairs_of_its_materials() {
    let settings = json!({
        "printer_settings_id": "Bench Mini 0.4 nozzle",
        "printable_area": ["0x0", "180x0", "180x180", "0x180"],
        "nozzle_diameter": ["0.4"],
        "print_settings_id": "Standard",
        "layer_height": "0.2",
        "filament_settings_id": ["Generic PLA", "Generic PETG"],
        "filament_type": ["PLA", "PETG"],
        "filament_colour": ["#FFFFFF", "#000000"],
        "flush_multiplier": ["2"],
        "flush_volumes_matrix": ["0", "100", "150", "0"],
        "flush_volumes_vector": ["140", "140", "140", "140"],
    });
    let data = project(
        "BambuStudio-02.00.00.00",
        vec![(
            "Metadata/project_settings.config",
            settings.to_string().into_bytes(),
        )],
        Vec::new(),
    );
    // The import keeps none of the table's keys as the slicer's text.
    let imported = mitcad_model::print_files::read_file("table.3mf", &data, None).unwrap();
    assert_eq!(imported.purge, vec![(0, 1, 200.0), (1, 0, 300.0)]);
    for key in ["flush_volumes_matrix", "flush_volumes_vector"] {
        assert!(imported.report.mapped.contains(key), "{key}");
        assert!(!imported.report.kept.contains(key), "{key}");
    }
    assert_eq!(imported.processes[0].values["flush_multiplier"], json!(2.0));

    let mut doc = design_of("table.3mf", &data, &[0, 1]);
    let t = table(&mut doc);
    assert_eq!(t["multiplier"], json!(2.0));
    // As the slicer purges them (the table times its multiplier), set by
    // hand.
    assert_eq!(cell(&t, 0, 1).0, 200.0);
    assert_eq!(cell(&t, 1, 0).0, 300.0);
    assert!(cell(&t, 0, 1).2 && cell(&t, 1, 0).2);
    // Written back as the project had them.
    let (_, back) = export_to(&mut doc, "table-back.3mf", "bambu_studio");
    let written = bambu_settings(&back);
    assert_eq!(
        written["flush_volumes_matrix"],
        json!(["0", "100", "150", "0"])
    );
    assert_eq!(written["flush_multiplier"], json!(["2"]));
}

#[test]
fn a_prusaslicer_projects_table_becomes_the_pairs_of_its_materials() {
    let config = "printer_settings_id = Bench MK\n\
        bed_shape = 0x0,250x0,250x210,0x210\n\
        nozzle_diameter = 0.4,0.4\n\
        print_settings_id = Standard\n\
        layer_height = 0.2\n\
        filament_settings_id = \"Generic PLA\";\"Generic PETG\"\n\
        filament_type = PLA;PETG\n\
        filament_colour = #FF0000;#00FF00\n\
        wiping_volumes_use_custom_matrix = 1\n\
        wiping_volumes_matrix = 0,150,250,0\n\
        wiping_volumes_extruders = 70,70,70,70\n";
    let mut doc = design_of("table.ini", config.as_bytes(), &[0, 1]);
    let t = table(&mut doc);
    assert_eq!(cell(&t, 0, 1), (150.0, cell(&t, 0, 1).1, true));
    assert_eq!(cell(&t, 1, 0), (250.0, cell(&t, 1, 0).1, true));
    let (_, back) = export_to(&mut doc, "table-prusa.3mf", "prusaslicer");
    assert!(
        prusa_settings(&back).contains("; wiping_volumes_matrix = 0,150,250,0\n"),
        "{}",
        prusa_settings(&back)
    );

    // A project that does not use its own table: nothing set by hand.
    let unused = config.replace(
        "wiping_volumes_use_custom_matrix = 1",
        "wiping_volumes_use_custom_matrix = 0",
    );
    let imported =
        mitcad_model::print_files::read_file("unused.ini", unused.as_bytes(), None).unwrap();
    assert!(imported.purge.is_empty());
    // A table of another size is left out, and said so.
    let odd = config.replace("0,150,250,0", "0,150,250");
    let imported = mitcad_model::print_files::read_file("odd.ini", odd.as_bytes(), None).unwrap();
    assert!(imported.purge.is_empty());
    assert!(
        imported
            .report
            .left_out
            .iter()
            .any(|l| l.contains("purge volumes")),
        "{:?}",
        imported.report.left_out
    );
}

/// The `flush_volumes_matrix` and `flush_multiplier` of a Bambu-family
/// export, with the number of tables the matrix holds (each the same).
fn tables_and_multiplier(written: &Map<String, Value>, slots: usize) -> (usize, Value) {
    let matrix = written["flush_volumes_matrix"].as_array().unwrap();
    let table = slots * slots;
    assert_eq!(matrix.len() % table, 0, "{matrix:?}");
    for other in matrix.chunks(table) {
        assert_eq!(other, &matrix[..table], "{matrix:?}");
    }
    (matrix.len() / table, written["flush_multiplier"].clone())
}

#[test]
fn the_bambu_multiplier_has_an_entry_per_extruder() {
    // mitcad#2207: a printer imported from the slicer with two extruders
    // (two nozzle diameters) gets the multiplier as a list of two, as many
    // as the purge tables, the multiplier set in each entry; Bambu Studio
    // 2.9 crashes slicing such a project with one text.
    let settings = json!({
        "printer_settings_id": "Bench Duo 0.4 nozzle",
        "printable_area": ["0x0", "180x0", "180x180", "0x180"],
        "nozzle_diameter": ["0.4", "0.4"],
        "print_settings_id": "Standard",
        "layer_height": "0.2",
        "filament_settings_id": ["Generic PLA", "Generic PETG"],
        "filament_type": ["PLA", "PETG"],
        "filament_colour": ["#FFFFFF", "#000000"],
        "flush_multiplier": ["1", "1"],
    });
    let data = project(
        "BambuStudio-02.00.00.00",
        vec![(
            "Metadata/project_settings.config",
            settings.to_string().into_bytes(),
        )],
        Vec::new(),
    );
    let mut doc = design_of("duo.3mf", &data, &[0, 1]);
    // No slicer's presets: the printer's own nozzles (its block kept for
    // Bambu Studio) count.
    doc.set_slicer_folders(Some(Vec::new()));
    let (_, back) = export_to(&mut doc, "duo.3mf", "bambu_studio");
    let written = bambu_settings(&back);
    assert_eq!(written["nozzle_diameter"].as_array().map(Vec::len), Some(2));
    assert_eq!(tables_and_multiplier(&written, 2), (2, json!(["1", "1"])));
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Process",
               "key": "flush_multiplier", "value": 2}),
    );
    let (_, back) = export_to(&mut doc, "duo-set.3mf", "bambu_studio");
    assert_eq!(
        tables_and_multiplier(&bambu_settings(&back), 2),
        (2, json!(["2", "2"]))
    );

    // A printer of one nozzle: one entry.
    let mut doc = design();
    doc.set_slicer_folders(Some(Vec::new()));
    let (_, back) = export_to(&mut doc, "solo.3mf", "bambu_studio");
    let written = bambu_settings(&back);
    assert_eq!(written["nozzle_diameter"], json!(["0.4"]));
    assert_eq!(tables_and_multiplier(&written, 2), (1, json!(["1"])));
}

#[test]
fn a_printer_named_by_a_two_extruder_preset_gets_an_entry_per_extruder() {
    // mitcad#2207: the design's printer names only the slicer's preset
    // (`printer_settings_id` "Bench 0.4 nozzle") and keeps one nozzle
    // diameter; the slicer loads the preset's two extruders. A fixture
    // data folder of Bambu Studio has that preset, its nozzles from the
    // base it inherits.
    let folder = mitcad_model::testing::unique_temp_dir("purge-preset").join("BambuStudio");
    let machine = folder.join("system").join("Bench").join("machine");
    std::fs::create_dir_all(&machine).unwrap();
    let write = |file: &str, preset: Value| {
        std::fs::write(machine.join(file), preset.to_string()).unwrap();
    };
    write(
        "fdm_bench_common.json",
        json!({"type": "machine", "name": "fdm_bench_common", "instantiation": "false",
               "nozzle_diameter": ["0.4", "0.4"], "printer_model": "Bench Duo"}),
    );
    write(
        "Bench 0.4 nozzle.json",
        json!({"type": "machine", "name": "Bench 0.4 nozzle", "inherits": "fdm_bench_common"}),
    );
    let mut doc = design();

    // The preset not found: one entry, as the printer's one nozzle says.
    doc.set_slicer_folders(Some(Vec::new()));
    let (_, back) = export_to(&mut doc, "preset-none.3mf", "bambu_studio");
    assert_eq!(
        tables_and_multiplier(&bambu_settings(&back), 2),
        (1, json!(["1"]))
    );

    // Found: two tables and two entries; the design's nozzle stays its
    // own.
    doc.set_slicer_folders(Some(vec![SlicerFolder {
        dialect: Dialect::BambuStudio,
        folder: folder.clone(),
        flatpak: false,
    }]));
    let (_, back) = export_to(&mut doc, "preset-duo.3mf", "bambu_studio");
    let written = bambu_settings(&back);
    assert_eq!(written["printer_settings_id"], "Bench 0.4 nozzle");
    assert_eq!(written["nozzle_diameter"], json!(["0.4"]));
    assert_eq!(tables_and_multiplier(&written, 2), (2, json!(["1", "1"])));

    // Another slicer's project: only its own data folder counts.
    let (_, back) = export_to(&mut doc, "preset-qidi.3mf", "qidi_studio");
    assert_eq!(
        tables_and_multiplier(&bambu_settings(&back), 2),
        (1, json!(["1"]))
    );
}
