// SPDX-License-Identifier: MIT
//! Lofted Flange (mitcad#2512): a transition made as sheet between two
//! sketch profiles, its body, its sheet state and its Flat Pattern.
//!
//! - **Profiles.** A region of a sketch (its outer boundary) or curves of
//!   a sketch joined end to end; both closed or both open. They lie on the
//!   sheet's front; the material lies the rules' thickness behind it,
//!   outwards of a closed transition (Flip: inwards), along the normals of
//!   an open one.
//! - **Press brake.** Profiles in parallel planes that turn one way are
//!   joined where they run the same way; the regions about corners and
//!   arcs are cut into flat facets (`facets` each, default 4, or as few as
//!   keep the chords within `chord_tolerance`). Other profiles (on angled
//!   planes, or not convex) are cut into polylines the same way and joined
//!   by triangles across the shorter diagonals. Facets meet at folds, the
//!   bend lines of the Flat Pattern with their angles.
//! - **Die formed.** Profiles in parallel planes that turn one way are
//!   joined by the smooth developable surface (planes and cones; corners
//!   meeting corners fold). Other profiles have no such surface: they are
//!   refused in words, or with `development: triangulated` joined by a
//!   ruled surface laid flat approximately by triangles (mitcad#2560).
//! - **Bends** (press brake, mitcad#2560): each fold a cylinder of the
//!   rules' bend radius between the facets, cut back to it.
//! - **Seam.** A closed transition opens along a ruling, by default the
//!   shortest, or the one through the point at the `seam` fraction of the
//!   first profile; both profiles lose half the `gap` (default the
//!   thickness) each side of it.
//! - **Body.** The front and the back of every region, the walls along
//!   both profiles and at the seam (`geometry::patches`), sewn into one
//!   solid; flat regions are exact planes behind each other, folds meet on
//!   their mitres.
//! - **Sheet.** One panel whose chart is the development of the neutral
//!   surface (K t behind the front, the rules' K-factor): exact for flat
//!   regions, within well under a micrometre for curved ones. The folds and
//!   the lines where flat regions meet formed ones are kept with the chart
//!   ([`LoftedEmbedding::lines`]) for the Flat Pattern; its frame is the
//!   plane touching the first ruling.
//!
//! The chart's curves are generated curves `g<1400000+>` of the feature.
//! Holes and cuts by ordinary features are taken into the chart
//! (`lofted_cut.rs`, mitcad#2560); other sheet metal features on the sheet
//! are not taken yet (refused in words).

use mitcad_vec3::{add, cross, dot, norm, scale, sub};

use super::*;
use crate::features::extrude::profile_regions;
use crate::features::sheet_contour::rules_of;
use crate::features::sheet_convert::SheetDevelopment;
use crate::features::sheet_lofted::{LoftedMethod, LoftedProfile, SheetLoftedFlangeDef};
use crate::kernel::PatchBodySpec;
use crate::notice::NoticePlace;

#[path = "lofted_geometry.rs"]
mod geometry;
use geometry::{Chain, RegionError, Strip, Unmatched, V3};

// Holes and cuts in the sheet by ordinary features (mitcad#2560).
#[path = "lofted_cut.rs"]
mod cut;
pub(crate) use cut::{Refused, rederived};

/// The chart's curves.
const CHART_CURVES: u32 = 1_400_000;

/// The sewing tolerance of the body's patches (mm).
const SEWING: f64 = 1e-5;

/// A profile read from its sketch, and the place that paints it.
struct Profile {
    chain: Chain,
    place: NoticePlace,
}

/// A box around points, as a place.
fn box_of(points: &[V3]) -> NoticePlace {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for p in points {
        for (i, v) in p.iter().enumerate() {
            min[i] = min[i].min(*v);
            max[i] = max[i].max(*v);
        }
    }
    NoticePlace::Box { min, max }
}

/// A refusal in words with the places it is about.
fn placed_at(places: Vec<NoticePlace>, said: impl Into<String>) -> String {
    let said = said.into();
    crate::notice::FeatureError::new(said.clone())
        .with_places(places)
        .with_detail(said)
        .text()
}

fn region_refusal(e: RegionError) -> String {
    placed_at(vec![box_of(&e.points)], e.message)
}

