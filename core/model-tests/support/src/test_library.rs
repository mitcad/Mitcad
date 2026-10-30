// SPDX-License-Identifier: MIT
//! A small library of synthetic BUILD parts for the model tests: square
//! slot profiles of three systems (made as the profile layout says: a
//! section on XY extruded by `L`, `mount:` frames at its ends and slots),
//! a profile with closed sides, a cap, a bracket with its nut and screw,
//! panels sized by parameters or fixed, a rail with its carriage and a
//! slider for a slot. A resolver in memory serves them at two versions.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::{Value, json};

use mitcad_model::features::SketchPlane;
use mitcad_model::file::MemoryStore;
use mitcad_model::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::library::{LibraryRequest, LibrarySource, LinkResolver};
use mitcad_model::test_support::def;
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model::{Document, ValueInput};
use mitcad_model_api::DocumentApi;

pub const V1: &str = "1111111111111111111111111111111111111111";
pub const V2: &str = "2222222222222222222222222222222222222222";
pub const LIBRARY: &str = "test-profiles";
pub const URL: &str = "https://example.org/test-profiles.git";

pub fn command(doc: &mut Document<MockKernel>, command: Value) -> Result<Value, String> {
    doc.command(&command.to_string())
        .map(|text| serde_json::from_str(&text).unwrap())
        .map_err(|e| e.to_string())
}

pub fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// A frame of a design: a joint origin named `mount:<name>` on a fixed
/// plane.
pub fn fixed_frame(
    doc: &mut Document<MockKernel>,
    name: &str,
    origin: [f64; 3],
    z: [f64; 3],
    x: [f64; 3],
) {
    doc.add_feature(
        &def(json!({"type": "joint_origin",
                    "geometry": {"origin": origin, "normal": z, "x_axis": x}})),
        Some(&format!("mount:{name}")),
    )
    .unwrap();
}

/// A square profile `w` wide, `L` long (1000 mm), its slot frames on the
/// sides `open` lists (`x+`, `x-`, `y+`, `y-`).
pub fn profile_design(w: f64, open: &[&str]) -> String {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "L", "expression": "1000 mm"}),
    )
    .unwrap();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(
            sketch,
            [-w / 2.0, -w / 2.0],
            &ValueInput::Number(w),
            &ValueInput::Number(w),
        )
        .unwrap()
        .region;
    let extrude = doc
        .add_feature(
            &def(
                json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                        "extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}),
            ),
            None,
        )
        .unwrap()
        .uid;
    fixed_frame(
        &mut doc,
        "end_a",
        [0.0; 3],
        [0.0, 0.0, -1.0],
        [1.0, 0.0, 0.0],
    );
    doc.add_feature(
        &def(json!({"type": "joint_origin",
                    "geometry": {"body": format!("{extrude}.b0"),
                                 "face": format!("{extrude}:end({region})")}})),
        Some("mount:end_b"),
    )
    .unwrap();
    let h = w / 2.0;
    for side in open {
        let (origin, z) = match *side {
            "x+" => ([h, 0.0, 0.0], [1.0, 0.0, 0.0]),
            "x-" => ([-h, 0.0, 0.0], [-1.0, 0.0, 0.0]),
            "y+" => ([0.0, h, 0.0], [0.0, 1.0, 0.0]),
            _ => ([0.0, -h, 0.0], [0.0, -1.0, 0.0]),
        };
        fixed_frame(
            &mut doc,
            &format!("slot_{side}_1"),
            origin,
            z,
            [0.0, 0.0, 1.0],
        );
    }
    doc.recompute();
    doc.to_json()
}

/// A guide rail 15 x 15, `L` long (1000 mm): the ends of a profile with no
/// slots, its carriage's frame `rail` on its top and its face frame `base`
/// on its bottom (z down, x along the rail).
fn rail_design() -> String {
    let mut doc: Document<MockKernel> =
        Document::from_json(&profile_design(15.0, &[]), MockKernel::default()).unwrap();
    fixed_frame(
        &mut doc,
        "rail",
        [0.0, 7.5, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    );
    fixed_frame(
        &mut doc,
        "base",
        [0.0, -7.5, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
    );
    doc.recompute();
    doc.to_json()
}

pub const ALL_SIDES: [&str; 4] = ["x+", "x-", "y+", "y-"];

/// A profile's `build` block.
pub fn profile_build(kind: &str, slot: f64, grid: f64, open: &[&str]) -> Value {
    let mut frames = vec![
        json!({"name": "end_a", "kind": "end"}),
        json!({"name": "end_b", "kind": "end"}),
    ];
    for side in open {
        frames.push(json!({"name": format!("slot_{side}_1"), "kind": "slot"}));
    }
    json!({
        "role": "profile",
        "system": {"type": kind, "slot": slot, "grid": grid},
        "length": {"parameter": "L", "min": "10 mm", "max": "6000 mm", "stock": ["6000 mm"]},
        "section": {"width": format!("{grid} mm"), "height": format!("{grid} mm")},
        "slot": {"depth": "9 mm", "opening": "8.2 mm"},
        "mass_per_metre": "0.9 kg",
        "frames": frames,
    })
}

/// A frame: (name, origin, z, x).
type Frame<'a> = (&'a str, [f64; 3], [f64; 3], [f64; 3]);

/// A block part with frames given as (name, origin, z, x).
fn block_design(size: [f64; 3], corner: [f64; 2], frames: &[Frame<'_>]) -> String {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(
            sketch,
            corner,
            &ValueInput::Number(size[0]),
            &ValueInput::Number(size[1]),
        )
        .unwrap()
        .region;
    doc.add_feature(
        &def(
            json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                    "extent": {"type": "distance", "distance": size[2]}, "operation": "new_body"}),
        ),
        None,
    )
    .unwrap();
    for (name, origin, z, x) in frames {
        fixed_frame(&mut doc, name, *origin, *z, *x);
    }
    doc.recompute();
    doc.to_json()
}

