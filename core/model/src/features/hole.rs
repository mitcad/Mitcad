// SPDX-License-Identifier: MIT
//! Hole: drilled holes cut into the participant bodies: simple, counterbore,
//! countersink or counterdrill, straight or tapered, with a drill point or
//! a flat bottom, to a depth, through all or up to an object, at points on
//! a planar face or at sketch points, optionally tapped with a thread whose
//! size sets the bore (cosmetic, or modelled). A taper pipe thread (NPT,
//! ISO 7-1 `Rc`; mitcad#733) with `taper` is drilled at the hole's
//! diameter (the tap drill) and tapers the bore from the hole's start,
//! 1:16 on the diameter from the thread's minor diameter at its opening,
//! cosmetic or modelled; without `taper` (holes saved before) its bore is
//! the size's minor diameter, as it was. A press-fit hole for printed
//! parts (mitcad#1517) is a hex or square hole or a bore with crush ribs,
//! its flats or rib tips the press fit's interference under the hole's
//! diameter, the pin's, with a flat bottom.
//!
//! Mitcad's definitions (experiments K24-K28 test them on .f3d designs):
//! the depth runs to the shoulder and the drill point is extra; a
//! counterbore's depth and the hole's depth are both measured from the
//! start; the countersink angle is the full angle; a
//! tapped hole's bore is the thread's basic minor diameter D1; the holes go
//! against the face or sketch normal, into the material, and `flip` turns
//! them round.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::extrude::{Operation, apply_operation, check_participants, participant_bodies};
use super::geom_ref::{GeomRef, Want};
use super::hole_sizes::ClearanceFit;
use super::place_at::PlaceAt;
use super::reference::{Across, ExtentObject, normalized};
use super::thread::{
    CustomSize, ThreadEnds, ThreadSize, check_clearance, check_custom, check_starts,
    clearance_value, fitted_clearance, flank_from_fit, size_of, starts_value, thread_clearance,
    thread_part, thread_starts,
};
use super::thread_table::ThreadStandard;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    ToolUse, is_false,
};
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::kernel::{
    Axis, Bound, BridgeMode, Cylinder, HoleBridging, HoleEnd, HolePrintShape, HoleShape, HoleSpec,
    Kernel, ThreadSpec, ThreadTaper,
};
use crate::parameters::ParamId;
use crate::sketch::geometry::Curve2;
use crate::topo::{EdgeName, FaceName};
use mitcad_model_base::internal::ids::curve_serde;
use mitcad_vec3::{cross, dot};

/// The default drill point, 118 degrees.
pub const DEFAULT_TIP_ANGLE: f64 = 118.0 * std::f64::consts::PI / 180.0;

/// A wall of a tapped hole whose modelled thread has its crest off the wall
/// (a radial clearance), as the wall was before the thread was cut into it
/// (mitcad#436). Such a thread leaves no piece of the wall, or only pieces
/// outside its part (mitcad#628). The hole makes the wall and threads it in
/// one step, so no body before the hole has it; the threads query measures
/// the thread on this.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadedWall {
    pub body: BodyUid,
    pub wall: FaceName,
    pub cylinder: Cylinder,
}

/// Where the holes are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HolePlacement<P = ParamId> {
    /// Points on a planar face, each put on its plane
    /// (`setPositionByPoint` in .f3d designs): one hole per point. On a
    /// cylindrical face each point is put on the cylinder and the hole goes
    /// along its radius (mitcad#719).
    Face {
        body: BodyUid,
        face: FaceName,
        points: Vec<[f64; 3]>,
    },
    /// One hole on a planar face, `offset1` from the line of `edge1` and
    /// `offset2` from the line of `edge2`, on the face's side of both
    /// (`setPositionByPlaneAndOffsets` in .f3d designs).
    FaceOffsets {
        body: BodyUid,
        face: FaceName,
        edge1: Box<EdgeName>,
        offset1: P,
        edge2: Box<EdgeName>,
        offset2: P,
    },
    /// Points of a sketch, against its normal
    /// (`setPositionBySketchPoints` in .f3d designs).
    SketchPoints {
        sketch: FeatureUid,
        points: Vec<SketchPoint>,
    },
    /// Points projected onto a plane (an origin plane, a construction
    /// plane or a planar face: `setPositionByPoint` on a plane in .f3d
    /// designs); the holes start on it, against its normal, in the
    /// participants that reach it.
    Plane {
        plane: GeomRef,
        points: Vec<[f64; 3]>,
    },
    /// One hole placed by picked geometry (mitcad#656, `place_at.rs`):
    /// along the axis into a shaft or hole, against the normal into a face.
    At { at: Box<PlaceAt<P>> },
    /// One hole on a cylindrical face (a shaft's side) whose axis point
    /// lies `offset` from `plane` (a planar face or a plane the axis
    /// crosses), measured along the plane's normal, on the face's side of
    /// the plane; on the side of the axis where `toward` lies; along the
    /// radius into the material (`setPositionByPlaneAndOffsets` on a
    /// cylindrical face in .f3d designs, mitcad#719). Across a plane square
    /// to the axis the offset is the distance along the axis.
    CylinderOffset {
        body: BodyUid,
        face: FaceName,
        plane: GeomRef,
        offset: P,
        toward: [f64; 3],
    },
}

/// A point of a sketch: coordinates in it, the centre of a circle, arc or
/// ellipse of it (`"c5"`), or a sketch point entity (`"p3"`, as
/// `setPositionBySketchPoints` in .f3d designs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SketchPoint {
    At([f64; 2]),
    Center(#[serde(with = "curve_serde")] EntityUid),
    Point(#[serde(with = "mitcad_sketch::internal::point_serde")] EntityUid),
}

/// The hole types; the countersink angle is the cone's full angle. A
/// counterdrill is a counterbore whose floor is a cone of the full `angle`
/// narrowing to the hole (FreeCAD's counterdrilled holes, mitcad#4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HoleKind<P = ParamId> {
    Simple,
    Counterbore {
        diameter: P,
        depth: P,
    },
    Countersink {
        diameter: P,
        angle: P,
    },
    Counterdrill {
        diameter: P,
        depth: P,
        angle: P,
    },
    /// A hex pocket for a nut about the hole (mitcad#1518), for printed
    /// parts: `across_flats` wide (the `hole_sizes` query's `nut_pocket`,
    /// ISO 4032's nut with a clearance), `depth` deep from `offset` below
    /// the hole's start (0 when left out: open at the start, where the
    /// screw comes out), turned by `angle` about the axis; `slot` runs it
    /// out sideways along its first flat to that far from the axis, so
    /// that the nut slides in from the side
    /// ([`crate::kernel::HoleShape::NutPocket`]).
    NutPocket {
        across_flats: P,
        depth: P,
        #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
        angle: Option<P>,
        #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
        offset: Option<P>,
        #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
        slot: Option<P>,
    },
}

// Not derived: that would need P: Default.
#[allow(clippy::derivable_impls)]
impl<P> Default for HoleKind<P> {
    fn default() -> Self {
        Self::Simple
    }
}

impl<P> HoleKind<P> {
    pub fn is_simple(&self) -> bool {
        matches!(self, Self::Simple)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HoleExtent<P = ParamId> {
    /// The depth of the cylindrical part from the start; the drill point
    /// is beyond it.
    Distance { depth: P },
    /// Through every participant body, with a flat end.
    ThroughAll,
    /// The cylindrical part ends where each hole's axis first meets the
    /// object (moved by `offset`); the drill point is beyond it.
    ToObject {
        object: Box<ExtentObject>,
        #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
        offset: Option<P>,
    },
}

/// A tapped hole's thread (`setToTappedHole` in .f3d designs). It sets the bore to
/// the size's basic minor diameter; a modelled one is cut into the wall. A
/// taper pipe thread with `taper` keeps the hole's diameter (the tap drill)
/// and tapers the bore from the hole's start (mitcad#733).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoleThread<P = ParamId> {
    #[serde(default, skip_serializing_if = "is_iso")]
    pub standard: ThreadStandard,
    /// None for a custom thread (mitcad#893).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub designation: String,
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub right_handed: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub modeled: bool,
    /// `length` from `offset` below the hole's start; the whole wall
    /// without.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub length: Option<P>,
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub offset: Option<P>,
    /// A modelled thread's extra clearance, radial and perpendicular to
    /// each flank (as the thread feature's); 0 without.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub radial_clearance: Option<P>,
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub flank_clearance: Option<P>,
    /// A taper pipe thread (`npt`, `iso_taper`) that tapers the bore from
    /// the hole's start, the hole drilled at its `diameter`, the tap drill
    /// (mitcad#733). Without it (holes saved before) the bore is the size's
    /// minor diameter and nothing tapers.
    #[serde(default, skip_serializing_if = "is_false")]
    pub taper: bool,
    /// A custom thread's size (`standard` `custom`, mitcad#893): its major
    /// diameter and pitch, as the thread feature's; the bore is its minor
    /// diameter.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub custom: Option<CustomSize<P>>,
    /// The number of starts of a modelled thread (mitcad#893), as the
    /// thread feature's; 1 without (or the designation's).
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub starts: Option<P>,
    /// How a modelled thread's ends are cut (mitcad#1300, mitcad#1307), as
    /// the thread feature's ([`ThreadEnds`]): `run_out` runs the grooves
    /// out into a countersink or counterdrill cone that opens away from
    /// the thread, `flat` stops them flat with a lip there. A stored
    /// modelled thread always has it, except in design files written
    /// before it existed, which are read as `flat`; a command without it
    /// means `run_out` ([`HoleDef::settle_thread_ends`]). Ignored on a
    /// cosmetic thread.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub ends: Option<ThreadEnds>,
}

/// A clearance hole's screw size and fit (mitcad#903): `{"size": "M6",
/// "fit": "normal"}`. It names where the hole's values came from (the
/// `hole_sizes` table: ISO 273's clearance hole, the head's counterbore or
/// countersink) so that the Hole panel shows them again; the hole itself
/// is cut at its own `diameter` and `kind`, plain values as any hole's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoleClearance {
    pub size: String,
    #[serde(default)]
    pub fit: ClearanceFit,
}

