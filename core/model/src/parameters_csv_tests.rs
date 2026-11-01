// SPDX-License-Identifier: MIT
//! The parameters as CSV (mitcad#666) on the mock kernel: Mitcad's table
//! exported and imported again (into the same design: nothing changes;
//! into a new one: the same names, units, expressions, values, comments
//! and favourites), and a synthetic parameter table of the `.f3d`
//! application imported with the right values (unitless counts, `floor`
//! and `round` of length ratios, degrees as `°`, quoted comments with
//! commas, a text parameter imported (mitcad#664), a reserved name renamed, forward
//! references, a circular reference and unknown names taking the values).

use std::collections::BTreeMap;
use std::f64::consts::PI;

use crate::Document;
use crate::document::{CsvAction, CsvImport, CsvLimits, CsvRowOutcome};
use crate::expr::{ParamLimits, Unit};
use crate::parameters_csv::{self, CsvFormat};
use crate::test_support::block;
use crate::testing::MockKernel;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs().max(1.0)
}

/// The outcome of the row whose file name is `name`.
fn row<'a>(import: &'a CsvImport, name: &str) -> &'a CsvRowOutcome {
    import
        .rows
        .iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("no row {name} in {import:#?}"))
}

/// A parameter's value in millimetres or radians.
fn value(doc: &Document<MockKernel>, name: &str) -> f64 {
    let params = doc.parameters();
    params
        .value(
            params
                .find(name)
                .unwrap_or_else(|| panic!("no parameter {name}")),
        )
        .unwrap()
}

/// The rows of Mitcad's table by name: unit, expression, value, comment,
/// favourite, min, max, step, strict (the kind left out).
fn table_by_name(csv: &str) -> BTreeMap<String, [String; 9]> {
    let table = parameters_csv::parse(csv).unwrap();
    assert_eq!(table.format, CsvFormat::Mitcad);
    table
        .rows
        .into_iter()
        .map(|r| {
            (
                r.name,
                [
                    r.unit.unwrap_or_default(),
                    r.expression,
                    r.value.unwrap_or_default(),
                    r.comment.unwrap_or_default(),
                    r.favorite.unwrap_or_default().to_string(),
                    r.min.unwrap_or_default(),
                    r.max.unwrap_or_default(),
                    r.step.unwrap_or_default(),
                    r.strict.unwrap_or_default().to_string(),
                ],
            )
        })
        .collect()
}

