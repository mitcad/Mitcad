// SPDX-License-Identifier: MIT
//! Contour on the line with a V-bit (mitcad#2589, GitHub mitcad/Mitcad#747):
//! an engraving cutter and a chamfer mill trace open and closed outlines
//! without an offset down to the final depth, the stock simulation cuts
//! their V groove, and outside, inside and wrong depths stay refused.

use super::*;
use crate::geom::Vertex;
use crate::machine::Bounds;
use crate::program::{Coolant, Program, ProgramOp, SpindleDirection};
use crate::stock::{HeightMap, MAX_NODES, finest_step, simulate};
use crate::toolpath::Move;

/// A V-bit of `diameter` with its included `angle` and tip, as the model
/// makes of the library's engraving cutters and chamfer mills.
fn v_bit(angle: f64, tip_diameter: f64, diameter: f64) -> Tool {
    let mut tool = Tool::flat_end_mill(3, diameter);
    tool.name = "V-bit".into();
    tool.shape = ToolShape::VBit {
        angle,
        tip_diameter,
    };
    tool.flute_length = 3.0;
    tool.length = 20.0;
    tool
}

fn op(contours: Vec<Contour>, side: Side, bottom: f64, step_down: f64) -> ContourOp {
    ContourOp {
        contours,
        side,
        direction: Direction::Climb,
        depths: Depths {
            top: 0.0,
            bottom,
            step_down,
        },
        heights: Heights {
            clearance: 10.0,
            retract: 2.0,
        },
        stock_to_leave: 0.0,
        cutting: Cutting {
            spindle_rpm: 18000.0,
            feed: 600.0,
            plunge_feed: 200.0,
        },
        entry: Entry::Plunge,
        tabs: None,
        lead: None,
    }
}

fn line(from: [f64; 2], to: [f64; 2]) -> Contour {
    let ends = vec![Vertex::line(from[0], from[1]), Vertex::line(to[0], to[1])];
    Contour::open(ends)
}

/// The ends of the moves below the top (the cuts and the plunges).
fn cut_ends(p: &Toolpath) -> Vec<[f64; 3]> {
    p.moves
        .iter()
        .filter(|m| !matches!(m, Move::Rapid { .. }))
        .map(|m| m.to())
        .filter(|q| q[2] < -1e-9)
        .collect()
}

#[test]
fn an_engraving_cutter_traces_an_open_line_on_the_line_to_the_final_depth() {
    let tool = v_bit(90.0, 0.0, 6.0);
    let p = contour(
        &op(vec![line([0.0, 0.0], [30.0, 0.0])], Side::On, -1.0, 0.5),
        &tool,
    )
    .unwrap();
    let ends = cut_ends(&p);
    assert!(!ends.is_empty());
    // No lateral offset: every cut on the line itself.
    for q in &ends {
        assert!(q[1].abs() < 1e-9, "off the line at {q:?}");
        assert!(
            (-1e-9..=30.0 + 1e-9).contains(&q[0]),
            "beyond the line at {q:?}"
        );
    }
    // Two passes, back and forth, the last at the bottom.
    let feeds: Vec<[f64; 3]> = p
        .moves
        .iter()
        .filter(|m| matches!(m, Move::Line { feed, .. } if *feed == 600.0))
        .map(|m| m.to())
        .collect();
    assert_eq!(feeds, vec![[30.0, 0.0, -0.5], [0.0, 0.0, -1.0]]);
    let (min, _) = p.bounds().unwrap();
    assert_eq!(min[2], -1.0);
}

#[test]
fn a_chamfer_mill_traces_a_closed_outline_without_an_offset() {
    let tool = v_bit(90.0, 1.0, 10.0);
    let square = Contour::rectangle([0.0, 0.0], [20.0, 10.0]);
    let o = op(vec![square.clone()], Side::On, -2.0, 2.0);
    let p = contour(&o, &tool).unwrap();
    let ends = cut_ends(&p);
    assert!(!ends.is_empty());
    for q in &ends {
        let d = path::distance_to(&o.contours, [q[0], q[1]]);
        assert!(d < 1e-9, "{d} off the outline at {q:?}");
    }
    let (min, max) = p.bounds().unwrap();
    assert_eq!(min[2], -2.0);
    assert!(min[0].abs() < 1e-9 && (max[0] - 20.0).abs() < 1e-9);
    assert!(min[1].abs() < 1e-9 && (max[1] - 10.0).abs() < 1e-9);
}

