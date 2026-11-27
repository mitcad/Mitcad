// SPDX-License-Identifier: MIT
//! Approximate flat patterns by triangulation (mitcad#2442): a cylinder's
//! face laid flat into its rectangle of chords, a square-to-round
//! transition's face and a rectangle-to-round transition with an offset
//! (four faces given in any order and way round, closed and opened at a
//! seam), each pattern's edge lengths against the faces' within the
//! deviation it states, which is the one measured; the strip found through
//! the faces, its fold lines, and more triangles giving smaller deviations.
//! Since mitcad#2558: the deviation of the transition 1 mm behind its faces
//! not rising from one triangle count to the next, a round hole and a face
//! cut out of its surface laid out with the strip (their lengths within the
//! stated deviation, the fold lines only on the material), and fans: one
//! whose rails meet where it starts, one from a point. Since mitcad#2574: a
//! strip whose rails meet between its ends split there into two fans laid
//! side by side, a square-to-round built of planes and cones laid flat in
//! one strip (the planes' grids built from their edges), and the outline of
//! a strip mixing a face cut out of its surface with an ordinary face; the
//! edges' lengths against the faces' within the stated deviation.

use super::*;

const PI: f64 = std::f64::consts::PI;

/// The unit normal of `surface` at `(u, v)`, `cross(d/du, d/dv)` (`out`)
/// or against it; where the surface has none (a fan's tip), the normal a
/// little way towards the middle, as the kernel gives it.
fn normal_of(surface: &dyn Fn(f64, f64) -> V3, u: f64, v: f64, out: bool) -> V3 {
    let h = 1e-6;
    for step in [0.0, 1e-4, 1e-3, 1e-2] {
        let (u, v) = (u + (0.5 - u) * step, v + (0.5 - v) * step);
        let (u0, u1) = ((u - h).max(0.0), (u + h).min(1.0));
        let (v0, v1) = ((v - h).max(0.0), (v + h).min(1.0));
        let n = cross(
            sub(surface(u1, v), surface(u0, v)),
            sub(surface(u, v1), surface(u, v0)),
        );
        let size = norm(n);
        if size > 1e-18 {
            let n = scale(n, 1.0 / size);
            return if out { n } else { scale(n, -1.0) };
        }
    }
    panic!("no normal at ({u}, {v})");
}

/// The grid of `surface` over `[0, 1]²` in `columns` by `rows` steps, the
/// normals `cross(d/du, d/dv)` (`out`) or against it, the area its own.
fn sampled(
    surface: &dyn Fn(f64, f64) -> V3,
    columns: usize,
    rows: usize,
    out: bool,
) -> SheetFaceGrid {
    let mut points = Vec::new();
    let mut normals = Vec::new();
    for j in 0..=rows {
        for i in 0..=columns {
            let (u, v) = (i as f64 / columns as f64, j as f64 / rows as f64);
            points.push(surface(u, v));
            normals.push(normal_of(surface, u, v, out));
        }
    }
    let mut grid = SheetFaceGrid {
        columns,
        rows,
        points,
        normals,
        area: 0.0,
        wires: Vec::new(),
    };
    grid.area = grid_area(&grid, 0.0);
    grid
}

/// An edge of a face's boundary: `path(t)` the surface's parameters for
/// `t` from 0 to 1, at EDGE_STEPS steps.
fn boundary_edge(
    surface: &dyn Fn(f64, f64) -> V3,
    path: &dyn Fn(f64) -> [f64; 2],
    [columns, rows]: [usize; 2],
    out: bool,
) -> SheetFaceEdge {
    let mut edge = SheetFaceEdge {
        cells: Vec::new(),
        points: Vec::new(),
        normals: Vec::new(),
    };
    for k in 0..=EDGE_STEPS {
        let [u, v] = path(k as f64 / EDGE_STEPS as f64);
        edge.cells.push([u * columns as f64, v * rows as f64]);
        edge.points.push(surface(u, v));
        edge.normals.push(normal_of(surface, u, v, out));
    }
    edge
}

/// The outer wire of a face filling its grid: its four sides.
fn whole_outline(surface: &dyn Fn(f64, f64) -> V3, size: [usize; 2], out: bool) -> SheetFaceWire {
    let sides: [&dyn Fn(f64) -> [f64; 2]; 4] =
        [&|t| [t, 0.0], &|t| [1.0, t], &|t| [1.0 - t, 1.0], &|t| {
            [0.0, 1.0 - t]
        }];
    SheetFaceWire {
        outer: true,
        edges: sides
            .iter()
            .map(|side| boundary_edge(surface, *side, size, out))
            .collect(),
    }
}

/// The area of a flat polygon.
fn polygon_area(points: &[[f64; 2]]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        .abs()
        / 2.0
}

/// A closed polygon's perimeter.
fn perimeter(points: &[[f64; 2]]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| flat_distance(points[i], points[(i + 1) % n]))
        .sum()
}

fn lerp(a: V3, b: V3, t: f64) -> V3 {
    add(a, scale(sub(b, a), t))
}

const R: f64 = 100.0;
const H: f64 = 50.0;

/// A quarter of a cylinder of radius R about z, H high, seen from outside.
fn cylinder(u: f64, v: f64) -> V3 {
    let a = u * PI / 2.0;
    [R * a.cos(), R * a.sin(), H * v]
}

/// The flat length of the strip's rail `k` (0 or 1) over stations
/// `s0..=s1`.
fn rail(layout: &Layout, (s0, s1): (usize, usize), k: usize) -> f64 {
    (s0..s1)
        .map(|s| flat_distance(layout.flat[2 * s + k], layout.flat[2 * s + 2 + k]))
        .sum()
}

fn oriented(grid: &SheetFaceGrid, orientation: Orientation) -> Oriented<'_> {
    Oriented { grid, orientation }
}

