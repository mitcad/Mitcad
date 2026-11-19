// SPDX-License-Identifier: MIT
//! The turning post: one driver for every lathe controller and a thin
//! dialect per controller ([`TurningPost`]), as the milling posts are
//! (docs/cam.md, "One post driver, thin dialects").
//!
//! The driver writes, after a tape mark and the program's number, the
//! notice that CAM is highly experimental, the comments, the dialect's
//! header, then per operation its name, at a new tool the coolant off, the
//! move to the tool change position and the turret's index, the spindle
//! (a constant surface speed under its limit, or a constant speed), the
//! coolant and the moves, then the end. Every move line names its motion
//! (G0 to G3, G32), X is a diameter, arcs carry I (a radius) and K from
//! the start to the centre, and unchanged words are left out.
//!
//! Where the dialect has them and the settings ask (`canned_cycles`), a
//! roughing toolpath is written as its stock removal cycle (G71) with the
//! finished contour as its blocks, a finishing toolpath along the same
//! contour as the finishing cycle (G70) that runs those blocks again, and
//! threading as the threading cycle (G76); otherwise the moves.

use serde::Serialize;

use super::tool::TurningTool;
use super::{PXZ, TurnCycle, TurnMove, TurnSpindle, TurningProgram};
use crate::CamError;
use crate::post::{EXPERIMENTAL_NOTICE, PostSettings, Units, number};
use crate::program::{Coolant, SpindleDirection};

/// A number with a decimal point always (`10.`, `0.`, `-1.6`): a Fanuc
/// control reads a number without one in its least increments.
pub fn decimal(v: f64, decimals: usize) -> String {
    let s = number(v, decimals);
    if s.contains('.') { s } else { format!("{s}.") }
}

/// The lines of a turning program being written, with the modal state.
#[derive(Clone)]
pub struct TurnOutput {
    lines: Vec<String>,
    inches: bool,
    numbers: bool,
    comments: bool,
    next_number: u32,
    next_label: u32,
    motion: Option<&'static str>,
    x: Option<String>,
    z: Option<String>,
    feed: Option<String>,
    at: Option<PXZ>,
}

impl TurnOutput {
    pub fn new(settings: &PostSettings) -> Self {
        TurnOutput {
            lines: Vec::new(),
            inches: settings.units == Units::Inches,
            numbers: settings.line_numbers,
            comments: settings.comments,
            next_number: 10,
            next_label: 1000,
            motion: None,
            x: None,
            z: None,
            feed: None,
            at: None,
        }
    }

    /// A line of code (numbered when asked).
    pub fn line(&mut self, text: impl Into<String>) {
        let text = text.into();
        if self.numbers {
            self.lines.push(format!("N{} {text}", self.next_number));
            self.next_number += 10;
        } else {
            self.lines.push(text);
        }
    }

    /// A line as it is (`%`, the program number).
    pub fn raw(&mut self, text: impl Into<String>) {
        self.lines.push(text.into());
    }

    /// Text a Fanuc control reads in a comment: upper case printable ASCII,
    /// parentheses as brackets.
    pub fn clean(text: &str) -> String {
        text.chars()
            .map(|c| match c {
                '(' => '[',
                ')' => ']',
                c if c.is_ascii() && !c.is_ascii_control() => c.to_ascii_uppercase(),
                _ => '?',
            })
            .collect()
    }

    pub fn comment(&mut self, text: &str) {
        if self.comments {
            self.notice(text);
        }
    }

    /// A comment even when comments are off.
    pub fn notice(&mut self, text: &str) {
        self.lines.push(format!("({})", Self::clean(text)));
    }

    /// A length (mm) in the program's units.
    pub fn len(&self, mm: f64) -> String {
        if self.inches {
            decimal(mm / 25.4, 4)
        } else {
            decimal(mm, 3)
        }
    }

    /// A radius written as a diameter.
    pub fn dia(&self, radius: f64) -> String {
        self.len(2.0 * radius)
    }

    /// A feed per revolution (mm/rev) or a lead in the program's units.
    pub fn per_rev(&self, mm: f64) -> String {
        if self.inches {
            decimal(mm / 25.4, 6)
        } else {
            decimal(mm, 4)
        }
    }

    /// A length in the control's least increments (micrometres, or tenths
    /// of a thousandth of an inch), as G76 takes its depths.
    pub fn increments(&self, mm: f64) -> String {
        let v = if self.inches {
            mm / 25.4 * 10_000.0
        } else {
            mm * 1000.0
        };
        format!("{}", v.round() as i64)
    }

