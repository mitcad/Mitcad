// SPDX-License-Identifier: MIT
//! The turning operations: facing, roughing and finishing of the outside
//! and the bore, grooving, parting off and threading. Each makes the
//! controlled point's moves ([`TurnPath`]) from the part's profile and
//! the stock, and records the canned cycle it stands for where there is
//! one (G71 roughing, G70 finishing, G76 threading).

use serde::{Deserialize, Serialize};

use super::plunge::{depth_under, reach};
use super::profile::{PV, Piece, chain_moves, envelope, first_reach, piece_at, pieces, tool_path};
use super::tool::{TurningTool, TurningToolKind};
use super::{PXZ, TurnCycle, TurnMove, TurnPart, TurnPath};
use crate::CamError;

/// Which side of the part an operation cuts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnSide {
    /// The outside diameters.
    #[default]
    Outside,
    /// The bore.
    Inside,
}

impl TurnSide {
    fn inside(self) -> bool {
        self == TurnSide::Inside
    }

    /// +1 outside, -1 inside: the direction away from the material.
    fn sign(self) -> f64 {
        if self.inside() { -1.0 } else { 1.0 }
    }
}

fn err(text: impl Into<String>) -> CamError {
    CamError::new(text)
}

fn positive(v: f64) -> bool {
    v.is_finite() && v > 0.0
}

/// Facing the part's front from the stock's face down to the part's, in
/// passes of at most `step`, from outside the stock to the centre (or into
/// the stock's bore).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FaceOp {
    pub step: f64,
    pub clearance: f64,
    /// mm/rev.
    pub feed: f64,
}

pub fn face(part: &TurnPart, op: &FaceOp, tool: &TurningTool) -> Result<TurnPath, CamError> {
    part.validate().map_err(err)?;
    if tool.kind != TurningToolKind::Turning {
        return Err(err("Facing needs a turning tool."));
    }
    if !(positive(op.step) && positive(op.clearance) && positive(op.feed)) {
        return Err(err(
            "Facing needs a step, a clearance and a feed above zero.",
        ));
    }
    let r = tool.nose_radius;
    let face_z = part.face_z();
    let total = (part.stock_face - face_z).max(0.0);
    let n = ((total / op.step - 1e-9).ceil() as usize).max(1);
    let x_start = part.stock_radius + op.clearance;
    let x_end = if part.stock_inner_radius > 0.0 {
        part.stock_inner_radius - op.clearance.min(part.stock_inner_radius)
    } else {
        // The nose's centre over the axis: the face is cut to its centre.
        -r
    };
    let z_start = part.stock_face + op.clearance;
    let mut moves = vec![TurnMove::Rapid {
        to: [x_start, z_start],
    }];
    for k in 1..=n {
        let z = part.stock_face - total * k as f64 / n as f64;
        moves.push(TurnMove::Rapid { to: [x_start, z] });
        moves.push(TurnMove::Line {
            to: [x_end, z],
            feed: op.feed,
        });
        moves.push(TurnMove::Rapid {
            to: [x_end, z + op.clearance],
        });
        moves.push(TurnMove::Rapid {
            to: [x_start, z + op.clearance],
        });
    }
    moves.push(TurnMove::Rapid {
        to: [x_start, z_start],
    });
    Ok(TurnPath { moves, cycle: None })
}

/// The side's chain from where it leaves the face: the vertices on the
/// face before it are the facing's.
fn side_chain(part: &TurnPart, side: TurnSide) -> Result<Vec<PV>, CamError> {
    let chain = match side {
        TurnSide::Outside => &part.outside,
        TurnSide::Inside => &part.inside,
    };
    if chain.len() < 2 {
        return Err(err(match side {
            TurnSide::Outside => "The part has no outside profile to turn.",
            TurnSide::Inside => "The part has no bore to turn.",
        }));
    }
    let face_z = chain[0].z;
    let first = chain
        .iter()
        .take_while(|v| (v.z - face_z).abs() < 1e-9)
        .count()
        .saturating_sub(1)
        .min(chain.len() - 2);
    Ok(chain[first..].to_vec())
}

/// Checks that the tool cuts the side.
fn side_tool(side: TurnSide, tool: &TurningTool, what: &str) -> Result<(), CamError> {
    let ok = matches!(
        (side, tool.kind),
        (TurnSide::Outside, TurningToolKind::Turning)
            | (TurnSide::Inside, TurningToolKind::Boring { .. })
    );
    if ok {
        Ok(())
    } else {
        Err(err(match side {
            TurnSide::Outside => format!("{what} of the outside needs a turning tool."),
            TurnSide::Inside => format!("{what} of the bore needs a boring bar."),
        }))
    }
}

