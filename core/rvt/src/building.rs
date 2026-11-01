// SPDX-License-Identifier: MIT
//! The building elements' own parameters (mitcad#275): what the import
//! rebuilds walls, floors, roofs, columns, beams and their openings from,
//! with the grids and the linked models. Every value is a field the schema
//! names, read by its name; lengths are in model feet, angles in radians.
//!
//! - **Curves** (`GLine`: `m_origin`, `m_dirVec`, `m_endParams`; `GArc`:
//!   `m_center`, `m_radius`, `m_xVec`, `m_yVec`, `m_endParams`, the angles
//!   from the x vector towards the y vector).
//! - **Walls** (`VWall` and its classes): the location curve
//!   (`m_pCurveDriver` → `m_pCrv`), the base and the height (the location
//!   reference face `m_pRefFaces[0]`, a `Plane` whose envelope spans the
//!   curve's length and the wall's height from the plane's origin), the
//!   type (`m_WallAttributesId`, its `CompoundStructure` layers), the joins
//!   at the two ends (`m_controlJoinsSet`: per end, the joined walls in
//!   order; the first one extends to the far face of the others), the
//!   walls joining its sides (`m_sideJoins`), the elements cutting it
//!   (`CutPairGStep.m_otherId` of its geometry steps), and its top and base
//!   attachments (`JoinToRoofGStep`: `m_ids`, `m_baseOrTop`, mitcad#348).
//! - **Reference planes** (`RefPlane`: `m_pSurface`, `m_text`), which walls
//!   attach to.
//! - **Floors and roofs** (`Floor`, `Roof`, ...): the curve loops of their
//!   extrusion helper cell (`FloorExtrusionHelper`, `RoofExtrusionHelper`:
//!   `m_pCurveLoops`, at the top of the slab), the top and bottom
//!   (`m_top`, `m_bottom`) or a roof's base level and offset and its layer
//!   reference faces (`m_pRefLayerFaces`, the planes of its slopes).
//! - **Family instances**: the symbol and transform (phase 1), the host,
//!   the level references (`FamilyLevelRef`: the base and top levels with
//!   their offsets), the location curve of a curve-driven instance (a
//!   beam's `FamInstCurveDriver`: `m_pCrv`, `m_setbacks`), and the
//!   elements cutting it.
//! - **Family types** (`FamilySymbol`): the parameters (`m_pParams` →
//!   `NamedParam`: id, value), the cut loops an insert cuts its host with
//!   (`m_pCutLoops`, in the type's coordinates) and their direction
//!   (`m_cutDirs`), else its void geometry (`m_complexCutGeoms`, B-reps,
//!   mitcad#345), and the geometry's materials (`m_geomTag2MaterialId`).
//! - **Openings** (`Opening`, `SWallRectOpening`, `ShaftOpening`): the
//!   host, the loops, a rectangular wall opening's corners
//!   (`m_oRectOpeningData` → `m_aPoint`, mitcad#345).
//! - **Parameter names**: a family parameter's element
//!   (`ParamElemFamily`, ...) → `m_pParamDef` → `m_caption`.
//! - **Grids** (`Grid`): the name (`m_text`) and the vertical surface its
//!   line lies in (`m_pSurface`: a `Plane` whose envelope's u range is the
//!   line; a cylinder's for an arc).
//! - **Links** (`RvtLinkInstance`: the transform and the link type;
//!   `RvtLinkSymbol`: the linked file's name and stored path).

use std::collections::BTreeMap;

use mitcad_vec3::{add, scale};
use serde::Serialize;

use crate::params::{self, ParamDef};
use crate::schema::Schema;
use crate::walk::{Object, Record, Value};

pub use mitcad_vec3::Vec3 as P3;

/// A curve in model feet.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Curve {
    Line {
        start: P3,
        end: P3,
    },
    /// A circular arc: `center + radius (cos t x_axis + sin t y_axis)`
    /// for t from `start` to `end` (`end > start`).
    Arc {
        center: P3,
        x_axis: P3,
        y_axis: P3,
        radius: f64,
        start: f64,
        end: f64,
    },
    /// An elliptical arc (`GEllipse`, mitcad#340): `center + rx cos t x_axis
    /// + ry sin t y_axis` for t from `start` to `end` (`end > start`).
    Ellipse {
        center: P3,
        x_axis: P3,
        y_axis: P3,
        rx: f64,
        ry: f64,
        start: f64,
        end: f64,
    },
    /// A helix (`GCylindricalHelix`, mitcad#385): `center + radius (cos t
    /// x_axis + sin t y_axis) + rise t z_axis` for t from `start` to `end`
    /// (`end > start`; `rise` per radian, `m_pitchOver2PI`).
    Helix {
        center: P3,
        x_axis: P3,
        y_axis: P3,
        z_axis: P3,
        radius: f64,
        rise: f64,
        start: f64,
        end: f64,
    },
}

/// A point moved by a transform (the 3×3 matrix by rows, then the origin).
pub fn apply(t: &[f64; 12], p: P3) -> P3 {
    std::array::from_fn(|r| t[3 * r] * p[0] + t[3 * r + 1] * p[1] + t[3 * r + 2] * p[2] + t[9 + r])
}

/// A direction turned by a transform.
pub fn turn(t: &[f64; 12], v: P3) -> P3 {
    std::array::from_fn(|r| t[3 * r] * v[0] + t[3 * r + 1] * v[1] + t[3 * r + 2] * v[2])
}

impl Curve {
    /// The point at a fraction of the curve (0 its start, 1 its end).
    pub fn at(&self, f: f64) -> P3 {
        match self {
            Curve::Line { start, end } => {
                std::array::from_fn(|k| start[k] + (end[k] - start[k]) * f)
            }
            Curve::Arc {
                center,
                x_axis,
                y_axis,
                radius,
                start,
                end,
            } => {
                let t = start + (end - start) * f;
                add(
                    *center,
                    add(
                        scale(*x_axis, radius * t.cos()),
                        scale(*y_axis, radius * t.sin()),
                    ),
                )
            }
            Curve::Ellipse {
                center,
                x_axis,
                y_axis,
                rx,
                ry,
                start,
                end,
            } => {
                let t = start + (end - start) * f;
                add(
                    *center,
                    add(scale(*x_axis, rx * t.cos()), scale(*y_axis, ry * t.sin())),
                )
            }
            Curve::Helix {
                center,
                x_axis,
                y_axis,
                z_axis,
                radius,
                rise,
                start,
                end,
            } => {
                let t = start + (end - start) * f;
                add(
                    *center,
                    add(
                        add(
                            scale(*x_axis, radius * t.cos()),
                            scale(*y_axis, radius * t.sin()),
                        ),
                        scale(*z_axis, rise * t),
                    ),
                )
            }
        }
    }

    pub fn start(&self) -> P3 {
        self.at(0.0)
    }

    pub fn end(&self) -> P3 {
        self.at(1.0)
    }

    pub fn length(&self) -> f64 {
        match self {
            Curve::Line { start, end } => ((end[0] - start[0]).powi(2)
                + (end[1] - start[1]).powi(2)
                + (end[2] - start[2]).powi(2))
            .sqrt(),
            Curve::Arc {
                radius, start, end, ..
            } => radius * (end - start),
            Curve::Helix {
                radius,
                rise,
                start,
                end,
                ..
            } => radius.hypot(*rise) * (end - start),
            // (Summed over chords: a length to compare, not a measure.)
            Curve::Ellipse { .. } => (0..64)
                .map(|k| {
                    let (a, b) = (self.at(k as f64 / 64.0), self.at((k + 1) as f64 / 64.0));
                    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt()
                })
                .sum(),
        }
    }

    /// The curve moved by a transform (rigid).
    pub fn transformed(&self, t: &[f64; 12]) -> Curve {
        match self {
            Curve::Line { start, end } => Curve::Line {
                start: apply(t, *start),
                end: apply(t, *end),
            },
            Curve::Arc {
                center,
                x_axis,
                y_axis,
                radius,
                start,
                end,
            } => Curve::Arc {
                center: apply(t, *center),
                x_axis: turn(t, *x_axis),
                y_axis: turn(t, *y_axis),
                radius: *radius,
                start: *start,
                end: *end,
            },
            Curve::Ellipse {
                center,
                x_axis,
                y_axis,
                rx,
                ry,
                start,
                end,
            } => Curve::Ellipse {
                center: apply(t, *center),
                x_axis: turn(t, *x_axis),
                y_axis: turn(t, *y_axis),
                rx: *rx,
                ry: *ry,
                start: *start,
                end: *end,
            },
            Curve::Helix {
                center,
                x_axis,
                y_axis,
                z_axis,
                radius,
                rise,
                start,
                end,
            } => Curve::Helix {
                center: apply(t, *center),
                x_axis: turn(t, *x_axis),
                y_axis: turn(t, *y_axis),
                z_axis: turn(t, *z_axis),
                radius: *radius,
                rise: *rise,
                start: *start,
                end: *end,
            },
        }
    }
}

/// A plane with the envelope of what lies in it: points `origin + u x +
/// v y` for u, v in the ranges.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Plane {
    pub origin: P3,
    pub x: P3,
    pub y: P3,
    pub u: [f64; 2],
    pub v: [f64; 2],
}

/// Where an element's end joins: the element and its end (0 the start, 1
/// the end, 2 and 3 its sides).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct JoinedEnd {
    pub element: i64,
    pub end: u8,
}

/// A layer of a wall, floor or roof type, from the exterior (or the top).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Layer {
    pub width: f64,
    pub material: Option<i64>,
    pub function: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Wall {
    pub id: i64,
    pub class: String,
    pub type_id: Option<i64>,
    /// The location curve at the wall's base.
    pub curve: Curve,
    /// The base and the height (from the location reference face), feet.
    pub base: Option<f64>,
    pub height: Option<f64>,
    pub top_level: Option<i64>,
    /// Per end (start, end): the walls joined there, in the file's order
    /// (this wall among them), and the join's kind (`m_wallJoinType`).
    pub joins: [Vec<JoinedEnd>; 2],
    pub join_types: [u64; 2],
    /// Joins its ends must not make (`m_noJoinAtEnd`).
    pub no_join: [bool; 2],
    /// The wall ends joining its sides.
    pub side_joins: Vec<JoinedEnd>,
    pub flipped: bool,
    /// The location line (`m_wallKeyRef`) and its offset.
    pub key_ref: u64,
    pub key_offset: f64,
    /// Its geometry steps' classes, in order.
    pub steps: Vec<String>,
    /// The elements whose geometry cuts it (`CutPairGStep`).
    pub cut_by: Vec<i64>,
    /// The inserts (doors, windows, openings) whose cut it takes
    /// (`WallCutoutGStep.m_instId`), and the walls embedded in it
    /// (`WallEmbedWallGStep`: a curtain wall in a basic wall).
    pub cutouts: Vec<i64>,
    /// Where its faces lie: the least and the greatest offset (feet, to
    /// the left of a line wall's location line) of its vertical reference
    /// faces (`m_pRefFaces`: the location line, the faces, the core's).
    pub faces: Option<[f64; 2]>,
    /// An edited profile: the loops of its extrusion helper cell
    /// (`VWallExtrusionHelper.m_pCurveLoops`), in the wall's vertical plane.
    pub profile: Vec<Vec<Curve>>,
    /// Its top and base attached to other elements (mitcad#348).
    pub attachments: Vec<Attachment>,
}