#[test]
fn export_and_import_round_trip() {
    let mut b = block();
    let doc = &mut b.doc;
    doc.add_parameter_expression(
        "width",
        "30 mm",
        Some(Unit::MM),
        "Outer width, \"nominal\"; both sides",
    )
    .unwrap();
    doc.add_parameter_expression("count", "4", Some(Unit::NONE), "")
        .unwrap();
    doc.add_parameter_expression("angle", "360 deg / count", Some(Unit::DEG), "Pitch angle")
        .unwrap();
    // A model parameter that uses a later user parameter.
    doc.set_parameter_expression("d1", "width * 2", None)
        .unwrap();
    doc.set_parameter_favorite("width", true).unwrap();
    doc.set_parameter_favorite("d3", true).unwrap();
    // Limits (mitcad#665): width's maximum uses a later parameter, count
    // is strict.
    doc.set_parameter_limits(
        "width",
        ParamLimits {
            min: Some("10".to_owned()),
            max: Some("count * 20 mm".to_owned()),
            step: Some("0.5 mm".to_owned()),
            strict: false,
            ..ParamLimits::default()
        },
        None,
        None,
    )
    .unwrap();
    doc.set_parameter_limits(
        "count",
        ParamLimits {
            min: Some("1".to_owned()),
            max: Some("10".to_owned()),
            step: Some("1".to_owned()),
            strict: true,
            ..ParamLimits::default()
        },
        None,
        None,
    )
    .unwrap();
    let csv = doc.parameters_csv();
    assert!(
        csv.starts_with(
            "name,kind,unit,expression,value,comment,favorite,min,max,step,strict,values\r\n"
        ),
        "{csv}"
    );
    assert!(csv.contains("\r\nd1,model,mm,width * 2,60,"), "{csv}");
    assert!(
        csv.contains(
            "\r\nwidth,user,mm,30 mm,30,\"Outer width, \"\"nominal\"\"; both sides\",true,\
             10,count * 20 mm,0.5 mm,false,\r\n"
        ),
        "{csv}"
    );
    assert!(
        csv.contains("\r\ncount,user,,4,4,,false,1,10,1,true,\r\n"),
        "{csv}"
    );
    assert!(
        csv.contains("\r\nangle,user,deg,360 deg / count,90,Pitch angle,false,,,,false,\r\n"),
        "{csv}"
    );

    // Into the same design: nothing changes, and no undo step.
    let depth = doc.undo_depth();
    let again = doc.import_parameters_csv(&csv).unwrap();
    assert!(!again.changes_anything(), "{again:#?}");
    assert_eq!(again.unchanged, 6);
    assert_eq!(again.summary(), "6 unchanged");
    assert_eq!(doc.undo_depth(), depth);

    // Into a new design: the same parameters, as user parameters, in one
    // undo step.
    let mut other = Document::new(MockKernel::default());
    let done = other.import_parameters_csv(&csv).unwrap();
    assert_eq!(done.format, CsvFormat::Mitcad);
    assert_eq!(done.added, 6, "{done:#?}");
    assert_eq!(done.refused + done.skipped, 0);
    // The model parameters' kind is noted: no feature owns them here.
    assert!(row(&done, "d1").notes[0].contains("user parameter"));
    assert_eq!(other.undo_depth(), 1);
    assert_eq!(other.undo_label(), Some("Import Parameters"));
    assert_eq!(table_by_name(&other.parameters_csv()), table_by_name(&csv));
    for name in ["d1", "d2", "d3", "width", "count", "angle"] {
        assert!(close(value(&other, name), value(doc, name)), "{name}");
    }
    assert!(close(value(&other, "angle"), PI / 2.0));
    assert!(other.is_favorite_parameter("width") && other.is_favorite_parameter("d3"));
    assert!(!other.is_favorite_parameter("d1"));
    // The limits came along; the preview's rows show them.
    let params = other.parameters();
    let count = params.get(params.find("count").unwrap()).unwrap();
    assert!(count.limits().strict);
    assert_eq!(count.limits().max.as_deref(), Some("10"));
    let width = row(&done, "width").limits.clone().unwrap();
    assert_eq!(
        width,
        CsvLimits {
            min: Some("10".to_owned()),
            max: Some("count * 20 mm".to_owned()),
            step: Some("0.5 mm".to_owned()),
            strict: false,
            values: None,
        }
    );
    assert_eq!(row(&done, "angle").limits, None);
    other.undo();
    assert!(other.parameters().is_empty());
}

/// A parameter table as the `.f3d` application exports it.
const PARAMETER_TABLE: &str = "\
Name,Unit,Expression,Value,Comments,Favorite\r
Length,mm,Pitch * Count + 2 * Margin,106.00,\"Total length, both margins\",true\r
Pitch,mm,12 mm,12.00,,false\r
Margin,mm,5 mm,5.00,,false\r
Count,,8,8.00,Number of teeth,false\r
Holes,,floor(Length / Pitch),8.00,,false\r
Rounded,,round(Length / 7 mm),15.00,\"Rounded, not truncated\",false\r
Angle,°,360 deg / Count,45.00,\"Between teeth, in degrees\",false\r
Half,°,Angle / 2,22.50,,false\r
Label,Text,'6qt','6qt',,false\r
mil,mm,0.5 mm,0.50,A unit name in Mitcad,false\r
Uses_mil,mm,mil * 2,1.00,,false\r
Broken,mm,Unknown * 2,7.50,,false\r
Refused,mm,Unknown * 3,,,false\r
Loop_a,mm,Loop_b + 1 mm,3.00,,false\r
Loop_b,mm,Loop_a + 1 mm,4.00,,false\r
";