fn profile<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    p: &LoftedProfile,
    what: &str,
) -> Result<Profile, String> {
    if let Some(reference) = p.region_ref() {
        let (sketch, regions) = profile_regions(ctx, std::slice::from_ref(&reference))?;
        let place = NoticePlace::region_of(p.sketch, reference.region.to_string());
        let region = match regions.as_slice() {
            [one] => one,
            _ => {
                return Err(placed_at(
                    vec![place],
                    format!(
                        "the {what} profile's region was found as {} regions: pick one region",
                        regions.len()
                    ),
                ));
            }
        };
        let outer = region
            .loops
            .first()
            .ok_or_else(|| format!("the {what} profile's region has no boundary"))?;
        let pieces = outer
            .segments
            .iter()
            .map(|s| geometry::piece_of_segment(&s.geometry, &sketch.frame))
            .collect::<Result<Vec<_>, _>>()?;
        if region.loops.len() > 1 {
            ctx.warn(format!(
                "the {what} profile's region has holes: its outer boundary is the profile"
            ));
        }
        let chain = geometry::chain(pieces, sketch.frame.normal(), what)
            .map_err(|e| placed_at(vec![place.clone()], e))?;
        return Ok(Profile { chain, place });
    }
    let sketch = ctx.sketch(p.sketch)?;
    if sketch.developed.is_some() {
        return Err(format!(
            "the {what} profile's sketch lies on a curved face: a lofted flange's profiles lie \
             in sketch planes"
        ));
    }
    let mut pieces = Vec::new();
    for id in &p.curves {
        let curve = crate::features::sketch::curve_of(&sketch.solved, *id).ok_or_else(|| {
            format!(
                "sketch curve {} of the {what} profile is unavailable",
                id.curve_name()
            )
        })?;
        pieces.push(geometry::piece_of_curve(curve, &sketch.frame));
    }
    let points: Vec<V3> = pieces.iter().flat_map(|p| p.samples(16)).collect();
    let place = box_of(&points);
    let chain = geometry::chain(pieces, sketch.frame.normal(), what)
        .map_err(|e| placed_at(vec![place.clone()], e))?;
    Ok(Profile { chain, place })
}

/// The press brake's transition of profiles that are not matched: their
/// polylines joined by triangles, a closed one opened at its seam.
fn triangulated(
    first: &Chain,
    second: &Chain,
    count: Option<usize>,
    tolerance: Option<f64>,
    seam: Option<f64>,
    gap: f64,
) -> Result<Vec<Strip>, String> {
    let pa = geometry::polyline(first, count, tolerance);
    let pb = geometry::polyline(second, count, tolerance);
    if !first.closed {
        return Ok(geometry::triangulated(&pa, &pb));
    }
    let (at_a, at_b) = match seam {
        Some(f) => {
            let at_a = f * geometry::polyline_length(&pa, true);
            let point = geometry::polyline_from(&pa, at_a)[0];
            (at_a, geometry::nearest_on(&pb, point).0)
        }
        None => {
            // The shortest line from a corner of the first polyline.
            let mut best = (0.0, 0.0, f64::INFINITY);
            let mut walked = 0.0;
            for (i, p) in pa.iter().enumerate() {
                let (at_b, d) = geometry::nearest_on(&pb, *p);
                if d < best.2 - 1e-9 {
                    best = (walked, at_b, d);
                }
                walked += mitcad_vec3::distance(*p, pa[(i + 1) % pa.len()]);
            }
            (best.0, best.1)
        }
    };
    let strips = geometry::triangulated(
        &geometry::polyline_from(&pa, at_a),
        &geometry::polyline_from(&pb, at_b),
    );
    let half = gap / 2.0;
    geometry::trimmed_end(&geometry::trimmed_start(&strips, half, half)?, half, half)
}

