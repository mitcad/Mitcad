// SPDX-License-Identifier: MIT
//! Fillet: rounds edges of one body in edge sets. Each set has its edges
//! (named edges, and every edge of named faces), a size, the continuity
//! and the tangent chain option. A body may have any number
//! of fillets; each names its faces, so later features can refer to the
//! edges a fillet made.
//!
//! The geometry kernel follows tangent chains (not past a vertex where the
//! edges turn, unless the next edge is rounded too). What it cannot build
//! (asymmetric and curvature continuous fillets that meet other rounded
//! edges at a corner, for instance) fails with an "unsupported:" message
//! (see [`super::face_refs`]).
//!
//! In files and commands a single plain set is written with the fields
//! `edges` and `radius` next to `body`; the general form lists `sets`.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::face_refs::{add_faces, check_faces, kernel_error, require_faces};
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::{FilletSet, FilletSize, Kernel};
use crate::parameters::ParamId;
use crate::topo::{EdgeName, FaceName, VertexName};

/// An edge set's continuity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Continuity {
    /// G1, circular cross-sections.
    #[default]
    Tangent,
    /// G2: cross-sections whose curvature continues the faces' (Mitcad's
    /// own shape, `commands.md`), shaped by the set's tangency weight.
    Curvature,
}

/// A radius along a variable fillet at a relative position (0 to 1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MidRadius<P = ParamId> {
    pub position: f64,
    pub radius: P,
}

/// Millimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub enum FilletSizeDef<P = ParamId> {
    Constant {
        radius: P,
    },
    /// The width across the rounding (chord length).
    ChordLength {
        length: P,
    },
    /// From `start` at `start_vertex` (an end of the edges' chain; the
    /// kernel's start of the chain when left out) to `end`, through the mid
    /// radii at their positions along the chain (0 at the start, 1 at the
    /// end), smoothly.
    Variable {
        start: P,
        end: P,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_vertex: Option<VertexName>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        mid: Vec<MidRadius<P>>,
    },
    /// Two distances from the edge (in .f3d designs since 11/2025):
    /// `distance1` on the set's reference face (or on the first face of
    /// each edge's name), `distance2` on the other; `flip` swaps the faces.
    /// The cross-section is an elliptic arc (a conic) tangent to both faces.
    Asymmetric {
        distance1: P,
        distance2: P,
        #[serde(default, skip_serializing_if = "super::is_false")]
        flip: bool,
    },
    /// The radius a clamped B-spline of the position along the chain
    /// (mitcad#464): 0 at `start_vertex` (the kernel's start of the chain
    /// when left out), 1 at the other end; `radii` its control radii and
    /// `knots` its full knot vector (as many as the radii and the degree
    /// and one more; degree 1 to 3, each end repeated degree + 1 times).
    Spline {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_vertex: Option<VertexName>,
        knots: Vec<f64>,
        radii: Vec<P>,
    },
}

impl<P> FilletSizeDef<P> {
    /// The vertex a variable or spline radius starts at.
    pub fn start_vertex_mut(&mut self) -> Option<&mut VertexName> {
        match self {
            Self::Variable { start_vertex, .. } | Self::Spline { start_vertex, .. } => {
                start_vertex.as_mut()
            }
            _ => None,
        }
    }

    /// The vertex a variable or spline radius starts at.
    pub fn start_vertex(&self) -> Option<&VertexName> {
        match self {
            Self::Variable { start_vertex, .. } | Self::Spline { start_vertex, .. } => {
                start_vertex.as_ref()
            }
            _ => None,
        }
    }
}

