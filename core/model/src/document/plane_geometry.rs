// SPDX-License-Identifier: MIT
//! Body edges and vertices in a sketch's plane (mitcad#256, mitcad#258):
//! what the sketch Dimension tool picks and the Point tool snaps to, in
//! sketch coordinates, each with what `sketch.project` needs to project
//! it. The kernel lists a body's edges and vertices and their curves
//! ([`Kernel::edges`], [`Kernel::curves_of`], [`Kernel::indexed_element`]);
//! whether they lie in the plane is decided here, within the model's linear
//! tolerance. On request the edges and vertices out of the plane come too
//! (mitcad#866), projected along its normal as `sketch.project` projects
//! them, with where they are above or below it. Only the bodies the view
//! shows count: those hidden by their light bulbs and those Isolate leaves
//! out are not looked at (mitcad#1702).

use std::collections::{BTreeMap, BTreeSet};

use mitcad_vec3::unit_div_above;

use super::{Document, ModelError, invalid};
use crate::features::FeatureDef;
use crate::fingerprint::Fingerprint;
use crate::ids::{BodyUid, FeatureUid, OccurrenceUid};
use crate::kernel::{BoundingBox, Curve3, ElementKind, Kernel};
use crate::links::OccurrenceLinkExt;
use crate::profile::SketchFrame;
use crate::recompute::{Change, Version, Versioned};
use crate::sketch::geometry::{Curve2, P2};
use crate::sketch::project::{Projected, project, transform_curve};
use crate::topo::TopoName;
use crate::transform::Transform;
use mitcad_model_base::curves::{bspline_point, conic_point, helix_point};
use mitcad_vec3::{cross, dot};

/// A body edge or vertex that lies in a sketch's plane.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaneItem {
    /// The edge's or the vertex's name: `sketch.project`'s `source`.
    pub source: TopoName,
    pub body: BodyUid,
    /// Where a body of another component was found: the occurrence path
    /// from the root to its component (`sketch.project`'s `occurrence`).
    /// None for the sketch's own component, which needs no link.
    pub occurrence: Option<Vec<OccurrenceUid>>,
    pub shape: PlaneShape,
    /// None for an item in the plane; for one out of it
    /// ([`Document::sketch_body_geometry`] with `out_of_plane`), where it
    /// is (mitcad#866).
    pub out_of_plane: Option<OutOfPlane>,
}

/// Where a body edge or vertex out of a sketch's plane is: what a client
/// needs to find it on the screen, where the view shows it, rather than its
/// projection.
#[derive(Debug, Clone, PartialEq)]
pub struct OutOfPlane {
    /// Its points in sketch coordinates with their height above the plane
    /// along its normal (negative below): a vertex's one point, an edge's
    /// points from its start to its end (a line's two ends, 24 steps of
    /// another curve).
    pub path: Vec<[f64; 3]>,
    /// The height of a vertex, of a conic's centre, else of the middle of
    /// the path.
    pub height: f64,
}

/// Where a [`PlaneItem`] is, in sketch coordinates: what projecting it
/// gives.
#[derive(Debug, Clone, PartialEq)]
pub enum PlaneShape {
    Vertex(P2),
    Curve(Curve2),
    /// An edge seen end on (a line along the plane's normal): the point it
    /// projects to, as `sketch.project` makes it (out of the plane only).
    Point(P2),
}

/// How far a model curve is from the plane through `origin` with the unit
/// `normal`, at most: zero for a curve in it.
fn distance_from_plane(curve: &Curve3, origin: [f64; 3], normal: [f64; 3]) -> f64 {
    let off = |p: [f64; 3]| dot(std::array::from_fn(|i| p[i] - origin[i]), normal).abs();
    match curve {
        Curve3::Point(p) => off(*p),
        Curve3::Line { start, end } => off(*start).max(off(*end)),
        // The whole conic: an arc of a circle that leaves the plane meets it
        // in two points only, so it lies in the plane only with its circle.
        Curve3::Conic {
            center,
            normal: axis,
            x_axis,
            major,
            minor,
            ..
        } => {
            let y_axis = cross(*axis, *x_axis);
            off(*center) + (major * dot(*x_axis, normal)).hypot(minor * dot(y_axis, normal))
        }
        // A B-spline lies in a plane exactly when its poles do (its basis
        // functions are independent; weights are positive).
        Curve3::BSpline { poles, .. } => poles.iter().map(|p| off(*p)).fold(0.0, f64::max),
        // A helix that rises leaves every plane (mitcad#385): its points.
        Curve3::Helix {
            center,
            axis,
            x_axis,
            radius,
            pitch,
            start,
            end,
        } => (0..=64)
            .map(|i| {
                let t = start + (end - start) * f64::from(i) / 64.0;
                off(crate::features::path::helix_point(
                    *center,
                    *axis,
                    *x_axis,
                    (*radius, *pitch),
                    t,
                ))
            })
            .fold(0.0, f64::max),
    }
}

