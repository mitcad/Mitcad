// SPDX-License-Identifier: MIT
//! Timber frames (mitcad#479; `commands.md`, "BUILD: timber frames"): a
//! wall, floor or roof frame generated from a few values and kept as one,
//! its members ordinary BUILD parts made from the library's timber.
//!
//! A frame's definition ([`FrameInput`]) is kept on its lead member's
//! record (`framing`); every member's record names the lead (`frame`) and
//! its place in the frame (`member`: `stud.3`, `plate.top.0`). Laying a
//! frame out is geometry from definitions only: a wall reads the walls it
//! butts against and the walls that butt against it, a roof the two walls
//! it stands on, never the members as they are. Editing a frame lays it out
//! again with the frames that depend on it, keeping the components of the
//! members whose place stays (`spacing::replace_member`), in one undo step
//! with the frames' fasteners driven anew.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::connect::{RemovePlan, apply_remove};
use crate::geometry::{V3, Z, add, cross, dot, inverse, norm, scale, sub, unit};
use crate::place::{
    PartInput, PlaceAt, PlaceInput, Plan, SideFrom, SourceUse, Spot, edit_part, make_part, refuse,
    value_in,
};
use crate::prelude::*;
use crate::spacing::replace_member;
use crate::timber::PointTo;
use crate::{BuildRole, SourceId};
use mitcad_model::expr::Unit;
use mitcad_model::features::ValueInput;
use mitcad_model::features::derived_part::{EndCut, Ends, PartCut, Tilt};
use mitcad_model::ids::{BodyUid, ComponentUid};
use mitcad_model::internal::build::round;
use mitcad_model::kernel::{BoundingBox, Kernel};
use mitcad_model::topo::FaceName;
use mitcad_model::transform::Transform;
use mitcad_model::{DocState, Document, ModelError};

/// Lengths within this are the same, mm.
const EPS: f64 = 1e-6;

/// A length for an answer: to a millionth of a millimetre.
fn round6(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

/// What a frame is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameKind {
    #[default]
    Wall,
    Floor,
    Roof,
}

impl FrameKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wall => "wall",
            Self::Floor => "floor",
            Self::Roof => "roof",
        }
    }

    /// The frame's name's first word (`Wall 1`).
    fn title(self) -> &'static str {
        match self {
            Self::Wall => "Wall",
            Self::Floor => "Floor",
            Self::Roof => "Roof",
        }
    }
}

/// A roof's shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoofKind {
    /// Two slopes meeting at a ridge board halfway between the walls.
    #[default]
    Gable,
    /// One slope from the lower wall over the higher one.
    Mono,
}

/// A wall's plates: how many at its bottom and at its top (1 or 2 each).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plates {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<u32>,
}

/// The wall frames a wall's ends butt against (corners and T-junctions).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameJoins {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    /// Where its start meets the wall `start` names (mitcad#510): on its
    /// side (left out), or `end`: that wall's end face butts against this
    /// wall's side at its start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_on: Option<JoinOn>,
}

/// Where a wall's start meets the wall it joins (mitcad#510).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinOn {
    /// Its start butts against that wall's side.
    Side,
    /// That wall's end face butts against its side.
    End,
}

/// How a wall's ends meet the walls they join (mitcad#510).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallCorner {
    /// Its end studs and plates butt square against the other wall's side
    /// face; the other wall is neither run on, lapped nor notched, only
    /// given a backing stud each side of it where they fit.
    Butt,
    /// Near the other wall's end an L corner, the other wall run on to its
    /// outer face and the plates lapped; elsewhere a T, a stud each side of
    /// it in the other wall (mitcad#493). Left out: this, as walls saved
    /// before mitcad#510 keep their corners.
    Lapped,
    /// A start on an end face continues the wall clicked in line, its end
    /// against that end face (mitcad#524); its ends otherwise butt as with
    /// `butt`.
    Straight,
}

impl WallCorner {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Butt => "butt",
            Self::Lapped => "lapped",
            Self::Straight => "straight",
        }
    }
}

/// A body's face a wall starts on (mitcad#524): the body (its uid), the
/// face (its name) and the occurrence that places it (its path from the
/// root, `O1/O4`; left out: the body's first placement).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartFace {
    pub body: String,
    pub face: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<String>,
}

/// Where an opening is: along a wall from its start, or a floor's
/// `[along the layout, along the joists]` from the outline's least corner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpeningOffset {
    Along(f64),
    At([f64; 2]),
}

/// An opening in a frame, mm: a wall's door or window (`offset`, `width`,
/// `height`, `sill`: 0 or left out for a door) or a floor's stair opening
/// (`offset`, `size`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opening {
    pub offset: OpeningOffset,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sill: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<[f64; 2]>,
}

/// `build_frame`'s fields, and `build_edit`'s `frame` (every field
/// optional there, an edit's fields replacing the definition's). Points are
/// in the BUILD host's coordinates.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<FrameKind>,
    /// The members' timber (studs, joists, rafters; the others' default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<PartInput>,
    /// A wall's plates and sills.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plate: Option<PartInput>,
    /// A wall's headers over its openings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<PartInput>,
    /// A floor's rim joists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rim: Option<PartInput>,
    /// A gable roof's ridge board.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ridge: Option<PartInput>,
    /// The wood fastener driven at every junction of the frame's members
    /// (two, sized from the members, as `build_connect`'s default; its
    /// `config` a chosen size, mitcad#497).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fastener: Option<PartInput>,
    /// How many at each junction (default 2, mitcad#497).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fastener_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    // A wall.
    /// The wall's centre line at its base: from `start` to `end`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<V3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<V3>,
    /// An edit's: the wall's end moved along it to this length.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<ValueInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<ValueInput>,
    /// The members' centre distance (a wall's studs, a floor's joists, a
    /// roof's rafters).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing: Option<ValueInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plates: Option<Plates>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openings: Option<Vec<Opening>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub joins: Option<FrameJoins>,
    /// `build_frame`'s and its preview's only (mitcad#493): `start` is a
    /// point on the side (or an end) of the wall `joins.start` names, and
    /// the wall starts square to that wall as `corner` has it; not kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub square: Option<bool>,
    /// How its ends meet the walls they join (mitcad#510; left out
    /// `lapped`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner: Option<WallCorner>,
    // A floor.
    /// The closed outline the floor fills (its first point's height).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<Vec<V3>>,
    /// The joists' direction (square to the outline's first side when
    /// left out).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<V3>,
    /// Rows of blocking between the joists (0 none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocking: Option<u32>,
    /// Trimmer joists (and headers) on each side of a floor's opening.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trimmers: Option<u32>,
    // A roof.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roof: Option<RoofKind>,
    /// The two wall frames it stands on (any member of each).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub walls: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pitch: Option<ValueInput>,
    /// The rafters' run beyond the walls' outer faces, horizontally.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overhang: Option<ValueInput>,
    // A wall started on another wall's side (`square`): `build_frame`'s
    // and its preview's only, not kept.
    /// Where along that wall it starts (mitcad#514): `offset` from `from`
    /// (end A, end B or the middle of that wall's end faces; as a side
    /// placement's, mitcad#417); either given, both are typed (end A and 0
    /// when left out), else the click decides, snapped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<SideFrom>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<ValueInput>,
    /// The app's least snap distance, mm (as `at.snap`, mitcad#417).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snap: Option<f64>,
    /// Without `end` (mitcad#515): it ends at the first wall frame its run
    /// meets, butting against its side (true or left out), or runs
    /// `length` (false).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_end: Option<bool>,
    /// With `square` and no `joins.start` (mitcad#524): the face of another
    /// body (an imported wall, a solid, a loose member) the wall starts
    /// square from, `start` the click on it; not kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<StartFace>,
    /// How a wall's openings are framed (mitcad#519): as in a load-bearing
    /// wall (true or left out, as walls saved before keep: a header of
    /// plies on edge over trimmer studs beside the king studs) or as in a
    /// partition (false: one header lying flat between the king studs, no
    /// trimmer studs; the king studs are the opening's sides).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_bearing: Option<bool>,
}

impl FrameInput {
    /// This definition with an edit's fields over it.
    pub fn merged(&self, edit: &FrameInput) -> FrameInput {
        macro_rules! pick {
            ($field:ident) => {
                edit.$field.clone().or_else(|| self.$field.clone())
            };
        }
        FrameInput {
            kind: self.kind.or(edit.kind),
            part: pick!(part),
            plate: pick!(plate),
            header: pick!(header),
            rim: pick!(rim),
            ridge: pick!(ridge),
            fastener: pick!(fastener),
            fastener_count: pick!(fastener_count),
            name: pick!(name),
            start: pick!(start),
            end: pick!(end),
            length: pick!(length),
            height: pick!(height),
            spacing: pick!(spacing),
            plates: pick!(plates),
            openings: pick!(openings),
            joins: pick!(joins),
            square: None,
            corner: pick!(corner),
            outline: pick!(outline),
            direction: pick!(direction),
            blocking: pick!(blocking),
            trimmers: pick!(trimmers),
            roof: pick!(roof),
            walls: pick!(walls),
            pitch: pick!(pitch),
            overhang: pick!(overhang),
            from: None,
            offset: None,
            snap: None,
            auto_end: None,
            face: None,
            load_bearing: pick!(load_bearing),
        }
    }

    fn kind(&self) -> FrameKind {
        self.kind.unwrap_or_default()
    }
}

/// What the lead member's record keeps of its frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FrameDef {
    pub frame: FrameInput,
    pub count: usize,
    /// The frame's own component in the BUILD host (mitcad#522), which
    /// holds its members, their fasteners and joints; the definition's
    /// points are in its coordinates. None: a frame saved before, its
    /// members flat in the host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<ComponentUid>,
}

/// What `build_frame` and a frame's `build_edit` did.
#[derive(Debug, Clone, Default)]
pub struct Framed {
    /// The frame's lead member.
    pub lead: Option<ComponentUid>,
    pub name: String,
    pub kind: &'static str,
    /// Every member: its component and its place in the frame.
    pub members: Vec<(ComponentUid, String)>,
    /// The junctions given fasteners.
    pub fasteners: usize,
    pub warnings: Vec<String>,
    /// Other frames laid out again with it (their leads).
    pub relaid: Vec<ComponentUid>,
    pub changed: Vec<String>,
    /// A wall started square on another wall's side: where it meets it.
    pub junction: Option<StartJunction>,
    /// The frame's own component in the host (mitcad#522); None for a
    /// flat frame saved before.
    pub component: Option<ComponentUid>,
    /// A wall's plates made of pieces (mitcad#536), as the answer gives
    /// them.
    pub splices: Vec<Value>,
}

/// A frame made in a step: its lead, its members (component and place) and
/// its own component (mitcad#522; None for a flat frame saved before).
#[derive(Debug, Clone)]
struct MadeFrame {
    lead: ComponentUid,
    members: Vec<(ComponentUid, String)>,
    component: Option<ComponentUid>,
}

/// Where a wall started square on another wall's side (`square`,
/// mitcad#493) or on any other body's face (mitcad#524) meets it.
#[derive(Debug, Clone, PartialEq)]
pub struct StartJunction {
    /// On a timber wall frame: with lapped corners `L` (a corner at one of
    /// the wall's ends) or `T`; butting (mitcad#510) `side` (from its side
    /// at the click) or `end` (a corner at its end face); `straight`
    /// (mitcad#524: in line beyond its end face). On another body's face
    /// (mitcad#524): `face` (square from it), `face_end` (its side against
    /// a thin end face, a corner) or `face_straight` (in line beyond it).
    pub kind: &'static str,
    /// `butt`, `lapped` or `straight` (mitcad#510, mitcad#524).
    pub corner: &'static str,
    /// The wall's name and lead; another body's name and no lead.
    pub wall: String,
    pub frame: Option<ComponentUid>,
    /// An L's or an end's end of that wall: `a` (its start) or `b`.
    pub end: Option<&'static str>,
    /// Along that wall from its start to the new wall's centre line, mm;
    /// along a body's face from its end A.
    pub at: f64,
    /// The new wall's length: from that wall's centre line, mm; from its
    /// far side at an `end`; from the face.
    pub length: f64,
    /// On the side (mitcad#514): what `offset` is from and the offset, mm
    /// (an L's end and 0); None at an `end`.
    pub from: Option<SideFrom>,
    pub offset: f64,
    /// The wall or face its end butts against (mitcad#515, mitcad#524).
    pub to: Option<EndMet>,
    /// The new wall between the faces it butts against (from that wall's
    /// near side, at an `end` from its far side, to its end), mm.
    pub run: f64,
    /// On another body's face (mitcad#524): where its base is.
    pub base: Option<WallBase>,
    /// A typed offset that would hang past an end, made flush with it.
    pub warnings: Vec<String>,
}

/// What a wall's end butts against (mitcad#515, mitcad#524): a wall
/// frame's side (its name and lead) or another body's face (its name), and
/// whether its run met it (`auto_end`) rather than an end clicked on it.
#[derive(Debug, Clone, PartialEq)]
pub struct EndMet {
    pub name: String,
    pub frame: Option<ComponentUid>,
    pub auto: bool,
}

/// What a wall's run met (mitcad#515, mitcad#524): how far from its start
/// the near face is, its name, and the wall frame's lead (None: another
/// body's face).
#[derive(Debug, Clone)]
pub(crate) struct RunMet {
    at: f64,
    name: String,
    wall: Option<ComponentUid>,
}

/// Where a wall started on a body's face stands (mitcad#524): the height
/// of its bottom plate's underside, and what decided it: `floor` (the top
/// of the body under the click, named), `face` (the clicked face's bottom
/// edge) or `origin` (the host's z = 0).
#[derive(Debug, Clone, PartialEq)]
pub struct WallBase {
    pub z: f64,
    pub on: &'static str,
    pub body: Option<String>,
}

impl StartJunction {
    pub fn to_json(&self) -> Value {
        let mm = |v: f64| (v * 1e6).round() / 1e6;
        let mut out = json!({
            "kind": self.kind,
            "corner": self.corner,
            "wall": self.wall,
            "frame": self.frame,
            "end": self.end,
            "at": mm(self.at),
            "length": mm(self.length),
            "from": self.from.map(SideFrom::as_str),
            "offset": self.from.map(|_| mm(self.offset)),
            "to": self.to.as_ref().map(|to| {
                let mut met = json!({"wall": to.name, "frame": to.frame, "auto": to.auto});
                // Another body's face (mitcad#524); a wall's as before.
                if to.frame.is_none() {
                    met["face"] = json!(true);
                }
                met
            }),
            "run": mm(self.run),
        });
        // A start on another body's face (mitcad#524): where its base is;
        // on a wall frame left out, as before.
        if let Some(b) = &self.base {
            out["base"] = json!({"z": mm(b.z), "on": b.on, "body": b.body});
        }
        out
    }
}

/// A frame's member before it is planned: where its axis runs (in the
/// BUILD host's coordinates), the way its section's larger size points,
/// its end cuts and its notches.
#[derive(Debug, Clone)]
pub(crate) struct MemberSpec {
    pub key: String,
    pub role: &'static str,
    which: Which,
    pub start: V3,
    pub end: V3,
    /// Square to the axis: the section's larger size along it.
    wide: V3,
    /// Plumb cuts (a rafter's): the angle and the lean (in the host) of
    /// end A's and end B's cut.
    plumb: Option<(f64, [V3; 2])>,
    notches: Vec<Notch>,
}

/// A box a member is notched by, in the host: from `origin` along `x`, `y`
/// and their cross product by `size`; `with` the other wall's lead (None:
/// the frame being made); `kind` the cut's (`birdsmouth`, a lapped
/// corner's `half_lap`).
#[derive(Debug, Clone)]
struct Notch {
    kind: &'static str,
    origin: V3,
    x: V3,
    y: V3,
    size: [f64; 3],
    with: Option<ComponentUid>,
}

/// Which of a frame's parts a member is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Which {
    Part,
    Plate,
    Header,
    Rim,
    Ridge,
}

/// A part of a frame: its source and its section (W along X, T along Y).
#[derive(Debug, Clone)]
pub(crate) struct FramePart {
    input: PartInput,
    source: SourceUse,
    w: f64,
    t: f64,
}

impl FramePart {
    fn depth(&self) -> f64 {
        self.w.max(self.t)
    }

    fn thick(&self) -> f64 {
        self.w.min(self.t)
    }

    /// Its least length, mm (0 when the library gives none).
    fn least(&self) -> f64 {
        self.source
            .meta
            .length
            .as_ref()
            .and_then(|l| l.min.as_ref())
            .and_then(|m| m.millimetres())
            .unwrap_or(0.0)
    }

    /// Its longest, mm (mitcad#536): the library's `length.max`, else its
    /// longest stock length; None when the library gives neither.
    fn longest(&self) -> Option<f64> {
        let length = self.source.meta.length.as_ref()?;
        length
            .max
            .as_ref()
            .and_then(|m| m.millimetres())
            .or_else(|| length.stock_lengths().first().copied())
            .filter(|l| *l > 0.0)
    }

    /// The longest piece a spliced plate is made of, mm (mitcad#536): its
    /// longest stock length, at most its longest; its longest when the
    /// library lists no stock.
    fn longest_piece(&self) -> Option<f64> {
        let longest = self.longest()?;
        let stock = self
            .source
            .meta
            .length
            .as_ref()
            .and_then(|l| l.stock_lengths().first().copied());
        Some(stock.map_or(longest, |s| s.min(longest)))
    }
}

