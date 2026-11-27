// SPDX-License-Identifier: MIT
//! The LinuxCNC turning post's programs (mitcad#2425) run through
//! LinuxCNC's own interpreter before they ship, as the milling posts' run
//! through grbl and grblHAL (`controller_sim.rs`, docs/cam.md, "Simulation
//! and checks").
//!
//! - `linuxcnc_sim`: LinuxCNC's stand-alone interpreter `rs274`
//!   (`MITCAD_LINUXCNC_SIM`; built from LinuxCNC's source, its rs274ngc
//!   interpreter with the canon layer of `src/emc/sai`), which prints each
//!   canonical machining call a block makes: a line per call, `<count>
//!   N<sequence number or .....> CALL(arguments)`. It runs in batch mode
//!   (`-g`), stops at the first error (`-n 2`, exit status 1), with an INI
//!   that makes the machine a millimetre one, a tool table (a tool per
//!   turret station and per offset number, each with X and Z offsets of
//!   its own) and a parameter file, all in the test's scratch folder (HOME
//!   and TMPDIR too, where it keeps its tool table's shared copy).
//!
//! A shaft is posted (facing, roughing outside and in the bore longhand,
//! finishing with a fillet's arcs, a relief groove, a thread in G33
//! passes, parting off, six tools, one on an offset number of its own), in
//! millimetres, and in inches with line numbers, canned cycles asked for
//! and a remapped M6 (no G43). Checked against the program: every block
//! accepted; the units, the ZX plane and path blending within the arc
//! tolerance; each motion (rapid, feed, arc, spindle-synchronised feed) of
//! the program's toolpaths and the tool change position's rapids in
//! order, its end in X as a radius (diameter mode) and Z, an arc's centre
//! and sense, a thread's K; at each cut the feeds per revolution and the
//! operation's feed, the spindle turning in its sense at its constant
//! surface speed with its limit or at its constant speed (always for a
//! thread), its coolant; each tool change's tool, and with G43 the tool
//! table's offsets of that tool or of its offset number. The interpreter
//! is a GPL program run apart, never linked; the test is ignored in
//! `core.cam` and run by the ctest `core.cam_linuxcnc_sim`, which prints
//! "simulator: skipped" (and is reported as skipped) without the variable.
//!
//! Resolution: X has three decimals of a diameter in millimetres, four in
//! inches; the interpreter prints four decimals in the program's units
//! (lengths and feeds; the inch feed, written with six, comes back within
//! 0.00005 in/rev).
//! The comparisons allow these, nothing more.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use mitcad_cam::turning::ops::{self, GrooveOp, PartOp, ThreadOp};
use mitcad_cam::turning::{
    FaceOp, FinishOp, LinuxCncTurning, PV, PXZ, RoughOp, TurnMove, TurnPart, TurnSide, TurnSpindle,
    TurningProgram, TurningProgramOp, TurningTool, TurningToolKind, face, finish, groove, part_off,
    rough, thread, turning_post,
};
use mitcad_cam::{Coolant, PostSettings, SpindleDirection, Units};

