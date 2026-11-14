// SPDX-License-Identifier: MIT
//! The stock simulation: a flat end mill's pocket floor, a ball's groove,
//! a V-bit's cone, a rapid through the stock, the runs, a body's height
//! map, and the material a faced stock leaves.

use super::*;
use crate::geom::{Contour, Region};
use crate::ops::{self, Depths, Direction, FaceOp, Heights, PocketOp};
use crate::program::{Coolant, ProgramOp, SpindleDirection};
use crate::tool::Cutting;
use crate::toolpath::Toolpath;

fn cutting() -> Cutting {
    Cutting {
        spindle_rpm: 18000.0,
        feed: 1200.0,
        plunge_feed: 300.0,
    }
}

fn heights() -> Heights {
    Heights {
        clearance: 10.0,
        retract: 2.0,
    }
}

fn program(ops: Vec<(&str, Tool, Toolpath)>) -> Program {
    let mut p = Program::new("Test", 15.0);
    for (name, tool, toolpath) in ops {
        p.operations.push(ProgramOp {
            name: name.into(),
            tool,
            spindle_rpm: 18000.0,
            spindle: SpindleDirection::Cw,
            coolant: Coolant::Off,
            toolpath,
        });
    }
    p
}

fn stock(min: P3, max: P3, program: &Program) -> HeightMap {
    HeightMap::of_box(&Bounds { min, max }, finest_step(program), MAX_NODES)
}

fn rapid(x: f64, y: f64, z: f64) -> Move {
    Move::Rapid { to: [x, y, z] }
}

fn line(x: f64, y: f64, z: f64) -> Move {
    Move::Line {
        to: [x, y, z],
        feed: 600.0,
    }
}

/// A closed box of twelve triangles.
fn box_mesh(min: P3, max: P3) -> TriMesh {
    let corner = |i: usize| -> P3 {
        std::array::from_fn(|k| if (i >> k) & 1 == 1 { max[k] } else { min[k] })
    };
    let quads: [[u32; 4]; 6] = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    TriMesh {
        vertices: (0..8).map(corner).collect(),
        triangles: quads
            .iter()
            .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
            .collect(),
    }
}

#[test]
fn the_grid_is_fine_for_the_smallest_tool_and_capped() {
    let grid = Grid::for_stock([0.0, 0.0], [40.0, 30.0], 0.75, MAX_NODES);
    assert!(grid.step[0] <= 0.75 + 1e-12 && grid.step[1] <= 0.75 + 1e-12);
    assert!((grid.x(grid.nx - 1) - 40.0).abs() < 1e-9);
    assert!((grid.y(grid.ny - 1) - 30.0).abs() < 1e-9);
    let capped = Grid::for_stock([0.0, 0.0], [1000.0, 1000.0], 0.01, 10_000);
    assert!(capped.nodes() <= 10_000, "{capped:?}");
    assert!(capped.step[0] > 9.0, "{capped:?}");
    let p = program(vec![
        ("A", Tool::flat_end_mill(1, 6.0), Toolpath::default()),
        ("B", Tool::flat_end_mill(2, 2.0), Toolpath::default()),
    ]);
    assert!((finest_step(&p) - 0.25).abs() < 1e-12);
}

#[test]
fn a_flat_end_mills_pocket_leaves_a_flat_floor_at_its_depth() {
    let tool = Tool::flat_end_mill(1, 6.0);
    let op = PocketOp {
        region: Region::new(Contour::rectangle([10.0, 8.0], [30.0, 22.0]), vec![]),
        stepover: None,
        direction: Direction::Climb,
        depths: Depths {
            top: 0.0,
            bottom: -5.0,
            step_down: 2.5,
        },
        heights: heights(),
        stock_to_leave: 0.0,
        cutting: cutting(),
        entry: ops::Entry::Plunge,
        adaptive: None,
    };
    let path = ops::pocket(&op, &tool).unwrap();
    let p = program(vec![("Pocket 1", tool, path)]);
    let start = stock([0.0, 0.0, -10.0], [40.0, 30.0, 0.0], &p);
    let step = start.grid.step[0].max(start.grid.step[1]);
    let sim = simulate(&p, start, None, |_, _| {});
    assert!(sim.rapid_cuts.is_empty(), "{:?}", sim.rapid_cuts);
    assert!(sim.removed > 0.0);
    let map = &sim.stock;
    let g = map.grid;
    let mut floor = 0usize;
    for j in 0..g.ny {
        for i in 0..g.nx {
            let (x, y) = (g.x(i), g.y(j));
            let z = map.at(i, j);
            // Away from the corners the tool cannot reach: the floor, flat
            // within the grid's step.
            let inner = (13.0..=27.0).contains(&x) && (8.0 + 1e-6..=22.0 - 1e-6).contains(&y)
                || (11.0..=19.0).contains(&y) && (10.0 + 1e-6..=30.0 - 1e-6).contains(&x);
            if inner {
                assert!((-5.0 - 1e-9..=-5.0 + step).contains(&z), "{x} {y}: {z}");
                floor += 1;
            }
            // Outside the pocket nothing is cut.
            if x < 10.0 - 1e-6 || x > 30.0 + 1e-6 || y < 8.0 - 1e-6 || y > 22.0 + 1e-6 {
                assert_eq!(z, 0.0, "{x} {y}");
            }
            // Nothing below the floor.
            assert!(z >= -5.0 - 1e-9, "{x} {y}: {z}");
        }
    }
    assert!(floor > 100, "{floor}");
    assert!((map.lowest() + 5.0).abs() < 1e-9);
}

