// SPDX-License-Identifier: MIT
//! Bounding solid (mitcad#460): a new body, the box around bodies grown by
//! a margin on every side. The box is aligned with the axes of the
//! feature's component and bounds the bodies' exact geometry; it is built
//! again from the bodies on every recompute, so it follows their edits.
//! The bodies themselves are only read. No bodies listed: every body of
//! the component at the feature's point of the timeline. Faces are those
//! of a box primitive (see [`super::primitive`]): `<feature>:bottom` at the
//! lowest z, `top` at the highest, `side0` at the lowest y, `side1` at the
//! highest x, `side2` at the highest y and `side3` at the lowest x.

use serde::{Deserialize, Serialize};

use super::mapping::map_params;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
};
use crate::ids::BodyUid;
use crate::kernel::{BoundingBox, Kernel};
use crate::parameters::ParamId;
use crate::profile::SketchFrame;
use crate::transform::{PrimitiveShape, PrimitiveSpec};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundingSolidDef<P = ParamId> {
    /// The bodies the box holds; empty: every body of the component.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bodies: Vec<BodyUid>,
    /// How far the box stands off the bodies on every side (a negative
    /// margin shrinks it).
    pub margin: P,
}

impl<P> BoundingSolidDef<P> {
    pub const TYPE: &'static str = "bounding_solid";
    pub const BASE_NAME: &'static str = "BoundingSolid";

    map_params! {
        BoundingSolidDef {
            bodies: clone,
            margin: param,
        }
    }
}

/// The box `bounds` grown by `margin` on every side, as the corner and
/// the sizes along x, y and z; Err when nothing is left of it.
fn grown(bounds: &BoundingBox, margin: f64) -> Result<([f64; 3], [f64; 3]), String> {
    if !margin.is_finite() {
        return Err(format!("the margin must be finite, got {margin}"));
    }
    let min: [f64; 3] = std::array::from_fn(|i| bounds.min[i] - margin);
    let size: [f64; 3] = std::array::from_fn(|i| bounds.max[i] + margin - min[i]);
    if size.iter().all(|s| s.is_finite() && *s > 0.0) {
        Ok((min, size))
    } else {
        Err(format!(
            "a margin of {margin} leaves no box around the bodies (they are {} × {} × {})",
            bounds.max[0] - bounds.min[0],
            bounds.max[1] - bounds.min[1],
            bounds.max[2] - bounds.min[2]
        ))
    }
}

