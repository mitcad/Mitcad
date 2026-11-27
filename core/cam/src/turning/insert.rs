// SPDX-License-Identifier: MIT
//! A turning tool's insert and holder (mitcad#2502, GitHub mitcad/Mitcad#705):
//! the ISO codes (ISO 1832 inserts, ISO 5608 holders, ISO 6261 boring
//! bars), the angles typed over them, the tool's outline and its back
//! angle.
//!
//! In the lathe's plane (z right, x up) a right-hand outside tool cuts
//! towards the chuck (-z), its holder lying towards +z and +x. The insert
//! has two cutting edges at the nose: the side edge, facing the chuck, at
//! the holder's approach angle κr to the feed direction (95° for an L
//! style holder: the edge leans 5° back), and the trailing edge, running
//! back towards +z. The trailing edge leaves the z axis at the **back
//! angle** 180° − κr − ε, ε being the insert's tip angle: 5° for a CNMG in
//! a PCLNR, 52° for a VBMT in an SVJBR (93°). It is the steepest wall
//! falling towards the chuck that the tool can follow down without its
//! trailing edge cutting into the wall behind it.
//!
//! A boring bar is the same seen in a mirror across the axis (its holder
//! lies towards smaller x), a left-hand tool in a mirror across x.
//!
//! What the user typed over the codes ([`TypedInsert`]) is kept beside the
//! values worked out, so that a tool opened again shows the same fields and
//! a field cleared goes back to the code's value.

use serde::{Deserialize, Serialize};

use super::PXZ;

/// Which way a tool's holder lies along the spindle: right-hand (the holder
/// towards +z, cutting towards the chuck), left-hand, or neutral (a
/// symmetric nose pointing at the axis).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hand {
    #[default]
    Right,
    Left,
    Neutral,
}

impl Hand {
    fn from_letter(letter: char) -> Option<Hand> {
        match letter {
            'R' => Some(Hand::Right),
            'L' => Some(Hand::Left),
            'N' => Some(Hand::Neutral),
            _ => None,
        }
    }

    pub fn words(self) -> &'static str {
        match self {
            Hand::Right => "right hand",
            Hand::Left => "left hand",
            Hand::Neutral => "neutral",
        }
    }
}

/// What the user typed for a tool's insert over its codes' values; none:
/// the code's value (or the default without a code). Kept with the tool so
/// that editing it again starts from the same fields.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TypedInsert {
    /// The insert's tip angle (degrees).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tip_angle: Option<f64>,
    /// The holder's approach angle (degrees).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approach_angle: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hand: Option<Hand>,
    /// The tool's nose radius as typed (mm; the tool's shape has it too).
    /// None: the insert code's corner radius, which then follows the code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nose_radius: Option<f64>,
}

impl TypedInsert {
    pub fn is_empty(&self) -> bool {
        self.tip_angle.is_none()
            && self.approach_angle.is_none()
            && self.hand.is_none()
            && self.nose_radius.is_none()
    }
}

/// The geometry of a turning tool or boring bar: its insert's tip angle,
/// its holder's approach angle and hand, the codes they came from and what
/// was typed over them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InsertGeometry {
    /// The insert's ISO code as read (`CNMG 120408`); empty when there is
    /// none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub insert: String,
    /// The holder's or boring bar's ISO code (`PCLNR 2525 M12`,
    /// `S25S-PCLNR 12`); empty when there is none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub holder: String,
    /// The insert's tip angle ε (degrees): 80 for C, 55 for D, 35 for V.
    pub tip_angle: f64,
    /// The holder's approach angle κr (degrees), from the feed direction
    /// to the side cutting edge: 95 for an L style, 93 for J.
    pub approach_angle: f64,
    #[serde(default)]
    pub hand: Hand,
    /// The insert's cutting edge length (mm), for the picture.
    #[serde(default = "default_edge")]
    pub edge_length: f64,
    /// What was typed over the codes.
    #[serde(default, skip_serializing_if = "TypedInsert::is_empty")]
    pub typed: TypedInsert,
}

fn default_edge() -> f64 {
    12.0
}

/// An insert and a holder read: the geometry with the codes as read.
#[derive(Clone, Debug, PartialEq)]
pub struct ReadInsert {
    pub geometry: InsertGeometry,
    pub insert: Option<InsertCode>,
    pub holder: Option<HolderCode>,
}

