// SPDX-License-Identifier: MIT
//! A small G-code reader (docs/cam.md, "Simulation and checks"): the
//! subset the posts write and a tool changer's lines use, read back into
//! the moves of the spindle's gauge line in machine coordinates, so that
//! the checks follow what the controller will run, a macro changer's own
//! moves included.
//!
//! It reads G0, G1, G2 and G3 (arcs by I, J and K or by R, in the planes
//! G17, G18 and G19, helices along the plane's normal), G20 and G21, G53,
//! G54 to G59, G90 and G91, G90.1 and G91.1, the canned drilling cycles
//! G73, G81, G82 and G83 with G98, G99 and G80 (a cycle's pecks are read
//! as one feed to the bottom, which the controller's pecks stay within),
//! T and M6, F, comments in parentheses and after a semicolon, quoted
//! texts, line numbers, `%` and checksums. Words without a move (S, the
//! other M codes, G4, G40, G43, G43.1, G49, G61, G64, G94) are read past:
//! the tool's length comes from the tool in the spindle, as the setup's
//! placement means it, not from the length offset words. After G27, G28, G30, a probing move (G38.2 to
//! G38.5, MillenniumOS's G6500 to G6699) or a tool change (M6, or T alone
//! where T changes the tool), the position is not known until the axes
//! are given again. What it does not follow it refuses in words, with the
//! line: work offsets set by the program (G10, G92), inverse time feeds,
//! parameters and expressions, other axes, other commands.
//!
//! Positions are the gauge line's in machine coordinates (mm): a move in
//! work coordinates puts the tip of the tool in the spindle at the work
//! offset's machine position plus the move's coordinates, so the gauge
//! line is the tool's length above it ([`crate::machine::Placement`]); a
//! move in machine coordinates (G53) places the gauge line itself.

use std::collections::BTreeMap;

use crate::CamError;
use crate::toolpath::{P3, arc_sweep};

const INCH: f64 = 25.4;

/// A position whose axes may not be known (after a home move or a tool
/// change, before the program names them again).
pub type Known = [Option<f64>; 3];

/// The position when all of its axes are known.
pub fn known(p: Known) -> Option<P3> {
    Some([p[0]?, p[1]?, p[2]?])
}

/// The plane of arcs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Plane {
    /// G17.
    #[default]
    Xy,
    /// G18.
    Zx,
    /// G19.
    Yz,
}

impl Plane {
    /// The plane's two axes in the order a positive (counter-clockwise,
    /// G3) turn goes from the first to the second, and its normal.
    pub fn axes(self) -> (usize, usize, usize) {
        match self {
            Plane::Xy => (0, 1, 2),
            Plane::Zx => (2, 0, 1),
            Plane::Yz => (1, 2, 0),
        }
    }
}

/// How a read move goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Motion {
    /// At the rapid rate (G0), in a straight line.
    Rapid,
    /// A straight cut at the feed (G1).
    Feed,
    /// An arc about `center` (machine coordinates; its normal coordinate is
    /// the start's), counter-clockwise (G3) or clockwise (G2) seen from
    /// the plane's normal; a helix when the normal coordinate changes, a
    /// whole turn when the ends meet in the plane.
    Arc { center: P3, ccw: bool, plane: Plane },
}

/// A move read from the program: where the gauge line goes, in machine
/// coordinates (mm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GMove {
    /// The line it was read from (1-based).
    pub line: usize,
    pub motion: Motion,
    /// The feed (mm/min) of a cut; none for a rapid.
    pub feed: Option<f64>,
    pub from: Known,
    pub to: Known,
    /// Written in machine coordinates (G53).
    pub machine: bool,
    /// The tool in the spindle.
    pub tool: Option<u32>,
}

impl GMove {
    /// Both ends, when every axis of them is known.
    pub fn ends(&self) -> Option<(P3, P3)> {
        Some((known(self.from)?, known(self.to)?))
    }