#[test]
fn the_parameter_table_of_the_f3d_application_imports_with_the_right_values() {
    let mut doc = Document::new(MockKernel::default());
    let preview = doc.preview_parameters_csv(PARAMETER_TABLE).unwrap();
    // The preview changes nothing.
    assert!(doc.parameters().is_empty());
    assert_eq!(doc.undo_depth(), 0);
    assert_eq!(preview.format, CsvFormat::ParameterTable);
    assert_eq!(
        preview.summary(),
        "14 to add, 1 renamed, 1 refused",
        "{preview:#?}"
    );

    let done = doc.import_parameters_csv(PARAMETER_TABLE).unwrap();
    assert_eq!(done, preview);
    assert_eq!(doc.undo_depth(), 1);
    assert_eq!(doc.undo_label(), Some("Import Parameters"));

    // Forward references: Length uses Pitch, Count and Margin of later
    // rows.
    assert!(close(value(&doc, "Length"), 106.0));
    let length = row(&done, "Length");
    assert_eq!(length.action, CsvAction::Add);
    assert_eq!(length.expression, "Pitch * Count + 2 * Margin");
    assert_eq!(length.text.as_deref(), Some("106 mm"));
    assert!(!length.from_value);
    // Unitless counts, floor and round of length ratios.
    let params = doc.parameters();
    let count = params.get(params.find("Count").unwrap()).unwrap();
    assert_eq!(count.unit(), Unit::NONE);
    assert_eq!(count.comment(), "Number of teeth");
    assert!(close(value(&doc, "Holes"), 8.0));
    assert!(close(value(&doc, "Rounded"), 15.0));
    assert_eq!(row(&done, "Rounded").unit, "");
    // Degrees as `°`.
    let angle = params.get(params.find("Angle").unwrap()).unwrap();
    assert_eq!(angle.unit(), Unit::DEG);
    assert!(close(angle.value(), PI / 4.0));
    assert_eq!(angle.comment(), "Between teeth, in degrees");
    assert!(close(value(&doc, "Half"), PI / 8.0));
    // Quoted comments with commas, and the favourite.
    let length = params.get(params.find("Length").unwrap()).unwrap();
    assert_eq!(length.comment(), "Total length, both margins");
    assert!(doc.is_favorite_parameter("Length"));
    assert!(!doc.is_favorite_parameter("Pitch"));
    // The text parameter is a text parameter (mitcad#664).
    let label = row(&done, "Label");
    assert_eq!(label.action, CsvAction::Add);
    assert_eq!(label.unit, "Text");
    assert_eq!(label.expression, "'6qt'");
    assert_eq!(label.text.as_deref(), Some("6qt"));
    assert_eq!(label.value, None);
    let id = params.find("Label").unwrap();
    assert_eq!(params.text(id), Some("6qt"));
    // A unit's name is renamed as the .f3d import renames it, and the
    // expressions that use it follow.
    let mil = row(&done, "mil");
    assert_eq!(mil.renamed.as_deref(), Some("mil_1"));
    assert!(close(value(&doc, "mil_1"), 0.5));
    assert_eq!(row(&done, "Uses_mil").expression, "mil_1 * 2");
    assert!(close(value(&doc, "Uses_mil"), 1.0));
    // An expression that does not evaluate takes the value, with the
    // reason; without a value the row is refused.
    let broken = row(&done, "Broken");
    assert_eq!(broken.action, CsvAction::Add);
    assert!(broken.from_value);
    assert!(broken.reason.as_deref().unwrap().contains("Unknown"));
    assert_eq!(broken.expression, "7.5 mm");
    assert!(close(value(&doc, "Broken"), 7.5));
    let refused = row(&done, "Refused");
    assert_eq!(refused.action, CsvAction::Refuse);
    assert!(refused.reason.as_deref().unwrap().contains("Unknown"));
    assert!(params.find("Refused").is_none());
    // A circular reference takes the values.
    for (name, expected) in [("Loop_a", 3.0), ("Loop_b", 4.0)] {
        let looped = row(&done, name);
        assert!(looped.from_value, "{name}");
        assert!(looped.reason.as_deref().unwrap().contains("circular"));
        assert!(close(value(&doc, name), expected), "{name}");
    }

    // The same table again: nothing changes, the renamed name is found.
    let again = doc.import_parameters_csv(PARAMETER_TABLE).unwrap();
    assert!(!again.changes_anything(), "{again:#?}");
    assert_eq!(row(&again, "mil").action, CsvAction::Unchanged);
    assert_eq!(doc.undo_depth(), 1);

    // The import is one undo step.
    doc.undo();
    assert!(doc.parameters().is_empty());
}