#[test]
fn a_cylinders_face_lies_flat_as_its_rectangle_of_chords() {
    let probe = sampled(&cylinder, PROBE, PROBE, true);
    let chain = chain(std::slice::from_ref(&probe)).expect("one face");
    assert!(!chain.closed);
    let (_, o) = chain.faces[0];
    // The rungs run along the straight lines of the cylinder.
    assert!(!o.transpose);
    let strips = 16;
    let grid = sampled(&cylinder, strips * FINE, ACROSS, true);
    let layout = lay_flat(&[oriented(&grid, o)], 0.0, false, 0.0).expect("laid flat");
    assert_eq!(layout.triangles, 2 * strips as u32);
    let chord = 2.0 * R * (PI / 4.0 / strips as f64).sin();
    for s in 0..=strips {
        // A rectangle: its first rail along x, its rungs H long along y.
        let (b, t) = (layout.flat[2 * s], layout.flat[2 * s + 1]);
        assert!((b[0] - s as f64 * chord).abs() <= 1e-9, "{b:?}");
        assert!(b[1].abs() <= 1e-9);
        assert!((t[0] - b[0]).abs() <= 1e-9 && (t[1] - H).abs() <= 1e-9);
    }
    // The rails are the arcs' chords: the deviation is the chords' error,
    // which the pattern states as measured; the lines between them are
    // arcs alike, the quadrilaterals plane.
    let error = R * PI / 2.0 - strips as f64 * chord;
    assert!(
        (layout.edges - error).abs() <= 1e-6,
        "{} {error}",
        layout.edges
    );
    assert!((layout.across - error).abs() <= 1e-6, "{}", layout.across);
    // Folded away from the front at every rung between: a sixteenth of the
    // quarter turn each.
    assert_eq!(layout.folds.len(), strips - 1);
    for fold in &layout.folds {
        assert!(
            (fold.angle + PI / 2.0 / strips as f64).abs() <= 1e-6,
            "{fold:?}"
        );
        assert!((fold.start[0] - fold.end[0]).abs() <= 1e-9);
    }
}

const A: f64 = 50.0;
const RO: f64 = 30.0;
const HT: f64 = 60.0;

/// A quarter of a square-to-round transition: from the square's side y =
/// -A at z = 0 to the circle's quarter round the corners' angles, radius
/// RO at z = HT; seen from outside.
fn square_to_round(u: f64, v: f64) -> V3 {
    let bottom = [-A + 2.0 * A * u, -A, 0.0];
    let angle = -0.75 * PI + u * PI / 2.0;
    let top = [RO * angle.cos(), RO * angle.sin(), HT];
    lerp(bottom, top, v)
}

#[test]
fn a_square_to_round_face_states_the_deviation_it_measures() {
    let probe = sampled(&square_to_round, PROBE, PROBE, true);
    let chain = chain(std::slice::from_ref(&probe)).expect("one face");
    let (_, o) = chain.faces[0];
    assert!(!o.transpose, "the rungs run along the straight lines");
    let lay = |strips: usize| {
        let grid = sampled(&square_to_round, strips * FINE, ACROSS, true);
        lay_flat(&[oriented(&grid, o)], 0.0, false, 0.0).expect("laid flat")
    };
    let layout = lay(32);
    // The pattern's edges against the face's: the square's side, the
    // circle's quarter and the two straight end rungs.
    let span = layout.spans[0];
    let end = |s: usize| flat_distance(layout.flat[2 * s], layout.flat[2 * s + 1]);
    let rung = |u: f64| distance(square_to_round(u, 0.0), square_to_round(u, 1.0));
    let errors = [
        (rail(&layout, span, 0) - 2.0 * A).abs(),
        (rail(&layout, span, 1) - RO * PI / 2.0).abs(),
        (end(0) - rung(0.0)).abs(),
        (end(32) - rung(1.0)).abs(),
    ];
    let measured = errors.into_iter().fold(0.0, f64::max);
    assert!(
        (layout.edges - measured).abs() <= 1e-6,
        "{} {measured}",
        layout.edges
    );
    assert!(layout.across > 0.0 && layout.across.is_finite());
    // More triangles, closer.
    let coarse = lay(4);
    assert!(
        coarse.edges > layout.edges,
        "{} {}",
        coarse.edges,
        layout.edges
    );
    // The same face parameterized the other way round gives the same
    // pattern.
    let swapped = |u: f64, v: f64| square_to_round(v, u);
    let probe = sampled(&swapped, PROBE, PROBE, false);
    let chain = super::chain(std::slice::from_ref(&probe)).expect("one face");
    let (_, t) = chain.faces[0];
    assert!(t.transpose);
    let grid = sampled(&swapped, ACROSS, 32 * FINE, false);
    let again = lay_flat(&[oriented(&grid, t)], 0.0, false, 0.0).expect("laid flat");
    assert!((again.edges - layout.edges).abs() <= 1e-9);
    assert!((again.across - layout.across).abs() <= 1e-9);
}

const HALF_X: f64 = 60.0;
const HALF_Y: f64 = 40.0;
const CENTRE: V3 = [10.0, 5.0, 80.0];

/// Face `k` of a rectangle-to-round transition with an offset: from the
/// rectangle's side `k` (counter-clockwise from its corner at -x, -y) at z
/// = 0 to the circle's quarter round the corners' angles about CENTRE.
fn transition(k: usize, u: f64, v: f64) -> V3 {
    let corners = [
        [-HALF_X, -HALF_Y, 0.0],
        [HALF_X, -HALF_Y, 0.0],
        [HALF_X, HALF_Y, 0.0],
        [-HALF_X, HALF_Y, 0.0],
    ];
    let bottom = lerp(corners[k], corners[(k + 1) % 4], u);
    let angle = -0.75 * PI + (k as f64 + u) * PI / 2.0;
    let top = add(CENTRE, [RO * angle.cos(), RO * angle.sin(), 0.0]);
    lerp(bottom, top, v)
}