/// A wall's top or base attached to roofs, floors, ceilings or reference
/// planes (mitcad#348): a `JoinToRoofGStep` of its geometry steps
/// (`m_baseOrTop`: 1 its top, 0 its base; `m_ids`: the elements). The
/// attached end follows the elements' surface (a top their lower one, a
/// base their upper one), below or above the wall's own top or base.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Attachment {
    pub top: bool,
    pub elements: Vec<i64>,
}

/// A reference plane (`RefPlane`, mitcad#348): its plane (`m_pSurface`)
/// and name (`m_text`).
#[derive(Debug, Clone, Serialize)]
pub struct RefPlane {
    pub id: i64,
    pub name: String,
    pub plane: Plane,
}

#[derive(Debug, Clone, Serialize)]
pub struct Slab {
    pub id: i64,
    pub class: String,
    pub type_id: Option<i64>,
    /// The boundary loops at the slab's top (floors) or base (roofs).
    pub loops: Vec<Vec<Curve>>,
    /// Curves of the loops that are no lines or arcs.
    pub unread_curves: Vec<String>,
    pub top: Option<f64>,
    pub bottom: Option<f64>,
    /// A roof's base level and offset.
    pub base_level: Option<i64>,
    pub base_offset: Option<f64>,
    /// A roof's layer reference faces (planes of its slopes).
    pub layer_faces: Vec<Plane>,
    /// Its reference faces' planes (a pad's top and bottom).
    pub ref_faces: Vec<Plane>,
    /// An extrusion roof's extents along its profile plane's normal
    /// (`m_extrusionDepth`).
    pub extrusion: Option<[f64; 2]>,
    /// A shape edit that is in use (`SlabShapeEditGStep` not dormant).
    pub shape_edited: bool,
    pub steps: Vec<String>,
    pub cut_by: Vec<i64>,
    pub cutouts: Vec<i64>,
}

/// A family level reference: the level and the offset from it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LevelRef {
    pub level: i64,
    pub offset: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Instance {
    pub id: i64,
    /// The symbol it places and its type.
    pub symbol: Option<i64>,
    pub master: Option<i64>,
    pub transform: Option<[f64; 12]>,
    pub host: Option<i64>,
    /// Its level references (`m_refs`' `FamilyLevelRef`s), in order: the
    /// base, then the top.
    pub levels: Vec<LevelRef>,
    /// A curve-driven instance's location curve and the setbacks at its
    /// ends.
    pub curve: Option<Curve>,
    pub setbacks: [f64; 2],
    /// `m_structType` (1 a beam, 2 a brace, 3 a column, ...).
    pub struct_type: u64,
    /// Whether it stands vertical (`m_bVertical`).
    pub vertical: bool,
    pub steps: Vec<String>,
    pub cut_by: Vec<i64>,
}

/// A parameter of a type (`NamedParam`): its number (`m_value`, in the
/// file's internal units), text (`m_str`), element (`m_elemId`) and
/// integer (`m_int`: a yes or no, an integer parameter's value); which of
/// them holds the value follows from the parameter's kind
/// ([`crate::params`], mitcad#501).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Param {
    pub id: i64,
    pub value: f64,
    pub text: String,
    pub element: Option<i64>,
    pub int: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Symbol {
    pub id: i64,
    pub params: Vec<Param>,
    /// The loops an insert cuts its host with, in the type's coordinates,
    /// and the directions it cuts in.
    pub cut_loops: Vec<Vec<Curve>>,
    pub cut_dirs: Vec<P3>,
    /// The void geometry an insert cuts its host with when it has no cut
    /// loops (`m_complexCutGeoms`: B-reps in the type's coordinates,
    /// mitcad#345), and what of it did not read.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cut_solids: Vec<crate::brep::Solid>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cut_notes: Vec<String>,
    /// Its geometry's materials.
    pub materials: Vec<i64>,
    /// A curtain mullion's own symbol: its profile and path (mitcad#333).
    pub mullion: Option<MullionSym>,
    /// A curtain panel's own symbol: its cell and thickness.
    pub panel: Option<PanelSym>,
    /// A profile family type's own profile (`m_arrProfileLoops`, in the
    /// family's xy plane, feet; mitcad#340): the family's curves at the
    /// type's dimensions.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub profile_loops: Vec<Vec<Curve>>,
}

/// A curtain mullion's own symbol (`SysMullionFamSym` with a
/// `MullionSymFamRef`): its profile's class (`SysRectMullionProfile`, ...)
/// and sizes (the type's built-in parameters: widths on the two sides
/// -1007300 and -1007301, thickness -1007304, offset -1007351), its path
/// (`m_pCurve`, in the symbol's coordinates), the normal its thickness
/// runs along (`m_pathNorm`) and its flags (`m_flags`: 4 for a mullion
/// at a border, whose width lies on one side of its path).
#[derive(Debug, Clone, Serialize)]
pub struct MullionSym {
    pub profile: String,
    /// A corner (quad) mullion's two depths (-1007357, -1007358).
    pub depths: [f64; 2],
    /// A custom profile's profile family type (`m_profileId`).
    pub profile_id: Option<i64>,
    pub widths: [f64; 2],
    pub thickness: f64,
    pub offset: f64,
    pub path: Option<Curve>,
    pub normal: P3,
    pub flags: u64,
    /// A V (corner) mullion's two legs (-1007354, -1007355) and the
    /// angles they run at (`m_cornerAngles`, from `normal × path` towards
    /// `normal`; mitcad#340).
    pub legs: [f64; 2],
    pub corner_angles: [f64; 2],
    /// How its ends are trimmed to the mullions there (`m_trimEnd0`,
    /// `m_trimEnd1`: `MullionTrimNeighbor`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub trims: Vec<MullionTrim>,
}

/// How a mullion's end meets a neighbouring mullion (`MullionTrimNeighbor`,
/// mitcad#385): the neighbour's path (`m_trimPath`, its first curve, in the
/// trimmed mullion's symbol's coordinates), `m_flags` (1: the end stops at
/// the neighbour's side; 2: the two meet in a mitre), the neighbour's type
/// (`m_nbrSymbolId`) and the normal its thickness runs along
/// (`m_trimPathNorm`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MullionTrim {
    /// 0 the path's start, 1 its end.
    pub end: u8,
    pub path: Curve,
    /// The neighbour's further pieces (`m_trimPath`'s later curves: a
    /// neighbour of two pieces meeting at the end, in line or bending
    /// there; mitcad#390).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub more: Vec<Curve>,
    pub flags: u64,
    pub neighbour: Option<i64>,
    pub normal: P3,
    /// `m_direction`, `m_flip` and `m_cornerAngles` (a corner
    /// neighbour's, else -1 and -1).
    pub direction: bool,
    pub flip: bool,
    pub corner_angles: [f64; 2],
}