    pub fn units_code(&self) -> &'static str {
        if self.inches { "G20" } else { "G21" }
    }

    /// A surface speed (m/min) in the program's units (ft/min in inches).
    pub fn surface_speed(&self, m_per_min: f64) -> String {
        if self.inches {
            number((m_per_min * 3.280_84).round(), 0)
        } else {
            number(m_per_min.round(), 0)
        }
    }

    /// After a move the program did not make through the output: the
    /// position and the modal words are not known.
    pub fn forget(&mut self) {
        self.motion = None;
        self.x = None;
        self.z = None;
        self.feed = None;
        self.at = None;
    }

    /// After a cycle: the tool is back at `at`; the motion and the feed
    /// are written again.
    fn after_cycle(&mut self, at: PXZ) {
        self.motion = None;
        self.feed = None;
        self.x = Some(self.dia(at[0]));
        self.z = Some(self.len(at[1]));
        self.at = Some(at);
    }

    /// The words of a move from where the tool is, with the modal state
    /// brought along; none when it moves nothing at the program's digits.
    pub fn move_text(&mut self, m: &TurnMove) -> Option<String> {
        let to = m.to();
        let x = self.dia(to[0]);
        let z = self.len(to[1]);
        let (motion, feed) = match *m {
            TurnMove::Rapid { .. } => ("G0", None),
            TurnMove::Line { feed, .. } => ("G1", Some(self.per_rev(feed))),
            TurnMove::Arc { cw, feed, .. } => {
                (if cw { "G2" } else { "G3" }, Some(self.per_rev(feed)))
            }
            TurnMove::Thread { lead, .. } => ("G32", Some(self.per_rev(lead))),
        };
        let moved_x = self.x.as_deref() != Some(x.as_str());
        let moved_z = self.z.as_deref() != Some(z.as_str());
        if !moved_x && !moved_z {
            self.at = Some(to);
            return None;
        }
        let mut words: Vec<String> = Vec::new();
        if self.motion != Some(motion) {
            words.push(motion.to_owned());
        }
        if let TurnMove::Arc { center, .. } = *m {
            let from = self.at.unwrap_or(to);
            words.push(format!("X{x}"));
            words.push(format!("Z{z}"));
            words.push(format!("I{}", self.len(center[0] - from[0])));
            words.push(format!("K{}", self.len(center[1] - from[1])));
        } else {
            if moved_x {
                words.push(format!("X{x}"));
            }
            if moved_z {
                words.push(format!("Z{z}"));
            }
        }
        if let Some(f) = feed
            && self.feed.as_deref() != Some(f.as_str())
        {
            words.push(format!("F{f}"));
            self.feed = Some(f);
        }
        self.motion = Some(motion);
        self.x = Some(x);
        self.z = Some(z);
        self.at = Some(to);
        Some(words.join(" "))
    }

    /// Writes a move.
    pub fn motion(&mut self, m: &TurnMove) {
        if let Some(text) = self.move_text(m) {
            self.line(text);
        }
    }

    /// Writes blocks a cycle refers to by their sequence numbers: numbered
    /// lines when every line is, else `N` on the first and the last.
    /// Returns those two numbers.
    fn blocks(&mut self, texts: &[String]) -> (u32, u32) {
        if self.numbers {
            let first = self.next_number;
            for t in texts {
                self.line(t.clone());
            }
            (first, self.next_number - 10)
        } else {
            let first = self.next_label;
            let last = first + 10;
            self.next_label += 100;
            let n = texts.len();
            for (i, t) in texts.iter().enumerate() {
                if i == 0 {
                    self.lines.push(format!("N{first} {t}"));
                } else if i + 1 == n {
                    self.lines.push(format!("N{last} {t}"));
                } else {
                    self.lines.push(t.clone());
                }
            }
            (first, if n == 1 { first } else { last })
        }
    }

    pub fn finish(self) -> String {
        let mut text = self.lines.join("\n");
        text.push('\n');
        text
    }
}

