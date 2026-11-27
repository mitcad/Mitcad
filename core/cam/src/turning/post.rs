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
//!
//! After the sub-spindle's hand-over (mitcad#2319) the operations on the
//! part's back are written in the sub-spindle's work offset: turning with
//! its spindle codes, live tools (mitcad#2375) with the sub-spindle stopped
//! and its own C axis on. On a control that mirrors only Z there
//! (`MillTurn::sub_mirror_z_only`, mitcad#2392) the live tools' Y and C are
//! written turned round, their arcs across Y the other way.
//!
//! Two dialects (mitcad#2425): Fanuc lathe controls ([`FanucTurning`]) and
//! LinuxCNC on a lathe ([`LinuxCncTurning`]), which writes every operation
//! longhand, its threads as G33 moves.
//!
//! Manual G-code (mitcad#2506, [`super::ManualBlock`]) is written as it is
//! between the operations, fenced by comments, with a defined state before
//! it and everything stated again after it.

use serde::Serialize;

use super::check::Control;
use super::tool::TurningTool;
use super::{HandOver, PXZ, TurnCycle, TurnMove, TurnSpindle, TurningProgram};
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
    // The dialect's threading move and the word of its lead (mitcad#2425):
    // G32 with a modal F (Fanuc), or G33 with K in every block (LinuxCNC).
    thread_code: &'static str,
    lead_word: char,
    /// The settings' tool length offset after a tool change (`None`: the
    /// dialect's default) and how far the control may blend corners (mm).
    pub tool_length_offset: Option<bool>,
    pub arc_tolerance: f64,
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
            thread_code: "G32",
            lead_word: 'F',
            tool_length_offset: settings.tool_length_offset,
            arc_tolerance: settings.arc_tolerance,
        }
    }

    /// Writes threading moves as `code` with their lead in `word`: `F`
    /// modal as other feeds, any other letter in every block, as the
    /// distance along the move per revolution (mitcad#2425).
    pub fn set_thread_words(&mut self, code: &'static str, word: char) {
        self.thread_code = code;
        self.lead_word = word;
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

    /// A feed per minute (mm/min) in the program's units, as a live tool's
    /// feeds are written (G98).
    pub fn per_min(&self, mm: f64) -> String {
        if self.inches {
            decimal(mm / 25.4, 2)
        } else {
            decimal(mm, 1)
        }
    }

    /// An angle of the C axis (degrees), with its decimal point.
    pub fn angle(degrees: f64) -> String {
        decimal(degrees, 3)
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
            TurnMove::Thread { lead, .. } if self.lead_word == 'F' => {
                (self.thread_code, Some(self.per_rev(lead)))
            }
            TurnMove::Thread { .. } => (self.thread_code, None),
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
        // A lead in every block (LinuxCNC's G33 K, mitcad#2425): the
        // distance along the move per revolution, the lead being along the
        // longer axis.
        if let TurnMove::Thread { lead, .. } = *m
            && self.lead_word != 'F'
        {
            let from = self.at.unwrap_or(to);
            let (dx, dz) = (to[0] - from[0], to[1] - from[1]);
            let major = dx.abs().max(dz.abs());
            let along = if major > 1e-12 {
                lead * dx.hypot(dz) / major
            } else {
                lead
            };
            words.push(format!("{}{}", self.lead_word, self.per_rev(along)));
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
    fn header(&self, out: &mut TurnOutput, program: &TurningProgram) {
        self.modes(out);
        out.line(format!("G{}", 53 + u32::from(program.work_offset)));
    }
    /// The program's modes (units, plane, compensation off, feeds per
    /// revolution and the rest of the dialect's), as the header writes
    /// them and again after a manual block (mitcad#2506).
    fn modes(&self, out: &mut TurnOutput);
    fn footer(&self, out: &mut TurnOutput, program: &TurningProgram);
    /// Indexes the turret to the tool with its offset.
    fn tool_change(&self, out: &mut TurnOutput, tool: &TurningTool);
    /// The spindle's speed limit (for a constant surface speed).
    fn spindle_limit(&self, out: &mut TurnOutput, rpm: f64);
    fn spindle_on(&self, out: &mut TurnOutput, spindle: &TurnSpindle, direction: SpindleDirection);
    fn spindle_off(&self, out: &mut TurnOutput) {
        out.line("M5");
    }
    /// The spindle's speed with the block that starts it: the
    /// sub-spindle's (mitcad#2319).
    fn spindle_on_with(&self, out: &mut TurnOutput, spindle: &TurnSpindle, code: &str) {
        match *spindle {
            TurnSpindle::Css { surface_speed, .. } => {
                out.line(format!("G96 S{} {code}", out.surface_speed(surface_speed)));
            }
            TurnSpindle::Rpm { rpm } => {
                out.line(format!("G97 S{} {code}", number(rpm.round(), 0)));
            }
        }
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
    // Live tools on the C axis (mitcad#2067, `super::live`).
    /// Whether it writes live tools on a C axis.
    fn mills(&self) -> bool {
        false
    }
    /// Feeds per minute (a live tool's) and per revolution (turning's).
    fn feed_per_minute(&self, out: &mut TurnOutput) {
        out.line("G98");
    }
    fn feed_per_rev(&self, out: &mut TurnOutput) {
        out.line("G99");
    }
    /// Polar coordinate interpolation on the face, on and off.
    fn polar(&self, _out: &mut TurnOutput, _on: bool) {}
    /// Cylindrical interpolation on the cylinder of a radius, and off
    /// (none).
    fn cylindrical(&self, _out: &mut TurnOutput, _radius: Option<f64>) {}
    /// The drilling cycle on the face or (`diameter`) the diameter; none
    /// when it has none (the moves are written).
    fn drill_cycle_code(&self, _diameter: bool) -> Option<&'static str> {
        None
    }
    // Other controls than Fanuc's (mitcad#2425).
    /// The threading move's code and the word of its lead: `F`, modal as
    /// the feeds (G32 F), or another letter written in every block.
    fn thread_words(&self) -> (&'static str, char) {
        ("G32", 'F')
    }
    /// How the program is read back to check it.
    fn control(&self) -> Control {
        Control::Fanuc
    }
    /// Whether it writes the sub-spindle's hand-over and the operations
    /// on the part's back.
    fn hands_over(&self) -> bool {
        true
    }
    /// How the program's list of tools names a tool: its station and
    /// offset as the tool change writes them.
    fn tool_words(&self, tool: &TurningTool) -> String {
        format!("T{:02}{:02}", tool.number, tool.offset_number())
    }
    // Manual G-code blocks (mitcad#2506).
    /// What a manual block's line holds that this control would not run
    /// as part of the program, in words; none when the post writes it.
    fn manual_refusal(&self, line: &str) -> Option<String> {
        manual_refusal(line, self.control())
    }
}

/// What a manual block's line holds that the control would not run as a
/// part of the program (mitcad#2506), in words; none when the post may
/// write it as it is. Refused: a character other than printable ASCII
/// (a tab too), the fencing comments' text, a comment that is not closed,
/// a tape mark (`%`), an N word (the post numbers the lines and the
/// cycles refer to them), the program's end (M2, M30) and, on a Fanuc
/// control, an O word (a program number), M99 (a return or a jump) and
/// lower case letters outside comments; on LinuxCNC an O word other than a
/// subroutine call (`o<name> call`). Runs of letters (a macro's `GOTO`,
/// `SIN`, `EQ`) are words of their own, not addresses; `;` starts a
/// comment on LinuxCNC.
pub fn manual_refusal(line: &str, control: Control) -> Option<String> {
    let lcnc = control == Control::LinuxCnc;
    if line.chars().any(|c| !c.is_ascii() || c.is_ascii_control()) {
        return Some(
            "a character the control does not read here (only printable ASCII, no tabs)".to_owned(),
        );
    }
    let upper = line.to_ascii_uppercase();
    if upper.contains(super::MANUAL_BEGIN) || upper.contains(super::MANUAL_END) {
        return Some("the text of the comments that fence the manual code".to_owned());
    }
    // The code outside comments.
    let mut code = String::new();
    let mut comment = false;
    for c in line.chars() {
        if comment {
            if c == ')' {
                comment = false;
            }
            continue;
        }
        match c {
            '(' => comment = true,
            ';' if lcnc => break,
            c => code.push(c),
        }
    }
    if comment {
        return Some("a comment that is not closed".to_owned());
    }
    if code.contains('%') {
        return Some("a tape mark (%), which ends the program there".to_owned());
    }
    if !lcnc && code.chars().any(|c| c.is_ascii_lowercase()) {
        return Some("lower case letters: the control reads capitals".to_owned());
    }
    let chars: Vec<char> = code.to_ascii_uppercase().chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i].is_ascii_alphabetic() {
            i += 1;
        }
        if i - start > 1 {
            continue;
        }
        let letter = chars[start];
        let mut j = i;
        while j < chars.len() && chars[j] == ' ' {
            j += 1;
        }
        let mut value = String::new();
        while j < chars.len() && (chars[j].is_ascii_digit() || matches!(chars[j], '.' | '-' | '+'))
        {
            value.push(chars[j]);
            j += 1;
        }
        match letter {
            'N' => {
                return Some(
                    "an N word: the post numbers the lines itself, and its cycles refer to them"
                        .to_owned(),
                );
            }
            'O' if !lcnc => {
                return Some("an O word, a program number inside the program".to_owned());
            }
            'O' => {
                let rest: String = chars[j..].iter().collect();
                let call = rest
                    .split(|c: char| !c.is_ascii_alphabetic())
                    .any(|w| w == "CALL");
                if !call {
                    return Some(
                        "an O word other than a subroutine call (o<name> call)".to_owned(),
                    );
                }
                // The rest is the call's name and arguments.
                return None;
            }
            'M' => match value.parse::<u32>() {
                Ok(v @ (2 | 30)) => {
                    return Some(format!(
                        "M{v}, the program's end; the post writes the end after the operations"
                    ));
                }
                Ok(99) if !lcnc => {
                    return Some(
                        "M99, a return or a jump that leaves the program's order".to_owned(),
                    );
                }
                _ => {}
            },
            _ => {}
        }
        i = j.max(i);
    }
    None
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

    fn modes(&self, out: &mut TurnOutput) {
        out.line(format!("{} G18 G40 G99", out.units_code()));
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

    fn mills(&self) -> bool {
        true
    }

    // G12.1 and G13.1 each in a block of its own, in the XY plane (X and
    // the C axis's length across); back in the ZX plane after.
    fn polar(&self, out: &mut TurnOutput, on: bool) {
        if on {
            out.line("G17");
            out.line("G12.1");
        } else {
            out.line("G13.1");
            out.line("G18");
        }
    }

    // G7.1 with the cylinder's radius in the YZ plane (C for Y), G7.1 C0.
    // off.
    fn cylindrical(&self, out: &mut TurnOutput, radius: Option<f64>) {
        match radius {
            Some(r) => {
                out.line("G19");
                out.line(format!("G7.1 C{}", out.len(r)));
            }
            None => {
                out.line("G7.1 C0.");
                out.line("G18");
            }
        }
    }

    fn drill_cycle_code(&self, diameter: bool) -> Option<&'static str> {
        Some(if diameter { "G87" } else { "G83" })
    }
}

