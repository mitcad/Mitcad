// SPDX-License-Identifier: MIT
//! The sketch definition, the feature's stored form (moved down from the
//! model's `features/sketch.rs`, mitcad#780; the model re-exports it as
//! `mitcad_model::features::sketch`, where its evaluation stays): the
//! plane, the entities, constraints, dimensions, projections, texts,
//! patterns and offsets, their checks and the solve, and the evaluated
//! output ([`SketchOutput`]).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use mitcad_vec3::{cross, unit_above, unit_div_above};

use crate::geometry::Curve2;
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::kernel::KernelError;
use crate::kernel::{FontRequest, Kernel};
use crate::legacy::{LegacyShape, convert};
use crate::offset::{self, SketchOffset, derived_entities};
use crate::parameters::ParamId;
use crate::pattern::{self, Binding, SketchPattern};
use crate::placement::SketchPlacement;
use crate::profile::{ProfileRegion, SketchFrame};
use crate::regions::{Layout, Region, RegionCurve, best_match, regions_and_layout};
use crate::solve::{
    DimensionInput, Extra, Goal, SketchStatus, SolveError, Solved, drag_extra, solve_extra,
};
use crate::steps::SketchSteps;
use crate::text::{self, TextOutline};
use crate::topo::{FaceName, RegionKey};
use crate::{
    Constraint, ConstraintKind, ConstraintUid, Dimension, DimensionKind, Entity, EntityIndex,
    EntityKind, Projection, Ref, SketchText, TextValue, ValueRange,
};
use mitcad_vec3::dot;

use crate::datum::{Datum, DatumPlane, OriginDatum};
use crate::occurrence::OccurrenceLink;

/// The plane a sketch lies on. Origin planes have the frames of the origin
/// datums ([`OriginDatum::datum`]): XY x = +X, y = +Y (normal +Z); XZ
/// x = +X, y = −Z (normal +Y); YZ x = −Z, y = +Y (normal +X, awaiting
/// experiment K1). A construction plane (written as its feature, `"F5"`)
/// gives its datum's frame. A planar face is found by name in the bodies
/// before the sketch; its frame is every planar face's
/// ([`DatumPlane::on_plane`]): the face's outward normal, the origin's
/// projection as origin and the x axis along the projection of +X (of +Y
/// when the face faces ±X), so a face facing +Z gets the XY frame and one
/// facing +Y the XZ frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum SketchPlane {
    #[default]
    Xy,
    Xz,
    Yz,
    /// A construction plane feature before the sketch.
    Construction(FeatureUid),
    /// A planar face, written `{"face": "F2:end(r{c1})", "body": "F2.b0"}`;
    /// without `body`, every body is searched.
    Face {
        face: Box<FaceName>,
        body: Option<BodyUid>,
    },
}

impl SketchPlane {
    /// The frame of an origin plane.
    pub fn origin_frame(&self) -> Option<SketchFrame> {
        let origin = match self {
            Self::Xy => OriginDatum::Xy,
            Self::Xz => OriginDatum::Xz,
            Self::Yz => OriginDatum::Yz,
            Self::Construction(_) | Self::Face { .. } => return None,
        };
        match origin.datum() {
            Datum::Plane(plane) => Some(plane.frame()),
            _ => None,
        }
    }

    /// The frame of a plane through `point` with the outward `normal`, as
    /// for every planar face ([`DatumPlane::on_plane`]).
    pub fn face_frame(point: [f64; 3], normal: [f64; 3]) -> SketchFrame {
        DatumPlane::on_plane(point, normalize(normal)).frame()
    }
}

/// `v` divided by its length; `v` itself when it has none.
fn normalize(v: [f64; 3]) -> [f64; 3] {
    unit_div_above(v, 0.0).unwrap_or(v)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FacePlane {
    face: FaceName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    body: Option<BodyUid>,
}

impl Serialize for SketchPlane {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Xy => serializer.serialize_str("xy"),
            Self::Xz => serializer.serialize_str("xz"),
            Self::Yz => serializer.serialize_str("yz"),
            Self::Construction(uid) => serializer.collect_str(uid),
            Self::Face { face, body } => FacePlane {
                face: (**face).clone(),
                body: *body,
            }
            .serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for SketchPlane {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match serde_json::Value::deserialize(deserializer)? {
            serde_json::Value::String(name) => match name.as_str() {
                "xy" => Ok(Self::Xy),
                "xz" => Ok(Self::Xz),
                "yz" => Ok(Self::Yz),
                other => other.parse().map(Self::Construction).map_err(|_| {
                    D::Error::custom(format!(
                        "unknown plane “{other}”, expected \"xy\", \"xz\", \"yz\", a \
                         construction plane like \"F5\" or {{\"face\": ...}}"
                    ))
                }),
            },
            value @ serde_json::Value::Object(_) => {
                let plane: FacePlane = serde_json::from_value(value).map_err(D::Error::custom)?;
                Ok(Self::Face {
                    face: Box::new(plane.face),
                    body: plane.body,
                })
            }
            _ => Err(D::Error::custom(
                "a plane is \"xy\", \"xz\", \"yz\", a construction plane like \"F5\" or \
                 {\"face\": ...}",
            )),
        }
    }
}

/// The sketch's own frame in its plane's frame (imports keep the file's
/// exact sketch frame this way): the origin and unit axes in the plane
/// frame's coordinates (x, y, normal).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameDef {
    pub origin: [f64; 3],
    pub x_axis: [f64; 3],
    pub y_axis: [f64; 3],
}

impl FrameDef {
    /// This frame placed in `plane`.
    pub fn apply(&self, plane: &SketchFrame) -> SketchFrame {
        let n = plane.normal();
        let place = |v: [f64; 3]| -> [f64; 3] {
            std::array::from_fn(|i| v[0] * plane.x_axis[i] + v[1] * plane.y_axis[i] + v[2] * n[i])
        };
        let o = place(self.origin);
        SketchFrame {
            origin: std::array::from_fn(|i| plane.origin[i] + o[i]),
            x_axis: normalize(place(self.x_axis)),
            y_axis: normalize(place(self.y_axis)),
        }
    }

    /// The own frame of a new sketch that shows the design upright
    /// (mitcad#692, Create Sketch's `upright`): its y axis, the screen's up
    /// when the sketch opens, is the design's +Z projected onto the plane,
    /// or the design's +Y when the plane is horizontal (its normal along
    /// ±Z). The normal stays the plane's, the side the sketch is drawn on,
    /// so the sketch is not mirrored, and x = y × normal: the screen's
    /// horizontal stays the sketch's x. `z` and `y` are the design's axes in
    /// the sketch's coordinates (its component's, through the occurrence
    /// the sketch is shown in). None when that is the plane's own frame
    /// (the XY plane, a top face) or no direction is found.
    ///
    /// `facing` is for a plane without an outside of its own, whose normal
    /// says nothing about the side to look from (the XZ origin plane,
    /// mitcad#1060): the side the sketch is seen from, in the same
    /// coordinates. When the plane's normal points away from it, the own
    /// frame's normal is the reversed one, and x = y × that normal, so the
    /// sketch is still not mirrored: on the XZ plane, seen from the
    /// design's -Y (the view cube's FRONT), x = +X, y = +Z, normal -Y.
    pub fn upright(
        plane: &SketchFrame,
        z: [f64; 3],
        y: [f64; 3],
        facing: Option<[f64; 3]>,
    ) -> Option<FrameDef> {
        let n = plane.normal();
        // The part of a direction in the plane; none when it is (within a
        // micro-radian) along the normal.
        let in_plane = |v: [f64; 3]| {
            let along = dot(n, v);
            unit_above(std::array::from_fn(|i| v[i] - along * n[i]), 1e-6)
        };
        let up = in_plane(z).or_else(|| in_plane(y))?;
        let seen_from = match facing {
            Some(side) if dot(n, side) < -1e-9 => n.map(|c| -c),
            _ => n,
        };
        let x = cross(up, seen_from);
        let local = |v: [f64; 3]| [dot(v, plane.x_axis), dot(v, plane.y_axis), 0.0];
        let frame = FrameDef {
            origin: [0.0; 3],
            x_axis: local(x),
            y_axis: local(up),
        };
        let same = |a: [f64; 3], b: [f64; 3]| (0..3).all(|i| (a[i] - b[i]).abs() <= 1e-12);
        (!(same(frame.x_axis, [1.0, 0.0, 0.0]) && same(frame.y_axis, [0.0, 1.0, 0.0])))
            .then_some(frame)
    }

