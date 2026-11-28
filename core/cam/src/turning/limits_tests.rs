// SPDX-License-Identifier: MIT
//! Turn Roughing and Turn Finishing within limits along Z (mitcad#2629,
//! GitHub mitcad/Mitcad#690): a shaft of three diameters, 16, 22 and 28
//! mm, each 15 mm long, from 32 mm stock. Limited to the middle one, the
//! tool's nose never crosses either limit below the stock's surface (the
//! neighbours and their stock are left as they are), and the middle
//! diameter is cut to its size; a limit towards the face inside the part
//! is a plunge into the stock, written as moves, steep as the entry angle;
//! a limit towards the chuck alone keeps the G71. Without limits nothing
//! changes; limits that leave nothing, or are the wrong way round, are
//! refused in words.

use super::*;

const NOSE: f64 = 0.4;
const STOCK: f64 = 16.0;

fn shaft() -> TurnPart {
    TurnPart {
        outside: vec![
            PV::new(0.0, 0.0),
            PV::new(8.0, 0.0),
            PV::new(8.0, -15.0),
            PV::new(11.0, -15.0),
            PV::new(11.0, -30.0),
            PV::new(14.0, -30.0),
            PV::new(14.0, -45.0),
        ],
        inside: vec![],
        stock_radius: STOCK,
        stock_inner_radius: 0.0,
        stock_face: 1.0,
    }
}

fn tool() -> TurningTool {
    TurningTool {
        number: 1,
        offset: 0,
        name: "Turning".into(),
        kind: TurningToolKind::Turning,
        nose_radius: NOSE,
        max_depth: 4.0,
        back_angle: None,
    }
}

fn rough_op() -> RoughOp {
    RoughOp {
        side: TurnSide::Outside,
        depth: 2.0,
        retract: 0.5,
        allowance_x: 0.2,
        allowance_z: 0.1,
        clearance: 2.0,
        feed: 0.25,
        plunge: 0.0,
    }
}

fn finish_op() -> FinishOp {
    FinishOp {
        side: TurnSide::Outside,
        clearance: 2.0,
        feed: 0.1,
        plunge: 0.0,
        roughed: false,
    }
}

fn limits(from: Option<f64>, to: Option<f64>) -> TurnLimits {
    TurnLimits {
        from,
        to,
        ..TurnLimits::default()
    }
}

/// Every point the controlled point (the imaginary tip) passes, rapids
/// included.
fn points(path: &TurnPath) -> Vec<PXZ> {
    let mut out = Vec::new();
    let mut at = path.moves.first().map_or([0.0, 0.0], TurnMove::to);
    out.push(at);
    for m in &path.moves {
        let pts = move_points(at, m, 1e-3);
        // Along straight moves too, so that a move that crosses a limit
        // is seen.
        let mut from = at;
        for p in pts {
            for k in 1..=20 {
                let u = k as f64 / 20.0;
                out.push([
                    from[0] + u * (p[0] - from[0]),
                    from[1] + u * (p[1] - from[1]),
                ]);
            }
            from = p;
        }
        at = m.to();
    }
    out
}

/// Panics where the nose (its centre a nose radius in from the tip in X
/// and Z) reaches beyond `from` or `to` below the stock's surface.
fn stays_within(path: &TurnPath, from: Option<f64>, to: Option<f64>) {
    for p in points(path) {
        let (cx, cz) = (p[0] + NOSE, p[1] + NOSE);
        if cx - NOSE >= STOCK - 1e-6 {
            continue;
        }
        if let Some(f) = from {
            assert!(cz + NOSE <= f + 1e-6, "the nose crosses Z {f} at {p:?}");
        }
        if let Some(t) = to {
            assert!(cz - NOSE >= t - 1e-6, "the nose crosses Z {t} at {p:?}");
        }
    }
}

/// Whether a feed move ends at X `x` between the two Z.
fn cuts_to(path: &TurnPath, x: f64, z_lo: f64, z_hi: f64) -> bool {
    path.moves.iter().any(|m| {
        matches!(m, TurnMove::Line { .. })
            && (m.to()[0] - x).abs() < 1e-6
            && m.to()[1] > z_lo
            && m.to()[1] < z_hi
    })
}

#[test]
fn no_limits_change_nothing() {
    let part = shaft();
    let none = TurnLimits::default();
    assert_eq!(
        rough_within(&part, &rough_op(), &tool(), &none).unwrap(),
        rough(&part, &rough_op(), &tool()).unwrap()
    );
    assert_eq!(
        finish_within(&part, &finish_op(), &tool(), &none).unwrap(),
        finish(&part, &finish_op(), &tool()).unwrap()
    );
    // Limits beyond the part's ends cover the whole side as well.
    let wide = limits(Some(5.0), Some(-60.0));
    assert_eq!(
        finish_within(&part, &finish_op(), &tool(), &wide).unwrap(),
        finish(&part, &finish_op(), &tool()).unwrap()
    );
    // A saved roughing without limits reads as none.
    let read: TurnLimits = serde_json::from_str("{}").unwrap();
    assert_eq!(read, none);
    // The whole side is turned: down to the smallest diameter at the
    // face, along to the largest at the back.
    let whole = finish(&part, &finish_op(), &tool()).unwrap();
    assert!(cuts_to(&whole, 8.0, -16.0, 0.0), "{:?}", whole.moves);
    assert!(cuts_to(&whole, 14.0, -46.0, -30.0), "{:?}", whole.moves);
}