/// A lathe controller's dialect: what differs between controllers.
pub trait TurningPost {
    fn id(&self) -> &'static str;
    fn label(&self) -> &'static str;
    fn extension(&self) -> &'static str {
        "nc"
    }
    /// The tape mark, the program's number and its name.
    fn start(&self, out: &mut TurnOutput, program: &TurningProgram);
    /// The modes and the work offset.
    fn header(&self, out: &mut TurnOutput, program: &TurningProgram);
    fn footer(&self, out: &mut TurnOutput, program: &TurningProgram);
    /// Indexes the turret to the tool with its offset.
    fn tool_change(&self, out: &mut TurnOutput, tool: &TurningTool);
    /// The spindle's speed limit (for a constant surface speed).
    fn spindle_limit(&self, out: &mut TurnOutput, rpm: f64);
    fn spindle_on(&self, out: &mut TurnOutput, spindle: &TurnSpindle, direction: SpindleDirection);
    fn spindle_off(&self, out: &mut TurnOutput) {
        out.line("M5");
    }
    fn coolant(&self, out: &mut TurnOutput, coolant: Coolant) -> Result<(), CamError> {
        let code = match coolant {
            Coolant::Off => "M9",
            Coolant::Flood => "M8",
            Coolant::Mist | Coolant::Air => {
                return Err(CamError::new(format!(
                    "{} has only flood coolant here.",
                    self.label()
                )));
            }
        };
        out.line(code);
        Ok(())
    }
    /// Whether it has the stock removal, finishing and threading cycles.
    fn has_cycles(&self) -> bool {
        false
    }
    /// The stock removal cycle's lines before its blocks, given their
    /// sequence numbers.
    fn rough_cycle(&self, _out: &mut TurnOutput, _cycle: &TurnCycle, _blocks: (u32, u32)) {}
    /// The finishing cycle over earlier blocks.
    fn finish_cycle(&self, _out: &mut TurnOutput, _blocks: (u32, u32)) {}
    /// The threading cycle; false when it cannot write this one (the
    /// moves are written instead).
    fn thread_cycle(&self, _out: &mut TurnOutput, _cycle: &TurnCycle) -> bool {
        false
    }
}

/// Fanuc lathe controls (Series 0i-TF, 31i-B and their kin) in G-code
/// system A: X as a diameter, feeds per revolution (G99), G71, G70 and
/// the two-block G76.
pub struct FanucTurning;

impl TurningPost for FanucTurning {
    fn id(&self) -> &'static str {
        "fanuc_turning"
    }

    fn label(&self) -> &'static str {
        "Fanuc (turning)"
    }

    fn start(&self, out: &mut TurnOutput, program: &TurningProgram) {
        out.raw("%");
        if out.comments {
            out.raw(format!(
                "O{:04} ({})",
                program.number,
                TurnOutput::clean(&program.name)
            ));
        } else {
            out.raw(format!("O{:04}", program.number));
        }
    }

    fn header(&self, out: &mut TurnOutput, program: &TurningProgram) {
        out.line(format!("{} G18 G40 G99", out.units_code()));
        out.line(format!("G{}", 53 + u32::from(program.work_offset)));
    }

    fn footer(&self, out: &mut TurnOutput, _program: &TurningProgram) {
        out.line("M30");
        out.raw("%");
    }

    fn tool_change(&self, out: &mut TurnOutput, tool: &TurningTool) {
        out.line(format!("T{:02}{:02}", tool.number, tool.offset_number()));
    }

    fn spindle_limit(&self, out: &mut TurnOutput, rpm: f64) {
        out.line(format!("G50 S{}", number(rpm.round(), 0)));
    }

    fn spindle_on(&self, out: &mut TurnOutput, spindle: &TurnSpindle, direction: SpindleDirection) {
        let m = match direction {
            SpindleDirection::Cw => "M3",
            SpindleDirection::Ccw => "M4",
        };
        match *spindle {
            TurnSpindle::Css { surface_speed, .. } => {
                out.line(format!("G96 S{} {m}", out.surface_speed(surface_speed)));
            }
            TurnSpindle::Rpm { rpm } => {
                out.line(format!("G97 S{} {m}", number(rpm.round(), 0)));
            }
        }
    }

    fn has_cycles(&self) -> bool {
        true
    }

    fn rough_cycle(&self, out: &mut TurnOutput, cycle: &TurnCycle, (ns, nf): (u32, u32)) {
        let TurnCycle::Rough {
            depth,
            retract,
            allowance_x,
            allowance_z,
            feed,
            profile,
            start,
            ..
        } = cycle
        else {
            return;
        };
        // Inside, the contour lies away from the axis from the start: the
        // allowance is written negative.
        let inside = profile.first().is_some_and(|m| m.to()[0] > start[0] + 1e-9);
        let u = if inside { -allowance_x } else { *allowance_x };
        out.line(format!("G71 U{} R{}", out.len(*depth), out.len(*retract)));
        out.line(format!(
            "G71 P{ns} Q{nf} U{} W{} F{}",
            out.dia(u),
            out.len(*allowance_z),
            out.per_rev(*feed)
        ));
    }

    fn finish_cycle(&self, out: &mut TurnOutput, (ns, nf): (u32, u32)) {
        out.line(format!("G70 P{ns} Q{nf}"));
    }

    fn thread_cycle(&self, out: &mut TurnOutput, cycle: &TurnCycle) -> bool {
        let TurnCycle::Thread {
            root_x,
            end_z,
            height,
            first_depth,
            min_depth,
            finish_allowance,
            spring_passes,
            angle,
            chamfer,
            lead,
            ..
        } = cycle
        else {
            return false;
        };
        let angle = angle.round();
        if ![0.0, 29.0, 30.0, 55.0, 60.0, 80.0].contains(&angle)
            || !(1..=99).contains(spring_passes)
            || !(0.0..=9.9).contains(chamfer)
        {
            return false;
        }
        out.line(format!(
            "G76 P{:02}{:02}{:02} Q{} R{}",
            spring_passes,
            (chamfer * 10.0).round() as u32,
            angle as u32,
            out.increments(*min_depth),
            out.len(*finish_allowance)
        ));
        out.line(format!(
            "G76 X{} Z{} P{} Q{} F{}",
            out.dia(*root_x),
            out.len(*end_z),
            out.increments(*height),
            out.increments(*first_depth),
            out.per_rev(*lead)
        ));
        true
    }
}