/// The degree of a spline radius of `radii` control radii and these
/// knots, checked (mitcad#464): 1 to 3, 2 to 100 control radii, finite
/// knots rising from 0 to 1, each end repeated degree + 1 times and no
/// inner knot more than the degree times.
pub fn spline_degree(knots: &[f64], radii: usize) -> Result<usize, String> {
    if !(2..=100).contains(&radii) {
        return Err(format!(
            "a spline radius has 2 to 100 control radii, got {radii}"
        ));
    }
    let degree = knots.len().saturating_sub(radii + 1);
    if !(1..=3).contains(&degree) || radii <= degree {
        return Err(format!(
            "a spline radius of {radii} control radii has {} to {} knots (degree 1 to 3), got {}",
            radii + 2,
            radii + 4,
            knots.len()
        ));
    }
    if knots.iter().any(|k| !k.is_finite()) || knots.windows(2).any(|w| w[1] < w[0]) {
        return Err("the knots of a spline radius must rise".to_owned());
    }
    let clamped = knots[..=degree].iter().all(|k| *k == 0.0)
        && knots[knots.len() - degree - 1..].iter().all(|k| *k == 1.0);
    if !clamped {
        return Err(format!(
            "the knots of a spline radius run from 0 to 1, each repeated {} times",
            degree + 1
        ));
    }
    let inner = &knots[degree + 1..knots.len() - degree - 1];
    if inner
        .windows(degree + 1)
        .any(|w| w.iter().all(|k| *k == w[0]))
        || inner.iter().any(|k| *k <= 0.0 || *k >= 1.0)
    {
        return Err(format!(
            "an inner knot of a spline radius lies between 0 and 1 and is repeated at most {degree} times"
        ));
    }
    Ok(degree)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct FilletSetDef<P = ParamId> {
    /// A name without `#k` rounds every edge between its faces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<EdgeName>,
    /// Every edge of these faces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<FaceName>,
    /// Every edge of the body as it is when the fillet is computed (the
    /// FreeCAD import's "use all edges", mitcad#793): no edges or faces
    /// listed, the only set of the fillet. Edges an earlier feature adds or
    /// takes away after an edit are rounded or not with it.
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub all_edges: bool,
    pub size: FilletSizeDef<P>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub tangent_chain: bool,
    #[serde(default, skip_serializing_if = "is_tangent")]
    pub continuity: Continuity,
    /// G2: how long the cross-section keeps to the faces, 0.1 to 2 (1 when
    /// left out); unitless.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tangency_weight: Option<P>,
    /// Asymmetric: the face `distance1` is measured on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_face: Option<FaceName>,
}

impl<P> FilletSetDef<P> {
    /// A set of edges with a size and the defaults: tangent chain, G1.
    pub fn new(edges: Vec<EdgeName>, size: FilletSizeDef<P>) -> Self {
        Self {
            edges,
            faces: Vec::new(),
            all_edges: false,
            size,
            tangent_chain: true,
            continuity: Continuity::Tangent,
            tangency_weight: None,
            reference_face: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FilletDef<P = ParamId> {
    pub body: BodyUid,
    pub sets: Vec<FilletSetDef<P>>,
    /// Rolling ball corners; false asks for setback corners.
    pub rolling_ball_corners: bool,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn is_tangent(value: &Continuity) -> bool {
    *value == Continuity::Tangent
}

/// The file and command form: `edges` and `radius` for one plain set, or
/// `sets`.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
struct FilletForm<P> {
    body: BodyUid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    edges: Option<Vec<EdgeName>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    radius: Option<P>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sets: Option<Vec<FilletSetDef<P>>>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    rolling_ball_corners: bool,
}

/// The form with borrowed fields, for writing.
#[derive(Serialize)]
struct FilletFormRef<'a, P> {
    body: &'a BodyUid,
    #[serde(skip_serializing_if = "Option::is_none")]
    edges: Option<&'a [EdgeName]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    radius: Option<&'a P>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sets: Option<&'a [FilletSetDef<P>]>,
    #[serde(skip_serializing_if = "is_true")]
    rolling_ball_corners: bool,
}

impl<P: Serialize> Serialize for FilletDef<P> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let plain = match self.sets.as_slice() {
            [set]
                if set.faces.is_empty()
                    && !set.all_edges
                    && set.tangent_chain
                    && is_tangent(&set.continuity)
                    && set.tangency_weight.is_none()
                    && set.reference_face.is_none() =>
            {
                match &set.size {
                    FilletSizeDef::Constant { radius } => Some((&set.edges, radius)),
                    _ => None,
                }
            }
            _ => None,
        };
        let form = FilletFormRef {
            body: &self.body,
            edges: plain.map(|(edges, _)| edges.as_slice()),
            radius: plain.map(|(_, radius)| radius),
            sets: plain.is_none().then_some(self.sets.as_slice()),
            rolling_ball_corners: self.rolling_ball_corners,
        };
        form.serialize(serializer)
    }
}

impl<'de, P: Deserialize<'de>> Deserialize<'de> for FilletDef<P> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let form = FilletForm::<P>::deserialize(deserializer)?;
        let sets = match (form.edges, form.radius, form.sets) {
            (Some(edges), Some(radius), None) => {
                vec![FilletSetDef::new(edges, FilletSizeDef::Constant { radius })]
            }
            (None, None, Some(sets)) => sets,
            _ => {
                return Err(serde::de::Error::custom(
                    "a fillet has either `edges` and `radius` or `sets`",
                ));
            }
        };
        Ok(Self {
            body: form.body,
            sets,
            rolling_ball_corners: form.rolling_ball_corners,
        })
    }
}

