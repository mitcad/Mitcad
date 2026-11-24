// SPDX-License-Identifier: MIT
//! URDF export for ROS 2 (mitcad#2100) with the mock kernel: an arm of
//! 10 x 10 x 5 blocks (each at its component's origin) whose package
//! folder is read back.
//!
//! - Base:1 grounded at the origin; A:1 on its top by a revolute joint
//!   limited to ±90° (Lift); B:1 on A by a revolute without limits
//!   (Spin, continuous); C:1 on B by a slider along x, 0 to 20 mm (Slide);
//!   D:1 on C by a ball joint (Wrist: a chain of three single-axis joints
//!   through two intermediate links);
//! - E:1 by a revolute whose `a` side is the base (Back, written reversed:
//!   its axis negated, its limits the MitCad joint's);
//! - F:1 and G:1 in a rigid group (one link), F on the base by a revolute
//!   (Hinge);
//! - H:1 and I:1 in a loop with the base (Loop1 H on the base, Loop2 I on
//!   H, Loop3 I on the base): the joint that closes it is left out with a
//!   warning.
//!
//! The URDF parses as XML and is a tree; the counts, types, parents,
//! children, axes and limits are checked, the link of the rigid group's
//! mass and inertia against the `properties` totals of its bodies, and one
//! mesh's bounding box in metres in its link's frame.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, OccurrenceUid};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let text = command.to_string();
    match doc.command(&text) {
        Ok(result) => serde_json::from_str(&result).unwrap(),
        Err(e) => panic!("{text}: {e}"),
    }
}

fn block_in(doc: &mut Document<MockKernel>) -> BodyUid {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    BodyUid::new(uid, 0)
}

fn face(doc: &Document<MockKernel>, body: BodyUid, role: &str) -> String {
    doc.body_shape(body)
        .unwrap()
        .faces
        .iter()
        .find(|f| f.role == role)
        .unwrap()
        .to_string()
}

fn part(doc: &mut Document<MockKernel>, name: &str, at: [f64; 3]) -> (OccurrenceUid, BodyUid) {
    let active = doc.assembly().active;
    let (_, occurrence) = doc
        .create_component(Some(name), Transform::translation(at), true)
        .unwrap();
    let body = block_in(doc);
    doc.activate_component(active).unwrap();
    (occurrence, body)
}

/// An origin on a block's top (`end`) or bottom (`start`) face.
fn origin(doc: &Document<MockKernel>, occurrence: &str, body: BodyUid, role: &str) -> Value {
    json!({"occurrence": occurrence, "geometry": {"body": body, "face": face(doc, body, role)}})
}

struct Arm {
    doc: Document<MockKernel>,
    bodies: BTreeMap<&'static str, BodyUid>,
}

fn arm() -> Arm {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().physical.set(true);
    let mut bodies = BTreeMap::new();
    let mut occurrences = BTreeMap::new();
    for (name, at) in [
        ("Base", [0.0, 0.0, 0.0]),
        ("A", [0.0, 0.0, 5.0]),
        ("B", [0.0, 0.0, 10.0]),
        ("C", [0.0, 0.0, 15.0]),
        ("D", [0.0, 0.0, 20.0]),
        ("E", [40.0, 0.0, 5.0]),
        ("F", [0.0, 40.0, 5.0]),
        ("G", [0.0, 60.0, 5.0]),
        ("H", [-40.0, 0.0, 5.0]),
        ("I", [-60.0, 0.0, 5.0]),
    ] {
        let (occurrence, body) = part(&mut doc, name, at);
        bodies.insert(name, body);
        occurrences.insert(name, occurrence);
    }
    doc.set_occurrence_grounded(occurrences["Base"], true)
        .unwrap();
    let top =
        |doc: &Document<MockKernel>, n: &str| origin(doc, &format!("{n}:1"), bodies[n], "end");
    let bottom =
        |doc: &Document<MockKernel>, n: &str| origin(doc, &format!("{n}:1"), bodies[n], "start");
    let joints = [
        json!({"cmd": "add_joint", "name": "Lift", "kind": "revolute", "flip": true,
               "a": bottom(&doc, "A"), "b": top(&doc, "Base"),
               "limits": {"rz": {"min": "-90 deg", "max": "90 deg"}}}),
        json!({"cmd": "add_joint", "name": "Spin", "kind": "revolute", "flip": true,
               "a": bottom(&doc, "B"), "b": top(&doc, "A")}),
        json!({"cmd": "add_joint", "name": "Slide", "kind": "slider", "slide_axis": "x",
               "flip": true, "a": bottom(&doc, "C"), "b": top(&doc, "B"),
               "limits": {"tx": {"min": 0, "max": 20}}, "position": {"tx": 0}}),
        json!({"cmd": "add_joint", "name": "Wrist", "kind": "ball", "flip": true,
               "a": bottom(&doc, "D"), "b": top(&doc, "C")}),
        json!({"cmd": "add_joint", "name": "Back", "kind": "revolute", "flip": true,
               "a": top(&doc, "Base"), "b": bottom(&doc, "E"),
               "limits": {"rz": {"min": "-30 deg", "max": "60 deg"}}}),
        json!({"cmd": "add_rigid_group", "occurrences": ["F:1", "G:1"]}),
        json!({"cmd": "add_joint", "name": "Hinge", "kind": "revolute", "flip": true,
               "a": bottom(&doc, "F"), "b": top(&doc, "Base"),
               "limits": {"rz": {"min": "0 deg", "max": "45 deg"}}}),
        json!({"cmd": "add_joint", "name": "Loop1", "kind": "revolute", "flip": true,
               "a": bottom(&doc, "H"), "b": top(&doc, "Base")}),
        json!({"cmd": "add_joint", "name": "Loop2", "kind": "revolute",
               "a": bottom(&doc, "I"), "b": bottom(&doc, "H")}),
        json!({"cmd": "add_joint", "name": "Loop3", "kind": "revolute", "flip": true,
               "a": bottom(&doc, "I"), "b": top(&doc, "Base")}),
    ];
    for joint in joints {
        command(&mut doc, joint);
    }
    Arm { doc, bodies }
}