/// A heat-set insert's hole (mitcad#1518): `{"size": "M3"}`, a size of the
/// `hole_sizes` query's insert series. As a clearance hole's size, it names
/// where the hole's values came from (the series' hole diameter and depth),
/// so that the Hole panel shows them again; the hole is cut at its own
/// `diameter` and `extent`, plain values as any hole's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoleInsert {
    pub size: String,
}

/// The shape of a press-fit hole (mitcad#1517).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PressFitShape {
    /// A hexagon whose flats grip the pin.
    Hex,
    /// A square whose flats grip the pin.
    Square,
    /// A round bore with small ribs that the pin or bearing crushes as it
    /// goes in.
    CrushRibs,
}

/// A press fit for printed parts (mitcad#1517): `{"shape": "hex",
/// "interference": "press_fit_interference"}`. The hole's `diameter` is the
/// nominal diameter of the pin or bearing that goes in; a hex's or square's
/// flats, or the crush ribs' tips, touch a circle `interference` smaller on
/// the diameter (the printed fit parameter `press_fit_interference`, which
/// the Hole panel offers). The section runs along the whole hole, which has
/// a flat bottom.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HolePressFit<P = ParamId> {
    pub shape: PressFitShape,
    pub interference: P,
    /// Crush ribs only: their number, 3 when left out.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub ribs: Option<P>,
    /// Crush ribs only: how far each stands out of the bore, 0.3 mm when
    /// left out; the bore is the tips' diameter plus twice this.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub rib_height: Option<P>,
    /// The section turned about the axis, 0 when left out
    /// ([`crate::kernel::HolePressFit`] names where it starts).
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub angle: Option<P>,
}

/// How a hole is printed when its axis lies parallel to the print bed
/// (mitcad#1515): a round hole's roof sags and comes out out-of-round and
/// undersized, so its upper arc gives way to a teardrop's point or a flat
/// roof that bridges ([`crate::kernel::HolePrintShape`]). The bed is the
/// participant body's print bed face (`set_print_bed_face`), the design's
/// −Z without one; holes whose axis does not lie parallel to it stay round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrintShape {
    #[default]
    Round,
    /// Two sides at 45 degrees to the bed's normal, tangent to the circle,
    /// meeting at a point above it.
    Teardrop,
    /// The teardrop's sides cut short by a flat roof the sag allowance
    /// (`roof_sag`) above the circle.
    FlatRoof,
}

impl PrintShape {
    pub fn is_round(&self) -> bool {
        *self == Self::Round
    }
}

/// How a counterbore, or a nut pocket at the hole's start, that opens
/// toward the print bed is bridged for printing (mitcad#1516): its hole
/// goes up away from the bed, so the bore's ring above the recess's floor
/// would be printed in the air. The layers above the floor, the print's
/// layer height each, bridge it ([`crate::kernel::HoleBridging`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrintBridging {
    #[default]
    None,
    /// One layer closes the bore over the floor: a membrane drilled out
    /// after printing.
    Sacrificial,
    /// Two layers: bridges across the recess beside a strip as wide as the
    /// hole, then across the strip beside the hole's square.
    Stepped,
}

impl PrintBridging {
    pub fn is_none(&self) -> bool {
        *self == Self::None
    }
}

/// A flat roof's sag allowance when a hole leaves it out (mitcad#1515):
/// how far above the circle the bridge is printed, so that it sags onto it.
pub const DEFAULT_ROOF_SAG: f64 = 0.4;

/// How far from right angles to the print bed's normal a hole's axis may
/// lie and still be printed lying (mitcad#1515): the cosine of the angle
/// between them.
const ALONG_BED: f64 = 1e-6;

/// Crush ribs' number and height when a press fit leaves them out.
pub const DEFAULT_CRUSH_RIBS: u32 = 3;
pub const DEFAULT_RIB_HEIGHT: f64 = 0.3;
/// The most crush ribs a hole takes.
const MAX_CRUSH_RIBS: u32 = 64;

/// The kernel's section of a press fit (mitcad#1517) from the hole's
/// nominal `diameter` and the press fit's values: the polygon's inscribed
/// diameter or the ribs' tips `interference` under it. Checks the values.
pub fn press_fit_section(
    shape: PressFitShape,
    diameter: f64,
    interference: f64,
    ribs: Option<f64>,
    rib_height: Option<f64>,
    angle: Option<f64>,
) -> Result<crate::kernel::HolePressFit, String> {
    use crate::kernel::HolePressFit as Section;
    if !interference.is_finite() {
        return Err(format!(
            "the press fit's interference must be a finite length, got {interference}"
        ));
    }
    let inner = diameter - interference;
    if inner.is_nan() || inner <= 0.0 {
        return Err(format!(
            "the press fit's interference must be less than the hole's diameter {diameter}, got {interference}"
        ));
    }
    let angle = angle.unwrap_or(0.0);
    if !angle.is_finite() {
        return Err(format!(
            "the press fit's angle must be finite, got {angle} rad"
        ));
    }
    let sides = match shape {
        PressFitShape::Hex => Some(6),
        PressFitShape::Square => Some(4),
        PressFitShape::CrushRibs => None,
    };
    if let Some(sides) = sides {
        if ribs.is_some() || rib_height.is_some() {
            return Err("only crush ribs have `ribs` and `rib_height`".to_owned());
        }
        return Ok(Section::Polygon {
            sides,
            inscribed: inner,
            angle,
        });
    }
    let count = ribs.unwrap_or(f64::from(DEFAULT_CRUSH_RIBS));
    let whole = count.round();
    if !(count.is_finite()
        && (count - whole).abs() <= 1e-9
        && (3.0..=f64::from(MAX_CRUSH_RIBS)).contains(&whole))
    {
        return Err(format!(
            "a press fit's number of crush ribs must be a whole number from 3 to {MAX_CRUSH_RIBS}, got {count}"
        ));
    }
    let ribs = whole as u32;
    let height = rib_height.unwrap_or(DEFAULT_RIB_HEIGHT);
    if !height.is_finite() || height <= 0.0 {
        return Err(format!(
            "the crush ribs' height must be greater than zero, got {height}"
        ));
    }
    // The pin touches the ribs only: the bore between them clears it.
    if 2.0 * height <= interference {
        return Err(format!(
            "the crush ribs must stand out more than half the interference ({}), or the pin \
             touches the bore between them; got a height of {height}",
            interference / 2.0
        ));
    }
    let bore = inner + 2.0 * height;
    // Each rib's base, twice its height wide, takes 2 asin(h / R) of the
    // bore; the ribs must not touch.
    let base = 2.0 * (height / (bore / 2.0)).min(1.0).asin();
    if f64::from(ribs) * base >= 2.0 * std::f64::consts::PI {
        return Err(format!(
            "{ribs} crush ribs {height} high do not fit round a bore of {bore}: use fewer or lower ones"
        ));
    }
    Ok(Section::CrushRibs {
        ribs,
        tips: inner,
        bore,
        angle,
    })
}