    /// Points along the move (its end included, its start not), an arc
    /// within `tolerance` (mm) of the true arc; none when its ends are
    /// not known.
    pub fn points(&self, tolerance: f64) -> Vec<P3> {
        let Some((from, to)) = self.ends() else {
            return Vec::new();
        };
        let Motion::Arc { center, ccw, plane } = self.motion else {
            return vec![to];
        };
        let (a, b, n) = plane.axes();
        let r = (from[a] - center[a]).hypot(from[b] - center[b]);
        if r <= 0.0 {
            return vec![to];
        }
        let sweep = arc_sweep(
            [from[a], from[b]],
            [to[a], to[b]],
            [center[a], center[b]],
            ccw,
        );
        let tol = tolerance.max(1e-6).min(r);
        // The chord of a step of angle t sags r (1 - cos(t / 2)).
        let step = 2.0 * (1.0 - tol / r).clamp(-1.0, 1.0).acos();
        let count = ((sweep.abs() / step.max(1e-3)).ceil() as usize).max(1);
        let a0 = (from[b] - center[b]).atan2(from[a] - center[a]);
        (1..=count)
            .map(|i| {
                if i == count {
                    return to;
                }
                let t = i as f64 / count as f64;
                let angle = a0 + sweep * t;
                let mut p = [0.0; 3];
                p[a] = center[a] + r * angle.cos();
                p[b] = center[b] + r * angle.sin();
                p[n] = from[n] + (to[n] - from[n]) * t;
                p
            })
            .collect()
    }
}

/// A line the reader does not follow: `message` in words, `line` where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GcodeError {
    /// 1-based; 0 when the error is not about one line.
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for GcodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GcodeError {}

impl From<GcodeError> for CamError {
    fn from(e: GcodeError) -> Self {
        CamError::new(e.message)
    }
}

#[derive(Clone, Copy, Debug)]
struct Word {
    letter: char,
    value: f64,
}

const NO_NUMBER: &str = "A word of a line has no number.";

/// The words of a line, without its comments, quoted texts, line marks
/// and checksum.
fn words(text: &str) -> Result<Vec<Word>, &'static str> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            ' ' | '\t' | '\r' | '%' => i += 1,
            // Block delete: the line is run unless the operator skips it.
            '/' if out.is_empty() => i += 1,
            '(' => {
                let Some(end) = chars[i..].iter().position(|&c| c == ')') else {
                    return Err("A comment is not closed.");
                };
                i += end + 1;
            }
            ';' | '*' => break,
            '#' | '[' | '<' => {
                return Err(
                    "A line uses parameters or expressions, which the reader does not follow.",
                );
            }
            c if c.is_ascii_alphabetic() => {
                let letter = c.to_ascii_uppercase();
                i += 1;
                while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') {
                    i += 1;
                }
                if i < chars.len() && chars[i] == '"' {
                    // A quoted text (RepRapFirmware's messages and names),
                    // a doubled quote inside it.
                    i += 1;
                    loop {
                        if i >= chars.len() {
                            return Err("A quoted text is not closed.");
                        }
                        if chars[i] == '"' {
                            if i + 1 < chars.len() && chars[i + 1] == '"' {
                                i += 2;
                                continue;
                            }
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                    continue;
                }
                let start = i;
                if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
                    i += 1;
                }
                let mut digits = 0;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    if chars[i].is_ascii_digit() {
                        digits += 1;
                    }
                    i += 1;
                }
                if digits == 0 {
                    return Err(NO_NUMBER);
                }
                let s: String = chars[start..i].iter().collect();
                let value: f64 = s.parse().map_err(|_| NO_NUMBER)?;
                out.push(Word { letter, value });
            }
            _ => return Err("A line has a character the reader does not know."),
        }
    }
    Ok(out)
}

/// A G or M code's number times ten (G43.1 is 431, G0 is 0).
fn code(v: f64) -> i32 {
    (v * 10.0).round() as i32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// No motion (G80): a line with axes is refused.
    Off,
    Rapid,
    Feed,
    Arc {
        ccw: bool,
    },
    /// A canned drilling cycle.
    Cycle,
}