#[test]
fn a_rectangle_to_round_transition_with_an_offset_opens_at_its_seam() {
    // The faces in another order and each its own way round: the picked
    // face, the third transposed, the fourth reversed along, the second
    // reversed across.
    type Surface = Box<dyn Fn(f64, f64) -> V3>;
    let surfaces: [(Surface, bool); 4] = [
        (Box::new(|u, v| transition(0, u, v)), true),
        (Box::new(|u, v| transition(2, v, u)), false),
        (Box::new(|u, v| transition(3, 1.0 - u, v)), false),
        (Box::new(|u, v| transition(1, u, 1.0 - v)), false),
    ];
    let probes: Vec<SheetFaceGrid> = surfaces
        .iter()
        .map(|(f, out)| sampled(f.as_ref(), PROBE, PROBE, *out))
        .collect();
    let chain = chain(&probes).expect("one strip");
    assert!(chain.closed);
    let order: Vec<usize> = chain.faces.iter().map(|(m, _)| *m).collect();
    assert_eq!(order, [0, 3, 1, 2]);
    let strips = 24;
    let grids: Vec<SheetFaceGrid> = chain
        .faces
        .iter()
        .map(|&(m, o)| {
            let (f, out) = &surfaces[m];
            if o.transpose {
                sampled(f.as_ref(), ACROSS, strips * FINE, *out)
            } else {
                sampled(f.as_ref(), strips * FINE, ACROSS, *out)
            }
        })
        .collect();
    let faces: Vec<Oriented<'_>> = grids
        .iter()
        .zip(&chain.faces)
        .map(|(g, &(_, o))| oriented(g, o))
        .collect();
    let gap = 1.0;
    let layout = lay_flat(&faces, 0.0, true, gap).expect("laid flat");
    assert_eq!(layout.spans.len(), 4);
    // Each face's rails against the rectangle's side and the circle's
    // quarter; the rungs are straight on the faces and laid exactly.
    let sides = [2.0 * HALF_X, 2.0 * HALF_Y, 2.0 * HALF_X, 2.0 * HALF_Y];
    let mut measured: f64 = 0.0;
    for (&span, &(m, _)) in layout.spans.iter().zip(&chain.faces) {
        let k = [0, 2, 3, 1][m];
        let bottom = (rail(&layout, span, 0) - sides[k]).abs();
        let top = (rail(&layout, span, 1) - RO * PI / 2.0).abs();
        assert!(
            bottom <= 1e-9,
            "a straight side lies flat at its length: {bottom}"
        );
        measured = measured.max(top);
    }
    assert!(
        (layout.edges - measured).abs() <= 1e-6,
        "{} {measured}",
        layout.edges
    );
    // Opened at the seam: half the gap off each end of each rail.
    let n = layout.flat.len() / 2 - 1;
    assert!((flat_distance(layout.outline[0], layout.flat[0]) - gap / 2.0).abs() <= 1e-9);
    let last_bottom = layout.outline[n];
    assert!((flat_distance(last_bottom, layout.flat[2 * n]) - gap / 2.0).abs() <= 1e-9);
    assert!(!crosses_itself(&layout.outline));
    // Folded away from the front where the faces meet at the rectangle's
    // corners.
    let creases = layout.folds.iter().filter(|f| f.angle < -0.1).count();
    assert!(creases >= 3, "{:?}", layout.folds);
    assert!(layout.folds.iter().all(|f| f.angle.abs() >= FOLD));
}

#[test]
fn faces_that_do_not_join_are_named() {
    let quarter = sampled(&square_to_round, PROBE, PROBE, true);
    let away = sampled(
        &|u, v| add(square_to_round(u, v), [500.0, 0.0, 0.0]),
        PROBE,
        PROBE,
        true,
    );
    let Err((m, _)) = chain(&[quarter.clone(), away.clone()]) else {
        panic!("faces apart make no strip");
    };
    assert_eq!(m, 1);
    // A third face joining the first does not take in the one apart.
    let next = sampled(&|u, v| transition(1, u, v), PROBE, PROBE, true);
    let first = sampled(&|u, v| transition(0, u, v), PROBE, PROBE, true);
    let Err((m, _)) = chain(&[first, next, away]) else {
        panic!("a face apart joins no strip");
    };
    assert_eq!(m, 2);
}

#[test]
fn triangles_are_shared_by_the_rails_lengths() {
    assert_eq!(strips(&[1.0, 3.0], 8), [2, 6]);
    assert_eq!(strips(&[1.0, 100.0], 4), [1, 4]);
    assert_eq!(strips(&[0.0, 0.0], 6), [3, 3]);
}

#[test]
fn a_curves_length_is_its_polylines_less_the_chords_error() {
    let points: Vec<V3> = (0..=8)
        .map(|i| {
            let a = f64::from(i) * PI / 16.0;
            [a.cos(), a.sin(), 0.0]
        })
        .collect();
    let exact = PI / 2.0;
    let polyline = length(points.iter().copied());
    let better = curve_length(&points, 2);
    assert!((better - exact).abs() < (polyline - exact).abs() / 50.0);
}

#[test]
fn outlines_that_cross_themselves_are_found() {
    let square = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    assert!(!crosses_itself(&square));
    let bow_tie = [[0.0, 0.0], [1.0, 1.0], [1.0, 0.0], [0.0, 1.0]];
    assert!(crosses_itself(&bow_tie));
}

#[test]
fn a_grids_area_is_its_surfaces() {
    let grid = sampled(&cylinder, 128, 4, true);
    assert!((grid_area(&grid, 0.0) / (R * PI / 2.0 * H) - 1.0).abs() < 1e-4);
    // Behind the outer face, towards the axis, the surface is smaller.
    assert!(grid_area(&grid, 2.0) < grid_area(&grid, 0.0));
}

/// The rectangle-to-round transition with an offset, its four faces in the
/// order the strip runs, laid flat `depth` behind them in `strips` strips
/// shared out by the rails' lengths as the convert shares them.
fn transition_layout(strips: usize, depth: f64) -> Layout {
    let probes: Vec<SheetFaceGrid> = (0..4)
        .map(|k| sampled(&|u, v| transition(k, u, v), PROBE, PROBE, true))
        .collect();
    let chain = chain(&probes).expect("one strip");
    assert!(chain.closed);
    let lengths: Vec<f64> = chain
        .faces
        .iter()
        .map(|&(m, o)| rail_length(oriented(&probes[m], o)))
        .collect();
    let shares = super::strips(&lengths, strips);
    let grids: Vec<SheetFaceGrid> = chain
        .faces
        .iter()
        .zip(&shares)
        .map(|(&(m, o), &count)| {
            let surface = |u: f64, v: f64| transition(m, u, v);
            if o.transpose {
                sampled(&surface, ACROSS, count * FINE, true)
            } else {
                sampled(&surface, count * FINE, ACROSS, true)
            }
        })
        .collect();
    let faces: Vec<Oriented<'_>> = grids
        .iter()
        .zip(&chain.faces)
        .map(|(g, &(_, o))| oriented(g, o))
        .collect();
    lay_flat(&faces, depth, true, 1.0).expect("laid flat")
}