/// A curtain panel's own symbol (`SysPanelFamSym` with a
/// `CurtainCellSymFamRef`): its cell's loops (`m_pCurveLoops`, in the
/// symbol's coordinates) on its plane (`m_pSurf`), and its thickness and
/// offset (`m_thickness`, `m_offset`: the middle of its thickness that far
/// along y × x from the plane); an empty panel (`m_empty`) has none.
#[derive(Debug, Clone, Serialize)]
pub struct PanelSym {
    pub loops: Vec<Vec<Curve>>,
    pub plane: Option<Plane>,
    pub thickness: f64,
    pub offset: f64,
    pub empty: bool,
    /// `m_flipped` of its cell: its thickness runs the other way.
    pub flipped: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Grid {
    pub id: i64,
    pub name: String,
    /// The grid's line or arc (at its surface's lowest height).
    pub curve: Option<Curve>,
    /// Why there is no curve.
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Link {
    /// The link instance and its type.
    pub id: i64,
    pub symbol: Option<i64>,
    pub instance_name: String,
    pub transform: Option<[f64; 12]>,
    /// The linked file's name and the path the file stores.
    pub file: Option<String>,
    pub path: Option<String>,
}

/// A stair run (`StairsRun`): its steps from its bottom to its top, its
/// start and end riser lines (in plan, at the bottom), the depths of its
/// treads and the underside of its monolithic support
/// (`m_oSupportPathCurveLoops`, a sloped line at each side).
#[derive(Debug, Clone, Serialize)]
pub struct StairRun {
    pub id: i64,
    pub bottom: f64,
    pub top: f64,
    /// Risers (`m_topRiserIndex - m_baseRiserIndex`).
    pub risers: u64,
    pub treads: Vec<f64>,
    pub start: Option<Curve>,
    pub end: Option<Curve>,
    pub support: Option<Curve>,
    /// A spiral run (its record holds a `StairsSpiralRunCell`, mitcad#340):
    /// its risers turn about its centre.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub spiral: bool,
    /// A spiral run's layout (mitcad#385), when it reads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<SpiralLayout>,
}

/// A spiral run's layout (mitcad#385): its centre and its inner and outer
/// radii (the first `StairsRunInfo`: `m_origin`, the radii of
/// `m_bndBox`'s corners) and its riser lines in plan, at its bottom
/// (`m_compatibleCrvInfo.m_orderedRisers` → `m_pRiserCurve`), the first
/// the start riser's.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpiralLayout {
    pub center: P3,
    pub radii: [f64; 2],
    pub risers: Vec<Curve>,
}

/// A stair run's or landing's type's parts (`StairsRunType`,
/// `StairsLandingType`, mitcad#333): treads and risers (`m_hasTread`,
/// `m_hasRiser`; their thicknesses `m_treadThickness`, `m_riserThickness`;
/// the treads reach `m_nosingLength` over the riser below), a landing's
/// thickness (`m_thickness`) and whether it is monolithic; a run type's
/// nosing profile (`m_nosingProfileTypeId`, a profile family type,
/// mitcad#385); a run type's slanted risers (`m_riserIsSlanted`, each
/// leaning from its base the nosing length behind its tread's front up to
/// that front), whether a riser stands on the tread below it (the tread
/// reaching under it to its back, `m_riserTreadConnection` 1) or reaches
/// down behind that tread (0), and a landing type's treads and risers
/// taken from its stair's run type (`m_useSameTriserAsRun`, mitcad#395).
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct StairParts {
    pub monolithic: bool,
    pub tread: Option<f64>,
    pub riser: Option<f64>,
    pub nosing: f64,
    pub thickness: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nosing_profile: Option<i64>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub slanted: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub tread_under_riser: bool,
    /// Where the nosing profile runs (`m_nosingPlacement`): 0 along the
    /// treads' fronts only, 3 along their sides too.
    #[serde(skip_serializing_if = "is_front_only")]
    pub nosing_placement: u64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub same_as_run: bool,
}

fn is_front_only(placement: &u64) -> bool {
    *placement == 0
}

/// A stair landing (`StairsLanding`, mitcad#333): its boundary
/// (`m_oBoundary`, at its top) and its top's elevation
/// (`m_baseElevation`); the runs joined to it (mitcad#395:
/// `m_logicJoinInfo.m_logiciJoins`, each `m_peerElementId` and which of
/// its ends meets the landing, `m_peerJoinEndType`: 1 its start, 2 its
/// end).
#[derive(Debug, Clone, Serialize)]
pub struct Landing {
    pub id: i64,
    pub boundary: Vec<Vec<Curve>>,
    pub top: f64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub joins: Vec<(i64, u64)>,
}

/// A profile swept along a path (mitcad#333): a wall sweep (`WallSweep`:
/// the curves of its `WallSweepCurveGStep`, its profile
/// `m_wallSweepInfo.m_profileId`), a slab edge, fascia or gutter
/// (`SweepHost`: the `m_pathLoop` of each `SweepHostLoopData` with its
/// `m_driveVec`, its type's profile, its offsets: parameters -1012825
/// across and -1012827 up) or a rail (`ContinuousRail`: its `Path3d`'s
/// loops, `m_oPathProvider.m_path3dId`, its profile
/// `m_profileData.m_profileFamSymId`).
#[derive(Debug, Clone, Serialize)]
pub struct Sweep {
    pub id: i64,
    pub class: String,
    pub paths: Vec<Vec<Curve>>,
    /// A wall sweep's paths' sides (`WallSweepCurveGStep.m_side`, one per path,
    /// mitcad#340): 0 its profile's x across the path to the left of it
    /// (up × the path's direction), 1 mirrored to the right.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sides: Vec<u32>,
    pub path3d: Option<i64>,
    /// The profile family's type, else the type that names one.
    pub profile: Option<i64>,
    pub type_id: Option<i64>,
    /// The direction the profile's y axis keeps (else up).
    pub up: Option<P3>,
    /// Offsets across and up (feet).
    pub offsets: [f64; 2],
    /// A slab edge's `m_flipped`; a wall sweep's `WallSweepInfo.m_flipped`
    /// (mitcad#340: its profile's y mirrored, hanging down from its path).
    pub flipped: bool,
    /// A wall sweep's wall (`WallSweepCurveGStep.m_hostId`): its path runs
    /// along the wall's location line, its profile at a face.
    pub host: Option<i64>,
    /// The elements whose geometry cuts it (`CutPairGStep`, mitcad#385:
    /// copings joined to the copings they meet).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cut_by: Vec<i64>,
    /// A wall sweep's returns at its paths' free ends (mitcad#385).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub returns: Vec<SweepReturn>,
    /// The wall sweeps its paths' ends meet (mitcad#385:
    /// `WallSweepCurveGStep.m_joinedSweeps`, each a `TagEnd` with its end
    /// `m_end` and an `EndInfo` naming the other sweep, `m_elemId`): the
    /// path, its end (0, 1) and the other sweep.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub joined: Vec<(usize, u8, i64)>,
    /// A rail's supports (`ContinuousRail.m_supportIds`, mitcad#391: family
    /// instances of their own, whose `m_ownerElemId` is the rail; an
    /// instance naming the rail that the rail does not list is none of its
    /// supports).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub supports: Vec<i64>,
}

/// A wall sweep's return at an end of one of its paths (mitcad#385):
/// `WallSweepCurveGStep.m_pullBackParam0` (at starts) and
/// `m_pullBackParam1` (at ends) hold `PullBack`s (`m_end`: the wall's end
/// pulled back from, or 0xffffffff where an opening in the wall breaks the
/// path; the pull-backs `m_offsets` already applied to its curves;
/// `m_returns`), each `ReturnInfo` the angle the return turns by
/// (`m_angle`) and its length (`m_length`; 0 and -1: the corner closed as
/// a return of no length would close it).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SweepReturn {
    /// The path (an index into [`Sweep::paths`]) and its end (0 its
    /// pieces' starts, 1 their ends).
    pub path: usize,
    pub end: u8,
    /// At an opening (`m_end` 0xffffffff), not at the wall's end.
    pub at_opening: bool,
    pub angle: f64,
    pub length: f64,
}

/// An opening element (a wall or floor opening, a shaft).
#[derive(Debug, Clone, Serialize)]
pub struct Opening {
    pub id: i64,
    pub class: String,
    pub host: Option<i64>,
    /// Its boundary loops, model coordinates.
    pub loops: Vec<Vec<Curve>>,
    /// A rectangular wall opening's two corners.
    pub corners: Option<[P3; 2]>,
    /// A shaft's base and top.
    pub base: Option<f64>,
    pub top: Option<f64>,
}

/// What one record gives, by its class.
#[derive(Debug, Clone)]
pub(crate) enum Raw {
    Wall(Box<Wall>),
    Slab(Box<Slab>),
    Instance(Box<Instance>),
    Symbol(Box<Symbol>),
    Structure(Vec<Layer>),
    Grid(Grid),
    LinkInstance(Link),
    LinkSymbol {
        name: String,
        path: Option<String>,
    },
    ParamDef(ParamDef),
    Opening(Box<Opening>),
    StairRun(Box<StairRun>),
    RunType(bool, f64, StairParts),
    /// A stair run's or landing's type's parts (mitcad#333).
    StairParts(StairParts),
    Landing(Box<Landing>),
    Sweep(Box<Sweep>),
    Path3d(Vec<Vec<Curve>>, Vec<String>),
    ProfileType(i64),
    /// A continuous rail's type: its profile and its joins (mitcad#387).
    RailType(Option<i64>, RailJoin),
    /// A reference plane (mitcad#348).
    RefPlane(Box<RefPlane>),
    /// A stair repeated on other levels (mitcad#391).
    Multistory(Multistory),
    /// A building element of one of these kinds whose parameters did not
    /// read: why.
    Unread(String),
}

/// Everything read of the building elements.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Building {
    pub walls: BTreeMap<i64, Wall>,
    pub slabs: BTreeMap<i64, Slab>,
    pub instances: BTreeMap<i64, Instance>,
    pub symbols: BTreeMap<i64, Symbol>,
    /// The compound structures of wall, floor and roof types.
    pub structures: BTreeMap<i64, Vec<Layer>>,
    pub grids: Vec<Grid>,
    pub links: Vec<Link>,
    pub openings: BTreeMap<i64, Opening>,
    pub stair_runs: BTreeMap<i64, StairRun>,
    /// Stair run types: monolithic (`m_hasMonolithicSupport`), and the
    /// depth of the support under the nosing line (`m_structuralDepth`).
    pub run_types: BTreeMap<i64, (bool, f64)>,
    /// Stair run and landing types' treads and risers (mitcad#333).
    pub stair_parts: BTreeMap<i64, StairParts>,
    pub landings: BTreeMap<i64, Landing>,
    /// Wall sweeps, slab edges (fascias, gutters) and rails (mitcad#333).
    pub sweeps: BTreeMap<i64, Sweep>,
    /// 3D paths (`Path3d`: a rail's path).
    pub paths3d: BTreeMap<i64, Vec<Vec<Curve>>>,
    /// The curves of a 3D path that did not read, and why (mitcad#340; a
    /// helix, `GCylindricalHelix`, reads since mitcad#385).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub paths3d_unread: BTreeMap<i64, Vec<String>>,
    /// Types' profiles (`m_profileId`: a profile family's type).
    pub profile_types: BTreeMap<i64, i64>,
    /// Project, shared and family parameters' definitions: their names
    /// and kinds (mitcad#501).
    pub param_defs: BTreeMap<i64, ParamDef>,
    /// Elements of these kinds whose parameters did not read, and why.
    pub unread: BTreeMap<i64, String>,
    /// Reference planes (walls attach to them, mitcad#348).
    pub ref_planes: BTreeMap<i64, RefPlane>,
    /// Continuous rail types' joins (mitcad#387).
    pub rail_joins: BTreeMap<i64, RailJoin>,
    /// Stairs repeated up a multistory stair (mitcad#391), by stair.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub multistory: BTreeMap<i64, Multistory>,
    /// What an element of a stair belongs to (mitcad#391): a run's,
    /// landing's or railing's stair (`m_stairsId`), a stair support's run
    /// or landing (`m_hostCompId`), a rail's railing (`m_railSystemId`), a
    /// family instance's owner (`m_ownerElemId`: a rail's supports, a
    /// railing's balusters).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub owners: BTreeMap<i64, i64>,
}

/// A stair of a multistory stair placed again on other levels
/// (`StairsElement` with a `MultistoryStairsDataCell`, mitcad#391): its
/// base level (`m_baseLevelId`) and the levels it is repeated on
/// (`m_levelIdIndexPairSet` → `m_data`: `ElementIdPlusIndex.m_elementId`,
/// with the copy's `m_index`), each copy its base moved from the stair's
/// base level up to that level.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Multistory {
    pub base_level: i64,
    pub levels: Vec<i64>,
}

/// How a continuous rail type joins its path's pieces where they meet at
/// an angle (`ContinuousRailType`, mitcad#387): `m_defaultContinuousRailJoinType`
/// (1: a fillet) and `m_filletRadius` (feet).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RailJoin {
    pub kind: u64,
    pub fillet_radius: f64,
}

impl Building {
    pub(crate) fn add(&mut self, id: i64, raw: Raw) {
        match raw {
            Raw::Wall(w) => {
                self.walls.insert(id, *w);
            }
            Raw::Slab(s) => {
                self.slabs.insert(id, *s);
            }
            Raw::Instance(i) => {
                self.instances.insert(id, *i);
            }
            Raw::Symbol(s) => {
                self.symbols.insert(id, *s);
            }
            Raw::Structure(layers) => {
                self.structures.insert(id, layers);
            }
            Raw::Grid(g) => self.grids.push(g),
            Raw::LinkInstance(l) => self.links.push(l),
            Raw::LinkSymbol { .. } => {}
            Raw::ParamDef(d) => {
                self.param_defs.insert(id, d);
            }
            Raw::RunType(m, d, p) => {
                self.run_types.insert(id, (m, d));
                self.stair_parts.insert(id, p);
            }
            Raw::StairParts(p) => {
                self.stair_parts.insert(id, p);
            }
            Raw::Landing(l) => {
                self.landings.insert(id, *l);
            }
            Raw::Sweep(s) => {
                self.sweeps.insert(id, *s);
            }
            Raw::Path3d(p, unread) => {
                self.paths3d.insert(id, p);
                if !unread.is_empty() {
                    self.paths3d_unread.insert(id, unread);
                }
            }
            Raw::ProfileType(p) => {
                self.profile_types.insert(id, p);
            }
            Raw::RailType(p, join) => {
                if let Some(p) = p {
                    self.profile_types.insert(id, p);
                }
                self.rail_joins.insert(id, join);
            }
            Raw::StairRun(r) => {
                self.stair_runs.insert(id, *r);
            }
            Raw::Opening(o) => {
                self.openings.insert(id, *o);
            }
            Raw::Unread(why) => {
                self.unread.insert(id, why);
            }
            Raw::RefPlane(p) => {
                self.ref_planes.insert(id, *p);
            }
            Raw::Multistory(m) => {
                self.multistory.insert(id, m);
            }
        }
    }

