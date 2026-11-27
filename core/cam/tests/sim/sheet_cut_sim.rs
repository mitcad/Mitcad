// SPDX-License-Identifier: MIT
//! A cut sheet's plasma program (mitcad#2553, `mitcad_cam::sheet_cut`) run
//! through grbl 1.1's own code before it ships, as the milling posts' are
//! (`controller_sim.rs`): every line accepted, no alarm, no realtime
//! character in the stream, no hold, and the torch's path the program's:
//! every point of every cut and lead-in reached in order, every point of
//! grbl's path on the program's (its arcs too), the extents (arcs with
//! their extremes) and the end within the G-code's rounding and grbl's
//! 32-bit numbers. The sheet holds a plate with rounded corners and a
//! round hole (mitcad#2570): their arcs and circles are written as G2 and
//! G3, another hole given as the lines of its points as G1. In
//! millimetres and inches, with line numbers. Run by the ctest
//! `core.cam_grbl_sim` (`MITCAD_GRBL_SIM`), skipped without it.

pub(crate) mod grbl_sim {
    use mitcad_cam::Units;
    use mitcad_cam::sheet_cut::{self, EdgeArc, P2, PlasmaSettings, SheetLoop};
    use mitcad_cam::toolpath::move_points;

    use super::super::controller_sim::grbl_sim::run;
    use super::super::controller_sim::{check_lines, simulator};

    type P3 = [f64; 3];

    fn rectangle(part: u32, lo: P2, hi: P2) -> SheetLoop {
        SheetLoop {
            part,
            outer: true,
            points: vec![lo, [hi[0], lo[1]], hi, [lo[0], hi[1]]],
            arcs: Vec::new(),
        }
    }

    /// A plate with rounded corners, counter-clockwise: its lines and its
    /// corners' quarter arcs about their centres.
    fn rounded(part: u32, lo: P2, hi: P2, r: f64) -> SheetLoop {
        let (x0, y0, x1, y1) = (lo[0], lo[1], hi[0], hi[1]);
        let arc = |center: P2| Some(EdgeArc { center, ccw: true });
        SheetLoop {
            part,
            outer: true,
            points: vec![
                [x0 + r, y0],
                [x1 - r, y0],
                [x1, y0 + r],
                [x1, y1 - r],
                [x1 - r, y1],
                [x0 + r, y1],
                [x0, y1 - r],
                [x0, y0 + r],
            ],
            arcs: vec![
                None,
                arc([x1 - r, y0 + r]),
                None,
                arc([x1 - r, y1 - r]),
                None,
                arc([x0 + r, y1 - r]),
                None,
                arc([x0 + r, y0 + r]),
            ],
        }
    }

    /// A round hole as the model gives it: one point and a clockwise
    /// whole turn about its centre (the material on its left).
    fn circle(part: u32, center: P2, r: f64) -> SheetLoop {
        SheetLoop {
            part,
            outer: false,
            points: vec![[center[0] + r, center[1]]],
            arcs: vec![Some(EdgeArc { center, ccw: false })],
        }
    }

    /// A hole given as the lines of its points (a curve that is not an
    /// arc), clockwise.
    fn polygon_hole(part: u32, center: P2, r: f64) -> SheetLoop {
        let points = (0..72)
            .map(|k| {
                let a = -(k as f64) * std::f64::consts::TAU / 72.0;
                [center[0] + r * a.cos(), center[1] + r * a.sin()]
            })
            .collect();
        SheetLoop {
            part,
            outer: false,
            points,
            arcs: Vec::new(),
        }
    }

    /// A representative sheet: two plates with holes, a third with a hole
    /// lying in the second plate's large hole, and a plate with rounded
    /// corners and a round hole.
    fn sheet() -> Vec<SheetLoop> {
        vec![
            rectangle(1, [10.0, 10.0], [110.0, 60.0]),
            circle(1, [30.0, 35.0], 5.0),
            polygon_hole(1, [80.0, 35.0], 8.0),
            rectangle(2, [130.0, 10.0], [230.0, 90.0]),
            circle(2, [180.0, 50.0], 30.0),
            rectangle(3, [170.0, 40.0], [190.0, 60.0]),
            circle(3, [180.0, 50.0], 2.0),
            rounded(4, [240.0, 110.0], [290.0, 180.0], 8.0),
            circle(4, [265.0, 145.0], 10.0),
        ]
    }

    fn near(a: P3, b: P3, tol: f64) -> bool {
        (0..3).all(|k| (a[k] - b[k]).abs() <= tol)
    }

