// SPDX-License-Identifier: MIT
//! Analyses kept in the document (mitcad#41): section analyses and print
//! analyses (mitcad#620), listed in the browser's Analysis folder and
//! saved in the design file. They change no geometry: adding, editing,
//! renaming, showing, hiding and deleting one is an undo step that
//! recomputes nothing.
//!
//! An analysis refers to geometry as features do (a plane reference, a
//! bed face), so a face reference follows the model by its topological
//! name; it is resolved at the timeline marker whenever it is asked for,
//! and an analysis whose plane, body or bed face is gone says why instead.
//!
//! At most one analysis is shown: showing one (adding it, or editing it
//! shown) hides the others, so the bodies are cut by one plane.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{Document, ModelError, invalid};
use crate::analysis;
use crate::datum::{ANGULAR, DatumAxis, DatumPlane, OriginDatum, Vec3};
use crate::features::geom_ref::GeomRef;
use crate::features::{OccurrencePath, is_false};
use crate::ids::{BodyUid, ComponentUid};
use crate::joints::path_component;
use crate::kernel::Kernel;
use crate::print_analysis;
use crate::transform::Transform;

/// An analysis kept in the document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    pub name: String,
    /// Shown (its section cuts the bodies); left out of files when true.
    #[serde(default = "shown", skip_serializing_if = "is_shown")]
    pub visible: bool,
    #[serde(flatten)]
    pub def: AnalysisDef,
}

fn shown() -> bool {
    true
}

fn is_shown(visible: &bool) -> bool {
    *visible
}

/// What an analysis is, by its `type`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnalysisDef {
    /// The bodies cut at a plane.
    Section(SectionAnalysis),
    /// Where bodies will print badly and what they will take (mitcad#620).
    Print(PrintAnalysis),
}

impl AnalysisDef {
    /// The first part of default names (`Section1`, `Print1`).
    fn name_stem(&self) -> &'static str {
        match self {
            Self::Section(_) => "Section",
            Self::Print(_) => "Print",
        }
    }
}

/// A Print Analysis (mitcad#620): the overhangs, thin walls and print
/// estimate of bodies, printed in the direction the bed face gives (its
/// outward normal pointing down), or along the design's −Z on a bed at
/// each body's lowest point without one. The bed face only sets the
/// analysis' direction: no body moves. Fields left out take the defaults
/// of `print_analysis`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintAnalysis {
    pub bodies: Vec<BodyUid>,
    /// A planar face of one of the bodies, lying on the bed; follows the
    /// model by its topological name.
    #[serde(default)]
    pub bed_face: Option<GeomRef>,
    #[serde(default)]
    pub overhang: OverhangCheck,
    #[serde(default)]
    pub thin_wall: ThinWallCheck,
    #[serde(default)]
    pub estimate: EstimateCheck,
    /// Narrow gaps between the bodies (mitcad#1519).
    #[serde(default)]
    pub min_gap: MinGapCheck,
}

fn yes() -> bool {
    true
}

fn default_angle() -> f64 {
    print_analysis::DEFAULT_ANGLE
}

fn default_minimum() -> f64 {
    print_analysis::DEFAULT_MINIMUM
}

/// Overhangs: faces steeper than `angle` degrees from the vertical.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverhangCheck {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "default_angle")]
    pub angle: f64,
}

impl Default for OverhangCheck {
    fn default() -> Self {
        Self {
            enabled: true,
            angle: print_analysis::DEFAULT_ANGLE,
        }
    }
}

/// Thin walls: walls thinner than `minimum` mm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThinWallCheck {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "default_minimum")]
    pub minimum: f64,
}

impl Default for ThinWallCheck {
    fn default() -> Self {
        Self {
            enabled: true,
            minimum: print_analysis::DEFAULT_MINIMUM,
        }
    }
}

fn default_line_width() -> f64 {
    print_analysis::DEFAULT_LINE_WIDTH
}

fn default_walls() -> u32 {
    print_analysis::DEFAULT_WALLS
}

fn default_layer_height() -> f64 {
    print_analysis::DEFAULT_LAYER_HEIGHT
}

fn default_top_bottom_layers() -> u32 {
    print_analysis::DEFAULT_TOP_BOTTOM_LAYERS
}