#[test]
fn a_ball_end_along_a_line_leaves_a_groove_of_its_profile() {
    let mut tool = Tool::flat_end_mill(1, 6.0);
    tool.shape = ToolShape::BallEndMill;
    let p = program(vec![(
        "Groove",
        tool,
        Toolpath {
            moves: vec![
                rapid(-5.0, 10.0, 5.0),
                line(-5.0, 10.0, -2.0),
                line(45.0, 10.0, -2.0),
                rapid(45.0, 10.0, 15.0),
            ],
            cycles: vec![],
        },
    )]);
    let start = stock([0.0, 0.0, -10.0], [40.0, 20.0, 0.0], &p);
    let sim = simulate(&p, start, None, |_, _| {});
    assert!(sim.rapid_cuts.is_empty(), "{:?}", sim.rapid_cuts);
    let map = &sim.stock;
    let g = map.grid;
    let i = g.nx / 2;
    let mut cut = 0;
    for j in 0..g.ny {
        let d = (g.y(j) - 10.0).abs();
        // The ball's centre 3 mm above its tip at -2.
        let expected = if d < 3.0 {
            (-2.0 + 3.0 - (9.0 - d * d).sqrt()).min(0.0)
        } else {
            0.0
        };
        let z = map.at(i, j);
        assert!(
            (z - expected).abs() < 1e-9,
            "y {}: {z}, not {expected}",
            g.y(j)
        );
        if z < 0.0 {
            cut += 1;
        }
        // The same across the whole length.
        assert!((map.at(0, j) - z).abs() < 1e-9 && (map.at(g.nx - 1, j) - z).abs() < 1e-9);
    }
    // The groove is 2 sqrt(3^2 - 1^2) = 5.66 mm wide at the top.
    let width = 2.0 * 8.0_f64.sqrt();
    let nodes = (width / g.step[1]) as i64;
    assert!(
        (cut as i64 - nodes).abs() <= 2,
        "{cut} nodes cut, about {nodes}"
    );
}

#[test]
fn a_v_bit_plunge_leaves_a_cone() {
    let mut tool = Tool::flat_end_mill(1, 10.0);
    tool.shape = ToolShape::VBit {
        angle: 90.0,
        tip_diameter: 0.0,
    };
    let p = program(vec![(
        "Chamfer",
        tool,
        Toolpath {
            moves: vec![
                rapid(20.0, 20.0, 5.0),
                line(20.0, 20.0, -3.0),
                rapid(20.0, 20.0, 15.0),
            ],
            cycles: vec![],
        },
    )]);
    let start = stock([0.0, 0.0, -10.0], [40.0, 40.0, 0.0], &p);
    let sim = simulate(&p, start, None, |_, _| {});
    let map = &sim.stock;
    let g = map.grid;
    for j in 0..g.ny {
        for i in 0..g.nx {
            let d = (g.x(i) - 20.0).hypot(g.y(j) - 20.0);
            // 90 degrees: up one for one, flat beyond the bit's 5 mm radius.
            let expected = if d <= 5.0 { (-3.0 + d).min(0.0) } else { 0.0 };
            assert!(
                (map.at(i, j) - expected).abs() < 1e-9,
                "{d}: {}",
                map.at(i, j)
            );
        }
    }
    assert!((Cutter::of(&p.operations[0].tool).height(5.0) - 5.0).abs() < 1e-12);
}