impl FeatureInfo for BoundingSolidDef {
    fn references(&self) -> References {
        let mut references = References::default();
        for body in &self.bodies {
            references.body(*body);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        for (i, body) in self.bodies.iter().enumerate() {
            if self.bodies[..i].contains(body) {
                return Err(format!("body {body} is listed more than once"));
            }
            ctx.body(*body)?;
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        let margin = value(self.margin);
        if margin.is_finite() {
            Ok(())
        } else {
            Err(format!("the margin must be finite, got {margin}"))
        }
    }

    fn creates_bodies(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for BoundingSolidDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let margin = ctx.param(self.margin)?;
        let shapes: Vec<K::Shape> = if self.bodies.is_empty() {
            ctx.bodies().into_iter().map(|(_, shape)| shape).collect()
        } else {
            self.bodies
                .iter()
                .map(|body| ctx.body(*body))
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut bounds: Option<BoundingBox> = None;
        for shape in &shapes {
            let Some(b) = ctx.kernel.bounding_box(shape).map_err(|e| e.to_string())? else {
                continue;
            };
            bounds = Some(match bounds {
                Some(a) => a.union(&b),
                None => b,
            });
        }
        let bounds = bounds.ok_or_else(|| "there are no bodies to bound".to_owned())?;
        let (corner, size) = grown(&bounds, margin)?;
        let spec = PrimitiveSpec {
            feature: ctx.uid,
            frame: SketchFrame {
                origin: corner,
                ..SketchFrame::XY
            },
            shape: PrimitiveShape::Box {
                length: size[0],
                width: size[1],
                height: size[2],
            },
        };
        let shape = ctx.kernel.primitive(&spec).map_err(|e| e.to_string())?;
        let body = ctx.new_body();
        Ok(FeatureOutput {
            changes: vec![BodyChange::Set(body, shape)],
            ..FeatureOutput::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::features::SketchPlane;
    use crate::test_support::{block, def, num};
    use crate::testing::{MockKernel, MockShape};
    use crate::topo::FaceName;
    use crate::{Document, FeatureStatus};

    fn shape(doc: &Document<MockKernel>, body: &str) -> MockShape {
        let uid: BodyUid = body.parse().unwrap();
        doc.body_shape(uid)
            .unwrap_or_else(|| panic!("no body {body}"))
            .clone()
    }

    fn assert_bounds(doc: &Document<MockKernel>, body: &str, min: [f64; 3], max: [f64; 3]) {
        let b = shape(doc, body).bounds.expect("bounds");
        let near = |a: [f64; 3], e: [f64; 3]| (0..3).all(|i| (a[i] - e[i]).abs() < 1e-9);
        assert!(
            near(b.min, min) && near(b.max, max),
            "{body}: {:?} .. {:?}, expected {min:?} .. {max:?}",
            b.min,
            b.max
        );
    }

    #[test]
    fn margins_grow_the_box_on_every_side() {
        let b = BoundingBox {
            min: [0.0, 0.0, 0.0],
            max: [60.0, 40.0, 20.0],
        };
        assert_eq!(grown(&b, 5.0), Ok(([-5.0, -5.0, -5.0], [70.0, 50.0, 30.0])));
        assert_eq!(grown(&b, 0.0), Ok(([0.0; 3], [60.0, 40.0, 20.0])));
        assert_eq!(grown(&b, -5.0), Ok(([5.0, 5.0, 5.0], [50.0, 30.0, 10.0])));
        let error = grown(&b, -10.0).unwrap_err();
        assert!(error.contains("leaves no box"), "{error}");
        assert!(grown(&b, f64::NAN).is_err());
    }

    /// A box around every body of the component, a new body with the
    /// faces of a box; the bodies stay as they were, and the box follows
    /// their edits and its margin's.
    #[test]
    fn a_bounding_solid_follows_its_bodies() {
        let mut b = block();
        let before = shape(&b.doc, "F2.b0");
        let added = b
            .doc
            .add_feature(&def(json!({"type": "bounding_solid", "margin": 5})), None)
            .unwrap();
        assert_eq!(added.name, "BoundingSolid1");
        assert_eq!(added.parameters, vec!["d4"]);
        assert_eq!(b.doc.status(added.uid), Some(&FeatureStatus::Ok));
        assert_bounds(&b.doc, "F3.b0", [-5.0, -5.0, -5.0], [65.0, 45.0, 25.0]);
        assert_eq!(shape(&b.doc, "F2.b0"), before);
        let faces = shape(&b.doc, "F3.b0").faces;
        for role in ["bottom", "top", "side0", "side1", "side2", "side3"] {
            assert!(
                faces.contains(&FaceName::new(added.uid, role, None)),
                "{role}: {faces:?}"
            );
        }
        // The block made higher: the box follows.
        b.doc.set_parameter("d3", 30.0).unwrap();
        assert_bounds(&b.doc, "F3.b0", [-5.0, -5.0, -5.0], [65.0, 45.0, 35.0]);
        // No margin: the box is the block's.
        b.doc.set_parameter("d4", 0.0).unwrap();
        assert_bounds(&b.doc, "F3.b0", [0.0; 3], [60.0, 40.0, 30.0]);
        // A margin that leaves nothing fails the feature.
        b.doc.set_parameter("d4", -20.0).unwrap();
        assert!(matches!(
            b.doc.status(added.uid),
            Some(FeatureStatus::Failed(m)) if m.contains("leaves no box")
        ));
    }

    /// Listed bodies only; a body listed twice is refused.
    #[test]
    fn a_bounding_solid_holds_the_bodies_it_lists() {
        let mut b = block();
        let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
        let region = b
            .doc
            .add_rectangle(sketch, [100.0, 0.0], &num(20.0), &num(10.0))
            .unwrap()
            .region;
        let second = b
            .doc
            .add_feature(
                &def(json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                            "extent": {"type": "distance", "distance": 50}, "operation": "new_body"})),
                None,
            )
            .unwrap()
            .uid;
        let second = BodyUid::new(second, 0).to_string();
        let one = b
            .doc
            .add_feature(
                &def(json!({"type": "bounding_solid", "bodies": [second], "margin": 1})),
                None,
            )
            .unwrap();
        assert_bounds(
            &b.doc,
            &BodyUid::new(one.uid, 0).to_string(),
            [99.0, -1.0, -1.0],
            [121.0, 11.0, 51.0],
        );
        let both = b
            .doc
            .add_feature(&def(json!({"type": "bounding_solid", "margin": 0})), None)
            .unwrap();
        // The first bounding solid is a body of the component too.
        assert_bounds(
            &b.doc,
            &BodyUid::new(both.uid, 0).to_string(),
            [0.0, -1.0, -1.0],
            [121.0, 40.0, 51.0],
        );
        let error = b
            .doc
            .add_feature(
                &def(json!({"type": "bounding_solid", "bodies": [second, second], "margin": 1})),
                None,
            )
            .unwrap_err();
        assert!(error.to_string().contains("more than once"), "{error}");
    }

    /// With no bodies there is nothing to bound.
    #[test]
    fn a_bounding_solid_needs_bodies() {
        let mut doc = Document::new(MockKernel::default());
        let added = doc
            .add_feature(&def(json!({"type": "bounding_solid", "margin": 1})), None)
            .unwrap();
        assert!(matches!(
            doc.status(added.uid),
            Some(FeatureStatus::Failed(m)) if m.contains("no bodies")
        ));
    }
}