/// A canned cycle's heights (work Z, mm).
#[derive(Clone, Copy, Debug)]
struct Cycle {
    retract: f64,
    bottom: f64,
    /// Where G98 returns to: the height the cycle started from, or the
    /// retract height when that is higher.
    clear: f64,
}

/// Reads G-code line by line, keeping the controller's modal state.
#[derive(Clone, Debug)]
pub struct GcodeReader {
    /// The machine position of each work offset's zero (G54 to G59) for a
    /// tool of no length; a move in an offset not known is refused.
    pub offsets: [Option<P3>; 6],
    /// The tools' lengths by number (mm): the gauge line is this far above
    /// the tip of the tool in the spindle.
    pub lengths: BTreeMap<u32, f64>,
    /// Whether M6 (and T where it changes the tool) puts the named tool in
    /// the spindle; off, the tool is only what [`GcodeReader::set_tool`]
    /// sets.
    pub follow_tools: bool,
    /// Whether T alone changes the tool (RepRapFirmware and MillenniumOS
    /// run their tool change macros on it), rather than M6.
    pub select_on_t: bool,
    at: Known,
    tool: Option<u32>,
    next_tool: Option<u32>,
    inches: bool,
    incremental: bool,
    arc_incremental: bool,
    plane: Plane,
    offset: usize,
    mode: Mode,
    feed: Option<f64>,
    retract_to_start: bool,
    cycle: Option<Cycle>,
}

impl GcodeReader {
    /// A reader at the controller's usual start: millimetres, absolute,
    /// arc centers relative to the start, the XY plane, G54, rapid motion,
    /// no tool and the position not known.
    pub fn new(offsets: [Option<P3>; 6], lengths: BTreeMap<u32, f64>) -> Self {
        GcodeReader {
            offsets,
            lengths,
            follow_tools: true,
            select_on_t: false,
            at: [None; 3],
            tool: None,
            next_tool: None,
            inches: false,
            incremental: false,
            arc_incremental: true,
            plane: Plane::Xy,
            offset: 0,
            mode: Mode::Rapid,
            feed: None,
            retract_to_start: true,
            cycle: None,
        }
    }

    /// Where the gauge line is (machine coordinates).
    pub fn at(&self) -> Known {
        self.at
    }

    pub fn set_at(&mut self, at: Known) {
        self.at = at;
    }

    /// The tool in the spindle.
    pub fn tool(&self) -> Option<u32> {
        self.tool
    }

    /// Puts a tool in the spindle without a move: the gauge line stays,
    /// the tip is the new tool's length below it.
    pub fn set_tool(&mut self, tool: Option<u32>) {
        self.tool = tool;
    }

    /// Whether the program's numbers are inches now (G20).
    pub fn inches(&self) -> bool {
        self.inches
    }

    /// Reads every line of `text`, numbered from 1.
    pub fn read(&mut self, text: &str) -> Result<Vec<GMove>, GcodeError> {
        let mut out = Vec::new();
        for (i, l) in text.lines().enumerate() {
            out.extend(self.line(l, i + 1)?);
        }
        Ok(out)
    }

    fn length(&self) -> f64 {
        self.tool
            .and_then(|t| self.lengths.get(&t).copied())
            .unwrap_or(0.0)
    }