/// Points along a model curve from its start to its end: a line's ends, 24
/// steps of a conic, a helix or a B-spline.
fn points_along(curve: &Curve3) -> Vec<[f64; 3]> {
    const STEPS: u32 = 24;
    let along = |f: &dyn Fn(f64) -> [f64; 3], t0: f64, t1: f64| -> Vec<[f64; 3]> {
        (0..=STEPS)
            .map(|k| f(t0 + (t1 - t0) * f64::from(k) / f64::from(STEPS)))
            .collect()
    };
    match curve {
        Curve3::Point(p) => vec![*p],
        Curve3::Line { start, end } => vec![*start, *end],
        Curve3::Conic {
            center,
            normal,
            x_axis,
            major,
            minor,
            start,
            end,
            closed,
        } => {
            let end = if *closed {
                start + std::f64::consts::TAU
            } else {
                *end
            };
            along(
                &|t| conic_point(*center, *normal, *x_axis, (*major, *minor), t),
                *start,
                end,
            )
        }
        Curve3::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => {
            let p = *degree as usize;
            if p == 0 || poles.len() <= p || knots.len() != poles.len() + p + 1 {
                return poles.clone();
            }
            along(
                &|t| bspline_point(p, poles, weights, knots, t),
                knots[p],
                knots[poles.len()],
            )
        }
        Curve3::Helix {
            center,
            axis,
            x_axis,
            radius,
            pitch,
            start,
            end,
        } => along(
            &|t| helix_point(*center, *axis, *x_axis, (*radius, *pitch), t),
            *start,
            *end,
        ),
    }
}

/// A model point in sketch coordinates with its height above the plane.
fn lifted(frame: &SketchFrame, normal: [f64; 3], p: [f64; 3]) -> [f64; 3] {
    let d: [f64; 3] = std::array::from_fn(|i| p[i] - frame.origin[i]);
    [dot(d, frame.x_axis), dot(d, frame.y_axis), dot(d, normal)]
}

/// Where an edge out of the plane is ([`OutOfPlane`]).
fn out_of_plane(curve: &Curve3, frame: &SketchFrame, normal: [f64; 3]) -> OutOfPlane {
    let path: Vec<[f64; 3]> = points_along(curve)
        .into_iter()
        .map(|p| lifted(frame, normal, p))
        .collect();
    let height = match curve {
        Curve3::Conic { center, .. } => lifted(frame, normal, *center)[2],
        _ => path.get(path.len() / 2).map_or(0.0, |p| p[2]),
    };
    OutOfPlane { path, height }
}

/// `v` divided by its length; `v` itself when it has none.
fn unit(v: [f64; 3]) -> [f64; 3] {
    unit_div_above(v, 0.0).unwrap_or(v)
}

/// Every named edge of a body with its curve: the kernel's
/// [`Kernel::named_edge_curves`] (by index, mitcad#1656), else each name of
/// [`Kernel::edges`] resolved by [`Kernel::curves_of`], as before it; an
/// edge whose name gives no single curve is left out either way.
fn named_edge_curves<K: Kernel>(kernel: &K, shape: &K::Shape) -> Vec<(TopoName, Curve3)> {
    let edges = match kernel.named_edge_curves(shape) {
        Ok(edges) => edges,
        Err(_) => kernel
            .edges(shape)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|edge| {
                let source = edge.name?.parse::<TopoName>().ok()?;
                if !matches!(source, TopoName::Edge(_)) {
                    return None;
                }
                let curves = kernel.curves_of(shape, &source).ok()?;
                let [curve] = <[Curve3; 1]>::try_from(curves).ok()?;
                Some((source, curve))
            })
            .collect(),
    };
    edges
        .into_iter()
        .filter(|(source, _)| matches!(source, TopoName::Edge(_)))
        .collect()
}