impl<P> FilletDef<P> {
    pub const TYPE: &'static str = "fillet";
    pub const BASE_NAME: &'static str = "Fillet";

    /// One set of edges with a constant radius.
    pub fn constant(body: BodyUid, edges: Vec<EdgeName>, radius: P) -> Self {
        Self {
            body,
            sets: vec![FilletSetDef::new(edges, FilletSizeDef::Constant { radius })],
            rolling_ball_corners: true,
        }
    }

    /// Slots: `radius` (`chord_length`, `start_radius`, `end_radius`,
    /// `mid[k].radius`, `distance1`, `distance2`, `tangency_weight`) for the
    /// first set, `sets[i].radius`, ... for the others.
    // Written by hand: each set and size kind has slots of its own (above).
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<FilletDef<Q>, E> {
        let mut sets = Vec::with_capacity(self.sets.len());
        for (i, set) in self.sets.iter().enumerate() {
            let prefix = if i == 0 {
                String::new()
            } else {
                format!("sets[{i}].")
            };
            let mut slot = |name: &str, value: &P| f(&format!("{prefix}{name}"), value);
            let size = match &set.size {
                FilletSizeDef::Constant { radius } => FilletSizeDef::Constant {
                    radius: slot("radius", radius)?,
                },
                FilletSizeDef::ChordLength { length } => FilletSizeDef::ChordLength {
                    length: slot("chord_length", length)?,
                },
                FilletSizeDef::Variable {
                    start,
                    end,
                    start_vertex,
                    mid,
                } => FilletSizeDef::Variable {
                    start: slot("start_radius", start)?,
                    end: slot("end_radius", end)?,
                    start_vertex: start_vertex.clone(),
                    mid: mid
                        .iter()
                        .enumerate()
                        .map(|(k, m)| {
                            Ok(MidRadius {
                                position: m.position,
                                radius: slot(&format!("mid[{k}].radius"), &m.radius)?,
                            })
                        })
                        .collect::<Result<_, E>>()?,
                },
                FilletSizeDef::Asymmetric {
                    distance1,
                    distance2,
                    flip,
                } => FilletSizeDef::Asymmetric {
                    distance1: slot("distance1", distance1)?,
                    distance2: slot("distance2", distance2)?,
                    flip: *flip,
                },
                FilletSizeDef::Spline {
                    start_vertex,
                    knots,
                    radii,
                } => FilletSizeDef::Spline {
                    start_vertex: start_vertex.clone(),
                    knots: knots.clone(),
                    radii: radii
                        .iter()
                        .enumerate()
                        .map(|(k, r)| slot(&format!("radii[{k}]"), r))
                        .collect::<Result<_, E>>()?,
                },
            };
            let tangency_weight = match &set.tangency_weight {
                Some(weight) => Some(slot("tangency_weight", weight)?),
                None => None,
            };
            sets.push(FilletSetDef {
                edges: set.edges.clone(),
                faces: set.faces.clone(),
                all_edges: set.all_edges,
                size,
                tangent_chain: set.tangent_chain,
                continuity: set.continuity,
                tangency_weight,
                reference_face: set.reference_face.clone(),
            });
        }
        Ok(FilletDef {
            body: self.body,
            sets,
            rolling_ball_corners: self.rolling_ball_corners,
        })
    }
}