/// Every turning dialect.
pub fn turning_dialects() -> Vec<Box<dyn TurningPost>> {
    vec![Box::new(FanucTurning)]
}

/// The turning dialect with the id.
pub fn turning_dialect(id: &str) -> Option<Box<dyn TurningPost>> {
    turning_dialects().into_iter().find(|d| d.id() == id)
}

/// A dialect as the api lists it.
#[derive(Clone, Debug, Serialize)]
pub struct DialectInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub extension: &'static str,
}

/// Whether two cycle profiles are the same contour (feeds aside).
fn same_contour(a: &[TurnMove], b: &[TurnMove]) -> bool {
    let close = |p: PXZ, q: PXZ| (p[0] - q[0]).abs() < 1e-9 && (p[1] - q[1]).abs() < 1e-9;
    a.len() == b.len()
        && a.iter().zip(b).all(|(m, n)| match (*m, *n) {
            (TurnMove::Rapid { to: p }, TurnMove::Rapid { to: q })
            | (TurnMove::Line { to: p, .. }, TurnMove::Line { to: q, .. }) => close(p, q),
            (
                TurnMove::Arc {
                    to: p,
                    center: c,
                    cw,
                    ..
                },
                TurnMove::Arc {
                    to: q,
                    center: d,
                    cw: dw,
                    ..
                },
            ) => close(p, q) && close(c, d) && cw == dw,
            _ => false,
        })
}

/// The profile with every feed set to `feed`.
fn with_feed(profile: &[TurnMove], feed: f64) -> Vec<TurnMove> {
    profile
        .iter()
        .map(|m| match *m {
            TurnMove::Line { to, .. } => TurnMove::Line { to, feed },
            TurnMove::Arc { to, center, cw, .. } => TurnMove::Arc {
                to,
                center,
                cw,
                feed,
            },
            other => other,
        })
        .collect()
}

