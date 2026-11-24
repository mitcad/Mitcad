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

mod folder;

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
    /// The tuck's depth of a tuck end box or a folder's lid (folded, below
    /// the lid); a third of the height, at most 20, when left out, of a
    /// folder whose lid has a front flap (0426, 0427, 0471) the height less
    /// the thickness.
    #[serde(default)]
    tuck: Option<f64>,
    /// A folder's locking tabs on each turned over wall (mitcad#2226); the
    /// style's (two, three for 0426, one for 0470 and 0471) when left out.
    #[serde(default)]
    tabs: Option<u32>,
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
    /// The full overlap slotted box: every flap the width deep, the long
    /// panels' flaps lying one over the other across the whole end.
    #[serde(rename = "fefco_0203")]
    Fefco0203,
    /// The slotted box with the short panels' flaps meeting at the middle
    /// (half the length deep) and the long panels' flaps the width deep,
    /// overlapping fully over them.
    #[serde(rename = "fefco_0206")]
    Fefco0206,
    /// The folder with double end walls locked by tabs, dust flaps on the
    /// front and back walls and a lid with a tuck (mitcad#2226).
    #[serde(rename = "fefco_0421")]
    Fefco0421,
    /// The folder with a double front wall locked by tabs, corner flaps on
    /// the end walls and a lid with ears and a front flap.
    #[serde(rename = "fefco_0426")]
    Fefco0426,
    /// The 0421 with ears on its lid and a front flap with ears.
    #[serde(rename = "fefco_0427")]
    Fefco0427,
    /// The folder whose end walls hang on the back wall, with floor flaps
    /// and turned over with tabs into them, and a lid with a tuck.
    #[serde(rename = "fefco_0470")]
    Fefco0470,
    /// The 0470 with a front flap with ears on its lid.
    #[serde(rename = "fefco_0471")]
    Fefco0471,
}

impl BoxStyle {
    /// Whether the long panels' flaps lie one over the other across the
    /// whole end (FEFCO 0203, 0206): the second long panel's creases lie
    /// a further thickness and seam gap out.
    fn full_overlap(self) -> bool {
        matches!(self, BoxStyle::Fefco0203 | BoxStyle::Fefco0206)
    }

    /// Whether the style is a FEFCO 04 folder ([`folder`]).
    fn folder(self) -> bool {
        matches!(
            self,
            BoxStyle::Fefco0421
                | BoxStyle::Fefco0426
                | BoxStyle::Fefco0427
                | BoxStyle::Fefco0470
                | BoxStyle::Fefco0471
        )
    }

    /// Whether a folder's lid closes with a front flap as deep as the box
    /// (0426, 0427, 0471) rather than a tuck.
    fn front_flap(self) -> bool {
        matches!(
            self,
            BoxStyle::Fefco0426 | BoxStyle::Fefco0427 | BoxStyle::Fefco0471
        )
    }

    /// A folder's locking tabs on each turned over wall when not given.
    fn tabs(self) -> u32 {
        match self {
            BoxStyle::Fefco0426 => 3,
            BoxStyle::Fefco0470 | BoxStyle::Fefco0471 => 1,
            _ => 2,
        }
    }

    /// The name of the component a box of the style is made in.
    fn component_name(self) -> &'static str {
        match self {
            BoxStyle::Fefco0201 => "FEFCO 0201 Box",
            BoxStyle::EcmaA20 => "ECMA A20 Carton",
            BoxStyle::Fefco0203 => "FEFCO 0203 Box",
            BoxStyle::Fefco0206 => "FEFCO 0206 Box",
            BoxStyle::Fefco0421 => "FEFCO 0421 Box",
            BoxStyle::Fefco0426 => "FEFCO 0426 Box",
            BoxStyle::Fefco0427 => "FEFCO 0427 Box",
            BoxStyle::Fefco0470 => "FEFCO 0470 Box",
            BoxStyle::Fefco0471 => "FEFCO 0471 Box",
        }
    }
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
/// of the line's direction or not), and the angle it folds by, radians
/// (towards the box's inside, the blank's front).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlankFold {
    pub a: [f64; 2],
    pub b: [f64; 2],
    pub stationary_left: bool,
    pub angle: f64,
}