impl ReadInsert {
    /// The insert code's corner radius (mm), when there is an insert code.
    pub fn code_nose_radius(&self) -> Option<f64> {
        self.insert.as_ref().map(|i| i.nose_radius)
    }
}

impl InsertGeometry {
    /// The angle from the z axis at which the trailing edge leaves the nose
    /// (degrees).
    pub fn back_angle(&self) -> f64 {
        180.0 - self.approach_angle - self.tip_angle
    }

    pub fn validate(&self) -> Result<(), String> {
        let (e, k) = (self.tip_angle, self.approach_angle);
        if !(e.is_finite() && e > 0.0 && e <= 90.0) {
            return Err(
                "The insert's tip angle must be above 0 and at most 90 degrees.".to_owned(),
            );
        }
        if !(k.is_finite() && k > 0.0 && k < 180.0) {
            return Err(
                "The holder's approach angle must be between 0 and 180 degrees.".to_owned(),
            );
        }
        if self.back_angle() < 0.0 {
            return Err(format!(
                "An insert of {} degrees does not fit a holder of {} degrees: its trailing edge would turn into the work.",
                num(e),
                num(k)
            ));
        }
        if !(self.edge_length.is_finite() && self.edge_length > 0.0) {
            return Err("The insert's edge length must be above zero.".to_owned());
        }
        if self
            .typed
            .nose_radius
            .is_some_and(|r| !(r.is_finite() && r >= 0.0))
        {
            return Err("The nose radius must be zero or more.".to_owned());
        }
        Ok(())
    }

    /// The geometry from an insert code and a holder code, either of them
    /// empty, and what was typed over them: a typed angle wins over the
    /// code's. Without a holder code the approach angle is 95 degrees (a
    /// neutral tool's 90 less half the tip angle), without an insert code
    /// the tip angle is the holder's insert shape's (or 80 degrees). A
    /// holder that takes another insert shape than the insert's, and a hand
    /// that differs from the holder code's, are refused.
    pub fn read(insert: &str, holder: &str, typed: TypedInsert) -> Result<ReadInsert, String> {
        let insert_code = if insert.trim().is_empty() {
            None
        } else {
            Some(parse_insert(insert)?)
        };
        let holder_code = if holder.trim().is_empty() {
            None
        } else {
            Some(parse_holder(holder)?)
        };
        if let (Some(i), Some(h)) = (&insert_code, &holder_code)
            && i.shape != h.shape
        {
            return Err(format!(
                "The holder {} takes {} inserts ({}), not {} ({}).",
                h.text,
                h.shape,
                shape_row(h.shape).map_or("of another shape", |s| s.1),
                i.shape,
                i.shape_name
            ));
        }
        if let (Some(h), Some(hand)) = (&holder_code, typed.hand)
            && h.hand != hand
        {
            return Err(format!(
                "The holder {} is {}, not {}.",
                h.text,
                h.hand.words(),
                hand.words()
            ));
        }
        let shape_tip = holder_code
            .as_ref()
            .and_then(|h| shape_row(h.shape))
            .map(|s| s.2);
        let tip_angle = typed
            .tip_angle
            .or(insert_code.as_ref().map(|i| i.tip_angle))
            .or(shape_tip)
            .unwrap_or(80.0);
        let hand = typed
            .hand
            .or(holder_code.as_ref().map(|h| h.hand))
            .unwrap_or_default();
        // A neutral tool points straight at the axis: its two cutting edges
        // lean equally, at 90° less half the tip angle.
        let approach_angle = typed
            .approach_angle
            .or(holder_code.as_ref().map(|h| h.approach_angle))
            .unwrap_or(if hand == Hand::Neutral {
                90.0 - tip_angle / 2.0
            } else {
                95.0
            });
        let geometry = InsertGeometry {
            insert: insert_code
                .as_ref()
                .map(|i| i.text.clone())
                .unwrap_or_default(),
            holder: holder_code
                .as_ref()
                .map(|h| h.text.clone())
                .unwrap_or_default(),
            tip_angle,
            approach_angle,
            hand,
            edge_length: insert_code
                .as_ref()
                .map_or(default_edge(), |i| i.edge_length),
            typed,
        };
        geometry.validate()?;
        Ok(ReadInsert {
            geometry,
            insert: insert_code,
            holder: holder_code,
        })
    }