#[test]
fn the_profiles_of_the_tools() {
    let mut t = Tool::flat_end_mill(1, 10.0);
    t.shape = ToolShape::BullNose { corner_radius: 1.0 };
    let bull = Cutter::of(&t);
    assert_eq!(bull.height(3.9), 0.0);
    assert!((bull.height(5.0) - 1.0).abs() < 1e-12);
    t.shape = ToolShape::Drill { point_angle: 118.0 };
    let drill = Cutter::of(&t);
    let h = 5.0 / (59.0_f64.to_radians().tan());
    assert!((drill.height(5.0) - h).abs() < 1e-12);
    t.shape = ToolShape::VBit {
        angle: 60.0,
        tip_diameter: 2.0,
    };
    let v = Cutter::of(&t);
    assert_eq!(v.height(1.0), 0.0);
    assert!((v.height(2.0) - 1.0 / 30.0_f64.to_radians().tan()).abs() < 1e-12);
    // Beyond the radius: the radius's.
    assert_eq!(Cutter::of(&Tool::flat_end_mill(1, 6.0)).height(9.0), 0.0);
}

#[test]
fn a_rapid_through_the_stock_is_reported() {
    let p = program(vec![(
        "Pocket 1",
        Tool::flat_end_mill(1, 6.0),
        Toolpath {
            moves: vec![
                rapid(20.0, 30.0, 5.0),
                line(20.0, 30.0, -2.0),
                rapid(80.0, 30.0, -2.0),
                rapid(80.0, 30.0, 15.0),
            ],
            cycles: vec![],
        },
    )]);
    let start = stock([0.0, 0.0, -30.0], [100.0, 60.0, 0.0], &p);
    let mut places = Vec::new();
    let sim = simulate(&p, start, None, |place, _| places.push(place));
    assert_eq!(places.len(), 4);
    assert_eq!(sim.rapid_cuts.len(), 1, "{:?}", sim.rapid_cuts);
    let c = &sim.rapid_cuts[0];
    assert_eq!(
        c.message,
        "Pocket 1: a rapid move takes the cutter into the stock."
    );
    assert_eq!((c.operation, c.move_index, c.leg), (0, 2, 0));
    assert!((c.depth - 2.0).abs() < 1e-9, "{c:?}");
    // It first cuts just past the hole it plunged.
    assert!(c.at[0] > 20.0 && c.at[0] < 21.0 && c.at[2] == -2.0, "{c:?}");
}

#[test]
fn the_simulation_stops_at_the_place_asked() {
    let p = program(vec![(
        "Slot",
        Tool::flat_end_mill(1, 6.0),
        Toolpath {
            moves: vec![
                rapid(10.0, 10.0, 5.0),
                line(10.0, 10.0, -1.0),
                line(30.0, 10.0, -1.0),
                line(30.0, 10.0, -3.0),
                rapid(30.0, 10.0, 15.0),
            ],
            cycles: vec![],
        },
    )]);
    let start = stock([0.0, 0.0, -10.0], [40.0, 20.0, 0.0], &p);
    let until = Place {
        operation: 0,
        move_index: 1,
    };
    let sim = simulate(&p, start.clone(), Some(until), |_, _| {});
    assert_eq!(sim.last, Some(until));
    assert!((sim.stock.height_at(10.0, 10.0).unwrap() + 1.0).abs() < 1e-9);
    assert_eq!(sim.stock.height_at(30.0, 10.0).unwrap(), 0.0);
    let all = simulate(&p, start, None, |_, _| {});
    assert!((all.stock.height_at(30.0, 10.0).unwrap() + 3.0).abs() < 1e-9);
    assert!((all.stock.height_at(20.0, 10.0).unwrap() + 1.0).abs() < 1e-9);
}

#[test]
fn runs_carry_the_heights_and_their_changes() {
    let grid = Grid::covering([0.0, 0.0], [4.0, 1.0], 1.0);
    let mut before = HeightMap::flat(grid, -5.0, 0.0);
    assert_eq!(before.runs(), vec![0, 10]);
    let mut after = before.clone();
    after.z[3] = -1.25;
    after.z[4] = -1.25;
    after.z[9] = -0.0004;
    assert_eq!(after.runs(), vec![0, 3, -1250, 2, 0, 5]);
    // -0.0004 rounds to the same micrometre as 0: unchanged.
    assert_eq!(after.runs_since(&before), vec![0, -3, -1250, 2, 0, -5]);
    before.apply_runs(&after.runs_since(&before)).unwrap();
    assert_eq!(before.z[3], -1.25);
    assert_eq!(before.z[9], 0.0);
    assert!(before.apply_runs(&[0, 3]).is_err());
}

#[test]
fn a_bodys_height_map_is_its_top() {
    let grid = Grid::covering([0.0, 0.0], [40.0, 30.0], 1.0);
    let part = HeightMap::top_of_mesh(
        grid,
        -10.0,
        &box_mesh([5.0, 5.0, -10.0], [20.0, 25.0, -3.0]),
    );
    assert_eq!(part.height_at(10.0, 10.0), Some(-3.0));
    // Its edges are its top's.
    assert_eq!(part.height_at(5.0, 5.0), Some(-3.0));
    assert_eq!(part.height_at(20.0, 25.0), Some(-3.0));
    assert_eq!(part.height_at(30.0, 10.0), Some(-10.0));
    assert_eq!(part.height_at(80.0, 10.0), None);
}

