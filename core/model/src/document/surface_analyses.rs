// SPDX-License-Identifier: MIT
//! Surface quality analyses kept in the document (mitcad#2517): Zebra,
//! Curvature Map, Isocurve Analysis and Environment Map, each on bodies
//! (all their faces) or on faces of bodies, listed in the browser's
//! Analysis folder and saved in the design file as the other analyses
//! (`analyses.rs`). The values are worked out by the `surface_analysis`
//! query (`crate::surface_analysis`); here are the definitions, their
//! checks and what they refer to at the timeline marker.

use serde::{Deserialize, Serialize};

use super::Document;
use crate::datum::Vec3;
use crate::features::geom_ref::GeomRef;
use crate::features::{OccurrencePath, is_false};
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::surface_analysis::CurvatureKind;
use crate::topo::FaceName;

/// A Zebra's stripes over the half turn when none are given.
pub const DEFAULT_ZEBRA_STRIPES: u32 = 12;
/// The most stripes a Zebra takes.
pub const MAX_ZEBRA_STRIPES: u32 = 200;
/// A Zebra's dark share of each stripe when none is given.
pub const DEFAULT_ZEBRA_WIDTH: f64 = 0.5;
/// An Isocurve Analysis' curves of each parameter per face when none are
/// given.
pub const DEFAULT_ISOCURVES: u32 = 10;
/// The most curves of each parameter per face.
pub const MAX_ISOCURVES: u32 = 100;

fn default_stripes() -> u32 {
    DEFAULT_ZEBRA_STRIPES
}

fn default_width() -> f64 {
    DEFAULT_ZEBRA_WIDTH
}

fn default_isocurves() -> u32 {
    DEFAULT_ISOCURVES
}

/// Zebra (mitcad#2517): stripes from a direction over the faces, to see
/// how they join (`crate::surface_analysis::zebra`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZebraAnalysis {
    /// Bodies analysed whole.
    #[serde(default)]
    pub bodies: Vec<BodyUid>,
    /// Faces of other bodies, each analysed on its own; they follow the
    /// model by their topological names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<GeomRef>,
    /// The direction the stripes come from: a plane's normal or a line's
    /// direction, as Draft Analysis' pull; none: the design's +Z.
    #[serde(default)]
    pub direction: Option<GeomRef>,
    /// The direction reference's occurrence, from the design root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<OccurrencePath>,
    /// How many stripes over the half turn from the direction to its
    /// reverse (their density), 2 to [`MAX_ZEBRA_STRIPES`].
    #[serde(default = "default_stripes")]
    pub stripes: u32,
    /// The dark share of each stripe (their width), more than 0 and less
    /// than 1.
    #[serde(default = "default_width")]
    pub width: f64,
}

/// Curvature Map (mitcad#2517): the faces colored by their curvature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurvatureAnalysis {
    #[serde(default)]
    pub bodies: Vec<BodyUid>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<GeomRef>,
    /// Gaussian, mean, largest or smallest principal curvature.
    #[serde(default)]
    pub kind: CurvatureKind,
    /// The colors reach their full strength at this value either way (in
    /// the kind's unit); none: at the largest magnitude found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
}

/// Isocurve Analysis (mitcad#2517): the faces' u and v iso curves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IsocurveAnalysis {
    #[serde(default)]
    pub bodies: Vec<BodyUid>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<GeomRef>,
    /// Curves of each parameter per face (their density), 1 to
    /// [`MAX_ISOCURVES`].
    #[serde(default = "default_isocurves")]
    pub count: u32,
    /// Each curve colored by the surface's normal curvature along it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub curvature: bool,
    /// With `curvature`: the colors' full strength at this curvature
    /// either way (1/mm); none: at the largest magnitude found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
}

/// Environment Map (mitcad#2517): a picture of surroundings reflected in
/// the faces as the view turns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentAnalysis {
    #[serde(default)]
    pub bodies: Vec<BodyUid>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<GeomRef>,
    /// The picture's file; none: the application's own studio picture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

/// Why the bodies and faces of a surface analysis cannot be kept, if they
/// cannot: none at all, or a reference that is no face.
pub(crate) fn check_targets(
    what: &str,
    bodies: &[BodyUid],
    faces: &[GeomRef],
) -> Result<(), String> {
    if bodies.is_empty() && faces.is_empty() {
        return Err(format!("the {what} needs a body or a face"));
    }
    if faces.iter().any(|f| !matches!(f, GeomRef::Face { .. })) {
        return Err(format!("the {what} takes faces of bodies only"));
    }
    Ok(())
}

/// Why a limit of a color scale is not one: more than zero and finite.
pub fn check_limit(limit: Option<f64>) -> Result<(), String> {
    match limit {
        Some(limit) if !(limit > 0.0 && limit.is_finite()) => Err(format!(
            "the color limit must be greater than zero, got {limit}"
        )),
        _ => Ok(()),
    }
}