/// Where a side's cycles start (`[x, z]`): outside the stock, or inside
/// its bore, in front of its face.
fn cycle_start(part: &TurnPart, side: TurnSide, clearance: f64) -> Result<PXZ, CamError> {
    let z = part.stock_face.max(part.face_z()) + clearance;
    match side {
        TurnSide::Outside => Ok([part.stock_radius + clearance, z]),
        TurnSide::Inside => {
            if part.stock_inner_radius <= 0.0 {
                return Err(err(
                    "The stock has no hole for the boring bar: drill it first or give the stock a bore.",
                ));
            }
            Ok([(part.stock_inner_radius - clearance).max(0.0), z])
        }
    }
}

/// The finished contour of a side on the part, before the tool: the
/// envelope, or with a `plunge` angle above zero the contour that goes
/// down walls falling towards the chuck no steeper than that
/// ([`reach`], mitcad#2503); in the bore up to where it meets the
/// stock's own bore.
fn side_contour(part: &TurnPart, side: TurnSide, plunge: f64) -> Result<Vec<PV>, CamError> {
    let profile = side_chain(part, side)?;
    let mut chain = if plunge > 0.0 {
        reach(&profile, side.inside(), plunge)
    } else {
        envelope(&profile, side.inside())
    };
    // In the bore, the contour ends where it meets the stock's own bore.
    if side == TurnSide::Inside
        && let Some(i) = chain
            .iter()
            .position(|v| v.x <= part.stock_inner_radius + 1e-9)
        && i >= 1
    {
        chain.truncate(i + 1);
        chain[i].bulge = 0.0;
    }
    Ok(chain)
}

/// The finished contour of a side for the tool (controlled point), with
/// its way out of the material, and the profile a G71 or G70 cycle runs:
/// a rapid in X alone from the start, then into the contour. Roughing
/// without a plunge bridges every undercut (`plunge` 0); finishing, and
/// roughing that plunges (mitcad#2549), go into them as the plunge angle
/// allows. `offset` leaves that much stock all round, a true offset of
/// the contour (roughing into undercuts); 0 for the contour itself.
fn finish_contour(
    part: &TurnPart,
    side: TurnSide,
    tool: &TurningTool,
    start: PXZ,
    clearance: f64,
    plunge: f64,
    offset: f64,
) -> Result<Vec<PV>, CamError> {
    let mut chain = side_contour(part, side, plunge)?;
    // A contour that ends along Z goes on by the nose radius, so that the
    // nose clears the corner where the part ends (the stock beyond is
    // parted off or held).
    let n = chain.len();
    if tool.nose_radius > 0.0
        && n >= 2
        && chain[n - 2].bulge == 0.0
        && (chain[n - 2].x - chain[n - 1].x).abs() < 1e-9
        && chain[n - 1].z < chain[n - 2].z
    {
        chain[n - 1].z -= tool.nose_radius;
    }
    let last = *chain.last().expect("a side's chain has two vertices");
    let out_x = match side {
        TurnSide::Outside => part.stock_radius + clearance,
        TurnSide::Inside => start[0],
    };
    if (last.x - out_x).abs() > 1e-9 {
        chain.push(PV::new(out_x, last.z));
    }
    // With an offset: the path of a nose bigger by the offset, moved to
    // the real nose's tip.
    let path = tool_path(
        &chain,
        side.inside(),
        tool.nose_radius + offset,
        offset,
        offset,
    );
    if path.len() < 2 {
        return Err(err("The part's profile is too short to turn."));
    }
    if let TurningToolKind::Boring { bar_diameter } = tool.kind
        && bar_diameter >= 2.0 * part.stock_inner_radius
    {
        return Err(err("The boring bar is too thick for the stock's bore."));
    }
    Ok(path)
}

/// The cycle profile of a contour: a rapid in X alone, a feed in Z to the
/// contour's start, then the contour.
fn cycle_profile(contour: &[PV], start: PXZ, feed: f64) -> Vec<TurnMove> {
    let first = contour[0];
    let mut out = vec![TurnMove::Rapid {
        to: [first.x, start[1]],
    }];
    out.push(TurnMove::Line {
        to: [first.x, first.z],
        feed,
    });
    out.extend(chain_moves(contour, feed));
    out
}

/// Roughing a side in passes along Z of at most `depth` (radius), each
/// backing off `retract` at 45 degrees, then once along the contour,
/// leaving `allowance_x` (radius) and `allowance_z` for finishing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoughOp {
    pub side: TurnSide,
    pub depth: f64,
    pub retract: f64,
    pub allowance_x: f64,
    pub allowance_z: f64,
    pub clearance: f64,
    pub feed: f64,
    /// Into undercuts as [`FinishOp::plunge`] (mitcad#2549, degrees from
    /// the Z axis): 0 bridges every undercut (a G71); above zero the
    /// passes go down into every pocket the contour at that angle has,
    /// pocket by pocket, and the stock left is the larger of the two
    /// allowances all round (a true offset of the contour), written as
    /// moves.
    #[serde(default)]
    pub plunge: f64,
}