/// How much the edges' deviation may grow from one triangle count to the
/// next (mm): the faces' lengths are measured anew at each count.
const MARGIN: f64 = 0.002;

#[test]
fn more_triangles_do_not_give_a_worse_pattern() {
    // 24, 48, 64, 98 and 128 triangles, 1 mm behind the faces, which meet
    // at right angles at the rectangle's corners: there each face's points
    // behind the shared rung lie apart (mitcad#2558). Moving only the
    // shared rung to their mean put that difference into the face's last
    // strip, shorter with more triangles, and the edges' deviation rose
    // with the count; shared out along the face it falls steadily.
    //
    // The deviation across is another matter (mitcad#2585): the faces are
    // not developable (straight lines from a rectangle's side to a quarter
    // circle at the same share of each, twisted), so a pattern that keeps
    // the rails and the rungs stretches the lines between the rails. Its
    // largest part is the middle line of a long side, about 1.38 mm longer
    // laid flat than on the face; fewer, wider triangles hide part of that
    // (they cut across the twist, and the pattern's line through few
    // points is short), so `across` climbs towards the surface's own
    // stretch: 1.129, 1.346, 1.364, 1.380 and 1.384 mm. It cannot fall
    // with the count; it settles, no rise larger than the one before
    // (within the margin), and no count states more than the finest.
    let mut before: Option<(usize, Layout)> = None;
    let mut rise = f64::INFINITY;
    let mut across = Vec::new();
    for strips in [12, 24, 32, 49, 64] {
        let layout = transition_layout(strips, 1.0);
        assert!(layout.edges.is_finite() && layout.across.is_finite());
        if let Some((fewer, earlier)) = &before {
            assert!(
                layout.edges <= earlier.edges + MARGIN,
                "{} triangles: edges {} after {} at {}",
                2 * strips,
                layout.edges,
                earlier.edges,
                2 * fewer
            );
            let now = layout.across - earlier.across;
            assert!(
                now <= rise + MARGIN,
                "{} triangles: across rose by {now} after a rise of {rise} at {}",
                2 * strips,
                2 * fewer
            );
            rise = now.max(0.0);
        }
        across.push(layout.across);
        before = Some((strips, layout));
    }
    let first = transition_layout(12, 1.0);
    let (_, last) = before.expect("laid flat");
    assert!(last.edges < first.edges, "{} {}", last.edges, first.edges);
    assert!(
        across.iter().all(|a| *a <= last.across + MARGIN),
        "{across:?}"
    );
}

#[test]
fn a_hole_is_laid_out_with_the_strip() {
    let strips = 16;
    let size = [strips * FINE, ACROSS];
    let mut grid = sampled(&cylinder, size[0], size[1], true);
    // A round hole on the developed face, 10 mm about the middle of its
    // arc, half way up.
    let (centre, radius) = (R * PI / 4.0, 10.0);
    let circle = |t: f64| {
        let a = 2.0 * PI * t;
        [
            (centre + radius * a.cos()) / (R * PI / 2.0),
            (H / 2.0 + radius * a.sin()) / H,
        ]
    };
    grid.wires = vec![
        whole_outline(&cylinder, size, true),
        SheetFaceWire {
            outer: false,
            edges: vec![boundary_edge(&cylinder, &circle, size, true)],
        },
    ];
    let probe = sampled(&cylinder, PROBE, PROBE, true);
    let (_, o) = chain(std::slice::from_ref(&probe)).expect("one face").faces[0];
    assert!(!oriented(&grid, o).cut_out());
    let layout = lay_flat(&[oriented(&grid, o)], 0.0, false, 0.0).expect("laid flat");
    assert_eq!(layout.holes.len(), 1);
    let hole = &layout.holes[0];
    // Its perimeter against the circle's (the cylinder develops exactly)
    // within the deviation the pattern states; it is as high as the circle
    // (the rungs are straight up), about as wide.
    let off = (perimeter(hole) - 2.0 * PI * radius).abs();
    assert!(off <= layout.edges + 1e-4, "{off} {}", layout.edges);
    assert!(layout.edges < 0.1, "{}", layout.edges);
    let ys = hole.iter().map(|p| p[1]);
    let (low, high) = ys.fold((f64::MAX, f64::MIN), |(l, h), y| (l.min(y), h.max(y)));
    assert!((high - low - 2.0 * radius).abs() < 1e-6, "{low} {high}");
    let xs = hole.iter().map(|p| p[0]);
    let (left, right) = xs.fold((f64::MAX, f64::MIN), |(l, h), x| (l.min(x), h.max(x)));
    assert!((right - left - 2.0 * radius).abs() < 0.05, "{left} {right}");
    // The outline is the face's rectangle of chords, the hole inside it.
    assert_eq!(layout.outline.len(), 2 * (strips + 1));
    assert!(!loops_cross(hole, &layout.outline));
    assert!(inside(hole[0], &[layout.outline.as_slice()]));
    // No fold line runs through the hole: the rungs across it fold on
    // each side of it.
    for fold in &layout.folds {
        let middle = [
            (fold.start[0] + fold.end[0]) / 2.0,
            (fold.start[1] + fold.end[1]) / 2.0,
        ];
        assert!(!inside(middle, &[hole.as_slice()]), "{fold:?}");
    }
    assert!(layout.folds.len() > strips - 1, "{}", layout.folds.len());
}