    pub(crate) fn check(&self) -> Result<(), String> {
        let finite = self
            .origin
            .iter()
            .chain(&self.x_axis)
            .chain(&self.y_axis)
            .all(|v| v.is_finite());
        let (x, y) = (self.x_axis, self.y_axis);
        let unit = |v: [f64; 3]| (dot(v, v).sqrt() - 1.0).abs() < 1e-6;
        if finite && unit(x) && unit(y) && dot(x, y).abs() < 1e-6 {
            Ok(())
        } else {
            Err("frame: the axes must be unit vectors at right angles".to_owned())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SketchDef<P = ParamId> {
    pub plane: SketchPlane,
    /// Where the plane is when it is another component's (mitcad#100,
    /// `links.rs`): a face, a construction plane or an origin plane of
    /// that component, moved into the sketch's coordinates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plane_link: Option<OccurrenceLink>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame: Option<Box<FrameDef>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<Entity>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<Constraint>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dimensions: Vec<Dimension<P>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub projections: Vec<Projection>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<SketchText>,
    /// Circular and rectangular patterns (copy groups, `pattern.rs`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub patterns: Vec<SketchPattern<P>>,
    /// Offsets with their distances (`offset.rs`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub offsets: Vec<SketchOffset<P>>,
    /// Texts whose text is a text parameter's value (mitcad#664).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub text_values: Vec<TextValue<P>>,
    /// Where the sketch lies on its plane (mitcad#787): an offset, a move,
    /// a rotation and a flip after the own frame (`placement.rs`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<Box<SketchPlacement<P>>>,
    /// Which use of a tool made which items (mitcad#1750, `steps.rs`):
    /// sketch mode's list of work steps. Not geometry: definitions that
    /// differ only in it are equal.
    #[serde(skip_serializing_if = "SketchSteps::is_empty")]
    pub steps: SketchSteps,
}

impl<P> Default for SketchDef<P> {
    fn default() -> Self {
        Self {
            plane: SketchPlane::Xy,
            plane_link: None,
            frame: None,
            entities: Vec::new(),
            constraints: Vec::new(),
            dimensions: Vec::new(),
            projections: Vec::new(),
            texts: Vec::new(),
            patterns: Vec::new(),
            offsets: Vec::new(),
            text_values: Vec::new(),
            placement: None,
            steps: SketchSteps::default(),
        }
    }
}

/// No placement: the default of files and commands without one.
fn no_placement<P>() -> Option<Box<SketchPlacement<P>>> {
    None
}

/// The file and command form; `shapes` is the transitional form of early
/// version 2 files, converted on reading.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SketchDefIn<P> {
    #[serde(default)]
    plane: SketchPlane,
    #[serde(default)]
    plane_link: Option<OccurrenceLink>,
    #[serde(default)]
    frame: Option<Box<FrameDef>>,
    #[serde(default)]
    entities: Vec<Entity>,
    #[serde(default)]
    constraints: Vec<Constraint>,
    #[serde(default = "Vec::new")]
    dimensions: Vec<Dimension<P>>,
    #[serde(default)]
    projections: Vec<Projection>,
    #[serde(default)]
    texts: Vec<SketchText>,
    #[serde(default = "Vec::new")]
    patterns: Vec<SketchPattern<P>>,
    #[serde(default = "Vec::new")]
    offsets: Vec<SketchOffset<P>>,
    #[serde(default = "Vec::new")]
    text_values: Vec<TextValue<P>>,
    #[serde(default = "no_placement")]
    placement: Option<Box<SketchPlacement<P>>>,
    #[serde(default = "Vec::new")]
    shapes: Vec<LegacyShape<P>>,
    /// None in designs saved before mitcad#1750.
    #[serde(default)]
    steps: SketchSteps,
}

impl<'de, P: Deserialize<'de>> Deserialize<'de> for SketchDef<P> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let def = SketchDefIn::<P>::deserialize(deserializer)?;
        let mut sketch = SketchDef {
            plane: def.plane,
            plane_link: def.plane_link,
            frame: def.frame,
            entities: def.entities,
            constraints: def.constraints,
            dimensions: def.dimensions,
            projections: def.projections,
            texts: def.texts,
            patterns: def.patterns,
            offsets: def.offsets,
            text_values: def.text_values,
            // An empty placement is none (mitcad#787).
            placement: def.placement.filter(|p| !p.is_empty()),
            steps: def.steps,
        };
        if !def.shapes.is_empty() {
            if !sketch.entities.is_empty() || !sketch.dimensions.is_empty() {
                return Err(D::Error::custom(
                    "a sketch has \"shapes\" (older files) or \"entities\", not both",
                ));
            }
            let converted = convert(def.shapes, &|_| None);
            sketch.entities = converted.entities;
            sketch.constraints = converted.constraints;
            sketch.dimensions = converted.dimensions;
        }
        Ok(sketch)
    }
}

/// The regions a profile reference finds ([`SketchOutput::found`]).
#[derive(Debug, Clone)]
pub struct FoundRegions<'a> {
    /// The main region first, then the other pieces in key order.
    pub regions: Vec<&'a Region>,
    /// Whether the key is the region's own.
    pub exact: bool,
}

impl FoundRegions<'_> {
    /// The regions as features use them for the reference `key`: the main
    /// one carries it, so that the faces made from it keep their names;
    /// the other pieces their own keys.
    pub fn profiles(&self, key: &RegionKey) -> Vec<ProfileRegion> {
        self.regions
            .iter()
            .enumerate()
            .map(|(i, region)| ProfileRegion {
                key: if i == 0 {
                    key.clone()
                } else {
                    region.profile.key.clone()
                },
                loops: region.profile.loops.clone(),
            })
            .collect()
    }

    /// The current keys, joined as a warning names them (`r{…} + r{…}`).
    pub fn keys(&self) -> String {
        self.regions
            .iter()
            .map(|r| r.profile.key.to_string())
            .collect::<Vec<_>>()
            .join(" + ")
    }

    /// The warning of a feature whose reference `key` to a region of
    /// `sketch` (its name) found these regions under other keys
    /// (mitcad#407); None for the region's own key.
    pub fn warning(&self, key: &RegionKey, sketch: &str) -> Option<String> {
        (!self.exact).then(|| {
            format!(
                "region {key} found as {} after a change in {sketch}",
                self.keys()
            )
        })
    }
}

