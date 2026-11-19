// SPDX-License-Identifier: MIT
//! Sheet parts by material and process (mitcad#1939; commands.md "Flat
//! Pack", "Standard boxes", "Die lines"): the commands that make several
//! things at once or write a die line.
//!
//! - `add_flat_pack`: the Flat Pack feature of a box solid and a cut sheet
//!   with its six plates nested.
//! - `add_standard_box`: a board box of a standard style from its options:
//!   its rules, the blank's outline and crease lines as sketches, a Face of
//!   the blank and a Fold at every crease (outermost first, so that each
//!   crease lies on the flat remainder when it folds), and the Flat
//!   Pattern item; the blank's geometry is [`blank`].
//! - `export_die_line`: a sheet's Flat Pattern as a die line, DXF with the
//!   profile's layers (cut and crease for board) or PDF with cuts solid,
//!   creases dashed and reverse creases chain-dotted.

use serde::Deserialize;
use serde_json::{Value, json};

use super::annotations::present;
use super::{ApiError, DocumentCrateOps};
use mitcad_model::Document;
use mitcad_model::drawing::cut_sheet_def::MarkKind;
use mitcad_model::ids::{BodyUid, EntityUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::sheet::flat_dxf::{FlatDxfOptions, drawing};
use mitcad_model::sheet::process::OutputPreset;
use mitcad_model::topo::{RegionKey, SegmentKey};

/// `add_flat_pack`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AddFlatPackCommand {
    body: BodyUid,
    thickness: Value,
    #[serde(default)]
    inside: bool,
    #[serde(default)]
    finger_width: Option<Value>,
    #[serde(default)]
    clearance: Option<Value>,
    /// The cut sheet's size, mm (the cut sheet's defaults when left out).
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    height: Option<f64>,
    #[serde(default)]
    kerf: Option<f64>,
}

/// `add_standard_box`: a style and its options, mm.
#[derive(Debug, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AddStandardBoxCommand {
    style: BoxStyle,
    length: f64,
    width: f64,
    height: f64,
    /// A board of the board profile (`c_flute`, `polypropylene_4`, …,
    /// `sheet_process`'s `boards`): its thickness, flute and deduction
    /// unless given.
    #[serde(default)]
    board: Option<String>,
    #[serde(default)]
    thickness: Option<f64>,
    /// The measured deduction of a 90 degree crease; the board's thickness
    /// when left out.
    #[serde(default)]
    deduction: Option<f64>,
    /// The inside radius of the creases; an eighth of the thickness when
    /// left out.
    #[serde(default)]
    radius: Option<f64>,
    #[serde(default = "glue_flap")]
    glue_flap: f64,
    /// The tuck's depth of a tuck end box; a third of the height, at most
    /// 20, when left out.
    #[serde(default)]
    tuck: Option<f64>,
    #[serde(default)]
    flute: Option<String>,
    /// Folded (the default), or the flat blank with its creases.
    #[serde(default = "yes")]
    folded: bool,
    /// In a new component of its own, activated: a part to save into a
    /// library.
    #[serde(default)]
    component: bool,
}

fn glue_flap() -> f64 {
    30.0
}

fn yes() -> bool {
    true
}

/// The standard box styles (FEFCO and ECMA codes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BoxStyle {
    /// The regular slotted box: four flaps at each end.
    #[serde(rename = "fefco_0201")]
    Fefco0201,
    /// The reverse tuck end carton: a closing panel with its tuck at the
    /// top of one long side and at the bottom of the other, dust (lock)
    /// flaps on the short sides.
    EcmaA20,
}

/// `export_die_line`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExportDieLineCommand {
    body: BodyUid,
    path: String,
    /// `dxf` or `pdf`; by the path's extension when left out.
    #[serde(default)]
    format: Option<String>,
    /// Sketches engraved (printed) or scored on the blank, laid on the
    /// Flat Pattern's fixed panel (mitcad#1939).
    #[serde(default)]
    marks: Vec<mitcad_model::drawing::cut_sheet_def::CutMark>,
}

/// A crease of a blank: its line from `a` to `b`, which side stays (left
/// of the line's direction or not), folded by 90 degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlankFold {
    pub a: [f64; 2],
    pub b: [f64; 2],
    pub stationary_left: bool,
}

/// A box's blank: its outline (counter-clockwise) and its creases in the
/// order they fold.
#[derive(Debug, Clone, PartialEq)]
pub struct Blank {
    pub outline: Vec<[f64; 2]>,
    pub folds: Vec<BlankFold>,
}

/// The sizes a blank is worked out from, mm: inside length, width and
/// height, board thickness, crease radius and allowance (of a 90 degree
/// crease), glue flap and tuck.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxSizes {
    pub length: f64,
    pub width: f64,
    pub height: f64,
    pub thickness: f64,
    pub radius: f64,
    pub allowance: f64,
    pub glue_flap: f64,
    pub tuck: f64,
}

/// A stack of flaps on one end of a side panel, from the panel outwards:
/// each (depth from the crease before it, inset from the panel's sides).
type Stack = Vec<(f64, f64)>;

