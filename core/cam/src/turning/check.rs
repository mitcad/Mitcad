// SPDX-License-Identifier: MIT
//! The checks of a turning program: against the lathe (the spindle's
//! speeds, the feeds, the travel, the zones and the chuck) and of the
//! written program itself, read back the way a Fanuc lathe control reads
//! it (diameter mode, feeds per revolution, the cycles' blocks).

use serde::{Deserialize, Serialize};

use super::{PXZ, TurnMove, TurnSpindle, TurningProgram, move_points};
use crate::CamError;
use crate::machine::{Finding, Machine};

/// Where the work zero is on the lathe: the machine Z of the work zero
/// (machine X is the spindle axis for both), and the work Z of the chuck
/// jaws' face, which no tool may pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TurnPlacement {
    pub work_z: f64,
    #[serde(default)]
    pub chuck_z: Option<f64>,
}

/// Checks a turning program against the lathe: one finding per operation
/// and kind, in words, with the place in `at` (`[x, 0, z]`, work
/// coordinates, x a radius).
pub fn check_lathe(
    machine: &Machine,
    program: &TurningProgram,
    placement: &TurnPlacement,
) -> Vec<Finding> {
    let mut out = Vec::new();
    let Some(lathe) = &machine.lathe else {
        out.push(Finding {
            message: "The setup's machine is not a lathe.".to_owned(),
            operation: 0,
            at: None,
        });
        return out;
    };
    let at3 = |p: PXZ| Some([p[0], 0.0, p[1]]);
    for (i, op) in program.operations.iter().enumerate() {
        let name = &op.name;
        let mut say = |message: String, at: Option<[f64; 3]>| {
            out.push(Finding {
                message,
                operation: i,
                at,
            })
        };
        if op.tool.number > lathe.turret_stations {
            say(
                format!("{name}: the tool's station is not on the turret."),
                None,
            );
        }
        match op.spindle {
            TurnSpindle::Css { max_rpm, .. } => {
                if max_rpm > machine.spindle.max_rpm {
                    say(
                        format!("{name}: the spindle limit is above what the spindle can do."),
                        None,
                    );
                }
            }
            TurnSpindle::Rpm { rpm } => {
                if rpm < machine.spindle.min_rpm || rpm > machine.spindle.max_rpm {
                    say(
                        format!("{name}: the spindle speed is outside what the spindle can do."),
                        None,
                    );
                }
            }
        }
        let mut fast = None;
        let mut beyond = None;
        let mut chuck = None;
        let mut zones: Vec<Option<PXZ>> = vec![None; machine.zones.len()];
        let mut from: Option<PXZ> = None;
        for m in &op.path.moves {
            let pts = match from {
                Some(f) => {
                    let mut pts = Vec::new();
                    let mut prev = f;
                    for q in move_points(f, m, 0.01) {
                        let d = (q[0] - prev[0]).hypot(q[1] - prev[1]);
                        let n = ((d / 0.5).ceil() as usize).max(1);
                        for k in 1..=n {
                            let t = k as f64 / n as f64;
                            pts.push([
                                prev[0] + (q[0] - prev[0]) * t,
                                prev[1] + (q[1] - prev[1]) * t,
                            ]);
                        }
                        prev = q;
                    }
                    pts
                }
                None => vec![m.to()],
            };
            if let (Some(f), Some(feed)) = (from, m.feed()) {
                let x = f[0].abs().min(m.to()[0].abs());
                if fast.is_none() && feed * op.spindle.rpm_at(x) > machine.max_feed + 1e-9 {
                    fast = Some(m.to());
                }
            }
            for p in pts {
                let machine_p = [p[0], 0.0, p[1] + placement.work_z];
                let t = &machine.travel;
                let inside = machine_p[0] >= t.min[0] - 1e-9
                    && machine_p[0] <= t.max[0] + 1e-9
                    && machine_p[2] >= t.min[2] - 1e-9
                    && machine_p[2] <= t.max[2] + 1e-9;
                if !inside && beyond.is_none() {
                    beyond = Some(p);
                }
                if let Some(cz) = placement.chuck_z
                    && chuck.is_none()
                    && p[1] < cz - 1e-9
                    && p[0] < lathe.chuck_diameter.max(0.0) / 2.0 + 1e-9
                {
                    chuck = Some(p);
                }
                for (k, zone) in machine.zones.iter().enumerate() {
                    let b = &zone.bounds;
                    if zones[k].is_none()
                        && machine_p[0] >= b.min[0] - 1e-9
                        && machine_p[0] <= b.max[0] + 1e-9
                        && machine_p[2] >= b.min[2] - 1e-9
                        && machine_p[2] <= b.max[2] + 1e-9
                    {
                        zones[k] = Some(p);
                    }
                }
            }
            from = Some(m.to());
        }
        if let Some(p) = fast {
            say(
                format!("{name}: a feed is faster than the machine can cut."),
                at3(p),
            );
        }
        if let Some(p) = beyond {
            say(
                format!("{name}: the tool goes beyond the machine's travel."),
                at3(p),
            );
        }
        if let Some(p) = chuck {
            say(format!("{name}: the tool runs into the chuck."), at3(p));
        }
        for (k, hit) in zones.into_iter().enumerate() {
            if let Some(p) = hit {
                say(
                    format!("{name}: the tool runs into {}.", machine.zones[k].name),
                    at3(p),
                );
            }
        }
    }
    out
}