/// LinuxCNC on a lathe (mitcad#2425; its interpreter as in 2.9), every
/// operation written longhand:
///
/// - Between `%` lines, without a program number (an O word calls a
///   subroutine there). The header sets the units, the ZX plane (G18),
///   diameter mode (G7: X is a diameter, I stays a radius), absolute
///   coordinates with incremental arc centres (G90 G91.1), no nose radius
///   compensation (G40, the toolpaths compensate it), no cycle (G80),
///   feeds per revolution (G95), corners blended within the arc tolerance
///   (G64 P) and the work offset. It ends with M2.
/// - Facing, roughing, finishing, grooving and parting off are their
///   moves (G0 to G3): no stock removal or finishing cycle, even when the
///   settings ask for canned cycles (the program says so in a comment).
///   Threads are G33 moves, the lead as K in every block (along the move,
///   as LinuxCNC takes it), each pass from a rapid.
/// - The tool change: `T<station> M6`, then `G43` (the tool table's X and
///   Z offsets of that tool) or `G43 H<offset>` for a tool with an offset
///   number of its own; without the tool length offset in the settings
///   only `T<station> M6`, for a lathe whose M6 is remapped to apply the
///   offset itself. M6 stops the spindle there, so it is started again
///   after each change. The tool table holds a tool per turret station.
/// - The spindle: a constant surface speed with its limit in one block
///   (`G96 D<rpm> S<m/min> M3`, ft/min in inches) or a constant speed
///   (`G97 S<rpm> M3`); coolant flood M8, mist M7, off M9.
/// - What it does not write is refused in words: live tools on a C axis,
///   the sub-spindle's hand-over, air blast, probing the work zero.
pub struct LinuxCncTurning;

