// SPDX-License-Identifier: MIT
//! Fill Cavity (mitcad#1818): a closed pocket or slit of a body filled with
//! material up to a plane, the cap. Two booleans, no flood:
//!
//! 1. a box below the cap: the body's extent in the cap's frame grown by a
//!    margin, its top on the cap; with a seed face first only the seed's
//!    neighbourhood (the seed face's extent grown by its own size), so that
//!    heavy faces elsewhere (threads) are not cut;
//! 2. the box less the body: the empty spaces below the cap as solids
//!    ([`Kernel::cavity_pieces`]), which tell by their faces' names what they
//!    touch: the cap (the box's top), a side or the bottom of the box, the
//!    seed face;
//! 3. with a seed the pieces that touch the seed face, without one every
//!    piece that touches the cap and nothing of the box besides; a chosen
//!    piece that touches a side or the bottom of the box opens out of the
//!    body below the cap, and the feature fails in words, the opening's
//!    places painted on hover. A seed's neighbourhood that a chosen piece
//!    leaves is not yet an opening: the whole box decides;
//! 4. the chosen pieces joined to the body, which keeps its id.
//!
//! The filled top is `<feature>:fill(<cap face>)`, or `<feature>:fill` for a
//! cap that is a plane; the pocket's walls are inside the body after the
//! join and keep no faces. The cap is filled against its normal (below a
//! planar face of the body, on its material's side; below an XY plane),
//! on the other side with `flip`.

use serde::{Deserialize, Serialize};

use super::base::apply_tool;
use super::face_refs::{add_faces, check_faces, kernel_error, map_offset};
use super::geom_ref::{GeomRef, Want};
use super::mapping::map_params;
use super::pattern::none;
use super::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, is_false,
};
use crate::datum::Vec3;
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::{BooleanOp, BoundingBox, CavitySpec, Kernel};
use crate::notice::{FeatureError, NoticePlace};
use crate::parameters::ParamId;
use crate::topo::{FaceName, RoleKey};
use mitcad_vec3::{add, cross, dot, norm, scale, sub};

/// The role of the filled top's faces.
pub const TOP_ROLE: &str = "fill";
/// The roles of the box's sides and bottom ([`Kernel::cavity_pieces`]).
pub const SIDE_ROLE: &str = "fill_side";
pub const BOTTOM_ROLE: &str = "fill_bottom";

/// Lengths in millimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FillCavityDef<P = ParamId> {
    pub body: BodyUid,
    /// The plane up to which the cavity is filled: an origin or
    /// construction plane, a planar face (of this body or another) or a
    /// fixed plane.
    pub cap: GeomRef,
    /// A face of the body inside the cavity: the pockets it bounds are
    /// filled, and no other. Without one every pocket the cap closes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<FaceName>,
    /// Moves the cap along its normal.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub offset: Option<P>,
    /// Fills on the side of the cap its normal points to.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
}

impl<P> FillCavityDef<P> {
    pub const TYPE: &'static str = "fill_cavity";
    pub const BASE_NAME: &'static str = "FillCavity";

    map_params! {
        FillCavityDef {
            body: copy,
            cap: clone,
            seed: clone,
            offset: with(map_offset),
            flip: copy,
        }
    }
}

impl FeatureInfo for FillCavityDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        self.cap.add_to(&mut references);
        add_faces(&mut references, &self.seed);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        self.cap
            .check(ctx, Want::Plane)
            .map_err(|e| format!("the cap: {e}"))?;
        if let Some(seed) = &self.seed {
            check_faces(ctx, std::slice::from_ref(seed), "seed face")?;
        }
        Ok(())
    }
}

/// The cap's frame: `normal` points away from the side that is filled.
struct CapFrame {
    origin: Vec3,
    x_axis: Vec3,
    y_axis: Vec3,
    normal: Vec3,
}