pub fn rough(part: &TurnPart, op: &RoughOp, tool: &TurningTool) -> Result<TurnPath, CamError> {
    part.validate().map_err(err)?;
    side_tool(op.side, tool, "Roughing")?;
    if !(positive(op.depth) && positive(op.retract) && positive(op.clearance) && positive(op.feed))
    {
        return Err(err(
            "Roughing needs a depth of cut, a retract, a clearance and a feed above zero.",
        ));
    }
    if !(op.allowance_x >= 0.0 && op.allowance_z >= 0.0) {
        return Err(err("The stock to leave must be zero or more."));
    }
    if op.depth > tool.max_depth {
        return Err(err("The depth of cut is deeper than the insert can cut."));
    }
    if !(op.plunge.is_finite() && (0.0..=90.0).contains(&op.plunge)) {
        return Err(err("The plunge angle must be from 0 to 90 degrees."));
    }
    let s = op.side.sign();
    let start = cycle_start(part, op.side, op.clearance)?;
    if op.plunge > 0.0 {
        return rough_pockets(part, op, tool, start);
    }
    let finish = finish_contour(part, op.side, tool, start, op.clearance, 0.0, 0.0)?;
    let contour: Vec<PV> = finish
        .iter()
        .map(|v| PV {
            x: v.x + s * op.allowance_x,
            z: v.z + op.allowance_z,
            bulge: v.bulge,
        })
        .collect();
    // The deepest the passes go: the contour's start (outside its lowest
    // point, inside its widest).
    let floor = contour[0].x;
    let surface = match op.side {
        TurnSide::Outside => part.stock_radius,
        TurnSide::Inside => part.stock_inner_radius,
    };
    let mut moves = vec![TurnMove::Rapid { to: start }];
    let mut k = 1;
    loop {
        let level = surface - s * op.depth * k as f64;
        if s * (level - floor) <= 1e-9 {
            break;
        }
        let Some(z_hit) = first_reach(&contour, level) else {
            break;
        };
        if z_hit < start[1] - 1e-9 {
            moves.push(TurnMove::Rapid {
                to: [level, start[1]],
            });
            moves.push(TurnMove::Line {
                to: [level, z_hit],
                feed: op.feed,
            });
            let back = [level + s * op.retract, z_hit + op.retract];
            moves.push(TurnMove::Line {
                to: back,
                feed: op.feed,
            });
            moves.push(TurnMove::Rapid {
                to: [back[0], start[1]],
            });
        }
        k += 1;
        if k > 10_000 {
            return Err(err("Roughing needs too many passes: take deeper cuts."));
        }
    }
    // Once along the contour, then back to the start.
    let along = cycle_profile(&contour, start, op.feed);
    moves.extend(along);
    let end = moves.last().map(TurnMove::to).unwrap_or(start);
    moves.push(TurnMove::Rapid {
        to: [end[0], start[1]],
    });
    moves.push(TurnMove::Rapid { to: start });
    let count = moves.len() - 1;
    Ok(TurnPath {
        moves,
        cycle: Some(TurnCycle::Rough {
            first: 1,
            count,
            start,
            depth: op.depth,
            retract: op.retract,
            allowance_x: op.allowance_x,
            allowance_z: op.allowance_z,
            feed: op.feed,
            profile: cycle_profile(&finish, start, op.feed),
        }),
    })
}

/// Finishing a side once along its contour.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FinishOp {
    pub side: TurnSide,
    pub clearance: f64,
    pub feed: f64,
    /// How steep a wall falling towards the chuck the contour goes down
    /// (degrees from the Z axis, mitcad#2503): 0 bridges every undercut
    /// (as roughing without a plunge does), 90 follows every wall.
    #[serde(default)]
    pub plunge: f64,
    /// The roughings before the finish leave no more in its undercuts
    /// than the insert cuts in one pass ([`too_deep_after`] found none
    /// too deep, mitcad#2549): an undercut deeper than the insert's depth
    /// of cut is not refused then.
    #[serde(default)]
    pub roughed: bool,
}