/// The kernel's nut pocket (mitcad#1518) from a hole's values: across the
/// flats and the depth greater than zero, the offset not negative (0 when
/// left out), the angle finite (0 when left out), and a slot that reaches
/// out of the pocket. Checks the values.
pub fn nut_pocket(
    across_flats: f64,
    depth: f64,
    angle: Option<f64>,
    offset: Option<f64>,
    slot: Option<f64>,
) -> Result<HoleShape, String> {
    if !(across_flats.is_finite() && across_flats > 0.0) {
        return Err(format!(
            "the nut pocket's width across the flats must be greater than zero, got {across_flats}"
        ));
    }
    if !(depth.is_finite() && depth > 0.0) {
        return Err(format!(
            "the nut pocket's depth must be greater than zero, got {depth}"
        ));
    }
    let offset = offset.unwrap_or(0.0);
    if !(offset.is_finite() && offset >= 0.0) {
        return Err(format!(
            "the nut pocket's offset from the hole's start must not be negative, got {offset}"
        ));
    }
    let angle = angle.unwrap_or(0.0);
    if !angle.is_finite() {
        return Err(format!(
            "the nut pocket's angle must be finite, got {angle} rad"
        ));
    }
    if let Some(slot) = slot {
        // Past the pocket's corners along the slot: the nut must come out.
        let corner = across_flats / 3f64.sqrt();
        if !(slot.is_finite() && slot > corner) {
            return Err(format!(
                "the nut pocket's slot must reach out of the pocket, more than {corner:.3} from the \
                 axis; got {slot}"
            ));
        }
    }
    Ok(HoleShape::NutPocket {
        across_flats,
        depth,
        angle,
        offset,
        slot,
    })
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn is_iso(standard: &ThreadStandard) -> bool {
    *standard == ThreadStandard::IsoMetric
}

impl<P> HoleThread<P> {
    /// Whether it is a taper pipe thread that tapers the bore (mitcad#733).
    pub fn tapers(&self) -> bool {
        self.taper && self.standard.is_tapered()
    }

    pub fn size(&self) -> ThreadSize {
        ThreadSize {
            standard: self.standard,
            designation: self.designation.clone(),
            class: self.class.clone(),
            right_handed: self.right_handed,
        }
    }

    /// Whether a modelled thread's ends are cut flat, as before
    /// mitcad#1300 ([`ThreadEnds::Flat`]).
    pub fn flat_ends(&self) -> bool {
        self.ends == Some(ThreadEnds::Flat)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoleDef<P = ParamId> {
    pub placement: HolePlacement<P>,
    /// The hole's diameter; a tapped hole's thread sets it instead (but a
    /// taper pipe thread's, mitcad#733: the tap drill).
    pub diameter: P,
    #[serde(
        default = "Default::default",
        skip_serializing_if = "HoleKind::is_simple"
    )]
    pub kind: HoleKind<P>,
    /// The drill point's full angle, 118 degrees when left out.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub tip_angle: Option<P>,
    /// A flat bottom instead of a drill point (Drill Point Flat).
    #[serde(default, skip_serializing_if = "is_false")]
    pub flat: bool,
    pub extent: HoleExtent<P>,
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub thread: Option<HoleThread<P>>,
    /// Turns the holes round, out of the face (`isDefaultDirection = false`
    /// in .f3d designs).
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    /// The bodies to cut; empty for all bodies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<BodyUid>,
    /// A tapered hole: the wall's angle to the axis, positive narrowing the
    /// hole as it goes deeper from its diameter at the start (FreeCAD's
    /// tapered holes: 90 degrees less their taper angle, mitcad#4).
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub taper: Option<P>,
    /// A clearance hole's screw size and fit (mitcad#903); not with a
    /// `thread`. Holes saved before have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clearance: Option<HoleClearance>,
    /// A press-fit hole for printed parts (mitcad#1517): a hex or square
    /// hole or crush ribs, sized from `diameter`; a simple hole with a flat
    /// bottom, no thread, taper or clearance.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub press_fit: Option<HolePressFit<P>>,
    /// How the holes whose axis lies parallel to the print bed are printed
    /// (mitcad#1515): round, a teardrop or a flat roof; only the solid is
    /// shaped, the hole keeps its diameter (drawings, callouts). Not with a
    /// press fit, a taper or a modelled thread.
    #[serde(default, skip_serializing_if = "PrintShape::is_round")]
    pub print_shape: PrintShape,
    /// A flat roof's sag allowance above the circle, [`DEFAULT_ROOF_SAG`]
    /// when left out; only with `print_shape` `flat_roof`.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub roof_sag: Option<P>,
    /// A heat-set insert's hole: the insert series' size its values came
    /// from (mitcad#1518); a simple hole with no thread, clearance size or
    /// press fit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert: Option<HoleInsert>,
    /// Bridging for printing of a counterbore, or of a nut pocket at the
    /// start without a slot, that opens toward the print bed (mitcad#1516):
    /// none, a sacrificial layer or stepped bridges, the print's layer
    /// height thick each. Holes that do not open toward the bed are cut as
    /// they are.
    #[serde(default, skip_serializing_if = "PrintBridging::is_none")]
    pub print_bridging: PrintBridging,
}

type Mapper<'a, P, Q, E> = dyn FnMut(&str, &P) -> Result<Q, E> + 'a;

fn map_option<P, Q, E>(
    slot: &str,
    value: &Option<P>,
    f: &mut Mapper<'_, P, Q, E>,
) -> Result<Option<Q>, E> {
    value.as_ref().map(|v| f(slot, v)).transpose()
}

impl<P> HoleDef<P> {
    pub const TYPE: &'static str = "hole";
    pub const BASE_NAME: &'static str = "Hole";

    /// Gives a modelled thread without `ends` `ends`, as the thread
    /// feature's (mitcad#1307): `run_out` for a command (a hole made or
    /// edited from now on), `flat` for a design file written before
    /// mitcad#1300, whose holes' threads were cut so.
    pub(crate) fn settle_thread_ends(&mut self, ends: ThreadEnds) {
        if let Some(thread) = &mut self.thread
            && thread.modeled
            && thread.ends.is_none()
        {
            thread.ends = Some(ends);
        }
    }

    // Written by hand: placement, kind, extent and thread map by their kind.
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<HoleDef<Q>, E> {
        let placement = match &self.placement {
            HolePlacement::Face { body, face, points } => HolePlacement::Face {
                body: *body,
                face: face.clone(),
                points: points.clone(),
            },
            HolePlacement::FaceOffsets {
                body,
                face,
                edge1,
                offset1,
                edge2,
                offset2,
            } => HolePlacement::FaceOffsets {
                body: *body,
                face: face.clone(),
                edge1: edge1.clone(),
                offset1: f("placement.offset1", offset1)?,
                edge2: edge2.clone(),
                offset2: f("placement.offset2", offset2)?,
            },
            HolePlacement::SketchPoints { sketch, points } => HolePlacement::SketchPoints {
                sketch: *sketch,
                points: points.clone(),
            },
            HolePlacement::Plane { plane, points } => HolePlacement::Plane {
                plane: plane.clone(),
                points: points.clone(),
            },
            HolePlacement::At { at } => HolePlacement::At {
                at: Box::new(at.map_params("placement.at.", &mut *f)?),
            },
            HolePlacement::CylinderOffset {
                body,
                face,
                plane,
                offset,
                toward,
            } => HolePlacement::CylinderOffset {
                body: *body,
                face: face.clone(),
                plane: plane.clone(),
                offset: f("placement.offset", offset)?,
                toward: *toward,
            },
        };
        let diameter = f("diameter", &self.diameter)?;
        let kind = match &self.kind {
            HoleKind::Simple => HoleKind::Simple,
            HoleKind::Counterbore { diameter, depth } => HoleKind::Counterbore {
                diameter: f("kind.diameter", diameter)?,
                depth: f("kind.depth", depth)?,
            },
            HoleKind::Countersink { diameter, angle } => HoleKind::Countersink {
                diameter: f("kind.diameter", diameter)?,
                angle: f("kind.angle", angle)?,
            },
            HoleKind::Counterdrill {
                diameter,
                depth,
                angle,
            } => HoleKind::Counterdrill {
                diameter: f("kind.diameter", diameter)?,
                depth: f("kind.depth", depth)?,
                angle: f("kind.angle", angle)?,
            },
            HoleKind::NutPocket {
                across_flats,
                depth,
                angle,
                offset,
                slot,
            } => HoleKind::NutPocket {
                across_flats: f("kind.across_flats", across_flats)?,
                depth: f("kind.depth", depth)?,
                angle: map_option("kind.angle", angle, f)?,
                offset: map_option("kind.offset", offset, f)?,
                slot: map_option("kind.slot", slot, f)?,
            },
        };
        let tip_angle = map_option("tip_angle", &self.tip_angle, f)?;
        let extent = match &self.extent {
            HoleExtent::Distance { depth } => HoleExtent::Distance {
                depth: f("extent.depth", depth)?,
            },
            HoleExtent::ThroughAll => HoleExtent::ThroughAll,
            HoleExtent::ToObject { object, offset } => HoleExtent::ToObject {
                object: object.clone(),
                offset: map_option("extent.offset", offset, f)?,
            },
        };
        let thread = match &self.thread {
            None => None,
            Some(thread) => Some(HoleThread {
                standard: thread.standard,
                designation: thread.designation.clone(),
                class: thread.class.clone(),
                right_handed: thread.right_handed,
                modeled: thread.modeled,
                length: map_option("thread.length", &thread.length, f)?,
                offset: map_option("thread.offset", &thread.offset, f)?,
                radial_clearance: map_option(
                    "thread.radial_clearance",
                    &thread.radial_clearance,
                    f,
                )?,
                flank_clearance: map_option("thread.flank_clearance", &thread.flank_clearance, f)?,
                taper: thread.taper,
                custom: thread
                    .custom
                    .as_ref()
                    .map(|c| c.map_params("thread.custom.", &mut *f))
                    .transpose()?,
                starts: map_option("thread.starts", &thread.starts, f)?,
                ends: thread.ends,
            }),
        };
        let taper = map_option("taper", &self.taper, f)?;
        let press_fit = match &self.press_fit {
            None => None,
            Some(fit) => Some(HolePressFit {
                shape: fit.shape,
                interference: f("press_fit.interference", &fit.interference)?,
                ribs: map_option("press_fit.ribs", &fit.ribs, f)?,
                rib_height: map_option("press_fit.rib_height", &fit.rib_height, f)?,
                angle: map_option("press_fit.angle", &fit.angle, f)?,
            }),
        };
        Ok(HoleDef {
            placement,
            diameter,
            kind,
            tip_angle,
            flat: self.flat,
            extent,
            thread,
            flip: self.flip,
            participants: self.participants.clone(),
            taper,
            clearance: self.clearance.clone(),
            press_fit,
            print_shape: self.print_shape,
            roof_sag: map_option("roof_sag", &self.roof_sag, f)?,
            insert: self.insert.clone(),
            print_bridging: self.print_bridging,
        })
    }

    /// The number of holes, when it does not depend on evaluation.
    pub fn count(&self) -> usize {
        match &self.placement {
            HolePlacement::Face { points, .. } => points.len(),
            HolePlacement::FaceOffsets { .. } => 1,
            HolePlacement::SketchPoints { points, .. } => points.len(),
            HolePlacement::Plane { points, .. } => points.len(),
            HolePlacement::At { .. } | HolePlacement::CylinderOffset { .. } => 1,
        }
    }
}

/// A flat roof's sag allowance (mitcad#1515): a finite length above zero.
fn roof_sag_value(sag: f64) -> Result<f64, String> {
    if sag.is_finite() && sag > 0.0 {
        Ok(sag)
    } else {
        Err(format!(
            "the flat roof's sag allowance must be greater than zero, got {sag}"
        ))
    }
}

/// A hole's taper: less than a right angle either way.
fn check_taper(taper: f64) -> Result<(), String> {
    if taper.is_finite() && taper.abs() < std::f64::consts::FRAC_PI_2 {
        Ok(())
    } else {
        Err(format!(
            "the hole's taper must be less than 90 degrees either way, got {taper} rad"
        ))
    }
}

/// The name of hole `index`'s part made by `feature`: `F5:hole0.wall`.
pub fn hole_face(feature: FeatureUid, index: usize, part: &str) -> FaceName {
    FaceName::new(feature, &format!("hole{index}.{part}"), None)
}

impl FeatureInfo for HoleDef {
    fn references(&self) -> References {
        let mut references = References::default();
        match &self.placement {
            HolePlacement::Face { body, face, .. } => {
                references.body(*body);
                references.features.extend(face.features());
            }
            HolePlacement::FaceOffsets {
                body,
                face,
                edge1,
                edge2,
                ..
            } => {
                references.body(*body);
                references.features.extend(face.features());
                references.edges([&**edge1, &**edge2]);
            }
            HolePlacement::SketchPoints { sketch, .. } => {
                references.features.insert(*sketch);
            }
            HolePlacement::Plane { plane, .. } => plane.add_to(&mut references),
            HolePlacement::At { at } => at.add_to(&mut references),
            HolePlacement::CylinderOffset {
                body, face, plane, ..
            } => {
                references.body(*body);
                references.features.extend(face.features());
                plane.add_to(&mut references);
            }
        }
        if let HoleExtent::ToObject { object, .. } = &self.extent {
            object.add_references(&mut references);
        }
        for body in &self.participants {
            references.body(*body);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        match &self.placement {
            HolePlacement::Face { body, face, points } => {
                ctx.body(*body)?;
                for feature in face.features() {
                    ctx.feature(feature).map_err(|e| {
                        crate::notice::about_place(
                            crate::notice::NoticePlace::face(face.to_string()),
                            &e.to_string(),
                        )
                    })?;
                }
                if points.is_empty() {
                    return Err("the hole has no position".to_owned());
                }
                if points.iter().flatten().any(|v| !v.is_finite()) {
                    return Err("the hole positions must be finite points".to_owned());
                }
            }
            HolePlacement::FaceOffsets {
                body,
                face,
                edge1,
                edge2,
                ..
            } => {
                ctx.body(*body)?;
                for feature in face.features() {
                    ctx.feature(feature).map_err(|e| {
                        crate::notice::about_place(
                            crate::notice::NoticePlace::face(face.to_string()),
                            &e.to_string(),
                        )
                    })?;
                }
                ctx.edges(&[(**edge1).clone(), (**edge2).clone()])?;
            }
            HolePlacement::Plane { plane, points } => {
                plane.check(ctx, Want::Plane)?;
                if points.is_empty() {
                    return Err("the hole has no position".to_owned());
                }
                if points.iter().flatten().any(|v| !v.is_finite()) {
                    return Err("the hole positions must be finite points".to_owned());
                }
            }
            HolePlacement::At { at } => at.check(ctx)?,
            HolePlacement::CylinderOffset {
                body,
                face,
                plane,
                toward,
                ..
            } => {
                ctx.body(*body)?;
                for feature in face.features() {
                    ctx.feature(feature).map_err(|e| {
                        crate::notice::about_place(
                            crate::notice::NoticePlace::face(face.to_string()),
                            &e.to_string(),
                        )
                    })?;
                }
                plane.check(ctx, Want::Plane)?;
                if !toward.iter().all(|v| v.is_finite()) {
                    return Err("the hole's side must be a finite point".to_owned());
                }
            }
            HolePlacement::SketchPoints { sketch, points } => {
                ctx.sketch(*sketch)?;
                if points.is_empty() {
                    return Err("the hole has no position".to_owned());
                }
                for point in points {
                    if let SketchPoint::At(p) = point
                        && !p.iter().all(|v| v.is_finite())
                    {
                        return Err("the hole positions must be finite points".to_owned());
                    }
                }
            }
        }
        if let HoleExtent::ToObject { object, .. } = &self.extent {
            object.check(ctx)?;
        }
        if let Some(thread) = &self.thread {
            if thread.offset.is_some() && thread.length.is_none() {
                return Err("a thread over the whole wall has no offset".to_owned());
            }
            if thread.taper && !thread.standard.is_tapered() {
                return Err(
                    "only a taper pipe thread (NPT, ISO 7-1) is tapered: leave out `taper`"
                        .to_owned(),
                );
            }
            if thread.modeled {
                super::thread::check_modeled(thread.standard, thread.taper)?;
            } else if thread.radial_clearance.is_some() || thread.flank_clearance.is_some() {
                return Err("only a modeled thread has extra clearance".to_owned());
            }
            check_custom(&thread.size(), thread.custom.is_some())?;
            if thread.custom.is_none() {
                thread.size().check(Some(true))?;
            }
        }
        // A clearance hole's size is one of the table's (mitcad#903).
        if let Some(clearance) = &self.clearance {
            if self.thread.is_some() {
                return Err(
                    "a clearance hole has no thread: leave out `clearance` or `thread`".to_owned(),
                );
            }
            super::hole_sizes::lookup(&clearance.size)?;
        }
        // A press-fit hole is a simple one with its own section (mitcad#1517).
        if let Some(fit) = &self.press_fit {
            if self.thread.is_some() || self.clearance.is_some() {
                return Err(
                    "a press-fit hole has no thread or clearance size: leave out `press_fit`, \
                     `thread` or `clearance`"
                        .to_owned(),
                );
            }
            if !self.kind.is_simple() {
                return Err(
                    "a press-fit hole is a simple hole: no counterbore, countersink or counterdrill"
                        .to_owned(),
                );
            }
            if self.taper.is_some() {
                return Err("a press-fit hole has no taper".to_owned());
            }
            if fit.shape != PressFitShape::CrushRibs
                && (fit.ribs.is_some() || fit.rib_height.is_some())
            {
                return Err("only crush ribs have `ribs` and `rib_height`".to_owned());
            }
        }
        // A print shape's roof over a round bore (mitcad#1515).
        if !self.print_shape.is_round() {
            if self.press_fit.is_some() {
                return Err(
                    "a press-fit hole has its own section: leave out `print_shape` or `press_fit`"
                        .to_owned(),
                );
            }
            if self.taper.is_some() {
                return Err("a tapered hole has no print shape: leave out `print_shape`".to_owned());
            }
            if self
                .thread
                .as_ref()
                .is_some_and(|t| t.modeled || t.tapers())
            {
                return Err(
                    "a modeled or tapering thread is cut round: leave out `print_shape` or make \
                     the thread cosmetic"
                        .to_owned(),
                );
            }
        }
        if self.roof_sag.is_some() && self.print_shape != PrintShape::FlatRoof {
            return Err("only a flat roof has `roof_sag`".to_owned());
        }
        // A nut pocket about a round bore (mitcad#1518): the nut carries the
        // thread.
        if matches!(self.kind, HoleKind::NutPocket { .. }) {
            if self.thread.is_some() {
                return Err(
                    "a nut pocket's hole has no thread, the nut has it: leave out `thread`"
                        .to_owned(),
                );
            }
            if self.taper.is_some() {
                return Err("a nut pocket's hole has no taper".to_owned());
            }
            if !self.print_shape.is_round() {
                return Err(
                    "a nut pocket's hole has no print shape: leave out `print_shape`".to_owned(),
                );
            }
        }
        // Print bridging of a recess's floor (mitcad#1516).
        if !self.print_bridging.is_none() {
            let recess = match &self.kind {
                HoleKind::Counterbore { .. } => true,
                HoleKind::NutPocket { offset, slot, .. } => offset.is_none() && slot.is_none(),
                _ => false,
            };
            if !recess {
                return Err(
                    "print bridging bridges a counterbore, or a nut pocket at the hole's start \
                     without a slot: leave out `print_bridging`"
                        .to_owned(),
                );
            }
            if self.taper.is_some() || !self.print_shape.is_round() {
                return Err(
                    "a bridged hole has no taper or print shape: leave out `print_bridging`"
                        .to_owned(),
                );
            }
            if self
                .thread
                .as_ref()
                .is_some_and(|t| t.modeled || t.tapers())
            {
                return Err(
                    "a modeled or tapering thread is not bridged: leave out `print_bridging` or \
                     make the thread cosmetic"
                        .to_owned(),
                );
            }
        }
        // A heat-set insert's hole names a size of the series (mitcad#1518).
        if let Some(insert) = &self.insert {
            if self.thread.is_some() || self.clearance.is_some() || self.press_fit.is_some() {
                return Err(
                    "a heat-set insert's hole has no thread, clearance size or press fit: \
                     leave out `insert` or those"
                        .to_owned(),
                );
            }
            if !self.kind.is_simple() {
                return Err(
                    "a heat-set insert's hole is a simple hole: no counterbore, countersink, \
                     counterdrill or nut pocket"
                        .to_owned(),
                );
            }
            super::hole_sizes::insert(&insert.size)?;
        }
        check_participants(ctx, Operation::Cut, &self.participants)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        let positive = |what: &str, id: ParamId| {
            let v = value(id);
            if v > 0.0 {
                Ok(v)
            } else {
                Err(format!("the {what} must be greater than zero, got {v}"))
            }
        };
        let angle = |what: &str, id: ParamId| {
            let v = value(id);
            if v > 0.0 && v < std::f64::consts::PI {
                Ok(())
            } else {
                Err(format!(
                    "the {what} must be between 0 and 180 degrees, got {v} rad"
                ))
            }
        };
        // (A tapering taper pipe thread keeps the hole's diameter,
        // mitcad#733.)
        let diameter = if self.thread.as_ref().is_some_and(|t| !t.tapers()) {
            None
        } else {
            Some(positive("hole diameter", self.diameter)?)
        };
        match &self.kind {
            HoleKind::Simple => {}
            HoleKind::Counterbore { diameter: d, depth } => {
                let d = positive("counterbore diameter", *d)?;
                positive("counterbore depth", *depth)?;
                if diameter.is_some_and(|hole| d <= hole) {
                    return Err("the counterbore must be wider than the hole".to_owned());
                }
            }
            HoleKind::Countersink {
                diameter: d,
                angle: a,
            } => {
                let d = positive("countersink diameter", *d)?;
                angle("countersink angle", *a)?;
                if diameter.is_some_and(|hole| d <= hole) {
                    return Err("the countersink must be wider than the hole".to_owned());
                }
            }
            HoleKind::Counterdrill {
                diameter: d,
                depth,
                angle: a,
            } => {
                let d = positive("counterdrill diameter", *d)?;
                positive("counterdrill depth", *depth)?;
                angle("counterdrill angle", *a)?;
                if diameter.is_some_and(|hole| d <= hole) {
                    return Err("the counterdrill must be wider than the hole".to_owned());
                }
            }
            HoleKind::NutPocket {
                across_flats,
                depth,
                angle,
                offset,
                slot,
            } => {
                let (width, deep, below) = (
                    value(*across_flats),
                    value(*depth),
                    offset.map(value).unwrap_or(0.0),
                );
                nut_pocket(width, deep, angle.map(value), Some(below), slot.map(value))?;
                if diameter.is_some_and(|hole| hole >= width) {
                    return Err("the nut pocket must be wider than the hole".to_owned());
                }
                if let HoleExtent::Distance { depth } = &self.extent
                    && below + deep >= value(*depth)
                {
                    return Err("the nut pocket must be shallower than the hole".to_owned());
                }
            }
        }
        if let Some(tip) = self.tip_angle {
            angle("drill point angle", tip)?;
        }
        if let Some(id) = self.taper {
            check_taper(value(id))?;
            if self.thread.is_some() {
                return Err("a tapered hole has no thread".to_owned());
            }
        }
        if let HoleExtent::Distance { depth } = &self.extent {
            positive("hole depth", *depth)?;
        }
        if let HolePlacement::FaceOffsets {
            offset1, offset2, ..
        } = &self.placement
        {
            for offset in [offset1, offset2] {
                if value(*offset) < 0.0 {
                    return Err(format!(
                        "the hole's edge offsets must not be negative, got {}",
                        value(*offset)
                    ));
                }
            }
        }
        if let HolePlacement::CylinderOffset { offset, .. } = &self.placement
            && value(*offset) < 0.0
        {
            return Err(format!(
                "the hole's offset must not be negative, got {}",
                value(*offset)
            ));
        }
        if let Some(thread) = &self.thread {
            clearance_value("radial clearance", thread.radial_clearance.map(value))?;
            clearance_value("flank clearance", thread.flank_clearance.map(value))?;
            if let Some(custom) = &thread.custom {
                super::thread_table::ThreadData::custom(value(custom.major), value(custom.pitch))?;
            }
            if let Some(starts) = thread.starts {
                let starts = starts_value(value(starts))?;
                if let Ok(data) = thread.size().data() {
                    check_starts(starts, &data)?;
                }
            }
        }
        if let (Some(fit), Some(diameter)) = (&self.press_fit, diameter) {
            press_fit_section(
                fit.shape,
                diameter,
                value(fit.interference),
                fit.ribs.map(value),
                fit.rib_height.map(value),
                fit.angle.map(value),
            )?;
        }
        if let Some(sag) = self.roof_sag {
            roof_sag_value(value(sag))?;
        }
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        // Pieces of bodies the holes split.
        true
    }

    fn tool_use(&self) -> Option<ToolUse> {
        // A pattern repeats only the tool, which has no modelled thread
        // and no taper (mitcad#733).
        if self
            .thread
            .as_ref()
            .is_some_and(|t| t.modeled || t.tapers())
        {
            return None;
        }
        Some(ToolUse {
            operation: Operation::Cut,
            participants: self.participants.clone(),
        })
    }
}

impl HoleDef {
    /// Holes at points of a cylindrical face: each point moved across the
    /// axis onto the cylinder, the hole along the radius into the material
    /// (towards the axis on a shaft, away from it on a bore's wall), the
    /// other way with `flip`.
    fn on_cylinder(&self, cylinder: &Cylinder, points: &[[f64; 3]]) -> Result<Vec<Axis>, String> {
        if points.is_empty() {
            return Err("the hole has no position".to_owned());
        }
        let axis = &cylinder.axis;
        points
            .iter()
            .map(|p| {
                let from: [f64; 3] = std::array::from_fn(|i| p[i] - axis.origin[i]);
                let along = dot(from, axis.direction);
                let out = normalized(std::array::from_fn(|i| from[i] - along * axis.direction[i]))
                    .ok_or("a hole's point lies on the axis of its cylindrical face")?;
                let origin = std::array::from_fn(|i| {
                    axis.origin[i] + along * axis.direction[i] + cylinder.radius * out[i]
                });
                let inward = if cylinder.internal { 1.0 } else { -1.0 };
                let sense = if self.flip { -inward } else { inward };
                Ok(Axis {
                    origin,
                    // (`+ 0.0`: no negative zeros.)
                    direction: out.map(|v| sense * v + 0.0),
                })
            })
            .collect()
    }

    /// The hole on a cylindrical face at the point of its axis `offset`
    /// from a plane through `origin` with `normal` (the distance along the
    /// normal, on the side of the face's middle), on the side of the axis
    /// where `toward` lies ([`HolePlacement::CylinderOffset`]).
    fn at_offset(
        &self,
        cylinder: &Cylinder,
        origin: [f64; 3],
        normal: [f64; 3],
        offset: f64,
        toward: [f64; 3],
    ) -> Result<Vec<Axis>, String> {
        let axis = &cylinder.axis;
        let across = dot(axis.direction, normal);
        if across.abs() < 1e-9 {
            return Err("the hole's plane does not cross the axis of its face".to_owned());
        }
        // Where the axis meets the plane, and the way to the face's middle.
        let t = dot(std::array::from_fn(|i| origin[i] - axis.origin[i]), normal) / across;
        let middle = cylinder.length / 2.0 - t;
        let along = t + middle.signum() * offset / across.abs();
        let centre: [f64; 3] = std::array::from_fn(|i| axis.origin[i] + along * axis.direction[i]);
        let from: [f64; 3] = std::array::from_fn(|i| toward[i] - centre[i]);
        let out = normalized(std::array::from_fn(|i| {
            from[i] - dot(from, axis.direction) * axis.direction[i]
        }))
        .ok_or("the hole's side point lies on the axis of its face")?;
        let point: [f64; 3] = std::array::from_fn(|i| centre[i] + cylinder.radius * out[i]);
        self.on_cylinder(cylinder, &[point])
    }

    /// The holes' start points and their direction into the material.
    pub(crate) fn positions<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<Vec<Axis>, String> {
        let (points, normal) = match &self.placement {
            HolePlacement::Face { body, face, points } => {
                let shape = ctx.surface_body(*body)?;
                let face = super::renamed::face_for(ctx, &shape, face)?;
                let plane = match ctx.kernel.face_plane(&shape, &face) {
                    Ok(plane) => plane,
                    Err(not_planar) => {
                        // A cylindrical face (a shaft's side, mitcad#719):
                        // each point put on the cylinder, the hole across
                        // its axis.
                        let Ok(cylinder) = ctx.kernel.face_cylinder(&shape, &face) else {
                            return Err(not_planar.to_string());
                        };
                        return self.on_cylinder(&cylinder, points);
                    }
                };
                let on_plane = points
                    .iter()
                    .map(|p| {
                        let height = dot(
                            std::array::from_fn(|i| p[i] - plane.origin[i]),
                            plane.normal,
                        );
                        std::array::from_fn(|i| p[i] - height * plane.normal[i])
                    })
                    .collect();
                (on_plane, plane.normal)
            }
            HolePlacement::FaceOffsets {
                body,
                face,
                edge1,
                offset1,
                edge2,
                offset2,
            } => {
                let shape = ctx.surface_body(*body)?;
                let face = super::renamed::face_for(ctx, &shape, face)?;
                let plane = ctx
                    .kernel
                    .face_plane(&shape, &face)
                    .map_err(|e| e.to_string())?;
                let mut lines = Vec::new();
                for edge in [edge1, edge2] {
                    let edge = ctx.edge_on(*body, &shape, edge)?;
                    let line = ctx.axis(&GeomRef::Edge { body: *body, edge })?;
                    lines.push(Axis {
                        origin: line.origin,
                        direction: line.direction,
                    });
                }
                let offsets = [ctx.param(*offset1)?, ctx.param(*offset2)?];
                let point = offset_point(plane.origin, plane.normal, &lines, offsets)?;
                (vec![point], plane.normal)
            }
            HolePlacement::SketchPoints { sketch, points } => {
                let output = ctx.sketch(*sketch)?;
                let mut located = Vec::new();
                for point in points {
                    let uv = match point {
                        SketchPoint::At(uv) => *uv,
                        SketchPoint::Center(curve) => match output.solved.curves.get(curve) {
                            Some(
                                Curve2::Circle { center, .. }
                                | Curve2::Arc { center, .. }
                                | Curve2::Ellipse { center, .. }
                                | Curve2::EllipticalArc { center, .. },
                            ) => *center,
                            _ => {
                                return Err(format!(
                                    "{} has no circle, arc or ellipse {}",
                                    ctx.feature_name(*sketch),
                                    curve.curve_name()
                                ));
                            }
                        },
                        SketchPoint::Point(p) => *output.solved.points.get(p).ok_or_else(|| {
                            format!("{} has no point p{}", ctx.feature_name(*sketch), p.0)
                        })?,
                    };
                    located.push(output.frame.point(uv));
                }
                (located, output.frame.normal())
            }
            HolePlacement::Plane { plane, points } => {
                let frame = ctx.plane(plane)?.frame();
                let normal = frame.normal();
                let on_plane = points
                    .iter()
                    .map(|p| {
                        let height = dot(std::array::from_fn(|i| p[i] - frame.origin[i]), normal);
                        std::array::from_fn(|i| p[i] - height * normal[i])
                    })
                    .collect();
                (on_plane, normal)
            }
            HolePlacement::CylinderOffset {
                body,
                face,
                plane,
                offset,
                toward,
            } => {
                let shape = ctx.surface_body(*body)?;
                let face = super::renamed::face_for(ctx, &shape, face)?;
                let cylinder = ctx
                    .kernel
                    .face_cylinder(&shape, &face)
                    .map_err(|e| e.to_string())?;
                let frame = ctx.plane(plane)?.frame();
                let offset = ctx.param(*offset)?;
                return self.at_offset(&cylinder, frame.origin, frame.normal(), offset, *toward);
            }
            // Placed by picked geometry (mitcad#656): into a shaft or a hole
            // along its axis, into a face against its normal.
            HolePlacement::At { at } => {
                let placed = ctx.place_def(at)?;
                let n = placed.frame.normal();
                let normal = if placed.seat.is_some() {
                    n.map(|v| -v)
                } else {
                    n
                };
                (vec![placed.frame.origin], normal)
            }
        };
        if points.is_empty() {
            return Err("the hole has no position".to_owned());
        }
        let direction = if self.flip {
            normal
        } else {
            normal.map(|v| -v)
        };
        Ok(points
            .into_iter()
            .map(|origin| Axis { origin, direction })
            .collect())
    }
}

impl HoleDef {
    /// The roofs of a print-shaped hole (mitcad#1515): for each hole whose
    /// axis lies parallel to the print bed, the direction away from it at
    /// right angles to the axis; None for round holes, or when no hole lies
    /// so (an information then) or the bed is not found here (a warning).
    fn print_roofs<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        participants: &[(BodyUid, K::Shape)],
        positions: &[Axis],
    ) -> Result<Option<HolePrintShape>, String> {
        let flat_roof = match self.print_shape {
            PrintShape::Round => return Ok(None),
            PrintShape::Teardrop => None,
            PrintShape::FlatRoof => Some(match self.roof_sag {
                Some(id) => roof_sag_value(ctx.param(id)?)?,
                None => DEFAULT_ROOF_SAG,
            }),
        };
        let Some(up) = away_from_bed(ctx, participants, "the holes are round") else {
            return Ok(None);
        };
        let roofs: Vec<Option<[f64; 3]>> = positions
            .iter()
            .map(|position| {
                let along = normalized(position.direction)?;
                let across = dot(up, along);
                if across.abs() > ALONG_BED {
                    return None;
                }
                // (`+ 0.0`: no negative zeros.)
                normalized(std::array::from_fn(|i| up[i] - across * along[i]))
                    .map(|roof| roof.map(|v| v + 0.0))
            })
            .collect();
        if roofs.iter().all(Option::is_none) {
            ctx.inform("no hole lies parallel to the print bed: the holes are round");
            return Ok(None);
        }
        Ok(Some(HolePrintShape { roofs, flat_roof }))
    }
}

impl HoleDef {
    /// The bridging of the holes whose recess opens toward the print bed
    /// (mitcad#1516): those whose axis points straight away from the bed
    /// (the participants' print bed face, as for print shapes), each bridge
    /// layer the design's layer height thick. None without bridging, or
    /// when no hole opens so (an information then) or the bed is not found
    /// here (a warning).
    fn bridging<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        participants: &[(BodyUid, K::Shape)],
        positions: &[Axis],
    ) -> Result<Option<HoleBridging>, String> {
        let mode = match self.print_bridging {
            PrintBridging::None => return Ok(None),
            PrintBridging::Sacrificial => BridgeMode::Sacrificial,
            PrintBridging::Stepped => BridgeMode::Stepped,
        };
        let layer = ctx.print_layer_height();
        let Some(up) = away_from_bed(ctx, participants, "the holes are not bridged") else {
            return Ok(None);
        };
        let holes: Vec<bool> = positions
            .iter()
            .map(|position| {
                normalized(position.direction).is_some_and(|along| dot(up, along) > 1.0 - ALONG_BED)
            })
            .collect();
        if !holes.contains(&true) {
            ctx.inform("no hole's recess opens toward the print bed: nothing is bridged");
            return Ok(None);
        }
        Ok(Some(HoleBridging { mode, layer, holes }))
    }
}