impl CapFrame {
    /// A point in the frame's coordinates (u, v, w).
    fn coordinates(&self, point: Vec3) -> Vec3 {
        let d = sub(point, self.origin);
        [
            dot(d, self.x_axis),
            dot(d, self.y_axis),
            dot(d, self.normal),
        ]
    }

    /// The extent of a box (in the body's coordinates) in the frame.
    fn extent(&self, b: &BoundingBox) -> Extent {
        let mut extent = Extent {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        };
        for corner in b.corners() {
            let c = self.coordinates(corner);
            for (k, &v) in c.iter().enumerate() {
                extent.min[k] = extent.min[k].min(v);
                extent.max[k] = extent.max[k].max(v);
            }
        }
        extent
    }
}

/// A box in the cap's frame.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Extent {
    min: Vec3,
    max: Vec3,
}

impl Extent {
    fn size(&self) -> f64 {
        norm(sub(self.max, self.min))
    }

    /// Grown by `margin` on every side but the top, which is put on the
    /// cap.
    fn below_cap(&self, margin: f64) -> Extent {
        Extent {
            min: self.min.map(|v| v - margin),
            max: [self.max[0] + margin, self.max[1] + margin, 0.0],
        }
    }

    /// The part of this box inside `other`.
    fn within(&self, other: &Extent) -> Extent {
        Extent {
            min: std::array::from_fn(|k| self.min[k].max(other.min[k])),
            max: std::array::from_fn(|k| self.max[k].min(other.max[k])),
        }
    }

    fn same(&self, other: &Extent) -> bool {
        (0..3).all(|k| {
            (self.min[k] - other.min[k]).abs() <= EPSILON
                && (self.max[k] - other.max[k]).abs() <= EPSILON
        })
    }
}

/// Lengths closer than this are equal (mm).
const EPSILON: f64 = 1e-6;
/// The least margin round the box (mm).
const MARGIN: f64 = 1.0;

/// What an empty space below the cap touches, by its faces' names.
#[derive(Debug, Default)]
struct Touches {
    cap: bool,
    /// Indices of its faces on a side or the bottom of the box.
    openings: Vec<usize>,
    seed: bool,
}

fn touches<K: Kernel>(
    kernel: &K,
    piece: &K::Shape,
    feature: FeatureUid,
    seed: Option<&FaceName>,
) -> Result<Touches, String> {
    let mut found = Touches::default();
    for (i, face) in kernel
        .faces(piece)
        .map_err(kernel_error)?
        .iter()
        .enumerate()
    {
        for name in &face.names {
            let Ok(name) = name.parse::<FaceName>() else {
                continue;
            };
            let own = name.feature == feature;
            let wall = own && (name.role == SIDE_ROLE || name.role == BOTTOM_ROLE);
            found.cap |= own && name.role == TOP_ROLE;
            if wall && !found.openings.contains(&i) {
                found.openings.push(i);
            }
            found.seed |= !own && seed.is_some_and(|seed| name.matches(seed));
        }
    }
    Ok(found)
}

/// The places of an opening: the boxes of a piece's faces on the sides or
/// the bottom of the box (where the empty space leaves it), each a box in
/// the body's coordinates.
fn opening_places<K: Kernel>(kernel: &K, piece: &K::Shape, faces: &[usize]) -> Vec<NoticePlace> {
    kernel
        .face_bounding_boxes(piece, faces)
        .map(|boxes| {
            boxes
                .into_iter()
                .flatten()
                .map(|b| NoticePlace::Box {
                    min: b.min,
                    max: b.max,
                })
                .collect()
        })
        .unwrap_or_default()
}

impl FillCavityDef {
    /// The name of the filled top: keyed by a cap that is a face.
    fn top(&self, feature: FeatureUid) -> FaceName {
        let key = match &self.cap {
            GeomRef::Face { face, .. } => Some(RoleKey::Face(Box::new(face.clone()))),
            _ => None,
        };
        FaceName::new(feature, TOP_ROLE, key)
    }