/// The strips of the transition as the method makes them.
fn strips(
    d: &SheetLoftedFlangeDef<f64>,
    first: &Profile,
    second: &Chain,
    second_place: &NoticePlace,
    gap: f64,
) -> Result<Vec<Strip>, String> {
    let count = d.facets.map(|n| n.round().max(1.0) as usize);
    let tolerance = d.chord_tolerance;
    let closed = first.chain.closed;
    match geometry::matched(&first.chain, second) {
        Ok(smooth) => {
            let open = if closed {
                let at = d.seam.map(|f| f * first.chain.length());
                let (strip, s) = geometry::seam_place(&smooth, at);
                let half = gap / 2.0;
                let opened = geometry::opened_at(&smooth, strip, s);
                // Bends cannot meet in the tip of a cone's facets beside
                // the seam (mitcad#2560): two arcs are cut at one angle.
                let fill = !d.bends;
                geometry::trimmed_end_with(
                    &geometry::trimmed_start_with(&opened, half, half, fill)?,
                    half,
                    half,
                    fill,
                )?
            } else {
                smooth
            };
            Ok(match d.method {
                LoftedMethod::DieFormed => open,
                LoftedMethod::PressBrake => geometry::faceted(&open, count, tolerance),
            })
        }
        Err(Unmatched(why)) => match d.method {
            LoftedMethod::DieFormed => Err(placed_at(
                vec![first.place.clone(), second_place.clone()],
                format!(
                    "a die formed transition is a smooth surface that lies flat without \
                     stretching, and these profiles have none: {why}; choose Press Brake to make \
                     it of flat facets, or the Approximate development to lay a smooth surface \
                     flat by triangulation"
                ),
            )),
            LoftedMethod::PressBrake => {
                triangulated(&first.chain, second, count, tolerance, d.seam, gap)
            }
        },
    }
}

pub(crate) fn lofted_flange<K: Kernel>(
    d: &SheetLoftedFlangeDef<f64>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let (rules, rules_version) = rules_of(ctx, d.rules)?;
    let t = rules.thickness;
    let k = super::curved::rolled_k(&rules)?;
    let [first_def, second_def] = d.profiles.as_slice() else {
        return Err(format!(
            "a lofted flange runs between two profiles, got {}",
            d.profiles.len()
        ));
    };
    let first = profile(ctx, first_def, "first")?;
    let second = profile(ctx, second_def, "second")?;
    let second_chain = geometry::aligned(&first.chain, second.chain)
        .map_err(|e| placed_at(vec![first.place.clone(), second.place.clone()], e))?;
    let gap = d.gap.unwrap_or(t);
    let closed = first.chain.closed;
    // Die formed between profiles without a developable surface, laid flat
    // by triangulation when asked (mitcad#2560).
    if d.method == LoftedMethod::DieFormed && d.development == Some(SheetDevelopment::Triangulated)
    {
        if geometry::matched(&first.chain, &second_chain).is_ok() {
            ctx.inform(
                "the profiles have a smooth surface that lies flat exactly between them: it is \
                 laid flat exactly, without the triangulation",
            );
        } else {
            return triangulated_die_formed(
                d,
                ctx,
                (&first, &second_chain, &second.place),
                (&rules, rules_version.as_str()),
                gap,
            );
        }
    }
    let strips = strips(d, &first, &second_chain, &second.place, gap)?;
    let mut transition = geometry::transition(strips, closed, d.flip).map_err(region_refusal)?;
    // Press brake folds as bends of the rules' radius (mitcad#2560).
    let mut bends = Vec::new();
    if d.bends && d.method == LoftedMethod::PressBrake && rules.bend_radius > TOLERANCE {
        let plane = |c: &geometry::Chain| (c.pieces[0].start(), c.normal);
        let (bent, made) = geometry::bent(
            &transition,
            rules.bend_radius,
            t,
            k,
            [plane(&first.chain), plane(&second_chain)],
        )
        .map_err(region_refusal)?;
        transition = bent;
        bends = made;
    }
    transition.check_depth(t).map_err(region_refusal)?;
    let neutral = k * t;
    let developed = geometry::developed(&transition, neutral).map_err(region_refusal)?;
    if geometry::overlaps(&geometry::outline_points(&developed)) {
        return Err(placed_at(
            vec![first.place.clone(), second.place.clone()],
            "the transition's blank would overlap itself when laid flat: move the seam, or make \
             the transition of two pieces",
        ));
    }
    let uid = ctx.uid;
    let mut region = super::region(uid, CHART_CURVES, geometry::outline(&developed))?;
    super::develop::orient(&mut region)?;
    let boundaries = geometry::boundary_rulings(&transition, &developed);
    let mut lines = Vec::new();
    for (i, join) in transition.joins.iter().enumerate() {
        let (a, b) = boundaries[i];
        // A bend (mitcad#2560): its centre line and its tangent lines, once,
        // where it starts.
        if transition.strips[i].bend() || transition.strips[i + 1].bend() {
            if let Some(bend) = bends.iter().find(|b| b.strip == i + 1)
                && let Some((centre, tangents)) =
                    geometry::bend_lines(&transition, &developed, i + 1)
            {
                lines.push(LoftedLine {
                    start: centre[0],
                    end: centre[1],
                    fold: true,
                    angle: bend.join.angle,
                    inside_front: bend.join.inside_front,
                    radius: bend.radius,
                    tangents: tangents.to_vec(),
                });
            }
            continue;
        }
        // A flat region meeting a formed one.
        let formed = transition.strips[i].flat() != transition.strips[i + 1].flat();
        if join.angle != 0.0 {
            lines.push(LoftedLine {
                start: a,
                end: b,
                fold: true,
                angle: join.angle,
                inside_front: join.inside_front,
                radius: 0.0,
                tangents: Vec::new(),
            });
        } else if formed {
            lines.push(LoftedLine {
                start: a,
                end: b,
                fold: false,
                angle: 0.0,
                inside_front: false,
                radius: 0.0,
                tangents: Vec::new(),
            });
        }
    }
    let rulings = geometry::neutral_rulings(&transition, &developed, neutral)
        .into_iter()
        .map(|(model, chart, front_to_back, depth)| LoftedRuling {
            model,
            chart,
            front_to_back,
            depth: ((depth - neutral).abs() > 1e-12).then_some(depth),
        })
        .collect();
    let patches = geometry::patches(&transition, t, uid);
    let shape = ctx
        .kernel
        .patch_body(&PatchBodySpec {
            patches: &patches,
            closed: true,
            tolerance: SEWING,
        })
        .map_err(|e| {
            placed_at(
                vec![first.place.clone(), second.place.clone()],
                format!(
                    "the lofted flange's body could not be built: {}",
                    crate::notice::user_message(&e.to_string())
                ),
            )
        })?;
    let owner = ctx
        .env
        .features
        .iter()
        .find(|f| f.uid == uid)
        .ok_or("the lofted flange is not in the timeline")?
        .component;
    let state = SheetState {
        schema: 2,
        algorithm: ALGORITHM_VERSION,
        body: BodyUid::new(uid, 0),
        owner,
        shape_generation: String::new(),
        rules: d.rules,
        rules_version,
        panels: Vec::new(),
        charts: vec![SheetPatch {
            domain: SheetDomainRef::Panel(PanelId {
                feature: uid,
                slot: 0,
            }),
            regions: vec![region],
            embedding: SheetEmbedding::Lofted(LoftedEmbedding {
                rulings,
                lines,
                neutral_depth: neutral,
                k_factor: k,
                die_formed: d.method == LoftedMethod::DieFormed,
            }),
        }],
        bends: Vec::new(),
        joins: Vec::new(),
        opened: Vec::new(),
        lineage: Vec::new(),
        overlays: Vec::new(),
        gaps: Vec::new(),
        invalidated: None,
        blockers: Vec::new(),
        approximation: None,
    };
    let mut output = FeatureOutput::default();
    finish(&mut output, state, &rules, shape)?;
    Ok(output)
}