/// Why a Zebra's stripes or width are not ones.
pub fn check_zebra_settings(stripes: u32, width: f64) -> Result<(), String> {
    if !(2..=MAX_ZEBRA_STRIPES).contains(&stripes) {
        return Err(format!(
            "the stripes must be from 2 to {MAX_ZEBRA_STRIPES}, got {stripes}"
        ));
    }
    if !(width > 0.0 && width < 1.0) {
        return Err(format!(
            "the stripe width must be more than 0 and less than 1, got {width}"
        ));
    }
    Ok(())
}

/// Why an Isocurve Analysis' count is not one.
pub fn check_isocurve_count(count: u32) -> Result<(), String> {
    if (1..=MAX_ISOCURVES).contains(&count) {
        Ok(())
    } else {
        Err(format!(
            "the iso curves per face must be from 1 to {MAX_ISOCURVES}, got {count}"
        ))
    }
}

impl ZebraAnalysis {
    pub(crate) fn check(&self) -> Result<(), String> {
        check_targets("zebra analysis", &self.bodies, &self.faces)?;
        check_zebra_settings(self.stripes, self.width)
    }
}

impl CurvatureAnalysis {
    pub(crate) fn check(&self) -> Result<(), String> {
        check_targets("curvature map", &self.bodies, &self.faces)?;
        check_limit(self.limit)
    }
}

impl IsocurveAnalysis {
    pub(crate) fn check(&self) -> Result<(), String> {
        check_targets("isocurve analysis", &self.bodies, &self.faces)?;
        check_isocurve_count(self.count)?;
        check_limit(self.limit)
    }
}

impl EnvironmentAnalysis {
    pub(crate) fn check(&self) -> Result<(), String> {
        check_targets("environment map", &self.bodies, &self.faces)?;
        if self.image.as_deref().is_some_and(|i| i.trim().is_empty()) {
            return Err("the environment map's picture has no file name".to_owned());
        }
        Ok(())
    }
}

impl super::AnalysisDef {
    /// A surface analysis' bodies and faces as it gives them; None for the
    /// other kinds.
    pub fn surface_inputs(&self) -> Option<(&[BodyUid], &[GeomRef])> {
        match self {
            Self::Zebra(a) => Some((&a.bodies, &a.faces)),
            Self::Curvature(a) => Some((&a.bodies, &a.faces)),
            Self::Isocurve(a) => Some((&a.bodies, &a.faces)),
            Self::Environment(a) => Some((&a.bodies, &a.faces)),
            _ => None,
        }
    }
}

/// A body of a surface analysis at the marker and its faces analysed: none
/// for the whole body.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceTarget {
    pub body: BodyUid,
    pub faces: Vec<FaceName>,
}

impl<K: Kernel> Document<K> {
    /// The bodies of a surface analysis (mitcad#2517) at the marker, each
    /// once in the order given, the whole bodies first, then those of the
    /// faces with their faces (a body analysed whole takes in its faces);
    /// or why a body is not there.
    pub fn surface_targets(
        &self,
        bodies: &[BodyUid],
        faces: &[GeomRef],
    ) -> Result<Vec<SurfaceTarget>, String> {
        let mut targets: Vec<SurfaceTarget> = Vec::new();
        for body in bodies {
            if !targets.iter().any(|t| t.body == *body) {
                targets.push(SurfaceTarget {
                    body: *body,
                    faces: Vec::new(),
                });
            }
        }
        let whole = targets.len();
        for face in faces {
            let GeomRef::Face { body, face } = face else {
                return Err("a surface analysis takes faces of bodies only".to_owned());
            };
            if targets[..whole].iter().any(|t| t.body == *body) {
                continue;
            }
            match targets.iter_mut().find(|t| t.body == *body) {
                Some(target) => {
                    if !target.faces.contains(face) {
                        target.faces.push(face.clone());
                    }
                }
                None => targets.push(SurfaceTarget {
                    body: *body,
                    faces: vec![face.clone()],
                }),
            }
        }
        for target in &targets {
            if self.body_shape(target.body).is_none() {
                return Err(format!(
                    "body {} does not exist at the timeline marker",
                    target.body
                ));
            }
        }
        Ok(targets)
    }

    /// A Zebra's direction (unit, design coordinates) at the marker, or
    /// why its bodies or its direction reference are not there.
    pub fn zebra_direction(&self, zebra: &ZebraAnalysis) -> Result<Vec3, String> {
        self.surface_targets(&zebra.bodies, &zebra.faces)?;
        match &zebra.direction {
            None => Ok([0.0, 0.0, 1.0]),
            Some(reference) => self
                .pull_direction(reference, zebra.occurrence.as_ref())
                .map_err(|e| format!("the stripe direction: {e}")),
        }
    }
}