impl TurningPost for LinuxCncTurning {
    fn id(&self) -> &'static str {
        "linuxcnc_turning"
    }

    fn label(&self) -> &'static str {
        "LinuxCNC (turning)"
    }

    fn extension(&self) -> &'static str {
        "ngc"
    }

    fn start(&self, out: &mut TurnOutput, _program: &TurningProgram) {
        out.raw("%");
    }

    fn modes(&self, out: &mut TurnOutput) {
        out.line(format!("{} G18 G7 G90 G91.1 G40 G80 G95", out.units_code()));
        out.line(format!("G64 P{}", out.len(out.arc_tolerance)));
    }

    fn footer(&self, out: &mut TurnOutput, _program: &TurningProgram) {
        out.line("M2");
        out.raw("%");
    }

    fn tool_change(&self, out: &mut TurnOutput, tool: &TurningTool) {
        let change = format!("T{} M6", tool.number);
        if !out.tool_length_offset.unwrap_or(true) {
            out.line(change);
        } else if tool.offset_number() != tool.number {
            out.line(format!("{change} G43 H{}", tool.offset_number()));
        } else {
            out.line(format!("{change} G43"));
        }
    }

    // The limit goes with G96 in its block.
    fn spindle_limit(&self, _out: &mut TurnOutput, _rpm: f64) {}

    fn spindle_on(&self, out: &mut TurnOutput, spindle: &TurnSpindle, direction: SpindleDirection) {
        let m = match direction {
            SpindleDirection::Cw => "M3",
            SpindleDirection::Ccw => "M4",
        };
        self.spindle_on_with(out, spindle, m);
    }

    fn spindle_on_with(&self, out: &mut TurnOutput, spindle: &TurnSpindle, code: &str) {
        match *spindle {
            TurnSpindle::Css {
                surface_speed,
                max_rpm,
            } => {
                out.line(format!(
                    "G96 D{} S{} {code}",
                    number(max_rpm.round(), 0),
                    out.surface_speed(surface_speed)
                ));
            }
            TurnSpindle::Rpm { rpm } => {
                out.line(format!("G97 S{} {code}", number(rpm.round(), 0)));
            }
        }
    }

    fn coolant(&self, out: &mut TurnOutput, coolant: Coolant) -> Result<(), CamError> {
        let code = match coolant {
            Coolant::Off => "M9",
            Coolant::Flood => "M8",
            Coolant::Mist => "M7",
            Coolant::Air => {
                return Err(CamError::new(format!(
                    "{} has flood and mist coolant here, no air blast.",
                    self.label()
                )));
            }
        };
        out.line(code);
        Ok(())
    }

    fn feed_per_minute(&self, out: &mut TurnOutput) {
        out.line("G94");
    }

    fn feed_per_rev(&self, out: &mut TurnOutput) {
        out.line("G95");
    }

    fn thread_words(&self) -> (&'static str, char) {
        ("G33", 'K')
    }

    fn control(&self) -> Control {
        Control::LinuxCnc
    }

    fn hands_over(&self) -> bool {
        false
    }

    fn tool_words(&self, tool: &TurningTool) -> String {
        if tool.offset_number() == tool.number {
            format!("T{}", tool.number)
        } else {
            format!("T{} H{}", tool.number, tool.offset_number())
        }
    }
}