/// Evaluated sketch: its frame, solved geometry and profile regions.
#[derive(Debug, Clone, PartialEq)]
pub struct SketchOutput {
    pub frame: SketchFrame,
    /// The frame before the placement on the plane (mitcad#787): the
    /// plane's frame with the sketch's own frame; `frame` when the sketch
    /// has no placement. What moves a sketch's own frame (Reorient Design)
    /// works on it.
    pub base: SketchFrame,
    pub regions: Vec<ProfileRegion>,
    /// Areas, centroids and outlines of the regions, in the same order.
    pub region_info: Vec<Region>,
    /// How the regions lie against each other, for references whose
    /// region a change renamed or split (mitcad#407).
    pub layout: Arc<Layout>,
    pub solved: Arc<Solved>,
    /// The texts' glyph outlines.
    pub texts: Arc<Vec<TextOutline>>,
    /// Entities of linked projections where their sources are now, when
    /// that differs from the definition (which keeps them where the last
    /// edit left them); see [`crate::project::with_moved`].
    pub moved: Arc<Vec<crate::Entity>>,
    /// The linked projections that did not follow their sources all the
    /// way at this evaluation (mitcad#232).
    pub follow: Arc<Vec<crate::project::ProjectionFollow>>,
    /// The regions of the sketch's own curves alone, for a sketch whose
    /// supporting face's edges bound its profiles (mitcad#1182).
    pub own: Arc<OwnRegions>,
    /// The definition with its offsets made again at this evaluation
    /// (mitcad#1234), with the stored one it was made from; None when none
    /// was. Queries show it, and edits start from it, while the stored
    /// definition is the one it was made from.
    pub retrimmed: Option<Arc<RetrimmedDef>>,
}

/// A sketch's definition with its offsets made again at an evaluation
/// (a parameter set outside the sketch, a linked projection that followed
/// its source, so that other curves vanish at their distances), and the
/// stored definition it was made from (mitcad#1234).
#[derive(Debug, PartialEq)]
pub struct RetrimmedDef {
    pub from: SketchDef,
    pub def: SketchDef,
}

/// The regions a face sketch's own curves bound without the supporting
/// face's edges, found when a reference first needs them: a reference to
/// such a region that the face's edges cut into pieces (a design saved
/// before the face's edges bounded profiles, or a face that changed under
/// the sketch) still means the whole region, not the piece most like it,
/// whose extra lobe changed the bodies made from it (mitcad#1182). Empty
/// for a sketch that is not on a face.
#[derive(Debug, Default)]
pub struct OwnRegions {
    curves: Vec<RegionCurve>,
    regions: OnceLock<Vec<Region>>,
}

impl OwnRegions {
    pub fn new(curves: Vec<RegionCurve>) -> Self {
        Self {
            curves,
            regions: OnceLock::new(),
        }
    }

    /// The regions (computed on the first call).
    pub fn regions(&self) -> &[Region] {
        self.regions
            .get_or_init(|| regions_and_layout(&self.curves).0)
    }
}

/// Equal when the curves are: the regions follow from them.
impl PartialEq for OwnRegions {
    fn eq(&self, other: &Self) -> bool {
        self.curves == other.curves
    }
}

impl SketchOutput {
    pub fn region(&self, key: &RegionKey) -> Option<&ProfileRegion> {
        self.regions.iter().find(|r| &r.key == key)
    }

    /// The regions a reference means now, the main one first (mitcad#407):
    /// the region with its key; else the regions inside the outline its
    /// key traces on the changed curves (see [`Layout::pieces`]: the region
    /// renamed, or its pieces when the change split it), the one with the
    /// most curves in common first; else, when the key traces no outline,
    /// the region with the most curves in common. None when no region
    /// shares a curve with it.
    pub fn found(&self, key: &RegionKey) -> Option<FoundRegions<'_>> {
        if let Some(region) = self.region_info.iter().find(|r| &r.profile.key == key) {
            return Some(FoundRegions {
                regions: vec![region],
                exact: true,
            });
        }
        // A region of the sketch's own curves alone that the supporting
        // face's edges now cut: the whole region, as its curves bound it
        // (mitcad#1182).
        if !key.segments().any(|s| s.curve.is_face_edge())
            && let Some(region) = self.own.regions().iter().find(|r| &r.profile.key == key)
        {
            return Some(FoundRegions {
                regions: vec![region],
                exact: true,
            });
        }
        // A supporting face that is gone must not fall back to an
        // unrelated sketch region sharing just the drawn half of its key.
        let required: Vec<_> = key
            .segments()
            .map(|s| s.curve.clone())
            .filter(|c| c.is_face_edge())
            .collect();
        if !required.is_empty() {
            let available: BTreeSet<_> = self
                .regions
                .iter()
                .flat_map(|r| &r.loops)
                .flat_map(|l| &l.segments)
                .flat_map(|s| s.key.curves())
                .filter(|c| c.is_face_edge())
                .collect();
            if required.iter().any(|c| !available.contains(c)) {
                return None;
            }
        }
        let pieces: Vec<&Region> = self
            .layout
            .pieces(key)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|i| self.region_info.get(i))
            .collect();
        let main = best_match(pieces.iter().copied(), key)
            .or_else(|| pieces.first().copied())
            .or_else(|| best_match(&self.region_info, key))?;
        let mut regions = vec![main];
        regions.extend(pieces.into_iter().filter(|r| !std::ptr::eq(*r, main)));
        Some(FoundRegions {
            regions,
            exact: false,
        })
    }

    /// The main region a reference means ([`Self::found`]), carrying the
    /// referenced key so that the faces made from it keep their names.
    /// None when no region shares a curve with it.
    pub fn resolve_region(&self, key: &RegionKey) -> Option<ProfileRegion> {
        self.resolved(key).map(|region| ProfileRegion {
            key: key.clone(),
            loops: region.loops.clone(),
        })
    }

    /// The main region a reference means (as [`Self::found`] finds it)
    /// under its own, current key: what the application compares two
    /// references by (mitcad#199).
    pub fn resolved(&self, key: &RegionKey) -> Option<&ProfileRegion> {
        self.found(key).map(|found| &found.regions[0].profile)
    }

    pub fn status(&self) -> &SketchStatus {
        &self.solved.status
    }
}

impl<P> SketchDef<P> {
    pub const TYPE: &'static str = "sketch";
    pub const BASE_NAME: &'static str = "Sketch";

    pub fn new(plane: SketchPlane) -> Self {
        Self {
            plane,
            ..Self::default()
        }
    }

    // Written by hand: dimensions, patterns and offsets map their own values.
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<SketchDef<Q>, E> {
        Ok(SketchDef {
            plane: self.plane.clone(),
            plane_link: self.plane_link.clone(),
            frame: self.frame.clone(),
            entities: self.entities.clone(),
            constraints: self.constraints.clone(),
            dimensions: self
                .dimensions
                .iter()
                .map(|d| d.map_value(f))
                .collect::<Result<_, _>>()?,
            projections: self.projections.clone(),
            texts: self.texts.clone(),
            patterns: self
                .patterns
                .iter()
                .map(|p| p.map_value(f))
                .collect::<Result<_, _>>()?,
            offsets: self
                .offsets
                .iter()
                .map(|o| o.map_value(f))
                .collect::<Result<_, _>>()?,
            text_values: self
                .text_values
                .iter()
                .map(|v| v.map_value(f))
                .collect::<Result<_, _>>()?,
            placement: match &self.placement {
                Some(p) => Some(Box::new(p.map_value(f)?)),
                None => None,
            },
            steps: self.steps.clone(),
        })
    }

    pub fn entity(&self, id: EntityUid) -> Option<&Entity> {
        self.entities.iter().find(|e| e.id == id)
    }