/// A panel `W` x `H` x `t` (parameters, or `W` fixed).
fn panel_design(fixed_width: Option<f64>) -> String {
    let mut doc = Document::new(MockKernel::default());
    for (name, value) in [("W", "200 mm"), ("H", "200 mm"), ("t", "8 mm")] {
        command(
            &mut doc,
            json!({"cmd": "add_parameter", "name": name, "expression": value}),
        )
        .unwrap();
    }
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let width = match fixed_width {
        Some(w) => ValueInput::Number(w),
        None => ValueInput::Name("W".into()),
    };
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &width, &ValueInput::Name("H".into()))
        .unwrap()
        .region;
    doc.add_feature(
        &def(
            json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                    "extent": {"type": "distance", "distance": "t"}, "operation": "new_body"}),
        ),
        None,
    )
    .unwrap();
    doc.recompute();
    doc.to_json()
}

/// One component of the library: its design, build block, supplier and
/// article number.
#[derive(Clone)]
pub struct Part {
    pub text: String,
    pub build: Option<Value>,
    pub part_number: String,
    pub name: String,
    pub designation: String,
}

/// The library in memory: each component at V1, some changed at V2.
#[derive(Default)]
pub struct TestLibrary {
    pub parts: BTreeMap<(String, String), Part>,
}