/// A crease folded square.
const QUARTER: f64 = std::f64::consts::FRAC_PI_2;

/// A box's blank: its outline (counter-clockwise), its holes (a folder's
/// slots) and its creases in the order they fold.
#[derive(Debug, Clone, PartialEq)]
pub struct Blank {
    pub outline: Vec<[f64; 2]>,
    pub holes: Vec<Vec<[f64; 2]>>,
    pub folds: Vec<BlankFold>,
}

/// The sizes a blank is worked out from, mm: inside length, width and
/// height, board thickness, crease radius and allowance (of a 90 degree
/// crease), glue flap and tuck, and a folder's locking tabs on each
/// turned over wall.
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
    pub tabs: u32,
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
/// short panels' flaps (of a full overlap box, 0203 and 0206, the second
/// long panel's a thickness further still, its flaps over the first's
/// across the whole end); the glue flap folds under the first panel's free
/// edge, touching it inside (the last panel's straight is short by the
/// thickness), and is a thickness in from the band's ends. Slots between
/// the flaps are a thickness wider than the creases' strips and reach half
/// a strip into the band, so that beside a crease's end no material of the
/// panel lies in its strip (each crease's ends lie on the outline, the
/// strip clear of the rest of the blank); a narrower flap on another (a
/// tuck) keeps its root's width over the strip before it narrows. A FEFCO
/// 04 folder's blank is [`folder::blank`]'s (mitcad#2226), which refuses
/// sizes that make none.
pub fn blank(style: BoxStyle, s: &BoxSizes) -> Result<Blank, String> {
    if style.folder() {
        return folder::blank(style, s);
    }
    Ok(slotted(style, s))
}