pub fn finish(part: &TurnPart, op: &FinishOp, tool: &TurningTool) -> Result<TurnPath, CamError> {
    part.validate().map_err(err)?;
    side_tool(op.side, tool, "Finishing")?;
    if !(positive(op.clearance) && positive(op.feed)) {
        return Err(err("Finishing needs a clearance and a feed above zero."));
    }
    if !(op.plunge.is_finite() && (0.0..=90.0).contains(&op.plunge)) {
        return Err(err("The plunge angle must be from 0 to 90 degrees."));
    }
    let start = cycle_start(part, op.side, op.clearance)?;
    // Into an undercut the finish takes it all in one pass (roughing
    // without a plunge bridges it): no deeper than the insert cuts, unless
    // the roughings before it leave no more than that (mitcad#2549).
    if !op.roughed
        && let Some(deep) = too_deep(part, op, tool)?
    {
        return Err(err(deep.message()));
    }
    let contour = finish_contour(part, op.side, tool, start, op.clearance, op.plunge, 0.0)?;
    let profile = cycle_profile(&contour, start, op.feed);
    let mut moves = vec![TurnMove::Rapid { to: start }];
    moves.extend(profile.iter().copied());
    let end = moves.last().map(TurnMove::to).unwrap_or(start);
    moves.push(TurnMove::Rapid {
        to: [end[0], start[1]],
    });
    moves.push(TurnMove::Rapid { to: start });
    let count = moves.len() - 1;
    Ok(TurnPath {
        moves,
        cycle: Some(TurnCycle::Finish {
            first: 1,
            count,
            start,
            profile,
        }),
    })
}

/// An undercut a plunging finish would take deeper than its insert cuts
/// (mitcad#2503): how deep it goes under the bridged contour (radius),
/// the insert's depth of cut, and the deepest point of the contour (for
/// the places a refusal is about; never in its message). With `roughed`
/// (mitcad#2549) the depth is what the roughings before the finish leave
/// there at the least ([`too_deep_after`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TooDeep {
    pub depth: f64,
    pub limit: f64,
    pub at: PXZ,
    pub roughed: bool,
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0 + 0.0
}

impl TooDeep {
    /// What a person reads: how deep, by how much too deep and the
    /// insert's limit, as lengths, and what to do; no place.
    pub fn message(&self) -> String {
        if self.roughed {
            format!(
                "After the roughing before it, {} mm (radius) is left in the undercut, {} mm more than the insert's depth of cut ({} mm): let the roughing plunge as far as the finish or leave less stock, or finish it with Don't plunge.",
                round3(self.depth),
                round3(self.depth - self.limit),
                round3(self.limit)
            )
        } else {
            format!(
                "The undercut is {} mm deep (radius), {} mm deeper than the insert's depth of cut ({} mm): rough it first with a Turn Roughing that plunges, finish it with Don't plunge, or with an insert that cuts that deep.",
                round3(self.depth),
                round3(self.depth - self.limit),
                round3(self.limit)
            )
        }
    }

    /// What agents and the log read: the same with where it is deepest
    /// (work Z).
    pub fn detail(&self) -> String {
        if self.roughed {
            format!(
                "After the roughing before it, the undercut at Z {} has {} mm (radius) left, {} mm more than the insert's depth of cut ({} mm).",
                round3(self.at[1]),
                round3(self.depth),
                round3(self.depth - self.limit),
                round3(self.limit)
            )
        } else {
            format!(
                "The undercut at Z {} is {} mm deep (radius), {} mm deeper than the insert's depth of cut ({} mm).",
                round3(self.at[1]),
                round3(self.depth),
                round3(self.depth - self.limit),
                round3(self.limit)
            )
        }
    }
}

/// Whether a finish plunging into undercuts goes deeper under the bridged
/// contour than the tool's insert cuts in one pass, and where (none for
/// Don't plunge or an undercut within the insert's depth of cut).
pub fn too_deep(
    part: &TurnPart,
    op: &FinishOp,
    tool: &TurningTool,
) -> Result<Option<TooDeep>, CamError> {
    if op.plunge <= 0.0 {
        return Ok(None);
    }
    let bridged = side_contour(part, op.side, 0.0)?;
    let reached = side_contour(part, op.side, op.plunge)?;
    let (depth, at) = depth_under(&bridged, &reached, op.side.inside());
    Ok((depth > tool.max_depth + 1e-6).then_some(TooDeep {
        depth,
        limit: tool.max_depth,
        at,
        roughed: false,
    }))
}

/// [`too_deep`] after the roughings before the finish (mitcad#2549): the
/// roughing operations and their tools, of any side and in any state;
/// those of the finish's side that generate count. A roughing plunging at
/// `angle` leaves, at a point of the finish's contour, the depth from it
/// up to the roughing's own contour at that angle (along X, as
/// [`too_deep`] measures it; zero where the roughing went at least as
/// far), plus the larger of its two allowances (the stock it leaves all
/// round), plus by how much its nose is bigger than the finishing
/// insert's (what it leaves in the inside corners). The least any of them
/// leaves counts; when that fits the finishing insert's depth of cut the
/// finish is not too deep (none), else it is, the depth being what is
/// left. Without a roughing that counts: [`too_deep`]'s answer.
pub fn too_deep_after(
    part: &TurnPart,
    op: &FinishOp,
    tool: &TurningTool,
    roughings: &[(RoughOp, TurningTool)],
) -> Result<Option<TooDeep>, CamError> {
    let Some(deep) = too_deep(part, op, tool)? else {
        return Ok(None);
    };
    let reached = side_contour(part, op.side, op.plunge)?;
    let mut least: Option<TooDeep> = None;
    for (rough_op, rough_tool) in roughings {
        // Another side's, or one that fails: nothing is known of what it
        // leaves.
        if rough_op.side != op.side || rough(part, rough_op, rough_tool).is_err() {
            continue;
        }
        let roughed = side_contour(part, op.side, rough_op.plunge)?;
        let (under, at) = depth_under(&roughed, &reached, op.side.inside());
        let left = under
            + rough_op.allowance_x.max(rough_op.allowance_z)
            + (rough_tool.nose_radius - tool.nose_radius).max(0.0);
        if least.is_none_or(|l| left < l.depth) {
            least = Some(TooDeep {
                depth: left,
                limit: tool.max_depth,
                at: if under > 1e-9 { at } else { deep.at },
                roughed: true,
            });
        }
    }
    Ok(match least {
        Some(left) if left.depth <= tool.max_depth + 1e-6 => None,
        Some(left) => Some(left),
        None => Some(deep),
    })
}