impl FeatureInfo for FilletDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        for set in &self.sets {
            references.edges(&set.edges);
            add_faces(&mut references, &set.faces);
            add_faces(&mut references, &set.reference_face);
            if let Some(vertex) = set.size.start_vertex() {
                for face in vertex.faces() {
                    references.features.extend(face.features());
                }
            }
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        if self.sets.is_empty() {
            return Err("no edges selected".to_owned());
        }
        for (i, set) in self.sets.iter().enumerate() {
            let at = |e: String| {
                if self.sets.len() > 1 {
                    format!("sets[{i}]: {e}")
                } else {
                    e
                }
            };
            check_all_edges(set.all_edges, &set.edges, &set.faces, self.sets.len()).map_err(at)?;
            if set.edges.is_empty() && set.faces.is_empty() && !set.all_edges {
                return Err(at("no edges selected".to_owned()));
            }
            if !set.edges.is_empty() {
                ctx.edges(&set.edges).map_err(at)?;
            }
            if !set.faces.is_empty() {
                check_faces(ctx, &set.faces, "faces").map_err(at)?;
            }
            for edge in &set.edges {
                if self.sets[..i].iter().any(|s| s.edges.contains(edge)) {
                    return Err(at(format!("edge {edge} is in two edge sets")));
                }
            }
            if set.all_edges && set.size.start_vertex().is_some() {
                return Err(at(
                    "a set of every edge has no start vertex (it is no one chain)".to_owned(),
                ));
            }
            if let FilletSizeDef::Variable { mid, .. } = &set.size {
                if let Some(m) = mid.iter().find(|m| !(m.position > 0.0 && m.position < 1.0)) {
                    return Err(at(format!(
                        "a mid radius position must be between 0 and 1, got {}",
                        m.position
                    )));
                }
                if let Some(pair) = mid.windows(2).find(|p| p[1].position <= p[0].position) {
                    return Err(at(format!(
                        "the mid radius positions must increase, got {} after {}",
                        pair[1].position, pair[0].position
                    )));
                }
            }
            if let FilletSizeDef::Spline { knots, radii, .. } = &set.size {
                spline_degree(knots, radii.len()).map_err(at)?;
                if set.continuity == Continuity::Curvature {
                    return Err(at(
                        "a spline radius is not taken with curvature continuity".to_owned()
                    ));
                }
            }
            if let Some(face) = &set.reference_face {
                if !matches!(set.size, FilletSizeDef::Asymmetric { .. }) {
                    return Err(at(
                        "a reference face is only for asymmetric fillets".to_owned()
                    ));
                }
                check_faces(ctx, std::slice::from_ref(face), "reference face").map_err(at)?;
            }
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        let positive = |id: &ParamId, what: &str| {
            let v = value(*id);
            if v > 0.0 {
                Ok(())
            } else {
                Err(format!("fillet {what} must be greater than zero, got {v}"))
            }
        };
        for set in &self.sets {
            match &set.size {
                FilletSizeDef::Constant { radius } => positive(radius, "radius")?,
                FilletSizeDef::ChordLength { length } => positive(length, "chord length")?,
                FilletSizeDef::Variable {
                    start, end, mid, ..
                } => {
                    positive(start, "start radius")?;
                    positive(end, "end radius")?;
                    for m in mid {
                        positive(&m.radius, "mid radius")?;
                    }
                }
                FilletSizeDef::Asymmetric {
                    distance1,
                    distance2,
                    ..
                } => {
                    positive(distance1, "distance")?;
                    positive(distance2, "distance")?;
                }
                FilletSizeDef::Spline { radii, .. } => {
                    for r in radii {
                        positive(r, "control radius")?;
                    }
                }
            }
            if let Some(weight) = set.tangency_weight {
                check_weight(value(weight))?;
            }
        }
        Ok(())
    }
}

/// A set of every edge (`all_edges`, mitcad#793) lists no edges or faces
/// and is the feature's only set (fillets and chamfers).
pub(super) fn check_all_edges(
    all_edges: bool,
    edges: &[EdgeName],
    faces: &[FaceName],
    sets: usize,
) -> Result<(), String> {
    if !all_edges {
        return Ok(());
    }
    if !edges.is_empty() || !faces.is_empty() {
        return Err("a set of every edge lists no edges or faces".to_owned());
    }
    if sets > 1 {
        return Err("a set of every edge is the only set".to_owned());
    }
    Ok(())
}