/// The blank of a box style (mitcad#1939). The four side panels lie along
/// x (short, long, short, long), the glue flap after the last, the bottom
/// flaps below y = 0 and the top flaps above. Creases fold at their middle
/// (`bend_line`), so the distance between two creases is the straight
/// length between their bends plus one allowance: a wall between two bends
/// `inside - 2 r + BA`. The long panels' end creases lie a thickness
/// further out than the short panels', so that their flaps fold over the
/// short panels' flaps; the glue flap folds under the first panel's free
/// edge, touching it inside (the last panel's straight is short by the
/// thickness), and is a thickness in from the band's ends. Slots between
/// the flaps are a thickness wider than the creases' strips and reach half
/// a strip into the band, so that beside a crease's end no material of the
/// panel lies in its strip (each crease's ends lie on the outline, the
/// strip clear of the rest of the blank); a narrower flap on another (a
/// tuck) keeps its root's width over the strip before it narrows.
pub fn blank(style: BoxStyle, s: &BoxSizes) -> Blank {
    let (l, w, h, t, r, ba) = (
        s.length,
        s.width,
        s.height,
        s.thickness,
        s.radius,
        s.allowance,
    );
    let half = ba / 2.0;
    let x1 = w + t - r + half;
    let x2 = x1 + l - 2.0 * r + ba;
    let x3 = x2 + w - 2.0 * r + ba;
    // Sheets that lie against each other in the folded box (a long
    // panel's flap over a short panel's, the glue flap and a tuck inside a
    // wall) keep a seam gap between them, as a die leaves them: a sheet body
    // holds no two walls touching over a face.
    let gap = seam_gap(t);
    let x4 = x3 + l - 2.0 * r - t - gap + ba;
    let x5 = x4 + half + s.glue_flap;
    let band = h - 2.0 * r + ba;
    let slot = ba.max(2.0 * t) + t;
    // A narrower flap keeps its root's width this far past its crease.
    let shoulder = half + t;
    let edges = [0.0, x1, x2, x3, x4];
    // Each panel's flap span [u, v] and whether it is a long panel.
    let spans: Vec<(f64, f64, bool)> = (0..4)
        .map(|i| {
            // The first panel's flaps keep half a slot off its free edge
            // too: in the folded box that edge meets the last panel's
            // corner, where the last panel's flaps bend.
            // A tuck end's dust flaps keep clear of the tucks, which stand
            // inside the long walls beside them: narrower by the tuck's
            // thickness, its seam gap and its bend's radius on each side.
            let long = i % 2 == 1;
            let clear = if style == BoxStyle::EcmaA20 && !long {
                2.0 * t + gap + r
            } else {
                0.0
            };
            let u = edges[i] + slot / 2.0 + clear;
            let v = edges[i + 1] - slot / 2.0 - clear;
            (u, v, long)
        })
        .collect();
    let flap = w / 2.0 - t;
    let closing = w - 2.0 * r - t - gap + ba;
    let tuck_inset = t + r;
    let (bottom, top): (Vec<Stack>, Vec<Stack>) = match style {
        BoxStyle::Fefco0201 => (vec![vec![(flap, 0.0)]; 4], vec![vec![(flap, 0.0)]; 4]),
        BoxStyle::EcmaA20 => (
            vec![
                vec![(flap, 0.0)],
                vec![],
                vec![(flap, 0.0)],
                vec![(closing, 0.0), (s.tuck, tuck_inset)],
            ],
            vec![
                vec![(flap, 0.0)],
                vec![(closing, 0.0), (s.tuck, tuck_inset)],
                vec![(flap, 0.0)],
                vec![],
            ],
        ),
    };
    let low = |long: bool| if long { -(t + gap) } else { 0.0 };
    let high = |long: bool| if long { band + t + gap } else { band };
    // The slots' ends, half a strip into the band.
    let (slot_low, slot_high) = (half, band - half);
    // A stack's sides from the panel outwards (`dir` -1 down, +1 up): its
    // left and right sides' points from the panel, and its creases (each
    // across the width of the flap before it).
    let stack_sides = |u: f64, v: f64, y0: f64, stack: &Stack, dir: f64| {
        let mut left: Vec<[f64; 2]> = Vec::new();
        let mut right: Vec<[f64; 2]> = Vec::new();
        let mut creases: Vec<([f64; 2], [f64; 2])> = Vec::new();
        let mut y = y0;
        let mut before = 0.0;
        for &(depth, inset) in stack {
            creases.push(([u + before, y], [v - before, y]));
            left.push([u + before, y]);
            right.push([v - before, y]);
            if inset > before {
                left.push([u + before, y + dir * shoulder]);
                left.push([u + inset, y + dir * shoulder]);
                right.push([v - before, y + dir * shoulder]);
                right.push([v - inset, y + dir * shoulder]);
            }
            y += dir * depth;
            left.push([u + inset, y]);
            right.push([v - inset, y]);
            before = inset;
        }
        (left, right, creases)
    };
    let mut outline: Vec<[f64; 2]> = Vec::new();
    // The flaps' creases per stack (outermost first) with whether the
    // stack is a long panel's: the short panels' fold first (below).
    let mut stacks: Vec<(bool, Vec<BlankFold>)> = Vec::new();
    // The bottom, left to right: each panel's stack down its left side and
    // up its right side, the slots between.
    for (i, &(u, v, long)) in spans.iter().enumerate() {
        let y0 = low(long);
        if i == 0 {
            outline.push([0.0, slot_low]);
        }
        outline.push([u, slot_low]);
        let stack = &bottom[i];
        if stack.is_empty() {
            outline.push([u, y0]);
            outline.push([v, y0]);
        } else {
            let (left, mut right, creases) = stack_sides(u, v, y0, stack, -1.0);
            outline.extend(left);
            right.reverse();
            outline.extend(right);
            stacks.push((
                long,
                creases
                    .into_iter()
                    .rev()
                    .map(|(a, b)| BlankFold {
                        a,
                        b,
                        stationary_left: true,
                    })
                    .collect(),
            ));
        }
        if i < 3 {
            outline.push([v, slot_low]);
        }
    }
    // The glue flap: the band past the last panel's flaps notched half a
    // strip in, so that its flaps' creases end clear of it.
    let (_, v4, _) = spans[3];
    // The glue flap and its crease keep a thickness clear of the flaps'
    // creases at both ends (in the folded box its bend stands in the first
    // panel's corner, where that panel's flaps bend too), the band narrowed
    // to it from where the last panel's flaps end; the flap as high as its
    // crease, so that its outline does not step on the bend's far tangent
    // line (a seam must pair one stretch with one, bends.rs pair_all).
    let (glue_low, glue_high) = (half + t, band - half - t);
    outline.push([v4, glue_low]);
    outline.push([x5, glue_low]);
    outline.push([x5, glue_high]);
    outline.push([v4, glue_high]);
    // The top, right to left.
    for i in (0..4).rev() {
        let (u, v, long) = spans[i];
        let y0 = high(long);
        if i < 3 {
            outline.push([v, slot_high]);
        }
        let stack = &top[i];
        if stack.is_empty() {
            outline.push([v, y0]);
            outline.push([u, y0]);
        } else {
            let (mut left, right, creases) = stack_sides(u, v, y0, stack, 1.0);
            outline.extend(right);
            left.reverse();
            outline.extend(left);
            stacks.push((
                long,
                creases
                    .into_iter()
                    .rev()
                    .map(|(a, b)| BlankFold {
                        a,
                        b,
                        stationary_left: false,
                    })
                    .collect(),
            ));
        }
        outline.push([u, slot_high]);
        if i == 0 {
            outline.push([0.0, slot_high]);
        }
    }
    dedup(&mut outline);
    // The short panels' flaps fold first: a fold refuses a seam of an
    // earlier bend that lies past its own near tangent outside its line's
    // ends (bends.rs, fold_panel), and the long panels' creases lie a
    // thickness further out, so a short panel's crease would find a long
    // panel's seam past it; the other way round each earlier seam lies
    // behind. A stack folds outermost first, while its creases still lie on
    // the flat blank.
    let mut folds: Vec<BlankFold> = stacks
        .iter()
        .filter(|(long, _)| !long)
        .chain(stacks.iter().filter(|(long, _)| *long))
        .flat_map(|(_, creases)| creases.iter().copied())
        .collect();
    folds.push(BlankFold {
        a: [x4, glue_low],
        b: [x4, glue_high],
        stationary_left: true,
    });
    // The walls' creases run past the blank at both ends, through the
    // slots: the whole side past each crease (the next panels with their
    // folded flaps and the glue flap) folds with it, every earlier seam
    // there within the crease's ends.
    let (y_min, y_max) = outline
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p[1]), hi.max(p[1]))
        });
    for x in [x3, x2, x1] {
        folds.push(BlankFold {
            a: [x, y_min - 1.0],
            b: [x, y_max + 1.0],
            stationary_left: true,
        });
    }
    Blank { outline, folds }
}