impl TestLibrary {
    pub fn new() -> Arc<Self> {
        let mut library = Self::default();
        let mut add = |component: &str, rev: &str, part: Part| {
            library
                .parts
                .insert((component.to_owned(), rev.to_owned()), part);
        };
        let profile =
            |w: f64, kind: &str, slot: f64, open: &[&str], number: &str, name: &str| Part {
                text: profile_design(w, open),
                build: Some(profile_build(kind, slot, w, open)),
                part_number: number.to_owned(),
                name: name.to_owned(),
                designation: format!("{name} L={{L}}"),
            };
        for rev in [V1, V2] {
            add(
                "p30",
                rev,
                profile(30.0, "B", 8.0, &ALL_SIDES, "P30_N8_B", "Profile 30x30 B8"),
            );
            add(
                "p20",
                rev,
                profile(20.0, "I", 5.0, &ALL_SIDES, "P20_N5_I", "Profile 20x20 I5"),
            );
            add(
                "p40",
                rev,
                profile(40.0, "I", 8.0, &ALL_SIDES, "P40_N8_I", "Profile 40x40 I8"),
            );
            add(
                "p30-2n",
                rev,
                profile(
                    30.0,
                    "B",
                    8.0,
                    &["x+", "x-"],
                    "P30_2N",
                    "Profile 30x30 B8 2N",
                ),
            );
        }
        // At V2 the 30 profile is 32 wide (another section).
        add(
            "p30",
            V2,
            profile(
                32.0,
                "B",
                8.0,
                &ALL_SIDES,
                "P30_N8_B_V2",
                "Profile 30x30 B8",
            ),
        );
        // A cap for B8/30 ends: its frame on its face at z = 0, pointing out
        // of it towards the end it covers.
        add(
            "cap30",
            V1,
            Part {
                text: block_design(
                    [30.0, 30.0, 4.0],
                    [-15.0, -15.0],
                    &[("end", [0.0; 3], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0])],
                ),
                build: Some(
                    json!({"role": "cap", "systems": [{"type": "B", "slot": 8, "grid": 30}],
                                   "frames": [{"name": "end", "kind": "end"}]}),
                ),
                part_number: "CAP30".to_owned(),
                name: "Cover cap 30 B8".to_owned(),
                designation: String::new(),
            },
        );
        // An angle bracket: arm a on the profile whose end meets, arm b on
        // the other; mount_a 20 mm from the corner.
        add(
            "bracket30",
            V1,
            Part {
                text: block_design(
                    [40.0, 20.0, 40.0],
                    [0.0, -10.0],
                    &[
                        (
                            "mount_a",
                            [20.0, 0.0, 0.0],
                            [0.0, 0.0, -1.0],
                            [1.0, 0.0, 0.0],
                        ),
                        (
                            "mount_b",
                            [0.0, 0.0, 20.0],
                            [-1.0, 0.0, 0.0],
                            [0.0, 0.0, 1.0],
                        ),
                    ],
                ),
                build: Some(json!({
                    "role": "connector",
                    "systems": [{"type": "B", "slot": 8}],
                    "frames": [{"name": "mount_a", "kind": "face"}, {"name": "mount_b", "kind": "face"}],
                    "connector": {
                        "junctions": ["L", "T"],
                        "space": "inner_corner",
                        "mates": [{"frame": "mount_a", "to": "slot"}, {"frame": "mount_b", "to": "slot"}],
                        "fasteners": [{"frame": "mount_a",
                                       "nut": {"library": LIBRARY, "component": "tnut8"},
                                       "screw": {"library": "test-fasteners", "component": "screw6"}}],
                        "machining": [{"on": "a", "frame": "end",
                                       "hole": {"diameter": "6.8 mm", "depth": "15 mm"}}]
                    }
                })),
                part_number: "BR30".to_owned(),
                name: "Bracket 30 B8".to_owned(),
                designation: String::new(),
            },
        );
        add(
            "tnut8",
            V1,
            Part {
                text: block_design(
                    [10.0, 16.0, 4.0],
                    [-5.0, -8.0],
                    &[("slot", [0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0])],
                ),
                build: Some(
                    json!({"role": "fastener", "systems": [{"type": "B", "slot": 8}],
                                   "frames": [{"name": "slot", "kind": "slot"}]}),
                ),
                part_number: "TN8".to_owned(),
                name: "T-nut B8 M6".to_owned(),
                designation: String::new(),
            },
        );
        add(
            "screw6",
            V1,
            Part {
                text: block_design([6.0, 6.0, 12.0], [-3.0, -3.0], &[]),
                build: None,
                part_number: String::new(),
                name: "Screw M6x12".to_owned(),
                designation: String::new(),
            },
        );
        // Panels: sized by W and H, or 300 mm wide.
        add(
            "panel",
            V1,
            Part {
                text: panel_design(None),
                build: Some(json!({"role": "panel",
                                   "panel": {"mount": "slot", "thickness": "t", "width": "W",
                                             "height": "H", "clearance": "0.5 mm"}})),
                part_number: String::new(),
                name: "Panel".to_owned(),
                designation: "Panel {W}x{H}".to_owned(),
            },
        );
        add(
            "sheet300",
            V1,
            Part {
                text: panel_design(Some(300.0)),
                build: Some(json!({"role": "panel",
                                   "panel": {"mount": "slot", "thickness": "t", "height": "H"}})),
                part_number: "SHEET300".to_owned(),
                name: "Sheet 300".to_owned(),
                designation: String::new(),
            },
        );
        // A panel on the outer faces of a frame, held by clamps: a block
        // 20 long along its slot frame's x axis.
        add(
            "surface-panel",
            V1,
            Part {
                text: panel_design(None),
                build: Some(json!({"role": "panel",
                                   "panel": {"mount": "surface", "thickness": "t", "width": "W",
                                             "height": "H",
                                             "clamp": {"library": LIBRARY, "component": "clamp30"}}})),
                part_number: "SP".to_owned(),
                name: "Surface panel".to_owned(),
                designation: "Surface panel {W}x{H}".to_owned(),
            },
        );
        add(
            "clamp30",
            V1,
            Part {
                text: block_design(
                    [20.0, 12.0, 6.0],
                    [-10.0, -6.0],
                    &[
                        ("mount_a", [0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
                        ("mount_b", [0.0, 0.0, 6.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
                    ],
                ),
                build: Some(
                    json!({"role": "connector", "systems": [{"type": "B", "slot": 8}],
                                   "frames": [{"name": "mount_a", "kind": "slot"},
                                              {"name": "mount_b", "kind": "face"}],
                                   "connector": {"junctions": ["surface"], "space": "outer",
                                                 "mates": [{"frame": "mount_a", "to": "slot"},
                                                           {"frame": "mount_b", "to": "face"}]}}),
                ),
                part_number: "CL30".to_owned(),
                name: "Clamp 30 B8".to_owned(),
                designation: String::new(),
            },
        );
        add(
            "rail15",
            V1,
            Part {
                text: rail_design(),
                build: Some(json!({
                    "role": "linear",
                    "length": {"parameter": "L", "min": "40 mm", "max": "1000 mm"},
                    "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
                               {"name": "rail", "kind": "rail"}, {"name": "base", "kind": "face"}],
                    "linear": {"kind": "rail"}
                })),
                part_number: "R15".to_owned(),
                name: "Rail 15".to_owned(),
                designation: "Rail 15 L={L}".to_owned(),
            },
        );
        // A slider that slides in a B8 slot (mitcad#448): a block 20 long
        // along its slot frame's x axis, the frame on its underside.
        add(
            "slider8",
            V1,
            Part {
                text: block_design(
                    [20.0, 12.0, 5.0],
                    [-10.0, -6.0],
                    &[
                        ("slot", [0.0; 3], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]),
                        ("top", [0.0, 0.0, 5.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
                    ],
                ),
                build: Some(json!({
                    "role": "linear",
                    "systems": [{"type": "B", "slot": 8}],
                    "frames": [{"name": "slot", "kind": "slot"}, {"name": "top", "kind": "face"}],
                    "linear": {"kind": "carriage",
                               "mate": {"frame": "slot", "on": "slot", "joint": "slider", "axis": "x"}}
                })),
                part_number: "SL8".to_owned(),
                name: "Slider B8".to_owned(),
                designation: String::new(),
            },
        );
        // Timber (mitcad#478): members, a wood screw in rows, a joist
        // hanger, a post base and a plywood sheet.
        add("post98", V1, board("Sawn 100x100", 98.0, 98.0, &[]));
        add("beam148", V1, board("Dimensioned 48x148", 148.0, 48.0, &[]));
        add("deck120", V1, board("Planed 28x120", 120.0, 28.0, &[]));
        add(
            "rows98",
            V1,
            board("Dimensioned 48x98", 98.0, 48.0, &["spruce", "pine"]),
        );
        add("wood-screw", V1, wood_screw());
        add(
            "proud-screw",
            V1,
            seated_fastener("screw", "countersunk", None, 3.0),
        );
        add("proud-nail", V1, seated_fastener("nail", "flat", None, 1.5));
        add("pan-screw", V1, seated_fastener("screw", "pan", None, 3.0));
        add("hex-bolt", V1, seated_fastener("screw", "hex", None, 4.0));
        add(
            "named-seat",
            V1,
            seated_fastener("screw", "pan", Some("drive"), 3.0),
        );
        add(
            "bearing-seat",
            V1,
            seated_fastener("screw", "countersunk", Some("head_underside"), 3.0),
        );
        add("oblique-implicit", V1, oblique_fastener(None, true));
        add("oblique-seat", V1, oblique_fastener(Some("drive"), true));
        add(
            "misaligned-seat",
            V1,
            oblique_fastener(Some("drive"), false),
        );
        add("hanger148", V1, hanger(148.0, 48.0));
        add("postbase98", V1, post_base());
        add("plywood", V1, plywood());
        // Profiled boards (mitcad#477): tongue and groove (two tongues),
        // shiplap, feather-edge and a grooved deck board.
        for (id, part) in boards() {
            add(id, V1, part);
        }
        // A ring nail in rows (mitcad#497).
        add("ring-nail", V1, ring_nail());
        // A tube whose rows switch a feature by a condition (mitcad#873).
        add("bound-rows", V1, bound_rows());
        // A profile without slots that is no timber: a steel tube.
        add("tube40", V1, tube40());
        Arc::new(library)
    }
}

/// A timber member `w` wide (along X) and `t` thick (along Y), `L` long
/// (3000 mm), with face frames on its four sides; with configuration rows
/// that change only its metadata when `rows` names them.
pub fn board_design(w: f64, t: f64, rows: &[&str]) -> String {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "L", "expression": "3000 mm"}),
    )
    .unwrap();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(
            sketch,
            [-w / 2.0, -t / 2.0],
            &ValueInput::Number(w),
            &ValueInput::Number(t),
        )
        .unwrap()
        .region;
    let extrude = doc
        .add_feature(
            &def(
                json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                        "extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}),
            ),
            None,
        )
        .unwrap()
        .uid;
    fixed_frame(
        &mut doc,
        "end_a",
        [0.0; 3],
        [0.0, 0.0, -1.0],
        [1.0, 0.0, 0.0],
    );
    doc.add_feature(
        &def(json!({"type": "joint_origin",
                    "geometry": {"body": format!("{extrude}.b0"),
                                 "face": format!("{extrude}:end({region})")}})),
        Some("mount:end_b"),
    )
    .unwrap();
    for (name, origin, z) in [
        ("face_xp", [w / 2.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
        ("face_xn", [-w / 2.0, 0.0, 0.0], [-1.0, 0.0, 0.0]),
        ("face_yp", [0.0, t / 2.0, 0.0], [0.0, 1.0, 0.0]),
        ("face_yn", [0.0, -t / 2.0, 0.0], [0.0, -1.0, 0.0]),
    ] {
        fixed_frame(&mut doc, name, origin, z, [0.0, 0.0, 1.0]);
    }
    // The wood its members show.
    command(
        &mut doc,
        json!({"cmd": "set_body_appearance", "uid": format!("{extrude}.b0"), "appearance": "wood_oak"}),
    )
    .unwrap();
    // Its end grain on the caps (mitcad#484).
    command(
        &mut doc,
        json!({"cmd": "set_face_appearance", "uid": format!("{extrude}.b0"),
               "faces": [format!("{extrude}:start({region})"), format!("{extrude}:end({region})")],
               "appearance": "wood_walnut"}),
    )
    .unwrap();
    if !rows.is_empty() {
        let rows: Vec<Value> = rows.iter().map(|r| json!({"name": r})).collect();
        command(
            &mut doc,
            json!({"cmd": "set_configurations", "configurations": {
                "parameters": [], "default": "spruce", "rows": rows}}),
        )
        .unwrap();
    }
    doc.recompute();
    doc.to_json()
}

/// A timber member's `build` block (the layout `docs/libraries.md` gives).
pub fn board_build(w: f64, t: f64, finish: &str) -> Value {
    json!({
        "role": "profile",
        "kind": "timber",
        "section": {"width": format!("{w} mm"), "height": format!("{t} mm")},
        "nominal": {"width": w + 2.0, "height": t + 2.0},
        "material": {"species": "spruce", "finish": finish, "grade": "", "density": 450,
                     "moisture": 20},
        "availability": "standard",
        "length": {"parameter": "L", "min": "100 mm", "max": "6000 mm",
                   "stock": [2700, 3000, 3300, 3600, 3900, 4200, 4500, 4800, 5100, 5400]},
        "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
                   {"name": "face_xp", "kind": "face"}, {"name": "face_xn", "kind": "face"},
                   {"name": "face_yp", "kind": "face"}, {"name": "face_yn", "kind": "face"}],
    })
}

fn board(name: &str, w: f64, t: f64, rows: &[&str]) -> Part {
    let finish = name
        .split_whitespace()
        .next()
        .unwrap_or("Sawn")
        .to_lowercase();
    let mut build = board_build(w, t, &finish);
    if !rows.is_empty() {
        // The pine row: another species and density, and less common.
        build["rows"] = json!({"pine": {"material": {"species": "pine", "density": 520},
                                        "availability": "less common"}});
    }
    Part {
        text: board_design(w, t, rows),
        build: Some(build),
        part_number: String::new(),
        name: name.to_owned(),
        designation: format!("{name} L={{L}}"),
    }
}

/// A wood screw 5 x 60 (rows 4x40 ... 6x120): its `axis` frame at the
/// head, z along the shank.
fn wood_screw() -> Part {
    let mut doc: Document<MockKernel> = Document::from_json(
        &block_design(
            [5.0, 5.0, 60.0],
            [-2.5, -2.5],
            &[("axis", [0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0])],
        ),
        MockKernel::default(),
    )
    .unwrap();
    let names = ["4x40", "5x60", "5x80", "5x100", "6x120"];
    let rows: Vec<Value> = names.iter().map(|r| json!({"name": r})).collect();
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": {
            "parameters": [], "default": "5x60", "rows": rows}}),
    )
    .unwrap();
    let mut by_row = serde_json::Map::new();
    for name in names {
        let (d, l) = name.split_once('x').unwrap();
        by_row.insert(
            name.to_owned(),
            json!({"wood": {"diameter": d.parse::<f64>().unwrap(),
                            "length": l.parse::<f64>().unwrap()}}),
        );
    }
    Part {
        text: doc.to_json(),
        build: Some(json!({
            "role": "fastener",
            "wood": {"kind": "screw", "diameter": "5 mm", "length": "60 mm", "head": "10 mm"},
            "frames": [{"name": "axis", "kind": "shaft"}],
            "rows": by_row,
        })),
        part_number: String::new(),
        name: "Wood screw".to_owned(),
        designation: "Wood screw {config}".to_owned(),
    }
}

