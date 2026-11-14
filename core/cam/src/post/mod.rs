// SPDX-License-Identifier: MIT
//! Post-processors: a program's toolpaths as the G-code of one controller
//! (docs/cam.md, "Post-processors").
//!
//! [`post`] walks the program the same way for every controller: the
//! notice that CAM is highly experimental ([`EXPERIMENTAL_NOTICE`]), the
//! header, then per operation the tool change, the spindle, the coolant
//! and the moves, then the end. A dialect ([`PostProcessor`]) writes only
//! what differs between controllers: the header and the footer, the tool
//! change, the spindle and coolant commands, pauses and comments. The
//! moves (G0, G1, G2, G3 with I and J from the start point, in the XY
//! plane) are the same in all of them and are written by [`Output`], which
//! also leaves out words that have not changed (modal values).

mod grbl;
mod linuxcnc;
mod mach;
mod reprap;

#[cfg(test)]
mod tests;

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::CamError;
use crate::machine::{Machine, MacroChanger, RackSlot, ToolChanger};
use crate::program::{Coolant, Program, SpindleDirection};
use crate::tool::Tool;
use crate::toolpath::{CycleKind, DrillCycle, Move, P3, move_points};

pub use grbl::{Grbl, GrblHal};
pub use linuxcnc::LinuxCnc;
pub use mach::{Mach3, Mach4};
pub use reprap::{MillenniumOs, RepRapFirmware};

/// The units of the program's numbers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    /// Millimetres (G21).
    #[default]
    Millimetres,
    /// Inches (G20).
    Inches,
}

fn yes() -> bool {
    true
}

fn default_tolerance() -> f64 {
    0.01
}

/// The user's choices for one post run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PostSettings {
    #[serde(default)]
    pub units: Units,
    /// Arcs as G2 and G3; off (or a dialect without arcs): short lines.
    #[serde(default = "yes")]
    pub arcs: bool,
    /// How far lines that stand for an arc may stray from it (mm).
    #[serde(default = "default_tolerance")]
    pub arc_tolerance: f64,
    /// N words on every line but `%`.
    #[serde(default)]
    pub line_numbers: bool,
    #[serde(default = "yes")]
    pub comments: bool,
    /// The tool length offset after each tool change; `None`: the
    /// dialect's default.
    #[serde(default)]
    pub tool_length_offset: Option<bool>,
    /// Drilling as canned cycles (G81, G83, G73) where the controller has
    /// them; the moves written out where it does not.
    #[serde(default)]
    pub canned_cycles: bool,
    /// Probe the work zero at the start (MillenniumOS's G6600); refused
    /// by the dialects that have no such command.
    #[serde(default)]
    pub probe_work_zero: bool,
}

impl Default for PostSettings {
    fn default() -> Self {
        PostSettings {
            units: Units::Millimetres,
            arcs: true,
            arc_tolerance: default_tolerance(),
            line_numbers: false,
            comments: true,
            tool_length_offset: None,
            canned_cycles: false,
            probe_work_zero: false,
        }
    }
}

/// How a dialect writes comments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommentStyle {
    /// `(text)`; parentheses in the text become brackets.
    Parentheses,
    /// `; text`
    Semicolon,
}

/// How a controller takes canned drilling cycles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CycleStyle {
    /// It has none: the moves are written out.
    None,
    /// Modal: `G98` and the cycle with the first hole, the other holes by
    /// their X and Y, then `G80`; between holes the tool goes back to the
    /// height it started from (the clearance height).
    Modal,
    /// One whole cycle line per hole, each followed by a rapid up to the
    /// clearance height (the cycle itself ends at the retract height).
    PerHole,
}

/// What the dialect knows of the program being written.
pub struct Job<'a> {
    pub program: &'a Program,
    pub machine: Option<&'a Machine>,
    pub settings: &'a PostSettings,
}

/// A tool change for the dialect to write.
pub struct ToolChange<'a> {
    pub tool: &'a Tool,
    /// The tool in the spindle before, if the program put one there.
    pub previous: Option<&'a Tool>,
    /// The first tool of the program.
    pub first: bool,
    /// The operator changes the tool (no changer, or a tool the changer
    /// has no slot for).
    pub manual: bool,
}