/// A body to look at: its shape in its component's coordinates (with the
/// version of the result that made it) and the transform into the
/// sketch's.
struct Seen<'a, S> {
    body: BodyUid,
    shape: &'a S,
    version: Version,
    occurrence: Option<Vec<OccurrenceUid>>,
    to_sketch: Option<Transform>,
}

/// The sketch's frame and the bodies the query looks at: everything its
/// answer is made from, with the kernel's edges, vertices and boxes of
/// those shapes.
struct Inputs<'a, S> {
    frame: SketchFrame,
    seen: Vec<Seen<'a, S>>,
}

impl<K: Kernel> Document<K> {
    /// The edges and vertices of the visible bodies before `sketch` that lie
    /// in its plane, in sketch coordinates (the `plane_geometry` query): an
    /// edge whose every point is in the plane within the model's linear
    /// tolerance (lines, arcs, circles, ellipses, splines), and every vertex
    /// in it. Bodies of other components count where an occurrence shows
    /// them, as `sketch.project` with `occurrence` takes them (mitcad#100),
    /// when the design policy lets the sketch reference them.
    pub fn plane_geometry(&self, sketch: FeatureUid) -> Result<Vec<PlaneItem>, ModelError> {
        self.sketch_body_geometry(sketch, false)
    }

    /// [`Document::plane_geometry`], and with `out_of_plane` also every
    /// other named edge and vertex of those bodies (mitcad#866), projected
    /// onto the plane along its normal as `sketch.project` projects them
    /// (a line along the normal to its point), each with
    /// [`PlaneItem::out_of_plane`]. The items in the plane come first, as
    /// without it.
    pub fn sketch_body_geometry(
        &self,
        sketch: FeatureUid,
        out_of_plane: bool,
    ) -> Result<Vec<PlaneItem>, ModelError> {
        let Inputs { frame, seen } = self.plane_inputs(sketch)?;
        let normal = unit(cross(frame.x_axis, frame.y_axis));
        let mut items = Vec::new();
        let mut others = Vec::new();
        for body in &seen {
            // A job that reads it in the background stops at a cancel
            // between bodies (mitcad#1656: the application's read gives way
            // to what the user does meanwhile).
            if self.monitor().is_some_and(|m| m.is_cancelled()) {
                return Err(ModelError::Cancelled);
            }
            let mut found = Vec::new();
            self.plane_items_of(body, &frame, normal, out_of_plane, &mut found);
            for item in found {
                if item.out_of_plane.is_some() {
                    others.push(item);
                } else {
                    items.push(item);
                }
            }
        }
        items.extend(others);
        Ok(items)
    }

    /// A key of what [`Document::sketch_body_geometry`] answers for
    /// `sketch` (the `plane_geometry_key` query, mitcad#1646): the sketch's
    /// frame and, for each body it looks at, the body, the version of its
    /// shape, the occurrence it is seen at and the transform into the
    /// sketch. The same key means the same answer, in plane and out of it,
    /// so a client keeps what it read while the key stays: an edit of the
    /// sketch itself (a dimension added) leaves it, unlike the document's
    /// revision. Cheap: no kernel call.
    pub fn plane_geometry_key(&self, sketch: FeatureUid) -> Result<u128, ModelError> {
        let Inputs { frame, seen } = self.plane_inputs(sketch)?;
        let mut key = Fingerprint::new("plane_geometry");
        for v in [frame.origin, frame.x_axis, frame.y_axis] {
            for x in v {
                key.f64(x);
            }
        }
        key.u64(seen.len() as u64);
        for body in &seen {
            key.str(&body.body.to_string()).u128(body.version);
            key.option(body.occurrence.as_ref(), |key, path| {
                key.u64(path.len() as u64);
                for o in path {
                    key.str(&o.to_string());
                }
            });
            key.option(body.to_sketch.as_ref(), |key, t| {
                for x in t.linear.iter().flatten().chain(&t.translation) {
                    key.f64(*x);
                }
            });
        }
        Ok(key.finish())
    }