#[test]
fn a_fully_faced_stock_leaves_nothing() {
    let tool = Tool::flat_end_mill(1, 6.0);
    let op = FaceOp {
        min: [0.0, 0.0],
        max: [40.0, 30.0],
        depths: Depths {
            top: 0.0,
            bottom: -1.0,
            step_down: 1.0,
        },
        heights: heights(),
        stepover: None,
        cutting: cutting(),
        direction: Direction::Climb,
        adaptive: None,
    };
    let path = ops::face(&op, &tool).unwrap();
    let p = program(vec![("Facing 1", tool, path)]);
    let start = stock([0.0, 0.0, -10.0], [40.0, 30.0, 0.0], &p);
    let part = HeightMap::top_of_mesh(
        start.grid,
        start.bottom,
        &box_mesh([0.0, 0.0, -10.0], [40.0, 30.0, -1.0]),
    );
    let before = remaining(&start, &part, 1e-6, 6.0);
    assert!((before.max - 1.0).abs() < 1e-9 && (before.mean - 1.0).abs() < 1e-9);
    assert!((before.volume - before.area).abs() < 1e-6 * before.volume.max(1.0));
    assert_eq!(before.thickest.len(), PLACES);
    let sim = simulate(&p, start, None, |_, _| {});
    assert!(sim.rapid_cuts.is_empty(), "{:?}", sim.rapid_cuts);
    let left = remaining(&sim.stock, &part, 1e-6, 6.0);
    assert!(left.max <= 1e-9, "{left:?}");
    assert!(left.mean <= 1e-9 && left.volume <= 1e-6, "{left:?}");
    assert!(
        left.thickest.is_empty() && left.gouges.is_empty(),
        "{left:?}"
    );
    assert_eq!(left.gouge, 0.0);
    assert_eq!(left.beside_nodes, 0);
    assert_eq!(left.part_nodes, sim.stock.grid.nodes());
}

#[test]
fn a_cut_into_the_part_is_a_gouge() {
    let tool = Tool::flat_end_mill(1, 6.0);
    let p = program(vec![(
        "Slot",
        tool,
        Toolpath {
            moves: vec![
                rapid(-5.0, 15.0, 5.0),
                line(-5.0, 15.0, -2.0),
                line(45.0, 15.0, -2.0),
                rapid(45.0, 15.0, 15.0),
            ],
            cycles: vec![],
        },
    )]);
    let start = stock([0.0, 0.0, -10.0], [40.0, 30.0, 0.0], &p);
    let part = HeightMap::top_of_mesh(
        start.grid,
        start.bottom,
        &box_mesh([0.0, 0.0, -10.0], [40.0, 30.0, -1.0]),
    );
    let sim = simulate(&p, start, None, |_, _| {});
    let left = remaining(&sim.stock, &part, 0.01, 6.0);
    assert!((left.gouge - 1.0).abs() < 1e-9, "{left:?}");
    assert!(!left.gouges.is_empty() && (left.gouges[0].at[1] - 15.0).abs() <= 3.0);
    assert!((left.max - 1.0).abs() < 1e-9);
    // The thickest places are apart from each other.
    for (k, a) in left.thickest.iter().enumerate() {
        for b in &left.thickest[k + 1..] {
            assert!((a.at[0] - b.at[0]).hypot(a.at[1] - b.at[1]) >= 6.0);
        }
    }
}

#[test]
fn the_pyramid_follows_the_cuts() {
    let grid = Grid::covering([0.0, 0.0], [20.0, 12.0], 1.0);
    let mut map = HeightMap::flat(grid, -5.0, 0.0);
    let mut pyramid = MaxPyramid::new(&map);
    assert_eq!(pyramid.max(pyramid.top(), 0, 0), 0.0);
    let tool = Tool::flat_end_mill(1, 40.0);
    let removal = map.cut_line(&Cutter::of(&tool), [10.0, 6.0, -2.0], [10.0, 6.0, -2.0]);
    pyramid.update(&map, removal.nodes.unwrap());
    // Every node is within the tool: all at -2.
    assert_eq!(pyramid.max(pyramid.top(), 0, 0), -2.0);
    let [i0, i1, j0, j1] = pyramid.block(1, 10, 6);
    assert_eq!((i0, i1, j0, j1), (20, 20, 12, 12));
}