    /// An error about the body in words, with its places and the detail
    /// for agents.
    fn error(&self, said: &str, detail: String, places: Vec<NoticePlace>) -> String {
        FeatureError::new(said)
            .with_detail(detail)
            .with_places(places)
            .in_body(&self.body.to_string())
            .text()
    }

    fn seed_place(seed: &FaceName) -> NoticePlace {
        NoticePlace::face(seed.to_string())
    }

    /// The seed's neighbourhood below the cap: the seed face's extent in
    /// the frame grown by its size (at least [`MARGIN`]), within `whole`;
    /// None when the kernel cannot tell the face's box, or when the
    /// neighbourhood is the whole box anyway.
    fn seed_extent<K: Kernel>(
        &self,
        kernel: &K,
        shape: &K::Shape,
        seed: &FaceName,
        frame: &CapFrame,
        whole: &Extent,
    ) -> Result<Option<Extent>, String> {
        let Ok(faces) = kernel.faces(shape) else {
            return Ok(None);
        };
        let indices: Vec<usize> = faces
            .iter()
            .enumerate()
            .filter(|(_, face)| {
                face.names
                    .iter()
                    .filter_map(|n| n.parse::<FaceName>().ok())
                    .any(|n| n.matches(seed))
            })
            .map(|(i, _)| i)
            .collect();
        if indices.is_empty() {
            return Ok(None);
        }
        let Ok(boxes) = kernel.face_bounding_boxes(shape, &indices) else {
            return Ok(None);
        };
        let Some(seed_box) = boxes.into_iter().flatten().reduce(|a, b| a.union(&b)) else {
            return Ok(None);
        };
        let extent = frame.extent(&seed_box);
        if extent.min[2] >= -EPSILON {
            return Err(self.error(
                "the seed face lies above the cap",
                format!("the seed face lies above the cap: face {seed} lies wholly above it"),
                vec![Self::seed_place(seed)],
            ));
        }
        let local = extent.below_cap(extent.size().max(MARGIN)).within(whole);
        Ok((!local.same(whole)).then_some(local))
    }
}