/// Roughing into undercuts (mitcad#2549): levels `depth` apart from the
/// stock's surface down to the contour's deepest point, each cutting
/// along Z wherever the contour is below it. The contour is the one at
/// the plunge angle, offset all round by the larger of the two
/// allowances (a true offset: a shift by the allowances would leave less
/// than either on a wall falling towards the chuck). A pass in a pocket
/// starts with a plunge in X where the contour falls through its level,
/// down a wall no steeper than the plunge angle; the passes go depth
/// first, each pocket to its bottom before the next, travelling only over
/// stock already cut and backing off at 45 degrees no further than half
/// the pass (clear of a pocket's far wall). Then once along the contour
/// and back to the start. Written as moves: a G71 cannot plunge.
fn rough_pockets(
    part: &TurnPart,
    op: &RoughOp,
    tool: &TurningTool,
    start: PXZ,
) -> Result<TurnPath, CamError> {
    let s = op.side.sign();
    let leave = op.allowance_x.max(op.allowance_z);
    let contour = finish_contour(part, op.side, tool, start, op.clearance, op.plunge, leave)?;
    let surface = match op.side {
        TurnSide::Outside => part.stock_radius,
        TurnSide::Inside => part.stock_inner_radius,
    };
    // In front of the face the contour is open to its start's X.
    let mut chain = vec![PV::new(contour[0].x, start[1])];
    chain.extend(contour.iter().copied());
    let ps = pieces(&chain);
    // Pieces are monotone, so the deepest point is at an end of one.
    let deepest = ps
        .iter()
        .flat_map(|p| [p.a[0], p.b[0]])
        .fold(f64::INFINITY, |m, x| m.min(s * x))
        * s;
    let mut levels: Vec<(f64, Vec<[f64; 2]>)> = Vec::new();
    loop {
        let level = surface - s * op.depth * (levels.len() + 1) as f64;
        if s * (level - deepest) <= 1e-9 {
            break;
        }
        levels.push((level, spans_below(&ps, level, s)));
        if levels.len() > 10_000 {
            return Err(err("Roughing needs too many passes: take deeper cuts."));
        }
    }
    let mut cut = Pockets {
        levels: &levels,
        s,
        face: start[1],
        retract: op.retract,
        feed: op.feed,
        moves: vec![TurnMove::Rapid { to: start }],
    };
    if let Some((_, top)) = levels.first() {
        for &span in top {
            cut.visit(0, span, start[0], start[0]);
        }
    }
    let mut moves = cut.moves;
    // Out over the cut stock, back in front of the face, once along the
    // contour, then back to the start.
    let at = moves.last().map(TurnMove::to).unwrap_or(start);
    for to in [[start[0], at[1]], [start[0], start[1]]] {
        if moves.last().map(TurnMove::to) != Some(to) {
            moves.push(TurnMove::Rapid { to });
        }
    }
    moves.extend(cycle_profile(&contour, start, op.feed));
    let end = moves.last().map(TurnMove::to).unwrap_or(start);
    moves.push(TurnMove::Rapid {
        to: [end[0], start[1]],
    });
    moves.push(TurnMove::Rapid { to: start });
    Ok(TurnPath { moves, cycle: None })
}

/// Where along Z (`[from, to]`, `from` towards the face) a chain is below
/// `level` (past it into the material, `s` the side's sign).
fn spans_below(ps: &[Piece], level: f64, s: f64) -> Vec<[f64; 2]> {
    let below = |x: f64| s * (x - level) < -1e-9;
    let mut out = Vec::new();
    let mut from: Option<f64> = None;
    for (i, p) in ps.iter().enumerate() {
        let (a, b) = (below(p.a[0]), below(p.b[0]));
        if i == 0 && a {
            from = Some(p.a[1]);
        }
        if a != b {
            let z = piece_at(p, 0, level).map_or(if b { p.a[1] } else { p.b[1] }, |q| q[1]);
            if b {
                from = Some(z);
            } else if let Some(f) = from.take() {
                out.push([f, z]);
            }
        }
    }
    if let (Some(f), Some(p)) = (from, ps.last()) {
        out.push([f, p.b[1]]);
    }
    out.retain(|[f, t]| f - t > 1e-6);
    out
}