fn default_infill() -> f64 {
    print_analysis::DEFAULT_INFILL
}

fn default_filament_diameter() -> f64 {
    print_analysis::DEFAULT_FILAMENT_DIAMETER
}

/// The print estimate: the material (a material id; null for each body's
/// own, else PLA), the slicer's settings and a price per kilogram (null:
/// no cost).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstimateCheck {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default = "default_line_width")]
    pub line_width: f64,
    #[serde(default = "default_walls")]
    pub walls: u32,
    #[serde(default = "default_layer_height")]
    pub layer_height: f64,
    #[serde(default = "default_top_bottom_layers")]
    pub top_bottom_layers: u32,
    #[serde(default = "default_infill")]
    pub infill: f64,
    #[serde(default = "default_filament_diameter")]
    pub filament_diameter: f64,
    #[serde(default)]
    pub price_per_kg: Option<f64>,
}

impl Default for EstimateCheck {
    fn default() -> Self {
        let defaults = print_analysis::EstimateSettings::default();
        Self {
            enabled: true,
            material: None,
            line_width: defaults.line_width,
            walls: defaults.walls,
            layer_height: defaults.layer_height,
            top_bottom_layers: defaults.top_bottom_layers,
            infill: defaults.infill,
            filament_diameter: defaults.filament_diameter,
            price_per_kg: defaults.price_per_kg,
        }
    }
}

impl EstimateCheck {
    /// The numbers of the estimate.
    pub fn settings(&self) -> print_analysis::EstimateSettings {
        print_analysis::EstimateSettings {
            line_width: self.line_width,
            walls: self.walls,
            layer_height: self.layer_height,
            top_bottom_layers: self.top_bottom_layers,
            infill: self.infill,
            filament_diameter: self.filament_diameter,
            price_per_kg: self.price_per_kg,
        }
    }
}

/// Narrow gaps (mitcad#1519): pairs of the bodies, as they are placed,
/// apart but closer than `gap` mm, which fuse when printed in place.
/// `gap` null: the design's printed fit parameter `print_in_place_gap`,
/// else [`print_analysis::DEFAULT_MIN_GAP`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MinGapCheck {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub gap: Option<f64>,
}

impl Default for MinGapCheck {
    fn default() -> Self {
        Self {
            enabled: true,
            gap: None,
        }
    }
}

impl PrintAnalysis {
    /// Why the settings cannot be kept, if they cannot (not the geometry).
    fn check(&self) -> Result<(), String> {
        if self.bodies.is_empty() {
            return Err("the print analysis needs a body".to_owned());
        }
        check_overhang_angle(self.overhang.angle)?;
        check_minimum_wall(self.thin_wall.minimum)?;
        if let Some(gap) = self.min_gap.gap {
            print_analysis::check_min_gap(gap)?;
        }
        self.estimate.settings().check()?;
        if let Some(material) = &self.estimate.material {
            print_material(material)?;
        }
        Ok(())
    }
}

/// Why an overhang angle (degrees from the vertical) is not one.
pub fn check_overhang_angle(angle: f64) -> Result<(), String> {
    if (0.0..=90.0).contains(&angle) {
        Ok(())
    } else {
        Err(format!(
            "the overhang angle must be from 0 to 90 degrees, got {angle}"
        ))
    }
}

/// Why a minimum wall (mm) is not one.
pub fn check_minimum_wall(minimum: f64) -> Result<(), String> {
    if minimum > 0.0 && minimum.is_finite() {
        Ok(())
    } else {
        Err(format!(
            "the minimum wall must be greater than zero, got {minimum}"
        ))
    }
}

/// A material of the library by its id (`pla`) or name (`PLA Plastic`).
pub fn print_material(material: &str) -> Result<&'static analysis::Material, String> {
    analysis::material(material)
        .or_else(|| {
            analysis::MATERIALS
                .iter()
                .find(|m| m.name.eq_ignore_ascii_case(material))
        })
        .ok_or_else(|| format!("there is no material {material}"))
}