impl<K: Kernel> Evaluate<K> for FillCavityDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let shape = ctx.surface_body(self.body)?;
        // The name the kernel gets: a face a sketch change renamed is found
        // again (mitcad#272); the definition keeps its name.
        let seed = match &self.seed {
            Some(face) => Some(ctx.face_on(self.body, &shape, face)?),
            None => None,
        };
        let plane = ctx.plane(&self.cap)?;
        let offset = match self.offset {
            Some(id) => ctx.param(id)?,
            None => 0.0,
        };
        let up = plane.normal();
        let normal = if self.flip { scale(up, -1.0) } else { up };
        let frame = CapFrame {
            origin: add(plane.origin, scale(up, offset)),
            x_axis: plane.x_axis,
            y_axis: cross(normal, plane.x_axis),
            normal,
        };
        let body_box = ctx
            .kernel
            .bounding_box(&shape)
            .map_err(kernel_error)?
            .ok_or_else(|| "the body is empty".to_owned())?;
        let extent = frame.extent(&body_box);
        if extent.min[2] >= -EPSILON {
            return Err(self.error(
                "the body lies wholly above the cap: there is nothing below it to fill",
                format!(
                    "the body lies wholly above the cap: there is nothing below it to fill \
                     (body {}, the cap through {:?}, normal {:?})",
                    self.body, frame.origin, frame.normal
                ),
                Vec::new(),
            ));
        }
        let whole = extent.below_cap((extent.size() * 0.01).max(MARGIN));
        let mut boxes = Vec::new();
        if let Some(seed) = &seed
            && let Some(local) = self.seed_extent(ctx.kernel, &shape, seed, &frame, &whole)?
        {
            boxes.push(local);
        }
        boxes.push(whole);

        let top = self.top(ctx.uid);
        // Where a chosen piece first left its box: the opening's places.
        let mut opening: Vec<NoticePlace> = Vec::new();
        let mut filled: Vec<K::Shape> = Vec::new();
        for (stage, extent) in boxes.iter().enumerate() {
            let last = stage + 1 == boxes.len();
            let spec = CavitySpec {
                feature: ctx.uid,
                body: &shape,
                top: &top,
                origin: frame.origin,
                x_axis: frame.x_axis,
                normal: frame.normal,
                min: extent.min,
                max: extent.max,
            };
            let pieces = ctx.kernel.cavity_pieces(&spec).map_err(kernel_error)?;
            let mut chosen = Vec::new();
            let mut open = None;
            for piece in pieces {
                let found = touches(ctx.kernel, &piece, ctx.uid, seed.as_ref())?;
                let take = match &seed {
                    Some(_) => found.seed,
                    None => found.cap && found.openings.is_empty(),
                };
                if !take {
                    continue;
                }
                if !found.openings.is_empty() && open.is_none() {
                    open = Some(opening_places(ctx.kernel, &piece, &found.openings));
                }
                chosen.push(piece);
            }
            if let Some(places) = open {
                if opening.is_empty() {
                    opening = places;
                }
                if !last {
                    // Beyond the seed's neighbourhood: the whole box tells.
                    continue;
                }
                let seed = seed.as_ref().expect("only a seed chooses open pieces");
                let mut places = vec![Self::seed_place(seed)];
                places.extend(opening);
                return Err(self.error(
                    "the cavity opens out of the body below the cap",
                    format!(
                        "the cavity opens out of the body below the cap: the empty space at the \
                         seed face {seed} reaches a side or the bottom of \
                         the box below the cap through {:?} (normal {:?}): it is not closed \
                         below the cap",
                        frame.origin, frame.normal
                    ),
                    places,
                ));
            }
            if chosen.is_empty() {
                return Err(match &seed {
                    Some(seed) => self.error(
                        "the seed face does not bound an empty space below the cap",
                        format!(
                            "the seed face does not bound an empty space below the cap: no \
                             empty space below the cap through {:?} (normal {:?}) touches \
                             the seed face {seed}",
                            frame.origin, frame.normal
                        ),
                        vec![Self::seed_place(seed)],
                    ),
                    None => self.error(
                        "found no closed cavity below the cap to fill",
                        format!(
                            "found no closed cavity below the cap to fill: every empty space \
                             below the cap through {:?} (normal {:?}) opens \
                             out of body {} or does not reach the cap",
                            frame.origin, frame.normal, self.body
                        ),
                        Vec::new(),
                    ),
                });
            }
            filled = chosen;
            break;
        }

        let tool = if filled.len() == 1 {
            filled.pop().expect("one piece")
        } else {
            let refs: Vec<&K::Shape> = filled.iter().collect();
            ctx.kernel.unite(&refs).map_err(kernel_error)?
        };
        let changes = apply_tool(
            ctx,
            BooleanOp::Join,
            &[(self.body, shape.clone())],
            &tool,
            "fill",
        )?;
        Ok(FeatureOutput {
            changes,
            tool: Some(tool),
            ..FeatureOutput::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::features::{FeatureDef, ValueInput};

    fn parse(json: &str) -> FeatureDef<ValueInput> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn the_definition_round_trips_with_its_slot() {
        let short = r#"{"type":"fill_cavity","body":"F2.b0","cap":"F5"}"#;
        assert_eq!(serde_json::to_string(&parse(short)).unwrap(), short);
        let full = r#"{"type":"fill_cavity","body":"F2.b0","cap":{"body":"F2.b0","face":"F2:top"},"seed":"F4:bottom","offset":-2.0,"flip":true}"#;
        let def = parse(full);
        assert_eq!(serde_json::to_string(&def).unwrap(), full);
        let mut slots = Vec::new();
        def.map_params(&mut |slot, _| {
            slots.push(slot.to_owned());
            Ok::<_, ()>(())
        })
        .unwrap();
        assert_eq!(slots, ["offset"]);
    }
}