    pub fn entity_mut(&mut self, id: EntityUid) -> Option<&mut Entity> {
        self.entities.iter_mut().find(|e| e.id == id)
    }

    /// The first entity id not used yet (points, curves and texts share
    /// the numbers).
    pub fn next_entity(&self) -> EntityUid {
        let used = self
            .entities
            .iter()
            .map(|e| e.id.0)
            .chain(self.texts.iter().map(|t| t.id.0));
        EntityUid(used.max().map_or(1, |n| n + 1))
    }

    /// The first constraint id not used yet (constraints, dimensions,
    /// patterns and offsets share the numbers).
    pub fn next_constraint(&self) -> ConstraintUid {
        let used = self
            .constraints
            .iter()
            .map(|c| c.id.0)
            .chain(self.dimensions.iter().map(|d| d.id.0))
            .chain(self.patterns.iter().map(|p| p.id.0))
            .chain(self.offsets.iter().map(|o| o.id.0));
        ConstraintUid(used.max().map_or(1, |n| n + 1))
    }

    /// The curves that bound profiles: not construction.
    pub fn profile_curves(&self) -> impl Iterator<Item = &Entity> {
        self.entities
            .iter()
            .filter(|e| !e.is_point() && !e.construction)
    }

    /// The frame from the plane's frame and the stored override: the base
    /// frame, which the placement on the plane (mitcad#787, its values
    /// parameters) moves further at evaluation ([`SketchOutput::base`]).
    pub fn place(&self, plane: &SketchFrame) -> SketchFrame {
        match &self.frame {
            Some(frame) => frame.apply(plane),
            None => *plane,
        }
    }
}

impl SketchDef {
    /// A copy whose texts that follow text parameters (`text_values`,
    /// mitcad#664) have the parameters' texts, given by `text_of`.
    pub fn with_parameter_texts(
        &self,
        text_of: &mut dyn FnMut(ParamId) -> Result<String, String>,
    ) -> Result<SketchDef, String> {
        let mut def = self.clone();
        for v in &self.text_values {
            let text = text_of(v.value).map_err(|e| format!("text t{}: {e}", v.text.0))?;
            if let Some(t) = def.texts.iter_mut().find(|t| t.id == v.text) {
                t.text = text;
            }
        }
        Ok(def)
    }

    /// The parameter a text follows (mitcad#664), if it follows one.
    pub fn text_value(&self, text: EntityUid) -> Option<ParamId> {
        self.text_values
            .iter()
            .find(|v| v.text == text)
            .map(|v| v.value)
    }