#[test]
fn the_stock_simulation_cuts_the_v_bits_groove() {
    let tool = v_bit(90.0, 0.0, 6.0);
    let p = contour(
        &op(vec![line([10.0, 20.0], [30.0, 20.0])], Side::On, -1.0, 1.0),
        &tool,
    )
    .unwrap();
    let mut program = Program::new("Engraving", 15.0);
    program.operations.push(ProgramOp {
        name: "Contour 1".into(),
        tool,
        spindle_rpm: 18000.0,
        spindle: SpindleDirection::Cw,
        coolant: Coolant::Off,
        toolpath: p,
    });
    let start = HeightMap::of_box(
        &Bounds {
            min: [0.0, 0.0, -10.0],
            max: [40.0, 40.0, 0.0],
        },
        finest_step(&program),
        MAX_NODES,
    );
    let sim = simulate(&program, start, None, |_, _| {});
    assert!(sim.rapid_cuts.is_empty(), "{:?}", sim.rapid_cuts);
    let map = &sim.stock;
    let g = map.grid;
    let mut deepest = 0.0_f64;
    for j in 0..g.ny {
        for i in 0..g.nx {
            let (x, y) = (g.x(i), g.y(j));
            deepest = deepest.min(map.at(i, j));
            // Along the middle of the line: a 90 degree V, 1 mm deep and
            // 2 mm wide at the top (a flat end mill would cut 6 mm).
            if (12.0..=28.0).contains(&x) {
                let d = (y - 20.0).abs();
                let expected = (-1.0 + d).min(0.0);
                assert!(
                    (map.at(i, j) - expected).abs() < 1e-3,
                    "at {x}, {y}: {} for {expected}",
                    map.at(i, j)
                );
            }
        }
    }
    assert!((deepest + 1.0).abs() < 1e-6, "{deepest}");
}

#[test]
fn outside_and_inside_refuse_a_v_bit() {
    let tool = v_bit(90.0, 0.0, 6.0);
    let square = Contour::rectangle([0.0, 0.0], [20.0, 10.0]);
    for side in [Side::Outside, Side::Inside] {
        let e = contour(&op(vec![square.clone()], side, -1.0, 1.0), &tool).unwrap_err();
        assert!(e.to_string().contains("on the line"), "{e}");
    }
}

#[test]
fn a_v_bit_keeps_the_depth_and_tool_checks() {
    let tool = v_bit(90.0, 0.0, 6.0);
    let path = vec![line([0.0, 0.0], [30.0, 0.0])];
    let on = |bottom: f64, step_down: f64| op(path.clone(), Side::On, bottom, step_down);
    assert!(contour(&on(-1.0, 1.0), &tool).is_ok());
    // The step down deeper than the flutes.
    assert!(contour(&on(-4.0, 4.0), &tool).is_err());
    // The bottom above the top.
    assert!(contour(&on(1.0, 1.0), &tool).is_err());
    // Deeper than the stick-out.
    assert!(contour(&on(-25.0, 2.0), &tool).is_err());
    // A tool without a diameter.
    let mut broken = tool.clone();
    broken.diameter = 0.0;
    assert!(contour(&on(-1.0, 1.0), &broken).is_err());
}

#[test]
fn end_mills_still_contour_on_every_side_and_a_ball_on_the_line_is_refused() {
    let square = Contour::rectangle([0.0, 0.0], [20.0, 10.0]);
    let flat = Tool::flat_end_mill(1, 3.0);
    let mut bull = Tool::flat_end_mill(2, 3.0);
    bull.shape = ToolShape::BullNose { corner_radius: 0.5 };
    let closed = |side: Side| op(vec![square.clone()], side, -2.0, 1.0);
    for tool in [&flat, &bull] {
        for side in [Side::Outside, Side::Inside, Side::On] {
            assert!(contour(&closed(side), tool).is_ok());
        }
    }
    let mut ball = flat.clone();
    ball.shape = ToolShape::BallEndMill;
    assert!(contour(&closed(Side::On), &ball).is_err());
}