/// The bodies shown cut at a plane: the half-space on the side the
/// section's normal points to is cut away.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionAnalysis {
    /// A plane reference: an origin plane, a construction plane, a planar
    /// face or a fixed plane.
    pub plane: GeomRef,
    /// The selected plane's occurrence, from the design root. Older
    /// analyses without one resolve a component reference only if unique.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<OccurrencePath>,
    /// How far the section is from the plane along its normal, in mm.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub offset: f64,
    /// The other side is cut away: the section's normal is the plane's
    /// reversed.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    /// How far the plane is turned about `axis` before the offset, in
    /// radians, right-handed about the axis' direction (mitcad#823).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub angle: f64,
    /// A line reference the plane turns about: a straight edge, a sketch
    /// line, a construction axis or an origin axis. Needed for an angle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<GeomRef>,
    /// The axis' occurrence, from the design root, as `occurrence` is the
    /// plane's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis_occurrence: Option<OccurrencePath>,
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

const NEEDS_AN_AXIS: &str = "an angle needs an axis to turn the plane about";

impl SectionAnalysis {
    /// Why the angle cannot be kept, if it cannot (not its geometry): not
    /// finite, or not zero without an axis to turn about. A face or an
    /// edge as the plane may have its own axis (mitcad#1769): whether it
    /// has is known only from its geometry ([`Document::section_plane`]).
    fn check_angle(&self) -> Result<(), String> {
        if !self.angle.is_finite() {
            return Err("the angle must be a finite angle".to_owned());
        }
        let may_have_axis = matches!(self.plane, GeomRef::Face { .. } | GeomRef::Edge { .. });
        if self.angle != 0.0 && self.axis.is_none() && !may_have_axis {
            return Err(NEEDS_AN_AXIS.to_owned());
        }
        Ok(())
    }
}

/// Where the turn of a section's plane about an axis (a face round about
/// it, a circular edge) starts (mitcad#1769), never from the view: the
/// normal of the first of the origin planes XY, XZ and YZ whose normal is
/// at least 45 degrees from the axis, made square to it. For an axis along
/// a model axis that is the first origin plane that contains it (XZ for
/// Z, XY for X and Y). At most one model axis is within 45 degrees of a
/// direction, so one of the three always is.
fn section_start_normal(axis: Vec3) -> Vec3 {
    let axis = mitcad_vec3::unit(axis).unwrap_or([0.0, 0.0, 1.0]);
    let candidates = [OriginDatum::Xy, OriginDatum::Xz, OriginDatum::Yz].map(|origin| {
        let crate::datum::Datum::Plane(plane) = origin.datum() else {
            unreachable!("an origin plane is a plane")
        };
        plane.normal()
    });
    let square = |n: Vec3| {
        mitcad_vec3::unit(mitcad_vec3::sub(
            n,
            mitcad_vec3::scale(axis, mitcad_vec3::dot(n, axis)),
        ))
    };
    candidates
        .iter()
        .find(|n| mitcad_vec3::dot(**n, axis).abs() <= std::f64::consts::FRAC_1_SQRT_2 + 1e-9)
        .and_then(|n| square(*n))
        // Not reached for a direction (see above); a zero axis as Z.
        .unwrap_or([0.0, 1.0, 0.0])
}

/// A section's plane in the design's coordinates: a point on it and the
/// unit normal of the side cut away.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionPlane {
    pub origin: Vec3,
    pub normal: Vec3,
}

impl Analysis {
    /// Why the analysis cannot be kept, if it cannot (not its geometry).
    pub(crate) fn check(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("the analysis needs a name".to_owned());
        }
        match &self.def {
            AnalysisDef::Section(section) => {
                if !section.offset.is_finite() {
                    return Err(format!("{}: the offset must be a finite length", self.name));
                }
                section
                    .check_angle()
                    .map_err(|e| format!("{}: {e}", self.name))?;
            }
            AnalysisDef::Print(print) => {
                print.check().map_err(|e| format!("{}: {e}", self.name))?;
            }
        }
        Ok(())
    }
}