/// A frame of the design (or the one being made, `lead` None).
#[derive(Debug, Clone)]
pub(crate) struct Known {
    lead: Option<ComponentUid>,
    def: FrameInput,
    /// Its own component (mitcad#522; None for a flat frame saved before,
    /// and for the one being made until it is made).
    component: Option<ComponentUid>,
    /// Its definition's coordinates in the BUILD host's: its component's
    /// placement there (the identity for a flat frame, and until the frame
    /// is moved as one).
    place: Transform,
}

impl Known {
    /// A frame being made: in the host's coordinates, its component made
    /// with it.
    fn new(def: FrameInput) -> Self {
        Self {
            lead: None,
            def,
            component: None,
            place: Transform::IDENTITY,
        }
    }

    /// A point of its definition in the host.
    fn point(&self, p: V3) -> V3 {
        self.place.apply_point(p)
    }

    /// A direction of its definition in the host.
    fn vector(&self, v: V3) -> V3 {
        self.place.apply_vector(v)
    }
}

/// A frame's members laid out.
#[derive(Debug, Default)]
struct Layout {
    members: Vec<MemberSpec>,
    warnings: Vec<String>,
    /// A wall's plates made of several pieces (mitcad#536).
    splices: Vec<Splice>,
}

/// A wall's plate made of pieces butted end to end (mitcad#536): its first
/// piece's place in the frame (`plate.top.0`, `plate.bottom.1`), its role,
/// where the joints are (mm along the wall from its start, each over a
/// stud's centre) and the longest piece its timber comes in.
#[derive(Debug, Clone)]
struct Splice {
    key: String,
    role: &'static str,
    joints: Vec<f64>,
    longest: f64,
}

impl Splice {
    /// As `build_preview` and `build_frame` answer it: `{plate, role, at,
    /// pieces, longest}`.
    fn to_json(&self) -> Value {
        json!({
            "plate": self.key,
            "role": self.role,
            "at": self.joints.iter().map(|t| round6(*t)).collect::<Vec<_>>(),
            "pieces": self.joints.len() + 1,
            "longest": round6(self.longest),
        })
    }
}

impl Layout {
    fn add(
        &mut self,
        key: impl Into<String>,
        role: &'static str,
        which: Which,
        ends: [V3; 2],
        wide: V3,
    ) {
        self.members.push(MemberSpec {
            key: key.into(),
            role,
            which,
            start: ends[0],
            end: ends[1],
            wide,
            plumb: None,
            notches: Vec::new(),
        });
    }
}

/// A member planned: what `build_place` would make of it, and its notches
/// in its own coordinates.
#[derive(Debug, Clone)]
struct PlannedMember {
    spec: MemberSpec,
    plan: Plan,
    cuts: Vec<PartCut<ValueInput>>,
}

/// A frame laid out and planned, ready to be made.
#[derive(Debug, Clone)]
pub(crate) struct FramePlan {
    lead: Option<ComponentUid>,
    /// Its own component (mitcad#522): None for a new frame (made with it)
    /// and for a flat frame saved before (laid out in the host as it was).
    component: Option<ComponentUid>,
    /// A flat frame saved before (mitcad#522): it stays in the host.
    flat: bool,
    name: String,
    kind: FrameKind,
    def: FrameInput,
    parts: BTreeMap<Which, FramePart>,
    members: Vec<PlannedMember>,
    /// The members that stay, by their place.
    keep: BTreeMap<String, ComponentUid>,
    warnings: Vec<String>,
    /// A wall's plates made of pieces (mitcad#536).
    splices: Vec<Splice>,
}

// --- Plane geometry.

fn cross2(a: V3, b: V3) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

/// A direction made horizontal.
fn level(v: V3) -> Option<V3> {
    unit([v[0], v[1], 0.0])
}

/// Where the line `p + t u` meets the line `q + s v` (in plan): `t`.
fn line_param(p: V3, u: V3, q: V3, v: V3) -> Option<f64> {
    let c = cross2(u, v);
    (c.abs() > 1e-6).then(|| cross2(sub(q, p), v) / c)
}

// --- Walls.

/// A wall frame's definition worked out.
#[derive(Debug, Clone)]
pub(crate) struct WallGeo {
    lead: Option<ComponentUid>,
    name: String,
    /// Its centre line's start at its base, the line's direction and the
    /// horizontal across it (to its left).
    start: V3,
    u: V3,
    n: V3,
    length: f64,
    height: f64,
    spacing: f64,
    bottom: usize,
    top: usize,
    stud: FramePart,
    plate: FramePart,
    header: FramePart,
    openings: Vec<Opening>,
    joins: [Option<ComponentUid>; 2],
    /// Its ends butt square against the walls they join, which get only
    /// backing studs (`corner: butt`, mitcad#510).
    butt: bool,
    /// The wall its start joins butts its end face against its side
    /// (`joins.start_on: end`, mitcad#510).
    start_on_end: bool,
    warnings: Vec<String>,
    /// Its openings framed as a load-bearing wall's (`load_bearing`,
    /// mitcad#519; left out: true).
    load_bearing: bool,
}

impl WallGeo {
    /// Its depth across: the studs' larger size.
    fn depth(&self) -> f64 {
        self.stud.depth()
    }

    /// A point `t` along it, `z` above its base and `a` across it.
    fn at(&self, t: f64, z: f64, a: f64) -> V3 {
        add(
            add(add(self.start, scale(self.u, t)), scale(Z, z)),
            scale(self.n, a),
        )
    }

    /// The top of its top plate.
    fn top_z(&self) -> f64 {
        self.start[2] + self.height
    }
}

/// Where a wall's plates run (`t0` to `t1` along it, after its corners)
/// and the extra studs where other walls butt against it.
#[derive(Debug, Clone, Default)]
struct WallSpan {
    t0: f64,
    t1: f64,
    extra: Vec<(f64, String)>,
    warnings: Vec<String>,
    /// The lapped corners at its start and its end (mitcad#493).
    laps: [Option<Lap>; 2],
}

/// A wall's plates at an L corner, lapped with the other wall's: `to`
/// along it, the lap's other end (beyond its end where it butts against
/// the other wall, `over` its plates; else the other wall's inner face,
/// under its plates); `with` the other wall's lead.
#[derive(Debug, Clone, Copy)]
struct Lap {
    to: f64,
    over: bool,
    with: Option<ComponentUid>,
}

/// A wall's span: its own ends butting against the walls they join, and
/// the corners and T-junctions of the walls that join it.
fn wall_span(w: &WallGeo, walls: &[WallGeo]) -> WallSpan {
    let mut span = WallSpan {
        t0: 0.0,
        t1: w.length,
        ..WallSpan::default()
    };
    for (k, end) in w.joins.iter().enumerate() {
        let Some(lead) = end else { continue };
        let which = if k == 0 { "start" } else { "end" };
        let Some(x) = walls.iter().find(|o| o.lead == Some(*lead)) else {
            span.warnings.push(format!(
                "{}: the wall frame its {which} joins is gone",
                w.name
            ));
            continue;
        };
        let along = dot(w.u, x.n);
        if along.abs() < 0.1 {
            span.warnings.push(format!(
                "{}: its {which} runs along {}, it does not butt against it",
                w.name, x.name
            ));
            continue;
        }
        if k == 0 && w.start_on_end {
            // That wall's end face against its side at its start
            // (mitcad#510): it runs on from its own start, a backing stud
            // each side of that wall where they fit.
            if let Some(tj) = line_param(w.start, w.u, x.start, x.u) {
                let (dx, ts) = (x.depth(), w.stud.thick());
                let key = format!("junction.{}.0", x.name.replace(' ', "_"));
                span.extra.push((tj - dx / 2.0 - ts, format!("{key}.a")));
                span.extra.push((tj + dx / 2.0, format!("{key}.b")));
            }
            continue;
        }
        let middle = w.at(w.length / 2.0, 0.0, 0.0);
        let side = if dot(sub(middle, x.start), x.n) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        let t = (side * x.depth() / 2.0 - dot(sub(w.start, x.start), x.n)) / along;
        if k == 0 {
            span.t0 = t;
        } else {
            span.t1 = t;
        }
        // At a corner (its centre line meets the other's within its depth of
        // an end, as the other wall finds it) its plates lap over the other's,
        // to that wall's outer face; butting, they stop at its face.
        if !w.butt
            && let Some(tx) = line_param(x.start, x.u, w.start, w.u)
            && (tx < w.depth() || tx > x.length - w.depth())
        {
            span.laps[k] = Some(Lap {
                to: (-side * x.depth() / 2.0 - dot(sub(w.start, x.start), x.n)) / along,
                over: true,
                with: x.lead,
            });
        }
    }
    let Some(me) = w.lead else {
        return span;
    };
    let ts = w.stud.thick();
    for b in walls {
        if b.lead == w.lead {
            continue;
        }
        for (k, end) in b.joins.iter().enumerate() {
            if *end != Some(me) {
                continue;
            }
            // Its end face against a wall started from it: nothing of it
            // changes (mitcad#510).
            if k == 0 && b.start_on_end {
                continue;
            }
            let Some(tj) = line_param(w.start, w.u, b.start, b.u) else {
                continue;
            };
            let db = b.depth();
            let key = format!("junction.{}.{k}", b.name.replace(' ', "_"));
            if b.butt {
                // A wall that butts against it (mitcad#510): neither run on
                // nor lapped, a backing stud each side of it where they fit
                // (as at a T).
                span.extra.push((tj - db / 2.0 - ts, format!("{key}.a")));
                span.extra.push((tj + db / 2.0, format!("{key}.b")));
            } else if tj < db {
                // A corner at its start: it runs to the other's outer face,
                // its plates lapped under the other's.
                span.t0 = span.t0.min(tj - db / 2.0);
                span.extra.push((tj + db / 2.0, key));
                span.laps[0] = Some(Lap {
                    to: tj + db / 2.0,
                    over: false,
                    with: b.lead,
                });
            } else if tj > w.length - db {
                span.t1 = span.t1.max(tj + db / 2.0);
                span.extra.push((tj - db / 2.0 - ts, key));
                span.laps[1] = Some(Lap {
                    to: tj - db / 2.0,
                    over: false,
                    with: b.lead,
                });
            } else {
                span.extra.push((tj - db / 2.0 - ts, format!("{key}.a")));
                span.extra.push((tj + db / 2.0, format!("{key}.b")));
            }
        }
    }
    span
}

/// An opening's place in a wall, worked out.
struct WallZone {
    /// Its number (from 1), where its studs start and end along the wall,
    /// its offset and width, its sill (0 for a door) and its header's
    /// top, where the cripple studs above it start.
    number: usize,
    lo: f64,
    hi: f64,
    offset: f64,
    width: f64,
    sill: f64,
    top: f64,
}

/// A wall's members: its plates, studs at the centre distance from its
/// start, its end studs, the studs where other walls meet it, and each
/// opening's king and trimmer studs, header, sill and cripple studs.
fn wall_layout(w: &WallGeo, span: &WallSpan) -> Result<Layout, ModelError> {
    let (t0, t1) = (span.t0, span.t1);
    let ts = w.stud.thick();
    let tp = w.plate.thick();
    let hd = w.header.depth();
    let mut out = Layout {
        warnings: [w.warnings.clone(), span.warnings.clone()].concat(),
        ..Layout::default()
    };
    if t1 - t0 < 2.0 * ts - EPS {
        return Err(refuse(format!(
            "{} is {} mm long: shorter than its two end studs",
            w.name,
            round(t1 - t0)
        )));
    }
    if w.spacing <= ts + EPS {
        return Err(refuse(format!(
            "the center distance of {} mm is not more than a stud's {} mm",
            round(w.spacing),
            round(ts)
        )));
    }
    let zb = w.bottom as f64 * tp;
    let zt = w.height - w.top as f64 * tp;
    if zt - zb < 1.0 {
        return Err(refuse(format!(
            "{} is {} mm high: no room for studs between its plates",
            w.name,
            round(w.height)
        )));
    }
    let across = w.n;
    let stud_wide = across;
    let mut studs: Vec<WallStud> = Vec::new();
    // The openings (their headers and sills after the plates and studs).
    let mut framing = Layout::default();
    let mut zones: Vec<WallZone> = Vec::new();
    let mut doors: Vec<(f64, f64)> = Vec::new();
    for (i, op) in w.openings.iter().enumerate() {
        let n = i + 1;
        let OpeningOffset::Along(o) = op.offset else {
            return Err(refuse(format!(
                "opening {n}: a wall's opening is at an offset along it (a number)"
            )));
        };
        let width = op
            .width
            .filter(|v| *v > EPS)
            .ok_or_else(|| refuse(format!("opening {n} needs its width")))?;
        let height = op
            .height
            .filter(|v| *v > EPS)
            .ok_or_else(|| refuse(format!("opening {n} needs its height")))?;
        let sill = op.sill.unwrap_or(0.0).max(0.0);
        let door = sill <= EPS;
        let head = sill + height;
        // A load-bearing wall's opening has a trimmer stud each side inside
        // its king studs and a header of plies on edge; a partition's
        // (mitcad#519) its king studs as its sides and one header lying
        // flat between them.
        let (side, header_depth) = if w.load_bearing {
            (2.0 * ts, hd)
        } else {
            (ts, w.header.thick())
        };
        let zone = WallZone {
            number: n,
            lo: o - side,
            hi: o + width + side,
            offset: o,
            width,
            sill: if door { 0.0 } else { sill },
            top: head + header_depth,
        };
        if zone.lo < t0 + ts - EPS || zone.hi > t1 - ts + EPS {
            return Err(refuse(format!(
                "opening {n} at {} mm, {} mm wide, does not fit between the end studs of {}",
                round(o),
                round(width),
                w.name
            )));
        }
        if let Some(j) = zones
            .iter()
            .position(|z| zone.lo < z.hi - EPS && z.lo < zone.hi - EPS)
        {
            return Err(refuse(format!(
                "openings {} and {n} overlap with their studs",
                j + 1
            )));
        }
        if zone.top > zt + EPS {
            return Err(refuse(format!(
                "opening {n}: its header reaches {} mm, above the top plate at {} mm",
                round(zone.top),
                round(zt)
            )));
        }
        if !door && sill - tp < zb - EPS {
            return Err(refuse(format!(
                "opening {n}: its sill at {} mm is below the top of the bottom plate ({} mm)",
                round(sill),
                round(zb + tp)
            )));
        }
        if head <= zb + EPS {
            return Err(refuse(format!(
                "opening {n} is no higher than the bottom plate"
            )));
        }
        let p = format!("opening.{n}");
        studs.push((format!("{p}.king.a"), "king stud", o - side, zb, zt));
        studs.push((
            format!("{p}.king.b"),
            "king stud",
            o + width + side - ts,
            zb,
            zt,
        ));
        if w.load_bearing {
            studs.push((format!("{p}.trimmer.a"), "trimmer stud", o - ts, zb, head));
            studs.push((
                format!("{p}.trimmer.b"),
                "trimmer stud",
                o + width,
                zb,
                head,
            ));
            let plies = ((w.depth() + 1.0) / w.header.thick()).floor().max(1.0) as usize;
            for k in 0..plies {
                let a = (k as f64 - (plies - 1) as f64 / 2.0) * w.header.thick();
                let z = head + hd / 2.0;
                framing.add(
                    format!("{p}.header.{k}"),
                    "header",
                    Which::Header,
                    [w.at(o - ts, z, a), w.at(o + width + ts, z, a)],
                    Z,
                );
            }
        } else {
            // Its wide face down, as a plate, from king stud to king stud.
            let z = head + header_depth / 2.0;
            framing.add(
                format!("{p}.header"),
                "flat header",
                Which::Header,
                [w.at(o, z, 0.0), w.at(o + width, z, 0.0)],
                across,
            );
        }
        if door {
            doors.push((o, o + width));
        } else {
            let z = sill - tp / 2.0;
            framing.add(
                format!("{p}.sill"),
                "sill",
                Which::Plate,
                [w.at(o, z, 0.0), w.at(o + width, z, 0.0)],
                across,
            );
        }
        zones.push(zone);
    }
    // The plates: the top ones through, the bottom ones cut at the doors;
    // at a lapped corner the outer layer runs over the corner on the wall
    // that butts and stops at its face on the other, one plate of each is
    // half-lapped (that wall's half outside).
    let pw = w.plate.depth();
    let plate_run = |top: bool, layer: usize, z: f64| -> ([f64; 2], Vec<Notch>) {
        let count = if top { w.top } else { w.bottom };
        let mut ends = [t0, t1];
        let mut notches = Vec::new();
        for (k, lap) in span.laps.iter().enumerate() {
            let Some(lap) = lap else { continue };
            if count == 1 {
                let face = ends[k];
                if lap.over {
                    ends[k] = lap.to;
                }
                let (mut lo, mut hi) = (face.min(lap.to), face.max(lap.to));
                if k == 0 {
                    lo -= 1.0;
                } else {
                    hi += 1.0;
                }
                // The half cut away: the inner one where it lies over the
                // other's, else the outer one.
                let upper = top != lap.over;
                let z0 = if upper { z } else { z - tp / 2.0 - 1.0 };
                notches.push(Notch {
                    kind: "half_lap",
                    origin: w.at(lo, z0, -pw / 2.0 - 1.0),
                    x: w.u,
                    y: w.n,
                    size: [hi - lo, pw + 2.0, tp / 2.0 + 1.0],
                    with: lap.with,
                });
            } else if layer == 0 {
                ends[k] = lap.to;
            }
        }
        (ends, notches)
    };
    // The studs: the ends, the junctions, the centre distance.
    studs.push(("stud.start".to_owned(), "stud", t0, zb, zt));
    studs.push(("stud.end".to_owned(), "stud", t1 - ts, zb, zt));
    let in_zone = |near: f64| {
        zones
            .iter()
            .find(|z| near < z.hi - EPS && near + ts > z.lo + EPS)
    };
    let mut junction_studs = Vec::new();
    for (near, key) in &span.extra {
        if *near < t0 + ts - EPS || *near + ts > t1 - ts + EPS {
            continue;
        }
        if in_zone(*near).is_some() {
            out.warnings.push(format!(
                "{}: the stud where another wall meets it at {} mm is in an opening's framing: left out",
                w.name,
                round(*near)
            ));
            continue;
        }
        junction_studs.push(*near);
        studs.push((key.clone(), "stud", *near, zb, zt));
    }
    for k in 1.. {
        let c = t0 + k as f64 * w.spacing;
        let near = c - ts / 2.0;
        if near + ts > t1 - ts + EPS {
            break;
        }
        if junction_studs
            .iter()
            .any(|j| near < j + ts - EPS && *j < near + ts - EPS)
        {
            continue;
        }
        match in_zone(near) {
            None => studs.push((format!("stud.{k}"), "stud", near, zb, zt)),
            Some(z) if near >= z.offset - EPS && near + ts <= z.offset + z.width + EPS => {
                // Cripple studs above the header and below the sill.
                let (n, sill, top) = (z.number, z.sill, z.top);
                if zt - top > 1.0 {
                    studs.push((
                        format!("opening.{n}.cripple.top.{k}"),
                        "cripple stud",
                        near,
                        top,
                        zt,
                    ));
                }
                if sill > EPS && sill - tp - zb > 1.0 {
                    studs.push((
                        format!("opening.{n}.cripple.bottom.{k}"),
                        "cripple stud",
                        near,
                        zb,
                        sill - tp,
                    ));
                }
            }
            Some(_) => {}
        }
    }
    // The plates, after the studs their joints lie over, made first (a top
    // plate is the lead): the top ones through, the bottom ones cut at the
    // doors. One longer than its timber's longest is spliced (mitcad#536):
    // pieces butted end to end, each joint over the centre of a stud that
    // bears on it (or a backing stud), a second plate's joints staggered
    // from the first's by a stud spacing.
    let splicing = w.plate.longest_piece().map(|longest| Splicing {
        w,
        longest,
        over: w.plate.longest().unwrap_or(longest),
        least: w.plate.least().max(2.0 * ts),
        ends: [t0, t1],
        heights: [zb, zt],
        zones: &zones,
    });
    let mut staggered: Vec<f64> = Vec::new();
    for j in 0..w.top {
        let z = w.height - (j as f64 + 0.5) * tp;
        let (ends, notches) = plate_run(true, j, z);
        let key = format!("plate.top.{j}");
        let joints = match &splicing {
            Some(s) => s.joints(
                ends,
                true,
                &staggered,
                &mut studs,
                &mut out.warnings,
                "top plate",
            )?,
            None => Vec::new(),
        };
        let cuts: Vec<f64> = [ends[0]]
            .into_iter()
            .chain(joints.iter().copied())
            .chain([ends[1]])
            .collect();
        for (k, piece) in cuts.windows(2).enumerate() {
            // Its first piece keeps the plate's place: the lead stays.
            let place = if k == 0 {
                key.clone()
            } else {
                format!("{key}.{k}")
            };
            out.add(
                place,
                "top plate",
                Which::Plate,
                [w.at(piece[0], z, 0.0), w.at(piece[1], z, 0.0)],
                across,
            );
            if let Some(m) = out.members.last_mut() {
                m.notches = if joints.is_empty() {
                    notches.clone()
                } else {
                    along_piece(w, &notches, piece[0], piece[1])
                };
            }
        }
        if let Some(s) = &splicing
            && !joints.is_empty()
        {
            out.splices.push(Splice {
                key,
                role: "top plate",
                joints: joints.clone(),
                longest: s.longest,
            });
        }
        staggered.extend(joints);
    }
    doors.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut staggered: Vec<f64> = Vec::new();
    for i in 0..w.bottom {
        let z = (i as f64 + 0.5) * tp;
        let (ends, notches) = plate_run(false, i, z);
        let mut runs = Vec::new();
        let mut from = ends[0];
        for (a, b) in &doors {
            runs.push((from, *a));
            from = *b;
        }
        runs.push((from, ends[1]));
        // Each run between the doors spliced where it is too long.
        let mut pieces = Vec::new();
        let mut joints = Vec::new();
        for (a, b) in runs {
            let mine = match &splicing {
                Some(s) if b - a >= 1.0 => s.joints(
                    [a, b],
                    false,
                    &staggered,
                    &mut studs,
                    &mut out.warnings,
                    "bottom plate",
                )?,
                _ => Vec::new(),
            };
            let mut at = a;
            for j in &mine {
                pieces.push((at, *j));
                at = *j;
            }
            pieces.push((at, b));
            joints.extend(mine);
        }
        let many = pieces.len() > 1;
        for (k, (a, b)) in pieces.into_iter().enumerate() {
            if b - a < 1.0 {
                continue;
            }
            let key = if many {
                format!("plate.bottom.{i}.{k}")
            } else {
                format!("plate.bottom.{i}")
            };
            out.add(
                key,
                "bottom plate",
                Which::Plate,
                [w.at(a, z, 0.0), w.at(b, z, 0.0)],
                across,
            );
            // A piece takes the laps along it.
            if let Some(m) = out.members.last_mut() {
                m.notches = along_piece(w, &notches, a, b);
            }
        }
        if let Some(s) = &splicing
            && !joints.is_empty()
        {
            out.splices.push(Splice {
                key: format!("plate.bottom.{i}"),
                role: "bottom plate",
                joints: joints.clone(),
                longest: s.longest,
            });
        }
        staggered.extend(joints);
    }
    for (key, role, near, z0, z1) in studs {
        let c = near + ts / 2.0;
        out.add(
            key,
            role,
            Which::Part,
            [w.at(c, z0, 0.0), w.at(c, z1, 0.0)],
            stud_wide,
        );
    }
    out.members.extend(framing.members);
    Ok(out)
}