    /// Fills the links' file names from their types, sorts what is listed.
    pub(crate) fn finish(&mut self, symbols: &BTreeMap<i64, (String, Option<String>)>) {
        for link in &mut self.links {
            if let Some((name, path)) = link.symbol.and_then(|s| symbols.get(&s)) {
                link.file = Some(name.clone());
                link.path = path.clone();
            }
        }
        self.links.sort_by_key(|l| l.id);
        self.grids.sort_by_key(|g| g.id);
    }

    /// A type parameter's value by its id, or by its family parameter's
    /// name (any of `names`, case ignored).
    pub fn param(&self, symbol: i64, ids: &[i64], names: &[&str]) -> Option<f64> {
        let s = self.symbols.get(&symbol)?;
        s.params
            .iter()
            .find(|p| ids.contains(&p.id))
            .or_else(|| {
                s.params.iter().find(|p| {
                    self.param_defs
                        .get(&p.id)
                        .is_some_and(|d| names.iter().any(|w| d.name.eq_ignore_ascii_case(w)))
                })
            })
            .map(|p| p.value)
    }

    /// The total width of a type's layers.
    pub fn width(&self, type_id: i64) -> Option<f64> {
        let layers = self.structures.get(&type_id)?;
        let w: f64 = layers.iter().map(|l| l.width).sum();
        (w > 0.0).then_some(w)
    }
}

/// Reads records' fields by name.
struct Reader<'a> {
    schema: &'a Schema,
    record: &'a Record,
}