/// The steps of a ruled band's grid from the first profile to the second.
const ACROSS: usize = 16;

/// A die formed transition between profiles without a developable surface
/// (mitcad#2560): the smooth ruled surface joining the points at the same
/// share of both profiles' lengths ([`geometry::ruled_transition`]), made
/// solid the thickness behind it, and laid flat approximately by
/// triangulation (mitcad#2442's [`super::triangulated::lay_flat`]) at the
/// neutral surface, saying how far the pattern's lengths are from the
/// surface's. The sheet is the triangulated pattern's, as Convert to Sheet
/// Metal makes it: sheet features refuse it and ordinary features take
/// its sheet meaning.
fn triangulated_die_formed<K: Kernel>(
    d: &SheetLoftedFlangeDef<f64>,
    ctx: &mut EvalContext<'_, K>,
    (first, second, second_place): (&Profile, &Chain, &NoticePlace),
    (rules, rules_version): (&SheetRulesDef<f64>, &str),
    gap: f64,
) -> Result<FeatureOutput<K::Shape>, String> {
    use super::triangulated::{FINE, Orientation, Oriented, lay_flat};
    let t = rules.thickness;
    let k = super::curved::rolled_k(rules)?;
    let places = || vec![first.place.clone(), second_place.clone()];
    let closed = first.chain.closed;
    let bands = geometry::ruled_transition(&first.chain, second, d.seam, gap)
        .map_err(|e| placed_at(places(), e))?;
    let normal = |band: &geometry::RuledBand, f: f64, v: f64| {
        band.normal(f, v).ok_or_else(|| {
            placed_at(
                places(),
                "the profiles meet here: a lofted flange needs room between them",
            )
        })
    };
    for band in &bands {
        for f in [0.0, 0.5, 1.0] {
            for v in [0.0, 0.5, 1.0] {
                normal(band, f, v)?;
            }
        }
    }
    // The material outwards of a closed transition, along the normals of
    // an open one; Flip: the other way.
    let mid = &bands[bands.len() / 2];
    let mut side = 1.0;
    if closed {
        let points: Vec<V3> = bands
            .iter()
            .flat_map(|b| [b.point(0.5, 0.0), b.point(0.5, 1.0)])
            .collect();
        let centre = scale(
            points.iter().fold([0.0; 3], |acc, p| add(acc, *p)),
            1.0 / points.len() as f64,
        );
        if dot(normal(mid, 0.5, 0.5)?, sub(mid.point(0.5, 0.5), centre)) < 0.0 {
            side = -1.0;
        }
    }
    if d.flip {
        side = -side;
    }
    // Each band a face of the strip: its grid of the front, the normals
    // out of the body (against the material), the triangles shared out by
    // the bands' lengths.
    let rail = |band: &geometry::RuledBand| {
        let n = 64;
        (0..n)
            .map(|i| {
                let (f0, f1) = (i as f64 / n as f64, (i + 1) as f64 / n as f64);
                (mitcad_vec3::distance(band.a.point(f0), band.a.point(f1))
                    + mitcad_vec3::distance(band.b.point(f0), band.b.point(f1)))
                    / 2.0
            })
            .sum::<f64>()
    };
    let lengths: Vec<f64> = bands.iter().map(rail).collect();
    let total = d
        .triangles
        .unwrap_or(crate::features::sheet_convert::DEFAULT_TRIANGLES);
    let shares = super::triangulated::strips(&lengths, (total / 2.0).round().max(1.0) as usize);
    let mut grids = Vec::with_capacity(bands.len());
    for (band, &count) in bands.iter().zip(&shares) {
        let grid = geometry::ruled_grid(band, count * FINE, ACROSS, side).ok_or_else(|| {
            placed_at(
                places(),
                "the profiles meet here: a lofted flange needs room between them",
            )
        })?;
        grids.push(grid);
    }
    let oriented: Vec<Oriented<'_>> = grids
        .iter()
        .map(|grid| Oriented {
            grid,
            orientation: Orientation {
                transpose: false,
                reverse_along: false,
                reverse_across: false,
            },
        })
        .collect();
    let layout = lay_flat(&oriented, k * t, false, 0.0).map_err(|e| {
        placed_at(
            places(),
            format!("the transition could not be laid flat by triangulation: {e}"),
        )
    })?;
    // The body: the front made solid the thickness behind it.
    let uid = ctx.uid;
    let patches = geometry::ruled_patches(&bands, uid);
    let built = |e: crate::kernel::KernelError| {
        placed_at(
            places(),
            format!(
                "the lofted flange's body could not be built: {}",
                crate::notice::user_message(&e.to_string())
            ),
        )
    };
    let surface = ctx
        .kernel
        .patch_body(&PatchBodySpec {
            patches: &patches,
            closed: false,
            tolerance: SEWING,
        })
        .map_err(built)?;
    let probe = add(
        mid.point(0.5, 0.5),
        scale(normal(mid, 0.5, 0.5)?, side * t / 2.0),
    );
    let mut shape = None;
    for thickness in [t, -t] {
        let Ok(solid) = ctx.kernel.thicken(uid, &surface, thickness, false) else {
            continue;
        };
        let inside = ctx.kernel.points_inside(&solid, &[probe]).map_err(built)?;
        if inside.first() == Some(&true) {
            shape = Some(solid);
            break;
        }
    }
    let shape = shape.ok_or_else(|| {
        placed_at(
            places(),
            "the lofted flange's body could not be built: its smooth surface could not be made \
             solid the thickness behind it",
        )
    })?;
    // The pattern on the plane touching the front at the strip's first
    // point, its first line along the chart's y axis, seen from the front.
    let first_band = &bands[0];
    let origin = first_band.point(0.0, 0.0);
    let n0 = scale(normal(first_band, 0.0, 0.0)?, -side);
    let rung = sub(first_band.point(0.0, 1.0), origin);
    let y = unit3(sub(rung, scale(n0, dot(rung, n0))))
        .ok_or("the transition's first line lies along its normal")?;
    let x = cross(y, n0);
    let mut region = super::polygon(uid, CHART_CURVES, &layout.outline)?;
    super::develop::orient(&mut region)?;
    ctx.inform(format!(
        "laid flat approximately by {} triangles: lengths along its edges differ from the \
         surface's by up to {:.3} mm, across it by up to {:.3} mm",
        layout.triangles, layout.edges, layout.across
    ));
    let owner = ctx
        .env
        .features
        .iter()
        .find(|f| f.uid == uid)
        .ok_or("the lofted flange is not in the timeline")?
        .component;
    let state = SheetState {
        schema: 2,
        algorithm: ALGORITHM_VERSION,
        body: BodyUid::new(uid, 0),
        owner,
        shape_generation: String::new(),
        rules: d.rules,
        rules_version: rules_version.to_owned(),
        panels: Vec::new(),
        charts: vec![SheetPatch {
            domain: SheetDomainRef::Panel(PanelId {
                feature: uid,
                slot: 0,
            }),
            regions: vec![region],
            embedding: SheetEmbedding::Plane {
                front_frame: SketchFrame {
                    origin,
                    x_axis: x,
                    y_axis: y,
                },
                front_to_back_sign: MaterialParity::Negative,
            },
        }],
        bends: Vec::new(),
        joins: Vec::new(),
        opened: Vec::new(),
        lineage: Vec::new(),
        overlays: Vec::new(),
        gaps: Vec::new(),
        invalidated: None,
        blockers: Vec::new(),
        approximation: Some(SheetApproximation {
            triangles: layout.triangles,
            edges: layout.edges,
            across: layout.across,
            seam: closed,
            folds: layout.folds,
        }),
    };
    let mut output = FeatureOutput::default();
    finish(&mut output, state, rules, shape)?;
    Ok(output)
}