/// A wall's stud before it is made: its place in the frame, its role, its
/// near side along the wall and its foot's and top's heights above the
/// base.
type WallStud = (String, &'static str, f64, f64, f64);

/// The notches of a plate's laps along a piece of it from `a` to `b`.
fn along_piece(w: &WallGeo, notches: &[Notch], a: f64, b: f64) -> Vec<Notch> {
    let along = |n: &Notch| dot(sub(n.origin, w.start), w.u);
    notches
        .iter()
        .filter(|n| along(n) < b - EPS && along(n) + n.size[0] > a + EPS)
        .cloned()
        .collect()
}

/// How a wall's plates are spliced (mitcad#536): a plate longer than its
/// timber's longest (`over`, the library's `length.max`) is made of pieces
/// at most `longest` long (its longest stock length) and at least `least`;
/// the studs stand between `ends` (the end studs' outer faces) and between
/// `heights` (the top of the bottom plates and the underside of the top
/// plates), out of the openings' framing (`zones`).
struct Splicing<'a> {
    w: &'a WallGeo,
    longest: f64,
    over: f64,
    least: f64,
    ends: [f64; 2],
    heights: [f64; 2],
    zones: &'a [WallZone],
}

impl Splicing<'_> {
    /// Where a plate from `run[0]` to `run[1]` along the wall is spliced:
    /// nowhere when it is no longer than its timber's longest; else the
    /// joints [`splice_joints`] finds over the centres of the studs that bear
    /// on it (`top`: those reaching the top plates, else those standing on
    /// the bottom plates), a stud spacing from each of `apart` (the first
    /// plate's joints) where the studs allow, else with a warning. Where no
    /// stud is in reach a backing stud goes in (full height,
    /// `stud.splice.<n>`, [`Splicing::backing`]); refused when even that
    /// has no place.
    fn joints(
        &self,
        run: [f64; 2],
        top: bool,
        apart: &[f64],
        studs: &mut Vec<WallStud>,
        warnings: &mut Vec<String>,
        what: &str,
    ) -> Result<Vec<f64>, ModelError> {
        if run[1] - run[0] <= self.over + EPS {
            return Ok(Vec::new());
        }
        let [zb, zt] = self.heights;
        let ts = self.w.stud.thick();
        let gap = self.w.spacing;
        // Each round adds a backing stud in the piece that found none.
        for _ in 0..1000 {
            let seats: Vec<f64> = studs
                .iter()
                .filter(|s| {
                    if top {
                        s.4 >= zt - EPS
                    } else {
                        s.3 <= zb + EPS
                    }
                })
                .map(|s| s.2 + ts / 2.0)
                .collect();
            let failed = match splice_joints(run, self.longest, self.least, &seats, apart, gap) {
                Ok(joints) => return Ok(joints),
                Err(from) => from,
            };
            if !apart.is_empty()
                && let Ok(joints) = splice_joints(run, self.longest, self.least, &seats, &[], gap)
            {
                warnings.push(format!(
                    "{}: the joints of its {what}s are less than a stud spacing apart",
                    self.w.name
                ));
                return Ok(joints);
            }
            let Some(c) = self.backing(failed, run[1], studs, apart) else {
                return Err(refuse(format!(
                    "{}: its {what} from {} to {} mm is longer than its timber's longest, {} mm, and no stud can take a joint between {} and {} mm",
                    self.w.name,
                    round(run[0]),
                    round(run[1]),
                    round(self.over),
                    round(failed + self.least),
                    round((failed + self.longest).min(run[1] - self.least))
                )));
            };
            let n = studs
                .iter()
                .filter(|s| s.0.starts_with("stud.splice."))
                .count()
                + 1;
            studs.push((format!("stud.splice.{n}"), "stud", c - ts / 2.0, zb, zt));
        }
        Err(refuse(format!(
            "{}: its {what} cannot be spliced over its studs",
            self.w.name
        )))
    }

    /// A backing stud's centre for the joint of a piece from `from` on its
    /// way to `to`: as far along as the piece may run (its longest, the
    /// rest at least the least piece), else just before an opening's
    /// framing or a stud in the way; between the end studs, out of the
    /// openings' framing and beside the other studs; a stud spacing from
    /// each of `apart` where it can be. None: nowhere.
    fn backing(&self, from: f64, to: f64, studs: &[WallStud], apart: &[f64]) -> Option<f64> {
        let ts = self.w.stud.thick();
        let (lo, hi) = (
            from + self.least,
            (from + self.longest).min(to - self.least),
        );
        if hi < lo - EPS {
            return None;
        }
        let mut places = vec![hi];
        places.extend(self.zones.iter().map(|z| z.lo - ts / 2.0));
        places.extend(studs.iter().map(|s| s.2 - ts / 2.0));
        let free = |c: f64| {
            let near = c - ts / 2.0;
            c >= lo - EPS
                && c <= hi + EPS
                && near >= self.ends[0] + ts - EPS
                && near + ts <= self.ends[1] - ts + EPS
                && !self
                    .zones
                    .iter()
                    .any(|z| near < z.hi - EPS && near + ts > z.lo + EPS)
                && !studs
                    .iter()
                    .any(|s| near < s.2 + ts - EPS && s.2 < near + ts - EPS)
        };
        let best = |stagger: bool| {
            places
                .iter()
                .copied()
                .filter(|c| free(*c))
                .filter(|c| !stagger || apart.iter().all(|j| (c - j).abs() >= self.w.spacing - EPS))
                .reduce(f64::max)
        };
        best(true).or_else(|| best(false))
    }
}

/// The joints of a plate from `run[0]` to `run[1]` (mitcad#536): from its
/// start, each over the farthest of `seats` that leaves the piece before it
/// at most `longest` and at least `least` long and the rest at least
/// `least`, at least `gap` from each of `apart`. Err: where the piece that
/// finds no seat starts.
fn splice_joints(
    run: [f64; 2],
    longest: f64,
    least: f64,
    seats: &[f64],
    apart: &[f64],
    gap: f64,
) -> Result<Vec<f64>, f64> {
    let mut joints = Vec::new();
    let mut from = run[0];
    while run[1] - from > longest + EPS {
        let seat = seats
            .iter()
            .copied()
            .filter(|c| {
                *c >= from + least - EPS && *c <= from + longest + EPS && run[1] - *c >= least - EPS
            })
            .filter(|c| apart.iter().all(|j| (c - j).abs() >= gap - EPS))
            .reduce(f64::max);
        match seat {
            Some(c) => {
                joints.push(c);
                from = c;
            }
            None => return Err(from),
        }
    }
    Ok(joints)
}

// --- Floors.

/// A floor frame's definition worked out: its outline, the layout
/// direction `a` (across the joists) and the joists' direction `v`.
#[derive(Debug, Clone)]
pub(crate) struct FloorGeo {
    name: String,
    outline: Vec<V3>,
    z0: f64,
    a: V3,
    v: V3,
    spacing: f64,
    blocking: usize,
    trimmers: usize,
    openings: Vec<Opening>,
    joist: FramePart,
    rim: FramePart,
}

/// A run of a floor's members along the joists: its centre across them
/// (`a`), its thickness and where it runs (`v`).
#[derive(Debug, Clone, Copy)]
struct Run {
    a: f64,
    half: f64,
    lo: f64,
    hi: f64,
}