#[test]
fn a_face_cut_out_of_its_surface_is_laid_out_by_its_edges() {
    let strips = 16;
    let size = [strips * FINE, ACROSS];
    let mut grid = sampled(&cylinder, size[0], size[1], true);
    // The part of the cylinder's face below the helix from its first
    // bottom corner to its last top corner: the bottom, the last rung and
    // the helix bound it.
    grid.wires = vec![SheetFaceWire {
        outer: true,
        edges: vec![
            boundary_edge(&cylinder, &|t| [t, 0.0], size, true),
            boundary_edge(&cylinder, &|t| [1.0, t], size, true),
            boundary_edge(&cylinder, &|t| [1.0 - t, 1.0 - t], size, true),
        ],
    }];
    let o = Orientation {
        transpose: false,
        reverse_along: false,
        reverse_across: false,
    };
    assert!(oriented(&grid, o).cut_out());
    let layout = lay_flat(&[oriented(&grid, o)], 0.0, false, 0.0).expect("laid flat");
    assert!(layout.holes.is_empty());
    assert!(!crosses_itself(&layout.outline));
    // A triangle: the bottom's chords, the last rung, and the helix laid
    // out across, its length against the helix's within the deviation
    // stated.
    let chords = strips as f64 * 2.0 * R * (PI / 4.0 / strips as f64).sin();
    let half = chords * H / 2.0;
    let area = polygon_area(&layout.outline);
    assert!((area - half).abs() < 0.01 * half, "{area} {half}");
    let helix = (R * PI / 2.0).hypot(H);
    let laid = perimeter(&layout.outline) - chords - H;
    let off = (laid - helix).abs();
    assert!(off <= layout.edges + 1e-4, "{off} {}", layout.edges);
    assert!(layout.edges < 0.2, "{}", layout.edges);
    // Fold lines only on the face: each rung below the helix.
    assert_eq!(layout.folds.len(), strips - 1);
    for fold in &layout.folds {
        let middle = [
            (fold.start[0] + fold.end[0]) / 2.0,
            (fold.start[1] + fold.end[1]) / 2.0,
        ];
        assert!(inside(middle, &[layout.outline.as_slice()]), "{fold:?}");
        assert!(fold.start[1].min(fold.end[1]).abs() <= 1e-9, "{fold:?}");
        assert!(fold.start[1].max(fold.end[1]) < H - 1.0, "{fold:?}");
    }
}

const RF: f64 = 60.0;

/// A fan whose rails meet where it starts: an arc in z = 0 and a slanting
/// line, both from the origin, joined by straight lines.
fn fan(u: f64, v: f64) -> V3 {
    let a = u * PI / 3.0;
    let arc = [RF * a.sin(), RF * (1.0 - a.cos()), 0.0];
    let line = [80.0 * u, 0.0, 40.0 * u];
    lerp(arc, line, v)
}

#[test]
fn a_fan_whose_edges_meet_where_it_starts_lies_flat() {
    let probe = sampled(&fan, PROBE, PROBE, true);
    let (_, o) = chain(std::slice::from_ref(&probe)).expect("one face").faces[0];
    assert!(!o.transpose, "the rungs run along the straight lines");
    let strips = 16;
    let grid = sampled(&fan, strips * FINE, ACROSS, true);
    let layout = lay_flat(&[oriented(&grid, o)], 0.0, false, 0.0).expect("laid flat");
    // The first rung is a point: the first quadrilateral one triangle,
    // the pattern's y axis to the next rung's second-rail end.
    assert_eq!(layout.axis, 3);
    assert_eq!(layout.flat[0], layout.flat[1]);
    assert_eq!(layout.triangles, 2 * strips as u32 - 1);
    // The arc lies flat as its chords, the line and the last rung at
    // their lengths; the deviation stated is the chords' error.
    let theta = PI / 3.0;
    let error = RF * (theta - 2.0 * strips as f64 * (theta / (2.0 * strips as f64)).sin());
    let span = layout.spans[0];
    assert!((RF * theta - rail(&layout, span, 0) - error).abs() < 1e-9);
    assert!((rail(&layout, span, 1) - 80.0_f64.hypot(40.0)).abs() < 1e-9);
    let end = flat_distance(layout.flat[2 * strips], layout.flat[2 * strips + 1]);
    assert!((end - distance(fan(1.0, 0.0), fan(1.0, 1.0))).abs() < 1e-9);
    assert!(
        (layout.edges - error).abs() <= 1e-6,
        "{} {error}",
        layout.edges
    );
    assert_eq!(layout.outline.len(), 2 * strips + 1);
    assert!(!crosses_itself(&layout.outline));
}

/// A fan from a point: the corner of a square-to-round transition's
/// square to the circle's quarter about it.
fn corner_fan(u: f64, v: f64) -> V3 {
    let angle = -0.75 * PI + u * PI / 2.0;
    let top = [RO * angle.cos(), RO * angle.sin(), HT];
    lerp([A, -A, 0.0], top, v)
}

#[test]
fn a_fan_from_a_point_lies_flat_as_its_sector_of_chords() {
    let probe = sampled(&corner_fan, PROBE, PROBE, true);
    let (_, o) = chain(std::slice::from_ref(&probe)).expect("one face").faces[0];
    assert!(!o.transpose, "the rungs run from the point");
    let strips = 16;
    let grid = sampled(&corner_fan, strips * FINE, ACROSS, true);
    let layout = lay_flat(&[oriented(&grid, o)], 0.0, false, 0.0).expect("laid flat");
    assert_eq!(layout.axis, 1);
    // Every rung from the point at its length, the point one place; the
    // deviation the arc's chords' error.
    for s in 0..=strips {
        assert!(flat_distance(layout.flat[2 * s], layout.flat[0]) < 1e-9);
        let u = s as f64 / strips as f64;
        let rung = distance(corner_fan(u, 0.0), corner_fan(u, 1.0));
        let laid = flat_distance(layout.flat[2 * s], layout.flat[2 * s + 1]);
        assert!((laid - rung).abs() < 1e-9, "{laid} {rung}");
    }
    let error = RO * (PI / 2.0 - 2.0 * strips as f64 * (PI / 4.0 / strips as f64).sin());
    assert!(
        (layout.edges - error).abs() <= 1e-6,
        "{} {error}",
        layout.edges
    );
    assert!(!crosses_itself(&layout.outline));
}