impl<K: Kernel> Document<K> {
    /// Resolve an analysis reference in design coordinates. Modelling
    /// features still use resolve_plane in their own component coordinates.
    pub fn analysis_plane(
        &self,
        reference: &GeomRef,
        occurrence: Option<&OccurrencePath>,
    ) -> Result<DatumPlane, String> {
        let plane = self.resolve_plane(reference)?;
        let path = self.analysis_plane_occurrence(reference, occurrence)?;
        let transform = self.path_transform(&path.0);
        Ok(DatumPlane {
            origin: transform.apply_point(plane.origin),
            x_axis: transform.apply_vector(plane.x_axis),
            y_axis: transform.apply_vector(plane.y_axis),
        })
    }

    pub fn analysis_plane_occurrence(
        &self,
        reference: &GeomRef,
        occurrence: Option<&OccurrencePath>,
    ) -> Result<OccurrencePath, String> {
        self.analysis_occurrence(reference, occurrence, "section plane")
    }

    /// Resolve a section's axis reference in design coordinates
    /// (mitcad#823), through its occurrence as [`Self::analysis_plane`].
    #[doc(hidden)]
    pub fn analysis_axis(
        &self,
        reference: &GeomRef,
        occurrence: Option<&OccurrencePath>,
    ) -> Result<DatumAxis, String> {
        let axis = self.resolve_axis(reference)?;
        let path = self.analysis_axis_occurrence(reference, occurrence)?;
        let transform = self.path_transform(&path.0);
        let direction = mitcad_vec3::unit(transform.apply_vector(axis.direction))
            .ok_or_else(|| "the axis has no direction".to_owned())?;
        Ok(DatumAxis {
            origin: transform.apply_point(axis.origin),
            direction,
        })
    }

    #[doc(hidden)]
    pub fn analysis_axis_occurrence(
        &self,
        reference: &GeomRef,
        occurrence: Option<&OccurrencePath>,
    ) -> Result<OccurrencePath, String> {
        self.analysis_occurrence(reference, occurrence, "section axis")
    }

    /// The occurrence a section's plane or axis (`what`) is in: the given
    /// path when it owns the reference, else the owner's only placement,
    /// else the root for references of no component.
    fn analysis_occurrence(
        &self,
        reference: &GeomRef,
        occurrence: Option<&OccurrencePath>,
        what: &str,
    ) -> Result<OccurrencePath, String> {
        let owner = match reference {
            GeomRef::Face { body, .. } | GeomRef::Edge { body, .. } => self.body_component(*body),
            GeomRef::Datum(uid) | GeomRef::SketchCurves { sketch: uid, .. } => {
                self.feature(*uid).map(|f| f.component)
            }
            _ => None,
        };
        let path = if let Some(path) = occurrence {
            let component = path_component(self.assembly(), ComponentUid::ROOT, &path.0)?;
            if owner.is_some_and(|owner| owner != component) {
                return Err(format!("the selected occurrence does not own the {what}"));
            }
            path.0.clone()
        } else if let Some(owner) = owner {
            let paths = self.assembly().paths_to(owner);
            if paths.len() != 1 {
                return Err(format!("select an occurrence for the {what}'s component"));
            }
            paths[0].clone()
        } else {
            Vec::new()
        };
        Ok(OccurrencePath(path))
    }

    /// The analyses in the order they were added.
    pub fn analyses(&self) -> &[Analysis] {
        &self.state.analyses
    }

    /// The plane a section's plane reference places, before its Angle,
    /// Distance and Flip, in design coordinates, and the axis it turns
    /// about without an Axis of its own (mitcad#1769): a plane as it is; a
    /// face round about an axis or a circular edge, the plane that contains
    /// that axis, its turn about it starting at `section_start_normal`,
    /// with that axis; a straight edge, the plane through its middle normal
    /// to it. Through the reference's occurrence as [`Self::analysis_plane`].
    pub fn section_pick(
        &self,
        reference: &GeomRef,
        occurrence: Option<&OccurrencePath>,
    ) -> Result<(DatumPlane, Option<DatumAxis>), String> {
        use crate::features::geom_ref::{SectionPick, resolve_section_pick};
        let (plane, own_axis) = match resolve_section_pick(&mut self.resolver(), reference)? {
            SectionPick::Plane(plane) => (plane, None),
            SectionPick::Axis(axis) => {
                let normal = section_start_normal(axis.direction);
                let plane = DatumPlane {
                    origin: axis.origin,
                    x_axis: axis.direction,
                    y_axis: mitcad_vec3::cross(normal, axis.direction),
                };
                (plane, Some(axis))
            }
            SectionPick::Across { point, normal } => {
                (DatumPlane::from_normal(point, normal, None), None)
            }
        };
        let path = self.analysis_plane_occurrence(reference, occurrence)?;
        let transform = self.path_transform(&path.0);
        let plane = DatumPlane {
            origin: transform.apply_point(plane.origin),
            x_axis: transform.apply_vector(plane.x_axis),
            y_axis: transform.apply_vector(plane.y_axis),
        };
        let own_axis = own_axis
            .map(|axis| {
                mitcad_vec3::unit(transform.apply_vector(axis.direction))
                    .map(|direction| DatumAxis {
                        origin: transform.apply_point(axis.origin),
                        direction,
                    })
                    .ok_or_else(|| "the axis has no direction".to_owned())
            })
            .transpose()?;
        Ok((plane, own_axis))
    }