/// The passes of [`rough_pockets`], depth first.
struct Pockets<'a> {
    levels: &'a [(f64, Vec<[f64; 2]>)],
    s: f64,
    /// The Z in front of the face the open passes start from.
    face: f64,
    retract: f64,
    feed: f64,
    moves: Vec<TurnMove>,
}

impl Pockets<'_> {
    fn rapid(&mut self, to: PXZ) {
        let at = self.moves.last().map(TurnMove::to);
        if at.is_none_or(|a| (a[0] - to[0]).abs() > 1e-9 || (a[1] - to[1]).abs() > 1e-9) {
            self.moves.push(TurnMove::Rapid { to });
        }
    }

    fn line(&mut self, to: PXZ) {
        self.moves.push(TurnMove::Line {
            to,
            feed: self.feed,
        });
    }

    /// The pass along `span` of level `k`, then the passes below it.
    /// `travel` is an X over cut stock all along the span (the level
    /// above's after its retract), `above` the level above's X (where its
    /// pass cut down to).
    fn visit(&mut self, k: usize, [from, to]: [f64; 2], travel: f64, above: f64) {
        let level = self.levels[k].0;
        let s = self.s;
        let at = self
            .moves
            .last()
            .map(TurnMove::to)
            .unwrap_or([travel, from]);
        self.rapid([travel, at[1]]);
        self.rapid([travel, from]);
        if (from - self.face).abs() < 1e-9 {
            // Open to the face: in at the level in front of it.
            self.rapid([level, from]);
        } else {
            // In a pocket: down to the level where the contour falls
            // through it.
            self.rapid([above, from]);
            self.line([level, from]);
        }
        self.line([level, to]);
        // Back off at 45 degrees, no further than half the span (a
        // pocket's far wall).
        let r = self.retract.min((from - to) / 2.0);
        let back = [level + s * r, to + r];
        self.line(back);
        if let Some((_, below)) = self.levels.get(k + 1) {
            let inner: Vec<[f64; 2]> = below
                .iter()
                .copied()
                .filter(|[f, t]| *f <= from + 1e-9 && *t >= to - 1e-9)
                .collect();
            for span in inner {
                self.visit(k + 1, span, back[0], level);
            }
        }
    }
}

/// The extreme X of a chain between two Z: the smallest when `min`.
fn extreme_x(chain: &[PV], z_lo: f64, z_hi: f64, min: bool) -> Option<f64> {
    let mut best: Option<f64> = None;
    for p in pieces(chain) {
        let mut pts = Vec::new();
        for q in [p.a, p.b] {
            if q[1] >= z_lo - 1e-9 && q[1] <= z_hi + 1e-9 {
                pts.push(q);
            }
        }
        for z in [z_lo, z_hi] {
            if let Some(q) = piece_at(&p, 1, z) {
                pts.push(q);
            }
        }
        for q in pts {
            best = Some(match best {
                None => q[0],
                Some(b) if min => b.min(q[0]),
                Some(b) => b.max(q[0]),
            });
        }
    }
    best
}

/// Plunges from `top` to `bottom` (radius) at `z`, in pecks of `peck`
/// (0: in one go), backing out to `top` after each; the tool comes and
/// goes at `travel` (X).
fn plunge(
    moves: &mut Vec<TurnMove>,
    travel: f64,
    [top, bottom]: [f64; 2],
    z: f64,
    peck: f64,
    feed: f64,
) {
    moves.push(TurnMove::Rapid { to: [travel, z] });
    if (travel - top).abs() > 1e-9 {
        moves.push(TurnMove::Rapid { to: [top, z] });
    }
    let s = if bottom <= top { -1.0 } else { 1.0 };
    let depth = (bottom - top).abs();
    if peck > 0.0 && peck < depth {
        let mut done = 0.0;
        loop {
            let next = (done + peck).min(depth);
            moves.push(TurnMove::Line {
                to: [top + s * next, z],
                feed,
            });
            if next >= depth - 1e-9 {
                break;
            }
            moves.push(TurnMove::Rapid { to: [top, z] });
            // Back down to just short of where the last peck ended.
            let again = (next - 0.2).max(0.0);
            if again > 0.0 {
                moves.push(TurnMove::Rapid {
                    to: [top + s * again, z],
                });
            }
            done = next;
        }
    } else {
        moves.push(TurnMove::Line {
            to: [bottom, z],
            feed,
        });
    }
    moves.push(TurnMove::Rapid { to: [top, z] });
    if (travel - top).abs() > 1e-9 {
        moves.push(TurnMove::Rapid { to: [travel, z] });
    }
}