    /// The outline of the insert and of its holder, `[x, z]` from the
    /// tool's controlled point (the imaginary tip of a nose of
    /// `nose_radius`), for a picture. `inside` for a boring bar (drawn on
    /// the bore's side, below the axis); `size` the holder's shank width or
    /// the bar's diameter. A left-hand tool lies towards -z.
    pub fn outline(&self, nose_radius: f64, inside: bool, size: f64) -> ToolOutline {
        let r = if nose_radius.is_finite() {
            nose_radius.max(0.0)
        } else {
            0.0
        };
        let (eps, kappa) = (
            self.tip_angle.to_radians(),
            self.approach_angle.to_radians(),
        );
        let len = self.edge_length;
        // In (z, x), the sharp corner at the origin: the side edge leaves
        // at 180° − κr from +z, the trailing edge at the back angle.
        let side = unit(std::f64::consts::PI - kappa);
        let trail = unit(std::f64::consts::PI - kappa - eps);
        // A triangle (T) has three corners, a rhombus (C, D, E, M, V) or a
        // square (S) four.
        let mut corners = vec![(0.0, 0.0), scale(trail, len)];
        if (self.tip_angle - 60.0).abs() < 1e-9 {
            corners.push(scale(side, len));
        } else {
            corners.push(add(scale(trail, len), scale(side, len)));
            corners.push(scale(side, len));
        }
        let (insert, centre) = round_corners(&corners, r);
        // The controlled point: the nose's centre less its radius in z and x.
        let tip = (centre.0 - r, centre.1 - r);
        let insert: Vec<(f64, f64)> = insert.iter().map(|p| (p.0 - tip.0, p.1 - tip.1)).collect();
        let top = insert.iter().fold(0.0f64, |m, p| m.max(p.1));
        let size = if size.is_finite() && size > 0.0 {
            size
        } else {
            20.0
        };
        // The holder stays behind both cutting edges' lines, by a clearance:
        // its underside leaves the trailing corner at the back angle and a
        // little more, its front follows the side edge leaning back a little
        // more, so that the insert, not the holder, meets a wall first.
        let clearance = 2f64.to_radians();
        let at = |c: (f64, f64)| (c.0 - tip.0, c.1 - tip.1);
        let trail_corner = at(corners[1]);
        let side_corner = at(corners[corners.len() - 1]);
        let under = unit(std::f64::consts::PI - kappa - eps + clearance);
        let front = unit(std::f64::consts::PI - kappa - clearance);
        // Up the front from the side corner to height `x` (not past it).
        let up_front = |x: f64| {
            let t = ((x - side_corner.1) / front.1.max(1e-9)).max(0.0);
            add(side_corner, scale(front, t))
        };
        // Along a ray from a point until it reaches `z`, or height `high`
        // when it would pass that first (or never reaches `z`).
        let ray = |from: (f64, f64), dir: (f64, f64), z: f64, high: f64| {
            if dir.0.abs() > 1e-9 {
                let t = (z - from.0) / dir.0;
                let p = add(from, scale(dir, t));
                if t >= 0.0 && p.1 <= high {
                    return p;
                }
            }
            let t = ((high - from.1) / dir.1.max(1e-9)).max(0.0);
            add(from, scale(dir, t))
        };
        let holder: Vec<(f64, f64)> = if inside {
            // A bar along the axis away from the chuck. Only the nose stands
            // out of it, by about a fifth of its diameter: the bar's side
            // meets the trailing edge there, or runs level with the
            // trailing corner when that is nearer.
            let low = trail_corner.1.min(0.18 * size);
            let nose_corner = at(corners[0]);
            let meet = if low < trail_corner.1 && trail.1 > 1e-9 {
                add(nose_corner, scale(trail, (low - nose_corner.1) / trail.1))
            } else {
                trail_corner
            };
            let high = (low + size).max(side_corner.1 + 0.2 * size);
            let b = up_front(high.min(side_corner.1 + 0.5 * size));
            let end = b.0.max(trail_corner.0) + 3.0 * size;
            vec![meet, (end, low), (end, high), (b.0, high), b, side_corner]
        } else {
            // A shank running up, away from the axis; a neutral tool's
            // centred over the nose.
            let high = top + 1.2 * size;
            let (left_aim, b) = if self.hand == Hand::Neutral {
                let left = (-size / 2.0).min(side_corner.0 - 1.0);
                (left, ray(side_corner, front, left, high))
            } else {
                let b = up_front(side_corner.1 + 0.5 * size);
                (b.0, b)
            };
            let left = left_aim.min(b.0);
            let right_aim = if self.hand == Hand::Neutral {
                (size / 2.0).max(trail_corner.0 + 1.0)
            } else {
                (left + size).max(trail_corner.0 + 1.0)
            };
            let a = ray(trail_corner, under, right_aim, high);
            let right = right_aim.max(a.0);
            let mut points = vec![trail_corner, a];
            for corner in [(right, high), (left, high)] {
                let last = points[points.len() - 1];
                if (corner.0 - last.0).abs() > 1e-9 || (corner.1 - last.1).abs() > 1e-9 {
                    points.push(corner);
                }
            }
            if (b.0 - left).abs() > 1e-9 || (b.1 - high).abs() > 1e-9 {
                points.push(b);
            }
            points.push(side_corner);
            points
        };
        let zs = if self.hand == Hand::Left { -1.0 } else { 1.0 };
        let xs = if inside { -1.0 } else { 1.0 };
        let put = |p: &(f64, f64)| [xs * p.1, zs * p.0];
        ToolOutline {
            insert: insert.iter().map(put).collect(),
            holder: holder.iter().map(put).collect(),
        }
    }
}