/// The direction away from the print bed for a print-shaped hole
/// (mitcad#1515): against the outward normal of the print bed face of the
/// first participant that has one, found in its shape before the hole; up
/// the design's Z when none has one (its −Z is down, as the 3D print export
/// and Print Analysis take it). None, with a warning, when the bed faces the
/// participants have are not found here, saying what that means:
/// `consequence` (print-bridged holes take the same bed, mitcad#1516).
fn away_from_bed<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    participants: &[(BodyUid, K::Shape)],
    consequence: &str,
) -> Option<[f64; 3]> {
    let mut missing = Vec::new();
    for (body, _) in participants {
        let Some(face) = ctx.print_bed_face(*body) else {
            continue;
        };
        // Its plane as the bed's is found (`document/print_bed.rs`), in
        // the body as it is before the hole.
        let plane = FaceName::from_str(&face)
            .map_err(|e| e.to_string())
            .and_then(|face| ctx.plane(&GeomRef::Face { body: *body, face }));
        match plane {
            // (`+ 0.0`: no negative zeros.)
            Ok(plane) => return Some(plane.frame().normal().map(|v| -v + 0.0)),
            Err(e) => missing.push(format!("body {body} ({e})")),
        }
    }
    if missing.is_empty() {
        return Some([0.0, 0.0, 1.0]);
    }
    ctx.warn(format!(
        "the print bed face of {} is not found before the hole: {consequence}",
        missing.join(", ")
    ));
    None
}