/// The lofted flange's panel of a sheet, if it is one (mitcad#2512).
pub(crate) fn lofted_panel(state: &SheetState) -> Option<PanelId> {
    state
        .charts
        .iter()
        .find_map(|p| match (p.domain, &p.embedding) {
            (SheetDomainRef::Panel(id), SheetEmbedding::Lofted(_)) => Some(id),
            _ => None,
        })
}

fn unit2(v: [f64; 2]) -> Option<[f64; 2]> {
    let n = v[0].hypot(v[1]);
    (n > 1e-12).then(|| [v[0] / n, v[1] / n])
}

fn unit3(v: V3) -> Option<V3> {
    let n = norm(v);
    (n > 1e-12).then(|| scale(v, 1.0 / n))
}

/// The plane touching a lofted flange's front at its first ruling, as the
/// frame its chart's coordinates lie in there (chart point `(x, y)` at
/// `origin + x x_axis + y y_axis`), and which way the material lies.
pub(crate) fn tangent_frame(l: &LoftedEmbedding) -> Result<(SketchFrame, MaterialParity), String> {
    let none = || "the lofted flange's chart has no rulings".to_owned();
    let first = l.rulings.first().ok_or_else(none)?;
    let next = l.rulings.get(1).ok_or_else(none)?;
    let e = unit3(sub(first.model[1], first.model[0])).ok_or_else(none)?;
    let e2 = unit2([
        first.chart[1][0] - first.chart[0][0],
        first.chart[1][1] - first.chart[0][1],
    ])
    .ok_or_else(none)?;
    // Across the ruling: towards the next one, at whichever end moves.
    let end = if mitcad_vec3::distance(next.model[0], first.model[0]) > 1e-9 {
        0
    } else {
        1
    };
    let f = sub(next.model[end], first.model[end]);
    let f = unit3(sub(f, scale(e, dot(f, e)))).ok_or_else(none)?;
    let g = [
        next.chart[end][0] - first.chart[end][0],
        next.chart[end][1] - first.chart[end][1],
    ];
    let along = g[0] * e2[0] + g[1] * e2[1];
    let f2 = unit2([g[0] - along * e2[0], g[1] - along * e2[1]]).ok_or_else(none)?;
    // Chart vector v is (v.e2) e + (v.f2) f in the body.
    let x_axis = add(scale(e, e2[0]), scale(f, f2[0]));
    let y_axis = add(scale(e, e2[1]), scale(f, f2[1]));
    let front = sub(
        first.model[0],
        scale(first.front_to_back, ruling_depth(l, first)),
    );
    let at = first.chart[0];
    let frame = SketchFrame {
        origin: sub(front, add(scale(x_axis, at[0]), scale(y_axis, at[1]))),
        x_axis,
        y_axis,
    };
    let sign = if dot(first.front_to_back, cross(x_axis, y_axis)) > 0.0 {
        MaterialParity::Positive
    } else {
        MaterialParity::Negative
    };
    Ok((frame, sign))
}