    /// The axis a section's plane turns about by its Angle (mitcad#823,
    /// #1769), in design coordinates: its Axis, else the axis of a face
    /// round about one or of a circular edge picked as its plane; None
    /// without either; or why it cannot be found.
    pub fn section_turn_axis(
        &self,
        section: &SectionAnalysis,
    ) -> Result<Option<DatumAxis>, String> {
        match self.section_axis(section)? {
            Some(axis) => Ok(Some(axis)),
            None => Ok(self
                .section_pick(&section.plane, section.occurrence.as_ref())?
                .1),
        }
    }

    /// A section's plane at the timeline marker, or why it cannot be found.
    pub fn section_plane(&self, section: &SectionAnalysis) -> Result<SectionPlane, String> {
        if !section.offset.is_finite() {
            return Err("the section offset must be finite".to_owned());
        }
        section.check_angle()?;
        let (plane, own_axis) = self.section_pick(&section.plane, section.occurrence.as_ref())?;
        let normal = plane.normal();
        let axis = match self.section_axis(section)? {
            Some(axis) => {
                if mitcad_vec3::norm(mitcad_vec3::cross(axis.direction, normal)) < ANGULAR {
                    return Err("the axis is along the plane's normal".to_owned());
                }
                Some(axis)
            }
            // A face round about an axis or a circular edge turns about its
            // own axis (mitcad#1769).
            None => own_axis,
        };
        let (base, n) = match axis {
            // Turned rigidly about the axis, then moved along the turned
            // normal (mitcad#823).
            Some(axis) => {
                let turn = Transform::rotation(axis.origin, axis.direction, section.angle)
                    .ok_or_else(|| "the axis has no direction".to_owned())?;
                (turn.apply_point(plane.origin), turn.apply_vector(normal))
            }
            None if section.angle != 0.0 => {
                return Err(NEEDS_AN_AXIS.to_owned());
            }
            None => (plane.origin, normal),
        };
        let origin = [0, 1, 2].map(|i| base[i] + n[i] * section.offset);
        // No -0 in the normal.
        let normal = n.map(|c| {
            if c == 0.0 {
                0.0
            } else if section.flip {
                -c
            } else {
                c
            }
        });
        Ok(SectionPlane { origin, normal })
    }

    /// A section's axis in design coordinates at the marker, None without
    /// one, or why it cannot be found.
    pub fn section_axis(&self, section: &SectionAnalysis) -> Result<Option<DatumAxis>, String> {
        section
            .axis
            .as_ref()
            .map(|axis| {
                self.analysis_axis(axis, section.axis_occurrence.as_ref())
                    .map_err(|e| format!("the axis: {e}"))
            })
            .transpose()
    }