/// The first of two fans whose rails meet at the origin, between the
/// strip's ends (mitcad#2574): a bump in z = 0 and a line falling to the
/// origin.
fn first_fan(u: f64, v: f64) -> V3 {
    let bump = [-80.0 + 80.0 * u, -15.0 * (PI * u).sin(), 0.0];
    let line = [-80.0 + 80.0 * u, 0.0, 60.0 * (1.0 - u)];
    lerp(bump, line, v)
}

/// The second: a bump the other way and a line rising from the origin.
fn second_fan(u: f64, v: f64) -> V3 {
    let bump = [80.0 * u, 15.0 * (PI * u).sin(), 0.0];
    let line = [80.0 * u, 0.0, 60.0 * u];
    lerp(bump, line, v)
}

/// The length of the curve `f` over `[0, 1]`, finely sampled.
fn dense_length(f: &dyn Fn(f64) -> V3) -> f64 {
    let n = 20_000;
    (0..n)
        .map(|i| distance(f(i as f64 / n as f64), f((i + 1) as f64 / n as f64)))
        .sum()
}

#[test]
fn rails_meeting_between_the_ends_split_the_strip_into_two_fans() {
    type Surface = fn(f64, f64) -> V3;
    let surfaces: [Surface; 2] = [first_fan, second_fan];
    let probes: Vec<SheetFaceGrid> = surfaces
        .iter()
        .map(|f| sampled(f, PROBE, PROBE, true))
        .collect();
    let chain = chain(&probes).expect("one strip");
    assert!(!chain.closed);
    let order: Vec<usize> = chain.faces.iter().map(|(m, _)| *m).collect();
    assert_eq!(order, [0, 1]);
    // The rung they share is a point, which every way across the second
    // face matches: the strip keeps its rails on their sides, as the faces'
    // fronts show them, also with the second face's parameter across them
    // the other way round.
    assert!(!chain.faces[1].1.reverse_across);
    let flipped = sampled(&|u, v| second_fan(u, 1.0 - v), PROBE, PROBE, false);
    let turned = super::chain(&[probes[0].clone(), flipped]).expect("one strip");
    assert!(turned.faces[1].1.reverse_across, "{:?}", turned.faces);
    let strips = 16;
    let grids: Vec<SheetFaceGrid> = chain
        .faces
        .iter()
        .map(|&(m, o)| {
            if o.transpose {
                sampled(&surfaces[m], ACROSS, strips * FINE, true)
            } else {
                sampled(&surfaces[m], strips * FINE, ACROSS, true)
            }
        })
        .collect();
    let faces: Vec<Oriented<'_>> = grids
        .iter()
        .zip(&chain.faces)
        .map(|(g, &(_, o))| oriented(g, o))
        .collect();
    let layout = lay_flat(&faces, 0.0, false, 0.0).expect("laid flat");
    // Split where the first fan ends: its rung there one point, the second
    // fan laid from it as one triangle and then on.
    assert_eq!(layout.pinches, [strips]);
    let p = layout.flat[2 * strips];
    assert!(flat_distance(p, layout.flat[2 * strips + 1]) < 1e-9);
    assert_eq!(layout.triangles, 4 * strips as u32 - 2);
    // Side by side: the second fan's corner at the point opens away from
    // the first's.
    let from_p = |v: usize| [layout.flat[v][0] - p[0], layout.flat[v][1] - p[1]];
    let before = [from_p(2 * strips - 2), from_p(2 * strips - 1)];
    let after = [from_p(2 * strips + 2), from_p(2 * strips + 3)];
    let middle = |pair: [[f64; 2]; 2]| {
        let (u, w) = (
            flat_unit(pair[0]).expect("apart"),
            flat_unit(pair[1]).expect("apart"),
        );
        [u[0] + w[0], u[1] + w[1]]
    };
    let m = middle(before);
    let dot2 = |a: [f64; 2], b: [f64; 2]| a[0] * b[0] + a[1] * b[1];
    assert!(dot2(middle(after), m) < 0.0, "{before:?} {after:?}");
    assert!(before.iter().all(|q| dot2(*q, m) > 0.0), "{before:?}");
    assert!(after.iter().all(|q| dot2(*q, m) < 0.0), "{after:?}");
    // Each fan's edges against the faces' within the deviation stated,
    // which is the one measured on both; the end rungs at their lengths.
    let lengths = [
        [
            dense_length(&|u| first_fan(u, 0.0)),
            dense_length(&|u| first_fan(u, 1.0)),
        ],
        [
            dense_length(&|u| second_fan(u, 0.0)),
            dense_length(&|u| second_fan(u, 1.0)),
        ],
    ];
    let mut measured: f64 = 0.0;
    for (f, (&span, faces_lengths)) in layout.spans.iter().zip(&lengths).enumerate() {
        for (k, length) in faces_lengths.iter().enumerate() {
            let off = (rail(&layout, span, k) - length).abs();
            assert!(
                off <= layout.edges + 1e-5,
                "face {f} rail {k}: {off} {}",
                layout.edges
            );
            measured = measured.max(off);
        }
    }
    assert!(
        (layout.edges - measured).abs() <= 1e-5,
        "{} {measured}",
        layout.edges
    );
    assert!(measured > 0.0 && layout.edges < 0.1, "{}", layout.edges);
    let n = 2 * strips;
    let rung = |a: V3, b: V3| distance(a, b);
    let start = flat_distance(layout.flat[0], layout.flat[1]);
    assert!((start - rung(first_fan(0.0, 0.0), first_fan(0.0, 1.0))).abs() < 1e-9);
    let end = flat_distance(layout.flat[2 * n], layout.flat[2 * n + 1]);
    assert!((end - rung(second_fan(1.0, 0.0), second_fan(1.0, 1.0))).abs() < 1e-9);
    // One outline through the point twice, touching itself there only;
    // both fans' areas add up (they run round the same way).
    let outline = &layout.outline;
    let at_p = outline
        .iter()
        .filter(|q| flat_distance(**q, p) < 1e-9)
        .count();
    assert_eq!(at_p, 2);
    assert!(crosses_itself(outline));
    assert!(!crosses_itself_but(outline, &[p]));
    let lobe = |stations: std::ops::RangeInclusive<usize>| {
        let mut points: Vec<[f64; 2]> = stations.clone().map(|s| layout.flat[2 * s]).collect();
        points.extend(stations.rev().map(|s| layout.flat[2 * s + 1]));
        polygon_area(&closed_loop(points))
    };
    let (first, second) = (lobe(0..=strips), lobe(strips..=n));
    let area = polygon_area(outline);
    assert!(
        (area - first - second).abs() <= 1e-6 * area,
        "{area} {first} {second}"
    );
}