/// The gap left between sheets of a box that lie against each other, mm: a
/// tenth of the board's thickness, at least 0.05 mm.
pub fn seam_gap(thickness: f64) -> f64 {
    (thickness / 10.0).max(0.05)
}

/// The outline without repeated points or points on a straight run.
fn dedup(points: &mut Vec<[f64; 2]>) {
    let near = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9;
    points.dedup_by(|a, b| near(*a, *b));
    while points.len() > 1 && near(points[0], *points.last().expect("points")) {
        points.pop();
    }
    loop {
        let n = points.len();
        let straight = (0..n).find(|&i| {
            let (a, b, c) = (points[(i + n - 1) % n], points[i], points[(i + 1) % n]);
            ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() < 1e-9
        });
        match straight {
            Some(i) if n > 3 => {
                points.remove(i);
            }
            _ => break,
        }
    }
}

/// The region key of a polygon of lines `c<first>` on, in its order.
fn polygon_key(first: u32, n: u32) -> String {
    let id = |i: u32| EntityUid(first + (i % n));
    let keys: Vec<SegmentKey> = (0..n)
        .map(|i| SegmentKey::between(id(i), id(i + n - 1), id(i + 1)))
        .collect();
    RegionKey::new(keys).expect("a polygon").to_string()
}

fn uid_of(answer: &Value) -> Result<String, ApiError> {
    answer["uid"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| ApiError(format!("no feature was added: {answer}")))
}