/// A synthetic fastener whose head stands behind its shaft's bearing
/// frame: body z = 0..60+K, axis at K, tip 60 mm beyond the axis.
fn seated_fastener(kind: &str, head: &str, seat: Option<&str>, k: f64) -> Part {
    Part {
        text: block_design(
            [10.0, 10.0, 60.0 + k],
            [-5.0, -5.0],
            &[
                ("axis", [0.0, 0.0, k], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
                ("drive", [0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
            ],
        ),
        build: Some(json!({
            "role": "fastener", "wood": {"kind": kind, "head": head,
                "diameter": 5, "length": 60, "seat": seat},
            "frames": [{"name": "axis", "kind": "shaft"}, {"name": "drive", "kind": "seat"}],
        })),
        part_number: String::new(),
        name: format!("{head} {kind}"),
        designation: format!("{head} {kind}"),
    }
}

/// A shaft just 1e-5 radians from +Z: its cosine alone looks aligned at
/// 1e-9 precision, while a 10 mm head's support plane differs by 5e-5 mm.
fn oblique_fastener(seat: Option<&str>, matching_seat: bool) -> Part {
    let z = [1e-5, 0.0, (1.0_f64 - 1e-10).sqrt()];
    Part {
        text: block_design(
            [10.0, 10.0, 63.0],
            [-5.0, -5.0],
            &[
                ("axis", [0.0, 0.0, 3.0], z, [0.0, 1.0, 0.0]),
                (
                    "drive",
                    [0.0; 3],
                    if matching_seat { z } else { [0.0, 0.0, 1.0] },
                    [0.0, 1.0, 0.0],
                ),
            ],
        ),
        build: Some(json!({
            "role": "fastener", "wood": {"kind": "screw", "head": "countersunk",
                "diameter": 5, "length": 60, "seat": seat},
            "frames": [{"name": "axis", "kind": "shaft"}, {"name": "drive", "kind": "seat"}],
        })),
        part_number: String::new(),
        name: "Oblique screw".to_owned(),
        designation: "Oblique screw".to_owned(),
    }
}

/// A ring nail 3.1 x 90 (rows 2.8x50 and 3.1x90, mitcad#497): its `axis`
/// frame at the head, z along the shank.
fn ring_nail() -> Part {
    let mut doc: Document<MockKernel> = Document::from_json(
        &block_design(
            [3.0, 3.0, 90.0],
            [-1.5, -1.5],
            &[("axis", [0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0])],
        ),
        MockKernel::default(),
    )
    .unwrap();
    let names = ["2.8x50", "3.1x90"];
    let rows: Vec<Value> = names.iter().map(|r| json!({"name": r})).collect();
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": {
            "parameters": [], "default": "3.1x90", "rows": rows}}),
    )
    .unwrap();
    let mut by_row = serde_json::Map::new();
    for name in names {
        let (d, l) = name.split_once('x').unwrap();
        by_row.insert(
            name.to_owned(),
            json!({"wood": {"diameter": d.parse::<f64>().unwrap(),
                            "length": l.parse::<f64>().unwrap()}}),
        );
    }
    Part {
        text: doc.to_json(),
        build: Some(json!({
            "role": "fastener",
            "wood": {"kind": "nail", "diameter": "3.1 mm", "length": "90 mm"},
            "frames": [{"name": "axis", "kind": "shaft"}],
            "rows": by_row,
        })),
        part_number: String::new(),
        name: "Ring nail".to_owned(),
        designation: "Ring nail {config}".to_owned(),
    }
}

