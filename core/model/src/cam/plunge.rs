// SPDX-License-Identifier: MIT
//! Turn Finishing into undercuts (mitcad#2503, GitHub mitcad/Mitcad#707):
//! the operation's Plunge choice and the angle it gives the engine
//! (`mitcad_cam::turning::plunge`).
//!
//! Turn Roughing and Turn Finishing bridge every undercut (a wall falling
//! towards the chuck) by default. With **Tool geometry** the finish goes
//! down walls as steep as the tool's back angle less
//! [`PLUNGE_CLEARANCE`], so that the trailing edge clears the wall it has
//! cut, and leaves the rest bridged; with **Free** it goes down every
//! wall, the user making sure the tool reaches. Either way the finish takes
//! the undercut in one pass, no deeper than the insert's depth of cut
//! unless a roughing before it leaves less there.
//!
//! Turn Roughing has the same choice (mitcad#2549, GitHub
//! mitcad/Mitcad#733): with Tool geometry or Free its passes go into the
//! undercuts pocket by pocket, written as moves.

use serde::{Deserialize, Serialize};

use super::CamTool;

/// How Turn Finishing and Turn Roughing go into undercuts: walls that
/// fall towards the chuck.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Plunge {
    /// Not at all: every undercut is bridged and left, as roughing leaves
    /// it (files before mitcad#2503 have it).
    #[default]
    None,
    /// Down walls as steep as the tool's back angle less
    /// [`PLUNGE_CLEARANCE`]; the rest is left. The tool needs its insert
    /// and holder.
    Tool,
    /// Down every wall: the user makes sure the tool reaches them.
    Free,
}

/// How much less steep than the tool's back angle Tool geometry goes down
/// a wall (degrees), so that the trailing edge clears it.
pub const PLUNGE_CLEARANCE: f64 = 2.0;

impl Plunge {
    pub fn is_none(&self) -> bool {
        *self == Plunge::None
    }

    /// The steepest wall falling towards the chuck a finish or a roughing
    /// with `tool` goes down (degrees from the Z axis): 0 bridges every undercut, 90
    /// follows every wall; or what the tool lacks, in words.
    pub fn angle(self, tool: &CamTool) -> Result<f64, String> {
        Ok(match self {
            Plunge::None => 0.0,
            Plunge::Free => 90.0,
            Plunge::Tool => {
                let back = tool.back_angle().ok_or_else(|| {
                    format!(
                        "Plunge by Tool geometry needs the insert and holder of tool {}: give them in Machines and Tools (Edit Tool), or choose Free or Don't plunge.",
                        tool.number
                    )
                })?;
                (back - PLUNGE_CLEARANCE).max(0.0)
            }
        })
    }
}

impl CamTool {
    /// The back angle of a turning tool's or boring bar's insert in its
    /// holder (degrees from the Z axis): the steepest wall falling towards
    /// the chuck its trailing edge clears. It comes from the insert and
    /// holder geometry a tool keeps (mitcad#2502); a tool without it has
    /// none. An insert whose edge leans forward of square (a negative back
    /// angle) follows no wall down: 0; one past 89 degrees is taken as 89.
    pub fn back_angle(&self) -> Option<f64> {
        self.insert
            .as_ref()
            .map(|g| g.back_angle())
            .filter(|b| b.is_finite())
            .map(|b| b.clamp(0.0, 89.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::ToolKind;

    fn turning() -> CamTool {
        serde_json::from_value(serde_json::json!({
            "number": 3, "name": "Finishing", "shape": {"kind": "turning", "nose_radius": 0.4},
            "diameter": 20, "flute_length": 2, "length": 30
        }))
        .unwrap()
    }

    #[test]
    fn the_modes_give_their_angles() {
        let tool = turning();
        assert!(matches!(tool.shape, ToolKind::Turning { .. }));
        assert_eq!(Plunge::None.angle(&tool), Ok(0.0));
        assert_eq!(Plunge::Free.angle(&tool), Ok(90.0));
        // Without its insert and holder the tool has no back angle.
        assert_eq!(tool.back_angle(), None);
        assert_eq!(
            Plunge::Tool.angle(&tool),
            Err("Plunge by Tool geometry needs the insert and holder of tool 3: give them in Machines and Tools (Edit Tool), or choose Free or Don't plunge.".to_owned())
        );
        // A VNMG in an SVJBR (mitcad#2502): a back angle of 52 degrees,
        // down walls of 50 with the clearance.
        let read = crate::cam::engine::turning::InsertGeometry::read(
            "VNMG 160404",
            "SVJBR 2525 M16",
            Default::default(),
        )
        .unwrap();
        let tool = CamTool {
            insert: Some(read.geometry),
            ..turning()
        };
        let back = tool.back_angle().unwrap();
        assert!((back - 52.0).abs() < 1e-9, "{back}");
        let angle = Plunge::Tool.angle(&tool).unwrap();
        assert!((angle - 50.0).abs() < 1e-9, "{angle}");
        // The engine's tool draws its trailing edge at it.
        let engine = tool.turning_engine().unwrap();
        assert_eq!(engine.back_angle, Some(back));
        assert_eq!(Plunge::default(), Plunge::None);
        assert_eq!(serde_json::to_value(Plunge::Tool).unwrap(), "tool");
        assert_eq!(
            serde_json::from_value::<Plunge>(serde_json::json!("free")).unwrap(),
            Plunge::Free
        );
    }
}