impl<'a> Reader<'a> {
    fn get(&self, o: &'a Object, name: &str) -> Option<&'a Value> {
        o.get(self.schema, name)
    }

    /// The object a field holds or points at.
    fn obj(&self, o: &'a Object, name: &str) -> Option<&'a Object> {
        self.record.deref(self.get(o, name)?)
    }

    fn deref(&self, v: &'a Value) -> Option<&'a Object> {
        self.record.deref(v)
    }

    fn id(&self, o: &Object, name: &str) -> Option<i64> {
        self.get(o, name)
            .and_then(Value::as_id)
            .filter(|id| *id != -1)
    }

    fn f64(&self, o: &Object, name: &str) -> Option<f64> {
        self.get(o, name)
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
    }

    fn u64(&self, o: &Object, name: &str) -> Option<u64> {
        self.get(o, name).and_then(Value::as_u64)
    }

    /// A four-byte integer, read as signed.
    fn int(&self, o: &Object, name: &str) -> Option<i64> {
        self.u64(o, name).map(|v| i64::from(v as u32 as i32))
    }

    fn bool(&self, o: &Object, name: &str) -> Option<bool> {
        self.get(o, name).and_then(Value::as_bool)
    }

    fn floats(&self, o: &Object, name: &str) -> Vec<f64> {
        self.get(o, name).map(Value::floats).unwrap_or_default()
    }

    fn vec3(&self, o: &Object, name: &str) -> Option<P3> {
        let v = self.floats(o, name);
        (v.len() == 3 && v.iter().all(|x| x.is_finite())).then(|| [v[0], v[1], v[2]])
    }

    fn list(&self, o: &'a Object, name: &str) -> &'a [Value] {
        self.get(o, name).and_then(Value::as_list).unwrap_or(&[])
    }

    fn class(&self, o: &Object) -> &'a str {
        self.schema.class_name(o.class)
    }

    fn curve(&self, o: &Object) -> Result<Curve, String> {
        match self.class(o) {
            "GLine" => {
                let origin = self
                    .vec3(o, "m_origin")
                    .ok_or("a line without its origin")?;
                let dir = self
                    .vec3(o, "m_dirVec")
                    .ok_or("a line without its direction")?;
                let t = self.floats(o, "m_endParams");
                if t.len() != 2 {
                    return Err("a line without its end parameters".into());
                }
                Ok(Curve::Line {
                    start: add(origin, scale(dir, t[0])),
                    end: add(origin, scale(dir, t[1])),
                })
            }
            "GArc" => {
                let center = self
                    .vec3(o, "m_center")
                    .ok_or("an arc without its center")?;
                let radius = self.f64(o, "m_radius").ok_or("an arc without its radius")?;
                let x_axis = self
                    .vec3(o, "m_xVec")
                    .ok_or("an arc without its x vector")?;
                let y_axis = self
                    .vec3(o, "m_yVec")
                    .ok_or("an arc without its y vector")?;
                let t = self.floats(o, "m_endParams");
                if t.len() != 2 || t[1] <= t[0] {
                    return Err("an arc without its end parameters".into());
                }
                Ok(Curve::Arc {
                    center,
                    x_axis,
                    y_axis,
                    radius,
                    start: t[0],
                    end: t[1],
                })
            }
            "GEllipse" => {
                let (Some(center), Some(x_axis), Some(y_axis), Some(rx), Some(ry)) = (
                    self.vec3(o, "m_center"),
                    self.vec3(o, "m_xVec"),
                    self.vec3(o, "m_yVec"),
                    self.f64(o, "m_xLen"),
                    self.f64(o, "m_yLen"),
                ) else {
                    return Err("an ellipse without its center, axes or radii".into());
                };
                let t = self.floats(o, "m_endParams");
                if t.len() != 2 || t[1] <= t[0] {
                    return Err("an ellipse without its end parameters".into());
                }
                Ok(Curve::Ellipse {
                    center,
                    x_axis,
                    y_axis,
                    rx,
                    ry,
                    start: t[0],
                    end: t[1],
                })
            }
            // (mitcad#385: its points and vectors are `XYZ` objects.)
            "GCylindricalHelix" => {
                let xyz = |name: &str| {
                    self.get(o, name)
                        .and_then(Value::as_object)
                        .and_then(|v| self.vec3(v, "m_xyz"))
                };
                let (Some(center), Some(x_axis), Some(y_axis), Some(z_axis)) = (
                    xyz("m_basePnt"),
                    xyz("m_xVec"),
                    xyz("m_yVec"),
                    xyz("m_zVec"),
                ) else {
                    return Err("a helix without its base point or axes".into());
                };
                let (Some(radius), Some(rise)) =
                    (self.f64(o, "m_radius"), self.f64(o, "m_pitchOver2PI"))
                else {
                    return Err("a helix without its radius or pitch".into());
                };
                let t = self.floats(o, "m_endParams");
                if t.len() != 2 || t[1] <= t[0] {
                    return Err("a helix without its end parameters".into());
                }
                Ok(Curve::Helix {
                    center,
                    x_axis,
                    y_axis,
                    z_axis,
                    radius,
                    rise,
                    start: t[0],
                    end: t[1],
                })
            }
            other => Err(format!("a curve of class {other}")),
        }
    }

    fn plane(&self, o: &Object) -> Option<Plane> {
        if self.class(o) != "Plane" {
            return None;
        }
        let corners = self
            .get(o, "m_Envelope")
            .and_then(Value::as_object)
            .map(|e| self.floats(e, "m_corners"))
            .unwrap_or_default();
        if corners.len() != 4 {
            return None;
        }
        Some(Plane {
            origin: self.vec3(o, "m_origin")?,
            x: self.vec3(o, "m_xVec")?,
            y: self.vec3(o, "m_yVec")?,
            u: [corners[0], corners[2]],
            v: [corners[1], corners[3]],
        })
    }

    /// A face's surface as a plane.
    fn face_plane(&self, face: &Object) -> Option<Plane> {
        self.plane(self.obj(face, "m_pSurf")?)
    }

    /// The curve loops a list of `CurveLoop` pointers holds.
    fn loops(&self, values: &[Value], unread: &mut Vec<String>) -> Vec<Vec<Curve>> {
        let mut out = Vec::new();
        for l in values {
            let Some(l) = self.deref(l) else { continue };
            let mut curves = Vec::new();
            for c in self.list(l, "m_curves") {
                let Some(c) = self.deref(c) else { continue };
                match self.curve(c) {
                    Ok(c) => curves.push(c),
                    Err(e) => unread.push(e),
                }
            }
            if !curves.is_empty() {
                out.push(curves);
            }
        }
        out
    }

    /// Its geometry steps' classes, and the elements cutting it.
    fn steps(&self, root: &'a Object) -> (Vec<String>, Vec<i64>, Vec<i64>) {
        let mut classes = Vec::new();
        let mut cut_by = Vec::new();
        let mut cutouts = Vec::new();
        let Some(list) = self.obj(root, "m_geomSteps") else {
            return (classes, cut_by, cutouts);
        };
        for field in [
            "m_bRepFormGList",
            "m_bRepAdjustGList",
            "m_bRepCutOutGList",
            "m_bRepPostCutOutGList",
            "m_bRepTweakGList",
        ] {
            for s in self.list(list, field) {
                let Some(step) = self.deref(s) else { continue };
                let class = self.class(step);
                classes.push(class.to_string());
                match class {
                    "CutPairGStep" => {
                        if let Some(other) = self.id(step, "m_otherId") {
                            cut_by.push(other);
                        }
                    }
                    // (Of a family's void geometry too, mitcad#345; of a wall
                    // embedded in it, a curtain wall in a basic wall.)
                    "WallCutoutGStep"
                    | "WallComplexCutoutGStep"
                    | "FamilyHostCutoutGStep"
                    | "WallEmbedWallGStep" => {
                        if let Some(inst) = self.id(step, "m_instId")
                            && !cutouts.contains(&inst)
                        {
                            cutouts.push(inst);
                        }
                    }
                    _ => {}
                }
            }
        }
        (classes, cut_by, cutouts)
    }

    /// Its top and base attachments (`JoinToRoofGStep` of its geometry
    /// steps, mitcad#348).
    fn attachments(&self, root: &'a Object) -> Vec<Attachment> {
        let mut out = Vec::new();
        let Some(list) = self.obj(root, "m_geomSteps") else {
            return out;
        };
        for field in [
            "m_bRepFormGList",
            "m_bRepAdjustGList",
            "m_bRepCutOutGList",
            "m_bRepPostCutOutGList",
            "m_bRepTweakGList",
        ] {
            for s in self.list(list, field) {
                let Some(step) = self.deref(s) else { continue };
                if self.class(step) != "JoinToRoofGStep" {
                    continue;
                }
                let elements: Vec<i64> = self
                    .list(step, "m_ids")
                    .iter()
                    .filter_map(Value::as_id)
                    .filter(|id| *id > 0)
                    .collect();
                if !elements.is_empty() {
                    out.push(Attachment {
                        top: self.u64(step, "m_baseOrTop").unwrap_or(1) != 0,
                        elements,
                    });
                }
            }
        }
        out
    }

    /// A join's ends (`CDJoinInfo.m_info`).
    fn join_info(&self, info: &Object) -> (Vec<JoinedEnd>, u64) {
        let ends = self
            .list(info, "m_info")
            .iter()
            .filter_map(Value::as_object)
            .filter_map(|e| {
                Some(JoinedEnd {
                    element: self.id(e, "m_elemId")?,
                    end: self.u64(e, "m_joinedEnd")? as u8,
                })
            })
            .collect();
        (ends, self.u64(info, "m_wallJoinType").unwrap_or(0))
    }

    fn params(&self, root: &'a Object) -> Vec<(i64, f64)> {
        let Some(set) = self.obj(root, "m_pParamValueSetDouble") else {
            return Vec::new();
        };
        self.list(set, "m_paramSet")
            .iter()
            .filter_map(Value::as_object)
            .filter_map(|p| Some((self.id(p, "m_paramId")?, self.f64(p, "m_value")?)))
            .collect()
    }

    fn wall(&self, id: i64, root: &'a Object) -> Result<Wall, String> {
        let driver = self
            .obj(root, "m_pCurveDriver")
            .ok_or("no location curve driver")?;
        let curve = self
            .curve(self.obj(driver, "m_pCrv").ok_or("no location curve")?)
            .map_err(|e| format!("the location curve: {e}"))?;
        let mut joins: [Vec<JoinedEnd>; 2] = [Vec::new(), Vec::new()];
        let mut join_types = [0, 0];
        for pair in self.list(driver, "m_controlJoinsSet") {
            let Some(pair) = pair.as_object() else {
                continue;
            };
            let index = self.u64(pair, "m_index").unwrap_or(9) as usize;
            let Some(info) = self.obj(pair, "m_JoinInfo") else {
                continue;
            };
            if index < 2 {
                let (ends, kind) = self.join_info(info);
                joins[index] = ends;
                join_types[index] = kind;
            }
        }
        let mut side_joins = Vec::new();
        for s in self.list(driver, "m_sideJoins") {
            let Some(info) = self.deref(s) else { continue };
            let (ends, _) = self.join_info(info);
            side_joins.extend(ends.into_iter().filter(|e| e.element != id));
        }
        let no_join = {
            let v = self.list(driver, "m_noJoinAtEnd");
            [
                v.first().and_then(Value::as_bool).unwrap_or(false),
                v.get(1).and_then(Value::as_bool).unwrap_or(false),
            ]
        };
        // The location reference face: the curve's plane, its envelope's v
        // range the wall's height above the plane's origin.
        let location = self
            .list(root, "m_pRefFaces")
            .first()
            .and_then(|f| self.deref(f))
            .and_then(|f| self.face_plane(f));
        let (mut base, mut height) = (None, None);
        if let Some(p) = &location
            && p.y[2].abs() > 1.0 - 1e-9
            && p.v[1] > p.v[0]
        {
            base = Some(p.origin[2] + p.y[2] * p.v[0]);
            height = Some(p.v[1] - p.v[0]);
        }
        if height.is_none() {
            // An arc wall's faces are no planes: the height parameter
            // (verified against the planes of the straight walls).
            let params = self.params(root);
            height = params
                .iter()
                .find(|(p, _)| *p == -1_001_101)
                .map(|(_, v)| *v);
            base = Some(curve.start()[2]);
        }
        let (steps, cut_by, cutouts) = self.steps(root);
        let mut faces = None;
        if let Curve::Line { start, end } = &curve {
            let (dx, dy) = (end[0] - start[0], end[1] - start[1]);
            let l = dx.hypot(dy);
            if l > 1e-9 {
                let n = [-dy / l, dx / l];
                let offsets: Vec<f64> = self
                    .list(root, "m_pRefFaces")
                    .iter()
                    .filter_map(|f| self.deref(f))
                    .filter_map(|f| self.face_plane(f))
                    .filter(|p| {
                        (p.x[0] * n[0] + p.x[1] * n[1]).abs() < 1e-6 && p.y[2].abs() > 1.0 - 1e-6
                    })
                    .map(|p| (p.origin[0] - start[0]) * n[0] + (p.origin[1] - start[1]) * n[1])
                    .collect();
                if offsets.len() >= 2 {
                    let lo = offsets.iter().copied().fold(f64::INFINITY, f64::min);
                    let hi = offsets.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                    if hi - lo > 1e-6 {
                        faces = Some([lo, hi]);
                    }
                }
            }
        }
        Ok(Wall {
            faces,
            id,
            class: self.class(root).to_string(),
            type_id: self.id(root, "m_WallAttributesId"),
            curve,
            base,
            height,
            top_level: self.id(root, "m_upToLevelId"),
            joins,
            join_types,
            no_join,
            side_joins,
            flipped: self.bool(root, "m_isFlipped").unwrap_or(false),
            key_ref: self.u64(root, "m_wallKeyRef").unwrap_or(0),
            key_offset: self.f64(root, "m_locLineOffset").unwrap_or(0.0),
            steps,
            cut_by,
            cutouts,
            profile: self.helper_loops(root, &mut Vec::new()).unwrap_or_default(),
            attachments: self.attachments(root),
        })
    }

    /// A stair's copies up its multistory stair (mitcad#391), None for a
    /// stair placed once.
    fn multistory(&self, root: &'a Object) -> Option<Multistory> {
        let cells = self.obj(root, "m_cellList")?;
        let cell = self
            .list(cells, "m_cells")
            .iter()
            .filter_map(|c| self.deref(c))
            .find(|c| self.class(c) == "MultistoryStairsDataCell")?;
        let set = self.obj(cell, "m_levelIdIndexPairSet")?;
        let levels: Vec<i64> = self
            .list(set, "m_data")
            .iter()
            .filter_map(|p| self.id(self.deref(p)?, "m_elementId"))
            .collect();
        (!levels.is_empty()).then(|| Multistory {
            base_level: self.id(root, "m_baseLevelId").unwrap_or(-1),
            levels,
        })
    }

    fn ref_plane(&self, id: i64, root: &'a Object) -> Option<RefPlane> {
        Some(RefPlane {
            id,
            name: self
                .get(root, "m_text")
                .and_then(Value::as_text)
                .unwrap_or_default()
                .to_string(),
            plane: self.plane(self.obj(root, "m_pSurface")?)?,
        })
    }

    /// The curve loops of an element's extrusion helper cell
    /// (`FloorExtrusionHelper`, `RoofExtrusionHelper`,
    /// `VWallExtrusionHelper`, ...), None without one.
    fn helper_loops(&self, root: &'a Object, unread: &mut Vec<String>) -> Option<Vec<Vec<Curve>>> {
        let cells = self.obj(root, "m_cellList")?;
        self.list(cells, "m_cells").iter().find_map(|c| {
            let cell = self.deref(c)?;
            self.class(cell)
                .ends_with("ExtrusionHelper")
                .then(|| self.loops(self.list(cell, "m_pCurveLoops"), unread))
        })
    }

    fn slab(&self, id: i64, root: &'a Object) -> Result<Slab, String> {
        let mut unread = Vec::new();
        // The loops of its extrusion helper cell, else its own
        // (`ProfileRoof.m_curveLoops`).
        let loops = match self.helper_loops(root, &mut unread) {
            Some(loops) => loops,
            None if root.get(self.schema, "m_curveLoops").is_some() => {
                self.loops(self.list(root, "m_curveLoops"), &mut unread)
            }
            None => return Err("no extrusion helper with its boundary loops".into()),
        };
        let layer_faces = self
            .list(root, "m_pRefLayerFaces")
            .iter()
            .filter_map(|f| self.deref(f))
            .filter_map(|f| self.face_plane(f))
            .collect();
        let (steps, cut_by, cutouts) = self.steps(root);
        let mut shape_edited = false;
        if let Some(list) = self.obj(root, "m_geomSteps") {
            for s in self.list(list, "m_bRepFormGList") {
                if let Some(step) = self.deref(s)
                    && self.class(step) == "SlabShapeEditGStep"
                    && !self.bool(step, "m_dormant").unwrap_or(true)
                {
                    shape_edited = true;
                }
            }
        }
        let type_id = [
            "m_floorAttributesId",
            "m_roofAttributesId",
            "m_ceilingAttributesId",
        ]
        .iter()
        .find_map(|f| self.id(root, f));
        Ok(Slab {
            id,
            class: self.class(root).to_string(),
            type_id,
            loops,
            unread_curves: unread,
            top: self.f64(root, "m_top").filter(|v| v.abs() < 1e20),
            bottom: self.f64(root, "m_bottom").filter(|v| v.abs() < 1e20),
            base_level: self.id(root, "m_baseLevelId"),
            base_offset: self.f64(root, "m_baseOffset"),
            extrusion: {
                let d = self.floats(root, "m_extrusionDepth");
                (d.len() == 2).then(|| [d[0], d[1]])
            },
            layer_faces,
            ref_faces: self
                .list(root, "m_pRefFaces")
                .iter()
                .filter_map(|f| self.deref(f))
                .filter_map(|f| self.face_plane(f))
                .collect(),
            shape_edited,
            steps,
            cut_by,
            cutouts,
        })
    }

    fn instance(&self, id: i64, root: &'a Object) -> Instance {
        let mut inst = Instance {
            id,
            master: self.id(root, "m_masterSymbolId"),
            host: self.id(root, "m_hostId"),
            struct_type: self.u64(root, "m_structType").unwrap_or(0),
            vertical: self.bool(root, "m_bVertical").unwrap_or(true),
            ..Instance::default()
        };
        if let Some(info) = self.obj(root, "m_pInstanceInfo") {
            inst.symbol = self.id(info, "m_symbolId");
            inst.transform = info
                .get(self.schema, "m_Trf")
                .and_then(Value::as_object)
                .and_then(|trf| {
                    let mut m = self.floats(trf, "m_3x3");
                    m.extend(self.floats(trf, "m_or"));
                    (m.len() == 12).then(|| std::array::from_fn(|i| m[i]))
                });
        }
        for r in self.list(root, "m_refs") {
            let Some(r) = self.deref(r) else { continue };
            if self.class(r) == "FamilyLevelRef"
                && let Some(level) = self.id(r, "m_elemId")
            {
                inst.levels.push(LevelRef {
                    level,
                    offset: self.f64(r, "m_offset").unwrap_or(0.0),
                });
            }
        }
        if let Some(driver) = self.obj(root, "m_pCurveDriver")
            && let Some(c) = self.obj(driver, "m_pCrv")
        {
            inst.curve = self.curve(c).ok();
            let s = self.floats(driver, "m_setbacks");
            if s.len() == 2 {
                inst.setbacks = [s[0], s[1]];
            }
        }
        let (steps, cut_by, _) = self.steps(root);
        inst.steps = steps;
        inst.cut_by = cut_by;
        inst
    }

    /// A parameter element's definition (`m_pParamDef`): its name
    /// (`m_caption`) and the kind its class and spec give (mitcad#501).
    fn param_def(&self, root: &'a Object) -> Option<ParamDef> {
        let def = self.obj(root, "m_pParamDef")?;
        let name = self
            .get(def, "m_caption")
            .and_then(Value::as_text)
            .unwrap_or_default()
            .to_string();
        let spec = self
            .get(def, "m_specTypeId")
            .and_then(Value::as_object)
            .and_then(|s| self.get(s, "m_typeId"))
            .and_then(Value::as_text)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let kind = params::kind_of_def(|c| self.schema.is_a(def.class, c), spec.as_deref());
        Some(ParamDef { name, kind, spec })
    }

    fn symbol(&self, id: i64, root: &'a Object) -> Symbol {
        let mut s = Symbol {
            id,
            ..Symbol::default()
        };
        if let Some(params) = self.obj(root, "m_pParams") {
            for p in self.list(params, "m_params") {
                let Some(p) = p.as_object() else { continue };
                let Some(pid) = self.id(p, "m_paramId") else {
                    continue;
                };
                s.params.push(Param {
                    id: pid,
                    value: self.f64(p, "m_value").unwrap_or(0.0),
                    text: self
                        .get(p, "m_str")
                        .and_then(Value::as_text)
                        .unwrap_or_default()
                        .to_string(),
                    element: self.id(p, "m_elemId"),
                    int: self.int(p, "m_int").unwrap_or(0),
                });
            }
        }
        let mut unread = Vec::new();
        s.cut_loops = self.loops(self.list(root, "m_pCutLoops"), &mut unread);
        s.profile_loops = self.loops(self.list(root, "m_arrProfileLoops"), &mut unread);
        s.cut_dirs = self
            .list(root, "m_cutDirs")
            .iter()
            .map(Value::floats)
            .filter(|v| v.len() == 3)
            .map(|v| [v[0], v[1], v[2]])
            .collect();
        let (solids, notes) = crate::brep::solids(
            self.schema,
            self.record,
            self.list(root, "m_complexCutGeoms"),
        );
        s.cut_solids = solids;
        s.cut_notes = notes;
        s.materials = self
            .list(root, "m_geomTag2MaterialId")
            .iter()
            .filter_map(|v| match v {
                Value::Object(o) => o.fields.iter().find_map(|(_, _, v)| v.as_id()),
                other => other.as_id(),
            })
            .filter(|id| *id > 0)
            .collect();
        s.mullion = self.mullion_sym(root);
        s.panel = self.panel_sym(root);
        s
    }

    /// A curtain mullion's own symbol's profile and path; a mullion type's
    /// (no `MullionSymFamRef`) profile and sizes only.
    fn mullion_sym(&self, root: &'a Object) -> Option<MullionSym> {
        let r = self
            .record
            .pointees
            .iter()
            .find(|p| self.class(p) == "MullionSymFamRef");
        if r.is_none() && self.class(root) != "SysMullionFamSym" {
            return None;
        }
        let normal = match r {
            Some(r) => self.vec3(r, "m_pathNorm")?,
            None => [0.0; 3],
        };
        let profile = self
            .obj(root, "m_sysProfile")
            .map(|p| self.class(p).to_string())
            .unwrap_or_default();
        let params = self.params(root);
        let p = |id: i64| params.iter().find(|(k, _)| *k == id).map(|(_, v)| *v);
        Some(MullionSym {
            profile,
            profile_id: self.id(root, "m_profileId").filter(|p| *p > 0),
            widths: [p(-1_007_300).unwrap_or(0.0), p(-1_007_301).unwrap_or(0.0)],
            depths: [p(-1_007_357).unwrap_or(0.0), p(-1_007_358).unwrap_or(0.0)],
            thickness: p(-1_007_304).unwrap_or(0.0),
            offset: p(-1_007_351).unwrap_or(0.0),
            path: r
                .and_then(|r| self.obj(r, "m_pCurve"))
                .and_then(|c| self.curve(c).ok()),
            normal,
            flags: r.and_then(|r| self.u64(r, "m_flags")).unwrap_or(0),
            legs: [p(-1_007_354).unwrap_or(0.0), p(-1_007_355).unwrap_or(0.0)],
            corner_angles: r.map(|r| self.angle_pair(r)).unwrap_or([0.0, 0.0]),
            trims: ["m_trimEnd0", "m_trimEnd1"]
                .iter()
                .zip(0u8..)
                .filter_map(|(f, end)| {
                    let n = self.obj(r?, f)?;
                    let mut curves = self
                        .obj(n, "m_trimPath")
                        .map(|l| self.list(l, "m_curves"))
                        .unwrap_or_default()
                        .iter()
                        .map(|c| self.deref(c).and_then(|c| self.curve(c).ok()));
                    let path = curves.next()??;
                    let more = curves.collect::<Option<Vec<_>>>()?;
                    Some(MullionTrim {
                        end,
                        path,
                        more,
                        flags: self.u64(n, "m_flags").unwrap_or(0),
                        neighbour: self.id(n, "m_nbrSymbolId"),
                        normal: self.vec3(n, "m_trimPathNorm").unwrap_or([0.0; 3]),
                        direction: self.bool(n, "m_direction").unwrap_or(false),
                        flip: self.bool(n, "m_flip").unwrap_or(false),
                        corner_angles: self.angle_pair(n),
                    })
                })
                .collect(),
        })
    }

    /// `m_cornerAngles`: two angles (0, 0 when not two).
    fn angle_pair(&self, o: &Object) -> [f64; 2] {
        match self.floats(o, "m_cornerAngles").as_slice() {
            [a, b] => [*a, *b],
            _ => [0.0, 0.0],
        }
    }

    /// A curtain panel's own symbol's cell.
    fn panel_sym(&self, root: &'a Object) -> Option<PanelSym> {
        let r = self
            .record
            .pointees
            .iter()
            .find(|p| self.class(p) == "CurtainCellSymFamRef")?;
        let mut unread = Vec::new();
        Some(PanelSym {
            loops: self.loops(self.list(r, "m_pCurveLoops"), &mut unread),
            plane: self.obj(r, "m_pSurf").and_then(|s| self.plane(s)),
            thickness: self.f64(root, "m_thickness").unwrap_or(0.0),
            offset: self.f64(root, "m_offset").unwrap_or(0.0),
            empty: self.bool(root, "m_empty").unwrap_or(false),
            flipped: self.bool(r, "m_flipped").unwrap_or(false),
        })
    }

    fn structure(&self, root: &'a Object) -> Option<Vec<Layer>> {
        let cs = self.obj(root, "m_pCompoundStructure")?;
        Some(
            self.list(cs, "m_layers")
                .iter()
                .filter_map(|l| self.deref(l))
                .map(|l| Layer {
                    width: self.f64(l, "m_layerWidth").unwrap_or(0.0),
                    material: self.id(l, "m_materialId"),
                    function: self.u64(l, "m_layerFunction").unwrap_or(0),
                })
                .collect(),
        )
    }

    fn grid(&self, id: i64, root: &'a Object) -> Grid {
        let name = self
            .get(root, "m_text")
            .and_then(Value::as_text)
            .unwrap_or_default()
            .to_string();
        let surface = self.obj(root, "m_pSurface");
        let (curve, note) = match surface.map(|s| (self.class(s), s)) {
            Some(("Plane", s)) => match self.plane(s) {
                Some(p) if p.u[1] > p.u[0] => {
                    // The line at the envelope's lowest height.
                    let at = |u: f64| add(add(p.origin, scale(p.x, u)), scale(p.y, p.v[0]));
                    (
                        Some(Curve::Line {
                            start: at(p.u[0]),
                            end: at(p.u[1]),
                        }),
                        None,
                    )
                }
                _ => (None, Some("its plane has no extent".to_string())),
            },
            Some(("CylinderSurface" | "Cylinder", s)) => match self.cylinder_arc(s) {
                Some(c) => (Some(c), None),
                None => (None, Some("its cylinder did not read".to_string())),
            },
            Some((other, _)) => (None, Some(format!("a surface of class {other}"))),
            None => (None, Some("no surface".to_string())),
        };
        Grid {
            id,
            name,
            curve,
            note,
        }
    }

    /// An arc grid's cylinder: its axis frame, radius and envelope.
    fn cylinder_arc(&self, s: &Object) -> Option<Curve> {
        let origin = self.vec3(s, "m_origin")?;
        let x = self.vec3(s, "m_xVec")?;
        let y = self.vec3(s, "m_yVec")?;
        let radius = self
            .f64(s, "m_radius")
            .unwrap_or_else(|| (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt());
        let corners = self
            .get(s, "m_Envelope")
            .and_then(Value::as_object)
            .map(|e| self.floats(e, "m_corners"))
            .unwrap_or_default();
        if corners.len() != 4 || corners[2] <= corners[0] || radius <= 0.0 {
            return None;
        }
        let unit = |v: P3| {
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            scale(v, 1.0 / l)
        };
        Some(Curve::Arc {
            center: [origin[0], origin[1], origin[2] + corners[1]],
            x_axis: unit(x),
            y_axis: unit(y),
            radius,
            start: corners[0],
            end: corners[2],
        })
    }

    fn link_instance(&self, id: i64, root: &'a Object) -> Link {
        let info = self.obj(root, "m_pInstanceInfo");
        Link {
            id,
            symbol: info.and_then(|i| self.id(i, "m_symbolId")),
            instance_name: self
                .get(root, "m_name")
                .and_then(Value::as_text)
                .unwrap_or_default()
                .to_string(),
            transform: info.and_then(|i| {
                let trf = i.get(self.schema, "m_Trf").and_then(Value::as_object)?;
                let mut m = self.floats(trf, "m_3x3");
                m.extend(self.floats(trf, "m_or"));
                (m.len() == 12).then(|| std::array::from_fn(|k| m[k]))
            }),
            file: None,
            path: None,
        }
    }

    fn link_symbol(&self, root: &'a Object) -> Raw {
        let name = self
            .obj(root, "m_symbolInfo")
            .and_then(|i| self.get(i, "m_name"))
            .and_then(Value::as_text)
            .unwrap_or_default()
            .to_string();
        let path = self
            .get(root, "m_otherDocModelLocation")
            .and_then(Value::as_object)
            .and_then(|l| self.get(l, "m_lastKnownModelPath"))
            .and_then(Value::as_object)
            .and_then(|p| self.get(p, "m_path"))
            .and_then(Value::as_text)
            .filter(|p| !p.is_empty())
            .map(str::to_string);
        Raw::LinkSymbol { name, path }
    }

    fn stair_run(&self, id: i64, root: &'a Object) -> StairRun {
        let line = |field: &str| self.obj(root, field).and_then(|c| self.curve(c).ok());
        let treads = self
            .list(root, "m_stairsTreadInfoArr")
            .iter()
            .filter_map(|t| self.deref(t))
            .filter_map(|t| self.f64(t, "m_actDepth"))
            .collect();
        let support = self
            .list(root, "m_oSupportPathCurveLoops")
            .iter()
            .filter_map(|l| self.deref(l))
            .find_map(|l| {
                self.list(l, "m_curves")
                    .first()
                    .and_then(|c| self.deref(c))
                    .and_then(|c| self.curve(c).ok())
            });
        let index = |field: &str| self.u64(root, field).unwrap_or(0);
        let spiral = self
            .record
            .pointees
            .iter()
            .any(|p| self.class(p) == "StairsSpiralRunCell");
        StairRun {
            id,
            bottom: self.f64(root, "m_bottomElevation").unwrap_or(0.0),
            top: self.f64(root, "m_topElevation").unwrap_or(0.0),
            risers: index("m_topRiserIndex").saturating_sub(index("m_baseRiserIndex")),
            treads,
            start: line("m_oStartRiser"),
            end: line("m_oEndRiser"),
            support,
            spiral,
            layout: if spiral {
                self.spiral_layout(root)
            } else {
                None
            },
        }
    }

    /// A spiral run's centre, radii and riser lines (mitcad#385).
    fn spiral_layout(&self, root: &'a Object) -> Option<SpiralLayout> {
        let info = self
            .list(root, "m_stairsRunInfoArr")
            .iter()
            .filter_map(|i| self.deref(i))
            .next()?;
        let center = self.vec3(info, "m_origin")?;
        let corners = self
            .get(info, "m_bndBox")
            .and_then(Value::as_object)
            .map(|b| self.floats(b, "m_corners"))
            .unwrap_or_default();
        if corners.len() != 4 {
            return None;
        }
        let risers = self
            .get(root, "m_compatibleCrvInfo")
            .and_then(Value::as_object)
            .map(|l| self.list(l, "m_orderedRisers"))
            .unwrap_or(&[])
            .iter()
            .map(|r| {
                let r = self.deref(r).or_else(|| r.as_object())?;
                self.obj(r, "m_pRiserCurve")
                    .and_then(|c| self.curve(c).ok())
            })
            .collect::<Option<Vec<Curve>>>()?;
        Some(SpiralLayout {
            center,
            radii: [corners[0], corners[2]],
            risers,
        })
    }

    fn sweep(&self, id: i64, root: &'a Object) -> Sweep {
        let mut paths = Vec::new();
        let mut host = None;
        let mut up = None;
        let mut sides = Vec::new();
        let mut returns = Vec::new();
        let mut joined = Vec::new();
        let mut flipped = false;
        let mut profile = None;
        let class = self.class(root).to_string();
        // A wall sweep's curves.
        if let Some(steps) = self.obj(root, "m_geomSteps") {
            for s in self.list(steps, "m_nonBRepGList") {
                let Some(step) = self.deref(s) else { continue };
                if self.class(step) == "WallSweepCurveGStep" {
                    host = host.or_else(|| self.id(step, "m_hostId"));
                    // (Its curves moved by the sweep's offset from the wall,
                    // `m_offsetCurves`, mitcad#340; else its curves.)
                    let read = |field: &str| -> Vec<Curve> {
                        self.list(step, field)
                            .iter()
                            .filter_map(|c| self.deref(c))
                            .filter_map(|c| self.curve(c).ok())
                            .collect()
                    };
                    let mut curves = read("m_offsetCurves");
                    if curves.is_empty() {
                        curves = read("m_curves");
                    }
                    if !curves.is_empty() {
                        for (end, field) in [(0, "m_pullBackParam0"), (1, "m_pullBackParam1")] {
                            for pair in self.list(step, field) {
                                let Some(pull) = pair
                                    .as_object()
                                    .and_then(|p| self.get(p, "second"))
                                    .and_then(Value::as_object)
                                else {
                                    continue;
                                };
                                let Some(r) = self
                                    .list(pull, "m_returns")
                                    .first()
                                    .and_then(Value::as_object)
                                else {
                                    continue;
                                };
                                returns.push(SweepReturn {
                                    path: paths.len(),
                                    end,
                                    at_opening: self.u64(pull, "m_end")
                                        == Some(u64::from(u32::MAX)),
                                    angle: self.f64(r, "m_angle").unwrap_or(0.0),
                                    length: self.f64(r, "m_length").unwrap_or(0.0),
                                });
                            }
                        }
                        for pair in self.list(step, "m_joinedSweeps") {
                            let Some(pair) = pair.as_object() else {
                                continue;
                            };
                            let end = self
                                .get(pair, "first")
                                .and_then(Value::as_object)
                                .and_then(|t| self.u64(t, "m_end"));
                            let other = self
                                .get(pair, "second")
                                .and_then(Value::as_object)
                                .and_then(|i| self.id(i, "m_elemId"));
                            if let (Some(end), Some(other)) = (end, other) {
                                joined.push((paths.len(), end.min(1) as u8, other));
                            }
                        }
                        paths.push(curves);
                        sides.push(self.u64(step, "m_side").unwrap_or(0) as u32);
                    }
                }
            }
        }
        if let Some(info) = self.get(root, "m_wallSweepInfo").and_then(Value::as_object) {
            profile = self.id(info, "m_profileId");
            flipped = self.bool(info, "m_flipped").unwrap_or(false);
        }
        // A slab edge's loops.
        for d in self
            .record
            .pointees
            .iter()
            .filter(|p| self.class(p) == "SweepHostLoopData")
        {
            if let Some(l) = self.obj(d, "m_pathLoop") {
                let curves: Vec<Curve> = self
                    .list(l, "m_curves")
                    .iter()
                    .filter_map(|c| self.deref(c))
                    .filter_map(|c| self.curve(c).ok())
                    .collect();
                if !curves.is_empty() {
                    paths.push(curves);
                }
            }
            up = up.or_else(|| self.vec3(d, "m_driveVec"));
        }
        // A rail's path and profile.
        let path3d = self
            .obj(root, "m_oPathProvider")
            .and_then(|p| self.id(p, "m_path3dId"));
        if let Some(pd) = self.get(root, "m_profileData").and_then(Value::as_object) {
            profile = profile.or_else(|| self.id(pd, "m_profileFamSymId"));
        }
        let params = self.params(root);
        let p = |k: i64| {
            params
                .iter()
                .find(|(id, _)| *id == k)
                .map_or(0.0, |(_, v)| *v)
        };
        Sweep {
            id,
            class,
            paths,
            sides,
            path3d,
            profile,
            type_id: self.id(root, "m_typeId"),
            up,
            offsets: [p(-1_012_825), p(-1_012_827)],
            flipped: flipped || self.bool(root, "m_flipped").unwrap_or(false),
            host,
            cut_by: self.steps(root).1,
            returns,
            joined,
            supports: self
                .list(root, "m_supportIds")
                .iter()
                .filter_map(Value::as_id)
                .filter(|id| *id > 0)
                .collect(),
        }
    }

    fn stair_parts(&self, root: &'a Object) -> StairParts {
        let has = |f: &str| self.bool(root, f).unwrap_or(false);
        let size = |f: &str| self.f64(root, f).filter(|v| *v > 1e-9);
        StairParts {
            monolithic: has("m_hasMonolithicSupport"),
            tread: size("m_treadThickness").filter(|_| has("m_hasTread")),
            riser: size("m_riserThickness").filter(|_| has("m_hasRiser")),
            nosing: self.f64(root, "m_nosingLength").unwrap_or(0.0),
            thickness: self.f64(root, "m_thickness").unwrap_or(0.0),
            nosing_profile: self.id(root, "m_nosingProfileTypeId").filter(|p| *p > 0),
            nosing_placement: self.u64(root, "m_nosingPlacement").unwrap_or(0),
            slanted: has("m_riserIsSlanted"),
            tread_under_riser: self.u64(root, "m_riserTreadConnection") == Some(1),
            same_as_run: has("m_useSameTriserAsRun"),
        }
    }

    fn landing(&self, id: i64, root: &'a Object) -> Landing {
        let mut unread = Vec::new();
        let boundary = match self.obj(root, "m_oBoundary") {
            Some(l) => {
                let curves: Vec<Curve> = self
                    .list(l, "m_curves")
                    .iter()
                    .filter_map(|c| self.deref(c))
                    .filter_map(|c| match self.curve(c) {
                        Ok(c) => Some(c),
                        Err(e) => {
                            unread.push(e);
                            None
                        }
                    })
                    .collect();
                if curves.is_empty() || !unread.is_empty() {
                    Vec::new()
                } else {
                    vec![curves]
                }
            }
            None => Vec::new(),
        };
        let joins = self
            .obj(root, "m_logicJoinInfo")
            .map(|j| {
                self.list(j, "m_logiciJoins")
                    .iter()
                    .filter_map(|v| self.deref(v))
                    .filter_map(|j| {
                        Some((
                            self.id(j, "m_peerElementId")?,
                            self.u64(j, "m_peerJoinEndType")?,
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Landing {
            id,
            boundary,
            top: self.f64(root, "m_baseElevation").unwrap_or(0.0),
            joins,
        }
    }

    fn opening(&self, id: i64, root: &'a Object) -> Opening {
        let mut unread = Vec::new();
        let mut loops = Vec::new();
        for field in ["m_pCurveLoops", "m_profileLoops", "m_loops"] {
            let l = self.loops(self.list(root, field), &mut unread);
            if !l.is_empty() {
                loops = l;
                break;
            }
        }
        if loops.is_empty()
            && let Some(cells) = self.obj(root, "m_cellList")
        {
            for c in self.list(cells, "m_cells") {
                if let Some(cell) = self.deref(c)
                    && self.class(cell).ends_with("Helper")
                {
                    loops = self.loops(self.list(cell, "m_pCurveLoops"), &mut unread);
                }
            }
        }
        // A rectangular wall opening's corners: `RectOpeningData.m_aPoint`
        // (mitcad#345, model coordinates).
        let corners = match (self.vec3(root, "m_pt1"), self.vec3(root, "m_pt2")) {
            (Some(a), Some(b)) => Some([a, b]),
            _ => self.obj(root, "m_oRectOpeningData").and_then(|d| {
                let v = self.floats(d, "m_aPoint");
                (v.len() == 6 && v.iter().all(|x| x.is_finite()))
                    .then(|| [[v[0], v[1], v[2]], [v[3], v[4], v[5]]])
            }),
        };
        Opening {
            id,
            class: self.class(root).to_string(),
            host: self
                .id(root, "m_hostId")
                .or_else(|| self.id(root, "m_hostElemId")),
            loops,
            corners,
            base: None,
            top: None,
        }
    }
}

/// A curve element's curve (`CurveElem.m_pCurveDriver` → `m_pCrv`), of
/// some length: a line, an arc or an elliptical arc (`GEllipse`:
/// `m_center`, `m_xVec`, `m_yVec`, `m_xLen`, `m_yLen`, `m_endParams`).
pub(crate) fn curve_elem(schema: &Schema, record: &Record) -> Vec<Curve> {
    let r = Reader { schema, record };
    let Some(c) = r
        .obj(&record.root, "m_pCurveDriver")
        .and_then(|d| r.obj(d, "m_pCrv"))
    else {
        return Vec::new();
    };
    r.curve(c)
        .ok()
        .filter(|c| c.length() > 1e-9)
        .into_iter()
        .collect()
}

/// What an element of a stair belongs to (mitcad#391, see
/// [`Building::owners`]): the first of its `m_stairsId`, `m_hostCompId`,
/// `m_railSystemId` and `m_ownerElemId` that names an element.
pub(crate) fn owner(schema: &Schema, record: &Record) -> Option<i64> {
    let r = Reader { schema, record };
    [
        "m_stairsId",
        "m_hostCompId",
        "m_railSystemId",
        "m_ownerElemId",
    ]
    .iter()
    .find_map(|f| r.id(&record.root, f).filter(|id| *id > 0))
}

/// What a record gives the building model, by its class (None for
/// classes it has nothing of).
pub(crate) fn read(schema: &Schema, record: &Record, id: i64) -> Option<Raw> {
    let r = Reader { schema, record };
    let root = &record.root;
    let class = schema.class_name(root.class);
    let is = |name: &str| schema.is_a(root.class, name);
    if is("VWall") {
        return Some(match r.wall(id, root) {
            Ok(w) => Raw::Wall(Box::new(w)),
            Err(e) => Raw::Unread(format!("wall: {e}")),
        });
    }
    if matches!(class, "Floor" | "Roof" | "Ceiling" | "BuildingPad") || is("RoofBase") {
        return Some(match r.slab(id, root) {
            Ok(s) => Raw::Slab(Box::new(s)),
            Err(e) => Raw::Unread(format!("{}: {e}", class.to_lowercase())),
        });
    }
    if is("FamilyInstance") {
        return Some(Raw::Instance(Box::new(r.instance(id, root))));
    }
    if is("FamilySymbol") {
        return Some(Raw::Symbol(Box::new(r.symbol(id, root))));
    }
    if is("ContinuousRailType") {
        return Some(Raw::RailType(
            r.id(root, "m_profileId").filter(|p| *p > 0),
            RailJoin {
                kind: r.u64(root, "m_defaultContinuousRailJoinType").unwrap_or(0),
                fillet_radius: r.f64(root, "m_filletRadius").unwrap_or(0.0),
            },
        ));
    }
    if is("Symbol")
        && let Some(p) = r.id(root, "m_profileId").filter(|p| *p > 0)
    {
        return Some(Raw::ProfileType(p));
    }
    if class == "WallSweep" || is("SweepHost") || is("ContinuousRail") {
        return Some(Raw::Sweep(Box::new(r.sweep(id, root))));
    }
    if class == "Path3d" {
        let mut unread = Vec::new();
        let loops = r.loops(r.list(root, "m_curveLoops"), &mut unread);
        return Some(Raw::Path3d(loops, unread));
    }
    if is("Symbol") && root.get(schema, "m_pCompoundStructure").is_some() {
        return r.structure(root).map(Raw::Structure);
    }
    match class {
        "Grid" => return Some(Raw::Grid(r.grid(id, root))),
        "RvtLinkInstance" => return Some(Raw::LinkInstance(r.link_instance(id, root))),
        "RvtLinkSymbol" => return Some(r.link_symbol(root)),
        "StairsRun" => return Some(Raw::StairRun(Box::new(r.stair_run(id, root)))),
        "StairsRunType" => {
            return Some(Raw::RunType(
                r.bool(root, "m_hasMonolithicSupport").unwrap_or(false),
                r.f64(root, "m_structuralDepth").unwrap_or(0.0),
                r.stair_parts(root),
            ));
        }
        "StairsLandingType" => return Some(Raw::StairParts(r.stair_parts(root))),
        "StairsLanding" => return Some(Raw::Landing(Box::new(r.landing(id, root)))),
        "Opening" | "SWallRectOpening" | "ShaftOpening" => {
            return Some(Raw::Opening(Box::new(r.opening(id, root))));
        }
        "RefPlane" => return r.ref_plane(id, root).map(|p| Raw::RefPlane(Box::new(p))),
        "StairsElement" => return r.multistory(root).map(Raw::Multistory),
        _ => {}
    }
    if class.starts_with("ParamElem") {
        return r.param_def(root).map(Raw::ParamDef);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_and_transforms() {
        let arc = Curve::Arc {
            center: [1.0, 1.0, 0.0],
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 1.0, 0.0],
            radius: 2.0,
            start: 0.0,
            end: std::f64::consts::FRAC_PI_2,
        };
        let e = arc.end();
        assert!((e[0] - 1.0).abs() < 1e-12 && (e[1] - 3.0).abs() < 1e-12);
        assert!((arc.length() - std::f64::consts::PI).abs() < 1e-12);
        // A quarter turn about z and a move.
        let t = [0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 10.0, 0.0, 0.0];
        let line = Curve::Line {
            start: [0.0, 0.0, 0.0],
            end: [1.0, 0.0, 0.0],
        }
        .transformed(&t);
        assert_eq!(line.end(), [10.0, 1.0, 0.0]);
        // Half an ellipse of radii 2 and 1 (mitcad#340).
        let half = Curve::Ellipse {
            center: [0.0, 0.0, 0.0],
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 1.0, 0.0],
            rx: 2.0,
            ry: 1.0,
            start: 0.0,
            end: std::f64::consts::PI,
        };
        let (m, e) = (half.at(0.5), half.end());
        assert!(m[0].abs() < 1e-12 && (m[1] - 1.0).abs() < 1e-12);
        assert!((e[0] + 2.0).abs() < 1e-12 && e[1].abs() < 1e-12);
        // (Half the perimeter, about 4.844.)
        assert!((half.length() - 4.844).abs() < 1e-2, "{}", half.length());
        let moved = half.transformed(&t);
        assert!((moved.start()[0] - 10.0).abs() < 1e-12 && (moved.start()[1] - 2.0).abs() < 1e-12);
    }

    #[test]
    fn the_building_project_reads() {
        use crate::testfile::building as t;
        let file = crate::RvtFile::parse(t::project()).unwrap();
        let schema = file.schema().unwrap();
        let model = crate::elements::Model::read(&file, &schema).unwrap();
        assert_eq!(model.header_errors + model.object_errors, 0);
        let b = &model.building;
        assert!(b.unread.is_empty(), "{:?}", b.unread);
        let wall = &b.walls[&t::WALL_A];
        assert_eq!(wall.curve.end(), [20.0, 0.0, 0.0]);
        assert_eq!(wall.height, Some(t::HEIGHT));
        assert_eq!(wall.base, Some(0.0));
        assert_eq!(
            wall.joins[0].iter().map(|j| j.element).collect::<Vec<_>>(),
            vec![t::WALL_A, t::WALL_B]
        );
        assert_eq!(b.width(t::WALL_TYPE), Some(t::WIDTH));
        assert_eq!(b.structures[&t::WALL_TYPE][0].material, Some(t::MATERIAL));
        let floor = &b.slabs[&t::FLOOR];
        assert_eq!((floor.top, floor.bottom), (Some(0.0), Some(-0.5)));
        assert_eq!(
            floor.loops.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![4, 4]
        );
        assert_eq!(b.grids.len(), 1);
        assert_eq!(b.grids[0].name, "A");
        assert_eq!(
            b.grids[0].curve,
            Some(Curve::Line {
                start: [-5.0, -2.0, 0.0],
                end: [25.0, -2.0, 0.0]
            })
        );
        assert_eq!(b.links.len(), 1);
        assert_eq!(b.links[0].file.as_deref(), Some(t::LINKED));
        assert_eq!(b.links[0].transform.unwrap()[9], 100.0);
        assert!(wall.attachments.is_empty());
    }

    #[test]
    fn the_house_reads_its_roof_and_the_wall_attached_to_it() {
        // mitcad#348: a gable roof (two slopes, two layer faces each) and
        // a wall whose top is attached to it.
        use crate::testfile::building as t;
        let file = crate::RvtFile::parse(t::house()).unwrap();
        let schema = file.schema().unwrap();
        let model = crate::elements::Model::read(&file, &schema).unwrap();
        assert_eq!(model.header_errors + model.object_errors, 0);
        let b = &model.building;
        assert!(b.unread.is_empty(), "{:?}", b.unread);
        assert_eq!(
            b.walls[&t::WALL_C].attachments,
            vec![Attachment {
                top: true,
                elements: vec![t::ROOF]
            }]
        );
        assert!(b.walls[&t::WALL_A].attachments.is_empty());
        let roof = &b.slabs[&t::ROOF];
        assert_eq!(roof.class, "ProfileRoof");
        assert_eq!(roof.type_id, Some(t::ROOF_TYPE));
        assert_eq!(roof.layer_faces.len(), 4);
        assert_eq!(roof.loops.iter().map(Vec::len).collect::<Vec<_>>(), vec![4]);
        assert_eq!(b.width(t::ROOF_TYPE), Some(t::ROOF_THICKNESS));
        // The lower face of the first slope passes through the eave.
        let p = &roof.layer_faces[0];
        assert_eq!(p.origin, [0.0, 0.0, t::HEIGHT]);
        // mitcad#345: the door in the gable wall, its type's cut loop and
        // direction; the window's type without either.
        let door = &b.instances[&t::DOOR];
        assert_eq!(door.host, Some(t::WALL_C));
        assert_eq!(
            (door.symbol, door.master),
            (Some(t::DOOR_TYPE), Some(t::DOOR_TYPE))
        );
        assert_eq!(door.transform.unwrap()[9..], [20.0, 3.0, 0.0]);
        let door_type = &b.symbols[&t::DOOR_TYPE];
        assert_eq!(
            door_type.cut_loops.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![4]
        );
        assert_eq!(door_type.cut_dirs, vec![[0.0, 1.0, 0.0]]);
        assert_eq!(
            door_type.cut_loops[0][2].end(),
            [-t::DOOR_WIDTH / 2.0, 0.0, t::DOOR_HEIGHT]
        );
        let window_type = &b.symbols[&t::WINDOW_TYPE];
        assert!(window_type.cut_loops.is_empty() && window_type.cut_solids.is_empty());
        assert_eq!(b.instances[&t::WINDOW].host, Some(t::WALL_B));
    }
}