/// Grooving between two Z (`z_from` towards the face, `z_to` towards the
/// chuck) down to the profile's bottom there, in plunges as wide as the
/// insert. The controlled point is the insert's corner towards the chuck;
/// the insert's corner radius rounds the groove's corners.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GrooveOp {
    pub side: TurnSide,
    pub z_from: f64,
    pub z_to: f64,
    /// Peck depth (radius); 0 plunges in one go.
    #[serde(default)]
    pub peck: f64,
    pub clearance: f64,
    pub feed: f64,
}

pub fn groove(part: &TurnPart, op: &GrooveOp, tool: &TurningTool) -> Result<TurnPath, CamError> {
    part.validate().map_err(err)?;
    let TurningToolKind::Grooving { width, inside } = tool.kind else {
        return Err(err("Grooving needs a grooving tool."));
    };
    if inside != op.side.inside() {
        return Err(err(match op.side {
            TurnSide::Outside => "Grooving the outside needs an outside grooving tool.",
            TurnSide::Inside => "Grooving the bore needs an inside grooving tool.",
        }));
    }
    if !(positive(op.clearance) && positive(op.feed) && op.peck >= 0.0) {
        return Err(err("Grooving needs a clearance and a feed above zero."));
    }
    let (z_lo, z_hi) = (op.z_from.min(op.z_to), op.z_from.max(op.z_to));
    if z_hi - z_lo < width - 1e-9 {
        return Err(err("The groove is narrower than the grooving insert."));
    }
    let chain = match op.side {
        TurnSide::Outside => &part.outside,
        TurnSide::Inside => &part.inside,
    };
    let inner = op.side.inside();
    let (Some(bottom), Some(top)) = (
        extreme_x(chain, z_lo, z_hi, !inner),
        extreme_x(chain, z_lo - 1e-6, z_hi + 1e-6, inner),
    ) else {
        return Err(err("The groove is not on the part's profile."));
    };
    let s = op.side.sign();
    let depth = s * (top - bottom);
    if depth > tool.max_depth + 1e-9 {
        return Err(err(
            "The groove is deeper than the grooving insert reaches.",
        ));
    }
    if depth <= 1e-9 {
        return Err(err("The part has no groove there."));
    }
    let above = top + s * op.clearance;
    // Outside, the tool comes along clear of the stock as it may still be.
    let travel = match op.side {
        TurnSide::Outside => above.max(part.stock_radius + op.clearance),
        TurnSide::Inside => above,
    };
    // The plunges: from the face's side towards the chuck, at most 80 % of
    // the insert's width apart.
    let span = z_hi - z_lo - width;
    let n = ((span / (0.8 * width) - 1e-9).ceil().max(0.0) as usize) + 1;
    let mut moves = vec![TurnMove::Rapid {
        to: [travel, part.stock_face.max(part.face_z()) + op.clearance],
    }];
    for k in 0..n {
        let z = if n == 1 {
            z_hi - width
        } else {
            z_hi - width - span * k as f64 / (n - 1) as f64
        };
        plunge(&mut moves, travel, [above, bottom], z, op.peck, op.feed);
    }
    Ok(TurnPath { moves, cycle: None })
}

/// Parting the part off at its back: the blade's side towards the face
/// cuts at `z` (the part's back when left out), down to `to_x` (radius;
/// 0 or a little past the centre).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PartOp {
    #[serde(default)]
    pub z: Option<f64>,
    #[serde(default)]
    pub to_x: f64,
    #[serde(default)]
    pub peck: f64,
    pub clearance: f64,
    pub feed: f64,
}

pub fn part_off(part: &TurnPart, op: &PartOp, tool: &TurningTool) -> Result<TurnPath, CamError> {
    part.validate().map_err(err)?;
    let TurningToolKind::Parting { width } = tool.kind else {
        return Err(err("Parting off needs a parting tool."));
    };
    if !(positive(op.clearance) && positive(op.feed) && op.peck >= 0.0) {
        return Err(err("Parting off needs a clearance and a feed above zero."));
    }
    if !(op.to_x.is_finite() && op.to_x < part.stock_radius) {
        return Err(err("Parting off must end inside the stock."));
    }
    if part.stock_radius - op.to_x > tool.max_depth + 1e-9 {
        return Err(err(
            "The parting blade does not reach the centre of the stock.",
        ));
    }
    let z = op.z.unwrap_or_else(|| part.back_z()) - width;
    let above = part.stock_radius + op.clearance;
    let mut moves = vec![TurnMove::Rapid {
        to: [above, part.stock_face.max(part.face_z()) + op.clearance],
    }];
    plunge(&mut moves, above, [above, op.to_x], z, op.peck, op.feed);
    Ok(TurnPath { moves, cycle: None })
}

fn three() -> f64 {
    3.0
}