/// [`blank`] of a slotted box or tuck end carton.
fn slotted(style: BoxStyle, s: &BoxSizes) -> Blank {
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
    // Flaps that run across the whole end (the long panels' of a full
    // overlap box) stop a thickness short of the opposite wall, as meeting
    // flaps stop a thickness short of the middle.
    let across = w - t;
    let (bottom, top): (Vec<Stack>, Vec<Stack>) = match style {
        BoxStyle::Fefco0203 | BoxStyle::Fefco0206 => {
            // The short panels' flaps: the width deep (0203) or meeting at
            // the middle (0206); the 0203's no deeper than that either, so
            // that they do not overlap each other at the same level.
            let short = if style == BoxStyle::Fefco0203 {
                w.min(l / 2.0) - t
            } else {
                l / 2.0 - t
            };
            let ends: Vec<Stack> = (0..4)
                .map(|i| vec![(if i % 2 == 1 { across } else { short }, 0.0)])
                .collect();
            (ends.clone(), ends)
        }
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
        // 0201 (a folder style never comes here: [`blank`]).
        _ => (vec![vec![(flap, 0.0)]; 4], vec![vec![(flap, 0.0)]; 4]),
    };
    // Each panel's flaps lie at a level: the short panels' lowest, the long
    // panels' a thickness and a seam gap over them, and of a full overlap
    // box the second long panel's (the last) over the first's.
    let level = |i: usize| -> u32 {
        match (i % 2 == 1, style.full_overlap() && i == 3) {
            (false, _) => 0,
            (true, false) => 1,
            (true, true) => 2,
        }
    };
    let step = t + gap;
    let low = |i: usize| -f64::from(level(i)) * step;
    let high = |i: usize| band + f64::from(level(i)) * step;
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
    // The flaps' creases per stack (outermost first) with the level of its
    // panel's flaps: the lowest fold first (below).
    let mut stacks: Vec<(u32, Vec<BlankFold>)> = Vec::new();
    // The bottom, left to right: each panel's stack down its left side and
    // up its right side, the slots between.
    for (i, &(u, v, _)) in spans.iter().enumerate() {
        let y0 = low(i);
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
                level(i),
                creases
                    .into_iter()
                    .rev()
                    .map(|(a, b)| BlankFold {
                        a,
                        b,
                        stationary_left: true,
                        angle: QUARTER,
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
        let (u, v, _) = spans[i];
        let y0 = high(i);
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
                level(i),
                creases
                    .into_iter()
                    .rev()
                    .map(|(a, b)| BlankFold {
                        a,
                        b,
                        stationary_left: false,
                        angle: QUARTER,
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
    // The short panels' flaps fold first, then the long panels' level by
    // level: a fold refuses a seam of an earlier bend that lies past its
    // own near tangent (bends.rs, fold_panel; since mitcad#2226 only along
    // its line's extent), and each level's creases lie a thickness further
    // out, so a lower level's crease could find a higher level's seam past
    // it; the other way round each earlier seam lies behind. A stack folds
    // outermost first, while its creases still lie on the flat blank.
    let mut folds: Vec<BlankFold> = (0..=2)
        .flat_map(|at| stacks.iter().filter(move |(level, _)| *level == at))
        .flat_map(|(_, creases)| creases.iter().copied())
        .collect();
    folds.push(BlankFold {
        a: [x4, glue_low],
        b: [x4, glue_high],
        stationary_left: true,
        angle: QUARTER,
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
            angle: QUARTER,
        });
    }
    Blank {
        outline,
        holes: Vec::new(),
        folds,
    }
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

/// The key of polygons of lines `c<first>` on, each polygon's lines in its
/// order, the polygons one after another. A sketch region's key is its
/// outer polygon's alone (its holes come with it), so a Face names its
/// outline only.
fn polygon_key(first: u32, sizes: &[u32]) -> String {
    let mut keys: Vec<SegmentKey> = Vec::new();
    let mut start = first;
    for &n in sizes {
        let id = |i: u32| EntityUid(start + (i % n));
        keys.extend((0..n).map(|i| SegmentKey::between(id(i), id(i + n - 1), id(i + 1))));
        start += n;
    }
    RegionKey::new(keys).expect("a polygon").to_string()
}

/// A blank's outline and holes, one after the other.
fn loops(blank: &Blank) -> Vec<&Vec<[f64; 2]>> {
    std::iter::once(&blank.outline)
        .chain(&blank.holes)
        .collect()
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
    // A folder's front flap reaches down to a thickness above the base.
    let tuck = c.tuck.unwrap_or(if c.style.front_flap() {
        c.height - t
    } else {
        (c.height / 3.0).min(20.0)
    });
    if c.style.folder() && !(tuck > 0.0 && tuck <= c.height - r - seam_gap(t)) {
        return Err(ApiError(format!(
            "the lid's tuck must be deeper than 0 and end above the base: at most {:.1} mm, \
             got {tuck}",
            c.height - r - seam_gap(t)
        )));
    }
    let tabs = c.tabs.unwrap_or(c.style.tabs());
    if tabs > 12 {
        return Err(ApiError(format!(
            "a wall takes at most 12 locking tabs, got {tabs}"
        )));
    }
    let sizes = BoxSizes {
        length: c.length,
        width: c.width,
        height: c.height,
        thickness: t,
        radius: r,
        allowance,
        glue_flap: c.glue_flap,
        tuck,
        tabs,
    };
    let blank = blank(c.style, &sizes).map_err(ApiError)?;
    let mut rules = json!({"type": "sheet_rules", "process": "board", "thickness": t,
        "bend_radius": r, "allowance": {"type": "bend_deduction", "deduction": deduction},
        "reference_side": "front",
        "bend_relief": {"type": "round", "width": t, "depth": t},
        "corner_relief": {"type": "round", "width": t, "depth": t},
        "standard_box": options_json(c)});
    if let Some(flute) = &flute {
        rules["flute"] = json!(flute);
    }
    // The outline and its holes: points p1.., the outline's first, and
    // lines after them, each polygon's in its order.
    let polygons = loops(&blank);
    let n: usize = polygons.iter().map(|p| p.len()).sum();
    let mut outline: Vec<Value> = polygons
        .iter()
        .flat_map(|p| p.iter())
        .enumerate()
        .map(|(i, p)| json!({"id": format!("p{}", i + 1), "type": "point", "at": p}))
        .collect();
    let mut start = 0;
    for polygon in &polygons {
        let k = polygon.len();
        for i in 0..k {
            outline.push(
                json!({"id": format!("c{}", n + 1 + start + i), "type": "line",
                                 "start": format!("p{}", start + i + 1),
                                 "end": format!("p{}", start + (i + 1) % k + 1)}),
            );
        }
        start += k;
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

/// The key of a blank's region in its outline sketch ([`make_box`]'s
/// entities). It names the outer loop only: the holes inside it (a
/// folder's slots) come with the region (mitcad#2226: naming them made the
/// Face find its region by its outline, with a warning).
fn blank_region(blank: &Blank) -> String {
    let sizes: Vec<u32> = loops(blank).iter().map(|p| p.len() as u32).collect();
    polygon_key(sizes.iter().sum::<u32>() + 1, &sizes[..1])
}

/// The definition of a box's Fold at its blank's crease `k` (the line
/// `c<3k+3>` of the creases sketch), on the Face's body.
fn fold_def(face: &str, creases: &str, k: usize, fold: &BlankFold) -> Value {
    json!({"type": "sheet_fold", "body": format!("{face}.b0"),
        "panel": {"feature": face, "slot": 0},
        "line": {"sketch": creases, "curve": format!("c{}", 3 * k + 3)},
        "stationary_side": if fold.stationary_left { "left" } else { "right" },
        "angle": fold.angle, "position": "bend_line"})
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
        let name = c.style.component_name();
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
    let region = blank_region(&made.blank);
    let face = uid_of(
        &doc.run_command(json!({"cmd": "add_feature", "def": {"type": "sheet_face",
        "rules": rules, "profiles": [{"sketch": outline, "region": region}]}}))?,
    )?;
    let body = format!("{face}.b0");
    let mut folds = Vec::new();
    if c.folded {
        for (k, fold) in made.blank.folds.iter().enumerate() {
            let def = fold_def(&face, &creases, k, fold);
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
    #[serde(default, deserialize_with = "present")]
    tabs: Option<Option<Value>>,
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
            ("tabs", &self.tabs),
        ]
        .into_iter()
        .filter_map(|(name, v)| v.clone().map(|v| (name, v)))
        .collect()
    }
}

/// The features a standard box made, as [`edit_standard_box`] finds them
/// from its rules.
struct BoxFeatures {
    /// Its Face (of the rules, making a body of its own) and the Face's
    /// definition as `edit_feature` takes it.
    face: mitcad_model::ids::FeatureUid,
    face_def: Value,
    /// Its region's key in the outline sketch, as the Face names it.
    region: String,
    component: String,
    outline: mitcad_model::ids::FeatureUid,
    /// Its creases sketch: the one [`add_standard_box`] made after the
    /// outline (a flat box's too, mitcad#2301), else the sketch its first
    /// Fold's line is on; none when neither is left. Its Folds by their
    /// crease (`k` of the line `c<3k+3>`), in timeline order.
    creases: Option<mitcad_model::ids::FeatureUid>,
    folds: Vec<(usize, mitcad_model::ids::FeatureUid)>,
}

/// Whether a sketch is a box's creases as [`make_box`] makes them: on the
/// xy plane, for each crease `k` the points `p<3k+1>` and `p<3k+2>` and
/// the line `c<3k+3>` from the one to the other, nothing else (the
/// sketch's own axes aside).
fn is_creases_sketch(sketch: &mitcad_model::features::SketchDef) -> bool {
    use mitcad_model::features::SketchPlane;
    use mitcad_model::sketch::EntityKind;
    if !matches!(sketch.plane, SketchPlane::Xy) || sketch.plane_link.is_some() {
        return false;
    }
    let entities: Vec<_> = sketch
        .entities
        .iter()
        .filter(|e| e.axis.is_none())
        .collect();
    if entities.is_empty() || entities.len() % 3 != 0 {
        return false;
    }
    entities.iter().enumerate().all(|(i, e)| {
        let id = i as u32 + 1;
        e.id == EntityUid(id)
            && match &e.kind {
                EntityKind::Point { .. } => i % 3 != 2,
                EntityKind::Line { start, end, .. } => {
                    i % 3 == 2 && *start == EntityUid(id - 2) && *end == EntityUid(id - 1)
                }
                _ => false,
            }
    })
}

fn box_features<K: Kernel>(
    doc: &Document<K>,
    rules: mitcad_model::ids::FeatureUid,
) -> Result<BoxFeatures, ApiError> {
    use mitcad_model::features::FeatureDef;
    let mut found: Option<BoxFeatures> = None;
    // The sketches before the Face, in timeline order, with their
    // component and whether they are made as creases are: the box's
    // creases sketch is the first such after its outline.
    let mut sketches: Vec<(mitcad_model::ids::FeatureUid, String, bool)> = Vec::new();
    for entry in doc.features() {
        match (&entry.def, found.as_mut()) {
            (FeatureDef::Sketch(s), None) => {
                sketches.push((entry.uid, entry.component.to_string(), is_creases_sketch(s)));
            }
            (FeatureDef::SheetFace(f), None) if f.rules == rules && f.body.is_none() => {
                let Some(profile) = f.profiles.first() else {
                    continue;
                };
                let mut face_def = serde_json::to_value(f)
                    .map_err(|e| ApiError(format!("the box's Face: {e}")))?;
                face_def["type"] = json!("sheet_face");
                let component = entry.component.to_string();
                let creases = sketches
                    .iter()
                    .skip_while(|(uid, _, _)| *uid != profile.sketch)
                    .skip(1)
                    .find(|(_, of, creases)| *creases && *of == component)
                    .map(|(uid, _, _)| *uid);
                found = Some(BoxFeatures {
                    face: entry.uid,
                    face_def,
                    region: profile.region.to_string(),
                    component,
                    outline: profile.sketch,
                    creases,
                    folds: Vec::new(),
                });
            }
            (FeatureDef::SheetFold(f), Some(made)) if f.body.feature == made.face => {
                let creases = *made.creases.get_or_insert(f.line.sketch);
                // Its own Folds only: one the user added on another
                // sketch stays as it is.
                let k = f.line.curve.0;
                if f.line.sketch == creases && k >= 3 && k % 3 == 0 {
                    made.folds.push(((k / 3 - 1) as usize, entry.uid));
                }
            }
            _ => {}
        }
    }
    found.ok_or_else(|| ApiError("the box's Face is gone".into()))
}

/// `edit_standard_box`: the box's rules, its blank's outline and its
/// creases made again from its options with those given changed, as one
/// undo step "Edit Standard Box"; its Face and Folds, which name the same
/// curves, follow. Options that change the number of the blank's lines
/// within the style (another number of a folder's locking tabs,
/// mitcad#2285) make the sketches again with the new lines: the Face names
/// the new outline's region, a Fold whose crease the blank no longer has
/// goes and a new crease gets a Fold after the box's last; the box's later
/// features stay where their references survive. A flat box's creases
/// sketch follows the same way and the box stays flat (mitcad#2301).
/// Refused for rules of no standard box and for another style; nothing
/// changes when a step is refused.
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
    if options.get("style") != stored.get("style") {
        return Err(ApiError(
            "another style changes the shape of the box's blank: make the box again".into(),
        ));
    }
    let command: AddStandardBoxCommand = serde_json::from_value(Value::Object(options))
        .map_err(|e| ApiError(format!("the box's options: {e}")))?;
    let made = make_box(&command)?;
    let found = box_features(doc, c.rules)?;
    let depth = doc.undo_depth();
    let edited = remake_box(doc, &c, &made, &found);
    if edited.is_err() {
        // What was changed before the refusal goes again.
        if doc.undo_depth() > depth {
            doc.merge_undo(depth, "Edit Standard Box");
            doc.undo();
        }
        return edited;
    }
    doc.merge_undo(depth, "Edit Standard Box");
    edited
}

/// [`edit_standard_box`]'s changes to the box's features: its rules, its
/// Face's region when the outline's lines are others, its outline and
/// creases sketches and its Folds.
fn remake_box<K: Kernel>(
    doc: &mut Document<K>,
    c: &EditStandardBoxCommand,
    made: &BoxMade,
    found: &BoxFeatures,
) -> Result<Value, ApiError> {
    doc.run_command(json!({"cmd": "edit_feature", "uid": c.rules, "def": made.rules}))?;
    // The Face names the new outline's region (its key names the lines by
    // their uids, which shift with their number) before the outline
    // changes, so that the outline's edit builds the Face and its Folds
    // once, on the new region.
    let region = blank_region(&made.blank);
    if region != found.region {
        let mut def = found.face_def.clone();
        def["profiles"][0]["region"] = json!(region);
        // The renamed region keeps its panel slot (0): its body stays
        // `<face>.b0`, which the Folds and the Flat Pattern name. Without
        // it the new key would get a new slot and body, the old slot left
        // as a tombstone (SheetFaceDef::settle).
        if let Some(slots) = def["panel_slots"].as_array_mut() {
            for slot in slots {
                if slot["profile"]["region"] == json!(found.region) {
                    slot["profile"]["region"] = json!(region);
                }
            }
        }
        doc.run_command(json!({"cmd": "edit_feature", "uid": found.face, "def": def}))?;
    }
    doc.run_command(json!({"cmd": "edit_feature", "uid": found.outline,
        "def": {"type": "sketch", "plane": "xy", "entities": made.outline}}))?;
    let face = found.face.to_string();
    let mut folds: Vec<Value> = Vec::new();
    if let Some(creases) = found.creases {
        let creases_name = creases.to_string();
        let crease_count = made.blank.folds.len();
        // A flat box (no Folds of its own) keeps having none: only its
        // creases sketch follows (mitcad#2301).
        let same = found.folds.is_empty()
            || (found.folds.len() == crease_count
                && found.folds.iter().all(|&(k, _)| k < crease_count));
        // The Folds of creases the blank no longer has go first, the last
        // first; one a later feature uses refuses the edit in words.
        if !same {
            for &(k, uid) in found.folds.iter().rev() {
                if k >= crease_count {
                    doc.run_command(json!({"cmd": "delete_feature", "uid": uid}))
                        .map_err(|e| {
                            ApiError(format!(
                                "the new blank has no crease for the box's Fold {uid}: {e}"
                            ))
                        })?;
                }
            }
        }
        doc.run_command(json!({"cmd": "edit_feature", "uid": creases,
            "def": {"type": "sketch", "plane": "xy", "entities": made.creases}}))?;
        if !same {
            // Each crease's Fold made again from the blank (its side and
            // angle may be others now), a new crease's after the box's last
            // Fold kept (after the Face when none is).
            let mut last = found
                .folds
                .iter()
                .filter(|&&(k, _)| k < crease_count)
                .map(|&(_, uid)| uid)
                .next_back()
                .unwrap_or(found.face)
                .to_string();
            for (k, fold) in made.blank.folds.iter().enumerate() {
                let def = fold_def(&face, &creases_name, k, fold);
                match found.folds.iter().find(|&&(at, _)| at == k) {
                    Some(&(_, uid)) => {
                        doc.run_command(json!({"cmd": "edit_feature", "uid": uid, "def": def}))?;
                        folds.push(json!(uid));
                    }
                    None => {
                        let uid = uid_of(&doc.run_command(json!({"cmd": "add_feature",
                            "def": def, "component": found.component, "after": last}))?)?;
                        folds.push(json!(uid));
                        last = uid;
                    }
                }
            }
        } else {
            folds = found.folds.iter().map(|&(_, uid)| json!(uid)).collect();
        }
    }
    Ok(
        json!({"rules": c.rules, "outline": found.outline, "creases": found.creases,
              "face": found.face, "folds": folds}),
    )
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
            tabs: 2,
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
        let b = blank(BoxStyle::Fefco0201, &sizes()).unwrap();
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
        let b = blank(BoxStyle::EcmaA20, &sizes()).unwrap();
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

    /// Whether `p` lies on an edge of the closed outline.
    fn on_outline(outline: &[[f64; 2]], p: [f64; 2]) -> bool {
        let n = outline.len();
        (0..n).any(|i| {
            let (a, c) = (outline[i], outline[(i + 1) % n]);
            let cross = (c[0] - a[0]) * (p[1] - a[1]) - (c[1] - a[1]) * (p[0] - a[0]);
            let within =
                (p[0] - a[0]) * (p[0] - c[0]) <= 1e-9 && (p[1] - a[1]) * (p[1] - c[1]) <= 1e-9;
            cross.abs() < 1e-6 && within
        })
    }

    /// Whether no two edges of the closed outline but neighbours meet.
    fn simple(outline: &[[f64; 2]]) -> bool {
        let n = outline.len();
        let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
            (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
        };
        let meet = |p: [f64; 2], q: [f64; 2], r: [f64; 2], s: [f64; 2]| {
            let (d1, d2) = (cross(r, s, p), cross(r, s, q));
            let (d3, d4) = (cross(p, q, r), cross(p, q, s));
            let box_overlap = p[0].min(q[0]) <= r[0].max(s[0]) + 1e-9
                && r[0].min(s[0]) <= p[0].max(q[0]) + 1e-9
                && p[1].min(q[1]) <= r[1].max(s[1]) + 1e-9
                && r[1].min(s[1]) <= p[1].max(q[1]) + 1e-9;
            box_overlap && d1 * d2 <= 1e-12 && d3 * d4 <= 1e-12
        };
        (0..n).all(|i| {
            (i + 2..n).all(|j| {
                (i == 0 && j == n - 1)
                    || !meet(
                        outline[i],
                        outline[(i + 1) % n],
                        outline[j],
                        outline[(j + 1) % n],
                    )
            })
        })
    }

    /// The outline's bounds: (min, max).
    fn bounds(outline: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
        outline.iter().fold(
            ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]),
            |(lo, hi), p| {
                (
                    [lo[0].min(p[0]), lo[1].min(p[1])],
                    [hi[0].max(p[0]), hi[1].max(p[1])],
                )
            },
        )
    }

    /// A FEFCO 0203 blank (mitcad#2226): every flap the width deep less
    /// the thickness (the short panels' no deeper than half the length
    /// less it), the second long panel's flaps a level (thickness and seam
    /// gap) over the first's; the levels fold from the lowest.
    #[test]
    fn a_full_overlap_blank_lays_its_long_flaps_one_over_the_other() {
        let s = BoxSizes {
            length: 500.0,
            ..sizes()
        };
        let b = blank(BoxStyle::Fefco0203, &s).unwrap();
        assert!(area(&b.outline) > 0.0, "counter-clockwise");
        assert!(simple(&b.outline), "{:?}", b.outline);
        assert_eq!(b.folds.len(), 8 + 1 + 3);
        for fold in &b.folds[..9] {
            assert!(
                on_outline(&b.outline, fold.a) && on_outline(&b.outline, fold.b),
                "{fold:?}"
            );
            assert_eq!(fold.angle, QUARTER);
        }
        // The panels: x1 = W + t - r + BA/2 = 206, x2 = x1 + L - 2r + BA
        // = 710, x3 = 914, the glue crease 1413.6 and the glue flap to
        // 1446.1; the band 150 - 1 + 5 = 154 high.
        let walls: Vec<f64> = b.folds[9..].iter().map(|f| f.a[0]).collect();
        for (x, want) in walls.iter().zip([914.0, 710.0, 206.0]) {
            assert!((x - want).abs() < 1e-9, "{walls:?}");
        }
        assert!((b.folds[8].a[0] - 1413.6).abs() < 1e-9, "{:?}", b.folds[8]);
        // Levels: the short panels' creases at 0 and 154, the first long
        // panel's 4.4 out, the second's 8.8; folded in that order.
        let out = |f: &BlankFold| {
            let y = f.a[1];
            if y < 77.0 { -y } else { y - 154.0 }
        };
        let levels: Vec<f64> = b.folds[..8].iter().map(out).collect();
        for (k, want) in [0.0, 0.0, 0.0, 0.0, 4.4, 4.4, 8.8, 8.8].iter().enumerate() {
            assert!((levels[k] - want).abs() < 1e-9, "{levels:?}");
        }
        // Every flap 196 deep: the blank from y = -8.8 - 196 to 154 + 8.8
        // + 196.
        let (lo, hi) = bounds(&b.outline);
        assert!((lo[1] + 204.8).abs() < 1e-9, "{lo:?}");
        assert!((hi[1] - 358.8).abs() < 1e-9, "{hi:?}");
        assert!(
            (hi[0] - 1446.1).abs() < 1e-9 && lo[0] == 0.0,
            "{lo:?} {hi:?}"
        );
        // A short box: the short panels' flaps half the length less the
        // thickness deep, so that they do not overlap.
        let short = blank(BoxStyle::Fefco0203, &sizes()).unwrap();
        let deepest_short = short
            .outline
            .iter()
            .filter(|p| p[0] > 1.0 && p[0] < 206.0)
            .map(|p| p[1])
            .fold(f64::INFINITY, f64::min);
        assert!((deepest_short + 146.0).abs() < 1e-9, "{deepest_short}");
    }

    /// A FEFCO 0206 blank (mitcad#2226): the short panels' flaps half the
    /// length deep less the thickness (meeting at the middle), the long
    /// panels' the width less it at two levels over them.
    #[test]
    fn a_0206_blank_meets_its_short_flaps_and_overlaps_its_long_ones() {
        let s = BoxSizes {
            length: 400.0,
            ..sizes()
        };
        let b = blank(BoxStyle::Fefco0206, &s).unwrap();
        assert!(area(&b.outline) > 0.0, "counter-clockwise");
        assert!(simple(&b.outline), "{:?}", b.outline);
        assert_eq!(b.folds.len(), 8 + 1 + 3);
        for fold in &b.folds[..9] {
            assert!(
                on_outline(&b.outline, fold.a) && on_outline(&b.outline, fold.b),
                "{fold:?}"
            );
        }
        // The first short panel (x from 0 to 206): its flaps 400 / 2 - 4
        // = 196 deep below y = 0; the long panels' 196 below their
        // creases at -4.4 and -8.8.
        let below = |x0: f64, x1: f64| {
            b.outline
                .iter()
                .filter(|p| p[0] > x0 && p[0] < x1)
                .map(|p| p[1])
                .fold(f64::INFINITY, f64::min)
        };
        assert!((below(1.0, 206.0) + 196.0).abs() < 1e-9);
        // x2 = 206 + 400 - 1 + 5 = 610, x3 = 814, x4 = 1213.6.
        assert!((below(210.0, 606.0) + 200.4).abs() < 1e-9);
        assert!((below(818.0, 1210.0) + 204.8).abs() < 1e-9);
    }

    #[test]
    fn the_region_key_names_the_polygon_lines() {
        assert_eq!(
            polygon_key(5, &[4]),
            "r{c5[c8,c6],c6[c5,c7],c7[c6,c8],c8[c7,c5]}"
        );
        // An outline with a hole: each loop's lines between their own.
        let key = polygon_key(9, &[4, 4]);
        for segment in ["c9[c12,c10]", "c12[c11,c9]", "c13[c16,c14]", "c16[c15,c13]"] {
            assert!(key.contains(segment), "{key}");
        }
    }
}