/// `add_flat_pack`.
pub(super) fn add_flat_pack<K: Kernel>(
    doc: &mut Document<K>,
    c: AddFlatPackCommand,
) -> Result<Value, ApiError> {
    let depth = doc.undo_depth();
    let mut def = json!({"type": "flat_pack", "body": c.body, "thickness": c.thickness});
    if c.inside {
        def["inside"] = json!(true);
    }
    if let Some(w) = c.finger_width {
        def["finger_width"] = w;
    }
    if let Some(v) = c.clearance {
        def["clearance"] = v;
    }
    let added = doc.run_command(json!({"cmd": "add_feature", "def": def}))?;
    let uid = uid_of(&added)?;
    let bodies: Vec<String> = (0..6).map(|k| format!("{uid}.b{k}")).collect();
    let mut sheet = json!({"cmd": "add_cut_sheet", "bodies": bodies, "nest": true});
    for (key, value) in [("width", c.width), ("height", c.height), ("kerf", c.kerf)] {
        if let Some(v) = value {
            sheet[key] = json!(v);
        }
    }
    let cut = doc.run_command(sheet)?;
    // One undo step for the feature and its cut sheet.
    doc.merge_undo(depth, "Flat Pack");
    Ok(
        json!({"uid": uid, "bodies": bodies, "drawing": cut["drawing"],
              "warnings": cut["warnings"]}),
    )
}

/// A box's options worked out: its rules' definition, its blank's outline
/// and crease sketches' entities and its creases.
struct BoxMade {
    rules: Value,
    outline: Vec<Value>,
    creases: Vec<Value>,
    blank: Blank,
}

/// The rules, sketches and blank of a box's options (`add_standard_box`'s
/// fields), refused in words where the options make no box.
fn make_box(c: &AddStandardBoxCommand) -> Result<BoxMade, ApiError> {
    let preset = match &c.board {
        Some(id) => Some(
            mitcad_model::sheet::process::board_preset(id)
                .ok_or_else(|| ApiError(format!("unknown board “{id}”")))?,
        ),
        None => None,
    };
    let thickness = c
        .thickness
        .or(preset.map(|p| p.thickness))
        .ok_or_else(|| ApiError("a standard box needs a board or a thickness".into()))?;
    let flute = c
        .flute
        .clone()
        .or(preset.and_then(|p| p.flute.map(str::to_owned)));
    for (what, v) in [
        ("length", c.length),
        ("width", c.width),
        ("height", c.height),
        ("thickness", thickness),
        ("glue flap", c.glue_flap),
    ] {
        if !v.is_finite() || v <= 0.0 {
            return Err(ApiError(format!(
                "the box's {what} must be greater than zero, got {v}"
            )));
        }
    }
    let t = thickness;
    let r = c.radius.unwrap_or(t / 8.0);
    let deduction = c.deduction.or(preset.map(|p| p.deduction)).unwrap_or(t);
    let allowance = 2.0 * (r + t) - deduction;
    if allowance <= 0.0 || !allowance.is_finite() {
        return Err(ApiError(
            "the crease deduction is too large for the board: a crease must develop to more \
             than 0 mm"
                .into(),
        ));
    }
    if c.glue_flap <= 2.0 * t {
        return Err(ApiError(
            "the glue flap must be wider than twice the board's thickness".into(),
        ));
    }
    let min_side = c.width.min(c.length).min(c.height);
    if min_side <= 4.0 * (r + t) + allowance {
        return Err(ApiError(
            "the box is too small for its board: each side must be longer than four times \
             the board's thickness and its creases"
                .into(),
        ));
    }
    let sizes = BoxSizes {
        length: c.length,
        width: c.width,
        height: c.height,
        thickness: t,
        radius: r,
        allowance,
        glue_flap: c.glue_flap,
        tuck: c.tuck.unwrap_or((c.height / 3.0).min(20.0)),
    };
    let blank = blank(c.style, &sizes);
    let mut rules = json!({"type": "sheet_rules", "process": "board", "thickness": t,
        "bend_radius": r, "allowance": {"type": "bend_deduction", "deduction": deduction},
        "reference_side": "front",
        "bend_relief": {"type": "round", "width": t, "depth": t},
        "corner_relief": {"type": "round", "width": t, "depth": t},
        "standard_box": options_json(c)});
    if let Some(flute) = &flute {
        rules["flute"] = json!(flute);
    }
    // The outline: points p1.. and lines after them.
    let n = blank.outline.len() as u32;
    let mut outline: Vec<Value> = blank
        .outline
        .iter()
        .enumerate()
        .map(|(i, p)| json!({"id": format!("p{}", i + 1), "type": "point", "at": p}))
        .collect();
    for i in 0..n {
        outline.push(json!({"id": format!("c{}", n + 1 + i), "type": "line",
                             "start": format!("p{}", i + 1),
                             "end": format!("p{}", (i + 1) % n + 1)}));
    }
    let mut creases: Vec<Value> = Vec::new();
    for (k, fold) in blank.folds.iter().enumerate() {
        creases.push(json!({"id": format!("p{}", 3 * k + 1), "type": "point", "at": fold.a}));
        creases.push(json!({"id": format!("p{}", 3 * k + 2), "type": "point", "at": fold.b}));
        creases.push(json!({"id": format!("c{}", 3 * k + 3), "type": "line",
                          "start": format!("p{}", 3 * k + 1), "end": format!("p{}", 3 * k + 2)}));
    }
    Ok(BoxMade {
        rules,
        outline,
        creases,
        blank,
    })
}