    /// The bodies before the timeline position from the last recompute,
    /// with their versions (as [`Document::bodies_before`], borrowed).
    fn versioned_bodies_before(&self, position: usize) -> BTreeMap<BodyUid, &Versioned<K::Shape>> {
        let mut bodies = BTreeMap::new();
        for result in self.result.results.iter().take(position) {
            let Some(output) = &result.output else {
                continue;
            };
            for change in &output.changes {
                match change {
                    Change::Set(uid, shape) => {
                        bodies.insert(*uid, shape);
                    }
                    Change::Remove(uid) => {
                        bodies.remove(uid);
                    }
                }
            }
        }
        bodies
    }

    /// What the `plane_geometry` query of `sketch` reads ([`Inputs`]).
    fn plane_inputs(&self, sketch: FeatureUid) -> Result<Inputs<'_, K::Shape>, ModelError> {
        let position = self.state.require(sketch)?;
        let frame = match (
            self.sketch_output(sketch),
            &self.state.features[position].def,
        ) {
            (Some(output), _) => output.frame,
            (None, FeatureDef::Sketch(def)) => def
                .plane
                .origin_frame()
                .map(|plane| def.place(&plane))
                .ok_or_else(|| invalid("the sketch has no plane yet (it does not evaluate)"))?,
            (None, _) => return Err(invalid(format!("{sketch} is not a sketch"))),
        };
        let component = self.state.features[position].component;
        let bodies = self.versioned_bodies_before(position);
        let assembly = &self.state.assembly;
        let placements = &self.result.placements;
        let mut placement = |o: OccurrenceUid| {
            placements.get(&o).copied().unwrap_or_else(|| {
                assembly
                    .occurrence(o)
                    .map_or(Transform::IDENTITY, |o| o.transform)
            })
        };
        let own_policy = self.policy(component);
        // Isolate leaves out of the view what it does not show, so out of
        // the picks and snaps too (mitcad#1702); the key follows, as it
        // lists the bodies seen.
        let isolation = self.isolation();
        // The visible bodies as the design places them: the sketch's own
        // component's once, in its own coordinates; another component's at
        // each occurrence that shows it, moved into the sketch's.
        let mut seen: Vec<Seen<'_, K::Shape>> = Vec::new();
        let mut own: BTreeSet<BodyUid> = BTreeSet::new();
        for instance in self.instances() {
            if !instance.visible
                || !super::display::isolation_shows(&isolation, instance.body, &instance.path)
            {
                continue;
            }
            let Some(shape) = bodies.get(&instance.body) else {
                continue;
            };
            if instance.component == component {
                if own.insert(instance.body) {
                    seen.push(Seen {
                        body: instance.body,
                        shape: &shape.value,
                        version: shape.version,
                        occurrence: None,
                        to_sketch: None,
                    });
                }
                continue;
            }
            if !own_policy.reference_others
                || !self.policy(instance.component).references_from_others
            {
                continue;
            }
            let Ok(Some(link)) = crate::links::link_for(assembly, component, &instance.path, None)
            else {
                continue;
            };
            let Ok(to_sketch) = link.transform(assembly, component, &mut placement) else {
                continue;
            };
            seen.push(Seen {
                body: instance.body,
                shape: &shape.value,
                version: shape.version,
                occurrence: Some(instance.path),
                to_sketch: Some(to_sketch),
            });
        }
        Ok(Inputs { frame, seen })
    }

    /// The edges and vertices of one body in the plane, and with
    /// `out_of_plane` the others too.
    fn plane_items_of(
        &self,
        seen: &Seen<'_, K::Shape>,
        frame: &SketchFrame,
        normal: [f64; 3],
        out_of_plane: bool,
        items: &mut Vec<PlaneItem>,
    ) {
        let kernel = &self.kernel;
        let moved = |curve: Curve3| match &seen.to_sketch {
            Some(t) => transform_curve(t, &curve),
            None => curve,
        };
        // The tolerance grows with the body's size; a body whose box does
        // not reach the plane has nothing in it (but what is out of it).
        // How far the box's corners are from the plane, lowest and highest,
        // and its diagonal.
        let reach = |b: &BoundingBox| -> (f64, f64, f64) {
            let mut low = f64::INFINITY;
            let mut high = f64::NEG_INFINITY;
            for i in 0..8 {
                let corner = [
                    if i & 1 == 0 { b.min[0] } else { b.max[0] },
                    if i & 2 == 0 { b.min[1] } else { b.max[1] },
                    if i & 4 == 0 { b.min[2] } else { b.max[2] },
                ];
                let p = match moved(Curve3::Point(corner)) {
                    Curve3::Point(p) => p,
                    _ => corner,
                };
                let off = dot(std::array::from_fn(|k| p[k] - frame.origin[k]), normal);
                low = low.min(off);
                high = high.max(off);
            }
            let diagonal: f64 = (0..3)
                .map(|k| (b.max[k] - b.min[k]).powi(2))
                .sum::<f64>()
                .sqrt();
            (low, high, diagonal)
        };
        let off_plane = |low: f64, high: f64, size: f64| {
            let tolerance = mitcad_model_base::internal::datum::LINEAR * size;
            low > tolerance || high < -tolerance
        };
        // A cheap box that holds the body first (mitcad#1656): one that
        // stays off the plane by its own tolerance has every edge and vertex
        // off it by the body's tighter one too (the tight box lies inside),
        // so the tight box, which the kernel works out at a cost on first
        // use, is needed only for a body that can reach the plane.
        let enclosing = match kernel.enclosing_box(seen.shape) {
            Ok(Some(b)) => {
                let (low, high, diagonal) = reach(&b);
                let size = diagonal.max(1.0);
                off_plane(low, high, size).then_some(size)
            }
            _ => None,
        };
        let mut size = 1.0_f64;
        if let Some(enclosing) = enclosing {
            if !out_of_plane {
                return;
            }
            size = enclosing;
        } else if let Ok(Some(b)) = kernel.bounding_box(seen.shape) {
            let (low, high, diagonal) = reach(&b);
            size = size.max(diagonal);
            if !out_of_plane && off_plane(low, high, size) {
                return;
            }
        }
        let tolerance = mitcad_model_base::internal::datum::LINEAR * size;
        let item = |source: TopoName, shape: PlaneShape, out: Option<OutOfPlane>| PlaneItem {
            source,
            body: seen.body,
            occurrence: seen.occurrence.clone(),
            shape,
            out_of_plane: out,
        };
        // Edges whose every point is in the plane (and the others).
        for (source, curve) in named_edge_curves(kernel, seen.shape) {
            let curve = moved(curve);
            if distance_from_plane(&curve, frame.origin, normal) > tolerance {
                // Out of the plane (mitcad#866): projected along the normal,
                // a line along it to a point.
                if out_of_plane {
                    let shape = match project(std::slice::from_ref(&curve), frame)
                        .into_iter()
                        .next()
                    {
                        Some(Projected::Curve(c)) => PlaneShape::Curve(c),
                        Some(Projected::Point(p)) => PlaneShape::Point(p),
                        None => continue,
                    };
                    let out = self::out_of_plane(&curve, frame, normal);
                    items.push(item(source, shape, Some(out)));
                }
                continue;
            }
            if let Some(Projected::Curve(c)) = project(std::slice::from_ref(&curve), frame)
                .into_iter()
                .next()
            {
                items.push(item(source, PlaneShape::Curve(c), None));
            }
        }
        // Vertices in the plane (and the others).
        for index in 0.. {
            let Ok(Some(element)) = kernel.indexed_element(seen.shape, ElementKind::Vertex, index)
            else {
                break;
            };
            let Some(Ok(source @ TopoName::Vertex(_))) =
                element.name.map(|n| n.parse::<TopoName>())
            else {
                continue;
            };
            let [Curve3::Point(at)] = &element.curves[..] else {
                continue;
            };
            let point = moved(Curve3::Point(*at));
            let out = distance_from_plane(&point, frame.origin, normal) > tolerance;
            if out && !out_of_plane {
                continue;
            }
            if let Some(Projected::Point(p)) = project(std::slice::from_ref(&point), frame)
                .into_iter()
                .next()
            {
                let place = out.then(|| self::out_of_plane(&point, frame, normal));
                items.push(item(source, PlaneShape::Vertex(p), place));
            }
        }
    }
}