/// A move read back from a program: `x` a radius, in millimetres.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReadMove {
    pub line: usize,
    #[serde(rename = "move")]
    pub motion: TurnMove,
}

/// What reading a program back found: its moves (the cycles' blocks as
/// they are written; the cycles themselves are listed by kind and line),
/// and the cycles.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProgramCheck {
    pub moves: Vec<ReadMove>,
    /// `G71`, `G70`, `G76` with their lines (1-based).
    pub cycles: Vec<(String, usize)>,
    /// Whether X was read as a diameter throughout (no radius mode).
    pub diameter_mode: bool,
    /// Lines that set a constant surface speed, each after a limit.
    pub css_lines: Vec<usize>,
}

fn word_value(text: &str) -> Option<f64> {
    text.parse::<f64>().ok()
}

/// One block's words: letter and value text.
fn words(line: &str) -> Result<Vec<(char, String)>, String> {
    let mut out = Vec::new();
    let mut chars = line.chars().peekable();
    let mut in_comment = false;
    while let Some(c) = chars.next() {
        if in_comment {
            if c == ')' {
                in_comment = false;
            }
            continue;
        }
        match c {
            '(' => in_comment = true,
            ' ' | '\t' => {}
            c if c.is_ascii_alphabetic() => {
                let mut v = String::new();
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() || d == '.' || d == '-' || d == '+' {
                        v.push(d);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if v.is_empty() {
                    return Err(format!("a word {c} without a value"));
                }
                out.push((c.to_ascii_uppercase(), v));
            }
            other => return Err(format!("a character {other} it does not read")),
        }
    }
    if in_comment {
        return Err("a comment that is not closed".to_owned());
    }
    Ok(out)
}

/// Reads a turning program back the way a Fanuc lathe control in G-code
/// system A does, refusing in words (with the line) what would make it
/// cut other than meant: a number without a decimal point where lengths
/// are meant, a feed before the feed mode is per revolution, a constant
/// surface speed before its limit, threading at a constant surface speed,
/// a cycle whose blocks are missing or do not run one way, words it does
/// not know.
pub fn check_program(text: &str) -> Result<ProgramCheck, CamError> {
    let lines: Vec<&str> = text.lines().collect();
    let fail = |n: usize, what: String| CamError::new(format!("Line {n}: {what}."));
    // Sequence numbers and their lines.
    let mut labels: Vec<(u32, usize)> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if let Ok(ws) = words(l)
            && let Some((_, v)) = ws.iter().find(|(c, _)| *c == 'N')
            && let Ok(n) = v.parse::<u32>()
        {
            labels.push((n, i));
        }
    }
    let mut check = ProgramCheck {
        diameter_mode: true,
        ..ProgramCheck::default()
    };
    let mut motion: Option<u32> = None;
    let mut per_rev = false;
    let mut css = false;
    let mut limit = false;
    let mut feed: Option<f64> = None;
    let mut at: [Option<f64>; 2] = [None, None];
    let mut scale = 1.0;
    let mut units = false;
    // A G71 or G76 waiting for its second block.
    let mut pending: Option<u32> = None;
    // The last line of a G71's contour blocks, which the control reads for
    // the cycle and then passes by.
    let mut skip_to: Option<usize> = None;
    let mut tape = 0;
    for (i, raw) in lines.iter().enumerate() {
        let n = i + 1;
        let l = raw.trim();
        if l.is_empty() {
            continue;
        }
        if l == "%" {
            tape += 1;
            continue;
        }
        let ws = words(l).map_err(|e| fail(n, e))?;
        if ws.is_empty() {
            continue;
        }
        let get = |c: char| ws.iter().find(|(w, _)| *w == c).map(|(_, v)| v.as_str());
        let num = |c: char| -> Result<Option<f64>, CamError> {
            match get(c) {
                None => Ok(None),
                Some(v) => word_value(v)
                    .map(Some)
                    .ok_or_else(|| fail(n, format!("the {c} word is not a number"))),
            }
        };
        let gs: Vec<u32> = ws
            .iter()
            .filter(|(c, _)| *c == 'G')
            .map(|(_, v)| {
                v.parse::<u32>()
                    .map_err(|_| fail(n, format!("G{v} is not a command it knows")))
            })
            .collect::<Result<_, _>>()?;
        for (c, v) in &ws {
            if matches!(c, 'X' | 'Z' | 'I' | 'K' | 'U' | 'W' | 'R') && !v.contains('.') {
                return Err(fail(
                    n,
                    format!(
                        "{c} needs a decimal point, or the control reads it in its smallest steps"
                    ),
                ));
            }
            if !matches!(
                c,
                'N' | 'O'
                    | 'G'
                    | 'M'
                    | 'T'
                    | 'S'
                    | 'F'
                    | 'X'
                    | 'Z'
                    | 'I'
                    | 'K'
                    | 'U'
                    | 'W'
                    | 'R'
                    | 'P'
                    | 'Q'
            ) {
                return Err(fail(n, format!("the word {c} is not one it reads")));
            }
        }
        if ws[0].0 == 'O' {
            continue;
        }
        if let Some(end) = skip_to {
            if i >= end {
                skip_to = None;
            }
            continue;
        }
        if let Some(kind) = pending.take() {
            if !gs.contains(&kind) {
                return Err(fail(n, format!("G{kind} needs its second block here")));
            }
            match kind {
                71 => {
                    let (Some(p), Some(q)) = (num('P')?, num('Q')?) else {
                        return Err(fail(
                            n,
                            "G71 needs the blocks of its contour (P and Q)".to_owned(),
                        ));
                    };
                    if num('F')?.is_none() {
                        return Err(fail(n, "G71 needs a feed".to_owned()));
                    }
                    let u = num('U')?.unwrap_or(0.0);
                    let end = check_contour(&lines, &labels, p as u32, q as u32, Some(u < 0.0), n)?;
                    if end.0 != i + 1 {
                        return Err(fail(n, "the contour's blocks must follow G71".to_owned()));
                    }
                    skip_to = Some(end.1);
                    check.cycles.push(("G71".to_owned(), n));
                }
                76 => {
                    for w in ['X', 'Z', 'P', 'Q', 'F'] {
                        if get(w).is_none() {
                            return Err(fail(n, format!("G76 needs its {w} word")));
                        }
                    }
                    if css {
                        return Err(fail(n, "threading at a constant surface speed".to_owned()));
                    }
                    check.cycles.push(("G76".to_owned(), n));
                }
                _ => {}
            }
            // The tool is back where the cycle started; the motion and the
            // feed are written again.
            motion = None;
            feed = None;
            continue;
        }
        for &g in &gs {
            match g {
                0..=3 | 32 => motion = Some(g),
                20 => {
                    scale = 25.4;
                    units = true;
                }
                21 => {
                    scale = 1.0;
                    units = true;
                }
                18 | 40 | 54..=59 => {}
                98 => per_rev = false,
                99 => per_rev = true,
                96 => {
                    if !limit {
                        return Err(fail(
                            n,
                            "a constant surface speed without a spindle limit (G50 S) before it"
                                .to_owned(),
                        ));
                    }
                    css = true;
                    check.css_lines.push(n);
                }
                97 => css = false,
                50 => {
                    if get('X').is_some() || get('Z').is_some() {
                        return Err(fail(
                            n,
                            "G50 sets the coordinates here, which the program must not".to_owned(),
                        ));
                    }
                    if get('S').is_none() {
                        return Err(fail(n, "G50 without a spindle limit".to_owned()));
                    }
                    limit = true;
                }
                71 | 76 => {
                    if g == 71 && (get('U').is_none() || get('R').is_none()) {
                        return Err(fail(n, "G71 needs its depth of cut and retract".to_owned()));
                    }
                    if g == 76 && (get('P').is_none() || get('Q').is_none() || get('R').is_none()) {
                        return Err(fail(
                            n,
                            "G76 needs its passes, smallest depth and allowance".to_owned(),
                        ));
                    }
                    pending = Some(g);
                }
                70 => {
                    let (Some(p), Some(q)) = (num('P')?, num('Q')?) else {
                        return Err(fail(
                            n,
                            "G70 needs the blocks of its contour (P and Q)".to_owned(),
                        ));
                    };
                    check_contour(&lines, &labels, p as u32, q as u32, None, n)?;
                    check.cycles.push(("G70".to_owned(), n));
                    motion = None;
                    feed = None;
                }
                other => return Err(fail(n, format!("G{other} is not a command it reads"))),
            }
        }
        if pending.is_some() || gs.contains(&70) {
            continue;
        }
        if let Some(f) = num('F')? {
            feed = Some(f * scale);
        }
        let x = num('X')?;
        let z = num('Z')?;
        if get('U').is_some() || get('W').is_some() {
            return Err(fail(n, "incremental moves are not read".to_owned()));
        }
        if x.is_none() && z.is_none() {
            continue;
        }
        if !units {
            return Err(fail(
                n,
                "a move before the program says its units".to_owned(),
            ));
        }
        let Some(mo) = motion else {
            return Err(fail(n, "a move without a motion (G0 to G3)".to_owned()));
        };
        let to = [
            x.map(|v| v * scale / 2.0).or(at[0]),
            z.map(|v| v * scale).or(at[1]),
        ];
        let (Some(tx), Some(tz)) = (to[0], to[1]) else {
            return Err(fail(
                n,
                "a move from where the tool is not known".to_owned(),
            ));
        };
        let to = [tx, tz];
        let need_feed = || -> Result<f64, CamError> {
            if !per_rev {
                return Err(fail(
                    n,
                    "a feed before the feeds are per revolution (G99)".to_owned(),
                ));
            }
            feed.ok_or_else(|| fail(n, "a cut without a feed".to_owned()))
        };
        let m = match mo {
            0 => TurnMove::Rapid { to },
            1 => TurnMove::Line {
                to,
                feed: need_feed()?,
            },
            2 | 3 => {
                let (Some(i_), Some(k_)) = (num('I')?, num('K')?) else {
                    return Err(fail(n, "an arc without its centre (I and K)".to_owned()));
                };
                let (Some(fx), Some(fz)) = (at[0], at[1]) else {
                    return Err(fail(
                        n,
                        "an arc from where the tool is not known".to_owned(),
                    ));
                };
                TurnMove::Arc {
                    to,
                    center: [fx + i_ * scale, fz + k_ * scale],
                    cw: mo == 2,
                    feed: need_feed()?,
                }
            }
            32 => {
                if css {
                    return Err(fail(n, "threading at a constant surface speed".to_owned()));
                }
                TurnMove::Thread {
                    to,
                    lead: feed.ok_or_else(|| fail(n, "a thread without its lead".to_owned()))?,
                }
            }
            _ => unreachable!("motions are 0 to 3 and 32"),
        };
        at = [Some(to[0]), Some(to[1])];
        check.moves.push(ReadMove { line: n, motion: m });
    }
    if pending.is_some() {
        return Err(CamError::new("The program ends in the middle of a cycle."));
    }
    if tape == 1 {
        return Err(CamError::new(
            "The program has a tape mark at its start but not at its end.",
        ));
    }
    Ok(check)
}