#[test]
fn a_limited_finish_turns_only_the_middle_diameter() {
    let part = shaft();
    let (from, to) = (Some(-15.0), Some(-30.0));
    let path = finish_within(&part, &finish_op(), &tool(), &limits(from, to)).unwrap();
    stays_within(&path, from, to);
    // Along the middle diameter, not the others.
    assert!(cuts_to(&path, 11.0, -30.0, -15.0), "{:?}", path.moves);
    assert!(
        path.moves
            .iter()
            .filter(|m| matches!(m, TurnMove::Line { .. }))
            .all(|m| m.to()[0] > 10.9),
        "{:?}",
        path.moves
    );
    // Straight out at the limit towards the chuck.
    let last_feed = path
        .moves
        .iter()
        .rev()
        .find(|m| matches!(m, TurnMove::Line { .. }))
        .unwrap();
    assert!((last_feed.to()[1] + 30.0).abs() < 1e-6, "{last_feed:?}");
    assert!(last_feed.to()[0] >= STOCK, "{last_feed:?}");
    // Still a G70 over its blocks.
    assert!(matches!(path.cycle, Some(TurnCycle::Finish { .. })));
}

#[test]
fn a_limited_roughing_leaves_the_neighbours() {
    let part = shaft();
    let (from, to) = (Some(-15.0), Some(-30.0));
    let path = rough_within(&part, &rough_op(), &tool(), &limits(from, to)).unwrap();
    stays_within(&path, from, to);
    // Down to the middle diameter and its stock to leave (the larger
    // allowance all round: written as moves, as a plunging roughing).
    assert!(cuts_to(&path, 11.2, -30.0, -15.0), "{:?}", path.moves);
    assert!(path.cycle.is_none());
    // The passes plunge at the limit towards the face: a feed in X alone
    // into the stock there.
    let mut at = path.moves[0].to();
    let mut plunges = 0;
    for m in &path.moves {
        let to = m.to();
        if matches!(m, TurnMove::Line { .. })
            && (to[1] - at[1]).abs() < 1e-9
            && to[0] < at[0]
            && at[0] > STOCK - 1e-6
        {
            plunges += 1;
        }
        at = to;
    }
    assert!(plunges >= 1, "{:?}", path.moves);
}

#[test]
fn a_ramped_entry_leaves_what_lies_under_it() {
    let part = shaft();
    let ramp = TurnLimits {
        from: Some(-15.0),
        to: Some(-30.0),
        entry: 45.0,
    };
    let path = rough_within(&part, &rough_op(), &tool(), &ramp).unwrap();
    stays_within(&path, ramp.from, ramp.to);
    // Down a 45 degree line from the stock's surface at Z -15: at Z -17
    // the tool is still above 14 (radius).
    for p in points(&path) {
        if p[1] > -17.0 && p[1] < -15.0 {
            assert!(p[0] > 12.0, "under the ramp at {p:?}");
        }
    }
    // Further in, it reaches the diameter.
    assert!(cuts_to(&path, 11.2, -30.0, -21.0), "{:?}", path.moves);
    // An entry no steeper than the Z axis cannot go in: refused in words.
    let flat = TurnLimits { entry: 0.0, ..ramp };
    let refused = rough_within(&part, &rough_op(), &tool(), &flat).unwrap_err();
    assert!(
        refused
            .message
            .starts_with("The tool cannot go down into the stock"),
        "{}",
        refused.message
    );
}

#[test]
fn a_limit_towards_the_chuck_alone_keeps_the_g71() {
    let part = shaft();
    let to = Some(-30.0);
    let path = rough_within(&part, &rough_op(), &tool(), &limits(None, to)).unwrap();
    stays_within(&path, None, to);
    assert!(matches!(path.cycle, Some(TurnCycle::Rough { .. })));
    // The first two diameters are roughed, the third is left.
    assert!(cuts_to(&path, 8.2, -15.0, 0.0), "{:?}", path.moves);
    assert!(cuts_to(&path, 11.2, -30.0, -15.0), "{:?}", path.moves);
    let finished = finish_within(&part, &finish_op(), &tool(), &limits(None, to)).unwrap();
    stays_within(&finished, None, to);
    assert!(
        !cuts_to(&finished, 14.0, -46.0, -30.0),
        "{:?}",
        finished.moves
    );
}

#[test]
fn limits_that_leave_nothing_are_refused() {
    let part = shaft();
    let message = |l: TurnLimits| {
        finish_within(&part, &finish_op(), &tool(), &l)
            .unwrap_err()
            .message
    };
    assert_eq!(
        message(limits(Some(-30.0), Some(-15.0))),
        "The limit towards the face must be in front of the limit towards the chuck."
    );
    assert_eq!(
        message(limits(None, Some(2.0))),
        "The operation's limits leave none of the part's profile between them."
    );
    assert_eq!(
        message(limits(Some(-50.0), None)),
        "The operation's limits leave none of the part's profile between them."
    );
}