/// Writes a turning program in a dialect.
pub fn turning_post(
    program: &TurningProgram,
    dialect: &dyn TurningPost,
    settings: &PostSettings,
) -> Result<String, CamError> {
    program.validate()?;
    if settings.probe_work_zero {
        return Err(CamError::new(format!(
            "{} has no probing of the work zero here.",
            dialect.label()
        )));
    }
    let cycles = settings.canned_cycles && dialect.has_cycles();
    // Which roughing cycle each finishing cycle runs again, and the feed
    // its blocks carry (the finishing's, which only G70 uses).
    let mut finish_of: Vec<Option<usize>> = vec![None; program.operations.len()];
    let mut block_feed: Vec<Option<f64>> = vec![None; program.operations.len()];
    if cycles {
        for (j, op) in program.operations.iter().enumerate() {
            let Some(TurnCycle::Finish { profile, .. }) = &op.path.cycle else {
                continue;
            };
            let rough = (0..j).rev().find(|&i| {
                matches!(&program.operations[i].path.cycle,
                    Some(TurnCycle::Rough { profile: p, .. }) if same_contour(p, profile))
            });
            if let Some(i) = rough {
                finish_of[j] = Some(i);
                if block_feed[i].is_none() {
                    block_feed[i] = profile.iter().find_map(TurnMove::feed);
                }
            }
        }
    }
    let mut out = TurnOutput::new(settings);
    dialect.start(&mut out, program);
    for line in EXPERIMENTAL_NOTICE {
        out.notice(line);
    }
    out.comment(&program.name);
    let mut seen: Vec<u32> = Vec::new();
    for op in &program.operations {
        if !seen.contains(&op.tool.number) {
            seen.push(op.tool.number);
            out.comment(&format!(
                "T{:02}{:02} {}",
                op.tool.number,
                op.tool.offset_number(),
                op.tool.name
            ));
        }
    }
    dialect.header(&mut out, program);
    let change = TurnMove::Rapid {
        to: program.change_position,
    };
    let mut tool: Option<&TurningTool> = None;
    let mut coolant = Coolant::Off;
    let mut running: Option<SpindleDirection> = None;
    let mut limit: Option<f64> = None;
    let mut speed: Option<TurnSpindle> = None;
    let mut blocks: Vec<Option<(u32, u32)>> = vec![None; program.operations.len()];
    for (index, op) in program.operations.iter().enumerate() {
        out.comment(&op.name);
        let threads = op
            .path
            .moves
            .iter()
            .any(|m| matches!(m, TurnMove::Thread { .. }));
        if threads && matches!(op.spindle, TurnSpindle::Css { .. }) {
            return Err(CamError::new(format!(
                "{}: threading needs a constant spindle speed.",
                op.name
            )));
        }
        if tool != Some(&op.tool) {
            if coolant != Coolant::Off {
                dialect.coolant(&mut out, Coolant::Off)?;
                coolant = Coolant::Off;
            }
            out.forget();
            out.motion(&change);
            dialect.tool_change(&mut out, &op.tool);
            out.forget();
            tool = Some(&op.tool);
            speed = None;
        }
        if running.is_some_and(|d| d != op.direction) {
            dialect.spindle_off(&mut out);
            running = None;
            speed = None;
        }
        if let TurnSpindle::Css { max_rpm, .. } = op.spindle
            && limit != Some(max_rpm)
        {
            dialect.spindle_limit(&mut out, max_rpm);
            limit = Some(max_rpm);
        }
        if speed != Some(op.spindle) {
            dialect.spindle_on(&mut out, &op.spindle, op.direction);
            speed = Some(op.spindle);
            running = Some(op.direction);
        }
        if op.coolant != coolant {
            dialect.coolant(&mut out, op.coolant)?;
            coolant = op.coolant;
        }
        let moves = &op.path.moves;
        let (skip_from, skip_to) = match (&op.path.cycle, cycles) {
            (Some(c), true) => {
                let (first, count) = c.span();
                (first, first + count)
            }
            _ => (usize::MAX, usize::MAX),
        };
        let mut i = 0;
        while i < moves.len() {
            if i != skip_from {
                out.motion(&moves[i]);
                i += 1;
                continue;
            }
            let cycle = op.path.cycle.as_ref().expect("a cycle was found");
            let end = moves[skip_to - 1].to();
            let written = match cycle {
                TurnCycle::Rough { profile, .. } => {
                    let profile = match block_feed[index] {
                        Some(feed) => with_feed(profile, feed),
                        None => profile.clone(),
                    };
                    // The blocks' words from the cycle's start.
                    let mut dry = out.clone();
                    dry.motion = None;
                    dry.feed = None;
                    let texts: Vec<String> =
                        profile.iter().filter_map(|m| dry.move_text(m)).collect();
                    let numbers = out.numbers;
                    let range = if numbers {
                        let first = out.next_number + 20;
                        (first, first + 10 * (texts.len() as u32 - 1))
                    } else {
                        (out.next_label, out.next_label + 10)
                    };
                    dialect.rough_cycle(&mut out, cycle, range);
                    let got = out.blocks(&texts);
                    debug_assert_eq!(got, range);
                    blocks[index] = Some(got);
                    true
                }
                TurnCycle::Finish { .. } => match finish_of[index].and_then(|r| blocks[r]) {
                    Some(range) => {
                        dialect.finish_cycle(&mut out, range);
                        true
                    }
                    None => false,
                },
                TurnCycle::Thread { .. } => dialect.thread_cycle(&mut out, cycle),
            };
            if written {
                out.after_cycle(end);
                i = skip_to;
            } else {
                out.motion(&moves[i]);
                i += 1;
            }
        }
    }
    if coolant != Coolant::Off {
        dialect.coolant(&mut out, Coolant::Off)?;
    }
    out.motion(&change);
    if running.is_some() {
        dialect.spindle_off(&mut out);
    }
    dialect.footer(&mut out, program);
    Ok(out.finish())
}