/// Every edge of a body as the kernel names them, for a set of every edge
/// (`all_edges`, mitcad#793): taken when the feature is computed, so that
/// the edges an edit upstream adds are in it.
pub(super) fn every_edge<K: Kernel>(kernel: &K, shape: &K::Shape) -> Result<Vec<EdgeName>, String> {
    let edges: Vec<EdgeName> = kernel
        .edges(shape)
        .map_err(kernel_error)?
        .into_iter()
        .filter_map(|e| e.name?.parse().ok())
        .collect();
    if edges.is_empty() {
        return Err("the body has no named edges".to_owned());
    }
    Ok(edges)
}

fn check_weight(weight: f64) -> Result<(), String> {
    if (0.1..=2.0).contains(&weight) {
        Ok(())
    } else {
        Err(format!(
            "the fillet's tangency weight must be between 0.1 and 2, got {weight}"
        ))
    }
}

/// The sizes of a set, in millimetres.
enum Size {
    Constant(f64),
    ChordLength(f64),
    /// Start, end and the mid radii (position, radius).
    Variable(f64, f64, Vec<(f64, f64)>),
    Asymmetric(f64, f64),
    /// The control radii of a spline radius.
    Spline(Vec<f64>),
}

impl<K: Kernel> Evaluate<K> for FilletDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let (sizes, weights) = self.values(ctx)?;
        let shape = ctx.surface_body(self.body)?;
        // The names the kernel gets: a face or edge a sketch change renamed
        // is found again (mitcad#272); the definition keeps its names.
        let mut found = self.clone();
        for set in &mut found.sets {
            if set.all_edges {
                set.edges = every_edge(ctx.kernel, &shape)?;
                continue;
            }
            set.edges = ctx.require_edges(self.body, &shape, &set.edges)?;
            set.faces = require_faces(ctx, self.body, &shape, &set.faces)?;
            if let Some(face) = &set.reference_face {
                set.reference_face = Some(ctx.face_on(self.body, &shape, face)?);
            }
            if let Some(vertex) = set.size.start_vertex_mut() {
                *vertex = ctx.vertex_on(&shape, vertex)?;
            }
        }
        let uid = ctx.uid;
        let rounded = found.apply(ctx, uid, &shape, &sizes, &weights)?;
        // The picked edges where they were, for the timeline (mitcad#957).
        let inputs = super::inputs::picked_edges(
            ctx.kernel,
            &shape,
            found
                .sets
                .iter()
                .map(|set| (set.edges.as_slice(), set.faces.as_slice())),
        );
        Ok(FeatureOutput {
            changes: vec![BodyChange::Set(self.body, rounded)],
            inputs,
            ..FeatureOutput::default()
        })
    }
}