    fn segment_distance(p: P3, a: P3, b: P3) -> f64 {
        let e = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let l2: f64 = e.iter().map(|v| v * v).sum();
        let t = if l2 > 0.0 {
            ((0..3).map(|k| (p[k] - a[k]) * e[k]).sum::<f64>() / l2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (0..3)
            .map(|k| (p[k] - a[k] - t * e[k]).powi(2))
            .sum::<f64>()
            .sqrt()
    }

    #[test]
    #[ignore = "needs the grbl simulator: ctest core.cam_grbl_sim"]
    fn a_plasma_program_runs_on_grbl_and_follows_its_cuts() {
        let Some(sim) = simulator("MITCAD_GRBL_SIM") else {
            return;
        };
        let mut failures = Vec::new();
        for (name, units, line_numbers) in [
            ("plasma in mm", Units::Millimetres, false),
            ("plasma in inches with line numbers", Units::Inches, true),
        ] {
            let settings = PlasmaSettings {
                units,
                line_numbers,
                ..PlasmaSettings::default()
            };
            let plan = sheet_cut::plan(&sheet(), [300.0, 200.0], &settings);
            let text = sheet_cut::post_grbl_plasma(
                &plan,
                &settings,
                "Sheet of four parts",
                &["Kerf 1.5 mm".to_owned()],
            )
            .unwrap();
            // The round holes as G3 (cut counter-clockwise, the part on the
            // right), the rounded corners as G2.
            let words = |code: &str| {
                text.lines()
                    .filter(|l| l.split_whitespace().any(|w| w == code))
                    .count()
            };
            if words("G3") != 8 || words("G2") != 4 {
                failures.push(format!(
                    "{name}: {} G2 and {} G3 lines, wanted 4 and 8",
                    words("G2"),
                    words("G3")
                ));
            }
            let r = run(&sim, name, &text);
            if let Err(e) = check_lines(&r, name) {
                failures.push(e);
            }
            if r.holds != 0 {
                failures.push(format!("{name}: {} holds", r.holds));
            }
            let rounding = match units {
                Units::Millimetres => 0.0005,
                Units::Inches => 0.00005 * 25.4,
            };
            let tol = rounding + r.resolution;
            // Every move's end reached in order.
            let moves = sheet_cut::moves(&plan, &settings);
            let wanted: Vec<P3> = moves.iter().map(|m| m.to()).collect();
            let mut at = 0;
            for w in &wanted {
                match r.path[at..].iter().position(|p| near(*p, *w, tol)) {
                    Some(k) => at += k,
                    None => {
                        failures.push(format!("{name}: {w:?} not reached after block {at}"));
                        break;
                    }
                }
            }
            // The program's path, its arcs within 0.0005 mm, from the
            // travel height above the work zero.
            let mut dense: Vec<P3> = vec![[0.0, 0.0, settings.travel_height]];
            for m in &moves {
                let from = *dense.last().unwrap();
                dense.extend(move_points(from, m, 0.0005));
            }
            // Every point of grbl's path on it: grbl splits an arc into
            // chords whose ends lie on its arc. The program rounds each
            // number on its own, in X, Y and Z at once (in inches the cut
            // height alone lies 1.14 µm off), so the bound is a distance:
            // Z one rounding off; in X and Y grbl's arc has its centre at
            // the written start plus the written I and J and its radius
            // their length: the centre off by the start's and the I and
            // J's rounding, the radius by the I and J's again, each up to
            // the rounding times the square root of two. Together at most
            // sqrt((3 * sqrt(2))^2 + 1) = sqrt(19) roundings.
            let on = tol * 19f64.sqrt() + 0.0005 + 1e-6;
            for p in &r.path {
                let off = dense
                    .windows(2)
                    .map(|w| segment_distance(*p, w[0], w[1]))
                    .fold(f64::INFINITY, f64::min);
                if off > on {
                    failures.push(format!(
                        "{name}: {p:?} lies {off} mm off the program's path"
                    ));
                    break;
                }
            }
            // The extents (an arc's extremes on it) and the end; grbl's
            // chords may fall short of an extreme by its arc tolerance.
            for k in 0..3 {
                let lo = |v: &[P3]| v.iter().map(|p| p[k]).fold(f64::INFINITY, f64::min);
                let hi = |v: &[P3]| v.iter().map(|p| p[k]).fold(f64::NEG_INFINITY, f64::max);
                let short = tol + 0.002 + 0.0005;
                let (got_lo, got_hi) = (lo(&r.path), hi(&r.path));
                let (want_lo, want_hi) = (lo(&dense), hi(&dense));
                if got_lo < want_lo - tol
                    || got_lo > want_lo + short
                    || got_hi > want_hi + tol
                    || got_hi < want_hi - short
                {
                    failures.push(format!(
                        "{name}: axis {k} runs from {got_lo} to {got_hi}, the program from {want_lo} to {want_hi}"
                    ));
                }
            }
            match r.end {
                Some(end) if near(end, [0.0, 0.0, settings.travel_height], tol) => {}
                end => failures.push(format!("{name}: ends at {end:?}")),
            }
        }
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }
}