/// The developed sheet of a lofted flange, for its Flat Pattern: its chart
/// (the development already) laid in the plane touching its first ruling.
/// None for another sheet.
pub(crate) fn developed(state: &SheetState, panel: PanelId) -> Option<Result<SheetState, String>> {
    let domain = SheetDomainRef::Panel(panel);
    let patch = state.charts.iter().find(|p| p.domain == domain)?;
    let SheetEmbedding::Lofted(l) = &patch.embedding else {
        return None;
    };
    Some(tangent_frame(l).map(|(front_frame, sign)| {
        let mut out = state.clone();
        out.charts = vec![SheetPatch {
            domain,
            regions: patch.regions.clone(),
            embedding: SheetEmbedding::Plane {
                front_frame,
                front_to_back_sign: sign,
            },
        }];
        out
    }))
}

/// A chart point of a lofted flange in the body, `depth` behind the front:
/// between the two rulings it lies between, as the triangles between them
/// map it (exact on flat facets).
pub(crate) fn chart_point(l: &LoftedEmbedding, p: [f64; 2], depth: f64) -> V3 {
    let cross2 = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    // Barycentric coordinates of p in the triangle (a, b, c).
    let inside = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| -> Option<[f64; 3]> {
        let area = cross2(a, b, c);
        if area.abs() <= 1e-18 {
            return None;
        }
        let u = cross2(p, b, c) / area;
        let v = cross2(a, p, c) / area;
        let w = 1.0 - u - v;
        (u >= -1e-9 && v >= -1e-9 && w >= -1e-9).then_some([u, v, w])
    };
    let mix =
        |w: [f64; 3], a: V3, b: V3, c: V3| add(add(scale(a, w[0]), scale(b, w[1])), scale(c, w[2]));
    for pair in l.rulings.windows(2) {
        let (r, s) = (&pair[0], &pair[1]);
        let [r0, r1, s0, s1] = pair_at(l, r, s, depth);
        if let Some(w) = inside(r.chart[0], r.chart[1], s.chart[0]) {
            return mix(w, r0, r1, s0);
        }
        if let Some(w) = inside(r.chart[1], s.chart[1], s.chart[0]) {
            return mix(w, r1, s1, s0);
        }
    }
    l.rulings.first().map_or([0.0; 3], |r| {
        add(
            r.model[0],
            scale(r.front_to_back, depth - ruling_depth(l, r)),
        )
    })
}

