// SPDX-License-Identifier: MIT
//! The `sketch` query's `order` (mitcad#1492): the points, curves,
//! constraints and dimensions in the order they were made, for sketch
//! mode's list of them.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn sketch(doc: &Document<MockKernel>) -> Value {
    let query = json!({"query": "sketch", "uid": "F1"});
    serde_json::from_str(
        &doc.query(&query.to_string())
            .unwrap_or_else(|e| panic!("{query}: {e}")),
    )
    .unwrap()
}

fn ids(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    item.as_str()
                        .unwrap_or_else(|| item["id"].as_str().unwrap())
                })
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn position(order: &[String], id: &str) -> usize {
    order
        .iter()
        .position(|item| item == id)
        .unwrap_or_else(|| panic!("{id} is not in {order:?}"))
}

#[test]
fn items_are_listed_in_the_order_they_were_made() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let rectangle = command(
        &mut d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "two_point",
               "a": [0, 0], "b": [20, 10]}),
    );
    let line = ids(&rectangle["made"])[0].clone();
    let dimension = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "length", "line": line}}),
    );
    let dimension = ids(&dimension["dimensions"])[0].clone();
    let circle = command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center",
               "center": [40, 0], "radius": 5}),
    );
    let s = sketch(&d);
    let order = ids(&s["order"]);

    // Every point, curve, constraint and dimension once.
    let mut all: Vec<String> = ids(&s["entities"]);
    all.extend(ids(&s["constraints"]));
    all.extend(ids(&s["dimensions"]));
    let mut listed = order.clone();
    all.sort();
    listed.sort();
    assert_eq!(listed, all);

    // The rectangle's points, lines and constraints, then the dimension,
    // then the circle and its centre.
    let rectangle_items: Vec<String> = ids(&rectangle["entities"])
        .into_iter()
        .chain(ids(&rectangle["constraints"]))
        .collect();
    assert!(!rectangle_items.is_empty());
    for item in &rectangle_items {
        assert!(
            position(&order, item) < position(&order, &dimension),
            "{item} comes before {dimension} in {order:?}"
        );
    }
    for item in ids(&circle["entities"]) {
        assert!(
            position(&order, &dimension) < position(&order, &item),
            "{dimension} comes before {item} in {order:?}"
        );
    }
}