/// The corner `k` of a square-to-round's square, 2 A wide, counter-clockwise
/// from (-A, -A).
fn square_corner(k: usize) -> V3 {
    let [x, y] = [[-A, -A], [A, -A], [A, A], [-A, A]][k % 4];
    [x, y, 0.0]
}

/// The point of its circle (radius RO, HT up) where the plane of the
/// square's side `k` touches it.
fn touch_point(k: usize) -> V3 {
    let a = -PI / 2.0 + k as f64 * PI / 2.0;
    [RO * a.cos(), RO * a.sin(), HT]
}

/// The cone of the square's corner `k`: from the corner to the circle
/// between the touch points of the sides before and after it, seen from
/// outside.
fn corner_cone(k: usize, u: f64, v: f64) -> V3 {
    let a = -PI / 2.0 + (k as f64 - 1.0 + u) * PI / 2.0;
    lerp(square_corner(k), [RO * a.cos(), RO * a.sin(), HT], v)
}

/// The plane of the square's side `k` as the kernel gives a plane: over the
/// rectangle of its own axes (turned 0.4 rad from the side) that holds the
/// side's triangle, which is cut out of it with its edges across it.
struct SidePlane {
    middle: V3,
    e: V3,
    f: V3,
    sin: f64,
    cos: f64,
    low: [f64; 2],
    high: [f64; 2],
}

impl SidePlane {
    fn new(k: usize) -> Self {
        let (a, b, p) = (square_corner(k), square_corner(k + 1), touch_point(k));
        let middle = scale(add(a, b), 0.5);
        let e = scale(sub(b, a), 1.0 / distance(a, b));
        let f = scale(sub(p, middle), 1.0 / distance(p, middle));
        let (sin, cos) = 0.4_f64.sin_cos();
        let mut plane = SidePlane {
            middle,
            e,
            f,
            sin,
            cos,
            low: [f64::MAX; 2],
            high: [f64::MIN; 2],
        };
        for q in [a, b, p] {
            let w = plane.axes(q);
            plane.low = [0, 1].map(|i| plane.low[i].min(w[i]));
            plane.high = [0, 1].map(|i| plane.high[i].max(w[i]));
        }
        plane
    }

    /// A point's coordinates along the plane's own axes.
    fn axes(&self, q: V3) -> [f64; 2] {
        let d = sub(q, self.middle);
        let (x, y) = (dot(d, self.e), dot(d, self.f));
        [x * self.cos + y * self.sin, -x * self.sin + y * self.cos]
    }

    /// The point at `(s, t)` of the rectangle, each from 0 to 1.
    fn point(&self, s: f64, t: f64) -> V3 {
        let xp = self.low[0] + s * (self.high[0] - self.low[0]);
        let yp = self.low[1] + t * (self.high[1] - self.low[1]);
        let (x, y) = (xp * self.cos - yp * self.sin, xp * self.sin + yp * self.cos);
        add(self.middle, add(scale(self.e, x), scale(self.f, y)))
    }

    /// The place of a point of the plane in the rectangle.
    fn place(&self, q: V3) -> [f64; 2] {
        let w = self.axes(q);
        [0, 1].map(|i| (w[i] - self.low[i]) / (self.high[i] - self.low[i]))
    }

    /// The face's grid as the kernel samples it, the triangle its outer
    /// wire: the side, the line to the touch point, the line back.
    fn grid(&self, k: usize, size: [usize; 2]) -> SheetFaceGrid {
        let surface = |s: f64, t: f64| self.point(s, t);
        let mut grid = sampled(&surface, size[0], size[1], true);
        let corners =
            [square_corner(k), square_corner(k + 1), touch_point(k)].map(|q| self.place(q));
        let edges = (0..3)
            .map(|i| {
                let (a, b) = (corners[i], corners[(i + 1) % 3]);
                let path = |t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                boundary_edge(&surface, &path, size, true)
            })
            .collect();
        grid.wires = vec![SheetFaceWire { outer: true, edges }];
        grid
    }
}