#[test]
fn existing_parameters_change_as_their_rows_say() {
    let mut b = block();
    let doc = &mut b.doc;
    doc.add_parameter_expression("width", "30 mm", Some(Unit::MM), "old")
        .unwrap();
    // No `strict` column: the parameters keep theirs.
    let table = "name,kind,unit,expression,value,comment,favorite,min,max,step\n\
                 d1,model,in,width * 2,,a new comment,true,40,,\n\
                 width,user,mm,25 mm,25,new comment,true,1,100,0.5\n\
                 d3,model,mm,30 deg,,,false,,,\n\
                 clamped,user,mm,150 mm,,,false,,100 mm,\n\
                 bad,user,mm,5 mm,5,,false,20 mm,10 mm,\n";
    let done = doc.import_parameters_csv(table).unwrap();
    assert_eq!(
        done.summary(),
        "1 to add, 2 to change, 2 refused",
        "{done:#?}"
    );
    // A model parameter changes only its expression and limits; its value
    // is the one after the whole import (width changed on a later row).
    let d1 = row(&done, "d1");
    assert_eq!(d1.action, CsvAction::Change);
    assert_eq!(d1.changes, ["expression", "limits"]);
    assert_eq!(d1.unit, "mm");
    assert!(close(d1.value.unwrap(), 50.0));
    assert_eq!(d1.limits.as_ref().unwrap().min.as_deref(), Some("40"));
    assert_eq!(
        d1.notes,
        ["a model parameter keeps its unit mm, its comment, its favorite"]
    );
    assert!(!doc.is_favorite_parameter("d1"));
    assert!(close(value(doc, "d1"), 50.0));
    // A user parameter changes its expression, limits, comment and
    // favourite.
    let width = row(&done, "width");
    assert_eq!(
        width.changes,
        ["expression", "limits", "comment", "favorite"]
    );
    assert!(width.notes.is_empty());
    assert!(doc.is_favorite_parameter("width"));
    let params = doc.parameters();
    let limits = params
        .get(params.find("width").unwrap())
        .unwrap()
        .limits()
        .clone();
    assert_eq!(
        limits,
        ParamLimits {
            min: Some("1".to_owned()),
            max: Some("100".to_owned()),
            step: Some("0.5".to_owned()),
            strict: false,
            ..ParamLimits::default()
        }
    );
    // An angle for a length, without a value: refused, d3 stays.
    let d3 = row(&done, "d3");
    assert_eq!(d3.action, CsvAction::Refuse);
    assert!(d3.reason.is_some());
    assert!(close(value(doc, "d3"), 20.0));
    // A new parameter clamped by its maximum.
    let clamped = row(&done, "clamped");
    assert_eq!(clamped.action, CsvAction::Add);
    assert_eq!(clamped.limited, Some("max"));
    assert!(close(value(doc, "clamped"), 100.0));
    // A wrong limit refuses the row with the reason, said once.
    let bad = row(&done, "bad");
    assert_eq!(bad.action, CsvAction::Refuse);
    assert_eq!(
        bad.reason.as_deref(),
        Some("parameter bad: the minimum 20 mm is above the maximum 10 mm")
    );
    assert_eq!(bad.limits.as_ref().unwrap().max.as_deref(), Some("10 mm"));
    assert!(doc.parameters().find("bad").is_none());
    assert_eq!(doc.undo_label(), Some("Import Parameters"));
}