impl FilletDef {
    /// Rounds the sets' edges of `shape`, the new faces named after
    /// `feature` (the fillet's copies on a pattern's copies, mitcad#105,
    /// are named after the pattern).
    pub(crate) fn round<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        feature: FeatureUid,
        shape: &K::Shape,
    ) -> Result<K::Shape, String> {
        let (sizes, weights) = self.values(ctx)?;
        self.apply(ctx, feature, shape, &sizes, &weights)
    }

    /// The sets' sizes and tangency weights.
    fn values<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<(Vec<Size>, Vec<f64>), String> {
        let mut sizes = Vec::with_capacity(self.sets.len());
        let mut weights = Vec::with_capacity(self.sets.len());
        for set in &self.sets {
            sizes.push(match &set.size {
                FilletSizeDef::Constant { radius } => Size::Constant(ctx.positive(*radius)?),
                FilletSizeDef::ChordLength { length } => Size::ChordLength(ctx.positive(*length)?),
                FilletSizeDef::Variable {
                    start, end, mid, ..
                } => {
                    let mut radii = Vec::with_capacity(mid.len());
                    for m in mid {
                        radii.push((m.position, ctx.positive(m.radius)?));
                    }
                    Size::Variable(ctx.positive(*start)?, ctx.positive(*end)?, radii)
                }
                FilletSizeDef::Asymmetric {
                    distance1,
                    distance2,
                    ..
                } => Size::Asymmetric(ctx.positive(*distance1)?, ctx.positive(*distance2)?),
                FilletSizeDef::Spline { radii, .. } => Size::Spline(
                    radii
                        .iter()
                        .map(|r| ctx.positive(*r))
                        .collect::<Result<_, _>>()?,
                ),
            });
            weights.push(match set.tangency_weight {
                Some(id) => {
                    let weight = ctx.param(id)?;
                    check_weight(weight).map_err(|e| format!("{}: {e}", ctx.param_name(id)))?;
                    weight
                }
                None => 1.0,
            });
        }
        Ok((sizes, weights))
    }

    fn apply<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        feature: FeatureUid,
        shape: &K::Shape,
        sizes: &[Size],
        weights: &[f64],
    ) -> Result<K::Shape, String> {
        let sets: Vec<FilletSet<'_>> = self
            .sets
            .iter()
            .zip(sizes.iter().zip(weights))
            .map(|(set, (size, weight))| FilletSet {
                edges: &set.edges,
                faces: &set.faces,
                size: match (size, &set.size) {
                    (Size::Constant(radius), _) => FilletSize::Constant { radius: *radius },
                    (Size::ChordLength(length), _) => FilletSize::ChordLength { length: *length },
                    (Size::Variable(start, end, mid), def) => FilletSize::Variable {
                        start: *start,
                        end: *end,
                        start_vertex: match def {
                            FilletSizeDef::Variable { start_vertex, .. } => start_vertex.as_ref(),
                            _ => None,
                        },
                        mid,
                    },
                    (Size::Asymmetric(distance1, distance2), def) => FilletSize::Asymmetric {
                        distance1: *distance1,
                        distance2: *distance2,
                        reference: set.reference_face.as_ref(),
                        flip: matches!(def, FilletSizeDef::Asymmetric { flip: true, .. }),
                    },
                    (Size::Spline(radii), def) => FilletSize::Spline {
                        start_vertex: def.start_vertex(),
                        knots: match def {
                            FilletSizeDef::Spline { knots, .. } => knots,
                            _ => &[],
                        },
                        radii,
                    },
                },
                tangent_chain: set.tangent_chain,
                curvature: set.continuity == Continuity::Curvature,
                weight: *weight,
            })
            .collect();
        let rounded = ctx
            .kernel
            .fillet(feature, shape, &sets, self.rolling_ball_corners)
            .map_err(kernel_error)?;
        ctx.warn_notes(&rounded);
        Ok(rounded)
    }
}

#[cfg(test)]
mod tests {
    use crate::features::{FeatureDef, ValueInput};