    /// The solver inputs for driving dimensions with `value`'s values
    /// (checked against their ranges), driven ones without. Inactive ones
    /// (mitcad#721) are left out.
    pub fn dimension_inputs(
        &self,
        value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
        name: &dyn Fn(ParamId) -> String,
    ) -> Result<Vec<DimensionInput<'_>>, String> {
        self.dimension_inputs_where(true, value, name)
    }

    /// The solver inputs of the dimensions the solve applies (`applied`)
    /// or of those it leaves out ([`SketchDef::dimension_applied`]); only the
    /// applied ones' values are checked against their ranges.
    fn dimension_inputs_where(
        &self,
        applied: bool,
        value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
        name: &dyn Fn(ParamId) -> String,
    ) -> Result<Vec<DimensionInput<'_>>, String> {
        self.dimensions
            .iter()
            .filter(|d| self.dimension_applied(d) == applied)
            .map(|d| {
                let v = match d.value {
                    Some(id) => {
                        let v = value(id)?;
                        if applied {
                            check_range(d.kind.range(), v)
                                .map_err(|e| format!("{} {e}", name(id)))?;
                        }
                        Some(v)
                    }
                    None => None,
                };
                Ok(DimensionInput {
                    id: d.id,
                    kind: &d.kind,
                    value: v,
                })
            })
            .collect()
    }

    /// The solver bindings of the patterns' copies with `value`'s values
    /// (checked against their ranges).
    pub fn pattern_bindings(
        &self,
        value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
        name: &dyn Fn(ParamId) -> String,
    ) -> Result<Vec<Binding>, String> {
        let index = EntityIndex::new(&self.entities);
        let mut out = Vec::new();
        for pattern in &self.patterns {
            let mut values = Vec::new();
            for (slot, id) in pattern.slots().iter().zip(pattern.values()) {
                let v = value(*id)?;
                pattern::check_value(slot, v).map_err(|e| format!("{} {e}", name(*id)))?;
                values.push(v);
            }
            out.extend(pattern.bindings(&values, &index));
        }
        Ok(out)
    }

    /// Solves with the given dimension, pattern and offset values (mm,
    /// rad): the solver moves everything but the curves of derived offsets,
    /// which are then computed from the solution.
    pub fn solve_with(
        &self,
        value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
        name: &dyn Fn(ParamId) -> String,
    ) -> Result<Solved, SolveError> {
        self.solve_goals(value, name, &[])
    }

    /// [`SketchDef::solve_with`], then pulls the goals as far as the
    /// constraints allow (a drag) when there are any.
    pub fn solve_goals(
        &self,
        value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
        name: &dyn Fn(ParamId) -> String,
        goals: &[Goal],
    ) -> Result<Solved, SolveError> {
        self.solve_from(value, name, goals, None)
    }

    /// [`SketchDef::solve_with`] from this definition's positions, with
    /// every orientation choice of the solve taken where `oriented` has the
    /// geometry: the side of each point-line, line-line and tangent
    /// distance (an offset's distances among them), of each tangency, an
    /// angle's quadrant. `oriented` is the definition as stored when its
    /// linked projections moved to their sources' new places before the
    /// solve (mitcad#1047): a point dimensioned 0.5 mm from a projected
    /// line that moved 0.5 mm toward it lay on the moved line, and its side
    /// came out by chance; geometry tied to a projection keeps the side it
    /// has in the stored sketch, whatever the size of the move. Its entities
    /// are matched by id; one this definition lacks is not used.
    pub fn solve_oriented(
        &self,
        value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
        name: &dyn Fn(ParamId) -> String,
        oriented: &[Entity],
    ) -> Result<Solved, SolveError> {
        self.solve_from(value, name, &[], Some(oriented))
    }

    fn solve_from(
        &self,
        value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
        name: &dyn Fn(ParamId) -> String,
        goals: &[Goal],
        oriented: Option<&[Entity]>,
    ) -> Result<Solved, SolveError> {
        let inputs = self
            .dimension_inputs(value, name)
            .map_err(SolveError::new)?;
        let mut bindings = self
            .pattern_bindings(value, name)
            .map_err(SolveError::new)?;
        let patterns = bindings.len();
        // Offsets' tangent corners where their results touch (mitcad#890).
        bindings.extend(offset::tangent_corners(
            &self.offsets,
            &self.entities,
            &self.constraints,
        ));
        // With them, the offsets' helpers held in place and their own
        // constraints that the corners imply left out (mitcad#1171).
        let (anchors, left_out) = offset::independent(
            &self.offsets,
            &self.entities,
            &self.constraints,
            &self.dimensions,
            &bindings[patterns..],
        );
        bindings.extend(anchors);
        let restating: BTreeSet<ConstraintUid> = self
            .offsets
            .iter()
            .filter(|o| !o.derived)
            .map(|o| o.id)
            .collect();
        let index = EntityIndex::new(&self.entities);
        let skip = derived_entities(&self.offsets, &index);
        self.check_derived(&skip).map_err(SolveError::new)?;
        // Offsets made by constraints start where their sources' offsets
        // are now (mitcad#641: a source moved farther than the distance
        // before the solve flipped them); in the geometry the orientation
        // comes from too, as they are there.
        let seeds = self.offsets.iter().any(|o| !o.derived);
        let seeded = seeds.then(|| {
            let mut entities = self.entities.clone();
            offset::seed(&self.offsets, &mut entities, value);
            entities
        });
        let entities = seeded.as_deref().unwrap_or(&self.entities);
        let seeded_oriented = oriented.filter(|_| seeds).map(|o| {
            let mut entities = o.to_vec();
            offset::seed(&self.offsets, &mut entities, value);
            entities
        });
        let oriented = seeded_oriented.as_deref().or(oriented);
        let extra = Extra {
            bindings: &bindings,
            skip: &skip,
            oriented,
            left_out: &left_out,
            restating: &restating,
        };
        let run = |extra: &Extra<'_>| {
            if goals.is_empty() {
                solve_extra(entities, &self.constraints, &inputs, extra)
            } else {
                drag_extra(entities, &self.constraints, &inputs, extra, goals)
            }
        };
        let mut solved = match run(&extra) {
            Ok(solved) => solved,
            // A sketch that fails says why as it did without the corners
            // held (mitcad#890): their tangencies restate what the offsets'
            // own constraints imply where the sources touch, and the
            // explanation at the failed solve's least-squares point counted
            // them, so that every constraint was named as contradicting.
            // Should the sketch solve without them, the failure is theirs
            // and its own explanation stands. (Without them, with every
            // constraint of the offsets, mitcad#1171.)
            Err(error) if bindings.len() > patterns => {
                let plain = Extra {
                    bindings: &bindings[..patterns],
                    skip: &skip,
                    oriented,
                    ..Extra::default()
                };
                return Err(run(&plain).err().unwrap_or(error));
            }
            Err(error) => return Err(error),
        };
        // What of the inactive constraints and dimensions the solution does
        // not meet (mitcad#721).
        if self.has_inactive() {
            let mut at = entities.to_vec();
            solved.store(&mut at);
            let left_out = self
                .dimension_inputs_where(false, value, name)
                .unwrap_or_default();
            let inactive: BTreeSet<ConstraintUid> = self
                .constraints
                .iter()
                .filter(|c| !c.active)
                .map(|c| c.id)
                .chain(left_out.iter().map(|d| d.id))
                .collect();
            let plain = Extra {
                skip: &skip,
                ..Extra::default()
            };
            solved.status.unmet_inactive =
                crate::solve::unmet(&at, &self.constraints, &left_out, &plain)
                    .into_iter()
                    .filter(|k| inactive.contains(k))
                    .collect();
            // An inactive driven dimension constrains nothing either way:
            // it measures as an active one does, so that its reference
            // parameter never keeps a stale value (mitcad#723).
            for d in left_out.iter().filter(|d| d.value.is_none()) {
                if let Some(v) = crate::solve::measure_in(d.kind, &solved, &self.constraints) {
                    solved.status.driven.insert(d.id, v);
                }
            }
        }
        for o in &self.offsets {
            let v = value(o.distance).map_err(SolveError::new)?;
            if !(v.is_finite() && v > 0.0) {
                return Err(SolveError::new(format!(
                    "{} must be greater than zero, got {v}",
                    name(o.distance)
                )));
            }
        }
        offset::regenerate(&self.offsets, &self.entities, &mut solved, value)
            .map_err(SolveError::new)?;
        // Derived curves are as determined as their sources.
        for o in self.offsets.iter().filter(|o| o.derived) {
            if o.curves
                .iter()
                .all(|c| solved.status.fully_constrained.contains(c))
            {
                let made = derived_entities(std::slice::from_ref(o), &index);
                solved.status.fully_constrained.extend(made);
            }
        }
        Ok(solved)
    }

    /// The profile regions of solved geometry and laid out texts.
    pub fn regions_of(&self, solved: &Solved, texts: &[TextOutline]) -> Vec<Region> {
        self.regions_and_layout_of(solved, texts).0
    }

    /// [`Self::regions_of`] with how the regions lie against each other.
    pub fn regions_and_layout_of(
        &self,
        solved: &Solved,
        texts: &[TextOutline],
    ) -> (Vec<Region>, Layout) {
        regions_and_layout(&self.region_curves_of(solved, texts))
    }

    /// Transient profile geometry, without adding entities to the sketch.
    /// Supporting-face boundaries can be combined with these curves by
    /// the model before finding the regions.
    pub fn region_curves_of(&self, solved: &Solved, texts: &[TextOutline]) -> Vec<RegionCurve> {
        let mut curves: Vec<RegionCurve> = self
            .profile_curves()
            .filter_map(|e| Some(RegionCurve::new(e.id, solved.curves.get(&e.id)?.clone())))
            .collect();
        for t in texts {
            curves.extend(t.region_curves());
        }
        curves
    }

    /// The texts laid out with the kernel's fonts, or with the outlines
    /// stored with a text whose font is not installed. Without font support
    /// (a test kernel) the texts have no outlines.
    pub fn text_outlines<K: Kernel + ?Sized>(
        &self,
        solved: &Solved,
        kernel: &K,
    ) -> Result<Vec<TextOutline>, String> {
        let mut out = Vec::new();
        for t in &self.texts {
            // A QR code's modules (mitcad#473).
            if let Some(code) = &t.code {
                out.push(crate::code::outline(t, code)?);
                continue;
            }
            // A font file's outlines, stored with the text (mitcad#473).
            if let Some(found) = text::font_file_glyphs(t) {
                let (glyphs, stored) = found?;
                let mut outline = text::layout(t, &glyphs, &solved.points, &solved.curves)?;
                outline.stored = stored;
                out.push(outline);
                continue;
            }
            // Right-to-left and joined scripts in an installed font: shaped
            // from the font's file (mitcad#473).
            if crate::fonts::needs_shaping(&t.text)
                && let Ok(source) = kernel.font_source(&text::request(t))
                && let Ok(font) =
                    crate::fonts::FontData::read(std::path::Path::new(&source.path), source.index)
            {
                let mut glyphs = font.shape(&t.text, source.synthetic_italic).glyphs;
                glyphs.family = source.family;
                out.push(text::layout(t, &glyphs, &solved.points, &solved.curves)?);
                continue;
            }
            let mut glyphs = match kernel.font_glyphs(&text::request(t)) {
                Ok(glyphs) => glyphs,
                Err(KernelError::Unsupported(_)) => return Ok(Vec::new()),
                Err(e) => return Err(format!("text t{}: {e}", t.id.0)),
            };
            if t.cap_height && glyphs.cap_height <= 0.0 {
                // The font's capital height: the top of an A (the glyph the file's own scale
                // follows: its apex is flat in a sans-serif face, but reaches the
                // serif overshoot of a roman one, 1387 against 1356 units for an H).
                let request = FontRequest {
                    text: "A",
                    ..text::request(t)
                };
                glyphs.cap_height = kernel
                    .font_glyphs(&request)
                    .map_err(|e| format!("text t{}: {e}", t.id.0))?
                    .glyphs
                    .first()
                    .into_iter()
                    .flat_map(|g| g.contours.iter().flatten().flatten())
                    .map(|p| p[1])
                    .fold(0.0, f64::max);
            }
            // Without kerning (mitcad#1177): each character's advance is
            // its own, as the font gives it for the character alone.
            if t.no_kerning {
                let mut own: BTreeMap<char, f64> = BTreeMap::new();
                for (c, g) in t.text.chars().zip(glyphs.glyphs.iter_mut()) {
                    if c == '\n' {
                        continue;
                    }
                    let advance = match own.get(&c) {
                        Some(a) => *a,
                        None => {
                            let one = c.to_string();
                            let request = FontRequest {
                                text: &one,
                                ..text::request(t)
                            };
                            let a = kernel
                                .font_glyphs(&request)
                                .map_err(|e| format!("text t{}: {e}", t.id.0))?
                                .glyphs
                                .first()
                                .map_or(g.advance, |alone| alone.advance);
                            own.insert(c, a);
                            a
                        }
                    };
                    g.advance = advance;
                }
            }
            // The outlines the file stored with the text while its font is
            // missing (mitcad#416).
            let stored = text::stored_glyphs(t, &glyphs);
            let mut outline = text::layout(
                t,
                stored.as_ref().unwrap_or(&glyphs),
                &solved.points,
                &solved.curves,
            )?;
            outline.stored = stored.is_some();
            out.push(outline);
        }
        Ok(out)
    }
}