    /// The `analyses` query: each analysis in its file form, with a
    /// section's plane at the marker (`section`) or why it is not there
    /// (`error`).
    pub fn analyses_json(&self) -> Value {
        Value::Array(
            self.state
                .analyses
                .iter()
                .map(|analysis| {
                    let mut value = serde_json::to_value(analysis).expect("analyses serialize");
                    value["visible"] = json!(analysis.visible);
                    match &analysis.def {
                        AnalysisDef::Section(section) => match self.section_plane(section) {
                            Ok(plane) => {
                                value["section"] =
                                    json!({"origin": plane.origin, "normal": plane.normal});
                            }
                            Err(error) => value["error"] = json!(error),
                        },
                        // The print direction at the marker, or why a body
                        // or the bed face is not there.
                        AnalysisDef::Print(print) => match self.print_direction(print) {
                            Ok(down) => value["down"] = json!(down),
                            Err(error) => value["error"] = json!(error),
                        },
                    }
                    value
                })
                .collect(),
        )
    }

    /// Why the definition cannot be kept now: its plane is no plane at the
    /// marker.
    fn check_analysis_def(&self, def: &AnalysisDef) -> Result<(), ModelError> {
        match def {
            AnalysisDef::Section(section) => {
                self.section_plane(section)
                    .map_err(|e| invalid(format!("the section's plane: {e}")))?;
            }
            AnalysisDef::Print(print) => {
                self.print_direction(print)
                    .map_err(|e| invalid(format!("the print analysis: {e}")))?;
            }
        }
        Ok(())
    }

    /// Where each body is printed from: its first shown placement in the
    /// design, else its first, else its own coordinates (mitcad#620).
    pub fn print_placements(&self, bodies: &[BodyUid]) -> Vec<Transform> {
        let instances = self.instances();
        bodies
            .iter()
            .map(|body| {
                let mut placed = instances.iter().filter(|i| i.body == *body);
                let first = placed.clone().next();
                placed
                    .find(|i| i.visible)
                    .or(first)
                    .map_or(Transform::IDENTITY, |i| i.transform)
            })
            .collect()
    }

    /// The gap (mm) under which bodies printed in place fuse, when none is
    /// given (mitcad#1519), and where it comes from: the design's user
    /// parameter `print_in_place_gap` when it is a length greater than
    /// zero (`"parameter"`), else [`print_analysis::DEFAULT_MIN_GAP`]
    /// (`"default"`).
    pub fn print_min_gap_default(&self) -> (f64, &'static str) {
        let params = self.parameters();
        params
            .find(crate::print_fits::PRINT_IN_PLACE_GAP)
            .and_then(|id| {
                let parameter = params.get(id)?;
                let value = params.value(id)?;
                (parameter.unit().dims() == crate::expr::Dims::LENGTH
                    && value > 0.0
                    && value.is_finite())
                .then_some(value)
            })
            .map_or((print_analysis::DEFAULT_MIN_GAP, "default"), |value| {
                (value, "parameter")
            })
    }

    /// The print direction (unit, towards the bed) and a point of the bed
    /// that a bed face gives, in design coordinates: the face's outward
    /// normal and its plane, where its body is printed from.
    pub fn print_bed(&self, bed_face: &GeomRef) -> Result<(Vec3, Vec3), String> {
        let GeomRef::Face { body, .. } = bed_face else {
            return Err("the bed face must be a planar face of a body".to_owned());
        };
        let plane = self
            .resolve_plane(bed_face)
            .map_err(|e| format!("the bed face: {e}"))?;
        let placement = self.print_placements(&[*body])[0];
        let down = mitcad_vec3::unit(placement.apply_vector(plane.normal()))
            .ok_or_else(|| "the bed face has no normal".to_owned())?;
        Ok((down, placement.apply_point(plane.origin)))
    }

    /// A print analysis' direction at the marker, or why its bodies or bed
    /// face are not there.
    pub fn print_direction(&self, print: &PrintAnalysis) -> Result<Vec3, String> {
        for body in &print.bodies {
            if self.body_shape(*body).is_none() {
                return Err(format!("body {body} does not exist at the timeline marker"));
            }
        }
        match &print.bed_face {
            None => Ok([0.0, 0.0, -1.0]),
            Some(face) => {
                if let GeomRef::Face { body, .. } = face
                    && !print.bodies.contains(body)
                {
                    return Err(format!(
                        "the bed face is on body {body}, which the analysis does not include"
                    ));
                }
                self.print_bed(face).map(|(down, _)| down)
            }
        }
    }