/// A tool's outline for a picture: closed polygons, `[x, z]` from the
/// controlled point.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ToolOutline {
    pub insert: Vec<PXZ>,
    pub holder: Vec<PXZ>,
}

fn unit(angle: f64) -> (f64, f64) {
    (angle.cos(), angle.sin())
}

fn scale(v: (f64, f64), s: f64) -> (f64, f64) {
    (v.0 * s, v.1 * s)
}

fn add(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 + b.0, a.1 + b.1)
}

/// A counter-clockwise polygon with its corners rounded by `r`, as points;
/// with the centre of the first corner's arc.
fn round_corners(corners: &[(f64, f64)], r: f64) -> (Vec<(f64, f64)>, (f64, f64)) {
    let n = corners.len();
    let mut out = Vec::new();
    let mut first_centre = corners[0];
    for i in 0..n {
        let v = corners[i];
        let p = corners[(i + n - 1) % n];
        let q = corners[(i + 1) % n];
        let a = norm((p.0 - v.0, p.1 - v.1));
        let b = norm((q.0 - v.0, q.1 - v.1));
        let theta = (a.0 * b.0 + a.1 * b.1).clamp(-1.0, 1.0).acos();
        if r <= 0.0 || theta <= 1e-9 {
            out.push(v);
            continue;
        }
        let t = r / (theta / 2.0).tan();
        let start = add(v, scale(a, t));
        let end = add(v, scale(b, t));
        let bis = norm(add(a, b));
        let centre = add(v, scale(bis, r / (theta / 2.0).sin()));
        if i == 0 {
            first_centre = centre;
        }
        let a0 = (start.1 - centre.1).atan2(start.0 - centre.0);
        let mut a1 = (end.1 - centre.1).atan2(end.0 - centre.0);
        while a1 < a0 {
            a1 += std::f64::consts::TAU;
        }
        let steps = 12;
        for s in 0..=steps {
            let ang = a0 + (a1 - a0) * f64::from(s) / f64::from(steps);
            out.push((centre.0 + r * ang.cos(), centre.1 + r * ang.sin()));
        }
    }
    (out, first_centre)
}

fn norm(v: (f64, f64)) -> (f64, f64) {
    let l = v.0.hypot(v.1);
    (v.0 / l, v.1 / l)
}

fn num(v: f64) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

// ISO 1832 inserts.

/// The insert shapes: letter, name, tip angle, edge length per inscribed
/// circle.
const SHAPES: [(char, &str, f64, f64); 7] = [
    ('S', "square", 90.0, 1.0),
    ('T', "triangle", 60.0, 1.732_051),
    ('C', "rhombic 80", 80.0, 1.015_427),
    ('D', "rhombic 55", 55.0, 1.220_775),
    ('E', "rhombic 75", 75.0, 1.035_276),
    ('M', "rhombic 86", 86.0, 1.002_442),
    ('V', "rhombic 35", 35.0, 1.743_447),
];

/// Shapes the codes know but the tool's outline does not draw yet.
const OTHER_SHAPES: &str = "HOPLRWABK";

/// The standard inscribed circles (mm).
const CIRCLES: [f64; 18] = [
    3.97, 5.56, 6.35, 7.94, 9.525, 12.7, 15.875, 19.05, 25.4, 31.75, 6.0, 8.0, 10.0, 12.0, 16.0,
    20.0, 25.0, 32.0,
];