fn check_range(range: ValueRange, v: f64) -> Result<(), String> {
    let ok = match range {
        ValueRange::Positive => v.is_finite() && v > 0.0,
        ValueRange::NotNegative => v.is_finite() && v >= 0.0,
        ValueRange::Angle => (0.0..=std::f64::consts::PI).contains(&v),
        ValueRange::Signed => v.is_finite(),
    };
    if ok {
        Ok(())
    } else {
        Err(match range {
            ValueRange::Positive => format!("must be greater than zero, got {v}"),
            ValueRange::NotNegative => format!("must not be negative, got {v}"),
            ValueRange::Signed => format!("must be a finite number, got {v}"),
            ValueRange::Angle => {
                format!("must be between 0 and 180 degrees, got {}°", v.to_degrees())
            }
        })
    }
}

/// What kinds of entities a constraint or dimension accepts.
fn check_kinds(index: &EntityIndex<'_>, what: &str, refs: &[(Ref, &[&str])]) -> Result<(), String> {
    for (r, kinds) in refs {
        let entity = index
            .get(r.uid())
            .ok_or_else(|| format!("{what}: {r} does not exist"))?;
        if entity.is_point() != r.is_point() {
            return Err(format!("{what}: {r} is a {}", entity.kind.type_name()));
        }
        if !kinds.is_empty() && !kinds.contains(&entity.kind.type_name()) {
            return Err(format!(
                "{what}: {r} is a {}, expected {}",
                entity.kind.type_name(),
                kinds.join(" or ")
            ));
        }
    }
    Ok(())
}

const LINE: &[&str] = &["line"];
const ROUND: &[&str] = &["circle", "arc"];
const ANY: &[&str] = &[];

impl<P> SketchDef<P> {
    /// Whether the solve applies a dimension (mitcad#721): an active one,
    /// unless it is the spacing of an inactive equal spacing.
    pub fn dimension_applied(&self, d: &Dimension<P>) -> bool {
        d.active
            && match d.kind {
                DimensionKind::Spacing { constraint } => self
                    .constraints
                    .iter()
                    .find(|c| c.id == constraint)
                    .is_none_or(|c| c.active),
                _ => true,
            }
    }

    /// Whether any constraint or dimension is inactive (mitcad#721).
    pub fn has_inactive(&self) -> bool {
        self.constraints.iter().any(|c| !c.active) || self.dimensions.iter().any(|d| !d.active)
    }

    /// Derived offset curves are computed, not solved: nothing may
    /// constrain or pattern them.
    fn check_derived(&self, derived: &BTreeSet<EntityUid>) -> Result<(), String> {
        if derived.is_empty() {
            return Ok(());
        }
        let refs = self
            .constraints
            .iter()
            .map(|c| (c.id, c.kind.refs()))
            .chain(self.dimensions.iter().map(|d| (d.id, d.kind.refs())));
        for (id, refs) in refs {
            if let Some(r) = refs.iter().find(|r| derived.contains(&r.uid())) {
                return Err(format!(
                    "{id}: {r} follows an offset of a spline or an ellipse and cannot be \
                     constrained; constrain its source"
                ));
            }
        }
        for p in &self.patterns {
            if let Some(r) = p.entities.iter().find(|r| derived.contains(&r.uid())) {
                return Err(format!(
                    "{}: {r} follows an offset and cannot be patterned",
                    p.id
                ));
            }
        }
        Ok(())
    }