    fn parse(json: &str) -> Result<FeatureDef<ValueInput>, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }

    #[test]
    fn one_plain_set_keeps_the_short_form() {
        let short = r#"{"type":"fillet","body":"F2.b0","edges":["E{F2:a|F2:b}"],"radius":2.0}"#;
        let def = parse(short).unwrap();
        assert_eq!(serde_json::to_string(&def).unwrap(), short);
        let FeatureDef::Fillet(fillet) = &def else {
            panic!("a fillet");
        };
        assert_eq!(fillet.sets.len(), 1);
        assert!(fillet.sets[0].tangent_chain && fillet.rolling_ball_corners);

        // The same set written out in full reads the same.
        let full = parse(
            r#"{"type":"fillet","body":"F2.b0","sets":[{"edges":["E{F2:a|F2:b}"],
                "size":{"type":"constant","radius":2.0}}]}"#,
        )
        .unwrap();
        assert_eq!(full, def);
    }

    #[test]
    fn sets_with_options_round_trip_and_name_their_slots() {
        let json = r#"{"type":"fillet","body":"F2.b0","sets":[
            {"edges":["E{F2:a|F2:b}"],"size":{"type":"constant","radius":5.0},"tangent_chain":false},
            {"faces":["F2:c"],"size":{"type":"chord_length","length":"d7"}},
            {"edges":["E{F2:a|F2:d}"],"size":{"type":"variable","start":2.0,"end":4.0,
             "start_vertex":"V{F2:a|F2:b|F2:d}","mid":[{"position":0.25,"radius":3.0},
             {"position":0.5,"radius":"d8"}]},"continuity":"curvature","tangency_weight":1.5},
            {"edges":["E{F2:b|F2:d}"],"size":{"type":"asymmetric","distance1":2.0,"distance2":3.0,
             "flip":true},"reference_face":"F2:b"}],
            "rolling_ball_corners":false}"#;
        let def = parse(json).unwrap();
        let again = parse(&serde_json::to_string(&def).unwrap()).unwrap();
        assert_eq!(again, def);
        let mut slots = Vec::new();
        def.map_params(&mut |slot, _| {
            slots.push(slot.to_owned());
            Ok::<_, ()>(())
        })
        .unwrap();
        assert_eq!(
            slots,
            [
                "radius",
                "sets[1].chord_length",
                "sets[2].start_radius",
                "sets[2].end_radius",
                "sets[2].mid[0].radius",
                "sets[2].mid[1].radius",
                "sets[2].tangency_weight",
                "sets[3].distance1",
                "sets[3].distance2"
            ]
        );
        // The weight is unitless, the mid radii lengths.
        use crate::parameters::slot_unit;
        assert!(slot_unit("sets[2].tangency_weight").is_unitless());
        assert!(!slot_unit("sets[2].mid[0].radius").is_unitless());
        // A plain set with a weight or a reference face keeps the general form.
        let weighted = parse(
            r#"{"type":"fillet","body":"F2.b0","sets":[{"edges":["E{F2:a|F2:b}"],
                "size":{"type":"constant","radius":2.0},"tangency_weight":1.0}]}"#,
        )
        .unwrap();
        assert!(
            serde_json::to_string(&weighted)
                .unwrap()
                .contains("\"sets\"")
        );
    }

    #[test]
    fn spline_radii_round_trip_name_their_slots_and_check_their_knots() {
        let json = r#"{"type":"fillet","body":"F2.b0","sets":[
            {"edges":["E{F2:a|F2:b}"],"size":{"type":"spline","start_vertex":"V{F2:a|F2:b|F2:d}",
             "knots":[0.0,0.0,0.0,0.5,1.0,1.0,1.0],"radii":[2.0,4.0,"d7",3.0]},"tangent_chain":false}]}"#;
        let def = parse(json).unwrap();
        let again = parse(&serde_json::to_string(&def).unwrap()).unwrap();
        assert_eq!(again, def);
        let mut slots = Vec::new();
        def.map_params(&mut |slot, _| {
            slots.push(slot.to_owned());
            Ok::<_, ()>(())
        })
        .unwrap();
        assert_eq!(slots, ["radii[0]", "radii[1]", "radii[2]", "radii[3]"]);
        assert!(!crate::parameters::slot_unit("sets[1].radii[0]").is_unitless());
        let FeatureDef::Fillet(fillet) = &def else {
            panic!("a fillet");
        };
        assert_eq!(
            fillet.sets[0].size.start_vertex().map(ToString::to_string),
            Some("V{F2:a|F2:b|F2:d}".to_owned())
        );

        use super::spline_degree;
        assert_eq!(
            spline_degree(&[0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0], 4),
            Ok(2)
        );
        assert_eq!(spline_degree(&[0.0, 0.0, 1.0, 1.0], 2), Ok(1));
        assert_eq!(
            spline_degree(&[0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0], 4),
            Ok(3)
        );
        for (knots, radii, expected) in [
            (&[0.0, 0.0, 1.0][..], 2, "degree 1 to 3"),
            (
                &[0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0][..],
                5,
                "degree 1 to 3",
            ),
            (&[0.0, 0.0, 1.0, 1.0][..], 1, "2 to 100 control radii"),
            (&[0.0, 0.0, 0.6, 0.5, 1.0, 1.0][..], 4, "must rise"),
            (&[0.0, 0.1, 0.5, 1.0, 1.0][..], 3, "from 0 to 1"),
            (
                &[0.0, 0.0, 0.5, 0.5, 1.0, 1.0][..],
                4,
                "repeated at most 1 times",
            ),
        ] {
            let error = spline_degree(knots, radii).unwrap_err();
            assert!(error.contains(expected), "{knots:?}: {error}");
        }
    }

    #[test]
    fn malformed_fillets_are_rejected() {
        for (json, expected) in [
            (
                r#"{"type":"fillet","body":"F2.b0","edges":["E{F2:a|F2:b}"]}"#,
                "either `edges` and `radius` or `sets`",
            ),
            (
                r#"{"type":"fillet","body":"F2.b0","edges":[],"radius":1,"sets":[]}"#,
                "either `edges` and `radius` or `sets`",
            ),
            (
                r#"{"type":"fillet","body":"F2.b0","sets":[{"edges":[],"size":{"type":"round"}}]}"#,
                "unknown variant `round`",
            ),
        ] {
            let error = parse(json).unwrap_err();
            assert!(error.contains(expected), "{error}");
        }
    }
}