/// A box's options as its rules keep them (`standard_box`): its fields but
/// `folded` and `component`, which only say how it is made.
fn options_json(c: &AddStandardBoxCommand) -> Value {
    let mut v = serde_json::to_value(c).unwrap_or_default();
    if let Some(o) = v.as_object_mut() {
        o.remove("folded");
        o.remove("component");
        o.retain(|_, value| !value.is_null());
    }
    v
}

/// `add_standard_box`.
pub(super) fn add_standard_box<K: Kernel>(
    doc: &mut Document<K>,
    c: AddStandardBoxCommand,
) -> Result<Value, ApiError> {
    let made = make_box(&c)?;
    let depth = doc.undo_depth();
    // As a component of its own: a part to save into a library.
    let component = if c.component {
        let name = match c.style {
            BoxStyle::Fefco0201 => "FEFCO 0201 Box",
            BoxStyle::EcmaA20 => "ECMA A20 Carton",
        };
        let answer = doc.run_command(json!({"cmd": "create_component", "name": name,
                                             "activate": true}))?;
        answer.get("component").cloned()
    } else {
        None
    };
    let rules = uid_of(&doc.run_command(json!({"cmd": "add_feature", "def": made.rules}))?)?;
    let outline = uid_of(&doc.run_command(json!({"cmd": "add_feature",
        "def": {"type": "sketch", "plane": "xy", "entities": made.outline}}))?)?;
    let creases = uid_of(&doc.run_command(json!({"cmd": "add_feature",
        "def": {"type": "sketch", "plane": "xy", "entities": made.creases}}))?)?;
    let n = made.blank.outline.len() as u32;
    let region = polygon_key(n + 1, n);
    let face = uid_of(
        &doc.run_command(json!({"cmd": "add_feature", "def": {"type": "sheet_face",
        "rules": rules, "profiles": [{"sketch": outline, "region": region}]}}))?,
    )?;
    let body = format!("{face}.b0");
    let mut folds = Vec::new();
    if c.folded {
        for (k, fold) in made.blank.folds.iter().enumerate() {
            let def = json!({"type": "sheet_fold", "body": body,
                "panel": {"feature": face, "slot": 0},
                "line": {"sketch": creases, "curve": format!("c{}", 3 * k + 3)},
                "stationary_side": if fold.stationary_left { "left" } else { "right" },
                "angle": std::f64::consts::FRAC_PI_2, "position": "bend_line"});
            folds.push(uid_of(
                &doc.run_command(json!({"cmd": "add_feature", "def": def}))?,
            )?);
        }
    }
    doc.run_command(json!({"cmd": "set_flat_pattern", "body": body,
                           "stationary": {"feature": face, "slot": 0}}))?;
    // One undo step for the whole box.
    doc.merge_undo(depth, "Standard Box");
    Ok(
        json!({"rules": rules, "outline": outline, "creases": creases, "face": face,
              "body": body, "folds": folds, "component": component}),
    )
}

/// `edit_standard_box`: the box of `rules` with the options given changed;
/// null takes an option away (the board's, or the default, then).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EditStandardBoxCommand {
    rules: mitcad_model::ids::FeatureUid,
    #[serde(default, deserialize_with = "present")]
    style: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    length: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    width: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    height: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    board: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    thickness: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    deduction: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    radius: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    glue_flap: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    tuck: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    flute: Option<Option<Value>>,
}

impl EditStandardBoxCommand {
    /// The options given: (name, value or null).
    fn options(&self) -> Vec<(&'static str, Option<Value>)> {
        [
            ("style", &self.style),
            ("length", &self.length),
            ("width", &self.width),
            ("height", &self.height),
            ("board", &self.board),
            ("thickness", &self.thickness),
            ("deduction", &self.deduction),
            ("radius", &self.radius),
            ("glue_flap", &self.glue_flap),
            ("tuck", &self.tuck),
            ("flute", &self.flute),
        ]
        .into_iter()
        .filter_map(|(name, v)| v.clone().map(|v| (name, v)))
        .collect()
    }
}