/// How far behind the front a ruling's model points lie: the neutral depth,
/// or a bend's own (mitcad#2560).
fn ruling_depth(l: &LoftedEmbedding, r: &LoftedRuling) -> f64 {
    r.depth.unwrap_or(l.neutral_depth)
}

/// Two rulings' ends `depth` behind the front, moved along the first one's
/// material direction (the region between them): `[r0, r1, s0, s1]`.
fn pair_at(l: &LoftedEmbedding, r: &LoftedRuling, s: &LoftedRuling, depth: f64) -> [V3; 4] {
    let f = r.front_to_back;
    let (dr, ds) = (depth - ruling_depth(l, r), depth - ruling_depth(l, s));
    [
        add(r.model[0], scale(f, dr)),
        add(r.model[1], scale(f, dr)),
        add(s.model[0], scale(f, ds)),
        add(s.model[1], scale(f, ds)),
    ]
}

/// A body point located on a lofted flange's chart (mitcad#2560).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Located {
    /// Its chart coordinates (its foot on the surface, laid flat).
    pub chart: [f64; 2],
    /// How far behind the front it lies.
    pub depth: f64,
    /// The material direction there.
    pub front_to_back: V3,
    /// The front's plane there as a frame that maps it into the chart
    /// exactly (`chart = ((p - origin) . x_axis, (p - origin) . y_axis)`;
    /// on a flat facet the whole facet's).
    pub frame: SketchFrame,
}

