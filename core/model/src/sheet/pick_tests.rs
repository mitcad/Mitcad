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