/// `edit_standard_box`: the box's rules, its blank's outline and its
/// creases made again from its options with those given changed, as one
/// undo step "Edit Standard Box"; its Face and Folds, which name the same
/// curves, follow. Refused for rules of no standard box, and where the
/// options change the blank's shape (another style).
pub(super) fn edit_standard_box<K: Kernel>(
    doc: &mut Document<K>,
    c: EditStandardBoxCommand,
) -> Result<Value, ApiError> {
    use mitcad_model::features::FeatureDef;
    let stored = match doc.feature(c.rules).map(|f| &f.def) {
        Some(FeatureDef::SheetRules(def)) => def.standard_box.clone(),
        _ => None,
    }
    .ok_or_else(|| ApiError(format!("{} are not the rules of a standard box", c.rules)))?;
    let mut options = stored.as_object().cloned().unwrap_or_default();
    for (key, value) in c.options() {
        match value {
            None | Some(Value::Null) => {
                options.remove(key);
            }
            Some(value) => {
                options.insert(key.to_owned(), value);
            }
        }
    }
    let command: AddStandardBoxCommand = serde_json::from_value(Value::Object(options))
        .map_err(|e| ApiError(format!("the box's options: {e}")))?;
    let made = make_box(&command)?;
    // The features the box made: its Face (of these rules), the Face's
    // outline sketch and the sketch its Folds' lines are on.
    let mut face = None;
    let mut outline = None;
    let mut creases = None;
    for entry in doc.features() {
        match &entry.def {
            FeatureDef::SheetFace(f) if f.rules == c.rules && f.body.is_none() => {
                face = Some(entry.uid);
                outline = f.profiles.first().map(|p| p.sketch);
            }
            FeatureDef::SheetFold(f) if Some(f.body.feature) == face => {
                creases = Some(f.line.sketch);
            }
            _ => {}
        }
    }
    let outline = outline.ok_or_else(|| ApiError("the box's Face is gone".into()))?;
    let count = |uid: mitcad_model::ids::FeatureUid| match doc.feature(uid).map(|f| &f.def) {
        Some(FeatureDef::Sketch(def)) => def.entities.len(),
        _ => 0,
    };
    if count(outline) != made.outline.len()
        || creases.is_some_and(|creases| count(creases) != made.creases.len())
    {
        return Err(ApiError(
            "these options change the shape of the box's blank: make the box again".into(),
        ));
    }
    let depth = doc.undo_depth();
    doc.run_command(json!({"cmd": "edit_feature", "uid": c.rules, "def": made.rules}))?;
    doc.run_command(json!({"cmd": "edit_feature", "uid": outline,
        "def": {"type": "sketch", "plane": "xy", "entities": made.outline}}))?;
    if let Some(creases) = creases {
        doc.run_command(json!({"cmd": "edit_feature", "uid": creases,
            "def": {"type": "sketch", "plane": "xy", "entities": made.creases}}))?;
    }
    doc.merge_undo(depth, "Edit Standard Box");
    Ok(json!({"rules": c.rules, "outline": outline, "creases": creases, "face": face}))
}

/// `add_glue_flap`: a Flange on a board sheet's edge whose wall is a glue
/// flap, its ends tapered.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AddGlueFlapCommand {
    /// The flange's definition, as `add_feature` takes it (its length is the
    /// flap's).
    def: Value,
    /// How far each end slants in, degrees from square (default 15).
    #[serde(default)]
    taper: Option<f64>,
    /// How far the flap keeps in from each end of the edge, mm (default 0).
    #[serde(default)]
    inset: Option<f64>,
}

/// `add_glue_flap` (mitcad#1939): the flange's profile sketch on its wall
/// (`sheet_flange_profile`) made a trapezoid, its root along the edge
/// `inset` in from each end, its sides slanting in by `taper` degrees, as
/// glue flaps are; the flange with that profile; one undo step "Glue Flap".
pub(super) fn add_glue_flap<K: Kernel>(
    doc: &mut Document<K>,
    c: AddGlueFlapCommand,
) -> Result<Value, ApiError> {
    use mitcad_model::features::FeatureDef;
    let taper = c.taper.unwrap_or(15.0);
    let inset = c.inset.unwrap_or(0.0);
    if !(0.0..60.0).contains(&taper) || !inset.is_finite() || inset < 0.0 {
        return Err(ApiError(
            "a glue flap's taper is from 0 to 60 degrees and its inset not negative".into(),
        ));
    }
    let depth = doc.undo_depth();
    let made = doc.run_command(json!({"cmd": "sheet_flange_profile", "def": c.def}))?;
    let sketch: mitcad_model::ids::FeatureUid = made["sketch"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| ApiError("no profile sketch was made".into()))?;
    // The wall's rectangle: its root on the bend line (y = 0) and its far
    // side at the flap's length.
    let points: Vec<[f64; 2]> = doc
        .sketch_output(sketch)
        .map(|o| o.solved.points.values().copied().collect())
        .unwrap_or_default();
    let root: Vec<f64> = points
        .iter()
        .filter(|p| p[1].abs() < 1e-6)
        .map(|p| p[0])
        .collect();
    let (x0, x1) = root
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| {
            (a.min(*x), b.max(*x))
        });
    let length = points.iter().map(|p| p[1]).fold(0.0f64, f64::max);
    let slant = length * taper.to_radians().tan();
    let (a, b) = (x0 + inset, x1 - inset);
    if length <= 0.0 || b - a - 2.0 * slant <= 0.0 {
        return Err(ApiError(
            "the glue flap's taper and inset leave no flap: make it shorter or its taper smaller"
                .into(),
        ));
    }
    let corners = [[a, 0.0], [b, 0.0], [b - slant, length], [a + slant, length]];
    let mut entities: Vec<Value> = corners
        .iter()
        .enumerate()
        .map(|(i, p)| json!({"id": format!("p{}", i + 1), "type": "point", "at": p}))
        .collect();
    for i in 0..4 {
        entities.push(json!({"id": format!("c{}", i + 5), "type": "line",
            "start": format!("p{}", i + 1), "end": format!("p{}", (i + 1) % 4 + 1)}));
    }
    // On its plane, the wall's, with the trapezoid's corners fixed where
    // they are.
    let plane = match doc.feature(sketch).map(|f| &f.def) {
        Some(FeatureDef::Sketch(def)) => serde_json::to_value(&def.plane).unwrap_or_default(),
        _ => return Err(ApiError("the profile sketch is gone".into())),
    };
    let sketch_def = json!({"type": "sketch", "plane": plane, "entities": entities});
    doc.run_command(json!({"cmd": "edit_feature", "uid": sketch, "def": sketch_def}))?;
    let mut flange = c.def;
    flange["profile"] = json!(sketch);
    let added = doc.run_command(json!({"cmd": "add_feature", "def": flange}))?;
    doc.merge_undo(depth, "Glue Flap");
    Ok(json!({"uid": added["uid"], "profile": sketch}))
}