#[test]
fn a_square_to_round_of_planes_and_cones_lies_flat_in_one_strip() {
    // Its eight faces in another order, the picked side's plane first:
    // (a plane, the side or corner).
    let order: [(bool, usize); 8] = [
        (true, 0),
        (false, 2),
        (true, 1),
        (false, 0),
        (true, 3),
        (false, 1),
        (true, 2),
        (false, 3),
    ];
    let planes: Vec<SidePlane> = (0..4).map(SidePlane::new).collect();
    let sample = |(plane, k): (bool, usize), size: [usize; 2]| -> SheetFaceGrid {
        if plane {
            planes[k].grid(k, size)
        } else {
            sampled(&|u, v| corner_cone(k, u, v), size[0], size[1], true)
        }
    };
    // The planes' triangles are cut out of their grids, whose lines run
    // across their edges: no rung of a plane is one of the cones'.
    let first: Vec<SheetFaceGrid> = order.iter().map(|&f| sample(f, [PROBE, PROBE])).collect();
    assert!(chain(&first).is_err());
    // Their grids built from their edges, the rungs the edges they share
    // with the cones beside them, join the cones in one strip round.
    let rungs = plane_rungs(&first);
    for (&(plane, k), r) in order.iter().zip(&rungs) {
        assert_eq!(r.is_some(), plane, "face {k} of the planes {plane}");
    }
    let probes: Vec<SheetFaceGrid> = first
        .iter()
        .zip(&rungs)
        .map(|(g, r)| {
            r.and_then(|r| plane_grid(g, r, [PROBE, PROBE]))
                .unwrap_or_else(|| g.clone())
        })
        .collect();
    let chain = chain(&probes).expect("one strip");
    assert!(chain.closed);
    assert_eq!(chain.faces.len(), 8);
    for w in chain.faces.windows(2) {
        assert_ne!(order[w[0].0].0, order[w[1].0].0, "planes and cones in turn");
    }
    let lengths: Vec<f64> = chain
        .faces
        .iter()
        .map(|&(m, o)| rail_length(oriented(&probes[m], o)))
        .collect();
    let shares = strips(&lengths, 64);
    let grids: Vec<SheetFaceGrid> = chain
        .faces
        .iter()
        .zip(&shares)
        .map(|(&(m, o), &count)| {
            let size = if o.transpose {
                [ACROSS, count * FINE]
            } else {
                [count * FINE, ACROSS]
            };
            match rungs[m] {
                Some(r) => plane_grid(&first[m], r, size).expect("a plane's grid"),
                None => sample(order[m], size),
            }
        })
        .collect();
    let faces: Vec<Oriented<'_>> = grids
        .iter()
        .zip(&chain.faces)
        .map(|(g, &(_, o))| oriented(g, o))
        .collect();
    assert!(faces.iter().all(|f| !f.cut_out()));
    let layout = lay_flat(&faces, 0.0, true, 1.0).expect("laid flat");
    assert!(layout.pinches.is_empty());
    // The strip's first rail runs round the square, its second round the
    // circle: each plane's a side of the square and its touch point, each
    // cone's its corner and a quarter of the circle. The sides lie flat at
    // their lengths, the quarters as their chords: the deviation stated is
    // the chords' error, as measured.
    let mut measured: f64 = 0.0;
    for (&span, &(m, _)) in layout.spans.iter().zip(&chain.faces) {
        let (plane, k) = order[m];
        let expected = if plane {
            [2.0 * A, 0.0]
        } else {
            [0.0, RO * PI / 2.0]
        };
        for (rail_index, length) in expected.into_iter().enumerate() {
            let off = (rail(&layout, span, rail_index) - length).abs();
            if plane {
                assert!(off <= 1e-9, "side {k}, rail {rail_index}: {off}");
            }
            measured = measured.max(off);
        }
    }
    assert!(measured > 0.0);
    assert!(
        (layout.edges - measured).abs() <= 1e-4,
        "{} {measured}",
        layout.edges
    );
    assert!(layout.edges < 0.5, "{}", layout.edges);
    // The lines from the corners to the circle, where planes meet cones,
    // lie flat at their lengths.
    let lines: Vec<f64> = (0..4)
        .flat_map(|c| {
            [
                distance(square_corner(c), touch_point(c)),
                distance(square_corner(c + 1), touch_point(c)),
            ]
        })
        .collect();
    for &(s0, _) in &layout.spans {
        let laid = flat_distance(layout.flat[2 * s0], layout.flat[2 * s0 + 1]);
        assert!(
            lines.iter().any(|l| (l - laid).abs() < 1e-9),
            "{laid} {lines:?}"
        );
    }
    assert!(!crosses_itself(&layout.outline));
}

#[test]
fn a_strip_of_a_cut_out_face_and_an_ordinary_face_has_one_outline() {
    // The cylinder's quarter in two halves, one strip: the first whole, the
    // second cut along the line from the top of the rung they share to the
    // bottom of its far end (its rung shared, its bottom, that line).
    let strips = 8;
    let size = [strips * FINE, ACROSS];
    let first_half = |u: f64, v: f64| cylinder(0.5 * u, v);
    let second_half = |u: f64, v: f64| cylinder(0.5 + 0.5 * u, v);
    let whole = sampled(&first_half, size[0], size[1], true);
    let mut cut = sampled(&second_half, size[0], size[1], true);
    cut.wires = vec![SheetFaceWire {
        outer: true,
        edges: vec![
            boundary_edge(&second_half, &|t| [t, 0.0], size, true),
            boundary_edge(&second_half, &|t| [1.0 - t, t], size, true),
            boundary_edge(&second_half, &|t| [0.0, 1.0 - t], size, true),
        ],
    }];
    let o = Orientation {
        transpose: false,
        reverse_along: false,
        reverse_across: false,
    };
    let faces = [oriented(&whole, o), oriented(&cut, o)];
    assert!(!faces[0].cut_out() && faces[1].cut_out());
    let layout = lay_flat(&faces, 0.0, false, 0.0).expect("laid flat");
    assert!(layout.holes.is_empty());
    assert!(!crosses_itself(&layout.outline));
    // One outline joined from the whole half's first rung and rails, the
    // cut half's bottom and its line across: the whole half's rectangle of
    // chords and half the cut one's.
    let chords = strips as f64 * 2.0 * R * (PI / 8.0 / strips as f64).sin();
    let area = polygon_area(&layout.outline);
    let expected = 1.5 * chords * H;
    assert!(
        (area - expected).abs() < 0.01 * expected,
        "{area} {expected}"
    );
    // The line across laid at the helix's length within the stated
    // deviation.
    let helix = (R * PI / 4.0).hypot(H);
    let laid = perimeter(&layout.outline) - H - 3.0 * chords;
    assert!(
        (laid - helix).abs() <= layout.edges + 1e-4,
        "{laid} {helix} {}",
        layout.edges
    );
    assert!(layout.edges < 0.2, "{}", layout.edges);
    // Its corners: the whole half's first rung's ends, the cut half's far
    // bottom corner, and the top of the rung the halves share, where the
    // line across starts.
    let at = |s: usize, k: usize| layout.flat[2 * s + k];
    for corner in [at(0, 0), at(0, 1), at(2 * strips, 0), at(strips, 1)] {
        assert!(
            layout
                .outline
                .iter()
                .any(|q| flat_distance(*q, corner) < 1e-6),
            "{corner:?}"
        );
    }
    // Fold lines only on the material.
    assert!(!layout.folds.is_empty());
    for fold in &layout.folds {
        let middle = [
            (fold.start[0] + fold.end[0]) / 2.0,
            (fold.start[1] + fold.end[1]) / 2.0,
        ];
        assert!(inside(middle, &[layout.outline.as_slice()]), "{fold:?}");
    }
}
