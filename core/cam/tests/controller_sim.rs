// SPDX-License-Identifier: MIT
//! The posts' G-code run through the controllers' own code before it ships
//! (docs/cam.md, "Simulation and checks"): every program is posted, run
//! through a simulator built from the controller's source, and checked
//! line by line (every line accepted, no alarm, no realtime command
//! character in the stream) and for its path (the final position and the
//! extents of the tool's path against the program's).
//!
//! - `grbl_sim`: grbl 1.1's protocol loop, G-code parser, arcs and planner
//!   on the host (`MITCAD_GRBL_SIM`, a trace program built beside
//!   grbl-sim's validator from the same objects): what grbl answers to
//!   each line and each motion block its planner is given, in machine and
//!   work coordinates. It takes the program's file and prints a record per
//!   line: `R <line> <answer>`, `B <line> <rapid 0|1> <mx> <my> <mz> <wx>
//!   <wy> <wz>` (mm), `P <line>` (a pause, resumed at once) and
//!   `T <line> <hex>` (a realtime character taken out of the stream).
//!   grbl-sim's own simulator steps in real time and has no flow control
//!   from a file, so it is not used.
//! - `grblhal_sim`: grblHAL's simulator (`MITCAD_GRBLHAL_SIM`), streamed
//!   over its telnet port as a sender would, the operator's cycle start
//!   sent when a tool change or a pause holds the program; the path is its
//!   planner's blocks in steps, the final position its own status report.
//!   `MITCAD_SIM_DEBUG` set prints the exchange.
//!
//! Both simulators are GPL programs run as separate processes, never
//! linked; the tests are ignored in `core.cam` and run by the ctests
//! `core.cam_grbl_sim` and `core.cam_grblhal_sim`, which print "simulator:
//! skipped" (and are reported as skipped) without the variable.
//!
//! Resolution: the G-code has three decimals in millimetres and four in
//! inches; grbl computes in 32-bit floats; grblHAL's path is in steps
//! (1000 per mm here) and its status reports have three decimals; grbl
//! and grblHAL split arcs into chords within their arc tolerance ($12,
//! 0.002 mm). The comparisons allow these, nothing more.

use std::f64::consts::{FRAC_PI_2, TAU};
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use mitcad_cam::machine::{Bounds, MacroChanger, RackSlot, Spindle};
use mitcad_cam::ops::{
    self, Adaptive, ContourOp, Depths, Direction, DrillOp, Entry, FaceOp, Heights, Lead, PocketOp,
    RestOp, Side, TabLayout, Tabs,
};
use mitcad_cam::toolpath::arc_sweep;
use mitcad_cam::{
    Contour, Coolant, Cutting, Machine, Move, PostSettings, Program, ProgramOp, Region,
    SpindleDirection, Tool, ToolChanger, Toolpath, Units, dialect, post,
};

type P3 = [f64; 3];

/// The arc tolerance grbl and grblHAL split arcs with by default ($12).
const CONTROLLER_ARC_TOLERANCE: f64 = 0.002;