/// `export_die_line`.
pub(super) fn export_die_line<K: Kernel>(
    doc: &Document<K>,
    c: ExportDieLineCommand,
) -> Result<Value, ApiError> {
    let extension = std::path::Path::new(&c.path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    let format = match c.format.map(|f| f.to_ascii_lowercase()).or(extension) {
        Some(f) if f == "dxf" || f == "pdf" => f,
        _ => {
            return Err(ApiError(format!(
                "{}: a die line is written as DXF or PDF",
                c.path
            )));
        }
    };
    let outcome = doc.flat_pattern(c.body);
    let pattern = match &*outcome {
        Ok(pattern) => pattern,
        Err(failure) => {
            return Err(ApiError(format!(
                "the sheet has no die line: {}",
                failure.diagnostics.join("; ")
            )));
        }
    };
    let process = doc.sheet_process_of_rules(pattern.developed.rules);
    let options = FlatDxfOptions {
        process,
        ..FlatDxfOptions::default()
    };
    let mut dxf = drawing(&pattern.regions, &pattern.bends, &options).map_err(ApiError)?;
    // The marks on the preset's engrave and score layers (mitcad#1939).
    let preset = OutputPreset::of(process);
    let frame = mitcad_model_drawing::cut_sheet::MarkFrame {
        transform: None,
        origin: pattern.frame.origin,
        x: pattern.frame.x_axis,
        y: pattern.frame.y_axis,
        mirror: pattern.def.reference_side == mitcad_model::sheet::SheetReferenceSide::Back,
    };
    for mark in &c.marks {
        let lines = doc
            .sketch_mark_polylines(mark.sketch)
            .ok_or_else(|| ApiError(format!("{} is no sketch", mark.sketch)))?;
        let layer = match mark.kind {
            MarkKind::Engrave => preset.engrave.clone(),
            MarkKind::Score => preset.score.clone(),
            MarkKind::Cut => preset.cut_inner.clone(),
        };
        if !dxf.layers.iter().any(|l| l.name == layer) {
            dxf.layers.push(mitcad_dxf::Layer {
                color: preset.colour(&layer),
                ..mitcad_dxf::Layer::new(layer.as_str())
            });
        }
        for line in lines {
            for pair in line.windows(2) {
                let (a, b) = (frame.point(pair[0]), frame.point(pair[1]));
                dxf.push(
                    layer.as_str(),
                    mitcad_dxf::Geometry::Line {
                        start: mitcad_dxf::Point2::new(a[0], a[1]),
                        end: mitcad_dxf::Point2::new(b[0], b[1]),
                    },
                );
            }
        }
    }
    let count = |layer: &str| dxf.entities.iter().filter(|e| e.layer == layer).count();
    let layers: Vec<Value> = dxf
        .layers
        .iter()
        .map(|l| json!({"name": l.name, "entities": count(&l.name)}))
        .collect();
    let bytes = if format == "dxf" {
        mitcad_dxf::write_string(&dxf, &mitcad_dxf::WriteOptions::default())
            .map_err(|e| ApiError(format!("cannot write {}: {e}", c.path)))?
            .into_bytes()
    } else {
        die_line_pdf(&dxf, &OutputPreset::of(process))
    };
    std::fs::write(&c.path, bytes)
        .map_err(|e| ApiError(format!("cannot write {}: {e}", c.path)))?;
    Ok(
        json!({"path": c.path, "format": format, "body": c.body, "process": process.id(),
              "layers": layers}),
    )
}

/// A die line drawing as a PDF page at 1:1 with a 10 mm margin: cuts
/// solid, creases dashed, reverse creases chain-dotted, the rest thin;
/// a B-spline or ellipse by its control points.
fn die_line_pdf(dxf: &mitcad_dxf::Drawing, preset: &OutputPreset) -> Vec<u8> {
    use mitcad_dxf::Geometry;
    use mitcad_model_drawing::sheet::{Group, Item, Pen, Sheet};
    const MARGIN: f64 = 10.0;
    let (lo, hi) = dxf
        .bounds()
        .map_or(([0.0, 0.0], [0.0, 0.0]), |(a, b)| ([a.x, a.y], [b.x, b.y]));
    let at = |x: f64, y: f64| [x - lo[0] + MARGIN, y - lo[1] + MARGIN];
    let mut group = Group::new("view");
    for entity in &dxf.entities {
        let pen = if entity.layer == preset.cut_outer || entity.layer == preset.cut_inner {
            Pen::Thick
        } else if entity.layer == preset.bend_up {
            Pen::Hidden
        } else if entity.layer == preset.bend_down {
            Pen::Chain
        } else if entity.layer == preset.score {
            Pen::Phantom
        } else {
            Pen::Thin
        };
        match &entity.geometry {
            Geometry::Line { start, end } => group.items.push(Item::Line {
                pen,
                a: at(start.x, start.y),
                b: at(end.x, end.y),
            }),
            Geometry::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            } => group.items.push(Item::Arc {
                pen,
                center: at(center.x, center.y),
                radius: *radius,
                start: *start_angle,
                end: *end_angle,
            }),
            Geometry::Circle { center, radius } => group.items.push(Item::Arc {
                pen,
                center: at(center.x, center.y),
                radius: *radius,
                start: 0.0,
                end: std::f64::consts::TAU,
            }),
            Geometry::Spline(spline) => group.items.push(Item::Polyline {
                pen,
                points: spline.control_points.iter().map(|p| at(p.x, p.y)).collect(),
                closed: spline.closed,
            }),
            Geometry::Ellipse {
                center,
                major_axis,
                ratio,
                start_param,
                end_param,
            } => {
                let minor = [-major_axis.y * ratio, major_axis.x * ratio];
                let points = (0..=64)
                    .map(|i| {
                        let t = start_param + (end_param - start_param) * i as f64 / 64.0;
                        at(
                            center.x + major_axis.x * t.cos() + minor[0] * t.sin(),
                            center.y + major_axis.y * t.cos() + minor[1] * t.sin(),
                        )
                    })
                    .collect();
                group.items.push(Item::Polyline {
                    pen,
                    points,
                    closed: false,
                });
            }
            Geometry::Point(_) | Geometry::Text(_) => {}
        }
    }
    let sheet = Sheet {
        width: hi[0] - lo[0] + 2.0 * MARGIN,
        height: hi[1] - lo[1] + 2.0 * MARGIN,
        title: "Die line".to_owned(),
        groups: vec![group],
    };
    mitcad_model_drawing::pdf::write(&[sheet], "Die line")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sizes() -> BoxSizes {
        BoxSizes {
            length: 300.0,
            width: 200.0,
            height: 150.0,
            thickness: 4.0,
            radius: 0.5,
            allowance: 2.0 * 4.5 - 4.0,
            glue_flap: 30.0,
            tuck: 20.0,
        }
    }

    fn area(points: &[[f64; 2]]) -> f64 {
        let n = points.len();
        (0..n)
            .map(|i| {
                let (a, b) = (points[i], points[(i + 1) % n]);
                a[0] * b[1] - b[0] * a[1]
            })
            .sum::<f64>()
            / 2.0
    }

    /// A FEFCO 0201 blank: a simple counter-clockwise outline, eight flap
    /// creases first (each on the outline at both ends), then the glue
    /// flap's and the three walls' creases from the far end.
    #[test]
    fn a_slotted_box_blank_folds_its_flaps_first() {
        let b = blank(BoxStyle::Fefco0201, &sizes());
        assert!(area(&b.outline) > 0.0, "counter-clockwise");
        assert_eq!(b.folds.len(), 8 + 1 + 3);
        // The walls: x1 = W + t - r + BA/2 = 200 + 4 - 0.5 + 2.5.
        let x1 = 206.0;
        assert!((b.folds[11].a[0] - x1).abs() < 1e-9, "{:?}", b.folds[11]);
        // The wall creases run past the blank at both ends.
        let (lo, hi) = b
            .outline
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), p| {
                (l.min(p[1]), h.max(p[1]))
            });
        assert_eq!(b.folds[11].a[1], lo - 1.0);
        assert_eq!(b.folds[11].b[1], hi + 1.0);
        // The short panels' flaps first (their creases at y = 0 and the
        // band's top), then the long panels' (a thickness further out).
        let band = 150.0 - 1.0 + 5.0;
        for fold in &b.folds[..4] {
            assert!(
                fold.a[1] == 0.0 || (fold.a[1] - band).abs() < 1e-9,
                "{fold:?}"
            );
        }
        for fold in &b.folds[4..8] {
            assert!(
                (fold.a[1] + 4.4).abs() < 1e-9 || (fold.a[1] - band - 4.4).abs() < 1e-9,
                "{fold:?}"
            );
        }
        // Every crease's ends lie on the outline.
        let on = |p: [f64; 2]| {
            let n = b.outline.len();
            (0..n).any(|i| {
                let (a, c) = (b.outline[i], b.outline[(i + 1) % n]);
                let cross = (c[0] - a[0]) * (p[1] - a[1]) - (c[1] - a[1]) * (p[0] - a[0]);
                let within =
                    (p[0] - a[0]) * (p[0] - c[0]) <= 1e-9 && (p[1] - a[1]) * (p[1] - c[1]) <= 1e-9;
                cross.abs() < 1e-6 && within
            })
        };
        for fold in &b.folds[..9] {
            assert!(on(fold.a) && on(fold.b), "{fold:?}");
        }
    }

    #[test]
    fn a_tuck_end_blank_has_closing_panels_and_tucks() {
        let b = blank(BoxStyle::EcmaA20, &sizes());
        assert!(area(&b.outline) > 0.0);
        // Dust flaps 2 + 2, two closing panels with their tucks (4), the
        // glue flap and three walls.
        assert_eq!(b.folds.len(), 4 + 4 + 1 + 3);
        // The tucks fold before their closing panels.
        let tuck = b.folds.iter().position(|f| f.a[1] > 154.0 + 190.0).unwrap();
        let closing = b
            .folds
            .iter()
            .position(|f| (f.a[1] - 158.4).abs() < 1e-6)
            .unwrap();
        assert!(tuck < closing);
    }

    #[test]
    fn the_region_key_names_the_polygon_lines() {
        assert_eq!(
            polygon_key(5, 4),
            "r{c5[c8,c6],c6[c5,c7],c7[c6,c8],c8[c7,c5]}"
        );
    }
}