/// A floor's members: rim joists along its outline (the ones across the
/// joists running through the corners), joists at the centre distance
/// from rim to rim, rows of blocking, and each opening's trimmers, headers
/// and tail joists.
fn floor_layout(f: &FloorGeo) -> Result<Layout, ModelError> {
    let mut out = Layout::default();
    let pts: Vec<V3> = f.outline.iter().map(|p| [p[0], p[1], f.z0]).collect();
    let n = pts.len();
    if n < 3 {
        return Err(refuse("a floor's outline has three points at least"));
    }
    let o = pts[0];
    let ca = |p: V3| dot(sub(p, o), f.a);
    let cv = |p: V3| dot(sub(p, o), f.v);
    let at = |a: f64, v: f64, z: f64| add(add(add(o, scale(f.a, a)), scale(f.v, v)), [0.0, 0.0, z]);
    // Its turn: the inward normals.
    let area: f64 = (0..n)
        .map(|i| cross2(pts[i], pts[(i + 1) % n]))
        .sum::<f64>()
        / 2.0;
    if area.abs() < 1.0 {
        return Err(refuse("a floor's outline encloses no area"));
    }
    let edges: Vec<(V3, V3, f64, V3)> = (0..n)
        .map(|i| {
            let p = pts[i];
            let q = pts[(i + 1) % n];
            let e = unit(sub(q, p)).unwrap_or(f.a);
            let left = cross(Z, e);
            let m = if area > 0.0 { left } else { scale(left, -1.0) };
            (p, e, norm(sub(q, p)), m)
        })
        .collect();
    let tr = f.rim.thick();
    let wr = f.rim.depth();
    let tj = f.joist.thick();
    let wj = f.joist.depth();
    if f.spacing <= tj + EPS {
        return Err(refuse(format!(
            "the center distance of {} mm is not more than a joist's {} mm",
            round(f.spacing),
            round(tj)
        )));
    }
    let through: Vec<bool> = edges.iter().map(|e| dot(e.1, f.v).abs() < 0.5).collect();
    // The rims: at each corner the one across the joists runs through.
    let inner = |i: usize, j: usize| -> V3 {
        let (p, e, _, m) = edges[i];
        let (q, g, _, l) = edges[j];
        let pi = add(p, scale(m, tr));
        let qj = add(q, scale(l, tr));
        line_param(pi, e, qj, g).map_or(add(edges[j].0, scale(l, tr)), |t| add(pi, scale(e, t)))
    };
    for i in 0..n {
        let prev = (i + n - 1) % n;
        let next = (i + 1) % n;
        let (p, e, len, m) = edges[i];
        // Who runs through at a corner: the one across the joists, else the
        // earlier side.
        let runs_start = through[i] && !through[prev];
        let runs_end = through[i] || !through[next];
        let v_start = dot(sub(inner(prev, i), p), e);
        let v_end = dot(sub(inner(i, next), p), e);
        let s0 = if runs_start {
            v_start.min(0.0)
        } else {
            v_start.max(0.0)
        };
        let s1 = if runs_end {
            v_end.max(len)
        } else {
            v_end.min(len)
        };
        if s1 - s0 < 1.0 {
            continue;
        }
        let base = add(add(p, scale(m, tr / 2.0)), [0.0, 0.0, wr / 2.0]);
        out.add(
            format!("rim.{i}"),
            "rim joist",
            Which::Rim,
            [add(base, scale(e, s0)), add(base, scale(e, s1))],
            Z,
        );
    }
    // The rims along the joists, as runs (their strips across).
    let mut runs: Vec<Run> = Vec::new();
    for (i, (p, e, len, m)) in edges.iter().enumerate() {
        if through[i] {
            continue;
        }
        let a0 = ca(*p);
        let a1 = a0 + tr * dot(*m, f.a);
        let (v0, v1) = (cv(*p), cv(add(*p, scale(*e, *len))));
        runs.push(Run {
            a: (a0 + a1) / 2.0,
            half: (a1 - a0).abs() / 2.0,
            lo: v0.min(v1),
            hi: v0.max(v1),
        });
    }
    let rim_runs = runs.clone();
    let (amin, amax) = pts
        .iter()
        .map(|p| ca(*p))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
            (a.min(v), b.max(v))
        });
    let (vmin, vmax) = pts
        .iter()
        .map(|p| cv(*p))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
            (a.min(v), b.max(v))
        });
    // Where a line along the joists at `a` is inside the outline, to the
    // rims' inner faces.
    let spans_at = |a: f64| -> Vec<(f64, f64)> {
        let mut hits: Vec<(f64, usize)> = Vec::new();
        for (i, (p, e, len, _)) in edges.iter().enumerate() {
            let q = add(*p, scale(*e, *len));
            let (pa, qa) = (ca(*p), ca(q));
            if (pa <= a && a < qa) || (qa <= a && a < pa) {
                let v = cv(*p) + (a - pa) / (qa - pa) * (cv(q) - cv(*p));
                hits.push((v, i));
            }
        }
        hits.sort_by(|x, y| x.0.total_cmp(&y.0));
        let face = |(v, i): (f64, usize)| -> f64 {
            let (p, _, _, m) = edges[i];
            let mv = dot(m, f.v);
            if mv.abs() < 0.05 {
                return v;
            }
            (tr - dot(sub(at(a, 0.0, 0.0), p), m)) / mv
        };
        hits.chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| (face(c[0]), face(c[1])))
            .filter(|(lo, hi)| hi - lo > 1.0)
            .collect()
    };
    let overlaps_rim = |a: f64, half: f64, lo: f64, hi: f64| {
        rim_runs
            .iter()
            .any(|r| (a - r.a).abs() < half + r.half - EPS && lo < r.hi - EPS && r.lo < hi - EPS)
    };
    // The openings, in (a, v).
    let nt = f.trimmers.max(1);
    let mut holes: Vec<[f64; 4]> = Vec::new();
    for (i, op) in f.openings.iter().enumerate() {
        let k = i + 1;
        let OpeningOffset::At([oa, ov]) = op.offset else {
            return Err(refuse(format!(
                "opening {k}: a floor's opening is at [a, b] from the outline's corner"
            )));
        };
        let [sa, sv] = op
            .size
            .filter(|s| s[0] > EPS && s[1] > EPS)
            .ok_or_else(|| refuse(format!("opening {k} needs its size")))?;
        let r = [amin + oa, amin + oa + sa, vmin + ov, vmin + ov + sv];
        let wide = nt as f64 * tj;
        if r[0] - wide < amin + tr - EPS
            || r[1] + wide > amax - tr + EPS
            || r[2] - wide < vmin + tr - EPS
            || r[3] + wide > vmax - tr + EPS
        {
            return Err(refuse(format!(
                "opening {k} with its trimmers and headers does not fit inside the rims of {}",
                f.name
            )));
        }
        holes.push(r);
        let p = format!("opening.{k}");
        for t in 0..nt {
            for (side, a) in [
                ("a", r[0] - (t as f64 + 0.5) * tj),
                ("b", r[1] + (t as f64 + 0.5) * tj),
            ] {
                for (j, (lo, hi)) in spans_at(a).into_iter().enumerate() {
                    let key = if j == 0 {
                        format!("{p}.trimmer.{side}.{t}")
                    } else {
                        format!("{p}.trimmer.{side}.{t}.{j}")
                    };
                    out.add(
                        key,
                        "trimmer joist",
                        Which::Part,
                        [at(a, lo, wj / 2.0), at(a, hi, wj / 2.0)],
                        Z,
                    );
                    runs.push(Run {
                        a,
                        half: tj / 2.0,
                        lo,
                        hi,
                    });
                }
            }
            for (side, v) in [
                ("a", r[2] - (t as f64 + 0.5) * tj),
                ("b", r[3] + (t as f64 + 0.5) * tj),
            ] {
                out.add(
                    format!("{p}.header.{side}.{t}"),
                    "header joist",
                    Which::Part,
                    [at(r[0], v, wj / 2.0), at(r[1], v, wj / 2.0)],
                    Z,
                );
            }
        }
    }
    // The joists at the centre distance from the outline's start.
    for k in 1.. {
        let a = amin + k as f64 * f.spacing;
        if a + tj / 2.0 > amax - EPS {
            break;
        }
        for (j, (lo, hi)) in spans_at(a).into_iter().enumerate() {
            if overlaps_rim(a, tj / 2.0, lo, hi) {
                continue;
            }
            let key = if j == 0 {
                format!("joist.{k}")
            } else {
                format!("joist.{k}.{j}")
            };
            let wide = nt as f64 * tj;
            let hole = holes.iter().position(|r| {
                a + tj / 2.0 > r[0] - wide + EPS
                    && a - tj / 2.0 < r[1] + wide - EPS
                    && lo < r[3]
                    && r[2] < hi
            });
            match hole {
                None => {
                    out.add(
                        key,
                        "joist",
                        Which::Part,
                        [at(a, lo, wj / 2.0), at(a, hi, wj / 2.0)],
                        Z,
                    );
                    runs.push(Run {
                        a,
                        half: tj / 2.0,
                        lo,
                        hi,
                    });
                }
                Some(h) => {
                    let r = holes[h];
                    if a - tj / 2.0 < r[0] - EPS || a + tj / 2.0 > r[1] + EPS {
                        continue;
                    }
                    // Tail joists from the rims to the headers.
                    for (side, from, to) in [("a", lo, r[2] - wide), ("b", r[3] + wide, hi)] {
                        if to - from > 1.0 {
                            out.add(
                                format!("{key}.tail.{side}"),
                                "tail joist",
                                Which::Part,
                                [at(a, from, wj / 2.0), at(a, to, wj / 2.0)],
                                Z,
                            );
                            runs.push(Run {
                                a,
                                half: tj / 2.0,
                                lo: from,
                                hi: to,
                            });
                        }
                    }
                }
            }
        }
    }
    // Rows of blocking between neighbouring runs.
    runs.sort_by(|x, y| x.a.total_cmp(&y.a));
    for row in 1..=f.blocking {
        let v = vmin + (vmax - vmin) * row as f64 / (f.blocking + 1) as f64;
        let across: Vec<&Run> = runs
            .iter()
            .filter(|r| r.lo <= v - tj / 2.0 + EPS && v + tj / 2.0 <= r.hi + EPS)
            .collect();
        for (k, pair) in across.windows(2).enumerate() {
            let from = pair[0].a + pair[0].half;
            let to = pair[1].a - pair[1].half;
            if to - from < 1.0 {
                continue;
            }
            let middle = (from + to) / 2.0;
            if holes
                .iter()
                .any(|r| middle > r[0] && middle < r[1] && v > r[2] && v < r[3])
            {
                continue;
            }
            out.add(
                format!("block.{row}.{k}"),
                "blocking",
                Which::Part,
                [at(from, v, wj / 2.0), at(to, v, wj / 2.0)],
                Z,
            );
        }
    }
    Ok(out)
}

// --- Roofs.

/// A roof frame's definition worked out.
#[derive(Debug, Clone)]
pub(crate) struct RoofGeo {
    name: String,
    kind: RoofKind,
    walls: [WallGeo; 2],
    spans: [WallSpan; 2],
    pitch: Option<f64>,
    spacing: f64,
    overhang: f64,
    rafter: FramePart,
    ridge: FramePart,
}