fn temp_dir() -> std::path::PathBuf {
    mitcad_model::testing::unique_temp_dir("urdf")
}

fn attribute<'a>(node: roxmltree::Node<'a, 'a>, child: &str, name: &str) -> Option<&'a str> {
    node.children()
        .find(|n| n.has_tag_name(child))
        .and_then(|n| n.attribute(name))
}

fn numbers(text: &str) -> Vec<f64> {
    text.split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect()
}

#[test]
fn an_arm_is_written_as_a_ros_package() {
    let mut m = arm();
    let dir = temp_dir();
    let path = dir.join("Test Arm.urdf");
    let result = command(
        &mut m.doc,
        json!({"cmd": "export", "path": path.to_str().unwrap(), "refinement": "low"}),
    );
    assert_eq!(result["format"], "urdf", "{result}");
    assert_eq!(result["package"], "test_arm");
    let folder = dir.join("test_arm");
    for file in ["package.xml", "CMakeLists.txt", "urdf/Test_Arm.urdf"] {
        assert!(folder.join(file).is_file(), "{file}: {result}");
    }
    let text = std::fs::read_to_string(folder.join("urdf/Test_Arm.urdf")).unwrap();
    let xml = roxmltree::Document::parse(&text).unwrap();
    let robot = xml.root_element();
    assert_eq!(robot.tag_name().name(), "robot");
    let links: Vec<_> = robot
        .children()
        .filter(|n| n.has_tag_name("link"))
        .collect();
    let joints: Vec<_> = robot
        .children()
        .filter(|n| n.has_tag_name("joint"))
        .collect();
    let link_names: BTreeSet<&str> = links.iter().map(|l| l.attribute("name").unwrap()).collect();
    // The base, A to F (G is in F's link), H and I; Wrist's two
    // intermediate links.
    let expected: BTreeSet<&str> = [
        "base_link",
        "A_1",
        "B_1",
        "C_1",
        "D_1",
        "E_1",
        "F_1",
        "H_1",
        "I_1",
        "Wrist_1",
        "Wrist_2",
    ]
    .into_iter()
    .collect();
    assert_eq!(link_names, expected, "{text}");
    // A tree: one joint per link but the base, every child once, every
    // parent a link.
    assert_eq!(joints.len(), links.len() - 1, "{text}");
    let mut children = BTreeSet::new();
    for joint in &joints {
        let child = attribute(*joint, "child", "link").unwrap();
        let parent = attribute(*joint, "parent", "link").unwrap();
        assert!(children.insert(child), "{child} has two parents");
        assert!(link_names.contains(parent) && link_names.contains(child));
        assert_ne!(child, "base_link");
    }
    let joint = |name: &str| {
        *joints
            .iter()
            .find(|j| j.attribute("name") == Some(name))
            .unwrap_or_else(|| panic!("no joint {name}: {text}"))
    };
    let check = |name: &str,
                 kind: &str,
                 parent: &str,
                 child: &str,
                 axis: [f64; 3],
                 limits: Option<[f64; 2]>| {
        let j = joint(name);
        assert_eq!(j.attribute("type"), Some(kind), "{name}");
        assert_eq!(attribute(j, "parent", "link"), Some(parent), "{name}");
        assert_eq!(attribute(j, "child", "link"), Some(child), "{name}");
        let a = numbers(attribute(j, "axis", "xyz").unwrap());
        for k in 0..3 {
            assert!((a[k] - axis[k]).abs() < 1e-9, "{name}: axis {a:?}");
        }
        match limits {
            Some([lower, upper]) => {
                let lo: f64 = attribute(j, "limit", "lower").unwrap().parse().unwrap();
                let hi: f64 = attribute(j, "limit", "upper").unwrap().parse().unwrap();
                assert!(
                    (lo - lower).abs() < 1e-9 && (hi - upper).abs() < 1e-9,
                    "{name}: {lo} {hi}"
                );
                assert_eq!(attribute(j, "limit", "effort"), Some("100"));
                assert_eq!(attribute(j, "limit", "velocity"), Some("1"));
            }
            None => assert!(attribute(j, "limit", "lower").is_none(), "{name}"),
        }
    };
    let quarter = std::f64::consts::FRAC_PI_2;
    check(
        "Lift",
        "revolute",
        "base_link",
        "A_1",
        [0.0, 0.0, 1.0],
        Some([-quarter, quarter]),
    );
    check("Spin", "continuous", "A_1", "B_1", [0.0, 0.0, 1.0], None);
    check(
        "Slide",
        "prismatic",
        "B_1",
        "C_1",
        [1.0, 0.0, 0.0],
        Some([0.0, 0.02]),
    );
    check(
        "Wrist_rz",
        "continuous",
        "C_1",
        "Wrist_1",
        [0.0, 0.0, 1.0],
        None,
    );
    check(
        "Wrist_ry",
        "continuous",
        "Wrist_1",
        "Wrist_2",
        [0.0, 1.0, 0.0],
        None,
    );
    check(
        "Wrist_rx",
        "continuous",
        "Wrist_2",
        "D_1",
        [1.0, 0.0, 0.0],
        None,
    );
    // Reversed: the base is its a side; the value and limits stay the
    // MitCad joint's, the axis turned round.
    let sixth = std::f64::consts::PI / 6.0;
    check(
        "Back",
        "revolute",
        "base_link",
        "E_1",
        [0.0, 0.0, -1.0],
        Some([-sixth, 2.0 * sixth]),
    );
    check(
        "Hinge",
        "revolute",
        "base_link",
        "F_1",
        [0.0, 0.0, 1.0],
        Some([0.0, quarter / 2.0]),
    );
    // Lift's origin: the base's top face middle, in metres.
    let xyz = numbers(attribute(joint("Lift"), "origin", "xyz").unwrap());
    for (k, v) in [0.005, 0.005, 0.005].iter().enumerate() {
        assert!((xyz[k] - v).abs() < 1e-9, "Lift origin {xyz:?}");
    }
    // The loop: Loop3 reaches I from the base first, so Loop2 closes it.
    assert!(joints.iter().all(|j| j.attribute("name") != Some("Loop2")));
    assert_eq!(result["left_out"].as_array().unwrap().len(), 1, "{result}");
    assert_eq!(result["left_out"][0]["name"], "Loop2");
    let warnings = result["warnings"].to_string();
    assert!(warnings.contains("Loop2 closes a loop"), "{warnings}");
    // Wrist's intermediate links weigh next to nothing.
    let wrist = links
        .iter()
        .find(|l| l.attribute("name") == Some("Wrist_1"))
        .unwrap();
    let inertial = wrist
        .children()
        .find(|n| n.has_tag_name("inertial"))
        .unwrap();
    assert_eq!(attribute(inertial, "mass", "value"), Some("0.000001"));

    // F_1 holds F's and G's bodies: its mass and inertia are their
    // properties' totals (the trace, which turning does not change), at
    // the default density their missing material gives.
    let properties: Value = serde_json::from_str(
        &m.doc
            .query(
                &json!({"query": "properties", "bodies": [m.bodies["F"], m.bodies["G"]],
                        "density": 7.85})
                .to_string(),
            )
            .unwrap(),
    )
    .unwrap();
    let total = &properties["total"];
    let f = links
        .iter()
        .find(|l| l.attribute("name") == Some("F_1"))
        .unwrap();
    assert_eq!(f.children().filter(|n| n.has_tag_name("visual")).count(), 2);
    let inertial = f.children().find(|n| n.has_tag_name("inertial")).unwrap();
    let mass: f64 = attribute(inertial, "mass", "value")
        .unwrap()
        .parse()
        .unwrap();
    let want = total["mass"].as_f64().unwrap();
    assert!(((mass - want) / want).abs() < 1e-6, "mass {mass} vs {want}");
    let inertia = inertial
        .children()
        .find(|n| n.has_tag_name("inertia"))
        .unwrap();
    let trace: f64 = ["ixx", "iyy", "izz"]
        .iter()
        .map(|k| inertia.attribute(*k).unwrap().parse::<f64>().unwrap())
        .sum();
    let rows = total["inertia"].as_array().unwrap();
    let want: f64 = (0..3).map(|i| rows[i][i].as_f64().unwrap()).sum::<f64>() / 1e6;
    assert!(
        want > 0.0 && ((trace - want) / want).abs() < 1e-6,
        "trace {trace} vs {want}"
    );
    // The whole tensor: its other invariants under a turn (the sum of the
    // squares of its entries and its determinant) match too, so it is the
    // properties' inertia turned into the link's axes. Each is compared
    // relative to the tensor's size (the trace to the power of its degree):
    // the mock kernel's inertia may be singular, its determinant 0 up to
    // rounding.
    let written = |a: &str| -> f64 { inertia.attribute(a).unwrap().parse().unwrap() };
    let link = [
        [written("ixx"), written("ixy"), written("ixz")],
        [written("ixy"), written("iyy"), written("iyz")],
        [written("ixz"), written("iyz"), written("izz")],
    ];
    let given: [[f64; 3]; 3] =
        std::array::from_fn(|i| std::array::from_fn(|j| rows[i][j].as_f64().unwrap() / 1e6));
    let squares = |m: &[[f64; 3]; 3]| m.iter().flatten().map(|v| v * v).sum::<f64>();
    let determinant = |m: &[[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    for (what, got, want, size) in [
        ("squares", squares(&link), squares(&given), trace * trace),
        (
            "determinant",
            determinant(&link),
            determinant(&given),
            trace * trace * trace,
        ),
    ] {
        assert!(((got - want) / size).abs() < 1e-6, "{what} {got} vs {want}");
    }

    // A's mesh: in metres in its link's frame, which is Lift's frame on
    // the base's top at value 0: x and y across ±5 mm, z from 0 to 5 mm.
    let meshes: Vec<String> = result["meshes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(meshes.len(), 10, "{result}");
    for mesh in &meshes {
        assert!(folder.join("meshes").join(mesh).is_file(), "{mesh}");
        assert!(
            text.contains(&format!("package://test_arm/meshes/{mesh}")),
            "{mesh}"
        );
    }
    let a_mesh = meshes.iter().find(|m| m.starts_with("A_1_")).unwrap();
    let data = std::fs::read(folder.join("meshes").join(a_mesh)).unwrap();
    let triangles = mitcad_model::exchange::read_stl(&data);
    assert_eq!(triangles.len(), 12);
    let mut low = [f64::MAX; 3];
    let mut high = [f64::MIN; 3];
    for p in triangles.iter().flatten() {
        for k in 0..3 {
            low[k] = low[k].min(p[k]);
            high[k] = high[k].max(p[k]);
        }
    }
    for (k, (lo, hi)) in [(-0.005, 0.005), (-0.005, 0.005), (0.0, 0.005)]
        .iter()
        .enumerate()
    {
        assert!(
            (low[k] - lo).abs() < 1e-6 && (high[k] - hi).abs() < 1e-6,
            "{low:?} {high:?}"
        );
    }
}

#[test]
fn urdf_options_are_checked() {
    let mut m = arm();
    let dir = temp_dir();
    let path = dir.join("arm.urdf");
    let refused = |doc: &mut Document<MockKernel>, extra: Value| {
        let mut cmd = json!({"cmd": "export", "path": path.to_str().unwrap()});
        for (k, v) in extra.as_object().unwrap() {
            cmd[k] = v.clone();
        }
        doc.command(&cmd.to_string()).unwrap_err().to_string()
    };
    assert!(refused(&mut m.doc, json!({"package": "My Arm"})).contains("no ROS package name"));
    assert!(refused(&mut m.doc, json!({"effort": 0})).contains("effort"));
    assert!(refused(&mut m.doc, json!({"bodies": [m.bodies["A"]]})).contains("does not apply"));
    let step = dir.join("arm.step");
    let e = m
        .doc
        .command(
            &json!({"cmd": "export", "path": step.to_str().unwrap(), "package": "arm"}).to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(e.contains("URDF"), "{e}");
    // A package named as given, with the effort and velocity given.
    let result = command(
        &mut m.doc,
        json!({"cmd": "export", "path": path.to_str().unwrap(), "format": "urdf",
               "package": "my_arm", "effort": 5, "velocity": 0.5}),
    );
    let text = std::fs::read_to_string(dir.join("my_arm/urdf/arm.urdf")).unwrap();
    assert!(text.contains("effort=\"5\" velocity=\"0.5\""), "{result}");
}