/// The point of a plane at given distances from the lines of two edges, on
/// the side of each line where the plane's origin (the face's middle) is.
fn offset_point(
    origin: [f64; 3],
    normal: [f64; 3],
    lines: &[Axis],
    offsets: [f64; 2],
) -> Result<[f64; 3], String> {
    // Rows of the system: across each line in the plane, and the normal.
    let mut rows = [[0.0; 3]; 3];
    let mut right = [0.0; 3];
    for (k, line) in lines.iter().enumerate() {
        let mut across = normalized(cross(normal, line.direction))
            .ok_or("an edge of the hole's position is at right angles to the face")?;
        let side = dot(std::array::from_fn(|i| origin[i] - line.origin[i]), across);
        if side < 0.0 {
            across = across.map(|v| -v);
        }
        rows[k] = across;
        right[k] = offsets[k] + dot(line.origin, across);
    }
    rows[2] = normal;
    right[2] = dot(origin, normal);
    let det = dot(rows[0], cross(rows[1], rows[2]));
    if det.abs() < 1e-9 {
        return Err("the edges of the hole's position are parallel".to_owned());
    }
    // Cramer's rule.
    let solve = |k: usize| {
        let mut m = rows;
        for r in 0..3 {
            m[r][k] = right[r];
        }
        dot(m[0], cross(m[1], m[2])) / det
    };
    Ok([solve(0), solve(1), solve(2)])
}