/// A roof's rafters at the centre distance along the walls (flush with
/// their common ends), each notched by a birdsmouth over each wall's top
/// plate and plumb cut at its ends, and a gable roof's ridge board.
fn roof_layout(r: &RoofGeo) -> Result<Layout, ModelError> {
    let mut out = Layout::default();
    let [wa, wb] = &r.walls;
    let u = wa.u;
    if dot(u, wb.u).abs() < 1.0 - 1e-3 {
        return Err(refuse(format!(
            "{} and {} are not parallel: a roof stands on two parallel walls",
            wa.name, wb.name
        )));
    }
    let d = sub(wb.start, wa.start);
    let s = level(sub(d, scale(u, dot(d, u))))
        .filter(|_| norm(sub(d, scale(u, dot(d, u)))) > 1.0)
        .ok_or_else(|| refuse(format!("{} and {} are on one line", wa.name, wb.name)))?;
    // Along the walls: where both run.
    let range = |w: &WallGeo, sp: &WallSpan| {
        let base = dot(w.start, u);
        let k = dot(w.u, u);
        let (x, y) = (base + k * sp.t0, base + k * sp.t1);
        (x.min(y), x.max(y))
    };
    let (a0, a1) = range(wa, &r.spans[0]);
    let (b0, b1) = range(wb, &r.spans[1]);
    let (r0, r1) = (a0.max(b0), a1.min(b1));
    let tr = r.rafter.thick();
    let dr = r.rafter.depth();
    if r1 - r0 < tr {
        return Err(refuse(format!(
            "{} and {} do not run side by side",
            wa.name, wb.name
        )));
    }
    let c = dot(d, s);
    let (da, db) = (wa.depth(), wb.depth());
    let (ha, hb) = (wa.top_z(), wb.top_z());
    let a_out = -da / 2.0;
    let a_in = da / 2.0;
    let b_in = c - db / 2.0;
    let b_out = c + db / 2.0;
    // In the plane across the walls: x along s from wall A's centre line.
    let origin = [wa.start[0], wa.start[1], 0.0];
    let origin = sub(origin, scale(u, dot(origin, u)));
    let point = |x: f64, z: f64, along: f64| {
        add(
            add(add(origin, scale(s, x)), scale(u, along)),
            [0.0, 0.0, z],
        )
    };
    let pitch = match (r.kind, r.pitch) {
        (_, Some(p)) => p,
        (RoofKind::Gable, None) => 30f64.to_radians(),
        (RoofKind::Mono, None) => {
            if (hb - ha).abs() < 1.0 {
                return Err(refuse(
                    "a mono-pitch roof on walls of one height needs its pitch",
                ));
            }
            (hb - ha).atan2(b_in - a_out)
        }
    };
    // A mono-pitch roof rises from the lower wall.
    let rising = match r.kind {
        RoofKind::Mono if hb < ha => -1.0,
        _ => 1.0,
    };
    let p = pitch.abs();
    if !(5f64.to_radians() - EPS..=75f64.to_radians() + EPS).contains(&p) {
        return Err(refuse(format!(
            "the pitch of {} degrees is not between 5 and 75",
            round(p.to_degrees())
        )));
    }
    if r.kind == RoofKind::Gable && (ha - hb).abs() > 0.5 {
        return Err(refuse(format!(
            "a gable roof stands on walls of one height: {} is {} mm high, {} {} mm",
            wa.name,
            round(ha),
            wb.name,
            round(hb)
        )));
    }
    let (sin, cos, tan) = (p.sin(), p.cos(), p.tan());
    let seat = da.min(db).min(dr / (3.0 * sin));
    let plumb = dr / cos;
    // The rafters' places along the walls.
    if r.spacing <= tr + EPS {
        return Err(refuse(format!(
            "the center distance of {} mm is not more than a rafter's {} mm",
            round(r.spacing),
            round(tr)
        )));
    }
    let mut places: Vec<(String, f64)> = vec![("start".to_owned(), r0 + tr / 2.0)];
    for k in 1.. {
        let centre = r0 + k as f64 * r.spacing;
        if centre + tr / 2.0 > r1 - tr + EPS {
            break;
        }
        places.push((k.to_string(), centre));
    }
    places.push(("end".to_owned(), r1 - tr / 2.0));
    // A birdsmouth over a wall's top plate: the box over its plate's column,
    // below its top.
    let e3 = cross(s, Z);
    let notch = |x0: f64, x1: f64, h: f64, along: f64, with: Option<ComponentUid>| Notch {
        kind: "birdsmouth",
        origin: sub(
            point(x0, h - plumb - (x1 - x0) * tan - 50.0, along),
            scale(e3, tr / 2.0 + 1.0),
        ),
        x: s,
        y: Z,
        size: [x1 - x0, plumb + (x1 - x0) * tan + 50.0, tr + 2.0],
        with,
    };
    // The depth of a birdsmouth across the rafter, where its bottom line
    // is `bottom(x)`.
    let depth_of = |bottom: &dyn Fn(f64) -> f64, x0: f64, x1: f64, h: f64| {
        [x0, x1]
            .iter()
            .map(|x| (h - bottom(*x)) * cos)
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let check = |name: &str, depth: f64, out: &mut Layout| {
        if depth <= EPS {
            out.warnings.push(format!(
                "{}: the rafters do not bear on {name}: they pass {} mm above its top plate",
                r.name,
                round(-depth / cos)
            ));
        } else if depth > dr / 3.0 + 0.5 {
            out.warnings.push(format!(
                "{}: the birdsmouth on {name} is {} mm deep, more than a third of the rafter's {} mm",
                r.name,
                round(depth),
                round(dr)
            ));
        }
    };
    let leans = |up: V3| [up, scale(up, -1.0)];
    match r.kind {
        RoofKind::Gable => {
            let tk = r.ridge.thick();
            let middle = (a_out + b_out) / 2.0;
            // Side A: up from its eave to the ridge board's face.
            let bottom_a = |x: f64| ha + (x - (a_out + seat)) * tan;
            let x_r = middle - tk / 2.0;
            let x_e = a_out - r.overhang;
            let up_a = add(scale(s, -sin), [0.0, 0.0, cos]);
            let up_b = add(scale(s, sin), [0.0, 0.0, cos]);
            let top_ridge = bottom_a(x_r) + plumb;
            check(&wa.name, depth_of(&bottom_a, a_out, a_in, ha), &mut out);
            for (key, along) in &places {
                let start = add(point(x_e, bottom_a(x_e), *along), scale(up_a, dr / 2.0));
                let end = sub(
                    point(x_r, bottom_a(x_r) + plumb, *along),
                    scale(up_a, dr / 2.0),
                );
                out.members.push(MemberSpec {
                    key: format!("rafter.a.{key}"),
                    role: "rafter",
                    which: Which::Part,
                    start,
                    end,
                    wide: up_a,
                    plumb: Some((p, leans(up_a))),
                    notches: vec![notch(a_out, a_in, ha, *along, wa.lead)],
                });
            }
            // Side B, the mirror.
            let bottom_b = |x: f64| hb + ((b_out - seat) - x) * tan;
            let x_rb = middle + tk / 2.0;
            let x_eb = b_out + r.overhang;
            check(&wb.name, depth_of(&bottom_b, b_in, b_out, hb), &mut out);
            for (key, along) in &places {
                let start = add(point(x_eb, bottom_b(x_eb), *along), scale(up_b, dr / 2.0));
                let end = sub(
                    point(x_rb, bottom_b(x_rb) + plumb, *along),
                    scale(up_b, dr / 2.0),
                );
                out.members.push(MemberSpec {
                    key: format!("rafter.b.{key}"),
                    role: "rafter",
                    which: Which::Part,
                    start,
                    end,
                    wide: up_b,
                    plumb: Some((p, leans(up_b))),
                    notches: vec![notch(b_in, b_out, hb, *along, wb.lead)],
                });
            }
            // The ridge board on edge, its top at the rafters' tops.
            let wk = r.ridge.depth();
            if wk + 0.5 < plumb {
                out.warnings.push(format!(
                    "{}: the ridge board is {} mm deep, less than the rafters' plumb cut of {} mm",
                    r.name,
                    round(wk),
                    round(plumb)
                ));
            }
            let z = top_ridge - wk / 2.0;
            out.members.insert(
                0,
                MemberSpec {
                    key: "ridge".to_owned(),
                    role: "ridge board",
                    which: Which::Ridge,
                    start: point(middle, z, r0),
                    end: point(middle, z, r1),
                    wide: Z,
                    plumb: None,
                    notches: Vec::new(),
                },
            );
        }
        RoofKind::Mono => {
            // From the lower wall (`low`) over the higher one; x runs from
            // the lower one's outer face towards the other.
            let (lo_out, lo_in, hi_in, hi_out, h_lo, h_hi, lo_lead, hi_lead, lo_name, hi_name) =
                if rising > 0.0 {
                    (
                        a_out, a_in, b_in, b_out, ha, hb, wa.lead, wb.lead, &wa.name, &wb.name,
                    )
                } else {
                    (
                        b_out, b_in, a_in, a_out, hb, ha, wb.lead, wa.lead, &wb.name, &wa.name,
                    )
                };
            let dir = if rising > 0.0 { 1.0 } else { -1.0 };
            // x' along the slope's run from the low wall's outer face.
            let run = |xp: f64| lo_out + dir * xp;
            let bottom = |xp: f64| h_lo + (xp - seat) * tan;
            let up = add(scale(s, -sin * dir), [0.0, 0.0, cos]);
            let (hi_in_p, hi_out_p) = ((hi_in - lo_out) * dir, (hi_out - lo_out) * dir);
            let (lo_in_p, lo_out_p) = ((lo_in - lo_out) * dir, 0.0);
            check(
                lo_name,
                depth_of(&bottom, lo_out_p, lo_in_p, h_lo),
                &mut out,
            );
            check(
                hi_name,
                depth_of(&bottom, hi_in_p, hi_out_p, h_hi),
                &mut out,
            );
            let x_e = -r.overhang;
            let x_h = hi_out_p + r.overhang;
            for (key, along) in &places {
                let start = add(point(run(x_e), bottom(x_e), *along), scale(up, dr / 2.0));
                let end = sub(
                    point(run(x_h), bottom(x_h) + plumb, *along),
                    scale(up, dr / 2.0),
                );
                let column = |x0: f64, x1: f64| (run(x0).min(run(x1)), run(x0).max(run(x1)));
                let (l0, l1) = column(lo_out_p, lo_in_p);
                let (h0, h1) = column(hi_in_p, hi_out_p);
                out.members.push(MemberSpec {
                    key: format!("rafter.{key}"),
                    role: "rafter",
                    which: Which::Part,
                    start,
                    end,
                    wide: up,
                    plumb: Some((p, leans(up))),
                    notches: vec![
                        notch(l0, l1, h_lo, *along, lo_lead),
                        notch(h0, h1, h_hi, *along, hi_lead),
                    ],
                });
            }
        }
    }
    Ok(out)
}

// --- Planning against the document.

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub trait FramesBuild<K: Kernel> {
    /// The lead of the frame a component is a member of, or whose own
    /// component it is (mitcad#522).
    fn frame_lead(&self, component: ComponentUid) -> Option<ComponentUid>;

    /// The own component of the frame a lead leads (mitcad#522); None for a
    /// flat frame saved before, or one whose component is gone.
    fn frame_component(&self, lead: ComponentUid) -> Option<ComponentUid>;

    /// `build_frame`: a wall, floor or roof frame.
    fn build_frame(&mut self, input: &FrameInput) -> Result<Framed, ModelError>;

    /// A frame's `build_edit`: its definition with the edit's fields over
    /// it (`config` the members' row), laid out again with the frames
    /// that depend on it.
    fn edit_frame(
        &mut self,
        component: ComponentUid,
        edit: &FrameInput,
        config: Option<&String>,
    ) -> Result<Framed, ModelError>;

    /// The `build_preview` of a frame: its members as `build_frame` (or a
    /// frame's edit, `component` given) would lay them out.
    fn preview_frame(
        &self,
        input: &FrameInput,
        component: Option<ComponentUid>,
    ) -> Result<Value, ModelError>;
}

impl<K: Kernel> FramesBuild<K> for Document<K> {
    fn frame_lead(&self, component: ComponentUid) -> Option<ComponentUid> {
        let a = self.assembly();
        if let Some(lead) = a.component(component)?.build.as_ref().and_then(|b| b.frame) {
            return Some(lead);
        }
        a.components
            .iter()
            .find(|c| self.frame_component(c.uid) == Some(component))
            .map(|c| c.uid)
    }

    fn frame_component(&self, lead: ComponentUid) -> Option<ComponentUid> {
        let a = self.assembly();
        let framing = a.component(lead)?.build.as_ref()?.framing.as_ref()?;
        let c: ComponentUid = serde_json::from_value(framing.get("component")?.clone()).ok()?;
        a.component(c).is_some().then_some(c)
    }

    fn build_frame(&mut self, input: &FrameInput) -> Result<Framed, ModelError> {
        let kind = input
            .kind
            .ok_or_else(|| refuse("a frame's kind is wall, floor or roof"))?;
        if input.part.is_none() {
            return Err(refuse("a frame needs its timber (part)"));
        }
        let mut world = self.known_frames();
        let mut def = input.clone();
        def.kind = Some(kind);
        def.fastener = def.fastener.map(|f| self.kept_fastener(&f));
        // Named first, so that a refusal names it (mitcad#536).
        match &def.name {
            Some(name) if world.iter().any(|k| k.def.name.as_deref() == Some(name)) => {
                return Err(refuse(format!("a frame is already named {name}")));
            }
            Some(_) => {}
            None => def.name = Some(self.free_frame_name(kind, &world)),
        }
        let junction = self.square_start(&mut def, &world)?;
        // A roof's walls and a wall's joins are kept as the frames' leads.
        if let Some(walls) = &mut def.walls {
            for w in walls.iter_mut() {
                *w = self.lead_of_ref(w)?.to_string();
            }
        }
        if let Some(joins) = &mut def.joins {
            for j in [&mut joins.start, &mut joins.end].into_iter().flatten() {
                *j = self.lead_of_ref(j)?.to_string();
            }
        }
        world.push(Known::new(def));
        let target = world.len() - 1;
        let affected = self.affected(target, None, &world);
        let mut framed = self.lay_out(&world, &affected, None, Vec::new())?;
        if let Some(j) = &junction {
            framed.warnings.extend(j.warnings.iter().cloned());
        }
        framed.junction = junction;
        Ok(framed)
    }

    fn edit_frame(
        &mut self,
        component: ComponentUid,
        edit: &FrameInput,
        config: Option<&String>,
    ) -> Result<Framed, ModelError> {
        let lead = self.frame_lead(component).ok_or_else(|| {
            refuse(format!(
                "{} is in no timber frame",
                self.assembly().name(component)
            ))
        })?;
        let mut world = self.known_frames();
        let target = world
            .iter()
            .position(|k| k.lead == Some(lead))
            .ok_or_else(|| refuse("the frame's definition is gone"))?;
        let before = world[target].def.clone();
        let mut def = self.edited_def(&before, edit)?;
        def.fastener = def.fastener.map(|f| self.kept_fastener(&f));
        if let Some(config) = config
            && let Some(part) = &mut def.part
        {
            part.config = Some(config.clone());
        }
        world[target].def = def;
        let affected = self.affected(target, Some(&before), &world);
        let mut changed = vec!["frame".to_owned()];
        if config.is_some() {
            changed.push("config".to_owned());
        }
        self.lay_out(&world, &affected, Some(before), changed)
    }

    fn preview_frame(
        &self,
        input: &FrameInput,
        component: Option<ComponentUid>,
    ) -> Result<Value, ModelError> {
        let mut world = self.known_frames();
        let mut junction = None;
        let target = match component {
            Some(c) => {
                let lead = self
                    .frame_lead(c)
                    .ok_or_else(|| refuse("the part is in no timber frame"))?;
                let i = world
                    .iter()
                    .position(|k| k.lead == Some(lead))
                    .ok_or_else(|| refuse("the frame's definition is gone"))?;
                world[i].def = self.edited_def(&world[i].def.clone(), input)?;
                i
            }
            None => {
                let mut def = input.clone();
                // Named first, so that a refusal names it (mitcad#536).
                if def.name.is_none() {
                    def.name = Some(self.free_frame_name(def.kind(), &world));
                }
                junction = match self.square_start(&mut def, &world) {
                    Ok(j) => j,
                    Err(e) => return Ok(refused_preview(&def, &e, None, None, Vec::new())),
                };
                world.push(Known::new(def));
                world.len() - 1
            }
        };
        // A wall's length as worked out (mitcad#536: the auto end's).
        let length = if world[target].def.kind() == FrameKind::Wall {
            self.wall_geo(&world[target]).ok().map(|g| round6(g.length))
        } else {
            None
        };
        let warnings: Vec<String> = junction
            .iter()
            .flat_map(|j| j.warnings.iter().cloned())
            .collect();
        let plan = match self.plan_frame(&world[target], &world) {
            Ok(plan) => plan,
            Err(e) => {
                return Ok(refused_preview(
                    &world[target].def,
                    &e,
                    junction.as_ref(),
                    length,
                    warnings,
                ));
            }
        };
        let mut roles: BTreeMap<&str, usize> = BTreeMap::new();
        for m in &plan.members {
            *roles.entry(m.spec.role).or_default() += 1;
        }
        let host = self.host_placement();
        let warnings: Vec<String> = plan.warnings.iter().cloned().chain(warnings).collect();
        Ok(json!({
            "place": "frame",
            "kind": plan.kind.as_str(),
            "name": plan.name,
            "count": plan.members.len(),
            "length": length,
            // A wall's plates made of pieces, each joint over a stud
            // (mitcad#536): along the wall from its start.
            "splices": plan.splices.iter().map(Splice::to_json).collect::<Vec<_>>(),
            "roles": roles,
            "members": plan.members.iter().map(|m| json!({
                "member": m.spec.key,
                "role": m.spec.role,
                "start": host.apply_point(m.spec.start),
                "end": host.apply_point(m.spec.end),
                "length": norm(sub(m.spec.end, m.spec.start)),
                // Its section's width along `x`, its thickness across.
                "x": host.apply_vector(m.plan.placement.apply_vector(crate::geometry::X)),
                "section": [plan.parts[&m.spec.which].w, plan.parts[&m.spec.which].t],
            })).collect::<Vec<_>>(),
            "warnings": warnings,
            "problems": [],
            "junction": junction.as_ref().map(StartJunction::to_json),
        }))
    }
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub(crate) trait FramesCrateOps<K: Kernel> {
    /// A part of a frame: timber with a section.
    fn frame_part(&self, part: &PartInput) -> Result<FramePart, ModelError>;

    /// The name of the frame a component is a member of (mitcad#522: the
    /// cut list's rows by frame).
    fn frame_name_of(&self, component: ComponentUid) -> Option<String>;

    /// Where a frame's definition is in the BUILD host (mitcad#522): its
    /// component's placement there; the identity for a flat frame.
    fn frame_place(&self, component: Option<ComponentUid>) -> Transform;

    /// A frame's members in the order they were made.
    fn frame_members(&self, lead: ComponentUid) -> Vec<ComponentUid>;

    /// The fasteners driven at junctions of a frame's members (each
    /// junction's first, which takes the others with it).
    fn frame_fasteners(&self, lead: ComponentUid) -> Vec<ComponentUid>;

    /// The frames of the design.
    fn known_frames(&self) -> Vec<Known>;

    /// The lead of the frame a reference names (any member of it).
    fn lead_of_ref(&self, text: &str) -> Result<ComponentUid, ModelError>;

    fn mm(&self, value: &Option<ValueInput>, default: f64) -> Result<f64, ModelError>;

    /// A wall's definition worked out.
    fn wall_geo(&self, known: &Known) -> Result<WallGeo, ModelError>;

    /// The walls of a world of frames (those that cannot be worked out are
    /// left out).
    fn walls_of(&self, world: &[Known]) -> Vec<WallGeo>;

    fn floor_geo(&self, known: &Known) -> Result<FloorGeo, ModelError>;

    fn roof_geo(&self, known: &Known, world: &[Known]) -> Result<RoofGeo, ModelError>;

    /// Lays a frame out and plans its members.
    fn plan_frame(&self, target: &Known, world: &[Known]) -> Result<FramePlan, ModelError>;

    /// The frames a change of `target` lays out again: itself, the walls
    /// it butts against (before and after the change), the walls that butt
    /// against it, and the roofs on any of these walls.
    fn affected(&self, target: usize, before: Option<&FrameInput>, world: &[Known]) -> Vec<usize>;

    /// A free frame name: `Wall 1`, `Wall 2`, ...
    fn free_frame_name(&self, kind: FrameKind, world: &[Known]) -> String;

    /// A wall started on another wall's side (`square`, mitcad#493): its
    /// start and end made square to that wall, as its `corner` has it.
    /// Butting (mitcad#510): a start on that wall's side starts it there,
    /// its centre line at the point's place along the wall, kept off its
    /// ends by half its own depth (`side`); a start on one of its end faces
    /// starts it beyond that end, its side against the end face and its
    /// start flush with that wall's far side, still joined to it (`end`,
    /// `joins.start_on: end`). Lapped (left out, mitcad#493): a start near
    /// one of that wall's ends (within half its stud spacing, at least its
    /// depth) starts it at that end's corner, an L, else at the click, a T;
    /// both on that wall's centre line. On the side its place along that
    /// wall is `offset` from `from` when either is given, else the click's,
    /// snapped (mitcad#514): flush with an end near it (butting within its
    /// own depth; lapped the L's reach), centred near the middle. At that
    /// wall's base, it runs to the side clicked (the end's side when the
    /// click is on the centre line; at an end face the end's side first),
    /// as far as `end` is along it, else to the first wall its run meets
    /// (mitcad#515, unless `auto_end` is false), else `length`.
    fn square_start(
        &self,
        def: &mut FrameInput,
        world: &[Known],
    ) -> Result<Option<StartJunction>, ModelError>;

    /// A wall started on another body's face (case B, mitcad#524): the
    /// clicked body is not changed and not linked; everything comes from
    /// the geometry. On the face's outward side, square from it (its
    /// horizontal normal) for a vertical face, else refused; its base on
    /// the floor under the click (the first body below it that the line
    /// down enters: a slab's top), else at the face's bottom edge; along
    /// the face at the click, or `offset` from `from` (the face's end A,
    /// its left end seen from in front, end B or its middle), snapped as on
    /// a timber wall's side (mitcad#514). A thin end face (narrower than the
    /// body is deep behind it) makes a corner (Butt, or Lapped plates, which
    /// has nothing to lap: its side against the face, running to the click's
    /// side, its start flush with the body's far side) or, with Straight, a
    /// wall in line beyond it. Its length as a start on a wall frame's:
    /// `end`, else the first wall frame or body face its run meets, else
    /// `length`.
    fn face_start(
        &self,
        def: &mut FrameInput,
        face: &StartFace,
        typed: (Option<SideFrom>, Option<ValueInput>),
        snap: Option<f64>,
        auto_end: Option<bool>,
        world: &[Known],
    ) -> Result<StartJunction, ModelError>;

    /// What a wall's end butts against (mitcad#515, mitcad#524): what its
    /// run met, else the wall an end clicked on names.
    fn end_met(&self, met: &Option<RunMet>, def: &FrameInput, world: &[Known]) -> Option<EndMet>;

    /// The nearest a wall's run from `start` along `d` meets (mitcad#515,
    /// mitcad#524): a wall frame (`wall_met`), or a body's face (`face_met`)
    /// nearer than it; `skip` the wall it starts on, `own` the body it
    /// starts on (met only beyond its own face, mitcad#536), `depth` the
    /// new wall's.
    fn run_met(
        &self,
        start: V3,
        d: V3,
        size: (f64, f64, f64),
        skip: Option<ComponentUid>,
        own: Option<BodyUid>,
        world: &[Known],
    ) -> Option<RunMet>;

    /// The nearest face of a body a wall's run meets (mitcad#524): the
    /// bodies shown in the design other than timber frames' members (their
    /// walls are `wall_met`'s), met beyond `least` from its start where a
    /// line of the run enters the body. The lines (mitcad#536): a quarter,
    /// half and three quarters of its height up and 10 mm under its top
    /// (over a door in the wall ahead), each along its centre line and a
    /// millimetre inside each of its sides (`depth` apart: a post or the
    /// end of a wall only its side meets). The body it starts on (`own`)
    /// counts beyond its own face (a millimetre on), so that the far wall
    /// of a room modelled as one body ends it.
    fn face_met(
        &self,
        start: V3,
        d: V3,
        size: (f64, f64, f64),
        own: Option<BodyUid>,
    ) -> Option<RunMet>;

    /// The nearest wall frame a wall's run from `start` along `d` meets
    /// (mitcad#515) and the distance to its near face: one it crosses (not
    /// running along it), its centre line between that wall's end faces
    /// (not merely touching it at a corner), beyond `least`, their heights
    /// overlapping (`height` the new wall's); `skip` the wall it starts on.
    fn wall_met(
        &self,
        start: V3,
        d: V3,
        least: f64,
        height: f64,
        skip: Option<ComponentUid>,
        world: &[Known],
    ) -> Option<(WallGeo, f64)>;

    /// An edit over a definition: a wall's `length` moves its end along it,
    /// joins and walls are kept as leads.
    fn edited_def(&self, before: &FrameInput, edit: &FrameInput) -> Result<FrameInput, ModelError>;

    /// Lays the `affected` frames of a world out and makes them in one undo
    /// step: the fasteners of the frames already there taken away, the
    /// members kept, added and removed, then every frame's fasteners
    /// driven.
    fn lay_out(
        &mut self,
        world: &[Known],
        affected: &[usize],
        before: Option<FrameInput>,
        changed: Vec<String>,
    ) -> Result<Framed, ModelError>;

    /// Drives the frame's fasteners (`count` at each, default 2; sized
    /// from the members unless `part` names a row) at every junction of a
    /// frame's members without a connector; returns how many junctions got
    /// them. A junction they are refused at is left as it is.
    fn fasten_frame(
        &mut self,
        lead: ComponentUid,
        part: &PartInput,
        count: Option<u32>,
        warnings: &mut Vec<String>,
    ) -> usize;

    /// Drives `count` wood fasteners (default 2) of `part` at each of the
    /// junctions (mitcad#479, mitcad#497); returns how many got them, the
    /// first ten fastener warnings added. A junction they are refused at is
    /// left as it is.
    fn fasten_junctions(
        &mut self,
        junctions: &[String],
        part: &PartInput,
        count: Option<u32>,
        warnings: &mut Vec<String>,
    ) -> usize;
}

impl<K: Kernel> FramesCrateOps<K> for Document<K> {
    fn frame_part(&self, part: &PartInput) -> Result<FramePart, ModelError> {
        let source = self.source_use(part)?;
        let label = source.def.label();
        if !source.meta.is_timber() {
            return Err(refuse(format!(
                "{label} is no timber: a frame is made of timber members"
            )));
        }
        if source.meta.role != BuildRole::Profile || source.meta.length.is_none() {
            return Err(refuse(format!("{label} is not cut to length")));
        }
        let [w, t] = source
            .meta
            .section_size()
            .ok_or_else(|| refuse(format!("{label} has no section (build.section)")))?;
        Ok(FramePart {
            input: part.clone(),
            source,
            w,
            t,
        })
    }

    fn frame_name_of(&self, component: ComponentUid) -> Option<String> {
        let a = self.assembly();
        let lead = a.component(component)?.build.as_ref()?.frame?;
        let framing = a.component(lead)?.build.as_ref()?.framing.as_ref()?;
        framing
            .get("frame")?
            .get("name")?
            .as_str()
            .map(str::to_owned)
    }

    fn frame_place(&self, component: Option<ComponentUid>) -> Transform {
        component
            .and_then(|c| self.placed_in_host(c))
            .map_or(Transform::IDENTITY, |path| self.path_transform(&path))
    }

    fn frame_members(&self, lead: ComponentUid) -> Vec<ComponentUid> {
        self.assembly()
            .components
            .iter()
            .filter(|c| c.build.as_ref().is_some_and(|b| b.frame == Some(lead)))
            .map(|c| c.uid)
            .collect()
    }

    fn frame_fasteners(&self, lead: ComponentUid) -> Vec<ComponentUid> {
        let members: BTreeSet<ComponentUid> = self.frame_members(lead).into_iter().collect();
        self.assembly()
            .components
            .iter()
            .filter(|c| {
                c.build.as_ref().is_some_and(|b| {
                    b.role == BuildRole::Fastener
                        && b.added_by.is_none()
                        && b.junction.as_deref().is_some_and(|j| {
                            j.split('|').all(|side| {
                                side.split(':')
                                    .next()
                                    .and_then(|c| c.parse::<ComponentUid>().ok())
                                    .is_some_and(|c| members.contains(&c))
                            })
                        })
                })
            })
            .map(|c| c.uid)
            .collect()
    }

    fn known_frames(&self) -> Vec<Known> {
        self.assembly()
            .components
            .iter()
            .filter_map(|c| {
                let record = c.build.as_ref()?;
                let def: FrameDef = serde_json::from_value(record.framing.clone()?).ok()?;
                let component = def
                    .component
                    .filter(|f| self.assembly().component(*f).is_some());
                Some(Known {
                    lead: Some(c.uid),
                    def: def.frame,
                    component,
                    place: self.frame_place(component),
                })
            })
            .collect()
    }

    fn lead_of_ref(&self, text: &str) -> Result<ComponentUid, ModelError> {
        let c = self.component_by_ref(text)?;
        self.frame_lead(c)
            .ok_or_else(|| refuse(format!("{text} is in no timber frame")))
    }

    fn mm(&self, value: &Option<ValueInput>, default: f64) -> Result<f64, ModelError> {
        value
            .as_ref()
            .map_or(Ok(default), |v| value_in(self, v, Unit::MM))
    }

    fn wall_geo(&self, known: &Known) -> Result<WallGeo, ModelError> {
        let def = &known.def;
        let name = def.name.clone().unwrap_or_else(|| "Wall".to_owned());
        // In the host: its component's placement there (mitcad#522).
        let start = known.point(
            def.start
                .ok_or_else(|| refuse(format!("{name} needs its start")))?,
        );
        let end = known.point(
            def.end
                .ok_or_else(|| refuse(format!("{name} needs its end")))?,
        );
        let u = level(sub(end, start))
            .ok_or_else(|| refuse(format!("{name}: its start and end are one point")))?;
        let mut length = dot(sub(end, start), u);
        if let Some(l) = &def.length {
            length = value_in(self, l, Unit::MM)?;
        }
        let part = def
            .part
            .as_ref()
            .ok_or_else(|| refuse(format!("{name} needs its timber (part)")))?;
        let stud = self.frame_part(part)?;
        let plate = match &def.plate {
            Some(p) => self.frame_part(p)?,
            None => stud.clone(),
        };
        let header = match &def.header {
            Some(p) => self.frame_part(p)?,
            None => stud.clone(),
        };
        let plates = def.plates.clone().unwrap_or_default();
        let count = |n: Option<u32>, which: &str| -> Result<usize, ModelError> {
            match n.unwrap_or(1) {
                n @ 1..=2 => Ok(n as usize),
                n => Err(refuse(format!("{name}: {n} {which} plates; one or two"))),
            }
        };
        let mut warnings = Vec::new();
        let mut joins = [None, None];
        if let Some(j) = &def.joins {
            for (k, text) in [&j.start, &j.end].into_iter().enumerate() {
                if let Some(text) = text {
                    match self.lead_of_ref(text) {
                        Ok(lead) => joins[k] = Some(lead),
                        Err(e) => warnings.push(format!("{name}: {e}")),
                    }
                }
            }
        }
        Ok(WallGeo {
            lead: known.lead,
            start,
            u,
            n: cross(Z, u),
            length,
            height: self.mm(&def.height, 2400.0)?,
            spacing: self.mm(&def.spacing, 600.0)?,
            bottom: count(plates.bottom, "bottom")?,
            top: count(plates.top, "top")?,
            stud,
            plate,
            header,
            openings: def.openings.clone().unwrap_or_default(),
            joins,
            butt: matches!(def.corner, Some(WallCorner::Butt | WallCorner::Straight)),
            start_on_end: def
                .joins
                .as_ref()
                .is_some_and(|j| j.start_on == Some(JoinOn::End)),
            warnings,
            name,
            load_bearing: def.load_bearing.unwrap_or(true),
        })
    }

    fn walls_of(&self, world: &[Known]) -> Vec<WallGeo> {
        world
            .iter()
            .filter(|k| k.def.kind() == FrameKind::Wall)
            .filter_map(|k| self.wall_geo(k).ok())
            .collect()
    }

    fn floor_geo(&self, known: &Known) -> Result<FloorGeo, ModelError> {
        let def = &known.def;
        let name = def.name.clone().unwrap_or_else(|| "Floor".to_owned());
        let outline: Vec<V3> = def
            .outline
            .clone()
            .filter(|o| o.len() >= 3)
            .ok_or_else(|| refuse(format!("{name} needs its outline (three points at least)")))?
            .into_iter()
            .map(|p| known.point(p))
            .collect();
        let part = def
            .part
            .as_ref()
            .ok_or_else(|| refuse(format!("{name} needs its timber (part)")))?;
        let joist = self.frame_part(part)?;
        let rim = match &def.rim {
            Some(p) => self.frame_part(p)?,
            None => joist.clone(),
        };
        let first = level(sub(outline[1], outline[0]))
            .ok_or_else(|| refuse(format!("{name}: its outline's first side has no length")))?;
        let v = match def.direction.map(|d| known.vector(d)).and_then(level) {
            Some(v) => v,
            None => {
                // Square to the first side, into the outline.
                let area: f64 = (0..outline.len())
                    .map(|i| cross2(outline[i], outline[(i + 1) % outline.len()]))
                    .sum();
                let left = cross(Z, first);
                if area >= 0.0 { left } else { scale(left, -1.0) }
            }
        };
        let a = cross(v, Z);
        Ok(FloorGeo {
            z0: outline[0][2],
            outline,
            a,
            v,
            spacing: self.mm(&def.spacing, 400.0)?,
            blocking: def.blocking.unwrap_or(0) as usize,
            trimmers: def.trimmers.unwrap_or(2) as usize,
            openings: def.openings.clone().unwrap_or_default(),
            joist,
            rim,
            name,
        })
    }

    fn roof_geo(&self, known: &Known, world: &[Known]) -> Result<RoofGeo, ModelError> {
        let def = &known.def;
        let name = def.name.clone().unwrap_or_else(|| "Roof".to_owned());
        let refs = def
            .walls
            .clone()
            .filter(|w| w.len() == 2)
            .ok_or_else(|| refuse(format!("{name} stands on two wall frames (walls)")))?;
        let walls = self.walls_of(world);
        let mut found = Vec::new();
        for text in &refs {
            let lead = self.lead_of_ref(text)?;
            let wall = walls
                .iter()
                .find(|w| w.lead == Some(lead))
                .cloned()
                .ok_or_else(|| refuse(format!("{text} is no wall frame")))?;
            found.push(wall);
        }
        let part = def
            .part
            .as_ref()
            .ok_or_else(|| refuse(format!("{name} needs its timber (part)")))?;
        let rafter = self.frame_part(part)?;
        let ridge = match &def.ridge {
            Some(p) => self.frame_part(p)?,
            None => rafter.clone(),
        };
        let spans = [wall_span(&found[0], &walls), wall_span(&found[1], &walls)];
        let pitch = match &def.pitch {
            Some(v) => Some(value_in(self, v, Unit::DEG)?),
            None => None,
        };
        let [a, b]: [WallGeo; 2] = found.try_into().expect("two walls");
        Ok(RoofGeo {
            kind: def.roof.unwrap_or_default(),
            walls: [a, b],
            spans,
            pitch,
            spacing: self.mm(&def.spacing, 600.0)?,
            overhang: self.mm(&def.overhang, 300.0)?,
            rafter,
            ridge,
            name,
        })
    }

    fn plan_frame(&self, target: &Known, world: &[Known]) -> Result<FramePlan, ModelError> {
        let kind = target.def.kind();
        let name = target
            .def
            .name
            .clone()
            .unwrap_or_else(|| kind.title().to_owned());
        let (layout, parts) = match kind {
            FrameKind::Wall => {
                let me = self.wall_geo(target)?;
                let mut walls: Vec<WallGeo> = self
                    .walls_of(world)
                    .into_iter()
                    .filter(|w| w.lead != target.lead)
                    .collect();
                walls.push(me.clone());
                let span = wall_span(&me, &walls);
                let parts = BTreeMap::from([
                    (Which::Part, me.stud.clone()),
                    (Which::Plate, me.plate.clone()),
                    (Which::Header, me.header.clone()),
                ]);
                (wall_layout(&me, &span)?, parts)
            }
            FrameKind::Floor => {
                let f = self.floor_geo(target)?;
                let parts =
                    BTreeMap::from([(Which::Part, f.joist.clone()), (Which::Rim, f.rim.clone())]);
                (floor_layout(&f)?, parts)
            }
            FrameKind::Roof => {
                let r = self.roof_geo(target, world)?;
                let parts = BTreeMap::from([
                    (Which::Part, r.rafter.clone()),
                    (Which::Ridge, r.ridge.clone()),
                ]);
                (roof_layout(&r)?, parts)
            }
        };
        let mut warnings = layout.warnings;
        let mut members = Vec::new();
        // From the host into the frame's own component.
        let to_frame = inverse(&target.place);
        for spec in layout.members {
            let part = &parts[&spec.which];
            let length = norm(sub(spec.end, spec.start));
            let least = part.least();
            if length < least - EPS {
                warnings.push(format!(
                    "{name}: the {} {} would be {} mm, shorter than its timber's least length of {} mm: left out",
                    spec.role,
                    spec.key,
                    round(length),
                    round(least)
                ));
                continue;
            }
            let input = PlaceInput {
                part: part.input.clone(),
                at: Some(PlaceAt {
                    point: Some(spec.start),
                    ..PlaceAt::default()
                }),
                to: Some(PointTo { point: spec.end }),
                ..PlaceInput::default()
            };
            let mut plan = self
                .plan_place(&input)
                .map_err(|e| refuse(format!("{name}: the {} {}: {e}", spec.role, spec.key)))?;
            // Its axes exactly as the frame wants them.
            let d = unit(sub(spec.end, spec.start)).unwrap_or(Z);
            let x = if part.w >= part.t {
                spec.wide
            } else {
                cross(d, spec.wide)
            };
            let end_a = plan.mounts.get("end_a").map_or([0.0; 3], |t| t.translation);
            let placement = crate::geometry::frame(spec.start, x, d)
                .after(&Transform::translation(scale(end_a, -1.0)));
            // Its occurrence in the frame's own component (mitcad#522), the
            // plan's placement in the host.
            plan.spot = Spot::Grounded(to_frame.after(&placement));
            plan.placement = placement;
            plan.points = None;
            let to_part = inverse(&placement);
            if let Some((angle, leans)) = &spec.plumb {
                let cut = |lean: V3| {
                    Tilt::towards(to_part.apply_vector(lean)).map(|tilt| EndCut {
                        angle: ValueInput::Number(*angle),
                        tilt: Some(tilt),
                    })
                };
                plan.own_ends = Ends {
                    a: cut(leans[0]),
                    b: cut(leans[1]),
                };
            }
            let tidy = |v: f64| ValueInput::Number((v * 1e6).round() / 1e6);
            let cuts = spec
                .notches
                .iter()
                .map(|n| {
                    let o = to_part.apply_point(n.origin);
                    PartCut {
                        kind: n.kind.to_owned(),
                        with: n.with,
                        junction: None,
                        origin: o.map(tidy),
                        x: to_part.apply_vector(n.x),
                        y: to_part.apply_vector(n.y),
                        size: n.size.map(tidy),
                        depth: None,
                    }
                })
                .collect();
            members.push(PlannedMember { spec, plan, cuts });
        }
        if members.is_empty() {
            return Err(refuse(format!("{name} has no members")));
        }
        // The members that stay: by their place, made of the same source.
        let mut keep = BTreeMap::new();
        if let Some(lead) = target.lead {
            for c in self.frame_members(lead) {
                let Some(record) = self.assembly().component(c).and_then(|c| c.build.as_ref())
                else {
                    continue;
                };
                let Some(key) = &record.member else { continue };
                let same = members.iter().any(|m| {
                    &m.spec.key == key
                        && !m.plan.source.new
                        && record.source == Some(m.plan.source.def.id)
                });
                if same {
                    keep.insert(key.clone(), c);
                }
            }
        }
        Ok(FramePlan {
            lead: target.lead,
            component: target.component,
            flat: target.lead.is_some() && target.component.is_none(),
            name,
            kind,
            def: target.def.clone(),
            parts,
            members,
            keep,
            warnings,
            splices: layout.splices,
        })
    }

    fn affected(&self, target: usize, before: Option<&FrameInput>, world: &[Known]) -> Vec<usize> {
        let mut out = vec![target];
        let index_of = |lead: ComponentUid| world.iter().position(|k| k.lead == Some(lead));
        let joins_of = |def: &FrameInput| -> Vec<ComponentUid> {
            def.joins
                .iter()
                .flat_map(|j| [&j.start, &j.end])
                .flatten()
                .filter_map(|t| self.lead_of_ref(t).ok())
                .collect()
        };
        let me = &world[target];
        if me.def.kind() == FrameKind::Wall {
            let mut joined = joins_of(&me.def);
            if let Some(before) = before {
                joined.extend(joins_of(before));
            }
            for lead in joined {
                if let Some(i) = index_of(lead) {
                    out.push(i);
                }
            }
            if let Some(lead) = me.lead {
                for (i, k) in world.iter().enumerate() {
                    if k.def.kind() == FrameKind::Wall && joins_of(&k.def).contains(&lead) {
                        out.push(i);
                    }
                }
            }
            let walls: Vec<ComponentUid> = out.iter().filter_map(|i| world[*i].lead).collect();
            for (i, k) in world.iter().enumerate() {
                if k.def.kind() != FrameKind::Roof {
                    continue;
                }
                let on = k
                    .def
                    .walls
                    .iter()
                    .flatten()
                    .filter_map(|t| self.lead_of_ref(t).ok())
                    .any(|l| walls.contains(&l));
                if on {
                    out.push(i);
                }
            }
        }
        let mut seen = BTreeSet::new();
        out.retain(|i| seen.insert(*i));
        out
    }

    fn free_frame_name(&self, kind: FrameKind, world: &[Known]) -> String {
        (1..)
            .map(|n| format!("{} {n}", kind.title()))
            .find(|n| {
                !world
                    .iter()
                    .any(|k| k.def.name.as_deref() == Some(n.as_str()))
            })
            .expect("a free name exists")
    }

    fn square_start(
        &self,
        def: &mut FrameInput,
        world: &[Known],
    ) -> Result<Option<StartJunction>, ModelError> {
        // Not kept: only how it starts (mitcad#514, mitcad#515).
        let (typed_from, typed_offset) = (def.from.take(), def.offset.take());
        let (snap, auto_end) = (def.snap.take(), def.auto_end.take());
        let face = def.face.take();
        if def.square.take() != Some(true) {
            return Ok(None);
        }
        let name = def.name.clone().unwrap_or_else(|| "The wall".to_owned());
        if def.kind() != FrameKind::Wall {
            return Err(refuse("only a wall starts square on another wall (square)"));
        }
        // A timber wall frame's member first (case A, as before); another
        // body's face otherwise (case B, mitcad#524).
        let started_on = def.joins.as_ref().and_then(|j| j.start.clone());
        if let (None, Some(face)) = (&started_on, face) {
            let typed = (typed_from, typed_offset);
            return self
                .face_start(def, &face, typed, snap, auto_end, world)
                .map(Some);
        }
        let text = started_on.ok_or_else(|| {
            refuse(format!(
                "{name}: square needs the wall it starts on (joins.start) or a face (face)"
            ))
        })?;
        let lead = self.lead_of_ref(&text)?;
        let known = world
            .iter()
            .find(|k| k.lead == Some(lead) && k.def.kind() == FrameKind::Wall)
            .ok_or_else(|| refuse(format!("{text} is no wall frame")))?;
        let x = self.wall_geo(known)?;
        // Where its plates end now (after its own corners): its end faces.
        let ends = {
            let span = wall_span(&x, &self.walls_of(world));
            [span.t0, span.t1]
        };
        let click = def
            .start
            .ok_or_else(|| refuse(format!("{name} needs its start")))?;
        let rel = sub(click, x.start);
        let (t, a) = (dot(rel, x.u), dot(rel, x.n));
        if t < ends[0] - x.depth() || t > ends[1] + x.depth() {
            return Err(refuse(format!(
                "{name}'s start is not on {}'s side: {} mm along it, beyond its ends",
                x.name,
                round(t)
            )));
        }
        let end_side = def
            .end
            .map(|e| dot(sub(e, x.start), x.n))
            .filter(|b| b.abs() > EPS)
            .map(f64::signum);
        let click_side = (a.abs() > EPS).then_some(a.signum());
        // Straight (mitcad#524) starts on an end face in line with it, else
        // butts.
        let straight = def.corner == Some(WallCorner::Straight);
        let butt = straight || def.corner == Some(WallCorner::Butt);
        // Where along it as typed (mitcad#514), else the click.
        let typed = typed_from.is_some() || typed_offset.is_some();
        // A butting start on an end face: within half a millimetre of it and
        // inside the wall's depth, or beyond the end (not with From and
        // Offset typed: they place it on the side).
        const ON: f64 = 0.5;
        let inside = a.abs() < x.depth() / 2.0 - ON;
        let on_end = if !butt || typed {
            None
        } else if t < ends[0] + ON && (inside || t < ends[0] - ON) {
            Some(0)
        } else if t > ends[1] - ON && (inside || t > ends[1] + ON) {
            Some(1)
        } else {
            None
        };
        let side = match on_end {
            Some(_) => end_side.or(click_side),
            None => click_side.or(end_side),
        }
        .unwrap_or(1.0);
        let mut d = scale(x.n, side);
        // Its own depth.
        let depth = match &def.part {
            Some(part) => self.frame_part(part)?.depth(),
            None => return Err(refuse(format!("{name} needs its timber (part)"))),
        };
        let half = depth / 2.0;
        let mut warnings = Vec::new();
        // The junction, the wall's start, how far it must run at least, and
        // what its place along that wall is from (mitcad#514).
        let (kind, end, at, start, least, from, offset) = match on_end {
            Some(k) if straight => {
                // In line beyond the end face (mitcad#524): its centre line
                // that wall's, its start on the end face.
                d = scale(x.u, if k == 0 { -1.0 } else { 1.0 });
                let start = x.at(ends[k], 0.0, 0.0);
                (
                    "straight",
                    Some(["a", "b"][k]),
                    ends[k],
                    start,
                    0.0,
                    None,
                    0.0,
                )
            }
            Some(k) => {
                // Its side against the end face, from the far side.
                let out = if k == 0 { -1.0 } else { 1.0 };
                let at = ends[k] + out * half;
                let start = x.at(at, 0.0, -side * x.depth() / 2.0);
                ("end", Some(["a", "b"][k]), at, start, x.depth(), None, 0.0)
            }
            None => {
                // Offsets as a side placement's (mitcad#417): from end A to
                // its near face, from end B to its far face, from the middle
                // (of that wall's end faces) to its centre line.
                let middle = (ends[0] + ends[1]) / 2.0;
                let at_of = |from: SideFrom, offset: f64| match from {
                    SideFrom::A => ends[0] + offset + half,
                    SideFrom::B => ends[1] - offset - half,
                    SideFrom::Middle => middle + offset,
                };
                let (mut from, mut offset) = if typed {
                    (typed_from.unwrap_or_default(), self.mm(&typed_offset, 0.0)?)
                } else {
                    // The click, snapped as a side placement's: flush with
                    // an end within its depth of it (lapped: within the L
                    // corner's reach of that wall's centre line's end, half
                    // its stud spacing, at least its depth), centred within
                    // a quarter of its depth of the middle, else measured
                    // from the nearer end.
                    let least = snap.filter(|s| s.is_finite() && *s > 0.0).unwrap_or(0.0);
                    let (to_a, to_b) = (t - half - ends[0], ends[1] - t - half);
                    let (near_a, near_b) = if butt {
                        let reach = depth.max(least);
                        (to_a < reach, to_b < reach)
                    } else {
                        let reach = (x.spacing / 2.0).max(x.depth()).min(x.length / 2.0);
                        (t <= reach, t >= x.length - reach)
                    };
                    if near_a {
                        (SideFrom::A, 0.0)
                    } else if near_b {
                        (SideFrom::B, 0.0)
                    } else if (t - middle).abs() < (depth / 4.0).max(least) {
                        (SideFrom::Middle, 0.0)
                    } else {
                        let (from, offset) = if to_b < to_a {
                            (SideFrom::B, to_b)
                        } else {
                            (SideFrom::A, to_a)
                        };
                        // A click is no finer than a thousandth of a mm.
                        (from, (offset * 1000.0).round() / 1000.0)
                    }
                };
                // Never past an end: its faces within that wall's end faces,
                // else flush with that end.
                let mut at = at_of(from, offset);
                let (lo, hi) = (ends[0] + half, ends[1] - half);
                let kept = if lo > hi { middle } else { at.min(hi).max(lo) };
                if (kept - at).abs() > 1e-9 {
                    let flush = if kept <= middle {
                        SideFrom::A
                    } else {
                        SideFrom::B
                    };
                    if typed {
                        warnings.push(format!(
                            "{name}: flush with {}'s end {} instead",
                            x.name,
                            if flush == SideFrom::A { "A" } else { "B" }
                        ));
                    }
                    (from, offset, at) = (flush, 0.0, kept);
                }
                let least = x.depth() / 2.0;
                if butt {
                    // Its faces within that wall's end faces: no overhang.
                    let start = x.at(at, 0.0, 0.0);
                    ("side", None, at, start, least, Some(from), offset)
                } else if from != SideFrom::Middle && offset.abs() <= 1e-9 {
                    // Flush with an end: the L corner there.
                    let (end, at) = if from == SideFrom::A {
                        ("a", 0.0)
                    } else {
                        ("b", x.length)
                    };
                    let start = x.at(at, 0.0, 0.0);
                    ("L", Some(end), at, start, least, Some(from), 0.0)
                } else {
                    let start = x.at(at, 0.0, 0.0);
                    ("T", None, at, start, least, Some(from), offset)
                }
            }
        };
        // Without an end, the first wall its run meets ends it (mitcad#515),
        // or another body's face nearer (mitcad#524).
        let height = self.mm(&def.height, 2400.0)?;
        let met = if def.end.is_none() && auto_end != Some(false) {
            self.run_met(start, d, (least, height, depth), Some(lead), None, world)
        } else {
            None
        };
        let length = match (&met, def.end, &def.length) {
            (Some(m), _, _) => m.at,
            (None, Some(e), _) => dot(sub(e, start), d),
            (None, None, Some(l)) => value_in(self, l, Unit::MM)?,
            (None, None, None) => {
                return Err(refuse(format!("{name} needs its end or its length")));
            }
        };
        if length <= least + EPS {
            return Err(refuse(if kind == "straight" {
                format!("{name} would be {} mm long", round(length))
            } else if on_end.is_some() {
                format!(
                    "{name} would be {} mm long from {}'s far side, not beyond its other side",
                    round(length),
                    x.name
                )
            } else {
                format!(
                    "{name} would end {} mm from {}'s center line, not beyond its side",
                    round(length),
                    x.name
                )
            }));
        }
        def.start = Some(start);
        def.end = Some(add(start, scale(d, length)));
        def.length = None;
        // At an end it stays joined to that wall, whose end face butts
        // against its side (laid out again with it); its end joins the wall
        // its run met.
        if let Some(joins) = &mut def.joins {
            joins.start_on = on_end.map(|_| JoinOn::End);
            if kind == "straight" {
                // In line, its end on that wall's end face: neither is
                // changed for the other (mitcad#524).
                joins.start = None;
                joins.start_on = None;
            }
            if let Some(m) = &met {
                joins.end = m.wall.map(|l| l.to_string());
            }
        }
        let to = self.end_met(&met, def, world);
        let run = length - if on_end.is_some() { 0.0 } else { least };
        Ok(Some(StartJunction {
            kind,
            corner: def.corner.unwrap_or(WallCorner::Lapped).as_str(),
            wall: x.name.clone(),
            frame: Some(lead),
            end,
            at,
            length,
            from,
            offset,
            to,
            run,
            base: None,
            warnings,
        }))
    }

    fn face_start(
        &self,
        def: &mut FrameInput,
        face: &StartFace,
        (typed_from, typed_offset): (Option<SideFrom>, Option<ValueInput>),
        snap: Option<f64>,
        auto_end: Option<bool>,
        world: &[Known],
    ) -> Result<StartJunction, ModelError> {
        const FAR: f64 = 1e5;
        let name = def.name.clone().unwrap_or_else(|| "The wall".to_owned());
        let body: BodyUid = face
            .body
            .parse()
            .map_err(|_| refuse(format!("{name}: {} is no body", face.body)))?;
        let instances = self.instances();
        let path_of = |i: &mitcad_model::InstanceView| {
            i.path
                .iter()
                .map(|o| o.to_string())
                .collect::<Vec<_>>()
                .join("/")
        };
        let instance = instances
            .iter()
            .filter(|i| i.body == body)
            .find(|i| face.occurrence.as_deref().is_none_or(|o| path_of(i) == o))
            .ok_or_else(|| refuse(format!("{name}: no body {} in the design", face.body)))?;
        let label = instance.name.clone();
        let shape = self
            .body_shape(body)
            .ok_or_else(|| refuse(format!("{name}: {label} has no shape here")))?;
        let face_name: FaceName = face
            .face
            .parse()
            .map_err(|_| refuse(format!("{name}: {} is no face name", face.face)))?;
        let host = inverse(&self.host_placement());
        let to_host = host.after(&instance.transform);
        let kernel = self.kernel();
        let plane = kernel
            .face_plane(shape, &face_name)
            .map_err(|e| refuse(format!("{name}: the face of {label}: {e}")))?;
        let n3 = unit(to_host.apply_vector(plane.normal))
            .ok_or_else(|| refuse(format!("{name}: the face of {label} has no normal")))?;
        if n3[2].abs() > 1e-3 {
            return Err(refuse(format!(
                "{name} starts from a vertical face: the face of {label} clicked is not vertical"
            )));
        }
        let n = level(n3).expect("a vertical face's normal is horizontal");
        let t = cross(Z, n);
        let o = to_host.apply_point(plane.origin);
        // The face's extent along it and its bottom edge: its box, else the
        // body's.
        let index = kernel
            .faces(shape)
            .ok()
            .and_then(|faces| faces.iter().position(|f| f.names.contains(&face.face)));
        let face_box = index.and_then(|i| kernel.face_boxes(shape).ok()?.get(i).copied());
        let body_box = kernel.bounding_box(shape).ok().flatten();
        let corners: Vec<V3> = match (face_box, body_box) {
            (Some([min, max]), _) => BoundingBox { min, max }.corners().collect(),
            (None, Some(b)) => b.corners().collect(),
            (None, None) => Vec::new(),
        }
        .into_iter()
        .map(|c| to_host.apply_point(c))
        .collect();
        if corners.is_empty() {
            return Err(refuse(format!("{name}: the face of {label} has no extent")));
        }
        let along = |p: V3| dot(p, t);
        let (t0, t1) = corners
            .iter()
            .map(|c| along(*c))
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
                (a.min(v), b.max(v))
            });
        let bottom = corners.iter().map(|c| c[2]).fold(f64::INFINITY, f64::min);
        // How deep the body is behind the face: an end face is narrower.
        let behind = body_box.map_or(0.0, |b| {
            b.corners()
                .map(|c| dot(sub(o, to_host.apply_point(c)), n))
                .fold(0.0, f64::max)
        });
        let width = t1 - t0;
        let end_face = width < behind - EPS;
        let click = def
            .start
            .ok_or_else(|| refuse(format!("{name} needs its start")))?;
        let c = along(click);
        let depth = match &def.part {
            Some(part) => self.frame_part(part)?.depth(),
            None => return Err(refuse(format!("{name} needs its timber (part)"))),
        };
        if c < t0 - depth || c > t1 + depth {
            return Err(refuse(format!(
                "{name}'s start is not on the face of {label}: beyond its ends"
            )));
        }
        let half = depth / 2.0;
        let middle = (t0 + t1) / 2.0;
        let straight = def.corner == Some(WallCorner::Straight);
        // On the face at `a` along it.
        let on_face = |a: f64| add(o, scale(t, a - along(o)));
        let mut warnings = Vec::new();
        let (kind, d, at, start, least, from, offset) = if end_face && !straight {
            // A corner: its side against the end face, towards the side
            // the end (else the click) is on, from the body's far side.
            let end_side = def
                .end
                .map(|e| along(e) - middle)
                .filter(|b| b.abs() > EPS)
                .map(f64::signum);
            let click_side = ((c - middle).abs() > EPS).then_some((c - middle).signum());
            let side = end_side.or(click_side).unwrap_or(1.0);
            let first = if side > 0.0 { t0 } else { t1 };
            let start = add(on_face(first), scale(n, half));
            let d = scale(t, side);
            ("face_end", d, first, start, width, None, 0.0)
        } else if end_face {
            // In line beyond it: its centre line the face's middle.
            ("face_straight", n, middle, on_face(middle), 0.0, None, 0.0)
        } else {
            // Square from the face (mitcad#514's places): from end A to its
            // near face, from end B to its far face, from the middle to its
            // centre line.
            let typed = typed_from.is_some() || typed_offset.is_some();
            let at_of = |from: SideFrom, offset: f64| match from {
                SideFrom::A => t0 + offset + half,
                SideFrom::B => t1 - offset - half,
                SideFrom::Middle => middle + offset,
            };
            let (mut from, mut offset) = if typed {
                (typed_from.unwrap_or_default(), self.mm(&typed_offset, 0.0)?)
            } else {
                let least = snap.filter(|s| s.is_finite() && *s > 0.0).unwrap_or(0.0);
                let (to_a, to_b) = (c - half - t0, t1 - c - half);
                let reach = depth.max(least);
                if to_a < reach {
                    (SideFrom::A, 0.0)
                } else if to_b < reach {
                    (SideFrom::B, 0.0)
                } else if (c - middle).abs() < (depth / 4.0).max(least) {
                    (SideFrom::Middle, 0.0)
                } else if to_b < to_a {
                    (SideFrom::B, (to_b * 1000.0).round() / 1000.0)
                } else {
                    (SideFrom::A, (to_a * 1000.0).round() / 1000.0)
                }
            };
            let mut at = at_of(from, offset);
            let (lo, hi) = (t0 + half, t1 - half);
            let kept = if lo > hi { middle } else { at.min(hi).max(lo) };
            if (kept - at).abs() > 1e-9 {
                let flush = if kept <= middle {
                    SideFrom::A
                } else {
                    SideFrom::B
                };
                if typed {
                    warnings.push(format!(
                        "{name}: flush with the end {} of the face of {label} instead",
                        if flush == SideFrom::A { "A" } else { "B" }
                    ));
                }
                (from, offset, at) = (flush, 0.0, kept);
            }
            ("face", n, at, on_face(at), 0.0, Some(from), offset)
        };
        // Its base: the floor under the click, else the face's bottom edge.
        let probe = add(on_face(c), scale(n, 1.0));
        let probe = [probe[0], probe[1], click[2]];
        let mut floor: Option<(f64, String)> = None;
        for i in instances.iter().filter(|i| i.visible) {
            let Some(s) = self.body_shape(i.body) else {
                continue;
            };
            let to_body = inverse(&host.after(&i.transform));
            let Ok(crossings) = kernel.segment_crossings(
                s,
                to_body.apply_point(probe),
                to_body.apply_point(sub(probe, scale(Z, FAR))),
            ) else {
                continue;
            };
            if let Some((f, _)) = crossings.into_iter().find(|(_, enters)| *enters) {
                let z = probe[2] - f * FAR;
                if floor.as_ref().is_none_or(|(b, _)| z > *b) {
                    floor = Some((z, i.name.clone()));
                }
            }
        }
        let base = match floor {
            Some((z, body)) => WallBase {
                z,
                on: "floor",
                body: Some(body),
            },
            None if bottom.is_finite() => WallBase {
                z: bottom,
                on: "face",
                body: Some(label.clone()),
            },
            None => WallBase {
                z: 0.0,
                on: "origin",
                body: None,
            },
        };
        let start = [start[0], start[1], base.z];
        // Its length: the end clicked, else what its run meets, else typed.
        let height = self.mm(&def.height, 2400.0)?;
        let met = if def.end.is_none() && auto_end != Some(false) {
            self.run_met(start, d, (least, height, depth), None, Some(body), world)
        } else {
            None
        };
        let length = match (&met, def.end, &def.length) {
            (Some(m), _, _) => m.at,
            (None, Some(e), _) => dot(sub(e, start), d),
            (None, None, Some(l)) => value_in(self, l, Unit::MM)?,
            (None, None, None) => {
                return Err(refuse(format!("{name} needs its end or its length")));
            }
        };
        if length <= least + EPS {
            return Err(refuse(if kind == "face_end" {
                format!(
                    "{name} would be {} mm long from the far side of {label}, not beyond its other side",
                    round(length)
                )
            } else {
                format!(
                    "{name} would be {} mm long from the face of {label}",
                    round(length)
                )
            }));
        }
        def.start = Some(start);
        def.end = Some(add(start, scale(d, length)));
        def.length = None;
        if let Some(lead) = met.as_ref().and_then(|m| m.wall) {
            def.joins.get_or_insert_with(FrameJoins::default).end = Some(lead.to_string());
        }
        let to = self.end_met(&met, def, world);
        Ok(StartJunction {
            kind,
            corner: if straight { "straight" } else { "butt" },
            wall: label,
            frame: None,
            end: None,
            at: at - t0,
            length,
            from,
            offset,
            to,
            run: length,
            base: Some(base),
            warnings,
        })
    }

    fn end_met(&self, met: &Option<RunMet>, def: &FrameInput, world: &[Known]) -> Option<EndMet> {
        match met {
            Some(m) => Some(EndMet {
                name: m.name.clone(),
                frame: m.wall,
                auto: true,
            }),
            None => def
                .joins
                .as_ref()
                .and_then(|j| j.end.as_deref())
                .and_then(|text| self.lead_of_ref(text).ok())
                .and_then(|l| {
                    let k = world.iter().find(|k| k.lead == Some(l))?;
                    let name = k.def.name.clone().unwrap_or_else(|| "Wall".to_owned());
                    Some(EndMet {
                        name,
                        frame: Some(l),
                        auto: false,
                    })
                }),
        }
    }

    fn run_met(
        &self,
        start: V3,
        d: V3,
        (least, height, depth): (f64, f64, f64),
        skip: Option<ComponentUid>,
        own: Option<BodyUid>,
        world: &[Known],
    ) -> Option<RunMet> {
        let wall = self
            .wall_met(start, d, least, height, skip, world)
            .map(|(w, s)| RunMet {
                at: s,
                name: w.name.clone(),
                wall: w.lead,
            });
        let face = self.face_met(start, d, (least, height, depth), own);
        match (wall, face) {
            (Some(w), Some(f)) if f.at < w.at - EPS => Some(f),
            (Some(w), _) => Some(w),
            (None, f) => f,
        }
    }

    fn face_met(
        &self,
        start: V3,
        d: V3,
        (least, height, depth): (f64, f64, f64),
        own: Option<BodyUid>,
    ) -> Option<RunMet> {
        const FAR: f64 = 1e5;
        // Crossings of the body it starts on nearer than this are its own
        // face's.
        const OWN: f64 = 1.0;
        let to_host = inverse(&self.host_placement());
        let across = cross(Z, d);
        let side = (depth / 2.0 - 1.0).max(0.0);
        let mut lines: Vec<V3> = Vec::new();
        for z in [0.25 * height, 0.5 * height, 0.75 * height, height - 10.0] {
            if z <= 0.0 {
                continue;
            }
            for a in [0.0, side, -side] {
                lines.push(add(
                    add(add(start, scale(Z, z)), scale(across, a)),
                    scale(d, least + EPS),
                ));
            }
        }
        let mut best: Option<RunMet> = None;
        for instance in self.instances() {
            if !instance.visible {
                continue;
            }
            let framed = self
                .assembly()
                .component(instance.component)
                .and_then(|c| c.build.as_ref())
                .is_some_and(|b| b.frame.is_some());
            if framed {
                continue;
            }
            let Some(shape) = self.body_shape(instance.body) else {
                continue;
            };
            let to_body = inverse(&to_host.after(&instance.transform));
            let segments: Vec<(V3, V3)> = lines
                .iter()
                .map(|from| {
                    (
                        to_body.apply_point(*from),
                        to_body.apply_point(add(*from, scale(d, FAR))),
                    )
                })
                .collect();
            // All the lines at once, else one by one.
            let crossings = match self.kernel().segments_crossings(shape, &segments) {
                Ok(all) => all,
                Err(_) => segments
                    .iter()
                    .map(|(a, b)| {
                        self.kernel()
                            .segment_crossings(shape, *a, *b)
                            .unwrap_or_default()
                    })
                    .collect(),
            };
            let skip = if Some(instance.body) == own { OWN } else { 0.0 };
            for line in crossings {
                let Some(at) = line
                    .into_iter()
                    .filter(|(_, enters)| *enters)
                    .map(|(f, _)| f * FAR)
                    .find(|at| *at >= skip)
                else {
                    continue;
                };
                let at = least + EPS + at;
                if best.as_ref().is_none_or(|b| at < b.at) {
                    best = Some(RunMet {
                        at,
                        name: instance.name.clone(),
                        wall: None,
                    });
                }
            }
        }
        best
    }

    fn wall_met(
        &self,
        start: V3,
        d: V3,
        least: f64,
        height: f64,
        skip: Option<ComponentUid>,
        world: &[Known],
    ) -> Option<(WallGeo, f64)> {
        let walls = self.walls_of(world);
        let mut best: Option<(WallGeo, f64)> = None;
        for o in &walls {
            if o.lead.is_none_or(|l| Some(l) == skip) {
                continue;
            }
            let along = dot(d, o.n);
            if along.abs() < 0.1 {
                continue;
            }
            // Its face on the start's side.
            let rel = dot(sub(start, o.start), o.n);
            let half = o.depth() / 2.0;
            if rel.abs() < half + EPS {
                continue;
            }
            let s = (rel.signum() * half - rel) / along;
            if s <= least + EPS || best.as_ref().is_some_and(|(_, b)| *b <= s) {
                continue;
            }
            let span = wall_span(o, &walls);
            let tj = dot(sub(add(start, scale(d, s)), o.start), o.u);
            if tj <= span.t0 + EPS || tj >= span.t1 - EPS {
                continue;
            }
            if o.start[2] >= start[2] + height - EPS || o.top_z() <= start[2] + EPS {
                continue;
            }
            best = Some((o.clone(), s));
        }
        best
    }

    fn edited_def(&self, before: &FrameInput, edit: &FrameInput) -> Result<FrameInput, ModelError> {
        let mut def = before.merged(edit);
        def.kind = before.kind;
        if let (Some(length), Some(start), Some(end)) = (&edit.length, def.start, def.end) {
            let l = value_in(self, length, Unit::MM)?;
            if l <= EPS {
                return Err(refuse("a wall's length must be greater than zero"));
            }
            let u = level(sub(end, start)).unwrap_or([1.0, 0.0, 0.0]);
            def.end = Some(add(start, scale(u, l)));
        }
        def.length = None;
        if let Some(joins) = &mut def.joins {
            for j in [&mut joins.start, &mut joins.end].into_iter().flatten() {
                *j = self.lead_of_ref(j)?.to_string();
            }
        }
        if let Some(walls) = &mut def.walls {
            for w in walls.iter_mut() {
                *w = self.lead_of_ref(w)?.to_string();
            }
        }
        Ok(def)
    }

    fn lay_out(
        &mut self,
        world: &[Known],
        affected: &[usize],
        before: Option<FrameInput>,
        changed: Vec<String>,
    ) -> Result<Framed, ModelError> {
        // The frame itself must lay out; one that depends on it and no
        // longer does stays as it is.
        let mut plans = Vec::new();
        let mut kept_back = Vec::new();
        for (n, i) in affected.iter().enumerate() {
            match self.plan_frame(&world[*i], world) {
                Ok(plan) => plans.push(plan),
                Err(e) if n > 0 => kept_back.push(format!(
                    "{} was not laid out again: {e}",
                    world[*i].def.name.clone().unwrap_or_default()
                )),
                Err(e) => return Err(e),
            }
        }
        let mut removals: Vec<RemovePlan> = Vec::new();
        for plan in &plans {
            let Some(lead) = plan.lead else { continue };
            for f in self.frame_fasteners(lead) {
                removals.push(self.plan_remove(f)?);
            }
            for c in self.frame_members(lead) {
                if !plan.keep.values().any(|k| *k == c) {
                    removals.push(self.plan_remove(c)?);
                }
            }
        }
        let active = self.build_host();
        self.state().editable(active)?;
        let first = &plans[0];
        let label = match before {
            Some(_) => format!("Edit {}", first.name),
            None => format!("Place {} frame {}", first.kind.as_str(), first.name),
        };
        let depth = self.undo_depth();
        let made = self.apply(|state| {
            for removal in &removals {
                apply_remove(state, removal)?;
            }
            let mut made: Vec<MadeFrame> = Vec::new();
            for plan in &plans {
                let first = made.first().map(|m| m.lead);
                made.push(apply_frame(state, plan, active, first)?);
            }
            Ok((label.clone(), made))
        })?;
        // The fasteners, each frame's anew, in its own component
        // (mitcad#522): their joints with its members there.
        let mut warnings: Vec<String> = plans.iter().flat_map(|p| p.warnings.clone()).collect();
        warnings.extend(kept_back);
        let mut fasteners = 0;
        for (plan, m) in plans.iter().zip(&made) {
            if let Some(part) = &plan.def.fastener {
                let before = self.set_build_host_override(m.component);
                fasteners +=
                    self.fasten_frame(m.lead, part, plan.def.fastener_count, &mut warnings);
                self.set_build_host_override(before);
            }
        }
        self.merge_undo(depth, &label);
        let (lead, members, component) = (made[0].lead, made[0].members.clone(), made[0].component);
        let mut seen = BTreeSet::new();
        warnings.retain(|w| seen.insert(w.clone()));
        Ok(Framed {
            lead: Some(lead),
            name: first.name.clone(),
            kind: first.kind.as_str(),
            members,
            fasteners: if first.def.fastener.is_some() {
                fasteners
            } else {
                0
            },
            warnings,
            relaid: made[1..].iter().map(|m| m.lead).collect(),
            changed,
            junction: None,
            component,
            splices: first.splices.iter().map(Splice::to_json).collect(),
        })
    }

    fn fasten_frame(
        &mut self,
        lead: ComponentUid,
        part: &PartInput,
        count: Option<u32>,
        warnings: &mut Vec<String>,
    ) -> usize {
        let members: BTreeSet<ComponentUid> = self.frame_members(lead).into_iter().collect();
        let list: Vec<ComponentUid> = members.iter().copied().collect();
        let junctions: Vec<String> = self
            .build_junctions(Some(&list))
            .into_iter()
            .filter(|j| members.contains(&j.a.component) && members.contains(&j.b.component))
            .filter(|j| j.connectors.is_empty())
            .map(|j| j.id)
            .collect();
        self.fasten_junctions(&junctions, part, count, warnings)
    }

    fn fasten_junctions(
        &mut self,
        junctions: &[String],
        part: &PartInput,
        count: Option<u32>,
        warnings: &mut Vec<String>,
    ) -> usize {
        let mut done = 0;
        let mut own = Vec::new();
        for id in junctions {
            let mut fields = json!({"junction": id, "part": part});
            if let Some(n) = count {
                fields["count"] = json!(n.max(1));
            }
            let input: crate::ConnectInput = match serde_json::from_value(fields) {
                Ok(input) => input,
                Err(_) => continue,
            };
            match self.build_connect(&input) {
                Ok(placed) => {
                    done += 1;
                    own.extend(placed.warnings);
                }
                Err(_) => continue,
            }
        }
        const SHOWN: usize = 10;
        let more = own.len().saturating_sub(SHOWN);
        warnings.extend(own.into_iter().take(SHOWN));
        if more > 0 {
            warnings.push(format!("and {more} more fastener warning(s)"));
        }
        done
    }
}