/// Every turning dialect.
pub fn turning_dialects() -> Vec<Box<dyn TurningPost>> {
    vec![Box::new(FanucTurning), Box::new(LinuxCncTurning)]
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

/// Stops the main spindle, or the sub-spindle after the hand-over
/// (mitcad#2319).
fn stop_spindle(
    out: &mut TurnOutput,
    dialect: &dyn TurningPost,
    program: &TurningProgram,
    sub: bool,
) {
    if sub {
        out.line(program.mill_turn.sub_stop.trim());
    } else {
        dialect.spindle_off(out);
    }
}

/// The hand-over's first half (mitcad#2319): the turret at its change
/// position, the main spindle at the synchronised speed, the sub-spindle
/// synchronised, up to the part with its chuck open, and gripping it.
fn grip(
    out: &mut TurnOutput,
    dialect: &dyn TurningPost,
    program: &TurningProgram,
    hand_over: &HandOver,
) {
    let codes = &program.mill_turn;
    let axis = codes.sub_letter();
    out.comment("Hand-over to the sub-spindle");
    out.forget();
    out.motion(&TurnMove::Rapid {
        to: program.change_position,
    });
    dialect.spindle_on(
        out,
        &TurnSpindle::Rpm { rpm: hand_over.rpm },
        SpindleDirection::Cw,
    );
    out.line(codes.sync_on.trim());
    out.line(codes.sub_chuck_open.trim());
    out.line(format!(
        "G0 {axis}{}",
        out.len(hand_over.grip_z + hand_over.withdraw)
    ));
    dialect.feed_per_minute(out);
    out.line(format!(
        "G1 {axis}{} F{}",
        out.len(hand_over.grip_z),
        out.per_min(hand_over.feed)
    ));
    dialect.feed_per_rev(out);
    out.line(codes.sub_chuck_close.trim());
    out.forget();
}

/// The hand-over's second half (mitcad#2319): the main chuck opened when
/// nothing parted the part off, the sub-spindle back with the part, the
/// spindles apart and stopped, the sub-spindle's work offset.
fn release(
    out: &mut TurnOutput,
    dialect: &dyn TurningPost,
    program: &TurningProgram,
    hand_over: &HandOver,
    parted: bool,
) {
    let codes = &program.mill_turn;
    if !parted {
        out.line(codes.main_chuck_open.trim());
    }
    out.line(format!(
        "G0 {}{}",
        codes.sub_letter(),
        out.len(hand_over.grip_z + hand_over.withdraw)
    ));
    out.line(codes.sync_off.trim());
    dialect.spindle_off(out);
    out.line(codes.sub_stop.trim());
    out.line(format!("G{}", 53 + u32::from(hand_over.work_offset)));
    out.forget();
}

/// The post's state between operations that a manual block changes
/// (mitcad#2506).
struct Between<'a> {
    coolant: Coolant,
    running: Option<SpindleDirection>,
    tool: Option<&'a TurningTool>,
    limit: Option<f64>,
    speed: Option<TurnSpindle>,
    sub: bool,
}