impl<K: Kernel> Evaluate<K> for HoleDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let positions = self.positions(ctx)?;
        let participants = participant_bodies(ctx, Operation::Cut, &self.participants)?;
        // The size: the table's, or a custom thread's (mitcad#893).
        let thread = match &self.thread {
            Some(thread) => {
                let data = size_of(ctx, &thread.size(), thread.custom.as_ref(), Some(true))?;
                Some((thread, data))
            }
            None => None,
        };
        // A tapering taper pipe thread's hole is drilled at its diameter,
        // the tap drill (mitcad#733); another's bore is the size's minor
        // diameter.
        let diameter = match &thread {
            Some((t, data)) if !t.tapers() => data.minor,
            _ => ctx.positive(self.diameter)?,
        };
        let shape = match &self.kind {
            HoleKind::Simple => HoleShape::Simple,
            HoleKind::Counterbore { diameter: d, depth } => HoleShape::Counterbore {
                diameter: ctx.positive(*d)?,
                depth: ctx.positive(*depth)?,
            },
            HoleKind::Countersink { diameter: d, angle } => HoleShape::Countersink {
                diameter: ctx.positive(*d)?,
                angle: ctx.positive(*angle)?,
            },
            HoleKind::Counterdrill {
                diameter: d,
                depth,
                angle,
            } => HoleShape::Counterdrill {
                diameter: ctx.positive(*d)?,
                depth: ctx.positive(*depth)?,
                angle: ctx.positive(*angle)?,
            },
            HoleKind::NutPocket {
                across_flats,
                depth,
                angle,
                offset,
                slot,
            } => {
                let mut optional = |id: Option<ParamId>| id.map(|id| ctx.param(id)).transpose();
                let (angle, offset, slot) =
                    (optional(*angle)?, optional(*offset)?, optional(*slot)?);
                nut_pocket(
                    ctx.param(*across_flats)?,
                    ctx.param(*depth)?,
                    angle,
                    offset,
                    slot,
                )?
            }
        };
        let taper = match self.taper {
            Some(id) => {
                let taper = ctx.param(id)?;
                check_taper(taper)?;
                taper
            }
            None => 0.0,
        };
        // A print shape's roofs over the holes printed lying (mitcad#1515);
        // the hole keeps its diameter.
        let print_shape = self.print_roofs(ctx, &participants, &positions)?;
        // Bridging of the recesses that open toward the print bed
        // (mitcad#1516), its layers the design's layer height.
        let bridging = self.bridging(ctx, &participants, &positions)?;
        // A press fit's section (mitcad#1517), from the nominal diameter.
        let press_fit = match &self.press_fit {
            Some(fit) => {
                let mut optional = |id: Option<ParamId>| id.map(|id| ctx.param(id)).transpose();
                let ribs = optional(fit.ribs)?;
                let rib_height = optional(fit.rib_height)?;
                let angle = optional(fit.angle)?;
                Some(press_fit_section(
                    fit.shape,
                    diameter,
                    ctx.param(fit.interference)?,
                    ribs,
                    rib_height,
                    angle,
                )?)
            }
            None => None,
        };
        let tip_angle = if self.flat || press_fit.is_some() {
            None
        } else {
            Some(match self.tip_angle {
                Some(id) => ctx.positive(id)?,
                None => DEFAULT_TIP_ANGLE,
            })
        };
        let end = match &self.extent {
            HoleExtent::Distance { depth } => HoleEnd::Distance(ctx.positive(*depth)?),
            HoleExtent::ThroughAll => HoleEnd::ThroughAll,
            HoleExtent::ToObject { object, offset } => {
                let offset = offset.map(|id| ctx.param(id)).transpose()?.unwrap_or(0.0);
                // A point or a line stands for the plane through it at
                // right angles to the hole's axis (mitcad#1329).
                let across = positions.first().map(|p| Across {
                    normal: p.direction,
                    lies: "in a plane at right angles to the hole's axis",
                });
                HoleEnd::Target(Bound {
                    target: object.resolve_across(ctx, &Default::default(), across)?,
                    offset,
                })
            }
        };
        // A simple hole through a sheet cuts its material domain, so the
        // body stays a sheet (mitcad#211); its tool is still the hole's,
        // which patterns and mirrors of it repeat.
        let sheet_depth = match &end {
            _ if thread.is_some()
                || !matches!(shape, HoleShape::Simple)
                || taper != 0.0
                || press_fit.is_some()
                || print_shape.is_some() =>
            {
                None
            }
            HoleEnd::Distance(d) => Some(Some(*d)),
            HoleEnd::ThroughAll => Some(None),
            HoleEnd::Target(_) => None,
        };
        let sheet_body = match participants.as_slice() {
            [(body, _)] => Some(*body),
            _ => None,
        };
        let spec = HoleSpec {
            feature: ctx.uid,
            positions: &positions,
            diameter,
            shape,
            tip_angle,
            end,
            bodies: participants.iter().map(|(_, s)| s.clone()).collect(),
            taper,
            press_fit,
            print_shape,
            bridging,
        };
        let tool = ctx.kernel.hole_tool(&spec).map_err(|e| e.to_string())?;
        if let (Some(depth), Some(body)) = (sheet_depth, sheet_body)
            && let Some(mut output) =
                crate::sheet::folded::cut::hole_cut(ctx, body, &positions, diameter, depth)?
        {
            output.tool = Some(tool);
            return Ok(output);
        }
        let mut output = apply_operation(ctx, Operation::Cut, participants, tool, "hole")?;
        // A modelled thread, or a cosmetic taper pipe thread's taper.
        let Some((thread, data)) = thread.filter(|(t, _)| t.modeled || t.tapers()) else {
            return Ok(output);
        };
        // A taper pipe thread's diameters where it starts, at the hole's
        // start (mitcad#733); a trapezoidal thread's with its crest
        // clearance (mitcad#893).
        let (major, minor, pitch_diameter) = (if thread.tapers() {
            data.taper_start(true)
        } else {
            None
        })
        .unwrap_or_else(|| data.profile(true));
        let flank_angle = data.flank_angle();
        let starts = thread_starts(ctx, thread.starts, &data)?;
        // Cut the thread into the walls each body has: from the holes' start,
        // which is the low end of the walls' axes when they point inwards.
        let walls: Vec<FaceName> = (0..positions.len())
            .map(|i| hole_face(ctx.uid, i, "wall"))
            .collect();
        let mut part = thread_part(ctx, thread.length, thread.offset, false)?;
        let clearance = thread_clearance(ctx, thread.radial_clearance, thread.flank_clearance)?;
        // The printed-fit flank clearance capped where the pitch is too
        // fine for it (mitcad#691).
        let clearance = fitted_clearance(
            data.pitch,
            (major, minor, pitch_diameter),
            true,
            clearance,
            flank_from_fit(ctx, thread.flank_clearance),
            flank_angle,
        );
        check_clearance(
            data.pitch,
            (major, minor, pitch_diameter),
            true,
            clearance,
            flank_angle,
        )?;
        let (radial_clearance, flank_clearance) = clearance;
        let mut threaded = Vec::new();
        for change in &mut output.changes {
            let BodyChange::Set(body, shape) = change else {
                continue;
            };
            let mut faces = Vec::new();
            for wall in &walls {
                if ctx
                    .kernel
                    .count_faces(shape, wall)
                    .map_err(|e| e.to_string())?
                    > 0
                {
                    faces.push(wall.clone());
                }
            }
            let Some(first) = faces.first() else {
                continue;
            };
            // The hole's start: the high end of the walls' axes where they
            // point out of the material.
            let start_high = if part.is_some() || thread.tapers() {
                let cylinder = ctx
                    .kernel
                    .face_cylinder(shape, first)
                    .map_err(|e| e.to_string())?;
                dot(cylinder.axis.direction, positions[0].direction) < 0.0
            } else {
                false
            };
            if let Some((_, _, high)) = &mut part {
                *high = start_high;
            }
            let spec = ThreadSpec {
                feature: ctx.uid,
                faces: &faces,
                pitch: data.pitch,
                major,
                minor,
                pitch_diameter,
                right_handed: thread.right_handed,
                angle: 0.0,
                face_angles: &[],
                part,
                radial_clearance,
                flank_clearance,
                groove: thread.modeled,
                chamfer: None,
                taper: data.taper.filter(|_| thread.tapers()).map(|t| ThreadTaper {
                    per_length: t.per_length,
                    high_end: start_high,
                }),
                starts,
                flank_angle,
                flat_ends: thread.flat_ends(),
            };
            let cut = ctx
                .kernel
                .modeled_thread(shape, &spec)
                .map_err(|e| e.to_string())?;
            // The walls the thread left no piece of, or only pieces outside
            // its part (the crest off the wall: what is left does not show
            // where the thread runs, mitcad#628), as they were.
            for wall in faces.iter().filter(|_| thread.modeled) {
                let Ok(cylinder) = ctx.kernel.face_cylinder(shape, wall) else {
                    continue;
                };
                let crest_off = (data.minor / 2.0 + radial_clearance - cylinder.radius).abs()
                    > super::thread::CREST_ON_FACE;
                if crest_off || ctx.kernel.count_faces(&cut, wall).is_ok_and(|n| n == 0) {
                    threaded.push(ThreadedWall {
                        body: *body,
                        wall: wall.clone(),
                        cylinder,
                    });
                }
            }
            *shape = cut;
        }
        output.threaded_walls = threaded;
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
    }

    #[test]
    fn offsets_from_two_edges_place_a_point() {
        // The top of a 60 x 40 block: 10 from the front edge (y = 0), 15
        // from the left edge (x = 0).
        let front = Axis {
            origin: [0.0, 0.0, 20.0],
            direction: [1.0, 0.0, 0.0],
        };
        let left = Axis {
            origin: [0.0, 40.0, 20.0],
            direction: [0.0, -1.0, 0.0],
        };
        let point = offset_point(
            [30.0, 20.0, 20.0],
            [0.0, 0.0, 1.0],
            &[front, left],
            [10.0, 15.0],
        )
        .unwrap();
        assert!(close(point, [15.0, 10.0, 20.0]), "{point:?}");
        let error = offset_point(
            [30.0, 20.0, 20.0],
            [0.0, 0.0, 1.0],
            &[front, front],
            [1.0, 2.0],
        )
        .unwrap_err();
        assert!(error.contains("parallel"), "{error}");
    }

    #[test]
    fn holes_serialize_with_defaults_left_out() {
        let def: HoleDef<String> = serde_json::from_value(serde_json::json!({
            "placement": {"type": "sketch_points", "sketch": "F3", "points": [[10, 10], "c5"]},
            "diameter": "d4", "extent": {"type": "through_all"},
            "thread": {"designation": "M6x1", "class": "6H"}}))
        .unwrap();
        let HolePlacement::SketchPoints { points, .. } = &def.placement else {
            panic!("sketch points");
        };
        assert_eq!(
            points,
            &vec![
                SketchPoint::At([10.0, 10.0]),
                SketchPoint::Center(EntityUid(5))
            ]
        );
        assert_eq!(
            serde_json::to_value(&def).unwrap(),
            serde_json::json!({
                "placement": {"type": "sketch_points", "sketch": "F3", "points": [[10.0, 10.0], "c5"]},
                "diameter": "d4", "extent": {"type": "through_all"},
                "thread": {"designation": "M6x1", "class": "6H"}})
        );
        assert_eq!(
            hole_face(FeatureUid(5), 2, "wall").to_string(),
            "F5:hole2.wall"
        );
    }

    #[test]
    fn clearance_holes_keep_their_size_and_fit() {
        // mitcad#903: a hole saved before has no `clearance` and writes
        // none; a clearance hole writes its size and fit (Normal when left
        // out).
        let before = serde_json::json!({
            "placement": {"type": "sketch_points", "sketch": "F3", "points": [[10.0, 10.0], "c5"]},
            "diameter": "d4", "kind": {"type": "counterbore", "diameter": "d5", "depth": "d6"},
            "extent": {"type": "through_all"}});
        let def: HoleDef<String> = serde_json::from_value(before.clone()).unwrap();
        assert_eq!(def.clearance, None);
        assert_eq!(serde_json::to_value(&def).unwrap(), before);
        let mut clearance = before.clone();
        clearance["clearance"] = serde_json::json!({"size": "M6"});
        let def: HoleDef<String> = serde_json::from_value(clearance.clone()).unwrap();
        assert_eq!(
            def.clearance,
            Some(HoleClearance {
                size: "M6".to_owned(),
                fit: ClearanceFit::Normal
            })
        );
        clearance["clearance"] = serde_json::json!({"size": "M6", "fit": "normal"});
        assert_eq!(serde_json::to_value(&def).unwrap(), clearance);
        // Mapped with the other values.
        let mapped = def
            .map_params::<String, ()>(&mut |_, p| Ok(p.clone()))
            .unwrap();
        assert_eq!(mapped.clearance, def.clearance);
        let mut wrong = before;
        wrong["clearance"] = serde_json::json!({"size": "M6", "fit": "loose"});
        assert!(serde_json::from_value::<HoleDef<String>>(wrong).is_err());
    }

    #[test]
    fn print_shapes_keep_their_values() {
        // mitcad#1515: a round hole (and one saved before) writes no print
        // shape; a teardrop or a flat roof writes it, with its sag.
        let before = serde_json::json!({
            "placement": {"type": "sketch_points", "sketch": "F3", "points": ["c5"]},
            "diameter": "d4", "extent": {"type": "through_all"}});
        let def: HoleDef<String> = serde_json::from_value(before.clone()).unwrap();
        assert_eq!(def.print_shape, PrintShape::Round);
        assert_eq!(def.roof_sag, None);
        assert_eq!(serde_json::to_value(&def).unwrap(), before);
        let mut round = before.clone();
        round["print_shape"] = serde_json::json!("round");
        let def: HoleDef<String> = serde_json::from_value(round).unwrap();
        assert_eq!(serde_json::to_value(&def).unwrap(), before);
        let mut flat = before;
        flat["print_shape"] = serde_json::json!("flat_roof");
        flat["roof_sag"] = serde_json::json!("hole_roof_sag");
        let def: HoleDef<String> = serde_json::from_value(flat.clone()).unwrap();
        assert_eq!(def.print_shape, PrintShape::FlatRoof);
        assert_eq!(serde_json::to_value(&def).unwrap(), flat);
        let mut slots = Vec::new();
        let mapped = def
            .map_params::<String, ()>(&mut |slot, p| {
                slots.push(slot.to_owned());
                Ok(p.clone())
            })
            .unwrap();
        assert_eq!(mapped.print_shape, PrintShape::FlatRoof);
        assert_eq!(mapped.roof_sag, def.roof_sag);
        assert!(slots.contains(&"roof_sag".to_owned()));
        let mut wrong = flat;
        wrong["print_shape"] = serde_json::json!("oval");
        assert!(serde_json::from_value::<HoleDef<String>>(wrong).is_err());
        assert!(roof_sag_value(0.0).is_err() && roof_sag_value(f64::NAN).is_err());
        assert_eq!(roof_sag_value(0.4), Ok(0.4));
    }

    #[test]
    fn nut_pockets_and_insert_holes_keep_their_values() {
        // mitcad#1518: a nut pocket writes what it was given, its optional
        // values left out; an insert hole its size.
        let pocket = serde_json::json!({
            "placement": {"type": "sketch_points", "sketch": "F3", "points": ["c5"]},
            "diameter": "d4",
            "kind": {"type": "nut_pocket", "across_flats": "d5", "depth": "d6", "slot": "d7"},
            "extent": {"type": "through_all"}});
        let def: HoleDef<String> = serde_json::from_value(pocket.clone()).unwrap();
        assert_eq!(serde_json::to_value(&def).unwrap(), pocket);
        let mut slots = Vec::new();
        let mapped = def
            .map_params::<String, ()>(&mut |slot, p| {
                slots.push(slot.to_owned());
                Ok(p.clone())
            })
            .unwrap();
        assert_eq!(mapped.kind, def.kind);
        for slot in ["kind.across_flats", "kind.depth", "kind.slot"] {
            assert!(slots.contains(&slot.to_owned()), "{slot}");
        }
        let mut wrong = pocket.clone();
        wrong["kind"]["width"] = serde_json::json!("d8");
        assert!(serde_json::from_value::<HoleDef<String>>(wrong).is_err());

        let mut insert = pocket;
        insert["kind"] = serde_json::json!({"type": "simple"});
        insert["insert"] = serde_json::json!({"size": "M3"});
        let def: HoleDef<String> = serde_json::from_value(insert.clone()).unwrap();
        assert_eq!(
            def.insert,
            Some(HoleInsert {
                size: "M3".to_owned()
            })
        );
        insert.as_object_mut().unwrap().remove("kind");
        assert_eq!(serde_json::to_value(&def).unwrap(), insert);

        // The kernel's pocket from the values, and the values refused.
        assert_eq!(
            nut_pocket(5.8, 2.7, None, None, Some(10.0)),
            Ok(HoleShape::NutPocket {
                across_flats: 5.8,
                depth: 2.7,
                angle: 0.0,
                offset: 0.0,
                slot: Some(10.0)
            })
        );
        for (result, expected) in [
            (nut_pocket(0.0, 2.7, None, None, None), "across the flats"),
            (nut_pocket(5.8, -1.0, None, None, None), "depth"),
            (
                nut_pocket(5.8, 2.7, None, Some(-0.5), None),
                "must not be negative",
            ),
            (nut_pocket(5.8, 2.7, Some(f64::NAN), None, None), "angle"),
            (nut_pocket(5.8, 2.7, None, None, Some(3.0)), "reach out"),
        ] {
            let error = result.unwrap_err();
            assert!(error.contains(expected), "'{expected}' not in '{error}'");
        }
    }

    #[test]
    fn print_bridging_keeps_its_mode() {
        // mitcad#1516: none (and a hole saved before) writes nothing; a
        // sacrificial layer or stepped bridges write their mode.
        let before = serde_json::json!({
            "placement": {"type": "sketch_points", "sketch": "F3", "points": ["c5"]},
            "diameter": "d4", "kind": {"type": "counterbore", "diameter": "d5", "depth": "d6"},
            "extent": {"type": "through_all"}});
        let def: HoleDef<String> = serde_json::from_value(before.clone()).unwrap();
        assert_eq!(def.print_bridging, PrintBridging::None);
        assert_eq!(serde_json::to_value(&def).unwrap(), before);
        let mut none = before.clone();
        none["print_bridging"] = serde_json::json!("none");
        let def: HoleDef<String> = serde_json::from_value(none).unwrap();
        assert_eq!(serde_json::to_value(&def).unwrap(), before);
        for (text, mode) in [
            ("sacrificial", PrintBridging::Sacrificial),
            ("stepped", PrintBridging::Stepped),
        ] {
            let mut bridged = before.clone();
            bridged["print_bridging"] = serde_json::json!(text);
            let def: HoleDef<String> = serde_json::from_value(bridged.clone()).unwrap();
            assert_eq!(def.print_bridging, mode);
            assert_eq!(serde_json::to_value(&def).unwrap(), bridged);
            let mapped = def
                .map_params::<String, ()>(&mut |_, p| Ok(p.clone()))
                .unwrap();
            assert_eq!(mapped.print_bridging, mode);
        }
        let mut wrong = before;
        wrong["print_bridging"] = serde_json::json!("arched");
        assert!(serde_json::from_value::<HoleDef<String>>(wrong).is_err());
    }

    #[test]
    fn press_fits_keep_their_values_and_size_their_sections() {
        // mitcad#1517: a hole saved before has no `press_fit` and writes
        // none; a press fit writes what it was given.
        let before = serde_json::json!({
            "placement": {"type": "sketch_points", "sketch": "F3", "points": ["c5"]},
            "diameter": "d4", "extent": {"type": "through_all"}});
        let def: HoleDef<String> = serde_json::from_value(before.clone()).unwrap();
        assert_eq!(def.press_fit, None);
        assert_eq!(serde_json::to_value(&def).unwrap(), before);
        let mut ribbed = before;
        ribbed["press_fit"] = serde_json::json!({"shape": "crush_ribs",
            "interference": "press_fit_interference", "ribs": "d5", "rib_height": "d6"});
        let def: HoleDef<String> = serde_json::from_value(ribbed.clone()).unwrap();
        assert_eq!(serde_json::to_value(&def).unwrap(), ribbed);
        let mut slots = Vec::new();
        let mapped = def
            .map_params::<String, ()>(&mut |slot, p| {
                slots.push(slot.to_owned());
                Ok(p.clone())
            })
            .unwrap();
        assert_eq!(mapped.press_fit, def.press_fit);
        assert!(slots.contains(&"press_fit.interference".to_owned()));
        assert!(slots.contains(&"press_fit.ribs".to_owned()));
        let mut wrong = ribbed;
        wrong["press_fit"]["shape"] = serde_json::json!("triangle");
        assert!(serde_json::from_value::<HoleDef<String>>(wrong).is_err());

        // The flats or the rib tips lie the interference under the
        // nominal diameter.
        use crate::kernel::HolePressFit as Section;
        for (shape, angle, sides, inscribed) in [
            (PressFitShape::Hex, None, 6, 4.8),
            (PressFitShape::Square, Some(0.5), 4, 4.8),
        ] {
            let Ok(Section::Polygon {
                sides: s,
                inscribed: d,
                angle: a,
            }) = press_fit_section(shape, 5.0, 0.2, None, None, angle)
            else {
                panic!("{shape:?}");
            };
            assert_eq!(s, sides);
            assert!((d - inscribed).abs() < 1e-12, "{d}");
            assert_eq!(a, angle.unwrap_or(0.0));
        }
        let Ok(Section::CrushRibs {
            ribs, tips, bore, ..
        }) = press_fit_section(PressFitShape::CrushRibs, 8.0, 0.2, None, None, None)
        else {
            panic!("crush ribs");
        };
        assert_eq!(ribs, DEFAULT_CRUSH_RIBS);
        assert!((tips - 7.8).abs() < 1e-12 && (bore - 8.4).abs() < 1e-12);
        for (result, expected) in [
            (
                press_fit_section(PressFitShape::Hex, 5.0, 5.0, None, None, None),
                "less than the hole's diameter",
            ),
            (
                press_fit_section(PressFitShape::Hex, 5.0, 0.2, Some(4.0), None, None),
                "only crush ribs",
            ),
            (
                press_fit_section(PressFitShape::CrushRibs, 8.0, 0.2, Some(2.0), None, None),
                "whole number from 3",
            ),
            (
                press_fit_section(PressFitShape::CrushRibs, 8.0, 0.2, Some(3.5), None, None),
                "whole number from 3",
            ),
            (
                press_fit_section(PressFitShape::CrushRibs, 8.0, 0.4, None, Some(0.2), None),
                "more than half the interference",
            ),
            (
                press_fit_section(
                    PressFitShape::CrushRibs,
                    2.0,
                    0.2,
                    Some(12.0),
                    Some(0.8),
                    None,
                ),
                "do not fit round a bore",
            ),
        ] {
            let error = result.unwrap_err();
            assert!(error.contains(expected), "'{expected}' not in '{error}'");
        }
    }
}