/// The chart coordinates and depth of a point of a lofted flange's body:
/// on the triangle between two rulings at the front over which it lies
/// (within `slack` beside it) nearest to `target` behind it, mapped into
/// the chart as that triangle lies there (exact on flat facets, the
/// rulings' triangles elsewhere). None away from the transition.
pub(crate) fn located(l: &LoftedEmbedding, x: V3, target: f64, slack: f64) -> Option<Located> {
    let mut best: Option<(f64, Located)> = None;
    for pair in l.rulings.windows(2) {
        let (r, s) = (&pair[0], &pair[1]);
        let [r0, r1, s0, s1] = pair_at(l, r, s, 0.0);
        for (m, c) in [
            ([r0, r1, s0], [r.chart[0], r.chart[1], s.chart[0]]),
            ([r1, s1, s0], [r.chart[1], s.chart[1], s.chart[0]]),
        ] {
            let Some((frame, normal)) = triangle_frame(m, c, r.front_to_back) else {
                continue;
            };
            let d = sub(x, frame.origin);
            let p = [dot(d, frame.x_axis), dot(d, frame.y_axis)];
            let beside = outside(p, c);
            if beside > slack {
                continue;
            }
            let depth = dot(sub(x, m[0]), normal);
            let score = (depth - target).abs() + beside;
            if best.as_ref().is_none_or(|(b, _)| score < *b) {
                best = Some((
                    score,
                    Located {
                        chart: p,
                        depth,
                        front_to_back: normal,
                        frame,
                    },
                ));
            }
        }
    }
    best.map(|(_, at)| at)
}

/// The frame mapping a triangle's plane (`m`) onto its chart triangle
/// (`c`), and its unit normal towards `towards` (the material direction).
fn triangle_frame(m: [V3; 3], c: [[f64; 2]; 3], towards: V3) -> Option<(SketchFrame, V3)> {
    let (e1, e2) = (sub(m[1], m[0]), sub(m[2], m[0]));
    let normal = unit3(cross(e1, e2))?;
    let normal = if dot(normal, towards) < 0.0 {
        scale(normal, -1.0)
    } else {
        normal
    };
    let (g1, g2) = (
        [c[1][0] - c[0][0], c[1][1] - c[0][1]],
        [c[2][0] - c[0][0], c[2][1] - c[0][1]],
    );
    // x = α e1 + β e2 with e1 . x = g1[0] and e2 . x = g2[0]; y alike.
    let (a11, a12, a22) = (dot(e1, e1), dot(e1, e2), dot(e2, e2));
    let det = a11 * a22 - a12 * a12;
    if det.abs() <= 1e-18 * a11.max(a22).powi(2).max(1e-300) {
        return None;
    }
    let solve = |b1: f64, b2: f64| {
        let alpha = (b1 * a22 - b2 * a12) / det;
        let beta = (a11 * b2 - a12 * b1) / det;
        add(scale(e1, alpha), scale(e2, beta))
    };
    let x_axis = solve(g1[0], g2[0]);
    let y_axis = solve(g1[1], g2[1]);
    // The origin's offset o from m[0] in the plane: o . x = c0[0], o . y
    // = c0[1] (x and y are at right angles and of unit length where the
    // triangle lies flat unchanged, as on a facet).
    let (b11, b12, b22) = (
        dot(x_axis, x_axis),
        dot(x_axis, y_axis),
        dot(y_axis, y_axis),
    );
    let det2 = b11 * b22 - b12 * b12;
    if det2.abs() <= 1e-18 {
        return None;
    }
    let gamma = (c[0][0] * b22 - c[0][1] * b12) / det2;
    let delta = (b11 * c[0][1] - b12 * c[0][0]) / det2;
    let origin = sub(m[0], add(scale(x_axis, gamma), scale(y_axis, delta)));
    Some((
        SketchFrame {
            origin,
            x_axis,
            y_axis,
        },
        normal,
    ))
}

/// How far a chart point lies outside a chart triangle (0 inside): its
/// distance from the nearest side it lies beyond.
fn outside(p: [f64; 2], c: [[f64; 2]; 3]) -> f64 {
    let cross2 = |a: [f64; 2], b: [f64; 2], q: [f64; 2]| {
        (b[0] - a[0]) * (q[1] - a[1]) - (b[1] - a[1]) * (q[0] - a[0])
    };
    let turn = cross2(c[0], c[1], c[2]).signum();
    let mut most: f64 = 0.0;
    for i in 0..3 {
        let (a, b) = (c[i], c[(i + 1) % 3]);
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        if len <= 1e-12 {
            continue;
        }
        let side = turn * cross2(a, b, p) / len;
        most = most.max(-side);
    }
    most
}