/// Writes a manual block (mitcad#2506, [`super::ManualBlock`]): into it
/// the coolant off, the live tool and the C axis off, the tool at the
/// change position, the spindle that holds the part stopped; its lines
/// between the fencing comments; out of it the modes and the work offset
/// (the sub-spindle's after the hand-over), the coolant and the spindle
/// off, and nothing known of the tool, the spindle's speed and limit or
/// where the tool is, so that the next operation writes them all.
fn manual_block(
    out: &mut TurnOutput,
    dialect: &dyn TurningPost,
    program: &TurningProgram,
    block: &super::ManualBlock,
    state: &mut Between<'_>,
    live_state: &mut super::live::LiveState,
) -> Result<(), CamError> {
    let lines: Vec<&str> = block
        .lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    for (k, line) in lines.iter().enumerate() {
        if let Some(why) = dialect.manual_refusal(line) {
            return Err(CamError::new(format!(
                "The manual G-code {} cannot be written for {}: its line {} has {why}.",
                block.name,
                dialect.label(),
                k + 1
            )));
        }
    }
    if state.coolant != Coolant::Off {
        dialect.coolant(out, Coolant::Off)?;
        state.coolant = Coolant::Off;
    }
    super::live::stop(out, dialect, program, live_state);
    out.motion(&TurnMove::Rapid {
        to: program.change_position,
    });
    if state.running.is_some() {
        stop_spindle(out, dialect, program, state.sub);
        state.running = None;
    }
    out.notice(&format!(
        "{}: {}, not simulated or read back",
        super::MANUAL_BEGIN,
        block.name
    ));
    for line in lines {
        out.line(line);
    }
    out.notice(super::MANUAL_END);
    // Nothing the block did is known.
    out.forget();
    *live_state = super::live::LiveState::default();
    dialect.modes(out);
    let offset = match (&program.hand_over, state.sub) {
        (Some(h), true) => h.work_offset,
        _ => program.work_offset,
    };
    out.line(format!("G{}", 53 + u32::from(offset)));
    dialect.coolant(out, Coolant::Off)?;
    stop_spindle(out, dialect, program, state.sub);
    state.coolant = Coolant::Off;
    state.running = None;
    state.tool = None;
    state.limit = None;
    state.speed = None;
    Ok(())
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
    // A control without the sub-spindle's codes here (mitcad#2425).
    if let Some(first) = program.first_sub()
        && !dialect.hands_over()
    {
        return Err(CamError::new(format!(
            "{}: {} has no sub-spindle here.",
            program.operations[first].name,
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
    let (thread_code, lead_word) = dialect.thread_words();
    out.set_thread_words(thread_code, lead_word);
    dialect.start(&mut out, program);
    for line in EXPERIMENTAL_NOTICE {
        out.notice(line);
    }
    out.comment(&program.name);
    // Canned cycles asked of a control the post writes none for
    // (mitcad#2425): the moves, and the program says so.
    if settings.canned_cycles && !dialect.has_cycles() {
        out.comment("Turning cycles are written as moves for this control");
    }
    let mut seen: Vec<u32> = Vec::new();
    for op in &program.operations {
        if !seen.contains(&op.tool.number) {
            seen.push(op.tool.number);
            out.comment(&format!(
                "{} {}",
                dialect.tool_words(&op.tool),
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
    // The C axis and the live tool (mitcad#2067).
    let mut live_state = super::live::LiveState::default();
    // The sub-spindle's hand-over (mitcad#2319): gripping before the
    // parting off (or the first operation on the sub-spindle), released
    // before that operation.
    let grip_at = program.hand_over_at();
    let first_sub = program.first_sub();
    let mut sub = false;
    for (index, op) in program.operations.iter().enumerate() {
        // The manual blocks before it (mitcad#2506).
        for block in program.manual.iter().filter(|b| b.at == index) {
            let mut state = Between {
                coolant,
                running,
                tool,
                limit,
                speed,
                sub,
            };
            manual_block(
                &mut out,
                dialect,
                program,
                block,
                &mut state,
                &mut live_state,
            )?;
            Between {
                coolant,
                running,
                tool,
                limit,
                speed,
                ..
            } = state;
        }
        if let Some(hand_over) = &program.hand_over {
            if Some(index) == grip_at {
                if coolant != Coolant::Off {
                    dialect.coolant(&mut out, Coolant::Off)?;
                    coolant = Coolant::Off;
                }
                super::live::stop(&mut out, dialect, program, &mut live_state);
                grip(&mut out, dialect, program, hand_over);
                running = Some(SpindleDirection::Cw);
                speed = Some(TurnSpindle::Rpm { rpm: hand_over.rpm });
            }
            if Some(index) == first_sub {
                if coolant != Coolant::Off {
                    dialect.coolant(&mut out, Coolant::Off)?;
                    coolant = Coolant::Off;
                }
                release(&mut out, dialect, program, hand_over, grip_at != first_sub);
                running = None;
                speed = None;
                limit = None;
                sub = true;
            }
        }
        out.comment(&op.name);
        if let Some(live) = &op.live {
            if tool != Some(&op.tool) {
                if coolant != Coolant::Off {
                    dialect.coolant(&mut out, Coolant::Off)?;
                    coolant = Coolant::Off;
                }
                super::live::stop_tool(&mut out, program, &mut live_state);
                out.forget();
                out.motion(&change);
                dialect.tool_change(&mut out, &op.tool);
                out.forget();
                tool = Some(&op.tool);
            }
            // The spindle that holds the part stops for its C axis: the
            // main spindle, or the sub-spindle after the hand-over
            // (mitcad#2375).
            if running.is_some() {
                stop_spindle(&mut out, dialect, program, sub);
                running = None;
                speed = None;
            }
            super::live::start(&mut out, dialect, program, op, &mut live_state)?;
            if op.coolant != coolant {
                dialect.coolant(&mut out, op.coolant)?;
                coolant = op.coolant;
            }
            // A control that mirrors only Z on the sub-spindle reads its Y
            // and C in the main spindle's senses (mitcad#2392).
            if op.sub && program.mill_turn.sub_mirror_z_only {
                let mirrored = live.mirrored_y();
                super::live::moves(
                    &mut out,
                    dialect,
                    program,
                    &mirrored,
                    cycles,
                    &mut live_state,
                )?;
            } else {
                super::live::moves(&mut out, dialect, program, live, cycles, &mut live_state)?;
            }
            continue;
        }
        if live_state.c_axis || live_state.running.is_some() {
            super::live::stop(&mut out, dialect, program, &mut live_state);
        }
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
            stop_spindle(&mut out, dialect, program, sub);
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
            if sub {
                // The sub-spindle's own codes (mitcad#2319).
                let code = program
                    .mill_turn
                    .sub_start(op.direction == SpindleDirection::Cw);
                dialect.spindle_on_with(&mut out, &op.spindle, code.trim());
            } else {
                dialect.spindle_on(&mut out, &op.spindle, op.direction);
            }
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
    // The manual blocks after the last operation, before the end
    // (mitcad#2506).
    let last = program.operations.len();
    for block in program.manual.iter().filter(|b| b.at == last) {
        let mut state = Between {
            coolant,
            running,
            tool,
            limit,
            speed,
            sub,
        };
        manual_block(
            &mut out,
            dialect,
            program,
            block,
            &mut state,
            &mut live_state,
        )?;
        Between {
            coolant,
            running,
            ..
        } = state;
    }
    if coolant != Coolant::Off {
        dialect.coolant(&mut out, Coolant::Off)?;
    }
    super::live::stop(&mut out, dialect, program, &mut live_state);
    out.motion(&change);
    if running.is_some() {
        stop_spindle(&mut out, dialect, program, sub);
    }
    dialect.footer(&mut out, program);
    Ok(out.finish())
}