    /// The consistency of entities, constraints and dimensions.
    pub fn check_definition(&self) -> Result<(), String> {
        // Points, curves and texts share one id numbering (mitcad#1485):
        // the first one keeps its number and a later one is named with it.
        let mut ids: BTreeMap<EntityUid, String> = BTreeMap::new();
        let mut number = |what: &str, id: EntityUid, name: String| -> Result<(), String> {
            if id.0 == 0 {
                return Err(format!("{what} {name}: id 0 is not allowed"));
            }
            match ids.get(&id) {
                Some(first) if *first == name => {
                    Err(format!("{what} {name} is used more than once"))
                }
                Some(first) => Err(format!(
                    "{what} {name} has the id number of {first}: curves, points and texts \
                     share one numbering"
                )),
                None => {
                    ids.insert(id, name);
                    Ok(())
                }
            }
        };
        for e in &self.entities {
            number("entity", e.id, e.as_ref().to_string())?;
        }
        for t in &self.texts {
            number("text", t.id, format!("t{}", t.id.0))?;
        }
        let index = EntityIndex::new(&self.entities);
        for t in &self.texts {
            text::check(t, &index)?;
        }
        // Texts from text parameters (mitcad#664): one value per text.
        let mut valued = BTreeSet::new();
        for v in &self.text_values {
            if !self.texts.iter().any(|t| t.id == v.text) {
                return Err(format!("text_values: text t{} does not exist", v.text.0));
            }
            if !valued.insert(v.text) {
                return Err(format!("text_values: text t{} has two values", v.text.0));
            }
        }
        let point = |what: &str, id: EntityUid| -> Result<(), String> {
            match index.get(id) {
                Some(e) if e.is_point() => Ok(()),
                Some(e) => Err(format!("{what}: p{} is a {}", id.0, e.kind.type_name())),
                None => Err(format!("{what}: point p{} does not exist", id.0)),
            }
        };
        for e in &self.entities {
            let what = e.as_ref().to_string();
            for p in e.kind.points() {
                point(&what, p)?;
            }
            let finite = |v: f64| v.is_finite();
            match &e.kind {
                EntityKind::Point { at } => {
                    if !at.iter().all(|v| v.is_finite()) {
                        return Err(format!("{what} must be a finite point, got {at:?}"));
                    }
                }
                EntityKind::Line { start, end, .. } if start == end => {
                    return Err(format!("{what}: a line needs two points"));
                }
                EntityKind::Circle { radius, .. } if !(finite(*radius) && *radius > 0.0) => {
                    return Err(format!("{what}: the radius must be positive"));
                }
                EntityKind::Arc { center, start, end }
                    if center == start || center == end || start == end =>
                {
                    return Err(format!("{what}: an arc needs three points"));
                }
                EntityKind::Ellipse { minor_radius, .. }
                | EntityKind::EllipticalArc { minor_radius, .. }
                    if !(finite(*minor_radius) && *minor_radius > 0.0) =>
                {
                    return Err(format!("{what}: the minor radius must be positive"));
                }
                EntityKind::Spline {
                    degree,
                    control,
                    weights,
                    knots,
                } => {
                    let (p, n) = (*degree as usize, control.len());
                    if p == 0 || n < p + 1 {
                        return Err(format!(
                            "{what}: a spline of degree {p} needs at least {} control points",
                            p + 1
                        ));
                    }
                    if !weights.is_empty()
                        && (weights.len() != n
                            || weights.iter().any(|w| !(w.is_finite() && *w > 0.0)))
                    {
                        return Err(format!("{what}: one positive weight per control point"));
                    }
                    if !knots.is_empty()
                        && (knots.len() != n + p + 1 || knots.windows(2).any(|w| w[1] < w[0]))
                    {
                        return Err(format!("{what}: {} non-decreasing knots", n + p + 1));
                    }
                }
                EntityKind::FittedSpline { points, .. } if points.len() < 2 => {
                    return Err(format!("{what}: a spline needs two fit points"));
                }
                EntityKind::FittedSpline {
                    points,
                    closed: true,
                } if points.len() < 3 => {
                    return Err(format!("{what}: a closed spline needs three fit points"));
                }
                _ => {}
            }
            if let EntityKind::Line {
                centerline: true, ..
            } = e.kind
                && e.construction
            {
                return Err(format!(
                    "{what}: a center line is not construction geometry"
                ));
            }
        }
        let mut constraint_ids = BTreeSet::new();
        for c in &self.constraints {
            if c.id.0 == 0 || !constraint_ids.insert(c.id) {
                return Err(format!(
                    "constraint {} is used more than once or is k0",
                    c.id
                ));
            }
            let what = c.id.to_string();
            use ConstraintKind as K;
            let p = Ref::Point;
            let cv = Ref::Curve;
            match &c.kind {
                K::Coincident { point, entity } => {
                    check_kinds(&index, &what, &[(p(*point), ANY), (*entity, ANY)])?;
                    if *entity == p(*point) {
                        return Err(format!("{what}: a point is coincident with itself"));
                    }
                }
                K::Horizontal { line } | K::Vertical { line } => {
                    check_kinds(&index, &what, &[(cv(*line), LINE)])?;
                }
                K::HorizontalPoints { a, b } | K::VerticalPoints { a, b } => {
                    check_kinds(&index, &what, &[(p(*a), ANY), (p(*b), ANY)])?;
                }
                K::Parallel { a, b } | K::Perpendicular { a, b } | K::Collinear { a, b } => {
                    check_kinds(&index, &what, &[(cv(*a), LINE), (cv(*b), ANY)])?;
                }
                K::Equal { a, b } => {
                    check_kinds(&index, &what, &[(cv(*a), ANY), (cv(*b), ANY)])?;
                }
                K::Tangent { a, b, at } | K::Smooth { a, b, at } => {
                    check_kinds(&index, &what, &[(cv(*a), ANY), (cv(*b), ANY)])?;
                    if let Some(q) = at {
                        check_kinds(&index, &what, &[(p(*q), ANY)])?;
                    }
                    // A spline smooth where it closes (mitcad#783).
                    let ends = index.get(*a).and_then(|e| e.kind.ends());
                    if a == b && (at.is_none() || ends != at.map(|q| (q, q))) {
                        return Err(format!(
                            "{what}: a curve is tangent to itself only at the point where both \
                             ends of a spline are"
                        ));
                    }
                }
                K::Concentric { a, b } => {
                    let round = &["circle", "arc", "ellipse", "elliptical_arc"][..];
                    check_kinds(&index, &what, &[(cv(*a), round), (cv(*b), round)])?;
                }
                K::Midpoint { point, curve } => {
                    check_kinds(
                        &index,
                        &what,
                        &[(p(*point), ANY), (cv(*curve), &["line", "arc"])],
                    )?;
                }
                K::Symmetric { a, b, axis, .. } => {
                    check_kinds(&index, &what, &[(*a, ANY), (*b, ANY), (cv(*axis), LINE)])?;
                    if a.is_point() != b.is_point() {
                        return Err(format!("{what}: two points or two curves"));
                    }
                }
                K::EqualSpacing {
                    anchors,
                    mode,
                    direction,
                } => {
                    crate::spacing::check(&index, &what, anchors, *mode, *direction)?;
                }
            }
            if c.kind.refs().windows(2).any(|w| w[0] == w[1]) {
                return Err(format!("{what}: an entity is constrained to itself"));
            }
        }
        let mut spaced_once = BTreeSet::new();
        for d in &self.dimensions {
            if d.id.0 == 0 || !constraint_ids.insert(d.id) {
                return Err(format!(
                    "dimension {} is used more than once or is k0",
                    d.id
                ));
            }
            let what = d.id.to_string();
            use DimensionKind as D;
            let p = Ref::Point;
            let cv = Ref::Curve;
            let refs: Vec<(Ref, &[&str])> = match &d.kind {
                D::Distance { a, b }
                | D::HorizontalDistance { a, b, .. }
                | D::VerticalDistance { a, b, .. } => {
                    if a == b {
                        return Err(format!("{what}: two different points"));
                    }
                    vec![(p(*a), ANY), (p(*b), ANY)]
                }
                D::PointLineDistance { point, line } => vec![(p(*point), ANY), (cv(*line), LINE)],
                D::LineDistance { a, b } | D::Angle { a, b, .. } => {
                    if a == b {
                        return Err(format!("{what}: two different lines"));
                    }
                    vec![(cv(*a), LINE), (cv(*b), LINE)]
                }
                D::TangentDistance { circle, line } => {
                    vec![(cv(*circle), ROUND), (cv(*line), LINE)]
                }
                D::ConcentricDistance { a, b } => {
                    if a == b {
                        return Err(format!("{what}: two different circles or arcs"));
                    }
                    vec![(cv(*a), ROUND), (cv(*b), ROUND)]
                }
                D::Length { line } => vec![(cv(*line), LINE)],
                D::Radius { curve } | D::Diameter { curve } => vec![(cv(*curve), ROUND)],
                D::ArcLength { arc } => vec![(cv(*arc), &["arc"][..])],
                D::MajorRadius { ellipse } | D::MinorRadius { ellipse } => {
                    vec![(cv(*ellipse), &["ellipse", "elliptical_arc"][..])]
                }
                D::LinearDiameter { axis, entity } => {
                    let other: &[&str] = if entity.is_point() { ANY } else { LINE };
                    vec![(cv(*axis), LINE), (*entity, other)]
                }
                // The common spacing of an equal spacing, one per constraint.
                D::Spacing { constraint } => {
                    let spaced = self.constraints.iter().any(|c| {
                        c.id == *constraint && matches!(c.kind, ConstraintKind::EqualSpacing { .. })
                    });
                    if !spaced {
                        return Err(format!("{what}: {constraint} is not an equal spacing"));
                    }
                    if !spaced_once.insert(*constraint) {
                        return Err(format!(
                            "{what}: {constraint} has another spacing dimension already"
                        ));
                    }
                    Vec::new()
                }
                D::TangentCircleDistance { a, b, .. } => {
                    if a == b {
                        return Err(format!("{what}: two different circles or arcs"));
                    }
                    vec![(cv(*a), ROUND), (cv(*b), ROUND)]
                }
                D::TangentPointDistance { circle, point, .. } => {
                    vec![(cv(*circle), ROUND), (p(*point), ANY)]
                }
            };
            check_kinds(&index, &what, &refs)?;
            if let Some(text) = d.text
                && !text.iter().all(|v| v.is_finite())
            {
                return Err(format!("{what}: the text position must be finite"));
            }
        }
        for (i, projection) in self.projections.iter().enumerate() {
            if projection.entities.is_empty() {
                return Err(format!("projections[{i}]: no entities"));
            }
            for r in &projection.entities {
                check_kinds(&index, &format!("projections[{i}]"), &[(*r, ANY)])?;
            }
        }
        for p in &self.patterns {
            if p.id.0 == 0 || !constraint_ids.insert(p.id) {
                return Err(format!("pattern {} is used more than once or is k0", p.id));
            }
            p.check(&index)?;
        }
        for o in &self.offsets {
            if o.id.0 == 0 || !constraint_ids.insert(o.id) {
                return Err(format!("offset {} is used more than once or is k0", o.id));
            }
            o.check(&index)?;
        }
        self.check_derived(&derived_entities(&self.offsets, &index))?;
        if let Some(frame) = &self.frame {
            frame.check()?;
        }
        Ok(())
    }
}