/// A joist hanger for an `h` x `t` joist: its `mount` frame at the joist's
/// end middle (z into the carrier, x up), two hole frames on its flanges.
fn hanger(h: f64, t: f64) -> Part {
    let flange = t / 2.0 + 2.0;
    Part {
        text: block_design(
            [h + 4.0, t + 4.0, 60.0],
            [-h / 2.0 - 2.0, -t / 2.0 - 2.0],
            &[
                ("mount", [0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
                ("h1", [0.0, flange, -30.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
                (
                    "h2",
                    [0.0, -flange, -30.0],
                    [0.0, -1.0, 0.0],
                    [1.0, 0.0, 0.0],
                ),
            ],
        ),
        build: Some(json!({
            "role": "connector",
            "frames": [{"name": "mount", "kind": "face"}, {"name": "h1", "kind": "hole"},
                       {"name": "h2", "kind": "hole"}],
            "connector": {
                "placement": "outer",
                "junctions": ["T", "L"],
                "fit": {"kind": "hanger", "section": [t, h]},
                "mates": [{"frame": "mount", "to": "face"}],
                "fasteners": [{"frames": ["h1", "h2"], "count": 2, "on": "a",
                               "screw": {"library": LIBRARY, "component": "wood-screw",
                                         "config": "4x40"}}]
            },
            "material": {"product": "galvanised steel", "density": 7850}
        })),
        part_number: "JH148".to_owned(),
        name: format!("Joist hanger {t}x{h}"),
        designation: String::new(),
    }
}

/// A post base for a 98 x 98 post: its `mount` frame at the post end's
/// middle, z into the post.
fn post_base() -> Part {
    Part {
        text: block_design(
            [110.0, 110.0, 40.0],
            [-55.0, -55.0],
            &[("mount", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0])],
        ),
        build: Some(json!({
            "role": "connector",
            "frames": [{"name": "mount", "kind": "face"}],
            "connector": {"placement": "outer", "fit": {"kind": "post_base", "section": [98, 98]},
                          "mates": [{"frame": "mount", "to": "end"}]}
        })),
        part_number: "PB98".to_owned(),
        name: "Post base 98".to_owned(),
        designation: String::new(),
    }
}

/// A profile of no slot system whose rows switch a feature by a condition
/// (mitcad#873, as a steel tube's rows switch its bores): 40 x 40 x `L`,
/// and a second body beside it (x from 20 to 60), which the condition
/// `S < 1 mm` suppresses in the default row `narrow` (S 0) and not in the
/// row `wide` (S 10).
fn bound_rows() -> Part {
    let mut doc: Document<MockKernel> =
        Document::from_json(&board_design(40.0, 40.0, &[]), MockKernel::default()).unwrap();
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "S", "expression": "0 mm"}),
    )
    .unwrap();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(
            sketch,
            [20.0, -20.0],
            &ValueInput::Number(40.0),
            &ValueInput::Number(40.0),
        )
        .unwrap()
        .region;
    let extra = doc
        .add_feature(
            &def(
                json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                        "extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}),
            ),
            Some("Extra"),
        )
        .unwrap()
        .uid;
    command(
        &mut doc,
        json!({"cmd": "suppress_feature", "uid": extra, "expression": "S < 1 mm"}),
    )
    .unwrap();
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": {
            "parameters": ["S"], "default": "narrow",
            "rows": [{"name": "narrow", "values": {"S": "0 mm"}},
                     {"name": "wide", "values": {"S": "10 mm"}}]}}),
    )
    .unwrap();
    doc.recompute();
    Part {
        text: doc.to_json(),
        build: Some(json!({
            "role": "profile",
            "section": {"width": "40 mm", "height": "40 mm"},
            "mass_per_metre": "1 kg",
            "length": {"parameter": "L", "min": "10 mm", "max": "6000 mm"},
            "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
                       {"name": "face_xp", "kind": "face"}, {"name": "face_xn", "kind": "face"},
                       {"name": "face_yp", "kind": "face"}, {"name": "face_yn", "kind": "face"}],
        })),
        part_number: String::new(),
        name: "Bound rows".to_owned(),
        designation: "Bound rows {config} L={L}".to_owned(),
    }
}

