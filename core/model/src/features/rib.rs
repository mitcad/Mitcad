// SPDX-License-Identifier: MIT
//! Rib and web: thin walls grown from open sketch curves and joined to the
//! bodies they reach. A rib grows from a chain of curves in the sketch
//! plane, its thickness across the plane; a web grows from each curve along
//! the sketch normal, its thickness in the plane. Either runs a depth, or
//! up to the next faces of the bodies.

use serde::{Deserialize, Serialize};

use super::extrude::{Operation, apply_operation, check_participants, participant_bodies};
use super::path::{self, SketchCurves};
use super::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, ToolUse, is_false,
};
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::sweeps::{RibSpec, ThicknessLocation};

/// How far a rib or a web grows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RibExtent<P = ParamId> {
    /// Up to the faces of the bodies it reaches first, which must close it
    /// off.
    ToNext,
    Depth {
        depth: P,
    },
}

macro_rules! thin_feature {
    ($(#[$doc:meta])* $name:ident, $type:literal, $base:literal, $web:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name<P = ParamId> {
            pub curves: SketchCurves,
            pub thickness: P,
            #[serde(default, skip_serializing_if = "is_symmetric")]
            pub thickness_location: ThicknessLocation,
            pub extent: RibExtent<P>,
            /// Grows the other way.
            #[serde(default, skip_serializing_if = "is_false")]
            pub flip: bool,
            /// The bodies it reaches and joins; empty for all.
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            pub participants: Vec<BodyUid>,
        }

        impl<P> $name<P> {
            pub const TYPE: &'static str = $type;
            pub const BASE_NAME: &'static str = $base;

            pub fn map_params<Q, E>(
                &self,
                f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
            ) -> Result<$name<Q>, E> {
                Ok($name {
                    curves: self.curves.clone(),
                    thickness: f("thickness", &self.thickness)?,
                    thickness_location: self.thickness_location,
                    extent: match &self.extent {
                        RibExtent::ToNext => RibExtent::ToNext,
                        RibExtent::Depth { depth } => RibExtent::Depth {
                            depth: f("extent.depth", depth)?,
                        },
                    },
                    flip: self.flip,
                    participants: self.participants.clone(),
                })
            }

            fn view(&self) -> Thin<'_, P> {
                Thin {
                    web: $web,
                    curves: &self.curves,
                    thickness: &self.thickness,
                    location: self.thickness_location,
                    extent: &self.extent,
                    flip: self.flip,
                    participants: &self.participants,
                }
            }
        }

        impl FeatureInfo for $name {
            fn references(&self) -> References {
                self.view().references()
            }

            fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
                self.view().check(ctx)
            }

            fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
                self.view().check_values(value)
            }

            fn creates_bodies(&self) -> bool {
                // A rib that reaches no body to join becomes one.
                true
            }

            fn tool_use(&self) -> Option<ToolUse> {
                Some(ToolUse {
                    operation: Operation::Join,
                    participants: self.participants.clone(),
                })
            }
        }

        impl<K: Kernel> Evaluate<K> for $name {
            fn evaluate(
                &self,
                ctx: &mut EvalContext<'_, K>,
            ) -> Result<FeatureOutput<K::Shape>, String> {
                self.view().evaluate(ctx)
            }
        }
    };
}

thin_feature!(
    /// A rib from one open chain of sketch curves.
    RibDef,
    "rib",
    "Rib",
    false
);
thin_feature!(
    /// A web from sketch curves (lines and arcs), one wall per curve.
    WebDef,
    "web",
    "Web",
    true
);

fn is_symmetric(location: &ThicknessLocation) -> bool {
    *location == ThicknessLocation::Symmetric
}

/// The fields of a rib or a web.
struct Thin<'a, P> {
    web: bool,
    curves: &'a SketchCurves,
    thickness: &'a P,
    location: ThicknessLocation,
    extent: &'a RibExtent<P>,
    flip: bool,
    participants: &'a [BodyUid],
}

impl Thin<'_, ParamId> {
    fn what(&self) -> &'static str {
        if self.web { "web" } else { "rib" }
    }

    fn references(&self) -> References {
        let mut references = References::default();
        self.curves.add_references(&mut references);
        for body in self.participants {
            references.body(*body);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_participants(ctx, Operation::Join, self.participants)?;
        self.curves.check(ctx)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        let positive = |id: ParamId, what: &str| {
            let v = value(id);
            if v > 0.0 && v.is_finite() {
                Ok(())
            } else {
                Err(format!(
                    "the {}'s {what} must be greater than zero, got {v}",
                    self.what()
                ))
            }
        };
        positive(*self.thickness, "thickness")?;
        if let RibExtent::Depth { depth } = self.extent {
            positive(*depth, "depth")?;
        }
        Ok(())
    }

    fn evaluate<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<FeatureOutput<K::Shape>, String> {
        let thickness = ctx.param(*self.thickness)?;
        let depth = match self.extent {
            RibExtent::ToNext => None,
            RibExtent::Depth { depth } => Some(ctx.param(*depth)?),
        };
        self.check_values(&|id| {
            if id == *self.thickness {
                thickness
            } else {
                depth.unwrap_or(f64::NAN)
            }
        })?;
        let participants = participant_bodies(ctx, Operation::Join, self.participants)?;
        let (frame, curves) = self.curves.curves(ctx)?;
        let chains = if self.web {
            for curve in &curves {
                if path::curve_ends(&curve.curve).is_none() {
                    return Err(format!("{} is closed; a web needs open curves", curve.name));
                }
            }
            curves.into_iter().map(|c| vec![c]).collect()
        } else {
            let chain = path::chain(curves)?;
            if path::is_closed(&chain) {
                return Err("a rib needs an open chain of curves".to_owned());
            }
            vec![chain]
        };
        let spec = RibSpec {
            feature: ctx.uid,
            frame,
            web: self.web,
            chains: &chains,
            thickness,
            location: self.location,
            depth,
            flip: self.flip,
            bodies: participants.iter().map(|(_, s)| s.clone()).collect(),
        };
        let tool = ctx.kernel.rib(&spec).map_err(|e| e.to_string())?;
        apply_operation(ctx, Operation::Join, participants, tool, self.what())
    }
}