/// Checks the blocks `p` to `q` a cycle runs and gives their lines
/// (0-based): for G71 (`rough` with the side, `Some(inside)`) the first
/// moves in X alone and X runs one way, out from the axis or towards it
/// inside.
fn check_contour(
    lines: &[&str],
    labels: &[(u32, usize)],
    p: u32,
    q: u32,
    rough: Option<bool>,
    n: usize,
) -> Result<(usize, usize), CamError> {
    let fail = |what: &str| CamError::new(format!("Line {n}: {what}."));
    let find = |label: u32| labels.iter().find(|(l, _)| *l == label).map(|(_, i)| *i);
    let (Some(a), Some(b)) = (find(p), find(q)) else {
        return Err(fail("the cycle's blocks are not in the program"));
    };
    if b < a {
        return Err(fail("the cycle's blocks are not where it runs them"));
    }
    let Some(inside) = rough else {
        return Ok((a, b));
    };
    let mut last_x: Option<f64> = None;
    for (k, l) in lines[a..=b].iter().enumerate() {
        let ws = words(l).map_err(|e| CamError::new(format!("Line {}: {e}.", a + k + 1)))?;
        let x = ws
            .iter()
            .find(|(c, _)| *c == 'X')
            .and_then(|(_, v)| word_value(v));
        if k == 0 && ws.iter().any(|(c, _)| *c == 'Z') {
            return Err(fail("the first block of the contour must move in X alone"));
        }
        if let Some(x) = x {
            if k > 0
                && let Some(prev) = last_x
                && (if inside {
                    x > prev + 1e-9
                } else {
                    x < prev - 1e-9
                })
            {
                return Err(fail("the contour does not run one way in X"));
            }
            last_x = Some(x);
        }
    }
    Ok((a, b))
}
