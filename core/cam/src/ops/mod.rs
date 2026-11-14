// SPDX-License-Identifier: MIT
//! The 2.5-axis operations: each makes a toolpath from planar outlines,
//! heights and a tool (docs/cam.md, "Operations"). All values are in the
//! work coordinates (mm), Z up, the stock's top usually at Z 0.

pub mod contour;
pub mod drill;
pub mod face;
pub mod pocket;

use serde::{Deserialize, Serialize};

use crate::CamError;
use crate::tool::{Cutting, Tool};

pub use contour::{ContourOp, Side, contour};
pub use drill::{DrillOp, drill};
pub use face::{FaceOp, face};
pub use pocket::{PocketOp, pocket};

// Entries, tabs, leads, adaptive clearing and rest clean-up (phase 2 of
// mitcad#1013).
pub mod adaptive;
pub mod entry;
pub(crate) mod path;
pub mod rest;

pub use adaptive::{Adaptive, adaptive_face, adaptive_pocket, engagement};
pub use contour::{Lead, TabLayout, Tabs};
pub use entry::{Entry, EntryUsed};
pub use rest::{RestOp, rest, rest_pieces};

// 3-axis machining of freeform bodies on their triangle meshes: roughing
// by levels, raster and waterline finishing (phase 3 of mitcad#1013).
pub mod raster;
pub mod roughing;
pub mod surface;
pub mod waterline;
pub(crate) mod zlevel;

pub use raster::{RasterOp, raster};
pub use roughing::{Roughing3dOp, roughing};
pub use surface::{Contact, Surface, SurfaceCutter};
pub use waterline::{WaterlineOp, waterline};

/// The depths an operation cuts between: from `top` down to `bottom` in
/// passes of at most `step_down`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Depths {
    /// Z of the material's top where the cut starts.
    pub top: f64,
    /// Z of the final floor.
    pub bottom: f64,
    /// The deepest pass (mm), more than zero.
    pub step_down: f64,
}

impl Depths {
    /// The Z of each pass, top to bottom, the passes equal in depth.
    pub fn levels(&self) -> Result<Vec<f64>, CamError> {
        let total = self.top - self.bottom;
        if !(self.top.is_finite() && self.bottom.is_finite()) {
            return Err(CamError::new("The depths must be numbers."));
        }
        if total <= 0.0 {
            return Err(CamError::new("The bottom must be below the top."));
        }
        if !(self.step_down.is_finite() && self.step_down > 0.0) {
            return Err(CamError::new("The step down must be more than zero."));
        }
        let n = ((total / self.step_down) - 1e-9).ceil().max(1.0) as usize;
        Ok((1..=n)
            .map(|i| {
                if i == n {
                    self.bottom
                } else {
                    self.top - total * i as f64 / n as f64
                }
            })
            .collect())
    }
}

/// The heights the tool moves at between cuts.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Heights {
    /// Rapids between features and at the start and end, clear of the
    /// stock and the clamps.
    pub clearance: f64,
    /// Rapids end here above the material before a plunge at the plunge
    /// feed; between `top` and `clearance`.
    pub retract: f64,
}

impl Heights {
    pub fn validate(&self, top: f64) -> Result<(), CamError> {
        if !(self.clearance.is_finite() && self.retract.is_finite()) {
            return Err(CamError::new("The heights must be numbers."));
        }
        if self.retract < top {
            return Err(CamError::new(
                "The retract height must not be below the top.",
            ));
        }
        if self.clearance < self.retract {
            return Err(CamError::new(
                "The clearance height must not be below the retract height.",
            ));
        }
        Ok(())
    }
}

/// Which way the tool runs along a wall, for a clockwise spindle (M3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// The cutter's edge meets the material at full chip thickness: the
    /// material on the right of the travel. The usual choice on rigid
    /// machines and for finishing.
    #[default]
    Climb,
    /// The material on the left of the travel.
    Conventional,
}

/// What every milling operation needs of its tool and speeds.
pub(crate) fn check_mill(tool: &Tool, cutting: &Cutting, depths: &Depths) -> Result<(), CamError> {
    tool.validate().map_err(CamError::new)?;
    cutting.validate().map_err(CamError::new)?;
    if !tool.shape.flat_bottom() {
        return Err(CamError::new(
            "The operation needs a flat or bull nose end mill; this tool's end is not flat.",
        ));
    }
    if depths.step_down > tool.flute_length + 1e-9 {
        return Err(CamError::new(
            "The step down is deeper than the tool's flutes are long.",
        ));
    }
    if depths.top - depths.bottom > tool.length + 1e-9 {
        return Err(CamError::new(
            "The cut is deeper than the tool sticks out of its holder.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_split_the_depth_evenly() {
        let d = Depths {
            top: 0.0,
            bottom: -10.0,
            step_down: 3.0,
        };
        assert_eq!(d.levels().unwrap(), vec![-2.5, -5.0, -7.5, -10.0]);
        let d = Depths {
            top: 0.0,
            bottom: -6.0,
            step_down: 3.0,
        };
        assert_eq!(d.levels().unwrap(), vec![-3.0, -6.0]);
        let d = Depths {
            top: 1.0,
            bottom: 0.5,
            step_down: 3.0,
        };
        assert_eq!(d.levels().unwrap(), vec![0.5]);
        assert!(
            Depths {
                top: 0.0,
                bottom: 1.0,
                step_down: 1.0
            }
            .levels()
            .is_err()
        );
        assert!(
            Depths {
                top: 0.0,
                bottom: -1.0,
                step_down: 0.0
            }
            .levels()
            .is_err()
        );
    }

    #[test]
    fn heights_are_ordered() {
        assert!(
            Heights {
                clearance: 10.0,
                retract: 2.0
            }
            .validate(0.0)
            .is_ok()
        );
        assert!(
            Heights {
                clearance: 1.0,
                retract: 2.0
            }
            .validate(0.0)
            .is_err()
        );
        assert!(
            Heights {
                clearance: 10.0,
                retract: -1.0
            }
            .validate(0.0)
            .is_err()
        );
    }
}