    fn origin(&self) -> Result<P3, &'static str> {
        self.offsets[self.offset]
            .ok_or("A move is in a work offset whose place on the machine is not known.")
    }

    /// The gauge line's machine coordinate on axis `k` of the work
    /// coordinate `v`.
    fn gauge(&self, k: usize, v: f64) -> Result<f64, &'static str> {
        let o = self.origin()?;
        Ok(o[k] + v + if k == 2 { self.length() } else { 0.0 })
    }

    /// The work coordinate on axis `k` of the gauge line's machine
    /// coordinate `g`.
    fn work(&self, k: usize, g: f64) -> Result<f64, &'static str> {
        let o = self.origin()?;
        Ok(g - o[k] - if k == 2 { self.length() } else { 0.0 })
    }

    /// Where a straight move or an arc with these axis words (mm) ends.
    fn target(&self, axes: Known, machine: bool) -> Result<Known, &'static str> {
        if machine && self.incremental {
            return Err("A move in machine coordinates (G53) must be absolute.");
        }
        let mut to = self.at;
        for (k, word) in axes.iter().enumerate() {
            let Some(v) = *word else {
                continue;
            };
            to[k] = Some(if machine {
                v
            } else if self.incremental {
                self.at[k].ok_or("An incremental move starts where the position is not known.")? + v
            } else {
                self.gauge(k, v)?
            });
        }
        Ok(to)
    }

    /// Reads one line (`number`, 1-based, goes into its moves and errors).
    pub fn line(&mut self, text: &str, number: usize) -> Result<Vec<GMove>, GcodeError> {
        let fail = |message: &str| GcodeError {
            line: number,
            message: message.to_string(),
        };
        let words = words(text).map_err(fail)?;
        let mut g = Vec::new();
        let mut m = Vec::new();
        let mut axes: Known = [None; 3];
        let mut ijk: Known = [None; 3];
        let (mut r, mut f, mut t, mut p) = (None, None, None, None);
        let mut other_axis = false;
        for w in &words {
            match w.letter {
                'G' => g.push(code(w.value)),
                'M' => m.push(code(w.value)),
                'X' | 'Y' | 'Z' => {
                    let k = (w.letter as u8 - b'X') as usize;
                    if axes[k].replace(w.value).is_some() {
                        return Err(fail("A line gives an axis twice."));
                    }
                }
                'I' | 'J' | 'K' => ijk[(w.letter as u8 - b'I') as usize] = Some(w.value),
                'R' => r = Some(w.value),
                'F' => f = Some(w.value),
                'T' => t = Some(w.value),
                'P' => p = Some(w.value),
                'A' | 'B' | 'C' | 'U' | 'V' => other_axis = true,
                // N, S, H, D, E, L, O, Q, W: not read.
                _ => {}
            }
        }
        if g.contains(&200) {
            self.inches = true;
        }
        if g.contains(&210) {
            self.inches = false;
        }
        let scale = if self.inches { INCH } else { 1.0 };
        if let Some(f) = f {
            if !(f.is_finite() && f > 0.0) {
                return Err(fail("A feed must be more than zero."));
            }
            self.feed = Some(f * scale);
        }
        let change = m.contains(&60);
        if let Some(t) = t {
            if t < 0.0 || t.fract() != 0.0 {
                return Err(fail("A tool number must be a whole number."));
            }
            self.next_tool = Some(t as u32);
            if self.select_on_t && !change {
                if self.follow_tools {
                    self.tool = self.next_tool;
                }
                self.at = [None; 3];
            }
        }
        if change {
            if self.follow_tools && self.next_tool.is_some() {
                self.tool = self.next_tool;
            }
            // The controller's change may move the machine.
            self.at = [None; 3];
        }
        let mut machine = false;
        // The axis words belong to a command that is not a move.
        let mut takes_axes = false;
        let mut lost = false;
        let mut new_cycle = false;
        for &c in &g {
            match c {
                0 => self.mode = Mode::Rapid,
                10 => self.mode = Mode::Feed,
                20 => self.mode = Mode::Arc { ccw: false },
                30 => self.mode = Mode::Arc { ccw: true },
                730 | 810 | 820 | 830 => {
                    self.mode = Mode::Cycle;
                    new_cycle = true;
                }
                800 => {
                    self.mode = Mode::Off;
                    self.cycle = None;
                }
                40 | 200 | 210 | 400 | 430 | 490 | 610 | 640 | 940 => {}
                431 => takes_axes = true,
                170 => self.plane = Plane::Xy,
                180 => self.plane = Plane::Zx,
                190 => self.plane = Plane::Yz,
                900 => self.incremental = false,
                910 => self.incremental = true,
                901 => self.arc_incremental = false,
                911 => self.arc_incremental = true,
                540 | 550 | 560 | 570 | 580 | 590 => self.offset = ((c - 540) / 10) as usize,
                530 => machine = true,
                980 => self.retract_to_start = true,
                990 => self.retract_to_start = false,
                270 | 280 | 300 | 382..=385 | 65000..=66999 => {
                    takes_axes = true;
                    lost = true;
                }
                100 | 920..=923 => {
                    return Err(fail(
                        "A line sets a work offset (G10 or G92), which the reader does not follow.",
                    ));
                }
                930 => {
                    return Err(fail(
                        "A line asks for inverse time feeds (G93), which the reader does not follow.",
                    ));
                }
                _ => return Err(fail("A line has a command the reader does not know.")),
            }
        }
        if lost {
            // Homed, parked or probed: where is not known.
            self.at = [None; 3];
            return Ok(Vec::new());
        }
        if takes_axes || axes.iter().all(Option::is_none) {
            return Ok(Vec::new());
        }
        if other_axis {
            return Err(fail(
                "A line moves an axis other than X, Y and Z, which the reader does not follow.",
            ));
        }
        let axes = axes.map(|a| a.map(|v| v * scale));
        let moves = match self.mode {
            Mode::Off => Err("A line moves without a motion command."),
            Mode::Cycle if machine => Err("Machine coordinates (G53) take only straight moves."),
            Mode::Cycle => self.cycle_moves(axes, r.map(|v| v * scale), new_cycle, number),
            Mode::Rapid | Mode::Feed => self.straight(axes, machine, number),
            Mode::Arc { .. } if machine => {
                Err("Machine coordinates (G53) take only straight moves.")
            }
            Mode::Arc { ccw } => {
                if p.is_some_and(|p| p != 1.0) {
                    Err("An arc of more than one turn is not read.")
                } else {
                    let ijk = ijk.map(|v| v.map(|v| v * scale));
                    self.arc(axes, ijk, r.map(|v| v * scale), ccw, number)
                }
            }
        };
        moves.map_err(fail)
    }

    fn straight(
        &mut self,
        axes: Known,
        machine: bool,
        number: usize,
    ) -> Result<Vec<GMove>, &'static str> {
        let feed = match self.mode {
            Mode::Feed => Some(self.feed.ok_or("A cut has no feed.")?),
            _ => None,
        };
        let to = self.target(axes, machine)?;
        let mv = GMove {
            line: number,
            motion: if feed.is_some() {
                Motion::Feed
            } else {
                Motion::Rapid
            },
            feed,
            from: self.at,
            to,
            machine,
            tool: self.tool,
        };
        self.at = to;
        Ok(vec![mv])
    }

    fn arc(
        &mut self,
        axes: Known,
        ijk: Known,
        r: Option<f64>,
        ccw: bool,
        number: usize,
    ) -> Result<Vec<GMove>, &'static str> {
        let feed = self.feed.ok_or("A cut has no feed.")?;
        let to = self.target(axes, false)?;
        let (Some(from), Some(end)) = (known(self.at), known(to)) else {
            return Err("An arc starts or ends where the position is not known.");
        };
        let (a, b, _) = self.plane.axes();
        let c = if let Some(r) = r {
            center_from_radius([from[a], from[b]], [end[a], end[b]], r, ccw)?
        } else {
            if ijk[a].is_none() && ijk[b].is_none() {
                return Err("An arc has neither a center nor a radius.");
            }
            let (u, v) = (ijk[a].unwrap_or(0.0), ijk[b].unwrap_or(0.0));
            if self.arc_incremental {
                [from[a] + u, from[b] + v]
            } else {
                [self.gauge(a, u)?, self.gauge(b, v)?]
            }
        };
        let r0 = (from[a] - c[0]).hypot(from[b] - c[1]);
        let r1 = (end[a] - c[0]).hypot(end[b] - c[1]);
        if r0 <= 1e-9 {
            return Err("An arc has no radius.");
        }
        // As grbl allows: 0.005 mm, or 0.1 % of the radius.
        let off = (r1 - r0).abs();
        if off > 0.005 && off > 0.001 * r0 {
            return Err("An arc's end is not on its circle.");
        }
        let mut center = from;
        center[a] = c[0];
        center[b] = c[1];
        let mv = GMove {
            line: number,
            motion: Motion::Arc {
                center,
                ccw,
                plane: self.plane,
            },
            feed: Some(feed),
            from: self.at,
            to,
            machine: false,
            tool: self.tool,
        };
        self.at = to;
        Ok(vec![mv])
    }

    /// A canned drilling cycle at the hole of this line: up to the retract
    /// height when below it, across, down to the retract height, the feed
    /// to the bottom, and back to the start height (G98) or the retract
    /// height (G99).
    fn cycle_moves(
        &mut self,
        axes: Known,
        r: Option<f64>,
        new: bool,
        number: usize,
    ) -> Result<Vec<GMove>, &'static str> {
        if self.incremental {
            return Err("A canned cycle in incremental distance is not read.");
        }
        let feed = self.feed.ok_or("A cut has no feed.")?;
        let at = known(self.at).ok_or("A canned cycle starts where the position is not known.")?;
        const HEIGHTS: &str = "A canned cycle needs its depth and its retract height.";
        let bottom = axes[2].or(self.cycle.map(|c| c.bottom)).ok_or(HEIGHTS)?;
        let retract = r.or(self.cycle.map(|c| c.retract)).ok_or(HEIGHTS)?;
        if bottom >= retract {
            return Err("A canned cycle's depth must be below its retract height.");
        }
        let clear = match self.cycle {
            Some(c) if !new => c.clear,
            _ => self.work(2, at[2])?.max(retract),
        };
        self.cycle = Some(Cycle {
            retract,
            bottom,
            clear,
        });
        let x = match axes[0] {
            Some(v) => self.gauge(0, v)?,
            None => at[0],
        };
        let y = match axes[1] {
            Some(v) => self.gauge(1, v)?,
            None => at[1],
        };
        let zr = self.gauge(2, retract)?;
        let zb = self.gauge(2, bottom)?;
        let up = self.gauge(
            2,
            if self.retract_to_start {
                clear
            } else {
                retract
            },
        )?;
        let mut steps: Vec<(bool, P3)> = Vec::new();
        let mut z = at[2];
        if z < zr {
            steps.push((false, [at[0], at[1], zr]));
            z = zr;
        }
        steps.push((false, [x, y, z]));
        steps.push((false, [x, y, zr]));
        steps.push((true, [x, y, zb]));
        steps.push((false, [x, y, up]));
        let mut out = Vec::new();
        for (cut, to) in steps {
            let from = self.at;
            if known(from) == Some(to) {
                continue;
            }
            out.push(GMove {
                line: number,
                motion: if cut { Motion::Feed } else { Motion::Rapid },
                feed: cut.then_some(feed),
                from,
                to: to.map(Some),
                machine: false,
                tool: self.tool,
            });
            self.at = to.map(Some);
        }
        Ok(out)
    }
}

/// The center of an arc given by its radius: for a radius above zero the
/// arc of at most half a turn, below zero the longer one.
fn center_from_radius(
    from: [f64; 2],
    to: [f64; 2],
    r: f64,
    ccw: bool,
) -> Result<[f64; 2], &'static str> {
    let d = [to[0] - from[0], to[1] - from[1]];
    let chord = d[0].hypot(d[1]);
    if chord <= 1e-9 {
        return Err("An arc given by its radius cannot be a whole turn.");
    }
    let half = chord / 2.0;
    let rr = r.abs();
    if rr < half - 0.005 {
        return Err("An arc's radius does not reach its end.");
    }
    let h = (rr * rr - half * half).max(0.0).sqrt();
    // A counter-clockwise arc of at most half a turn has its center left
    // of the chord.
    let left = [-d[1] / chord, d[0] / chord];
    let side = if ccw == (r > 0.0) { 1.0 } else { -1.0 };
    Ok([
        from[0] + d[0] / 2.0 + side * h * left[0],
        from[1] + d[1] / 2.0 + side * h * left[1],
    ])
}

#[cfg(test)]
mod tests;