fn one() -> u32 {
    1
}

/// Threading from `z_start` (towards the face) to `z_end` at `lead` per
/// revolution, `height` deep (radius) from `crest` (the radius the thread
/// starts at: the major radius outside, the minor inside). Passes get
/// shallower as they go (the n-th pass reaches `first_depth` times the
/// square root of n), none under `min_depth`, the last
/// `finish_allowance` left for the finishing passes, `spring_passes` of
/// them (at least one) at the full depth. The tool starts `run_in` leads before the thread.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThreadOp {
    pub side: TurnSide,
    pub crest: f64,
    pub z_start: f64,
    pub z_end: f64,
    pub lead: f64,
    pub height: f64,
    pub first_depth: f64,
    pub min_depth: f64,
    #[serde(default)]
    pub finish_allowance: f64,
    #[serde(default = "one")]
    pub spring_passes: u32,
    #[serde(default = "three")]
    pub run_in: f64,
    pub clearance: f64,
}

impl ThreadOp {
    /// The depths of the passes (radius, from the crest).
    pub fn depths(&self) -> Vec<f64> {
        let mut out = Vec::new();
        let rough_to = self.height - self.finish_allowance;
        let mut done = 0.0;
        let mut n = 1.0_f64;
        while done < rough_to - 1e-9 {
            let next = (self.first_depth * n.sqrt())
                .max(done + self.min_depth)
                .min(rough_to);
            out.push(next);
            done = next;
            n += 1.0;
            if out.len() > 500 {
                break;
            }
        }
        for _ in 0..self.spring_passes.max(1) {
            out.push(self.height);
        }
        out
    }
}

pub fn thread(op: &ThreadOp, tool: &TurningTool) -> Result<TurnPath, CamError> {
    let TurningToolKind::Threading { angle, inside } = tool.kind else {
        return Err(err("Threading needs a threading tool."));
    };
    if inside != op.side.inside() {
        return Err(err(match op.side {
            TurnSide::Outside => "An outside thread needs an outside threading tool.",
            TurnSide::Inside => "A thread in the bore needs an inside threading tool.",
        }));
    }
    if !(positive(op.lead)
        && positive(op.height)
        && positive(op.first_depth)
        && positive(op.min_depth)
        && positive(op.clearance)
        && positive(op.crest))
    {
        return Err(err(
            "Threading needs a diameter, a pitch, a depth, pass depths and a clearance above zero.",
        ));
    }
    if !(op.finish_allowance >= 0.0 && op.finish_allowance < op.height) {
        return Err(err(
            "The thread's finishing allowance must be less than its depth.",
        ));
    }
    if !(op.run_in.is_finite() && op.run_in >= 0.0) {
        return Err(err("The run-in must be zero or more."));
    }
    if op.z_end >= op.z_start {
        return Err(err("A thread runs from its start towards the chuck."));
    }
    if op.height > tool.max_depth + 1e-9 {
        return Err(err(
            "The thread is deeper than the threading insert can cut.",
        ));
    }
    if op.spring_passes > 99 {
        return Err(err("A thread has at most 99 finishing passes."));
    }
    let s = op.side.sign();
    if op.side.inside() && op.crest + op.height <= 0.0 {
        return Err(err("The thread's diameter must be above zero."));
    }
    if !op.side.inside() && op.crest - op.height <= 0.0 {
        return Err(err("The thread is deeper than its radius."));
    }
    let z_s = op.z_start + op.run_in * op.lead;
    let away = op.crest + s * op.clearance;
    let start = [away, z_s];
    let mut moves = vec![TurnMove::Rapid { to: start }];
    for d in op.depths() {
        let x = op.crest - s * d;
        moves.push(TurnMove::Rapid { to: [x, z_s] });
        moves.push(TurnMove::Thread {
            to: [x, op.z_end],
            lead: op.lead,
        });
        moves.push(TurnMove::Rapid {
            to: [away, op.z_end],
        });
        moves.push(TurnMove::Rapid { to: start });
    }
    let count = moves.len() - 1;
    Ok(TurnPath {
        moves,
        cycle: Some(TurnCycle::Thread {
            first: 1,
            count,
            start,
            root_x: op.crest - s * op.height,
            end_z: op.z_end,
            height: op.height,
            first_depth: op.first_depth,
            min_depth: op.min_depth,
            finish_allowance: op.finish_allowance,
            spring_passes: op.spring_passes.max(1),
            angle,
            chamfer: 0.0,
            lead: op.lead,
        }),
    })
}

/// The usual depth of an outside metric or unified thread (radius) for a
/// pitch: 0.6134 of it.
pub fn external_thread_height(pitch: f64) -> f64 {
    0.6134 * pitch
}

/// The usual depth of an inside thread (radius) for a pitch: 0.5413 of it.
pub fn internal_thread_height(pitch: f64) -> f64 {
    0.5413 * pitch
}
