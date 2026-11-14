// SPDX-License-Identifier: MIT
//! Face and edge names of sheet bodies as semantic picks (mitcad#210).

use super::*;

fn uid(text: &str) -> crate::ids::FeatureUid {
    text.parse().unwrap()
}

#[test]
fn face_names_name_their_domain_and_wall() {
    let panel = |f: &str, slot| {
        SheetDomainRef::Panel(PanelId {
            feature: uid(f),
            slot,
        })
    };
    let bend = |f: &str, slot| {
        SheetDomainRef::Bend(BendId {
            feature: uid(f),
            slot,
        })
    };
    assert_eq!(
        parse_face("F3:sheet_front(0)"),
        Some(PickedFace::Surface(panel("F3", 0)))
    );
    assert_eq!(
        parse_face("F4:sheet_back(1)#2"),
        Some(PickedFace::Surface(panel("F4", 1)))
    );
    assert_eq!(
        parse_face("F4:sheet_bend_front(0)"),
        Some(PickedFace::Surface(bend("F4", 0)))
    );
    assert_eq!(
        parse_face("F3:sheet_side0(F2:sheet_curve(c2[c1,c3]))"),
        Some(PickedFace::Wall(
            panel("F3", 0),
            SheetCurveId::Sketch {
                sketch: uid("F2"),
                segment: "c2[c1,c3]".parse().unwrap()
            }
        ))
    );
    assert_eq!(
        parse_face("F4:sheet_side12(F4:sheet_curve(7))#0"),
        Some(PickedFace::Wall(
            panel("F4", 12),
            SheetCurveId::Generated {
                feature: uid("F4"),
                slot: 7
            }
        ))
    );
    assert_eq!(
        parse_face("F4:sheet_bend_side0(F4:sheet_curve(3))"),
        Some(PickedFace::Wall(
            bend("F4", 0),
            SheetCurveId::Generated {
                feature: uid("F4"),
                slot: 3
            }
        ))
    );
    // Faces of other features are not sheet picks.
    assert_eq!(parse_face("F2:end(r{c1[c4,c2],c2[c1,c3]})"), None);
    assert_eq!(parse_face("F2:side(c1[c4,c2])"), None);
    assert_eq!(parse_face("F3:sheet_side0(c2)"), None);
    // A mirror's or pattern's copy of a sheet body keeps its domains
    // (mitcad#1450): the copies' wrappers name the same domain and wall.
    assert_eq!(
        parse_face("F6:inst1(F3:sheet_back(0))"),
        Some(PickedFace::Surface(panel("F3", 0)))
    );
    assert_eq!(
        parse_face("F9:inst2(F6:inst1(F4:sheet_bend_front(0)))#1"),
        Some(PickedFace::Surface(bend("F4", 0)))
    );
    assert_eq!(
        parse_face("F6:inst1(F3:sheet_side0(F2:sheet_curve(c2[c1,c3]))#2)"),
        Some(PickedFace::Wall(
            panel("F3", 0),
            SheetCurveId::Sketch {
                sketch: uid("F2"),
                segment: "c2[c1,c3]".parse().unwrap()
            }
        ))
    );
    assert_eq!(parse_face("F6:inst1(F2:side(c1[c4,c2]))"), None);
}

#[test]
fn edge_names_split_at_their_own_bar() {
    assert_eq!(
        edge_faces("E{F3:sheet_front(0)|F3:sheet_side0(F2:sheet_curve(c2[c1,c3]))}#1"),
        Some((
            "F3:sheet_front(0)".to_owned(),
            "F3:sheet_side0(F2:sheet_curve(c2[c1,c3]))".to_owned()
        ))
    );
    // A bar inside a key (a nested edge name) is not the edge's.
    assert_eq!(
        edge_faces("E{F5:fillet(E{F2:a|F2:b})|F2:c}"),
        Some(("F5:fillet(E{F2:a|F2:b})".to_owned(), "F2:c".to_owned()))
    );
    assert_eq!(edge_faces("F3:sheet_front(0)"), None);
}

/// The panel of [`panel_beside_bend`].
fn beside_panel() -> PanelId {
    PanelId {
        feature: uid("F3"),
        slot: 0,
    }
}

/// A side of the panel of [`panel_beside_bend`].
fn beside_curve(slot: u32) -> SheetCurveId {
    SheetCurveId::Generated {
        feature: uid("F3"),
        slot,
    }
}

