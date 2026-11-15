// SPDX-License-Identifier: MIT
//! Press Pull on a face continued as a profile (mitcad#1850): planar faces
//! of one body carried Straight along their own normals (the face extruded
//! and joined) or Along their walls (revolved about the walls' axis, the
//! face turning with them, or carried along the walls' direction), the new
//! piece's walls drafted or its outline scaled over the run. A positive
//! distance adds the piece, a negative one cuts it. Offset, the face moved
//! with its neighbours following, stays [`super::OffsetFaceDef`].
//!
//! The new walls are `<feature>:side(<edge>)`, the moved end
//! `<feature>:end(<face>)`, which also keeps the face's own names
//! ([`Kernel::press_pull`]).

use serde::{Deserialize, Serialize};

use super::face_refs::{add_faces, check_faces, kernel_error, require_faces};
use super::mapping::map_params;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
};
use crate::ids::BodyUid;
use crate::kernel::{Kernel, PressPullRun, PressPullSpec, PressPullTaper};
use crate::parameters::ParamId;
use crate::topo::FaceName;

/// How the face continues.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Continuation {
    /// Along its own normal, the new walls straight from its outline.
    #[default]
    Straight,
    /// Along its walls, turning with them about their axis (or carried
    /// along their direction), so that it stays square to the run.
    Along,
}

/// How the new piece widens or narrows over the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaperType {
    #[default]
    None,
    /// Each wall leans by `taper_angle` from its edge on the face, outward
    /// positive, as an extrusion's taper.
    Draft,
    /// The outline grows or shrinks about the face's area centre to
    /// `end_scale` at the end (1.1: 110 %).
    Scale,
}

impl TaperType {
    fn is_none(&self) -> bool {
        *self == Self::None
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PressPullDef<P = ParamId> {
    pub body: BodyUid,
    pub faces: Vec<FaceName>,
    #[serde(rename = "continue", default)]
    pub run: Continuation,
    /// The length of the path of the face's area centre (an arc's along a
    /// turn), positive out of the material; not zero.
    pub distance: P,
    #[serde(default, skip_serializing_if = "TaperType::is_none")]
    pub taper_type: TaperType,
    /// Draft: between -85 and 85 degrees.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub taper_angle: Option<P>,
    /// Scale: more than zero.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub end_scale: Option<P>,
}

impl<P> PressPullDef<P> {
    pub const TYPE: &'static str = "press_pull";
    pub const BASE_NAME: &'static str = "PressPull";

    map_params! {
        PressPullDef {
            body: copy,
            faces: clone,
            run: copy,
            distance: param,
            taper_type: copy,
            taper_angle: option,
            end_scale: option,
        }
    }
}

/// Below 90 degrees by a margin, as an extrusion's tapers.
const MAX_TAPER: f64 = 85.0 * std::f64::consts::PI / 180.0 + 1e-9;

fn check_taper(angle: Option<f64>, scale: Option<f64>, taper: TaperType) -> Result<(), String> {
    match taper {
        TaperType::None => Ok(()),
        TaperType::Draft => match angle {
            None => Err("a draft taper needs its taper angle".to_owned()),
            Some(a) if !a.is_finite() || a.abs() > MAX_TAPER => {
                Err("the taper angle must be between -85 and 85 degrees".to_owned())
            }
            Some(_) => Ok(()),
        },
        TaperType::Scale => match scale {
            None => Err("a scale taper needs its end scale".to_owned()),
            Some(s) if !(s.is_finite() && s > 0.0) => Err(
                "the end scale must be more than 0 %: the face would close to nothing".to_owned(),
            ),
            Some(_) => Ok(()),
        },
    }
}

impl FeatureInfo for PressPullDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        add_faces(&mut references, &self.faces);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        check_faces(ctx, &self.faces, "faces to press pull")?;
        match self.taper_type {
            TaperType::Draft if self.taper_angle.is_none() => {
                Err("a draft taper needs its taper angle".to_owned())
            }
            TaperType::Scale if self.end_scale.is_none() => {
                Err("a scale taper needs its end scale".to_owned())
            }
            _ => Ok(()),
        }
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        if value(self.distance) == 0.0 {
            return Err("the press pull distance must not be zero".to_owned());
        }
        check_taper(
            self.taper_angle.map(value),
            self.end_scale.map(value),
            self.taper_type,
        )
    }
}

impl<K: Kernel> Evaluate<K> for PressPullDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let distance = ctx.param(self.distance)?;
        if distance == 0.0 || !distance.is_finite() {
            return Err(format!(
                "{} must not be zero",
                ctx.param_name(self.distance)
            ));
        }
        let angle = self.taper_angle.map(|id| ctx.param(id)).transpose()?;
        let scale = self.end_scale.map(|id| ctx.param(id)).transpose()?;
        check_taper(angle, scale, self.taper_type)?;
        let taper = match self.taper_type {
            TaperType::None => PressPullTaper::None,
            TaperType::Draft => PressPullTaper::Draft {
                angle: angle.unwrap_or(0.0),
            },
            TaperType::Scale => PressPullTaper::Scale {
                end: scale.unwrap_or(1.0),
            },
        };
        let spec = PressPullSpec {
            run: match self.run {
                Continuation::Straight => PressPullRun::Straight,
                Continuation::Along => PressPullRun::Along,
            },
            distance,
            taper,
        };
        let shape = ctx.surface_body(self.body)?;
        let faces = require_faces(ctx, self.body, &shape, &self.faces)?;
        let made = ctx
            .kernel
            .press_pull(ctx.uid, &shape, &faces, &spec)
            .map_err(kernel_error)?;
        Ok(FeatureOutput {
            changes: vec![BodyChange::Set(self.body, made)],
            ..FeatureOutput::default()
        })
    }
}