/// The simulator's path from its variable, or `None` (with the skip
/// message ctest looks for) when it is not set.
fn simulator(var: &str) -> Option<PathBuf> {
    match std::env::var_os(var) {
        Some(p) if !p.is_empty() => {
            let p = PathBuf::from(p);
            assert!(p.is_file(), "{var} names no file: {}", p.display());
            Some(p)
        }
        _ => {
            println!("{var} is not set: the controller simulator: skipped");
            None
        }
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("controller-sim");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

// ---------------------------------------------------------------------------
// The programs.

fn cutting(rpm: f64, feed: f64, plunge: f64) -> Cutting {
    Cutting {
        spindle_rpm: rpm,
        feed,
        plunge_feed: plunge,
    }
}

fn op(name: &str, tool: &Tool, rpm: f64, coolant: Coolant, toolpath: Toolpath) -> ProgramOp {
    ProgramOp {
        name: name.into(),
        tool: tool.clone(),
        spindle_rpm: rpm,
        spindle: SpindleDirection::Cw,
        coolant,
        toolpath,
    }
}

fn moves(moves: Vec<Move>) -> Toolpath {
    Toolpath {
        moves,
        cycles: vec![],
    }
}

/// The post tests' two operations: a circle cut with tool 1 and a hole
/// with tool 2 under flood coolant.
fn bracket() -> Program {
    let mut p = Program::new("Bracket", 15.0);
    p.operations.push(op(
        "Contour 1",
        &Tool::flat_end_mill(1, 6.0),
        18000.0,
        Coolant::Off,
        moves(vec![
            Move::Rapid {
                to: [10.0, 0.0, 5.0],
            },
            Move::Line {
                to: [10.0, 0.0, -2.0],
                feed: 300.0,
            },
            Move::Arc {
                to: [-10.0, 0.0, -2.0],
                center: [0.0, 0.0],
                ccw: true,
                feed: 1000.0,
            },
            Move::Arc {
                to: [10.0, 0.0, -2.0],
                center: [0.0, 0.0],
                ccw: true,
                feed: 1000.0,
            },
            Move::Line {
                to: [20.0, 0.0, -2.0],
                feed: 1000.0,
            },
            Move::Rapid {
                to: [20.0, 0.0, 15.0],
            },
        ]),
    ));
    p.operations.push(op(
        "Drill",
        &Tool::flat_end_mill(2, 3.0),
        10000.0,
        Coolant::Flood,
        moves(vec![
            Move::Rapid {
                to: [5.0, 5.0, 15.0],
            },
            Move::Rapid {
                to: [5.0, 5.0, 2.0],
            },
            Move::Line {
                to: [5.0, 5.0, -5.0],
                feed: 200.0,
            },
            Move::Rapid {
                to: [5.0, 5.0, 15.0],
            },
        ]),
    ));
    p
}

/// A plate from outlines to G-code: facing, the outline cut out, a pocket
/// round an island and two holes, with three tools.
fn plate() -> Program {
    let t1 = Tool::flat_end_mill(1, 6.0);
    let t2 = Tool::flat_end_mill(2, 3.0);
    let t3 = Tool::flat_end_mill(3, 2.5);
    let heights = Heights {
        clearance: 10.0,
        retract: 2.0,
    };
    let face = ops::face(
        &FaceOp {
            min: [0.0, 0.0],
            max: [60.0, 40.0],
            depths: Depths {
                top: 0.5,
                bottom: 0.0,
                step_down: 0.5,
            },
            heights,
            stepover: None,
            cutting: cutting(18000.0, 1500.0, 300.0),
            direction: Direction::Climb,
            adaptive: None,
        },
        &t1,
    )
    .unwrap();
    let outline = ops::contour(
        &ContourOp {
            contours: vec![Contour::rectangle([0.0, 0.0], [60.0, 40.0])],
            side: Side::Outside,
            direction: Direction::Climb,
            depths: Depths {
                top: 0.0,
                bottom: -8.0,
                step_down: 4.0,
            },
            heights,
            stock_to_leave: 0.0,
            cutting: cutting(18000.0, 1200.0, 300.0),
            entry: Entry::Plunge,
            tabs: None,
            lead: None,
        },
        &t1,
    )
    .unwrap();
    let pocket = ops::pocket(
        &PocketOp {
            region: Region::new(
                Contour::rectangle([8.0, 8.0], [38.0, 32.0]),
                vec![Contour::circle([23.0, 20.0], 3.0)],
            ),
            stepover: None,
            direction: Direction::Climb,
            depths: Depths {
                top: 0.0,
                bottom: -3.0,
                step_down: 1.5,
            },
            heights,
            stock_to_leave: 0.0,
            cutting: cutting(20000.0, 900.0, 250.0),
            entry: Entry::Plunge,
            adaptive: None,
        },
        &t2,
    )
    .unwrap();
    let holes = ops::drill(
        &DrillOp {
            points: vec![[48.0, 10.0], [48.0, 30.0]],
            depths: Depths {
                top: 0.0,
                bottom: -6.0,
                step_down: 2.0,
            },
            heights,
            peck: true,
            chip_break: false,
            canned_cycle: true,
            cutting: cutting(12000.0, 400.0, 150.0),
        },
        &t3,
    )
    .unwrap();
    let mut p = Program::new("Plate", 15.0);
    p.operations
        .push(op("Face 1", &t1, 18000.0, Coolant::Off, face));
    p.operations
        .push(op("Pocket 1", &t2, 20000.0, Coolant::Flood, pocket));
    p.operations
        .push(op("Holes", &t3, 12000.0, Coolant::Flood, holes));
    p.operations
        .push(op("Contour 1", &t1, 18000.0, Coolant::Off, outline));
    p
}

/// A plate with the other ways of cutting: adaptive facing, a pocket
/// entered by a helix and its corners cleaned up after by a smaller tool
/// (rest), an adaptive pocket, plain and chip-breaking holes (both canned
/// cycles where asked), and the outline entered by a ramp with leads and
/// holding tabs; five tools.
fn plate_of_many_ways() -> Program {
    let t1 = Tool::flat_end_mill(1, 6.0);
    let t2 = Tool::flat_end_mill(2, 4.0);
    let t3 = Tool::flat_end_mill(3, 2.5);
    let t4 = Tool::flat_end_mill(4, 1.5);
    let heights = Heights {
        clearance: 10.0,
        retract: 2.0,
    };
    let face = ops::face(
        &FaceOp {
            min: [0.0, 0.0],
            max: [70.0, 40.0],
            depths: Depths {
                top: 0.5,
                bottom: 0.0,
                step_down: 0.5,
            },
            heights,
            stepover: None,
            cutting: cutting(18000.0, 1500.0, 300.0),
            direction: Direction::Climb,
            adaptive: Some(Adaptive::default()),
        },
        &t1,
    )
    .unwrap_or_else(|e| panic!("adaptive facing: {}", e.message));
    let square = Region::new(Contour::rectangle([6.0, 6.0], [30.0, 30.0]), vec![]);
    let depths = Depths {
        top: 0.0,
        bottom: -3.0,
        step_down: 1.5,
    };
    let pocket = ops::pocket(
        &PocketOp {
            region: square.clone(),
            stepover: None,
            direction: Direction::Climb,
            depths,
            heights,
            stock_to_leave: 0.0,
            cutting: cutting(20000.0, 900.0, 250.0),
            entry: Entry::Helix {
                radius: 1.5,
                angle: 3.0,
            },
            adaptive: None,
        },
        &t2,
    )
    .unwrap_or_else(|e| panic!("helix pocket: {}", e.message));
    let rest = ops::rest(
        &RestOp {
            region: square,
            previous_diameter: t2.diameter,
            previous_stock_to_leave: 0.0,
            direction: Direction::Climb,
            depths,
            heights,
            stock_to_leave: 0.0,
            cutting: cutting(24000.0, 500.0, 150.0),
            entry: Entry::Plunge,
        },
        &t4,
    )
    .unwrap_or_else(|e| panic!("rest: {}", e.message));
    let adaptive = ops::pocket(
        &PocketOp {
            region: Region::new(Contour::rectangle([36.0, 6.0], [64.0, 24.0]), vec![]),
            stepover: None,
            direction: Direction::Climb,
            depths,
            heights,
            stock_to_leave: 0.0,
            cutting: cutting(20000.0, 1200.0, 250.0),
            entry: Entry::Helix {
                radius: 1.5,
                angle: 3.0,
            },
            adaptive: Some(Adaptive::default()),
        },
        &t2,
    )
    .unwrap_or_else(|e| panic!("adaptive pocket: {}", e.message));
    let drill = |points: Vec<[f64; 2]>, peck: bool| {
        ops::drill(
            &DrillOp {
                points,
                depths: Depths {
                    top: 0.0,
                    bottom: -6.0,
                    step_down: 2.0,
                },
                heights,
                peck,
                chip_break: peck,
                canned_cycle: true,
                cutting: cutting(12000.0, 400.0, 150.0),
            },
            &t3,
        )
        .unwrap_or_else(|e| panic!("drilling: {}", e.message))
    };
    let holes = drill(vec![[40.0, 32.0], [50.0, 32.0]], false);
    let pecks = drill(vec![[60.0, 32.0]], true);
    let outline = ops::contour(
        &ContourOp {
            contours: vec![Contour::rectangle([0.0, 0.0], [70.0, 40.0])],
            side: Side::Outside,
            direction: Direction::Climb,
            depths: Depths {
                top: 0.0,
                bottom: -8.0,
                step_down: 4.0,
            },
            heights,
            stock_to_leave: 0.0,
            cutting: cutting(18000.0, 1200.0, 300.0),
            entry: Entry::Ramp { angle: 5.0 },
            tabs: Some(Tabs {
                layout: TabLayout::Count { count: 4 },
                width: 6.0,
                height: 2.0,
            }),
            lead: Some(Lead {
                radius: 4.0,
                length: 2.0,
            }),
        },
        &t1,
    )
    .unwrap_or_else(|e| panic!("outline: {}", e.message));
    let mut p = Program::new("Plate of many ways", 15.0);
    for (name, tool, path) in [
        ("Adaptive face", &t1, face),
        ("Helix pocket", &t2, pocket),
        ("Adaptive pocket", &t2, adaptive),
        ("Rest", &t4, rest),
        ("Holes", &t3, holes),
        ("Pecked hole", &t3, pecks),
        ("Outline with tabs", &t1, outline),
    ] {
        p.operations
            .push(op(name, tool, 18000.0, Coolant::Flood, path));
    }
    p
}

fn machine(changer: ToolChanger, park: Option<P3>) -> Machine {
    Machine {
        name: "Router".into(),
        controller: "grbl".into(),
        travel: Bounds {
            min: [0.0, 0.0, -150.0],
            max: [800.0, 800.0, 0.0],
        },
        spindle: Spindle {
            min_rpm: 8000.0,
            max_rpm: 24000.0,
            spin_up_seconds: 1.5,
            nose_diameter: 40.0,
            nose_length: 60.0,
        },
        max_feed: 5000.0,
        rapid_feed: 8000.0,
        zones: vec![],
        tool_changer: changer,
        park,
    }
}

/// A rack of three slots driven by the program, every line one grbl has.
fn rack() -> MacroChanger {
    MacroChanger {
        slots: (1..=3)
            .map(|n| RackSlot {
                tool: n,
                position: [20.0 * n as f64, 780.0, -60.0],
            })
            .collect(),
        safe_z: -5.0,
        unload: vec![
            "G53 G0 Z{safe_z}".into(),
            "G53 G0 X{x} Y{y}".into(),
            "G53 G1 Z{z} F500".into(),
            "G4 P0.5".into(),
            "G53 G0 Z{safe_z}".into(),
        ],
        load: vec![
            "G53 G0 X{x} Y{y}".into(),
            "G53 G1 Z{z} F500".into(),
            "G4 P0.5".into(),
            "G53 G0 Z{safe_z}".into(),
        ],
    }
}

/// One program posted one way.
struct Case {
    name: &'static str,
    program: Program,
    settings: PostSettings,
    machine: Option<Machine>,
    /// Places the program goes in machine coordinates besides its moves
    /// (the rack, the park), with zero work offsets.
    extra: Vec<P3>,
    /// The final position in machine coordinates, when it is not the safe
    /// height above the last move.
    end: Option<P3>,
}

fn case(name: &'static str, program: Program) -> Case {
    Case {
        name,
        program,
        settings: PostSettings::default(),
        machine: None,
        extra: vec![],
        end: None,
    }
}

/// Every way of posting the tests run through the simulators.
fn cases() -> Vec<Case> {
    let mut v = vec![
        case("bracket", bracket()),
        case("plate", plate()),
        case("plate of many ways", plate_of_many_ways()),
    ];
    let mut c = case("plate of many ways in inches", plate_of_many_ways());
    c.settings.units = Units::Inches;
    v.push(c);
    let mut c = case("plate in inches", plate());
    c.settings.units = Units::Inches;
    v.push(c);
    let mut c = case("bracket without arcs", bracket());
    c.settings.arcs = false;
    v.push(c);
    let mut c = case("bracket with line numbers and no comments", bracket());
    c.settings.line_numbers = true;
    c.settings.comments = false;
    v.push(c);
    // Names with characters grbl acts on at once wherever they are in the
    // stream (a status report, a feed hold, a cycle start) and ones
    // outside ASCII.
    let mut p = bracket();
    p.name = "Bracket! rev ~2?".into();
    p.operations[0].name = "Contour ø6 (outside)!".into();
    for o in &mut p.operations {
        o.tool.name = format!("{} ~ Ø{}? «new»!", o.tool.name, o.tool.diameter);
    }
    v.push(case("bracket with names a controller acts on", p));
    let mut c = case("plate with a spin-up dwell", plate());
    c.machine = Some(machine(ToolChanger::Manual, None));
    v.push(c);
    let mut c = case("plate from a rack, parked", plate());
    let rack = rack();
    let park = [5.0, 790.0, -2.0];
    c.extra = rack.slots.iter().map(|s| s.position).collect();
    c.extra.push([20.0, 780.0, rack.safe_z]);
    c.extra.push(park);
    c.end = Some(park);
    c.machine = Some(machine(ToolChanger::Macro(rack), Some(park)));
    v.push(c);
    v
}

// ---------------------------------------------------------------------------
// The program's own path.

/// The axis extremes an arc passes between its ends.
fn arc_extremes(from: [f64; 2], to: [f64; 2], c: [f64; 2], ccw: bool) -> Vec<[f64; 2]> {
    let r = (from[0] - c[0]).hypot(from[1] - c[1]);
    let a0 = (from[1] - c[1]).atan2(from[0] - c[0]);
    let sweep = arc_sweep(from, to, c, ccw);
    (0..4)
        .filter_map(|k| {
            let th = k as f64 * FRAC_PI_2;
            let inside = if sweep > 0.0 {
                (th - a0).rem_euclid(TAU) <= sweep
            } else {
                (a0 - th).rem_euclid(TAU) <= -sweep
            };
            inside.then(|| [c[0] + r * th.cos(), c[1] + r * th.sin()])
        })
        .collect()
}

/// The extents and the end of a program's path in work coordinates: from
/// the work zero up to the safe height, every move (arcs with their
/// extremes), and the extra places.
struct Expected {
    min: P3,
    max: P3,
    end: P3,
}

fn expected(c: &Case) -> Expected {
    let p = &c.program;
    let mut min = [0.0, 0.0, 0.0f64.min(p.safe_z)];
    let mut max = [0.0, 0.0, 0.0f64.max(p.safe_z)];
    let mut grow = |q: P3| {
        for k in 0..3 {
            min[k] = min[k].min(q[k]);
            max[k] = max[k].max(q[k]);
        }
    };
    let mut at: P3 = [0.0, 0.0, p.safe_z];
    for o in &p.operations {
        for m in &o.toolpath.moves {
            if let Move::Arc {
                to, center, ccw, ..
            } = *m
            {
                for e in arc_extremes([at[0], at[1]], [to[0], to[1]], center, ccw) {
                    grow([e[0], e[1], to[2]]);
                }
            }
            grow(m.to());
            at = m.to();
        }
    }
    for &q in &c.extra {
        grow(q);
    }
    Expected {
        min,
        max,
        end: c.end.unwrap_or([at[0], at[1], p.safe_z]),
    }
}

// ---------------------------------------------------------------------------
// What a simulator made of a program.

#[derive(Default)]
struct Run {
    /// The program's lines.
    lines: Vec<String>,
    /// The controller's answer to each line (`ok`, `error:N`), `None`
    /// when it gave none.
    answers: Vec<Option<String>>,
    /// Alarms and messages other than the program end.
    alarms: Vec<String>,
    /// Realtime command characters found in the stream, by line.
    realtime: Vec<(usize, u8)>,
    /// How many times the program held for the operator.
    holds: usize,
    /// The tool's path (mm): the planner's targets in order.
    path: Vec<P3>,
    /// Where the tool ends (mm).
    end: Option<P3>,
    /// How far the path's numbers may be off the program's, beyond the
    /// G-code's own rounding (mm).
    resolution: f64,
}

impl Run {
    /// The lines the controller refused, with its error.
    fn refused(&self) -> Vec<String> {
        self.lines
            .iter()
            .zip(&self.answers)
            .filter(|(_, a)| a.as_deref() != Some("ok"))
            .map(|(l, a)| format!("{l} -> {}", a.as_deref().unwrap_or("no answer")))
            .collect()
    }

    /// The constructs of the refused lines: their words without line
    /// numbers, G and M codes as they are and other words by their letter
    /// (`T1 M6` is `T M6`, `G43 H2` is `G43 H`), sorted, each once.
    fn refused_constructs(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .lines
            .iter()
            .zip(&self.answers)
            .filter(|(_, a)| a.as_deref() != Some("ok"))
            .map(|(l, _)| {
                // Quoted messages are not words.
                let unquoted: String = l.split('"').step_by(2).collect::<Vec<_>>().join("");
                unquoted
                    .split_whitespace()
                    .filter(|w| !w.starts_with('N'))
                    .map(|w| {
                        if w.starts_with(['G', 'M']) || !w.starts_with(char::is_alphabetic) {
                            w.to_string()
                        } else {
                            w[..1].to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

/// The G-code's own rounding: half its last digit (mm).
fn rounding(settings: &PostSettings) -> f64 {
    match settings.units {
        Units::Millimetres => 0.0005,
        Units::Inches => 0.00005 * 25.4,
    }
}

/// Checks the path against the program: the end within the resolution,
/// the extents within it and the arcs' chords.
fn check_path(run: &Run, c: &Case, what: &str) -> Result<(), String> {
    let e = expected(c);
    let tol = rounding(&c.settings) + run.resolution;
    let chord = if c.settings.arcs {
        CONTROLLER_ARC_TOLERANCE
    } else {
        c.settings.arc_tolerance + CONTROLLER_ARC_TOLERANCE
    };
    let Some(end) = run.end else {
        return Err(format!("{what}: no position"));
    };
    let mut problems = Vec::new();
    for k in 0..3 {
        if (end[k] - e.end[k]).abs() > tol {
            problems.push(format!(
                "ends at {end:?}, the program at {:?} (axis {k})",
                e.end
            ));
        }
    }
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    // The path starts at the work zero.
    for q in std::iter::once([0.0; 3]).chain(run.path.iter().copied()) {
        for k in 0..3 {
            min[k] = min[k].min(q[k]);
            max[k] = max[k].max(q[k]);
        }
    }
    for k in 0..3 {
        if !(min[k] >= e.min[k] - tol && min[k] <= e.min[k] + tol + chord) {
            problems.push(format!(
                "axis {k} goes down to {}, the program to {}",
                min[k], e.min[k]
            ));
        }
        if !(max[k] <= e.max[k] + tol && max[k] >= e.max[k] - tol - chord) {
            problems.push(format!(
                "axis {k} goes up to {}, the program to {}",
                max[k], e.max[k]
            ));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("{what}: {}", problems.join("; ")))
    }
}

/// Checks that the controller took every line and reports what it did not.
fn check_lines(run: &Run, what: &str) -> Result<(), String> {
    let mut problems = run.refused();
    problems.extend(run.alarms.iter().cloned());
    problems.extend(
        run.realtime
            .iter()
            .map(|(n, c)| format!("realtime command 0x{c:02x} in line {n}")),
    );
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("{what}:\n  {}", problems.join("\n  ")))
    }
}

fn post_case(c: &Case, id: &str) -> String {
    let d = dialect(id).expect("a dialect");
    post(&c.program, d.as_ref(), &c.settings, c.machine.as_ref())
        .unwrap_or_else(|e| panic!("{id}, {}: {}", c.name, e.message))
}

/// The number of tool changes after the first.
fn later_changes(p: &Program) -> usize {
    p.operations
        .windows(2)
        .filter(|w| w[0].tool.number != w[1].tool.number)
        .count()
}

/// Every check failure of all cases at once, so that one run shows them.
fn report(failures: Vec<String>) {
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}

// ---------------------------------------------------------------------------
// grbl 1.1 through its own code on the host.

mod grbl_sim {
    use super::*;

    /// Runs the G-code through the grbl trace program.
    pub(crate) fn run(sim: &PathBuf, name: &str, gcode: &str) -> Run {
        let file = scratch(&format!("grbl-{}.nc", name.replace(' ', "-")));
        std::fs::write(&file, gcode).unwrap();
        let out = Command::new(sim)
            .arg(&file)
            .stdin(Stdio::null())
            .output()
            .expect("the grbl simulator runs");
        assert!(
            out.status.success(),
            "the grbl simulator failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let lines: Vec<String> = gcode.lines().map(str::to_string).collect();
        let mut run = Run {
            answers: vec![None; lines.len()],
            lines,
            // grbl computes in 32-bit floats: a few units in the last
            // place of numbers up to a metre.
            resolution: 1e-4,
            ..Run::default()
        };
        for l in String::from_utf8_lossy(&out.stdout).lines() {
            let mut w = l.splitn(3, ' ');
            let kind = w.next().unwrap_or("");
            let n: usize = w.next().and_then(|n| n.parse().ok()).unwrap_or(0);
            let rest = w.next().unwrap_or("");
            match kind {
                "R" if rest == "ok" || rest.starts_with("error:") => {
                    if let Some(a) = n.checked_sub(1).and_then(|i| run.answers.get_mut(i)) {
                        if a.is_none() {
                            *a = Some(rest.to_string());
                        } else {
                            run.alarms.push(format!("line {n} answered twice: {rest}"));
                        }
                    }
                }
                "R" if rest == "[MSG:Pgm End]" => {}
                "R" => run.alarms.push(format!("line {n}: {rest}")),
                "B" => {
                    let v: Vec<f64> = rest
                        .split_whitespace()
                        .skip(1)
                        .map(|x| x.parse().unwrap())
                        .collect();
                    let work = [v[3], v[4], v[5]];
                    run.path.push(work);
                    run.end = Some(work);
                }
                "P" => run.holds += 1,
                "T" => run
                    .realtime
                    .push((n, u8::from_str_radix(rest.trim(), 16).unwrap_or(0))),
                _ => run.alarms.push(format!("unknown record: {l}")),
            }
        }
        run
    }

    fn sim() -> Option<PathBuf> {
        simulator("MITCAD_GRBL_SIM")
    }

    #[test]
    #[ignore = "needs the grbl simulator: ctest core.cam_grbl_sim"]
    fn grbl_takes_every_line_and_follows_the_path() {
        let Some(sim) = sim() else { return };
        let mut failures = Vec::new();
        for c in cases() {
            let what = format!("grbl, {}", c.name);
            let run = run(&sim, &what, &post_case(&c, "grbl"));
            failures.extend(check_lines(&run, &what).err());
            failures.extend(check_path(&run, &c, &what).err());
            // A manual change stops the program for the operator.
            let manual = c
                .machine
                .as_ref()
                .is_none_or(|m| matches!(m.tool_changer, ToolChanger::Manual));
            if manual && run.holds != later_changes(&c.program) {
                failures.push(format!(
                    "{what}: {} holds for {} tool changes",
                    run.holds,
                    later_changes(&c.program)
                ));
            }
        }
        report(failures);
    }

    #[test]
    #[ignore = "needs the grbl simulator: ctest core.cam_grbl_sim"]
    fn grbl_follows_the_dynamic_tool_length_offset() {
        let Some(sim) = sim() else { return };
        let mut c = case("plate with the length offset", plate());
        c.settings.tool_length_offset = Some(true);
        let what = "grbl, plate with the length offset";
        let run = run(&sim, what, &post_case(&c, "grbl"));
        let mut failures = Vec::new();
        failures.extend(check_lines(&run, what).err());
        // The work path is the program's although the machine's Z moves
        // by each tool's length.
        failures.extend(check_path(&run, &c, what).err());
        report(failures);
    }

    #[test]
    #[ignore = "needs the grbl simulator: ctest core.cam_grbl_sim"]
    fn a_controller_changer_on_grbl_leaves_m6_to_the_sender() {
        let Some(sim) = sim() else { return };
        let mut c = case("plate with the controller's changer", plate());
        c.machine = Some(machine(ToolChanger::Controller, None));
        let run = run(&sim, c.name, &post_case(&c, "grbl"));
        // grbl has no M6: a sender that handles tool changes takes it out
        // of the stream (docs/cam.md, "Tool changers").
        assert_eq!(
            run.refused_constructs(),
            vec!["T M6"],
            "{:#?}",
            run.refused()
        );
        assert!(run.refused().iter().all(|l| l.ends_with("M6 -> error:20")));
        assert!(run.alarms.is_empty(), "{:?}", run.alarms);
        check_path(&run, &c, c.name).unwrap();
    }

    /// The other dialects' output through grbl: their common subset runs,
    /// and what grbl refuses is each dialect's own commands, which are
    /// listed here.
    #[test]
    #[ignore = "needs the grbl simulator: ctest core.cam_grbl_sim"]
    fn the_other_dialects_run_on_grbl_but_for_their_own_commands() {
        let Some(sim) = sim() else { return };
        // grbl has no M6 (grblHAL, Mach3, Mach4 and LinuxCNC change tools
        // with it), no tool table (G43 H), no path blending (G64) and no
        // tape marks; RepRapFirmware's message box, MillenniumOS's tool
        // table, its waiting spindle commands and its park are their own.
        let refused: [(&str, &[&str]); 6] = [
            ("grblhal", &["T M6"]),
            ("reprapfirmware", &["M291 P R S"]),
            ("millenniumos", &["G27", "M3.9 S", "M4000 P R S", "M5.9"]),
            ("mach3", &["G43 H", "T M6"]),
            ("mach4", &["G43 H", "T M6"]),
            ("linuxcnc", &["%", "G43 H", "G64 P", "T M6"]),
        ];
        let mut failures = Vec::new();
        for c in [
            case("bracket", bracket()),
            case("plate", plate()),
            case("plate of many ways", plate_of_many_ways()),
        ] {
            for (id, words) in refused {
                let what = format!("{id}, {}", c.name);
                let run = run(&sim, &what, &post_case(&c, id));
                let got = run.refused_constructs();
                if got != words {
                    failures.push(format!(
                        "{what}: grbl refused {got:?}, expected {words:?}:\n  {}",
                        run.refused().join("\n  ")
                    ));
                }
                if !run.alarms.is_empty() || !run.realtime.is_empty() {
                    failures.push(format!("{what}: {:?} {:?}", run.alarms, run.realtime));
                }
                if let Err(e) = check_path(&run, &c, &what) {
                    failures.push(e);
                }
            }
        }
        report(failures);
    }

    /// Canned drilling cycles asked for: grbl has none and writes the
    /// moves (taken and followed); the dialects that write cycles are
    /// refused them by grbl, which the test lists, and their path is not
    /// followed then.
    #[test]
    #[ignore = "needs the grbl simulator: ctest core.cam_grbl_sim"]
    fn canned_cycles_are_written_out_for_grbl_and_refused_by_it_elsewhere() {
        let Some(sim) = sim() else { return };
        let mut c = case(
            "plate of many ways with canned cycles",
            plate_of_many_ways(),
        );
        c.settings.canned_cycles = true;
        let mut failures = Vec::new();
        let what = format!("grbl, {}", c.name);
        let own = run(&sim, &what, &post_case(&c, "grbl"));
        failures.extend(check_lines(&own, &what).err());
        failures.extend(check_path(&own, &c, &what).err());
        let refused: [(&str, &[&str]); 6] = [
            (
                "grblhal",
                &["G98 G73 X Y Z R Q F", "G98 G81 X Y Z R F", "T M6"],
            ),
            ("reprapfirmware", &["M291 P R S"]),
            (
                "millenniumos",
                &[
                    "G27",
                    "G73 X Y Z R Q F",
                    "G81 X Y Z R F",
                    "M3.9 S",
                    "M4000 P R S",
                    "M5.9",
                ],
            ),
            (
                "mach3",
                &["G43 H", "G98 G73 X Y Z R Q F", "G98 G81 X Y Z R F", "T M6"],
            ),
            (
                "mach4",
                &["G43 H", "G98 G73 X Y Z R Q F", "G98 G81 X Y Z R F", "T M6"],
            ),
            (
                "linuxcnc",
                &[
                    "%",
                    "G43 H",
                    "G64 P",
                    "G98 G73 X Y Z R Q F",
                    "G98 G81 X Y Z R F",
                    "T M6",
                ],
            ),
        ];
        // MillenniumOS's probing of the work zero is its own too.
        let mut probe = case("plate probed", plate());
        probe.settings.probe_work_zero = true;
        let what = format!("millenniumos, {}", probe.name);
        let got = run(&sim, &what, &post_case(&probe, "millenniumos")).refused_constructs();
        let words = ["G27", "G6600 W", "M3.9 S", "M4000 P R S", "M5.9"];
        if got != words {
            failures.push(format!("{what}: grbl refused {got:?}, expected {words:?}"));
        }
        for (id, words) in refused {
            let what = format!("{id}, {}", c.name);
            let run = run(&sim, &what, &post_case(&c, id));
            let got = run.refused_constructs();
            if got != words {
                failures.push(format!(
                    "{what}: grbl refused {got:?}, expected {words:?}:\n  {}",
                    run.refused().join("\n  ")
                ));
            }
        }
        report(failures);
    }
}

// ---------------------------------------------------------------------------
// grblHAL's simulator, streamed over its telnet port.

mod grblhal_sim {
    use super::*;

    /// Steps per millimetre the simulator is set to: its path's resolution.
    const STEPS_PER_MM: f64 = 1000.0;
    /// How long a line may take before the stream asks for the status.
    const QUIET: Duration = Duration::from_millis(150);
    /// How long the whole run may take.
    const LIMIT: Duration = Duration::from_secs(120);

    struct Sim {
        child: Child,
        stream: TcpStream,
        pending: String,
        started: Instant,
        /// The planner's blocks; 0 until known.
        capacity: u32,
        /// How often the stepper was started again (see `restart`).
        restarts: usize,
    }

    impl Sim {
        fn start(sim: &PathBuf) -> (Sim, PathBuf) {
            let port = TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
                .port();
            // The simulator takes a settings file name of at most 127
            // characters: short names in the scratch folder, which the
            // simulator runs in.
            static RUNS: AtomicUsize = AtomicUsize::new(0);
            let n = RUNS.fetch_add(1, Ordering::Relaxed);
            let eeprom = format!("grblhal-{n}.eeprom");
            let dir = scratch("");
            // Default settings every run.
            std::fs::write(dir.join(&eeprom), b"").unwrap();
            let blocks = dir.join(format!("grblhal-{n}.blocks"));
            let child = Command::new(sim)
                .current_dir(&dir)
                .args(["-t", "0", "-n", "-p", &port.to_string(), "-e", &eeprom])
                .arg("-b")
                .arg(format!("grblhal-{n}.blocks"))
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("the grblHAL simulator starts");
            let started = Instant::now();
            let mut child = child;
            let stream = loop {
                match TcpStream::connect(("127.0.0.1", port)) {
                    Ok(s) => break s,
                    Err(e) => {
                        if let Ok(Some(status)) = child.try_wait() {
                            panic!("the grblHAL simulator ended at its start: {status}");
                        }
                        assert!(
                            started.elapsed() < Duration::from_secs(20),
                            "the grblHAL simulator does not listen: {e}"
                        );
                        std::thread::sleep(Duration::from_millis(20));
                    }
                }
            };
            stream.set_read_timeout(Some(QUIET)).unwrap();
            stream.set_nodelay(true).unwrap();
            (
                Sim {
                    child,
                    stream,
                    pending: String::new(),
                    started,
                    capacity: 0,
                    restarts: 0,
                },
                blocks,
            )
        }

        /// The next line from the controller, `None` after a quiet spell.
        fn line(&mut self) -> Option<String> {
            loop {
                if let Some(i) = self.pending.find('\n') {
                    let l = self.pending[..i].trim().to_string();
                    self.pending.drain(..=i);
                    if l.is_empty() {
                        continue;
                    }
                    if std::env::var_os("MITCAD_SIM_DEBUG").is_some() {
                        eprintln!("{:.3} < {l}", self.started.elapsed().as_secs_f64());
                    }
                    return Some(l);
                }
                assert!(
                    self.started.elapsed() < LIMIT,
                    "the grblHAL simulator took too long"
                );
                let mut buf = [0u8; 4096];
                match self.stream.read(&mut buf) {
                    Ok(0) => panic!("the grblHAL simulator closed the connection"),
                    Ok(n) => self.pending.push_str(&String::from_utf8_lossy(&buf[..n])),
                    Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                        return None;
                    }
                    Err(e) => panic!("reading from the grblHAL simulator: {e}"),
                }
            }
        }

        fn send(&mut self, bytes: &[u8]) {
            if std::env::var_os("MITCAD_SIM_DEBUG").is_some() {
                eprintln!(
                    "{:.3} > {:?}",
                    self.started.elapsed().as_secs_f64(),
                    String::from_utf8_lossy(bytes)
                );
            }
            self.stream.write_all(bytes).unwrap();
        }

        /// A status report: `<State|MPos:x,y,z|...>`.
        fn status(&mut self, others: &mut Vec<String>) -> String {
            self.send(b"?");
            loop {
                match self.line() {
                    Some(l) if l.starts_with('<') => return l,
                    Some(l) => others.push(l),
                    None => self.send(b"?"),
                }
            }
        }

        /// Sends a line and waits for its answer, resuming a hold (a tool
        /// change, a pause) as the operator would; other lines go to
        /// `others`.
        fn command(&mut self, text: &str, others: &mut Vec<String>, holds: &mut usize) -> String {
            self.send(format!("{text}\n").as_bytes());
            loop {
                match self.line() {
                    Some(l) if is_answer(&l) => return l,
                    Some(l) => others.push(l),
                    None => {
                        let s = self.status(others);
                        // The answer may have come before the status.
                        if let Some(i) = others.iter().position(|l| is_answer(l)) {
                            return others.remove(i);
                        }
                        if s.starts_with("<Hold") || s.starts_with("<Tool") {
                            *holds += 1;
                            self.send(b"~");
                        } else if s.starts_with("<Alarm") {
                            others.push(s);
                            return "no answer (alarm)".into();
                        } else {
                            self.restart(&s);
                        }
                    }
                }
            }
        }
    }

    impl Drop for Sim {
        /// A failed check leaves no simulator running.
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    impl Sim {
        /// Waits until the machine is idle with nothing left in the
        /// planner (or in an alarm) and returns that status.
        fn settle(&mut self, others: &mut Vec<String>) -> String {
            loop {
                let st = self.status(others);
                if st.starts_with("<Alarm")
                    || (st.starts_with("<Idle") && planner_free(&st) == Some(self.capacity))
                {
                    return st;
                }
                self.restart(&st);
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        /// Starts the stepper again when the machine is idle with blocks
        /// left in the planner. As fast as it can (`-t 0`), the simulated
        /// processor's clock can run ahead of the thread that prepares the
        /// stepper's segments, which then runs dry and stops, a thing of
        /// the simulation that real hardware with one clock does not do;
        /// the blocks' targets are unchanged.
        fn restart(&mut self, status: &str) {
            if status.starts_with("<Idle")
                && planner_free(status).is_some_and(|f| f < self.capacity)
            {
                self.restarts += 1;
                self.send(b"~");
            }
        }
    }

    /// The free planner blocks of a status report (`Bf:blocks,bytes`).
    fn planner_free(status: &str) -> Option<u32> {
        let bf = status.split('|').find_map(|f| f.strip_prefix("Bf:"))?;
        bf.split([',', '>']).next()?.parse().ok()
    }

    fn is_answer(line: &str) -> bool {
        line == "ok" || line.starts_with("error:")
    }

    fn mpos(status: &str) -> Option<P3> {
        let m = status.split('|').find_map(|f| f.strip_prefix("MPos:"))?;
        let v: Vec<f64> = m
            .trim_end_matches('>')
            .split(',')
            .filter_map(|x| x.parse().ok())
            .collect();
        (v.len() >= 3).then(|| [v[0], v[1], v[2]])
    }

    /// Streams the G-code to grblHAL's simulator.
    pub(crate) fn run(sim: &PathBuf, name: &str, gcode: &str) -> Run {
        let (mut s, blocks) = Sim::start(sim);
        // The program beside the simulator's files, to look at.
        std::fs::write(
            blocks.with_extension("nc"),
            format!("({})\n{gcode}", name.replace(['(', ')'], "")),
        )
        .unwrap();
        let mut others = Vec::new();
        let mut holds = 0;
        let setup = [
            "$X".to_string(),
            format!("$100={STEPS_PER_MM}"),
            format!("$101={STEPS_PER_MM}"),
            format!("$102={STEPS_PER_MM}"),
        ];
        for l in &setup {
            let a = s.command(l, &mut others, &mut holds);
            assert!(a == "ok", "grblHAL setting {l}: {a}");
        }
        others.clear();
        // The planner's size: its free blocks when nothing moves.
        let first = s.status(&mut others);
        s.capacity = planner_free(&first).expect("the status reports the planner");
        let lines: Vec<String> = gcode.lines().map(str::to_string).collect();
        let mut run = Run {
            resolution: 0.5 / STEPS_PER_MM,
            ..Run::default()
        };
        for (i, l) in lines.iter().enumerate() {
            if let Some(c) = l
                .bytes()
                .find(|&c| matches!(c, b'?' | b'!' | b'~' | 0x18) || c > 0x7f)
            {
                // A realtime command would act at once; the line goes
                // without it.
                run.realtime.push((i + 1, c));
            }
            let clean: String = l
                .chars()
                .filter(|&c| !matches!(c, '?' | '!' | '~' | '\u{18}') && c.is_ascii())
                .collect();
            // A sender lets the motion end before a tool change or a
            // pause; the simulator can lose a block that is still running
            // when an M6 holds it.
            let code = clean.split('(').next().unwrap_or("");
            if code
                .split_whitespace()
                .any(|w| matches!(w, "M6" | "M0" | "M1" | "M06" | "M00" | "M01"))
            {
                s.settle(&mut others);
            }
            let a = s.command(&clean, &mut others, &mut holds);
            run.answers.push(Some(a));
        }
        // Wait for the motion to end; the final position is the
        // controller's own report.
        let last = s.settle(&mut others);
        if s.restarts > 0 {
            println!(
                "{name}: the simulator's stepper was started again {} times",
                s.restarts
            );
        }
        let end = if last.starts_with("<Idle") {
            mpos(&last)
        } else {
            others.push(last);
            None
        };
        s.send(&[0x06]);
        let ok = s.child.wait().expect("the grblHAL simulator ends");
        assert!(ok.success(), "the grblHAL simulator failed: {ok}");
        run.lines = lines;
        run.alarms = others
            .into_iter()
            .filter(|l| !l.starts_with("[MSG:Pgm End") && l != "[MSG:]" && !l.starts_with('<'))
            .collect();
        run.holds = holds;
        run.end = end;
        // The planner's blocks: steps of each axis and the block's speed.
        let text = std::fs::read_to_string(&blocks).unwrap_or_default();
        run.path = text
            .lines()
            .filter_map(|l| {
                let v: Vec<f64> = l
                    .split(',')
                    .take(3)
                    .filter_map(|x| x.trim().parse().ok())
                    .collect();
                (v.len() == 3).then(|| {
                    [
                        v[0] / STEPS_PER_MM,
                        v[1] / STEPS_PER_MM,
                        v[2] / STEPS_PER_MM,
                    ]
                })
            })
            .collect();
        run
    }

    fn sim() -> Option<PathBuf> {
        simulator("MITCAD_GRBLHAL_SIM")
    }

    #[test]
    #[ignore = "needs the grblHAL simulator: ctest core.cam_grblhal_sim"]
    fn grblhal_takes_every_line_and_follows_the_path() {
        let Some(sim) = sim() else { return };
        let mut failures = Vec::new();
        // grblHAL has mist coolant (M7), which grbl's standard build lacks.
        let mut p = bracket();
        p.operations[1].coolant = Coolant::Mist;
        let mist = case("bracket with mist coolant", p);
        // grblHAL runs the canned drilling cycles itself.
        let mut canned = case(
            "plate of many ways with canned cycles",
            plate_of_many_ways(),
        );
        canned.settings.canned_cycles = true;
        for (c, ids) in cases()
            .into_iter()
            // grbl's output of the long many-ways plate runs on grbl only:
            // streamed a line at a time it takes minutes here.
            .map(|c| {
                let ids = if c.name.starts_with("plate of many ways") {
                    &["grblhal"][..]
                } else {
                    &["grblhal", "grbl"][..]
                };
                (c, ids)
            })
            .chain([(mist, &["grblhal"][..]), (canned, &["grblhal"][..])])
        {
            for id in ids {
                let what = format!("{id}, {}", c.name);
                let run = run(&sim, &what, &post_case(&c, id));
                failures.extend(check_lines(&run, &what).err());
                failures.extend(check_path(&run, &c, &what).err());
            }
        }
        report(failures);
    }

    #[test]
    #[ignore = "needs the grblHAL simulator: ctest core.cam_grblhal_sim"]
    fn grblhal_ends_above_the_part_by_the_last_tool_s_length() {
        let Some(sim) = sim() else { return };
        let mut c = case("plate with the length offset", plate());
        c.settings.tool_length_offset = Some(true);
        let what = "grblhal, plate with the length offset";
        let run = run(&sim, what, &post_case(&c, "grblhal"));
        check_lines(&run, what).unwrap();
        // grblHAL reports the machine position: the work position plus the
        // last tool's length (G43.1 Z).
        let last = &c.program.operations.last().unwrap().tool;
        let mut e = expected(&c).end;
        e[2] += last.length;
        let end = run.end.expect("a position");
        let tol = rounding(&c.settings) + run.resolution;
        assert!(
            (0..3).all(|k| (end[k] - e[k]).abs() <= tol),
            "ends at {end:?}, expected {e:?}"
        );
    }
}