#[test]
fn text_parameters_go_out_and_come_back() {
    // mitcad#664: a text row in Mitcad's table, imported again.
    let mut doc = Document::new(MockKernel::default());
    doc.add_parameter_expression("GridX", "6", None, "")
        .unwrap();
    doc.add_parameter_expression("Label", "'Box ' & GridX & ', it''s'", None, "Lid text")
        .unwrap();
    let csv = doc.parameters_csv();
    assert!(
        csv.contains(
            "\r\nLabel,user,Text,\"'Box ' & GridX & ', it''s'\",\"Box 6, it's\",Lid text,\
             false,,,,false,\r\n"
        ),
        "{csv}"
    );
    // Into a new design: the same text parameter.
    let mut other = Document::new(MockKernel::default());
    let done = other.import_parameters_csv(&csv).unwrap();
    assert_eq!(done.added, 2, "{done:#?}");
    let label = row(&done, "Label");
    assert_eq!(label.action, CsvAction::Add);
    assert_eq!(label.unit, "Text");
    assert_eq!(label.text.as_deref(), Some("Box 6, it's"));
    assert_eq!(label.value, None);
    assert_eq!(table_by_name(&other.parameters_csv()), table_by_name(&csv));
    // The same table again changes nothing.
    assert!(
        !other
            .import_parameters_csv(&csv)
            .unwrap()
            .changes_anything()
    );
    // A text row whose expression does not evaluate takes its value as a
    // text; the kind alone makes it one.
    let table = "name,kind,unit,expression,value\nNote,text,,Missing & 'x',plain words\n";
    let done = other.import_parameters_csv(table).unwrap();
    let note = row(&done, "Note");
    assert_eq!(note.action, CsvAction::Add, "{done:#?}");
    assert!(note.from_value);
    assert_eq!(note.expression, "'plain words'");
    assert_eq!(note.text.as_deref(), Some("plain words"));
    assert_eq!(note.unit, "Text");
    // A number cannot be a text, nor a text a length.
    let table = "name,unit,expression\nW,mm,Label * 2\nT,Text,GridX\n";
    let done = other.import_parameters_csv(table).unwrap();
    let w = row(&done, "W");
    assert_eq!(w.action, CsvAction::Refuse);
    assert!(
        w.reason
            .as_deref()
            .unwrap()
            .contains("text where a number is expected"),
        "{w:#?}"
    );
    let t = row(&done, "T");
    assert_eq!(t.action, CsvAction::Refuse);
    assert!(
        t.reason
            .as_deref()
            .unwrap()
            .contains("a number where text is expected"),
        "{t:#?}"
    );
}

#[test]
fn a_table_that_is_not_a_parameter_csv_is_refused() {
    let mut doc = Document::new(MockKernel::default());
    let error = doc.import_parameters_csv("a,b\n1,2\n").unwrap_err();
    assert!(error.to_string().contains("no name column"), "{error}");
    let error = doc
        .preview_parameters_csv("Name,Expression\n\"open,1\n")
        .unwrap_err();
    assert_eq!(error.to_string(), "line 2: a quoted field is not closed");
    // Rows without a name, and the second row of a name, are refused.
    let done = doc
        .import_parameters_csv("Name,Expression\n,5 mm\na,5 mm\na,6 mm\n")
        .unwrap();
    assert_eq!(done.summary(), "1 to add, 2 refused");
    assert_eq!(done.rows[2].reason.as_deref(), Some("a is also on line 3"));
    assert!(close(value(&doc, "a"), 5.0));
}