/// The simulator's path from its variable, or `None` (with the skip
/// message ctest looks for) when it is not set.
fn simulator() -> Option<PathBuf> {
    let var = "MITCAD_LINUXCNC_SIM";
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
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("controller-sim")
        .join("linuxcnc")
        .join(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ---------------------------------------------------------------------------
// The program.

fn tool(
    number: u32,
    offset: u32,
    name: &str,
    kind: TurningToolKind,
    nose: f64,
    depth: f64,
) -> TurningTool {
    TurningTool {
        number,
        offset,
        name: name.into(),
        kind,
        nose_radius: nose,
        max_depth: depth,
        back_angle: None,
    }
}

fn op(
    name: &str,
    tool: &TurningTool,
    spindle: TurnSpindle,
    coolant: Coolant,
    path: mitcad_cam::TurnPath,
) -> TurningProgramOp {
    TurningProgramOp {
        name: name.into(),
        tool: tool.clone(),
        spindle,
        direction: SpindleDirection::Cw,
        coolant,
        path,
        live: None,
        sub: false,
    }
}

fn css(surface_speed: f64, max_rpm: f64) -> TurnSpindle {
    TurnSpindle::Css {
        surface_speed,
        max_rpm,
    }
}

/// A shaft from a 40 mm bar with 1 mm to face: a chamfer, the 20 mm
/// thread diameter with a 17 mm relief groove behind it, a 28 mm shoulder,
/// 30 mm ending in a 1 mm fillet against a 35 mm shoulder; a 12 mm bore 20
/// deep with a chamfer at its mouth in a tube of 8 mm bore.
fn shaft() -> TurnPart {
    let fillet = (-std::f64::consts::FRAC_PI_8).tan();
    TurnPart {
        outside: vec![
            PV::new(0.0, 0.0),
            PV::new(9.0, 0.0),
            PV::new(10.0, -1.0),
            PV::new(10.0, -14.0),
            PV::new(8.5, -14.0),
            PV::new(8.5, -17.0),
            PV::new(10.0, -17.0),
            PV::new(10.0, -20.0),
            PV::new(14.0, -20.0),
            PV::new(15.0, -21.0),
            PV::arc(15.0, -34.0, fillet),
            PV::new(16.0, -35.0),
            PV::new(17.5, -35.0),
            PV::new(17.5, -50.0),
        ],
        inside: vec![
            PV::new(7.0, 0.0),
            PV::new(6.0, -1.0),
            PV::new(6.0, -20.0),
            PV::new(4.0, -20.0),
            PV::new(4.0, -50.0),
        ],
        stock_radius: 20.0,
        stock_inner_radius: 4.0,
        stock_face: 1.0,
    }
}

/// The threading tool's offset number, a tool table entry of its own.
const THREAD_OFFSET: u32 = 15;

fn shaft_program() -> TurningProgram {
    let part = shaft();
    let rougher = tool(1, 0, "Roughing", TurningToolKind::Turning, 0.4, 4.0);
    let finisher = tool(2, 0, "Finishing", TurningToolKind::Turning, 0.4, 4.0);
    let bar = tool(
        3,
        0,
        "Boring bar",
        TurningToolKind::Boring { bar_diameter: 6.0 },
        0.4,
        2.0,
    );
    let grooving = tool(
        4,
        0,
        "Grooving 2 mm",
        TurningToolKind::Grooving {
            width: 2.0,
            inside: false,
        },
        0.1,
        3.0,
    );
    let threading = tool(
        5,
        THREAD_OFFSET,
        "Thread 60",
        TurningToolKind::Threading {
            angle: 60.0,
            inside: false,
        },
        0.0,
        2.0,
    );
    let parting = tool(
        6,
        0,
        "Parting 3 mm",
        TurningToolKind::Parting { width: 3.0 },
        0.0,
        22.0,
    );
    let rough_op = |side| RoughOp {
        side,
        depth: 1.5,
        retract: 0.5,
        allowance_x: 0.2,
        allowance_z: 0.1,
        clearance: 1.0,
        feed: 0.25,
        plunge: 0.0,
    };
    let finish_op = |side| FinishOp {
        side,
        clearance: 1.0,
        feed: 0.1,
        plunge: 0.0,
        roughed: false,
    };
    let flood = Coolant::Flood;
    let mut program = TurningProgram::new("Shaft", [100.0, 150.0]);
    program.operations = vec![
        op(
            "Face",
            &rougher,
            css(200.0, 3000.0),
            flood,
            face(
                &part,
                &FaceOp {
                    step: 2.0,
                    clearance: 2.0,
                    feed: 0.2,
                },
                &rougher,
            )
            .unwrap(),
        ),
        op(
            "Rough outside",
            &rougher,
            css(200.0, 3000.0),
            flood,
            rough(&part, &rough_op(TurnSide::Outside), &rougher).unwrap(),
        ),
        op(
            "Rough bore",
            &bar,
            css(150.0, 2500.0),
            Coolant::Mist,
            rough(&part, &rough_op(TurnSide::Inside), &bar).unwrap(),
        ),
        op(
            "Finish outside",
            &finisher,
            css(250.0, 3500.0),
            flood,
            finish(&part, &finish_op(TurnSide::Outside), &finisher).unwrap(),
        ),
        op(
            "Finish bore",
            &bar,
            css(150.0, 2500.0),
            flood,
            finish(&part, &finish_op(TurnSide::Inside), &bar).unwrap(),
        ),
        op(
            "Relief groove",
            &grooving,
            css(100.0, 1500.0),
            flood,
            groove(
                &part,
                &GrooveOp {
                    side: TurnSide::Outside,
                    z_from: -14.0,
                    z_to: -17.0,
                    peck: 0.0,
                    clearance: 1.0,
                    feed: 0.05,
                },
                &grooving,
            )
            .unwrap(),
        ),
        op(
            "Thread M20",
            &threading,
            TurnSpindle::Rpm { rpm: 600.0 },
            flood,
            thread(
                &ThreadOp {
                    side: TurnSide::Outside,
                    crest: 10.0,
                    z_start: -1.0,
                    z_end: -13.0,
                    lead: 1.5,
                    height: ops::external_thread_height(1.5),
                    first_depth: 0.35,
                    min_depth: 0.05,
                    finish_allowance: 0.05,
                    spring_passes: 1,
                    run_in: 3.0,
                    clearance: 1.0,
                },
                &threading,
            )
            .unwrap(),
        ),
        op(
            "Part off",
            &parting,
            css(80.0, 1200.0),
            flood,
            part_off(
                &part,
                &PartOp {
                    z: None,
                    to_x: 0.0,
                    peck: 2.0,
                    clearance: 2.0,
                    feed: 0.05,
                },
                &parting,
            )
            .unwrap(),
        ),
    ];
    program
}

/// A tool table entry's X and Z offsets (mm), different for each.
fn table_offset(number: u32) -> [f64; 2] {
    [
        0.5 + 0.25 * f64::from(number),
        -1.0 - 0.5 * f64::from(number),
    ]
}

/// The tool table: an entry per turret station and per offset number.
fn tool_table(program: &TurningProgram) -> String {
    let mut numbers: Vec<u32> = Vec::new();
    for o in &program.operations {
        for n in [o.tool.number, o.tool.offset_number()] {
            if !numbers.contains(&n) {
                numbers.push(n);
            }
        }
    }
    numbers.sort_unstable();
    numbers
        .iter()
        .map(|&n| {
            let [x, z] = table_offset(n);
            format!("T{n} P{n} X{x:.4} Z{z:.4} D0.8 Q3 ;station {n}\n")
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The interpreter's run and its calls.

/// One canonical call: its name and its arguments, the numbers among
/// them parsed (the rest, as `CANON_PLANE_XZ`, as text).
#[derive(Debug)]
struct Call {
    name: String,
    args: Vec<String>,
}

impl Call {
    fn num(&self, i: usize) -> f64 {
        self.args
            .get(i)
            .and_then(|a| a.parse::<f64>().ok())
            .unwrap_or_else(|| panic!("{} has no number {i}: {:?}", self.name, self.args))
    }
}

/// Runs the interpreter on `text` and gives its calls; fails when the
/// interpreter refuses the program.
fn interpret(sim: &Path, name: &str, text: &str, table: &str) -> Vec<Call> {
    run_interpreter(sim, name, text, table).unwrap_or_else(|e| panic!("{e}"))
}

/// Runs the interpreter on `text`: its calls, or what it said when it
/// stopped (a block it refuses: exit status 1) with its calls up to there.
fn run_interpreter(sim: &Path, name: &str, text: &str, table: &str) -> Result<Vec<Call>, String> {
    let dir = scratch(name);
    let program = dir.join("program.ngc");
    let out = dir.join("canon.txt");
    let ini = dir.join("lathe.ini");
    let tbl = dir.join("tool.tbl");
    let var = dir.join("lathe.var");
    std::fs::write(&program, text).unwrap();
    std::fs::write(&tbl, table).unwrap();
    // The parameter file, empty (every parameter 0, the work offsets
    // too): rs274 reads it at the start and, when it saves the parameters
    // at the end, links the old file to its backup first, which says
    // "link (updating variable file): No such file or directory" when
    // there is none (mitcad#2425).
    std::fs::write(&var, "").unwrap();
    std::fs::write(
        &ini,
        format!(
            "[EMC]\nMACHINE = MitCad lathe test\n[TRAJ]\nLINEAR_UNITS = mm\n[RS274NGC]\nPARAMETER_FILE = {}\n",
            var.display()
        ),
    )
    .unwrap();
    let _ = std::fs::write(&out, "");
    let run = Command::new(sim)
        .arg("-g")
        .args(["-n", "2"])
        .arg("-i")
        .arg(&ini)
        .arg("-t")
        .arg(&tbl)
        .arg("-v")
        .arg(&var)
        .arg(&program)
        .arg(&out)
        .current_dir(&dir)
        .env("HOME", &dir)
        .env("TMPDIR", &dir)
        .stdin(Stdio::null())
        .output()
        .expect("the interpreter runs");
    let stderr = String::from_utf8_lossy(&run.stderr);
    let canon = std::fs::read_to_string(&out).unwrap_or_default();
    if !run.status.success() {
        return Err(format!(
            "{name}: the interpreter stopped ({}):\n{stderr}\nits calls up to there:\n{canon}",
            run.status
        ));
    }
    let calls: Vec<Call> = canon
        .lines()
        .filter_map(|l| {
            // `<count> N<…> CALL(arguments)`.
            let mut parts = l.split_whitespace();
            parts.next()?;
            parts.next()?;
            let call: Vec<&str> = parts.collect();
            let call = call.join(" ");
            let open = call.find('(')?;
            let close = call.rfind(')')?;
            let args = call[open + 1..close]
                .split([',', ' '])
                .filter(|a| !a.is_empty())
                .map(str::to_owned)
                .collect();
            Some(Call {
                name: call[..open].to_owned(),
                args,
            })
        })
        .collect();
    if calls.is_empty() {
        return Err(format!("{name}: no calls:\n{stderr}"));
    }
    if let Some(e) = calls.iter().find(|c| c.name == "CANON_ERROR") {
        return Err(format!("{name}: {e:?}"));
    }
    Ok(calls)
}

/// A motion the interpreter made, with the machine's state at it (lengths
/// in millimetres, X a radius).
#[derive(Debug)]
struct Motion {
    kind: Kind,
    to: PXZ,
    /// Feeds per revolution, and the feed.
    per_rev: bool,
    feed: f64,
    /// The spindle: turning clockwise (`Some(true)`), counter-clockwise,
    /// or stopped; its mode's limit (none: a constant speed) and speed.
    spindle: Option<bool>,
    css_limit: Option<f64>,
    speed: f64,
    flood: bool,
    mist: bool,
    tool: Option<u32>,
    /// In the ZX plane, and the path blending's tolerance (continuous
    /// mode), in the program's units.
    xz: bool,
    blend: Option<f64>,
}

#[derive(Debug, PartialEq)]
enum Kind {
    Rapid,
    Feed,
    /// The centre and whether it runs clockwise as drawn (Z right, X up).
    Arc {
        center: PXZ,
        cw: bool,
    },
    /// A spindle-synchronised feed: the distance along it per revolution.
    Synced {
        per_rev: f64,
    },
}

/// What the run did: its motions, its tool changes (the tool, and the
/// offsets G43 applied before the next motion), whether the program ended.
struct Run {
    motions: Vec<Motion>,
    changes: Vec<(u32, Option<[f64; 2]>)>,
    ended: bool,
}

fn replay(calls: &[Call], inches: bool) -> Run {
    let unit = if inches { 25.4 } else { 1.0 };
    let mut run = Run {
        motions: Vec::new(),
        changes: Vec::new(),
        ended: false,
    };
    let mut per_rev = false;
    let mut feed = 0.0;
    let mut spindle: Option<bool> = None;
    let mut css_limit: Option<f64> = None;
    let mut speed = 0.0;
    let mut flood = false;
    let mut mist = false;
    let mut selected: Option<u32> = None;
    let mut tool: Option<u32> = None;
    let mut synced: Option<f64> = None;
    let mut units_seen = false;
    let mut changing = false;
    let mut xz = false;
    let mut blend: Option<f64> = None;
    for c in calls {
        let motion = move |kind: Kind, to: PXZ| Motion {
            kind,
            to,
            per_rev,
            feed: feed * unit,
            spindle,
            css_limit,
            speed,
            flood,
            mist,
            tool,
            xz,
            blend,
        };
        match c.name.as_str() {
            "USE_LENGTH_UNITS" => {
                let want = if inches {
                    "CANON_UNITS_INCHES"
                } else {
                    "CANON_UNITS_MM"
                };
                units_seen = c.args.first().map(String::as_str) == Some(want);
            }
            "SELECT_PLANE" => xz = c.args.first().map(String::as_str) == Some("CANON_PLANE_XZ"),
            "SET_MOTION_CONTROL_MODE" => {
                blend = (c.args.first().map(String::as_str) == Some("CANON_CONTINUOUS"))
                    .then(|| c.num(1));
            }
            "SET_FEED_MODE" => per_rev = c.num(1) == 1.0,
            "SET_FEED_RATE" => feed = c.num(0),
            "SET_SPINDLE_MODE" => {
                let limit = c.num(1);
                css_limit = (limit > 0.0).then_some(limit);
            }
            "SET_SPINDLE_SPEED" => speed = c.num(1),
            "START_SPINDLE_CLOCKWISE" => spindle = Some(true),
            "START_SPINDLE_COUNTERCLOCKWISE" => spindle = Some(false),
            "STOP_SPINDLE_TURNING" => spindle = None,
            "FLOOD_ON" => flood = true,
            "FLOOD_OFF" => flood = false,
            "MIST_ON" => mist = true,
            "MIST_OFF" => mist = false,
            "SELECT_TOOL" => selected = Some(c.num(0) as u32),
            "CHANGE_TOOL" => {
                tool = selected;
                run.changes.push((selected.expect("T before M6"), None));
                changing = true;
            }
            "USE_TOOL_LENGTH_OFFSET" => {
                // `x y z, a b c, u v w` between a change and the next
                // motion: G43's offsets.
                if changing && let Some(last) = run.changes.last_mut() {
                    last.1 = Some([c.num(0) * unit, c.num(2) * unit]);
                }
            }
            "START_SPEED_FEED_SYNC" => synced = Some(c.num(0) * unit),
            "STOP_SPEED_FEED_SYNCH" => synced = None,
            "STRAIGHT_TRAVERSE" => {
                assert!(units_seen, "a motion before the program's units: {c:?}");
                let m = motion(Kind::Rapid, [c.num(0) * unit, c.num(2) * unit]);
                run.motions.push(m);
                changing = false;
            }
            "STRAIGHT_FEED" => {
                assert!(units_seen, "a motion before the program's units: {c:?}");
                let kind = match synced {
                    Some(k) => Kind::Synced { per_rev: k },
                    None => Kind::Feed,
                };
                let m = motion(kind, [c.num(0) * unit, c.num(2) * unit]);
                run.motions.push(m);
                changing = false;
            }
            "ARC_FEED" => {
                // In the ZX plane: Z first, X second; the rotation's sign
                // counter-clockwise from Z towards X.
                assert!(xz, "an arc outside the ZX plane: {c:?}");
                let m = motion(
                    Kind::Arc {
                        center: [c.num(3) * unit, c.num(2) * unit],
                        cw: c.num(4) < 0.0,
                    },
                    [c.num(1) * unit, c.num(0) * unit],
                );
                run.motions.push(m);
                changing = false;
            }
            "PROGRAM_END" => run.ended = true,
            _ => {}
        }
    }
    run
}

/// The program's moves as the interpreter must make them: the change
/// position's rapid before each tool change and at the end, every move of
/// the toolpaths but those that move nothing at the program's digits (the
/// post writes the change rapid and the first move after a change always),
/// each with its operation's index (none for the change rapids).
fn wanted(program: &TurningProgram, inches: bool) -> Vec<(Option<usize>, TurnMove)> {
    let change = TurnMove::Rapid {
        to: program.change_position,
    };
    // A place as the program writes it: the diameter and Z in its units
    // and digits.
    let written = |p: PXZ| -> (i64, i64) {
        let (unit, digits) = if inches { (25.4, 1e4) } else { (1.0, 1e3) };
        (
            (2.0 * p[0] / unit * digits).round() as i64,
            (p[1] / unit * digits).round() as i64,
        )
    };
    let mut out: Vec<(Option<usize>, TurnMove)> = Vec::new();
    let mut at: Option<(i64, i64)> = None;
    let mut push = |o: Option<usize>, m: TurnMove, at: &mut Option<(i64, i64)>| {
        let to = written(m.to());
        if *at == Some(to) {
            return;
        }
        *at = Some(to);
        out.push((o, m));
    };
    let mut tool: Option<u32> = None;
    for (i, o) in program.operations.iter().enumerate() {
        if tool != Some(o.tool.number) {
            at = None;
            push(None, change, &mut at);
            at = None;
            tool = Some(o.tool.number);
        }
        for m in &o.path.moves {
            push(Some(i), *m, &mut at);
        }
    }
    push(None, change, &mut at);
    out
}

/// Posts the shaft with `settings`, runs it and checks the run against
/// the program.
fn check_run(sim: &Path, name: &str, settings: &PostSettings) {
    let program = shaft_program();
    let text = turning_post(&program, &LinuxCncTurning, settings).unwrap();
    let inches = settings.units == Units::Inches;
    let calls = interpret(sim, name, &text, &tool_table(&program));
    let run = replay(&calls, inches);
    assert!(run.ended, "{name}: no program end");
    // Every motion in the ZX plane, blending corners within the arc
    // tolerance.
    for m in &run.motions {
        assert!(m.xz, "{name}: {m:?} outside the ZX plane");
        let blend = m
            .blend
            .unwrap_or_else(|| panic!("{name}: {m:?} without path blending"));
        let blend = blend * if inches { 25.4 } else { 1.0 };
        assert!(
            (blend - settings.arc_tolerance).abs() < 3e-3,
            "{name}: blending within {blend} mm"
        );
    }

    // The G-code's rounding: three decimals of a diameter in millimetres,
    // four in inches; the interpreter's four decimals in its units.
    let tolerance = if inches { 0.003 } else { 0.0006 };
    let wanted = wanted(&program, inches);
    assert_eq!(
        run.motions.len(),
        wanted.len(),
        "{name}: the motions:\n{:#?}\nagainst\n{wanted:#?}",
        run.motions
    );
    let close =
        |p: PXZ, q: PXZ| (p[0] - q[0]).abs() <= tolerance && (p[1] - q[1]).abs() <= tolerance;
    let mut from: Option<PXZ> = None;
    for (k, (got, (index, want))) in run.motions.iter().zip(&wanted).enumerate() {
        let what = format!("{name}: motion {k} {got:?} against {want:?}");
        assert!(close(got.to, want.to()), "{what}: its end");
        match (&got.kind, *want) {
            (Kind::Rapid, TurnMove::Rapid { .. }) | (Kind::Feed, TurnMove::Line { .. }) => {}
            (
                Kind::Arc { center, cw },
                TurnMove::Arc {
                    center: c, cw: w, ..
                },
            ) => {
                // The start's rounding and the centre's own (I, K).
                let near = if inches { 0.004 } else { 0.0012 };
                assert!(
                    (center[0] - c[0]).abs() <= near && (center[1] - c[1]).abs() <= near,
                    "{what}: the arc's centre"
                );
                assert_eq!(*cw, w, "{what}: the arc's sense");
            }
            (Kind::Synced { per_rev }, TurnMove::Thread { lead, to }) => {
                // The lead along the longer axis, K along the move.
                let f = from.expect("a thread from a known place");
                let (dx, dz) = (to[0] - f[0], to[1] - f[1]);
                let along = lead * dx.hypot(dz) / dx.abs().max(dz.abs());
                assert!(
                    (per_rev - along).abs() < 1e-4 * if inches { 25.4 } else { 1.0 },
                    "{what}: the lead"
                );
            }
            _ => panic!("{what}: another kind of motion"),
        }
        from = Some(got.to);
        let Some(i) = *index else {
            continue;
        };
        let o = &program.operations[i];
        // The operation's tool, its spindle and coolant at its cuts.
        assert_eq!(got.tool, Some(o.tool.number), "{what}: the tool");
        if matches!(want, TurnMove::Rapid { .. }) {
            continue;
        }
        assert_eq!(
            got.spindle,
            Some(o.direction == SpindleDirection::Cw),
            "{what}: the spindle"
        );
        match o.spindle {
            TurnSpindle::Css {
                surface_speed,
                max_rpm,
            } => {
                assert_eq!(
                    got.css_limit,
                    Some(max_rpm.round()),
                    "{what}: the spindle's limit"
                );
                let s = if inches {
                    (surface_speed * 3.280_84).round()
                } else {
                    surface_speed.round()
                };
                assert_eq!(got.speed, s, "{what}: the surface speed");
            }
            TurnSpindle::Rpm { rpm } => {
                assert_eq!(got.css_limit, None, "{what}: a constant speed");
                assert_eq!(got.speed, rpm.round(), "{what}: the speed");
            }
        }
        assert_eq!(
            got.flood,
            o.coolant == Coolant::Flood,
            "{what}: flood coolant"
        );
        assert_eq!(got.mist, o.coolant == Coolant::Mist, "{what}: mist coolant");
        if let Some(f) = want.feed()
            && !matches!(want, TurnMove::Thread { .. })
        {
            assert!(got.per_rev, "{what}: feeds per revolution");
            // The interpreter prints the feed with four decimals in the
            // program's units: exact in millimetres (the program's own
            // four decimals), within 0.00005 in/rev in inches, where the
            // program writes six (mitcad#2425).
            let near = if inches { 0.0013 } else { 1e-4 };
            assert!((got.feed - f).abs() < near, "{what}: the feed {}", got.feed);
        }
    }

    // A tool change per change of tool, with G43's offsets of that tool or
    // of its offset number.
    let mut tools: Vec<&TurningTool> = Vec::new();
    for o in &program.operations {
        if tools.last().is_none_or(|t| t.number != o.tool.number) {
            tools.push(&o.tool);
        }
    }
    assert_eq!(run.changes.len(), tools.len(), "{name}: {:?}", run.changes);
    let with_offset = settings.tool_length_offset.unwrap_or(true);
    for ((number, offsets), t) in run.changes.iter().zip(&tools) {
        assert_eq!(*number, t.number, "{name}: the tool changed to");
        if with_offset {
            let [x, z] = table_offset(t.offset_number());
            let got = offsets.unwrap_or_else(|| panic!("{name}: no G43 after T{number} M6"));
            assert!(
                (got[0] - x).abs() < 1e-3 && (got[1] - z).abs() < 1e-3,
                "{name}: tool {number}'s offsets {got:?} against {x} {z}"
            );
        } else {
            assert_eq!(*offsets, None, "{name}: G43 after T{number} M6");
        }
    }
    // It ends at the change position.
    let last = run.motions.last().unwrap();
    assert!(
        close(last.to, program.change_position),
        "{name}: it ends at {:?}",
        last.to
    );
}

mod linuxcnc_sim {
    use super::*;

    #[test]
    #[ignore = "needs LinuxCNC's interpreter: ctest core.cam_linuxcnc_sim"]
    fn the_shaft_runs_through_linuxcnc_in_millimetres() {
        let Some(sim) = simulator() else {
            return;
        };
        check_run(&sim, "shaft-mm", &PostSettings::default());
    }

    #[test]
    #[ignore = "needs LinuxCNC's interpreter: ctest core.cam_linuxcnc_sim"]
    fn the_shaft_runs_in_inches_with_line_numbers_and_a_remapped_m6() {
        let Some(sim) = simulator() else {
            return;
        };
        check_run(
            &sim,
            "shaft-inch",
            &PostSettings {
                units: Units::Inches,
                line_numbers: true,
                canned_cycles: true,
                tool_length_offset: Some(false),
                ..PostSettings::default()
            },
        );
    }

    /// A program LinuxCNC would refuse is seen as refused: a Fanuc
    /// control's thread block (G33 with F and no K), the interpreter's own
    /// refusal, so that the check is known to see one.
    #[test]
    #[ignore = "needs LinuxCNC's interpreter: ctest core.cam_linuxcnc_sim"]
    fn the_interpreter_refuses_a_program_it_cannot_run() {
        let Some(sim) = simulator() else {
            return;
        };
        let refused = run_interpreter(
            &sim,
            "refused",
            "%\nG21 G18 G7 G95\nG0 X20. Z2.\nG33 Z-10. F1.5\nM2\n%\n",
            "T1 P1 X0 Z0 D0.8 Q3 ;\n",
        );
        let Err(message) = refused else {
            panic!("G33 with F and no K was taken");
        };
        // LinuxCNC 2.9: "K word missing with g33/g33.1".
        assert!(
            message.contains("the interpreter stopped") && message.to_lowercase().contains("g33"),
            "{message}"
        );
    }
}