const CLEARANCES: [(char, f64); 9] = [
    ('A', 3.0),
    ('B', 5.0),
    ('C', 7.0),
    ('D', 15.0),
    ('E', 20.0),
    ('F', 25.0),
    ('G', 30.0),
    ('N', 0.0),
    ('P', 11.0),
];

const THICKNESSES: [(&str, f64); 11] = [
    ("01", 1.59),
    ("T1", 1.98),
    ("02", 2.38),
    ("T2", 2.78),
    ("03", 3.18),
    ("T3", 3.97),
    ("04", 4.76),
    ("05", 5.56),
    ("06", 6.35),
    ("07", 7.94),
    ("09", 9.52),
];

const RADII: [(&str, f64); 10] = [
    ("00", 0.0),
    ("02", 0.2),
    ("04", 0.4),
    ("08", 0.8),
    ("12", 1.2),
    ("16", 1.6),
    ("20", 2.0),
    ("24", 2.4),
    ("28", 2.8),
    ("32", 3.2),
];

fn shape_row(letter: char) -> Option<(char, &'static str, f64, f64)> {
    SHAPES.iter().copied().find(|s| s.0 == letter)
}

/// An ISO insert code read.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InsertCode {
    /// Normalised: `CNMG 120408`.
    pub text: String,
    pub shape: char,
    pub shape_name: &'static str,
    pub tip_angle: f64,
    /// The clearance angle (degrees), when the letter is a standard one.
    pub clearance: Option<f64>,
    pub edge_length: f64,
    pub inscribed_circle: f64,
    pub thickness: f64,
    pub nose_radius: f64,
}

/// Reads an ISO insert code: shape, clearance, tolerance and type letters,
/// then size, thickness and corner radius (`CNMG 120408`); a hand and a
/// maker's chip breaker after it are allowed and ignored.
pub fn parse_insert(text: &str) -> Result<InsertCode, String> {
    let chars: Vec<char> = text
        .to_uppercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let shown = text.trim();
    if chars.len() < 10 {
        return Err(format!(
            "The insert code {shown} is too short: an insert code looks like CNMG 120408."
        ));
    }
    if !chars[..4].iter().all(char::is_ascii_alphabetic) {
        return Err(format!(
            "The insert code {shown} must start with four letters: an insert code looks like CNMG 120408."
        ));
    }
    let shape = chars[0];
    let Some((_, shape_name, tip_angle, factor)) = shape_row(shape) else {
        return Err(if OTHER_SHAPES.contains(shape) {
            format!("Insert shape {shape} is not supported yet (S, T, C, D, E, M and V are).")
        } else {
            format!("{shape} is not an insert shape letter.")
        });
    };
    let clearance = CLEARANCES.iter().find(|c| c.0 == chars[1]).map(|c| c.1);
    let size: String = chars[4..6].iter().collect();
    let thick: String = chars[6..8].iter().collect();
    let radius: String = chars[8..10].iter().collect();
    let Ok(size_mm) = size.parse::<u32>() else {
        return Err(format!(
            "{size} is not an insert size code (the edge length in mm, two digits)."
        ));
    };
    let Some((edge_length, inscribed_circle)) = CIRCLES
        .iter()
        .map(|c| (c * factor, *c))
        .find(|(l, _)| (l + 0.05).floor() as u32 == size_mm)
    else {
        return Err(format!(
            "Size {size} does not fit a {shape} insert with a standard inscribed circle."
        ));
    };
    let Some(thickness) = THICKNESSES.iter().find(|t| t.0 == thick).map(|t| t.1) else {
        return Err(format!("{thick} is not an insert thickness code."));
    };
    let Some(nose_radius) = RADII.iter().find(|t| t.0 == radius).map(|t| t.1) else {
        return Err(format!(
            "{radius} is not a corner radius code (00, 02, 04, 08, 12 and so on)."
        ));
    };
    let letters: String = chars[..4].iter().collect();
    Ok(InsertCode {
        text: format!("{letters} {size}{thick}{radius}"),
        shape,
        shape_name,
        tip_angle,
        clearance,
        edge_length,
        inscribed_circle,
        thickness,
        nose_radius,
    })
}

// ISO 5608 holders and ISO 6261 boring bars.