/// A square steel tube 40 x 40 x `L` (3000 mm; mitcad#873): a profile of
/// no slot system and no timber kind, with face frames on its four sides.
fn tube40() -> Part {
    Part {
        text: board_design(40.0, 40.0, &[]),
        build: Some(json!({
            "role": "profile",
            "section": {"width": "40 mm", "height": "40 mm"},
            "mass_per_metre": "3.303 kg",
            "material": {"density": 7850},
            "length": {"parameter": "L", "min": "10 mm", "max": "6000 mm", "stock": [6000]},
            "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
                       {"name": "face_xp", "kind": "face"}, {"name": "face_xn", "kind": "face"},
                       {"name": "face_yp", "kind": "face"}, {"name": "face_yn", "kind": "face"}],
        })),
        part_number: String::new(),
        name: "SHS 40x40x3".to_owned(),
        designation: "SHS 40x40x3 L={L}".to_owned(),
    }
}

/// A plywood sheet `W` x `H` x `t` lying on a face.
fn plywood() -> Part {
    Part {
        text: panel_design(None),
        build: Some(json!({"role": "panel",
                           "panel": {"mount": "face", "thickness": "t", "width": "W", "height": "H",
                                     "stock": [[1200, 2400]]},
                           "material": {"product": "plywood", "density": 600}})),
        part_number: String::new(),
        name: "Plywood".to_owned(),
        designation: "Plywood {W}x{H}".to_owned(),
    }
}