/// A 10 x 10 panel of F3 whose right side (curve 2) is its tangent line
/// to the bend of F4 (mitcad#1765); its sides 1 (bottom), 2, 3 and 4
/// (left) run counter-clockwise.
fn panel_beside_bend() -> SheetState {
    let panel = beside_panel();
    let bend = BendId {
        feature: uid("F4"),
        slot: 0,
    };
    let curve = beside_curve;
    let corners = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
    let region = MaterialRegion::from_legacy(&SheetRegion {
        loops: vec![SheetLoop {
            segments: (0..4)
                .map(|i| SheetSegment {
                    id: curve(i as u32 + 1),
                    geometry: crate::profile::SegmentGeometry::Line {
                        start: corners[i],
                        end: corners[(i + 1) % 4],
                    },
                })
                .collect(),
        }],
    })
    .unwrap();
    let side = |domain, piece| ShapePortSide {
        boundary: BoundaryPieceRef {
            domain,
            piece: CurvePieceRef {
                curve: piece,
                piece: 0,
            },
        },
        span: ParamSpan {
            first: 0.0,
            last: 1.0,
        },
        axial: AxialMap {
            offset: 0.0,
            scale: 1.0,
        },
    };
    SheetState {
        schema: 2,
        algorithm: ALGORITHM_VERSION,
        body: crate::ids::BodyUid::new(uid("F3"), 0),
        owner: crate::ids::ComponentUid::ROOT,
        shape_generation: String::new(),
        rules: uid("F1"),
        rules_version: String::new(),
        panels: Vec::new(),
        charts: vec![SheetPatch {
            domain: SheetDomainRef::Panel(panel),
            regions: vec![region],
            embedding: SheetEmbedding::Plane {
                front_frame: SketchFrame {
                    origin: [0.0, 0.0, 0.0],
                    x_axis: [1.0, 0.0, 0.0],
                    y_axis: [0.0, 1.0, 0.0],
                },
                front_to_back_sign: MaterialParity::Positive,
            },
        }],
        bends: Vec::new(),
        joins: vec![ShapePort {
            token: 0,
            axial_interval: ParamSpan {
                first: 0.0,
                last: 10.0,
            },
            first: side(SheetDomainRef::Panel(panel), curve(2)),
            second: side(
                SheetDomainRef::Bend(bend),
                SheetCurveId::Generated {
                    feature: uid("F4"),
                    slot: 43,
                },
            ),
        }],
        opened: Vec::new(),
        lineage: Vec::new(),
        overlays: Vec::new(),
        invalidated: None,
        blockers: Vec::new(),
    }
}

#[test]
fn the_edge_between_a_wall_and_a_bend_side_is_the_corner_beside_the_bend() {
    let state = panel_beside_bend();
    let panel = beside_panel();
    let curve = beside_curve;
    let picked = |edge: &str| reference(&state, 1.0, None, Some(edge), None).unwrap();
    let ends = |answer: &Value| -> (SheetCurveId, SheetCurveId) {
        (
            serde_json::from_value(answer["corner"]["incoming"].clone()).unwrap(),
            serde_json::from_value(answer["corner"]["outgoing"].clone()).unwrap(),
        )
    };
    // The bottom wall meets the bend's side where the tangent line starts,
    // whichever face the edge name gives first.
    for edge in [
        "E{F3:sheet_side0(F3:sheet_curve(1))|F4:sheet_bend_side0(F4:sheet_curve(40))}",
        "E{F4:sheet_bend_side0(F4:sheet_curve(40))|F3:sheet_side0(F3:sheet_curve(1))}",
    ] {
        let answer = picked(edge);
        assert_eq!(ends(&answer), (curve(1), curve(2)), "{answer}");
        assert_eq!(answer["straight"], json!(true), "{answer}");
    }
    // The top wall where it ends.
    let answer =
        picked("E{F3:sheet_side0(F3:sheet_curve(3))|F4:sheet_bend_side0(F4:sheet_curve(42))}");
    assert_eq!(ends(&answer), (curve(2), curve(3)), "{answer}");
    // The left wall does not meet the bend: the panel alone.
    let answer =
        picked("E{F3:sheet_side0(F3:sheet_curve(4))|F4:sheet_bend_side0(F4:sheet_curve(42))}");
    assert!(answer.get("corner").is_none(), "{answer}");
    assert_eq!(
        serde_json::from_value::<SheetDomainRef>(answer["domain"].clone()).unwrap(),
        SheetDomainRef::Panel(panel)
    );
}

#[test]
fn a_stored_corner_finds_the_edge_it_lies_on() {
    // The `sheet_corner_edges` query (mitcad#1766): among the body's
    // edges, the one `reference` answers as the stored corner.
    let state = panel_beside_bend();
    let stored = |incoming: u32, outgoing: u32| SheetCorner {
        panel: beside_panel(),
        incoming: beside_curve(incoming),
        outgoing: beside_curve(outgoing),
        incoming_piece: None,
        outgoing_piece: None,
    };
    let beside = "E{F3:sheet_side0(F3:sheet_curve(1))|F4:sheet_bend_side0(F4:sheet_curve(40))}";
    let walls = "E{F3:sheet_side0(F3:sheet_curve(1))|F3:sheet_side0(F3:sheet_curve(4))}";
    let listed = [
        "E{F3:sheet_front(0)|F3:sheet_side0(F3:sheet_curve(1))}",
        "E{F3:sheet_side0(F3:sheet_curve(4))|F4:sheet_bend_side0(F4:sheet_curve(42))}",
        beside,
        walls,
    ]
    .map(String::from);
    let nowhere = |_: &str| -> Option<[f64; 3]> { None };
    let found =
        |corner: &SheetCorner, names: &[String]| corner_edge(&state, 1.0, corner, names, &nowhere);
    // Beside the bend: the edge between the wall and the bend's side.
    assert_eq!(found(&stored(1, 2), &listed).as_deref(), Some(beside));
    // Between two walls: the edge between them.
    assert_eq!(found(&stored(4, 1), &listed).as_deref(), Some(walls));
    // A corner the body has no edge for.
    assert_eq!(found(&stored(2, 3), &listed), None);
    // Curves that do not meet.
    assert_eq!(found(&stored(1, 3), &listed), None);
    // A kernel that lists no edges: the edge between the two walls.
    let unlisted: [String; 0] = [];
    assert_eq!(found(&stored(4, 1), &unlisted).as_deref(), Some(walls));
    assert_eq!(found(&stored(1, 3), &unlisted), None);
}