/// The holder styles: letter and approach angle κr (degrees).
const STYLES: [(char, f64); 21] = [
    ('A', 90.0),
    ('B', 75.0),
    ('C', 90.0),
    ('D', 45.0),
    ('E', 60.0),
    ('F', 90.0),
    ('G', 90.0),
    ('H', 107.5),
    ('J', 93.0),
    ('K', 75.0),
    ('L', 95.0),
    ('M', 50.0),
    ('N', 63.0),
    ('P', 117.5),
    ('Q', 107.5),
    ('R', 75.0),
    ('S', 45.0),
    ('T', 60.0),
    ('U', 93.0),
    ('V', 72.5),
    ('Y', 85.0),
];

/// A holder or boring bar code read.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HolderCode {
    /// Normalised: `PCLNR 2525 M12`, `S25S-PCLNR 12`.
    pub text: String,
    /// Whether it is a boring bar.
    pub bar: bool,
    pub clamping: char,
    /// The insert shape it takes.
    pub shape: char,
    pub style: char,
    pub approach_angle: f64,
    pub hand: Hand,
    /// The shank's width or the bar's diameter (mm), when the code has it.
    pub size: Option<f64>,
}

/// Reads a holder code (`PCLNR`, `PCLNR 2525 M12`) or a boring bar code
/// (`S25S-PCLNR 12`): clamping, insert shape, style, clearance and hand
/// letters, with the sizes optional.
pub fn parse_holder(text: &str) -> Result<HolderCode, String> {
    let upper = text.trim().to_uppercase();
    let compact: Vec<char> = upper
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    // A boring bar: a type letter, the diameter, a length letter, then the
    // holder letters.
    let bar = compact.len() >= 9
        && compact[0].is_ascii_alphabetic()
        && compact[1].is_ascii_digit()
        && compact[2].is_ascii_digit()
        && compact[3].is_ascii_alphabetic();
    let (letters, size) = if bar {
        let diameter: String = compact[1..3].iter().collect();
        (&compact[4..], diameter.parse::<f64>().ok())
    } else {
        let size = if compact.len() >= 9 && compact[5..9].iter().all(char::is_ascii_digit) {
            let w: String = compact[7..9].iter().collect();
            w.parse::<f64>().ok()
        } else {
            None
        };
        (&compact[..], size)
    };
    if letters.len() < 5 || !letters[..5].iter().all(char::is_ascii_alphabetic) {
        return Err(format!(
            "{} is not a holder code: a holder code looks like PCLNR 2525 M12, a boring bar's like S25S-PCLNR 12.",
            text.trim()
        ));
    }
    let (clamping, shape, style, hand) = (letters[0], letters[1], letters[2], letters[4]);
    if !"PMSCDTW".contains(clamping) {
        return Err(format!(
            "{clamping} is not a clamping letter (P, M, S, C, D, T, W)."
        ));
    }
    if shape_row(shape).is_none() && !OTHER_SHAPES.contains(shape) {
        return Err(format!("{shape} is not an insert shape letter."));
    }
    if shape_row(shape).is_none() {
        return Err(format!(
            "Holders for insert shape {shape} are not supported yet (S, T, C, D, E, M and V are)."
        ));
    }
    let Some(approach_angle) = STYLES.iter().find(|s| s.0 == style).map(|s| s.1) else {
        return Err(format!("{style} is not a holder style letter."));
    };
    let Some(hand) = Hand::from_letter(hand) else {
        return Err(format!("{hand} is not a hand letter (R, L or N)."));
    };
    Ok(HolderCode {
        text: upper.split_whitespace().collect::<Vec<_>>().join(" "),
        bar,
        clamping,
        shape,
        style,
        approach_angle,
        hand,
        size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(insert: &str, holder: &str) -> Result<ReadInsert, String> {
        InsertGeometry::read(insert, holder, TypedInsert::default())
    }

    #[test]
    fn an_insert_code_is_read() {
        let i = parse_insert("cnmg120408").unwrap();
        assert_eq!(i.text, "CNMG 120408");
        assert_eq!((i.shape, i.tip_angle, i.nose_radius), ('C', 80.0, 0.8));
        assert!((i.inscribed_circle - 12.7).abs() < 1e-9);
        let v = parse_insert("VBMT 160404").unwrap();
        assert_eq!(
            (v.tip_angle, v.nose_radius, v.clearance),
            (35.0, 0.4, Some(5.0))
        );
        assert!((v.edge_length - 16.6).abs() < 0.1);
        // A maker's chip breaker after the code is ignored.
        assert_eq!(parse_insert("CNMG 120408-PM").unwrap().text, "CNMG 120408");
        assert!(
            parse_insert("RCMT 1204M0")
                .unwrap_err()
                .contains("not supported")
        );
        assert!(
            parse_insert("CNMG 120499")
                .unwrap_err()
                .contains("corner radius")
        );
        assert!(parse_insert("CNMG").unwrap_err().contains("too short"));
        assert!(
            parse_insert("XNMG 120408")
                .unwrap_err()
                .contains("not an insert shape")
        );
    }

    #[test]
    fn holder_and_bar_codes_are_read() {
        let h = parse_holder("PCLNR 2525 M12").unwrap();
        assert_eq!(
            (h.bar, h.shape, h.approach_angle, h.hand),
            (false, 'C', 95.0, Hand::Right)
        );
        assert_eq!(h.size, Some(25.0));
        let b = parse_holder("S25S-PCLNR 12").unwrap();
        assert_eq!((b.bar, b.style, b.size), (true, 'L', Some(25.0)));
        assert_eq!(b.text, "S25S-PCLNR 12");
        let v = parse_holder("SVJBL").unwrap();
        assert_eq!((v.approach_angle, v.hand, v.size), (93.0, Hand::Left, None));
        assert!(parse_holder("PCXNR").unwrap_err().contains("style"));
        assert!(parse_holder("PCLNX").unwrap_err().contains("hand letter"));
        assert!(
            parse_holder("12")
                .unwrap_err()
                .contains("not a holder code")
        );
    }

    #[test]
    fn the_back_angle_follows_the_insert_and_the_holder() {
        let r = read("CNMG 120408", "PCLNR").unwrap();
        assert_eq!(
            (r.geometry.back_angle(), r.code_nose_radius()),
            (5.0, Some(0.8))
        );
        assert!(r.geometry.typed.is_empty());
        let r = read("VBMT 160404", "SVJBR").unwrap();
        assert_eq!(r.geometry.back_angle(), 52.0);
        let g = read("", "SVVBN").unwrap().geometry;
        assert_eq!(
            (g.tip_angle, g.back_angle(), g.hand),
            (35.0, 72.5, Hand::Neutral)
        );
        // A neutral tool without a holder code points straight down.
        let typed = TypedInsert {
            hand: Some(Hand::Neutral),
            ..TypedInsert::default()
        };
        let g = InsertGeometry::read("TNMG 160408", "", typed)
            .unwrap()
            .geometry;
        assert_eq!((g.approach_angle, g.back_angle()), (60.0, 60.0));
        let o = g.outline(0.8, false, 25.0);
        let mid = |pts: &[PXZ]| pts.iter().map(|p| p[1]).sum::<f64>() / pts.len() as f64;
        // Centred on the nose's centre, a nose radius from the controlled point.
        assert!(
            (mid(&o.insert) - 0.8).abs() < 0.01,
            "insert centred: {}",
            mid(&o.insert)
        );
        let e = read("DNMG 150608", "PCLNR").unwrap_err();
        assert_eq!(
            e,
            "The holder PCLNR takes C inserts (rhombic 80), not D (rhombic 55)."
        );
        let typed = TypedInsert {
            approach_angle: Some(107.5),
            ..TypedInsert::default()
        };
        let e = InsertGeometry::read("CNMG 120408", "", typed).unwrap_err();
        assert!(e.contains("does not fit"), "{e}");
    }

    /// Typed angles win over the codes' and are kept as typed; a hand that
    /// is not the holder's is refused; angles without codes work.
    #[test]
    fn typed_values_win_over_the_codes() {
        let typed = TypedInsert {
            tip_angle: Some(75.0),
            approach_angle: Some(93.0),
            hand: None,
            nose_radius: Some(0.4),
        };
        let r = InsertGeometry::read("CNMG 120408", "PCLNR 2525 M12", typed.clone()).unwrap();
        assert_eq!(
            (r.geometry.tip_angle, r.geometry.approach_angle),
            (75.0, 93.0)
        );
        assert_eq!(r.geometry.back_angle(), 12.0);
        assert_eq!(r.geometry.typed, typed);
        assert_eq!(r.code_nose_radius(), Some(0.8));
        let left = TypedInsert {
            hand: Some(Hand::Left),
            ..TypedInsert::default()
        };
        assert_eq!(
            InsertGeometry::read("", "PCLNR", left.clone()).unwrap_err(),
            "The holder PCLNR is right hand, not left hand."
        );
        // Without codes: the angles alone.
        let angles = TypedInsert {
            tip_angle: Some(55.0),
            approach_angle: Some(93.0),
            ..left
        };
        let r = InsertGeometry::read("", "", angles).unwrap();
        assert_eq!(r.geometry.back_angle(), 32.0);
        assert_eq!(r.geometry.hand, Hand::Left);
        assert!(r.geometry.insert.is_empty() && r.geometry.holder.is_empty());
        assert_eq!(r.code_nose_radius(), None);
    }

    /// The stored form reads back the same, and a tool kept before the
    /// typed values existed reads with none.
    #[test]
    fn the_geometry_reads_back_as_written() {
        let typed = TypedInsert {
            approach_angle: Some(93.0),
            ..TypedInsert::default()
        };
        let g = InsertGeometry::read("CNMG 120408", "PCLNR", typed)
            .unwrap()
            .geometry;
        let text = serde_json::to_string(&g).unwrap();
        assert_eq!(serde_json::from_str::<InsertGeometry>(&text).unwrap(), g);
        let old: InsertGeometry =
            serde_json::from_str(r#"{"tip_angle": 80, "approach_angle": 95}"#).unwrap();
        assert!(old.typed.is_empty());
        assert_eq!((old.hand, old.edge_length), (Hand::Right, 12.0));
        let untyped = read("CNMG 120408", "PCLNR").unwrap().geometry;
        assert!(!serde_json::to_string(&untyped).unwrap().contains("typed"));
    }

    #[test]
    fn the_outline_starts_at_the_controlled_point() {
        let g = read("CNMG 120408", "PCLNR").unwrap().geometry;
        let o = g.outline(0.8, false, 25.0);
        // The nose touches x = 0 and z = 0 and the insert lies above and
        // behind them.
        let min_x = o.insert.iter().fold(f64::MAX, |m, p| m.min(p[0]));
        let min_z = o.insert.iter().fold(f64::MAX, |m, p| m.min(p[1]));
        assert!(min_x.abs() < 0.01 && min_z.abs() < 0.01, "{min_x} {min_z}");
        assert!(o.holder.iter().all(|p| p[0] > 0.0));
        // The holder stays behind the trailing edge's line: it never
        // reaches below it, whatever the insert.
        for (ins, hol, approach) in [
            ("CNMG 120408", "PCLNR", None),
            ("TNMG 160408", "", Some(95.0)),
            ("VBMT 160404", "SVJBR", None),
            ("DNMG 150608", "PDJNR", None),
        ] {
            let typed = TypedInsert {
                approach_angle: approach,
                ..TypedInsert::default()
            };
            let r = InsertGeometry::read(ins, hol, typed).unwrap();
            let o = r
                .geometry
                .outline(r.code_nose_radius().unwrap(), false, 25.0);
            let t = o.holder[0];
            let slope = r.geometry.back_angle().to_radians().tan();
            for p in &o.holder {
                if p[1] > t[1] + 1e-9 {
                    assert!(
                        p[0] - t[0] >= slope * (p[1] - t[1]) - 1e-9,
                        "{ins} {hol}: {p:?} below the trailing edge from {t:?}"
                    );
                }
            }
        }
        // A boring bar lies on the bore's side, below the axis; a left-hand
        // tool towards -z.
        let o = g.outline(0.8, true, 25.0);
        assert!(o.insert.iter().all(|p| p[0] <= 0.01));
        assert!(o.holder.iter().all(|p| p[0] <= 0.01));
        let left = InsertGeometry {
            hand: Hand::Left,
            ..g
        };
        let o = left.outline(0.8, false, 25.0);
        assert!(o.insert.iter().all(|p| p[1] <= 0.01));
        assert!(o.insert.iter().all(|p| p[0] >= -0.01));
        // A neutral tool's holder stands over the nose, on both sides of it.
        let neutral = read("VNMG 160404", "SVVBN").unwrap().geometry;
        let o = neutral.outline(0.4, false, 25.0);
        let (lo, hi) = o
            .holder
            .iter()
            .fold((f64::MAX, f64::MIN), |(l, h), p| (l.min(p[1]), h.max(p[1])));
        assert!(lo < 0.0 && hi > 0.0, "{lo} {hi}");
    }
}