/// A controller's G-code dialect.
pub trait PostProcessor {
    /// The id the machine and the API name it by.
    fn id(&self) -> &'static str;
    /// The name people see.
    fn label(&self) -> &'static str;
    /// The file name extension, without the dot.
    fn extension(&self) -> &'static str {
        "nc"
    }
    fn comment_style(&self) -> CommentStyle {
        CommentStyle::Parentheses
    }
    /// Characters the controller acts on wherever they are in the stream
    /// (grbl's realtime commands), kept out of comments and names.
    fn stream_commands(&self) -> &'static str {
        ""
    }
    /// Whether the controller has G2 and G3.
    fn supports_arcs(&self) -> bool {
        true
    }
    /// Whether a tool length offset follows a tool change unless the
    /// settings say otherwise.
    fn tool_length_offset(&self) -> bool {
        false
    }
    /// The canned drilling cycles the controller has.
    fn canned_cycles(&self) -> CycleStyle {
        CycleStyle::None
    }
    /// Whether the header probes the work zero when the settings ask.
    fn probes_work_zero(&self) -> bool {
        false
    }
    /// Lines before everything else (a tape mark).
    fn start(&self, _out: &mut Output) {}
    /// The modes and the work offset, after the program's comments.
    fn header(&self, out: &mut Output, job: &Job) -> Result<(), CamError>;
    /// The end of the program, after the spindle is stopped and the tool
    /// is up.
    fn footer(&self, out: &mut Output, job: &Job);
    /// The controller's own tool change.
    fn tool_change(&self, out: &mut Output, change: &ToolChange) -> Result<(), CamError>;
    /// The tool length offset of `tool`, when asked for.
    fn length_offset(&self, out: &mut Output, tool: &Tool) -> Result<(), CamError> {
        out.line(format!("G43 H{}", tool.number));
        Ok(())
    }
    /// Stops for the operator with a message, to go on when told to.
    fn pause(&self, out: &mut Output, message: &str) {
        out.comment(message);
        out.line("M0");
    }
    fn spindle_on(
        &self,
        out: &mut Output,
        rpm: f64,
        direction: SpindleDirection,
    ) -> Result<(), CamError> {
        let m = match direction {
            SpindleDirection::Cw => "M3",
            SpindleDirection::Ccw => "M4",
        };
        out.line(format!("S{} {m}", out.rpm(rpm)));
        Ok(())
    }
    /// Whether the spindle command waits for the speed itself (no dwell).
    fn spindle_waits(&self) -> bool {
        false
    }
    fn spindle_off(&self, out: &mut Output) {
        out.line("M5");
    }
    fn coolant(&self, out: &mut Output, coolant: Coolant) -> Result<(), CamError> {
        let code = match coolant {
            Coolant::Off => "M9",
            Coolant::Flood => "M8",
            Coolant::Mist => "M7",
            Coolant::Air => {
                return Err(CamError::new(format!(
                    "{} has no command for air blast.",
                    self.label()
                )));
            }
        };
        out.line(code);
        Ok(())
    }
    /// A pause of `seconds`.
    fn dwell(&self, out: &mut Output, seconds: f64) {
        out.line(format!("G4 P{}", out.seconds(seconds)));
    }
    /// Moves to a place in machine coordinates, Z first.
    fn park(&self, out: &mut Output, at: P3) {
        out.line(format!("G53 G0 Z{}", out.len(at[2])));
        out.line(format!("G53 G0 X{} Y{}", out.len(at[0]), out.len(at[1])));
        out.forget_position();
    }
}

/// Every dialect, in the order people choose from.
pub fn dialects() -> Vec<Box<dyn PostProcessor>> {
    vec![
        Box::new(Grbl),
        Box::new(GrblHal),
        Box::new(RepRapFirmware),
        Box::new(MillenniumOs),
        Box::new(Mach3),
        Box::new(Mach4),
        Box::new(LinuxCnc),
    ]
}

/// The notice every program starts with, after a tape mark (the owner's
/// order of 13.11.2026, mitcad#1013): CAM is highly experimental, so the
/// code is checked before it cuts. Written even when comments are off.
/// Plain ASCII without anything a controller acts on in a comment: no
/// `?`, `~` or `!` (grbl's realtime commands), no parentheses, and no line
/// starts with a word LinuxCNC reads as a command (`MSG,`, `DEBUG,`,
/// `PRINT,`, `LOG…`, `PROBE…`).
pub const EXPERIMENTAL_NOTICE: &[&str] = &[
    "WARNING: MitCad CAM is highly experimental.",
    "Treat this machine code with caution. Check every toolpath",
    "in a simulator, then run it first without stock, at a low",
    "feed, with a hand on the emergency stop, before cutting.",
];

