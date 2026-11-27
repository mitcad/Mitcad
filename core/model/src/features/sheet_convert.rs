// SPDX-License-Identifier: MIT
//! Convert to Sheet Metal (mitcad#461): a body becomes a sheet of the
//! rules. Without bends, a plate of the rules' thickness whose picked face
//! is its front; with bends, the planar faces reached from the picked face
//! across the bend edges become panels on the sheet's front, with the
//! rules' thickness behind them and a bend at every bend edge, its virtual
//! sharp on that edge. Every other edge between two of those faces is a
//! rip (mitcad#1949): the faces are cut apart there, a corner gap between
//! them, as the walls of a flange on several edges meet at a corner.

use super::face_refs::add_faces;
use super::mapping::map_params;
use super::{
    CheckContext, EvalContext, Evaluate, FeatureDef, FeatureInfo, FeatureOutput, References,
};
use crate::features::sheet_folded::SheetMiter;
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::topo::{EdgeName, FaceName};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct SheetConvertDef<P = ParamId> {
    pub rules: FeatureUid,
    pub body: BodyUid,
    /// The planar face that becomes the first panel's front; it stays where
    /// it is.
    pub face: FaceName,
    /// Edges between two planar faces that become bends.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bends: Vec<EdgeName>,
    /// The bends' inside radius in place of the rules'.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<P>,
    /// Edges between two converted faces picked as rips (mitcad#1949).
    /// Every edge between two converted faces that is no bend is a rip
    /// anyway; a picked one must be such an edge.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rips: Vec<EdgeName>,
    /// The gap between the faces at a rip; none: the sheet's thickness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_gap: Option<P>,
    /// How the faces meet at the rips; none: `bisect`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub miter: Option<SheetMiter>,
    /// How a curved face is laid flat (mitcad#2442); none: `exact`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub development: Option<SheetDevelopment>,
    /// The triangles of a `triangulated` development; none: 64.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triangles: Option<P>,
    /// Further faces developed with `face` by triangulation, each joined
    /// to the one before it along a whole edge across the strip.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<FaceName>,
}

/// How Convert to Sheet Metal lays a curved face flat (mitcad#2442).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SheetDevelopment {
    /// Exactly, without stretching: flat, cylindrical and conical faces.
    Exact,
    /// Approximately, by triangulation between two edges of the face (a
    /// transition): the pattern says how far its lengths are from the
    /// face's.
    Triangulated,
}

/// The triangles of a triangulated development unless the feature names
/// another number, and the most it takes.
pub const DEFAULT_TRIANGLES: f64 = 64.0;
pub const MAX_TRIANGLES: f64 = 4000.0;

impl<P> SheetConvertDef<P> {
    pub const TYPE: &'static str = "sheet_convert";
    pub const BASE_NAME: &'static str = "SheetConvert";
    map_params! {
        SheetConvertDef {
            rules: copy,
            body: copy,
            face: clone,
            bends: clone,
            radius: option,
            rips: clone,
            corner_gap: option,
            miter: copy,
            development: copy,
            triangles: option,
            faces: clone,
        }
    }
}

impl FeatureInfo for SheetConvertDef {
    fn references(&self) -> References {
        let mut r = References::default();
        r.body(self.body);
        r.features.insert(self.rules);
        add_faces(&mut r, [&self.face]);
        add_faces(&mut r, &self.faces);
        for edge in self.bends.iter().chain(&self.rips) {
            r.features.extend(edge.features());
        }
        r
    }
    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        if !matches!(ctx.feature(self.rules)?.def, FeatureDef::SheetRules(_)) {
            return Err(format!("{} is not a sheet rules feature", self.rules));
        }
        for (i, edge) in self.bends.iter().enumerate() {
            if self.bends[..i].contains(edge) {
                return Err(crate::notice::placed(
                    "an edge is listed more than once",
                    crate::notice::NoticePlace::edge(edge.to_string()),
                    format!("edge {edge} is listed more than once"),
                ));
            }
        }
        for (i, edge) in self.rips.iter().enumerate() {
            if self.rips[..i].contains(edge) || self.bends.contains(edge) {
                return Err(crate::notice::placed(
                    "an edge is picked as a rip more than once, or as a bend and a rip",
                    crate::notice::NoticePlace::edge(edge.to_string()),
                    format!(
                        "edge {edge} is picked as a rip more than once, or as a bend and a rip"
                    ),
                ));
            }
        }
        for (i, face) in self.faces.iter().enumerate() {
            if self.faces[..i].contains(face) || *face == self.face {
                return Err(crate::notice::placed(
                    "a face is picked more than once",
                    crate::notice::NoticePlace::face(face.to_string()),
                    format!("face {face} is picked more than once"),
                ));
            }
        }
        for f in self.references().features {
            ctx.feature(f)?;
        }
        Ok(())
    }
    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        if let Some(r) = self.radius.map(value)
            && !(r.is_finite() && r > crate::sheet::TOLERANCE)
        {
            return Err(format!("the bend radius must be positive, got {r}"));
        }
        if let Some(g) = self.corner_gap.map(value)
            && !(g.is_finite() && g >= 0.0)
        {
            return Err(format!("the corner gap must not be negative, got {g}"));
        }
        if let Some(n) = self.triangles.map(value)
            && !(n.fract() == 0.0 && (2.0..=MAX_TRIANGLES).contains(&n))
        {
            return Err(format!(
                "the number of triangles must be a whole number from 2 to {MAX_TRIANGLES}, got {n}"
            ));
        }
        Ok(())
    }
    fn creates_bodies(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for SheetConvertDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let d = self.map_params(&mut |_, p| ctx.param(*p))?;
        crate::sheet::folded::convert(&d, ctx).map_err(|e| crate::sheet::folded::worded(ctx, &e))
    }
}