impl SketchDef {
    /// Checks the dimension, pattern and offset values against their
    /// ranges (the feature's `FeatureInfo::check_values`).
    pub fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        // An inactive dimension's value is checked when it is activated
        // (mitcad#721).
        for d in self.dimensions.iter().filter(|d| self.dimension_applied(d)) {
            if let Some(id) = d.value {
                check_range(d.kind.range(), value(id))
                    .map_err(|e| format!("{} {e}", d.kind.type_name()))?;
            }
        }
        for p in &self.patterns {
            for (slot, id) in p.slots().iter().zip(p.values()) {
                pattern::check_value(slot, value(*id))
                    .map_err(|e| format!("{} {e}", p.type_name()))?;
            }
        }
        for o in &self.offsets {
            check_range(ValueRange::Positive, value(o.distance))
                .map_err(|e| format!("offset distance {e}"))?;
        }
        // The placement on the plane (mitcad#787): any finite values.
        if let Some(placement) = &self.placement {
            placement.resolve(&mut |id| Ok::<f64, String>(value(id)))?;
        }
        Ok(())
    }
}

/// The curves of solved geometry by id, for callers outside the module.
pub fn curve_of(solved: &Solved, id: EntityUid) -> Option<&Curve2> {
    solved.curves.get(&id)
}

#[cfg(test)]
mod upright_tests {
    use super::{FrameDef, SketchFrame};
    use crate::datum::DatumPlane;

    const X: [f64; 3] = [1.0, 0.0, 0.0];
    const Y: [f64; 3] = [0.0, 1.0, 0.0];
    const Z: [f64; 3] = [0.0, 0.0, 1.0];

    fn near(a: [f64; 3], b: [f64; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-12)
    }

    /// The upright frame of a plane in the design (its axes, x and y), and
    /// whether it is stored (differs from the plane's own).
    fn upright(plane: &SketchFrame) -> ([f64; 3], [f64; 3], bool) {
        match FrameDef::upright(plane, Z, Y, None) {
            Some(own) => {
                own.check().unwrap();
                let placed = own.apply(plane);
                assert!(near(placed.origin, plane.origin), "the origin stays");
                assert!(near(placed.normal(), plane.normal()), "the normal stays");
                (placed.x_axis, placed.y_axis, true)
            }
            None => (plane.x_axis, plane.y_axis, false),
        }
    }

    /// A plane, its upright x and y in the design, and whether the own
    /// frame is stored.
    type Case<'a> = (&'a str, SketchFrame, [f64; 3], [f64; 3], bool);

    /// mitcad#692: the design's +Z up on every plane that is not
    /// horizontal, +Y on a horizontal one; the normal kept, x = up ×
    /// normal.
    #[test]
    fn upright_frames_put_the_design_z_up() {
        let face = |normal: [f64; 3]| DatumPlane::on_plane([5.0, 7.0, 3.0], normal).frame();
        let s = 0.6;
        let c = 0.8;
        let cases: [Case<'_>; 7] = [
            ("+Z face", face(Z), X, Y, false),
            ("-Z face", face([0.0, 0.0, -1.0]), [-1.0, 0.0, 0.0], Y, true),
            ("+X face", face(X), Y, Z, false),
            ("-X face", face([-1.0, 0.0, 0.0]), [0.0, -1.0, 0.0], Z, true),
            ("+Y face", face(Y), [-1.0, 0.0, 0.0], Z, true),
            ("-Y face", face([0.0, -1.0, 0.0]), X, Z, false),
            // Slanted, facing +Y and up: its own y points down the slope.
            (
                "slanted face",
                face([0.0, s, c]),
                [-1.0, 0.0, 0.0],
                [0.0, -c, s],
                true,
            ),
        ];
        for (name, plane, x, y, stored) in cases {
            let (got_x, got_y, got_stored) = upright(&plane);
            assert!(near(got_x, x), "{name}: x {got_x:?}, expected {x:?}");
            assert!(near(got_y, y), "{name}: y {got_y:?}, expected {y:?}");
            assert_eq!(got_stored, stored, "{name}: stored");
        }
        // The design's axes in other coordinates (a sketch of a component
        // shown through a turned occurrence): there they are the given ones.
        let plane = face(Z);
        let own = FrameDef::upright(&plane, [0.0, -1.0, 0.0], [0.0, 0.0, 1.0], None).unwrap();
        let placed = own.apply(&plane);
        assert!(near(placed.y_axis, [0.0, -1.0, 0.0]), "{:?}", placed.y_axis);
        assert!(near(placed.x_axis, [-1.0, 0.0, 0.0]), "{:?}", placed.x_axis);
    }

    /// mitcad#1060: a plane seen from a given side. The XZ origin plane
    /// (normal +Y) seen from -Y: x = +X, y = +Z, the normal reversed to -Y,
    /// the frame right-handed (not mirrored). A plane whose normal already
    /// points to that side, or lies across it, keeps its normal.
    #[test]
    fn a_plane_seen_from_a_side_turns_its_normal_there() {
        let minus_y = [0.0, -1.0, 0.0];
        let xz = crate::datum::OriginDatum::Xz.datum();
        let crate::datum::Datum::Plane(xz) = xz else {
            panic!("XZ is a plane")
        };
        let plane = xz.frame();
        let own = FrameDef::upright(&plane, Z, Y, Some(minus_y)).unwrap();
        own.check().unwrap();
        let placed = own.apply(&plane);
        assert!(near(placed.origin, [0.0; 3]), "{:?}", placed.origin);
        assert!(near(placed.x_axis, X), "x {:?}", placed.x_axis);
        assert!(near(placed.y_axis, Z), "y {:?}", placed.y_axis);
        assert!(
            near(placed.normal(), minus_y),
            "normal {:?}",
            placed.normal()
        );
        // Without a side: the plane's normal as before (x = -X).
        let kept = FrameDef::upright(&plane, Z, Y, None).unwrap().apply(&plane);
        assert!(near(kept.x_axis, [-1.0, 0.0, 0.0]), "x {:?}", kept.x_axis);
        assert!(near(kept.normal(), Y), "normal {:?}", kept.normal());
        // A face already facing -Y, and one facing +X, are not turned.
        let face = |normal: [f64; 3]| DatumPlane::on_plane([5.0, 7.0, 3.0], normal).frame();
        for normal in [minus_y, X] {
            let plane = face(normal);
            let placed = match FrameDef::upright(&plane, Z, Y, Some(minus_y)) {
                Some(own) => own.apply(&plane),
                None => plane,
            };
            assert!(
                near(placed.normal(), normal),
                "{normal:?}: {:?}",
                placed.normal()
            );
        }
    }
}