/// The dialect with the id.
pub fn dialect(id: &str) -> Option<Box<dyn PostProcessor>> {
    dialects().into_iter().find(|d| d.id() == id)
}

/// The lines of a program being written, with the modal state: every
/// move line names its motion (G0 to G3), which every controller reads the
/// same, and leaves out the axes and the feed that have not changed.
pub struct Output {
    lines: Vec<String>,
    inches: bool,
    numbers: bool,
    comments: bool,
    style: CommentStyle,
    stream_commands: &'static str,
    next_number: u32,
    words: [Option<String>; 3],
    feed: Option<String>,
    at: [Option<f64>; 3],
    /// The lines each call of [`Output::motion`] wrote: (first, end) as
    /// indices into `lines`.
    spans: Vec<(usize, usize)>,
    /// Lines that stand for the next moves (a canned cycle), one list per
    /// move, and where the tool is after them with the feed.
    replace: VecDeque<Vec<String>>,
    after_replace: Option<(P3, f64)>,
    /// Per operation, the lines of the tool change before it
    /// ([`Posted::changes`]).
    changes: Vec<Option<ChangeLines>>,
}

impl Output {
    pub fn new(settings: &PostSettings, style: CommentStyle) -> Self {
        Output {
            lines: Vec::new(),
            inches: settings.units == Units::Inches,
            numbers: settings.line_numbers,
            comments: settings.comments,
            style,
            stream_commands: "",
            next_number: 10,
            words: [None, None, None],
            feed: None,
            at: [None; 3],
            spans: Vec::new(),
            replace: VecDeque::new(),
            after_replace: None,
            changes: Vec::new(),
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

    /// A line as it is (`%`).
    pub fn raw(&mut self, text: impl Into<String>) {
        self.lines.push(text.into());
    }

    /// Keeps the characters the controller acts on anywhere in the stream
    /// (see [`PostProcessor::stream_commands`]) out of comments and names.
    pub fn set_stream_commands(&mut self, chars: &'static str) {
        self.stream_commands = chars;
    }

    /// Text the controller reads as it is, for a comment or a name in a
    /// line: parentheses in a parenthesis comment become brackets, what is
    /// not printable ASCII becomes `?`, and the characters the controller
    /// acts on in the stream (and `?` among them) become `_`.
    pub fn clean(&self, text: &str) -> String {
        let other = if self.stream_commands.contains('?') {
            '_'
        } else {
            '?'
        };
        text.chars()
            .map(|c| match c {
                '(' if self.style == CommentStyle::Parentheses => '[',
                ')' if self.style == CommentStyle::Parentheses => ']',
                c if self.stream_commands.contains(c) => '_',
                c if c.is_ascii() && !c.is_ascii_control() => c,
                _ => other,
            })
            .collect()
    }

    /// A comment line in the dialect's style, unless comments are off.
    pub fn comment(&mut self, text: &str) {
        if self.comments {
            self.notice(text);
        }
    }

    /// A comment line in the dialect's style even when comments are off
    /// (the [`EXPERIMENTAL_NOTICE`]).
    pub fn notice(&mut self, text: &str) {
        let clean = self.clean(text);
        let line = match self.style {
            CommentStyle::Parentheses => format!("({clean})"),
            CommentStyle::Semicolon => format!("; {clean}"),
        };
        self.lines.push(line);
    }

    /// A length (mm) in the program's units.
    pub fn len(&self, mm: f64) -> String {
        if self.inches {
            number(mm / 25.4, 4)
        } else {
            number(mm, 3)
        }
    }

    /// A feed (mm/min) in the program's units.
    pub fn feed(&self, mm_per_min: f64) -> String {
        if self.inches {
            number(mm_per_min / 25.4, 2)
        } else {
            number(mm_per_min, 1)
        }
    }

    pub fn rpm(&self, rpm: f64) -> String {
        number(rpm.round(), 0)
    }

    pub fn seconds(&self, s: f64) -> String {
        number(s, 3)
    }

    /// G21 or G20.
    pub fn units_code(&self) -> &'static str {
        if self.inches { "G20" } else { "G21" }
    }

    /// After moves the program did not make through [`Output`] (a tool
    /// change, machine coordinate moves): the position and the feed are
    /// not known any more.
    pub fn forget_position(&mut self) {
        self.words = [None, None, None];
        self.at = [None; 3];
        self.feed = None;
    }

    fn axis_words(&mut self, to: P3, mask: [bool; 3], all: bool) -> String {
        let mut s = String::new();
        for (k, name) in ["X", "Y", "Z"].iter().enumerate() {
            if !mask[k] {
                continue;
            }
            let w = self.len(to[k]);
            if all || self.words[k].as_deref() != Some(w.as_str()) {
                s.push(' ');
                s.push_str(name);
                s.push_str(&w);
            }
            self.words[k] = Some(w);
            self.at[k] = Some(to[k]);
        }
        s
    }

    fn feed_word(&mut self, feed: f64) -> String {
        let f = self.feed(feed);
        if self.feed.as_deref() == Some(f.as_str()) {
            String::new()
        } else {
            self.feed = Some(f.clone());
            format!(" F{f}")
        }
    }

    /// A move line, unless no axis word changed (the move is shorter than
    /// the numbers' last digit).
    fn emit(&mut self, g: &str, words: String, feed: String) {
        if !words.is_empty() {
            self.line(format!("{g}{words}{feed}"));
        }
    }

    /// A rapid of Z alone.
    pub fn rapid_z(&mut self, z: f64) {
        let w = self.axis_words([0.0, 0.0, z], [false, false, true], false);
        self.emit("G0", w, String::new());
    }

    /// A rapid: up first when it rises, across first when it sinks, so
    /// that it never moves down and across at once.
    pub fn rapid(&mut self, to: P3) {
        let rises = self.at[2].is_none_or(|z| to[2] > z + 1e-9);
        let order: [[bool; 3]; 2] = if rises {
            [[false, false, true], [true, true, false]]
        } else {
            [[true, true, false], [false, false, true]]
        };
        for mask in order {
            let w = self.axis_words(to, mask, false);
            self.emit("G0", w, String::new());
        }
    }

    pub fn linear(&mut self, to: P3, feed: f64) {
        let w = self.axis_words(to, [true, true, true], false);
        if w.is_empty() {
            return;
        }
        let f = self.feed_word(feed);
        self.emit("G1", w, f);
    }

    fn arc(&mut self, to: P3, center: [f64; 2], ccw: bool, feed: f64) -> Result<(), CamError> {
        let (Some(x), Some(y)) = (self.at[0], self.at[1]) else {
            return Err(CamError::new(
                "An arc starts where the tool's position is not known.",
            ));
        };
        let i = self.len(center[0] - x);
        let j = self.len(center[1] - y);
        let g = if ccw { "G3" } else { "G2" };
        let mut w = self.axis_words(to, [true, true, false], true);
        w.push_str(&self.axis_words(to, [false, false, true], false));
        let f = self.feed_word(feed);
        self.line(format!("{g}{w} I{i} J{j}{f}"));
        Ok(())
    }

    /// The next moves are written as these lines instead, one list per
    /// move (each move still goes through [`Output::motion`], so that
    /// every move keeps its lines); after them the tool is at `end` with
    /// the feed `feed` in force.
    pub fn replace_next(&mut self, lines: Vec<Vec<String>>, end: P3, feed: f64) {
        self.replace = lines.into();
        self.after_replace = Some((end, feed));
    }

    /// The tool is at `at` with `feed` in force, as if written here.
    pub fn assume(&mut self, at: P3, feed: f64) {
        for (k, v) in at.iter().enumerate() {
            self.words[k] = Some(self.len(*v));
            self.at[k] = Some(*v);
        }
        self.feed = Some(self.feed(feed));
    }

    /// Writes a move, arcs as lines when `arcs` is off, and remembers its
    /// lines ([`post_with_lines`]); a move a canned cycle stands for writes
    /// the cycle's lines given for it ([`Output::replace_next`]).
    pub fn motion(&mut self, m: &Move, arcs: bool, tolerance: f64) -> Result<(), CamError> {
        let first = self.lines.len();
        let written = self.write_motion(m, arcs, tolerance);
        self.spans.push((first, self.lines.len()));
        written
    }

    fn write_motion(&mut self, m: &Move, arcs: bool, tolerance: f64) -> Result<(), CamError> {
        if let Some(lines) = self.replace.pop_front() {
            for l in lines {
                self.line(l);
            }
            if self.replace.is_empty()
                && let Some((at, feed)) = self.after_replace.take()
            {
                self.assume(at, feed);
            }
            return Ok(());
        }
        match *m {
            Move::Rapid { to } => self.rapid(to),
            Move::Line { to, feed } => self.linear(to, feed),
            Move::Arc {
                to,
                center,
                ccw,
                feed,
            } => {
                if arcs {
                    self.arc(to, center, ccw, feed)?;
                } else {
                    let from = match self.at {
                        [Some(x), Some(y), Some(z)] => [x, y, z],
                        _ => {
                            return Err(CamError::new(
                                "An arc starts where the tool's position is not known.",
                            ));
                        }
                    };
                    for p in move_points(from, m, tolerance) {
                        self.linear(p, feed);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn finish(self) -> String {
        let mut s = self.lines.join("\n");
        s.push('\n');
        s
    }
}

/// `v` with at most `decimals` decimals, without trailing zeros, never
/// `-0`.
pub fn number(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    };
    if s == "-0" { "0".to_string() } else { s }
}

/// Fills a tool changer's line with the tool and its slot.
fn fill(
    line: &str,
    out: &Output,
    tool: &Tool,
    slot: &RackSlot,
    changer: &MacroChanger,
) -> Result<String, CamError> {
    let mut s = String::new();
    let mut rest = line;
    while let Some(open) = rest.find('{') {
        s.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            return Err(CamError::new(
                "A line of the tool changer's G-code has an unclosed brace.",
            ));
        };
        let value = match &after[..close] {
            "tool" => tool.number.to_string(),
            "name" => out.clean(&tool.name),
            "x" => out.len(slot.position[0]),
            "y" => out.len(slot.position[1]),
            "z" => out.len(slot.position[2]),
            "safe_z" => out.len(changer.safe_z),
            _ => {
                return Err(CamError::new(
                    "A line of the tool changer's G-code names a value it does not know in braces.",
                ));
            }
        };
        s.push_str(&value);
        rest = &after[close + 1..];
    }
    s.push_str(rest);
    Ok(s)
}

/// The words of a drilling cycle at a hole.
fn cycle_words(out: &Output, c: &DrillCycle, hole: [f64; 2]) -> String {
    let mut s = format!(
        "{} X{} Y{} Z{} R{}",
        c.kind.code(),
        out.len(hole[0]),
        out.len(hole[1]),
        out.len(c.bottom),
        out.len(c.retract)
    );
    if c.kind != CycleKind::Drill {
        s.push_str(&format!(" Q{}", out.len(c.peck)));
    }
    s.push_str(&format!(" F{}", out.feed(c.feed)));
    s
}

/// The lines a drilling cycle is written as, one list per move it stands
/// for: a hole's cycle with the move that brings the tool down to the
/// retract height above it, the end with its last move. `None` when the
/// moves are not what the cycle says (they are written out then).
fn cycle_lines(
    c: &DrillCycle,
    moves: &[Move],
    style: CycleStyle,
    out: &Output,
) -> Option<(Vec<Vec<String>>, P3)> {
    let near = |a: P3, b: P3| (0..3).all(|k| (a[k] - b[k]).abs() <= 1e-9);
    let peck_ok = match c.kind {
        CycleKind::Drill => true,
        CycleKind::Peck | CycleKind::ChipBreak => c.peck.is_finite() && c.peck > 0.0,
    };
    if style == CycleStyle::None
        || c.holes.is_empty()
        || c.first == 0
        || c.count == 0
        || c.first + c.count > moves.len()
        || !(c.clearance >= c.retract && c.retract > c.bottom && c.feed > 0.0 && peck_ok)
    {
        return None;
    }
    let h0 = c.holes[0];
    if !near(moves[c.first - 1].to(), [h0[0], h0[1], c.clearance]) {
        return None;
    }
    let last = c.holes[c.holes.len() - 1];
    let end = [last[0], last[1], c.clearance];
    let span = &moves[c.first..c.first + c.count];
    if !near(span[span.len() - 1].to(), end) {
        return None;
    }
    let mut lines: Vec<Vec<String>> = vec![Vec::new(); span.len()];
    let mut from = 0;
    for (k, h) in c.holes.iter().enumerate() {
        let above = [h[0], h[1], c.retract];
        let j = (from..span.len()).find(|&j| near(span[j].to(), above))?;
        lines[j] = match style {
            CycleStyle::Modal if k == 0 => vec![format!("G98 {}", cycle_words(out, c, *h))],
            CycleStyle::Modal => vec![format!("X{} Y{}", out.len(h[0]), out.len(h[1]))],
            _ => vec![
                cycle_words(out, c, *h),
                format!("G0 Z{}", out.len(c.clearance)),
            ],
        };
        from = j + 1;
    }
    if style == CycleStyle::Modal {
        lines[span.len() - 1].push("G80".to_string());
    }
    Some((lines, end))
}

fn change_message(tool: &Tool) -> String {
    format!("Change to tool {}: {}", tool.number, tool.name)
}

/// The program as the dialect's G-code.
pub fn post(
    program: &Program,
    dialect: &dyn PostProcessor,
    settings: &PostSettings,
    machine: Option<&Machine>,
) -> Result<String, CamError> {
    write(program, dialect, settings, machine).map(Output::finish)
}

/// Where a move of the program was written: its first line (1-based, as
/// an editor counts them) and how many lines it took (0: nothing, the move
/// was shorter than the numbers' last digit; a rapid takes up to two, an
/// arc written as lines many).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveLines {
    pub first: usize,
    pub count: usize,
}

/// Where the tool change before an operation was written: the lines that
/// put the tool in the spindle away (a macro changer's `unload` lines;
/// none for the other changers), then the lines that bring the
/// operation's tool (a macro changer's `load` lines, or the dialect's
/// tool change or its pause for the operator).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeLines {
    pub unload: MoveLines,
    pub load: MoveLines,
    /// Whether the program moves the spindle through the change itself
    /// (the `load` lines are a macro changer's), rather than the operator
    /// or the controller's own changer.
    pub by_program: bool,
}

/// A posted program with the lines of its moves.
#[derive(Clone, Debug, PartialEq)]
pub struct Posted {
    pub text: String,
    /// Per operation, per move of its toolpath; empty when the moves were
    /// not all written one by one (they cannot be told apart then).
    pub moves: Vec<Vec<MoveLines>>,
    /// Per operation, the tool change written before it, if any (for the
    /// check of the changer's moves, [`crate::collision::check_tool_changes`]).
    pub changes: Vec<Option<ChangeLines>>,
}

/// The program as the dialect's G-code, with the lines of its moves (for
/// the findings of the checks).
pub fn post_with_lines(
    program: &Program,
    dialect: &dyn PostProcessor,
    settings: &PostSettings,
    machine: Option<&Machine>,
) -> Result<Posted, CamError> {
    let mut out = write(program, dialect, settings, machine)?;
    let spans = std::mem::take(&mut out.spans);
    let changes = std::mem::take(&mut out.changes);
    let total: usize = program
        .operations
        .iter()
        .map(|op| op.toolpath.moves.len())
        .sum();
    let mut moves = Vec::new();
    if spans.len() == total {
        let mut rest = spans.as_slice();
        for op in &program.operations {
            let (mine, after) = rest.split_at(op.toolpath.moves.len());
            moves.push(
                mine.iter()
                    .map(|&(first, end)| MoveLines {
                        first: first + 1,
                        count: end - first,
                    })
                    .collect(),
            );
            rest = after;
        }
    }
    Ok(Posted {
        text: out.finish(),
        moves,
        changes,
    })
}

/// Writes the program into an [`Output`].
fn write(
    program: &Program,
    dialect: &dyn PostProcessor,
    settings: &PostSettings,
    machine: Option<&Machine>,
) -> Result<Output, CamError> {
    program.validate()?;
    if !(settings.arc_tolerance.is_finite() && settings.arc_tolerance > 0.0) {
        return Err(CamError::new("The arc tolerance must be more than zero."));
    }
    if settings.probe_work_zero && !dialect.probes_work_zero() {
        return Err(CamError::new(format!(
            "{} has no command for probing the work zero.",
            dialect.label()
        )));
    }
    let style = if settings.canned_cycles {
        dialect.canned_cycles()
    } else {
        CycleStyle::None
    };
    let job = Job {
        program,
        machine,
        settings,
    };
    let arcs = settings.arcs && dialect.supports_arcs();
    let tlo = settings
        .tool_length_offset
        .unwrap_or(dialect.tool_length_offset());
    let changer = machine.map(|m| &m.tool_changer);
    let spin_up = machine.map(|m| m.spindle.spin_up_seconds).unwrap_or(0.0);
    let mut out = Output::new(settings, dialect.comment_style());
    out.set_stream_commands(dialect.stream_commands());
    dialect.start(&mut out);
    for line in EXPERIMENTAL_NOTICE {
        out.notice(line);
    }
    out.comment(&program.name);
    for t in program.tools() {
        out.comment(&format!(
            "T{} {}, D{} mm",
            t.number,
            t.name,
            crate::tool::fmt_mm(t.diameter)
        ));
    }
    dialect.header(&mut out, &job)?;
    out.rapid_z(program.safe_z);
    let mut tool: Option<&Tool> = None;
    let mut spindle: Option<(f64, SpindleDirection)> = None;
    let mut coolant = Coolant::Off;
    for op in &program.operations {
        out.comment(&op.name);
        let mut change_lines = None;
        if tool.is_none_or(|t| t.number != op.tool.number) {
            if spindle.take().is_some() {
                dialect.spindle_off(&mut out);
            }
            if coolant != Coolant::Off {
                dialect.coolant(&mut out, Coolant::Off)?;
                coolant = Coolant::Off;
            }
            out.rapid_z(program.safe_z);
            let change = ToolChange {
                tool: &op.tool,
                previous: tool,
                first: tool.is_none(),
                manual: false,
            };
            let unload_from = out.lines.len();
            let mut load_from = unload_from;
            let mut by_program = false;
            match changer {
                Some(ToolChanger::Macro(mc)) => {
                    if let Some(prev) = tool
                        && let Some(slot) = mc.slot(prev.number)
                    {
                        for l in &mc.unload {
                            let text = fill(l, &out, prev, slot, mc)?;
                            out.line(text);
                        }
                    }
                    load_from = out.lines.len();
                    match mc.slot(op.tool.number) {
                        Some(slot) => {
                            for l in &mc.load {
                                let text = fill(l, &out, &op.tool, slot, mc)?;
                                out.line(text);
                            }
                            by_program = true;
                        }
                        None => dialect.pause(&mut out, &change_message(&op.tool)),
                    }
                    out.forget_position();
                }
                Some(ToolChanger::Controller) => {
                    dialect.tool_change(&mut out, &change)?;
                    out.forget_position();
                }
                Some(ToolChanger::Manual) | None => {
                    dialect.tool_change(
                        &mut out,
                        &ToolChange {
                            manual: true,
                            ..change
                        },
                    )?;
                    out.forget_position();
                }
            }
            change_lines = Some(ChangeLines {
                unload: MoveLines {
                    first: unload_from + 1,
                    count: load_from - unload_from,
                },
                load: MoveLines {
                    first: load_from + 1,
                    count: out.lines.len() - load_from,
                },
                by_program,
            });
            if tlo {
                dialect.length_offset(&mut out, &op.tool)?;
            }
            out.rapid_z(program.safe_z);
            tool = Some(&op.tool);
        }
        out.changes.push(change_lines);
        let wanted = if op.spindle_rpm > 0.0 {
            Some((op.spindle_rpm, op.spindle))
        } else {
            None
        };
        if wanted != spindle {
            match wanted {
                Some((rpm, dir)) => {
                    dialect.spindle_on(&mut out, rpm, dir)?;
                    if spin_up > 0.0 && !dialect.spindle_waits() {
                        dialect.dwell(&mut out, spin_up);
                    }
                }
                None => dialect.spindle_off(&mut out),
            }
            spindle = wanted;
        }
        if op.coolant != coolant {
            dialect.coolant(&mut out, op.coolant)?;
            coolant = op.coolant;
        }
        let moves = &op.toolpath.moves;
        for (i, m) in moves.iter().enumerate() {
            if style != CycleStyle::None
                && let Some(c) = op.toolpath.cycles.iter().find(|c| c.first == i)
                && let Some((lines, end)) = cycle_lines(c, moves, style, &out)
            {
                out.replace_next(lines, end, c.feed);
            }
            out.motion(m, arcs, settings.arc_tolerance)?;
        }
    }
    if spindle.is_some() {
        dialect.spindle_off(&mut out);
    }
    if coolant != Coolant::Off {
        dialect.coolant(&mut out, Coolant::Off)?;
    }
    out.rapid_z(program.safe_z);
    if let Some(at) = machine.and_then(|m| m.park) {
        dialect.park(&mut out, at);
    }
    dialect.footer(&mut out, &job);
    Ok(out)
}