impl LinkResolver for TestLibrary {
    fn library_source(&self, request: &LibraryRequest) -> Result<LibrarySource, String> {
        let rev = match request.rev.as_str() {
            "v1.0.0" | "HEAD" | "" => V1,
            "v1.1.0" => V2,
            other => other,
        };
        let part = self
            .parts
            .get(&(request.component.clone(), rev.to_owned()))
            .cloned()
            .ok_or_else(|| format!("{} is not in the library at {rev}", request.component))?;
        Ok(LibrarySource {
            text: part.text,
            store: MemoryStore::new(),
            rev: rev.to_owned(),
            label: if rev == V1 { "v1.0.0" } else { "v1.1.0" }.to_owned(),
            library: request.library.clone(),
            url: URL.to_owned(),
            library_name: "Test profiles".to_owned(),
            component: request.component.clone(),
            authors: vec!["Profile makers".to_owned()],
            path: format!("parts/{}.mitcad", request.component),
            name: part.name,
            standard: String::new(),
            license: "MIT".to_owned(),
            designation: part.designation,
            supplier: Some("Test supplier".to_owned()),
            part_number: Some(part.part_number).filter(|p| !p.is_empty()),
            supplier_url: Some("https://example.org/shop".to_owned()),
            build: part.build,
            rows: Default::default(),
        })
    }
}

/// A design with the test library set up.
pub fn design() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(TestLibrary::new()));
    doc
}

/// A library part input of the test library.
pub fn part(component: &str) -> Value {
    json!({"library": LIBRARY, "url": URL, "rev": "v1.0.0", "component": component})
}

/// Places a part; returns the answer.
pub fn place(doc: &mut Document<MockKernel>, fields: Value) -> Value {
    let mut command_value = fields;
    command_value["cmd"] = json!("build_place");
    command(doc, command_value).unwrap_or_else(|e| panic!("build_place: {e}"))
}