/// A frame's refused `build_preview` (mitcad#536): which frame it is, what
/// its start and its auto end worked out (`junction`, a wall's `length`)
/// and the reason (`problems`), no members.
fn refused_preview(
    def: &FrameInput,
    problem: &ModelError,
    junction: Option<&StartJunction>,
    length: Option<f64>,
    warnings: Vec<String>,
) -> Value {
    json!({
        "place": "frame",
        "kind": def.kind().as_str(),
        "name": def.name,
        "count": 0,
        "roles": {},
        "members": [],
        "warnings": warnings,
        "problems": [problem.to_string()],
        "junction": junction.map(StartJunction::to_json),
        "length": length,
        "splices": [],
    })
}

/// The cuts a frame makes in its members (laid out anew with it): a
/// rafter's birdsmouth, a lapped corner's half lap (no junction).
fn frame_cut<P>(c: &PartCut<P>) -> bool {
    c.kind == "birdsmouth" || (c.kind == "half_lap" && c.junction.is_none())
}

/// Makes a planned frame in `state`: the sources it adds, its own component
/// (a new frame's, named after it, in the BUILD host `active`; mitcad#522),
/// its members kept (placed anew), made or notched in that component (a
/// flat frame saved before: in the host), its definition on its lead.
/// Returns the lead, the members and the component. `made_first`: the lead
/// of the frame made first in the step, for the cuts with the frame being
/// made.
fn apply_frame(
    state: &mut DocState,
    plan: &FramePlan,
    active: ComponentUid,
    made_first: Option<ComponentUid>,
) -> Result<MadeFrame, ModelError> {
    // Where its members go: its own component, made with a new frame.
    let component = match plan.component.filter(|c| state.assembly.exists(*c)) {
        _ if plan.flat => None,
        Some(c) => {
            // Renamed with the frame, when that name is free.
            let named = state
                .assembly
                .component(c)
                .is_some_and(|d| d.name == plan.name);
            if !named
                && state.check_component_name(&plan.name, c).is_ok()
                && let Some(d) = state.assembly.component_mut(c)
            {
                d.name = plan.name.clone();
            }
            Some(c)
        }
        None => {
            let name = state.free_component_name(&plan.name);
            let c = state.assembly.create(Some(&name), None);
            state.assembly.place(c, active, Transform::IDENTITY);
            Some(c)
        }
    };
    let container = component.unwrap_or(active);
    // The sources, each once.
    let mut ids: BTreeMap<Which, SourceId> = BTreeMap::new();
    for (which, part) in &plan.parts {
        let source = &part.source;
        let id = if !source.new {
            source.def.id
        } else if let Some(id) = state.library_sources.find(
            &source.def.library.library,
            &source.def.library.component,
            &source.def.library.rev,
        ) {
            id
        } else {
            let mut def = source.def.clone();
            def.id = state.library_sources.next_id();
            let id = def.id;
            state.library_sources.0.push(def);
            id
        };
        ids.insert(*which, id);
    }
    let as_source = |which: Which, input: &PartInput| PartInput {
        source: Some(ids[&which]),
        config: input.config.clone(),
        ..PartInput::default()
    };
    let mut def = plan.def.clone();
    for (which, slot) in [
        (Which::Part, &mut def.part),
        (Which::Plate, &mut def.plate),
        (Which::Header, &mut def.header),
        (Which::Rim, &mut def.rim),
        (Which::Ridge, &mut def.ridge),
    ] {
        if let Some(input) = slot.as_ref()
            && ids.contains_key(&which)
        {
            *slot = Some(as_source(which, input));
        }
    }
    let mut made = Vec::new();
    for m in &plan.members {
        let mut p = m.plan.clone();
        p.source.def.id = ids[&m.spec.which];
        p.source.new = false;
        let kept = plan
            .keep
            .get(&m.spec.key)
            .copied()
            .filter(|c| state.assembly.exists(*c));
        let c = match kept {
            Some(c) => {
                replace_member(state, &p, container, c)?;
                c
            }
            None => {
                p.name = Some(state.free_component_name(&format!("{} {}", plan.name, m.spec.role)));
                make_part(state, &p, container)?
                    .component
                    .expect("a member is made")
            }
        };
        let had = state
            .assembly
            .component(c)
            .and_then(|c| c.build.as_ref())
            .and_then(|b| b.part);
        if let Some(part) = had {
            // A cut with the frame being made: with its lead.
            let notches: Vec<PartCut<ValueInput>> = m
                .cuts
                .iter()
                .cloned()
                .map(|mut c| {
                    c.with = c.with.or(made_first);
                    c
                })
                .collect();
            let has = state.entry(part).is_some_and(|e| match &e.def {
                mitcad_model::features::FeatureDef::DerivedPart(d) => d.cuts.iter().any(frame_cut),
                _ => false,
            });
            if has || !notches.is_empty() {
                edit_part(state, part, |d| {
                    d.cuts.retain(|c| !frame_cut(c));
                    d.cuts.extend(notches);
                })?;
            }
        }
        made.push((c, m.spec.key.clone()));
    }
    let lead = plan
        .lead
        .filter(|l| made.iter().any(|(c, _)| c == l))
        .unwrap_or(made[0].0);
    let value = serde_json::to_value(FrameDef {
        frame: def,
        count: made.len(),
        component,
    })
    .map_err(|e| refuse(e.to_string()))?;
    for (c, key) in &made {
        if let Some(record) = state
            .assembly
            .component_mut(*c)
            .and_then(|c| c.build.as_mut())
        {
            record.frame = Some(lead);
            record.member = Some(key.clone());
            record.framing = (*c == lead).then(|| value.clone());
            record.set = None;
            record.spacing = None;
        }
    }
    Ok(MadeFrame {
        lead,
        members: made,
        component,
    })
}

/// When a frame's lead goes alone, the next member that stays takes the
/// frame over.
pub(crate) fn hand_over_frame(state: &mut DocState, going: &BTreeSet<ComponentUid>) {
    let leads: Vec<(ComponentUid, Value)> = going
        .iter()
        .filter_map(|c| {
            let record = state.assembly.component(*c)?.build.as_ref()?;
            Some((*c, record.framing.clone()?))
        })
        .collect();
    for (lead, framing) in leads {
        let members: Vec<ComponentUid> = state
            .assembly
            .components
            .iter()
            .filter(|c| c.build.as_ref().is_some_and(|b| b.frame == Some(lead)))
            .map(|c| c.uid)
            .collect();
        let Some(heir) = members.iter().copied().find(|c| !going.contains(c)) else {
            continue;
        };
        for c in members {
            if let Some(r) = state
                .assembly
                .component_mut(c)
                .and_then(|c| c.build.as_mut())
            {
                r.frame = Some(heir);
                r.framing = (c == heir).then(|| framing.clone());
            }
        }
    }
}