    /// Adds an analysis, shown, and hides the others. Without a name it is
    /// `Section1`, `Section2`, ... (the next free number). Returns the name.
    pub fn add_analysis(
        &mut self,
        def: AnalysisDef,
        name: Option<&str>,
    ) -> Result<String, ModelError> {
        let name = match name.map(str::trim).filter(|n| !n.is_empty()) {
            Some(name) => name.to_owned(),
            None => {
                let stem = def.name_stem();
                (1..)
                    .map(|n| format!("{stem}{n}"))
                    .find(|name| self.analysis_index(name).is_err())
                    .expect("a free name")
            }
        };
        if self.analysis_index(&name).is_ok() {
            return Err(invalid(format!("an analysis {name} exists")));
        }
        let analysis = Analysis {
            name: name.clone(),
            visible: true,
            def,
        };
        analysis.check().map_err(invalid)?;
        self.check_analysis_def(&analysis.def)?;
        let label = format!("Add {name}");
        self.apply_with(|state| {
            for other in &mut state.analyses {
                other.visible = false;
            }
            state.analyses.push(analysis);
            Ok((label, (), false))
        })?;
        Ok(name)
    }

    /// Changes an analysis' definition; `visible` shows (hiding the others)
    /// or hides it too. A change that changes nothing is no undo step.
    pub fn edit_analysis(
        &mut self,
        name: &str,
        def: AnalysisDef,
        visible: Option<bool>,
    ) -> Result<(), ModelError> {
        let i = self.analysis_index(name)?;
        let old = &self.state.analyses[i];
        let visible = visible.unwrap_or(old.visible);
        let unchanged = old.def == def
            && old.visible == visible
            && (!visible || self.state.analyses.iter().filter(|a| a.visible).count() <= 1);
        if unchanged {
            return Ok(());
        }
        let analysis = Analysis {
            name: name.to_owned(),
            visible,
            def,
        };
        analysis.check().map_err(invalid)?;
        self.check_analysis_def(&analysis.def)?;
        let label = format!("Edit {name}");
        self.apply_with(|state| {
            show_only(state, i, visible);
            state.analyses[i] = analysis;
            Ok((label, (), false))
        })
    }

    /// Shows (hiding the others) or hides an analysis; an undo step
    /// `Show Section1` / `Hide Section1` unless nothing changes.
    pub fn set_analysis_visible(&mut self, name: &str, visible: bool) -> Result<(), ModelError> {
        let i = self.analysis_index(name)?;
        let others_shown = self
            .state
            .analyses
            .iter()
            .enumerate()
            .any(|(j, a)| j != i && a.visible);
        if self.state.analyses[i].visible == visible && !(visible && others_shown) {
            return Ok(());
        }
        let label = format!("{} {name}", if visible { "Show" } else { "Hide" });
        self.apply_with(|state| {
            show_only(state, i, visible);
            state.analyses[i].visible = visible;
            Ok((label, (), false))
        })
    }

    pub fn rename_analysis(&mut self, name: &str, new_name: &str) -> Result<(), ModelError> {
        let i = self.analysis_index(name)?;
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(invalid("the analysis needs a name"));
        }
        if new_name == name {
            return Ok(());
        }
        if self.analysis_index(new_name).is_ok() {
            return Err(invalid(format!("an analysis {new_name} exists")));
        }
        let label = format!("Rename {name}");
        let new_name = new_name.to_owned();
        self.apply_with(|state| {
            state.analyses[i].name = new_name;
            Ok((label, (), false))
        })
    }

    pub fn delete_analysis(&mut self, name: &str) -> Result<(), ModelError> {
        let i = self.analysis_index(name)?;
        let label = format!("Delete {name}");
        self.apply_with(|state| {
            state.analyses.remove(i);
            Ok((label, (), false))
        })
    }

    fn analysis_index(&self, name: &str) -> Result<usize, ModelError> {
        self.state
            .analyses
            .iter()
            .position(|a| a.name == name)
            .ok_or_else(|| invalid(format!("there is no analysis {name}")))
    }
}

/// When analysis `i` is to be shown, the others are hidden.
fn show_only(state: &mut super::DocState, i: usize, visible: bool) {
    if visible {
        for (j, other) in state.analyses.iter_mut().enumerate() {
            if j != i {
                other.visible = false;
            }
        }
    }
}