pub fn component(answer: &Value) -> ComponentUid {
    answer["component"].as_str().unwrap().parse().unwrap()
}

pub fn occurrence(answer: &Value) -> OccurrenceUid {
    answer["occurrence"].as_str().unwrap().parse().unwrap()
}

/// The derived part of a component.
pub fn derived(doc: &Document<MockKernel>, component: ComponentUid) -> FeatureUid {
    doc.assembly()
        .component(component)
        .and_then(|c| c.build.as_ref())
        .and_then(|b| b.part)
        .unwrap()
}

pub fn close(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6)
}

/// An occurrence's placement at the marker.
pub fn placed(doc: &Document<MockKernel>, occurrence: OccurrenceUid) -> Transform {
    doc.placement(occurrence).unwrap()
}

/// Asserts that every feature succeeded.
pub fn all_ok(doc: &Document<MockKernel>) {
    for item in doc.timeline() {
        assert!(
            item.status.is_some_and(|s| s.is_ok()),
            "{} ({}): {:?} {:?}",
            item.entry.name,
            item.entry.uid,
            item.status,
            item.message()
        );
    }
}

/// A grounded first profile at a point, along +Z.
pub fn first(doc: &mut Document<MockKernel>, component: &str, at: [f64; 3], length: f64) -> Value {
    place(
        doc,
        json!({"part": part(component), "at": {"point": at}, "length": length}),
    )
}

/// A profiled board of the test library: `w` its full width (along X), `t`
/// its thickness, its `board` block, and for one that joins its `groove`
/// (at -w/2, z -X) and `tongue` (the cover width on from it, z +X) frames.
fn profiled(name: &str, w: f64, t: f64, board: Value) -> Part {
    let joined = matches!(
        board["profile"].as_str(),
        Some("tongue_and_groove" | "shiplap" | "feather_edge")
    );
    let mut doc: Document<MockKernel> =
        Document::from_json(&board_design(w, t, &[]), MockKernel::default()).unwrap();
    let mut build = board_build(w, t, "planed");
    build["nominal"] = json!({"width": w, "height": t});
    build["board"] = board.clone();
    if joined {
        let cover = board["cover"].as_f64().unwrap();
        fixed_frame(
            &mut doc,
            "groove",
            [-w / 2.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
        );
        fixed_frame(
            &mut doc,
            "tongue",
            [-w / 2.0 + cover, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
        );
        doc.recompute();
        let frames = build["frames"].as_array_mut().unwrap();
        frames.push(json!({"name": "groove", "kind": "groove"}));
        frames.push(json!({"name": "tongue", "kind": "tongue"}));
    }
    Part {
        text: doc.to_json(),
        build: Some(build),
        part_number: String::new(),
        name: name.to_owned(),
        designation: format!("{name} L={{L}}"),
    }
}

/// The test library's profiled boards.
pub fn boards() -> Vec<(&'static str, Part)> {
    vec![
        // A floorboard 28 x 120: a tongue 8 long and 8 thick, its groove 9
        // deep and 8.5 wide: 112 x 28 + 8 x 8 - 9 x 8.5.
        (
            "tg120",
            profiled(
                "Floorboard 28x120",
                120.0,
                28.0,
                json!({"profile": "tongue_and_groove", "cover": 112.0, "area": 3123.5,
                       "tongue": {"length": 8, "thickness": 8}}),
            ),
        ),
        // The same with a thinner tongue: it does not join the first.
        (
            "tg120-thin",
            profiled(
                "Floorboard 28x120 thin tongue",
                120.0,
                28.0,
                json!({"profile": "tongue_and_groove", "cover": 112.0, "area": 3127.0,
                       "tongue": {"length": 8, "thickness": 6}}),
            ),
        ),
        // Shiplap 22 x 145, overlapping by 12, its lip 9 thick: 145 x 22
        // less 12 x 13 at the front of the lip and 12 x 9 behind the overlap.
        (
            "shiplap145",
            profiled(
                "Shiplap 22x145",
                145.0,
                22.0,
                json!({"profile": "shiplap", "cover": 133.0, "area": 2926.0, "overlap": 12}),
            ),
        ),
        // Feather-edge 22/6 x 150, overlapping by 30: (22 + 6) / 2 x 150.
        (
            "feather150",
            profiled(
                "Feather-edge 22x150",
                150.0,
                22.0,
                json!({"profile": "feather_edge", "cover": 120.352, "area": 2100.0,
                       "overlap": 30}),
            ),
        ),
        // A deck board 28 x 120 with rounded edges (radius 5) and seven
        // grooves 5 x 3: 120 x 28 less the corners and the grooves.
        (
            "deck-grooved",
            profiled(
                "Deck board grooved 28x120",
                120.0,
                28.0,
                json!({"profile": "deck", "area": 3233.54, "edge_radius": 5, "grooves": 7}),
            ),
        ),
    ]
}
