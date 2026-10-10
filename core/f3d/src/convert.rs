// SPDX-License-Identifier: MIT
//! Conversion of ASM bodies to the neutral B-rep model (millimetres).

use std::collections::HashMap;

use crate::asm::file::AsmFile;
use crate::asm::geom::{
    self, AsmCurve, AsmSurface, Bs3Curve, Bs3Surface, Cursor, GeomError, HelixCircle, HelixLine,
    IntGeom, SplineGeom,
};
use crate::brep::{
    self, BSplineCurve, BSplineSurface, Body, CheckReport, Coedge, Curve, Edge, Face, Lump, P3,
    Shell, Surface, Transform, Vertex,
};

/// A converted body with what could not be converted.
#[derive(Clone, Debug)]
pub struct ConvertedBody {
    /// Record index of the `body` in the ASM file.
    pub record: usize,
    /// The body is a top-level entity of the file (otherwise it owns a
    /// top-level face or edge).
    pub top_level: bool,
    pub body: Body,
    /// Unsupported or undecodable geometry and topology, one line each.
    pub issues: Vec<String>,
    /// Faces left out because their surface could not be converted.
    pub skipped_faces: usize,
    /// Edges without a curve (at cone apexes and sphere poles), left out of
    /// their loops.
    pub degenerate_edges: usize,
    /// Loops made fit for the builder ([`tidy_loop`]).
    pub tidied: LoopTidy,
    pub check: CheckReport,
}

/// Options for [`convert_file`].
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Millimetres per model unit. .f3d files store centimetres (10).
    pub scale: f64,
    /// Points per turn when sampling helices (curves and thread
    /// surfaces, which the builder interpolates).
    pub helix_samples_per_turn: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            scale: 10.0,
            helix_samples_per_turn: 48,
        }
    }
}

/// An ASM edge in the neutral model.
#[derive(Clone, Copy, Debug)]
enum EdgeRef {
    /// Neutral edge, and whether it runs along the ASM edge.
    Edge(usize, bool),
    /// Edge without a curve (a cone apex, a sphere pole) at this neutral
    /// vertex.
    Degenerate(usize),
}

/// Converts the bodies of the file (see [`body_records`]).
pub fn convert_file(file: &AsmFile, options: &Options) -> Vec<ConvertedBody> {
    body_records(file)
        .into_iter()
        .map(|(i, top)| {
            let mut b = convert_body(file, i, options);
            b.top_level = top;
            b
        })
        .collect()
}

/// The body records of a file with `true` for top-level bodies: first the
/// top-level entities that are bodies or belong to one (`.smb` files also
/// save faces and edges the design refers to, and assembly files only
/// those), each body once, in save order.
pub fn body_records(file: &AsmFile) -> Vec<(usize, bool)> {
    let mut out: Vec<(usize, bool)> = Vec::new();
    for i in file.top_level() {
        let top = file.records[i].type_name == "body";
        if let Some(b) = owner_body(file, i) {
            match out.iter_mut().find(|(r, _)| *r == b) {
                Some(entry) => entry.1 |= top,
                None => out.push((b, top)),
            }
        }
    }
    out
}

/// The body a topological entity belongs to, following owner pointers.
pub fn owner_body(file: &AsmFile, record: usize) -> Option<usize> {
    let mut r = record;
    for _ in 0..8 {
        let mut c = entity_cursor(file, r);
        let next = match file.records[r].base_type() {
            "body" => return Some(r),
            // next, shell, body
            "lump" => {
                c.ptr().ok()?;
                c.ptr().ok()?;
                c.ptr().ok()?
            }
            // next, subshell, face, wire, lump
            "shell" => {
                for _ in 0..4 {
                    c.ptr().ok()?;
                }
                c.ptr().ok()?
            }
            // next, loop, shell
            "face" => {
                c.ptr().ok()?;
                c.ptr().ok()?;
                c.ptr().ok()?
            }
            // next, coedge, face
            "loop" => {
                c.ptr().ok()?;
                c.ptr().ok()?;
                c.ptr().ok()?
            }
            // next, previous, partner, edge, sense, loop
            "coedge" => {
                for _ in 0..4 {
                    c.ptr().ok()?;
                }
                c.boolean().ok()?;
                c.ptr().ok()?
            }
            // start, start param, end, end param, coedge
            "edge" => {
                c.ptr().ok()?;
                c.double().ok()?;
                c.ptr().ok()?;
                c.double().ok()?;
                c.ptr().ok()?
            }
            // edge
            "vertex" => c.ptr().ok()?,
            _ => return None,
        };
        r = ptr_index(file, next)?;
    }
    None
}

/// What [`tidy_loop`] changed in a body's loops.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoopTidy {
    /// Edges that ran out and back in one loop (a slit), left out.
    pub spurs: usize,
    /// Loops that passed a vertex twice, split there into loops of their
    /// own.
    pub pinches: usize,
}

/// The start vertex of a coedge.
fn coedge_start(edges: &[Edge], c: Coedge) -> usize {
    let v = edges[c.edge].vertices;
    if c.forward { v[0] } else { v[1] }
}

/// A loop made fit for the builder:
/// - An edge whose two coedges follow one another in the loop, out and
///   back, is a slit ending inside the face (a cylinder touching a plane
///   along a line keeps its line of contact so): it bounds nothing and is
///   left out.
/// - A loop of a planar face that passes a vertex twice (a hole touching
///   the boundary at a point is one loop with it) is split there into
///   simple loops sharing the vertex. On a periodic surface a closed edge
///   across the period passes its vertex twice in a simple loop.
///
/// Returns the loops, the slits left out and the splits made.
pub fn tidy_loop(
    mut coedges: Vec<Coedge>,
    edges: &[Edge],
    planar: bool,
) -> (Vec<Vec<Coedge>>, usize, usize) {
    let mut spurs = 0;
    loop {
        let n = coedges.len();
        let found = (0..n).find(|&i| {
            let (a, b) = (coedges[i], coedges[(i + 1) % n]);
            n >= 2 && a.edge == b.edge && a.forward != b.forward
        });
        let Some(i) = found else { break };
        let j = (i + 1) % n;
        let (lo, hi) = (i.min(j), i.max(j));
        coedges.remove(hi);
        coedges.remove(lo);
        spurs += 1;
    }
    if coedges.is_empty() {
        return (Vec::new(), spurs, 0);
    }
    if !planar {
        return (vec![coedges], spurs, 0);
    }
    // Split at the first vertex passed twice, until none is.
    let mut out = Vec::new();
    let mut pinches = 0;
    let mut work = vec![coedges];
    while let Some(lp) = work.pop() {
        let starts: Vec<usize> = lp.iter().map(|&c| coedge_start(edges, c)).collect();
        // Two parts that share an edge (the seam of a periodic face
        // between its boundary loops) stay one loop.
        let apart = |a: usize, b: usize| {
            let inner: std::collections::HashSet<usize> = lp[a..b].iter().map(|c| c.edge).collect();
            lp[..a]
                .iter()
                .chain(&lp[b..])
                .all(|c| !inner.contains(&c.edge))
        };
        let mut seen: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut split = None;
        'find: for (k, &v) in starts.iter().enumerate() {
            let before = seen.entry(v).or_default();
            for &first in before.iter() {
                if apart(first, k) {
                    split = Some((first, k));
                    break 'find;
                }
            }
            before.push(k);
        }
        match split {
            Some((a, b)) if b - a < lp.len() => {
                pinches += 1;
                let inner: Vec<Coedge> = lp[a..b].to_vec();
                let mut outer: Vec<Coedge> = lp[..a].to_vec();
                outer.extend_from_slice(&lp[b..]);
                // The part with the loop's first coedge first.
                let (first, second) = if a == 0 {
                    (inner, outer)
                } else {
                    (outer, inner)
                };
                if !second.is_empty() {
                    work.push(second);
                }
                work.push(first);
            }
            _ => out.push(lp),
        }
    }
    (out, spurs, pinches)
}

struct Ctx<'a> {
    file: &'a AsmFile,
    /// Entity -> record holding its data at an earlier history state
    /// (`None`: the entity did not exist then); see [`crate::asm::history`].
    view: Option<&'a HashMap<usize, Option<usize>>>,
    opt: Options,
    body: Body,
    issues: Vec<String>,
    vertex_map: HashMap<usize, usize>,
    edge_map: HashMap<usize, EdgeRef>,
    /// Edges without a curve (left out of their loops).
    degenerate_edges: usize,
    /// Loops made fit for the builder ([`tidy_loop`]).
    tidied: LoopTidy,
    /// ASM surface record -> (neutral surface, normal flipped).
    surface_map: HashMap<usize, Option<(usize, bool)>>,
}

fn unit(v: P3) -> P3 {
    brep::normalize(v)
}

/// Header fields common to entity records: attribute, history id, and one
/// more pointer. Returns a cursor after them.
fn entity_cursor(file: &AsmFile, record: usize) -> Cursor<'_> {
    Cursor::new(&file.tokens, file.records[record].fields.start + 3)
}

fn ptr_index(file: &AsmFile, p: i64) -> Option<usize> {
    file.record_of(p)
}

/// Upper bound for linked lists (faces of a shell, coedges of a loop).
const MAX_LIST: usize = 1_000_000;

impl<'a> Ctx<'a> {
    fn issue(&mut self, msg: String) {
        if self.issues.len() < 200 {
            self.issues.push(msg);
        }
    }

    /// The record holding an entity's data at the converted history state:
    /// the entity's own record unless rolling back.
    fn data(&self, entity: usize) -> Result<usize, GeomError> {
        match self.view.and_then(|v| v.get(&entity)) {
            None => Ok(entity),
            Some(Some(r)) => Ok(*r),
            Some(None) => Err(GeomError(format!(
                "entity {entity} does not exist at this history state"
            ))),
        }
    }

    /// A cursor after the entity header of the entity's data record.
    fn cursor(&self, entity: usize) -> Result<Cursor<'a>, GeomError> {
        Ok(entity_cursor(self.file, self.data(entity)?))
    }

    fn type_name(&self, entity: usize) -> Result<&'a str, GeomError> {
        Ok(&self.file.records[self.data(entity)?].type_name)
    }

    fn expect_type(&self, record: usize, base: &str) -> Result<(), GeomError> {
        let r = &self.file.records[self.data(record)?];
        if r.base_type() == base || r.type_name.ends_with(&format!("-{base}")) {
            Ok(())
        } else {
            Err(GeomError(format!(
                "record {record} is {} instead of {base}",
                r.type_name
            )))
        }
    }

    fn vertex(&mut self, record: usize) -> Result<usize, GeomError> {
        if let Some(&v) = self.vertex_map.get(&record) {
            return Ok(v);
        }
        self.expect_type(record, "vertex")?;
        let mut c = self.cursor(record)?;
        c.ptr()?; // edge
        c.int()?;
        let point_rec = ptr_index(self.file, c.ptr()?)
            .ok_or_else(|| GeomError("vertex without point".into()))?;
        let mut tolerance = 0.0;
        if self.type_name(record)? == "tvertex-vertex" {
            // int, then two tolerances (the larger one is used).
            let _ = c.int();
            if let (Ok(a), Ok(b)) = (c.double(), c.double()) {
                tolerance = a.max(b) * self.opt.scale;
            }
        }
        self.expect_type(point_rec, "point")?;
        let mut pc = self.cursor(point_rec)?;
        let p = brep::scale(pc.point()?, self.opt.scale);
        self.body.vertices.push(Vertex {
            point: p,
            tolerance,
        });
        let v = self.body.vertices.len() - 1;
        self.vertex_map.insert(record, v);
        Ok(v)
    }

    fn bspline_curve(&self, b: &Bs3Curve) -> Result<BSplineCurve, GeomError> {
        let s = self.opt.scale;
        let mut mults = b.mults.clone();
        let n = mults.len();
        mults[0] += 1;
        mults[n - 1] += 1;
        let poles = b
            .ctrl
            .iter()
            .map(|c| [c[0] * s, c[1] * s, c[2] * s])
            .collect();
        let weights = b.rational.then(|| b.ctrl.iter().map(|c| c[3]).collect());
        let out = BSplineCurve {
            degree: b.degree,
            knots: b.knots.clone(),
            mults,
            poles,
            weights,
            periodic: b.closure == 2,
        };
        if !out.is_valid() {
            return Err(GeomError("invalid B-spline curve".into()));
        }
        Ok(out)
    }

    fn bspline_surface(&self, b: &Bs3Surface) -> Result<BSplineSurface, GeomError> {
        let s = self.opt.scale;
        let mut um = b.u_mults.clone();
        let mut vm = b.v_mults.clone();
        let (nu_k, nv_k) = (um.len(), vm.len());
        um[0] += 1;
        um[nu_k - 1] += 1;
        vm[0] += 1;
        vm[nv_k - 1] += 1;
        // Stored with u varying fastest; the neutral model is u-major.
        let mut poles = Vec::with_capacity(b.nu * b.nv);
        let mut weights = Vec::with_capacity(b.nu * b.nv);
        for iu in 0..b.nu {
            for iv in 0..b.nv {
                let c = b.ctrl[iv * b.nu + iu];
                poles.push([c[0] * s, c[1] * s, c[2] * s]);
                weights.push(c[3]);
            }
        }
        let out = BSplineSurface {
            u_degree: b.u_degree,
            v_degree: b.v_degree,
            u_knots: b.u_knots.clone(),
            u_mults: um,
            v_knots: b.v_knots.clone(),
            v_mults: vm,
            nu: b.nu,
            nv: b.nv,
            poles,
            weights: b.rational.then_some(weights),
            u_periodic: b.u_closure == 2,
            v_periodic: b.v_closure == 2,
        };
        if !out.is_valid() {
            return Err(GeomError("invalid B-spline surface".into()));
        }
        Ok(out)
    }

    /// Neutral curve and the factor `k` so that neutral parameter = `k * t`
    /// for ASM curve parameter `t`.
    fn curve(&self, c: &AsmCurve) -> Result<(Curve, f64), GeomError> {
        let s = self.opt.scale;
        match c {
            AsmCurve::Straight { root, dir } => {
                let len = brep::norm(*dir);
                if len == 0.0 {
                    return Err(GeomError("straight curve without direction".into()));
                }
                Ok((
                    Curve::Line {
                        origin: brep::scale(*root, s),
                        dir: unit(*dir),
                    },
                    len * s,
                ))
            }
            AsmCurve::Ellipse {
                center,
                normal,
                major,
                ratio,
            } => {
                let r = brep::norm(*major);
                if r == 0.0 || *ratio <= 0.0 {
                    return Err(GeomError("degenerate ellipse".into()));
                }
                if *ratio > 1.0 + 1e-12 {
                    return Err(GeomError("ellipse with ratio > 1".into()));
                }
                let normal = unit(*normal);
                let major_dir = unit(*major);
                let minor = if (*ratio - 1.0).abs() <= 1e-12 {
                    r * s
                } else {
                    r * ratio * s
                };
                Ok((
                    Curve::Ellipse {
                        center: brep::scale(*center, s),
                        normal,
                        major_dir,
                        major: r * s,
                        minor,
                    },
                    1.0,
                ))
            }
            AsmCurve::Int {
                reversed,
                subtype,
                geom,
            } => {
                let k = if *reversed { -1.0 } else { 1.0 };
                match geom {
                    IntGeom::BSpline(b) => Ok((Curve::BSpline(self.bspline_curve(b)?), k)),
                    IntGeom::Helix {
                        range,
                        center,
                        major,
                        minor,
                        pitch,
                        taper,
                    } => {
                        let (lo, hi) = (range[0].min(range[1]), range[0].max(range[1]));
                        let turns = ((hi - lo) / std::f64::consts::TAU).max(0.05);
                        let n = helix_samples(turns, &self.opt).unwrap_or_else(|| {
                            ((turns * self.opt.helix_samples_per_turn as f64).ceil() as usize)
                                .clamp(8, 20_000)
                        });
                        let mut params = Vec::with_capacity(n + 1);
                        let mut points = Vec::with_capacity(n + 1);
                        for i in 0..=n {
                            let t = lo + (hi - lo) * i as f64 / n as f64;
                            let grow = 1.0 + taper * t / std::f64::consts::TAU;
                            let p = brep::add(
                                *center,
                                brep::add(
                                    brep::scale(
                                        brep::add(
                                            brep::scale(*major, t.cos()),
                                            brep::scale(*minor, t.sin()),
                                        ),
                                        grow,
                                    ),
                                    brep::scale(*pitch, t / std::f64::consts::TAU),
                                ),
                            );
                            params.push(t);
                            points.push(brep::scale(p, s));
                        }
                        Ok((Curve::Interpolated { params, points }, k))
                    }
                    IntGeom::Unsupported(why) => Err(GeomError(format!("{subtype}: {why}"))),
                }
            }
        }
    }

    fn edge(&mut self, record: usize) -> Result<EdgeRef, GeomError> {
        if let Some(e) = self.edge_map.get(&record) {
            return Ok(*e);
        }
        self.expect_type(record, "edge")?;
        let mut c = self.cursor(record)?;
        let v0 = ptr_index(self.file, c.ptr()?)
            .ok_or_else(|| GeomError("edge without start vertex".into()))?;
        let p0 = c.double()?;
        let v1 = ptr_index(self.file, c.ptr()?)
            .ok_or_else(|| GeomError("edge without end vertex".into()))?;
        let p1 = c.double()?;
        c.ptr()?; // coedge
        let curve_rec = ptr_index(self.file, c.ptr()?);
        let sense_reversed = c.boolean()?;
        let _convexity = c.string().ok();
        let mut tolerance = 0.0;
        if self.type_name(record)? == "tedge-edge"
            && let Ok(t) = c.double()
        {
            tolerance = t * self.opt.scale;
        }
        let va = self.vertex(v0)?;
        let vb = self.vertex(v1)?;
        let Some(curve_rec) = curve_rec else {
            // Degenerate edge (no curve), e.g. at a cone apex: the face
            // gets a point loop (when the loop has no other edges), or
            // OCCT's healing adds its own degenerated edge.
            self.degenerate_edges += 1;
            let e = EdgeRef::Degenerate(va);
            self.edge_map.insert(record, e);
            return Ok(e);
        };
        let asm_curve = geom::curve_record(self.file, self.data(curve_rec)?)?;
        let (curve, k) = self.curve(&asm_curve)?;
        // Edge parameters are along the edge; a reversed edge runs against
        // its curve, whose parameter is then the negated edge parameter.
        let sigma = if sense_reversed { -1.0 } else { 1.0 };
        let g0 = k * sigma * p0;
        let g1 = k * sigma * p1;
        let (mut t, vertices, along) = if g0 <= g1 {
            ([g0, g1], [va, vb], true)
        } else {
            ([g1, g0], [vb, va], false)
        };
        // Approximations end where the definition's parameter range ends;
        // edge parameters may overshoot by rounding.
        // On periodic curves an edge may cross the seam instead.
        let periodic = matches!(&curve, Curve::BSpline(b) if b.periodic);
        if let Some((a, b)) = curve.range() {
            if periodic {
                if t[1] - t[0] > (b - a) * (1.0 + 1e-9) {
                    self.issue(format!("edge {record}: longer than its periodic curve"));
                }
            } else {
                let excess = (a - t[0]).max(t[1] - b);
                if excess > 1e-3 * (b - a) {
                    self.issue(format!(
                        "edge {record}: parameters [{}, {}] exceed the curve range [{a}, {b}]",
                        t[0], t[1]
                    ));
                }
                t = [t[0].clamp(a, b), t[1].clamp(a, b)];
            }
        }
        self.body.curves.push(curve);
        let ci = self.body.curves.len() - 1;
        self.body.edges.push(Edge {
            curve: ci,
            t,
            vertices,
            tolerance,
        });
        let e = EdgeRef::Edge(self.body.edges.len() - 1, along);
        self.edge_map.insert(record, e);
        Ok(e)
    }

    /// Neutral surface for an ASM surface; `flip` when the neutral natural
    /// normal is opposite to the ASM surface normal.
    fn surface_geom(&self, s: &AsmSurface) -> Result<(Surface, bool), GeomError> {
        let k = self.opt.scale;
        match s {
            AsmSurface::Plane {
                root,
                normal,
                u_dir,
            } => Ok((
                Surface::Plane {
                    origin: brep::scale(*root, k),
                    normal: unit(*normal),
                    u_dir: unit(*u_dir),
                },
                false,
            )),
            AsmSurface::Cone {
                center,
                normal,
                major,
                ratio,
                sin,
                cos,
            } => {
                if cos.abs() < 1e-12 {
                    return Err(GeomError("planar cone (cos = 0)".into()));
                }
                if (*ratio - 1.0).abs() > 1e-9 {
                    if *sin != 0.0 {
                        return Err(GeomError(format!("elliptic cone (ratio {ratio})")));
                    }
                    // Elliptic cylinder: the base ellipse swept along the
                    // axis; for a counter-clockwise ellipse the extrusion
                    // normal points away from the axis, as for cones.
                    let (curve, _) = self.curve(&AsmCurve::Ellipse {
                        center: *center,
                        normal: *normal,
                        major: *major,
                        ratio: *ratio,
                    })?;
                    return Ok((
                        Surface::Extrusion {
                            curve,
                            dir: unit(*normal),
                        },
                        *cos < 0.0,
                    ));
                }
                let half_angle = (sin / cos).atan();
                Ok((
                    Surface::Cone {
                        origin: brep::scale(*center, k),
                        axis: unit(*normal),
                        ref_dir: unit(*major),
                        radius: brep::norm(*major) * k,
                        half_angle,
                    },
                    *cos < 0.0,
                ))
            }
            AsmSurface::Sphere {
                center,
                radius,
                u_dir,
                pole,
            } => Ok((
                Surface::Sphere {
                    center: brep::scale(*center, k),
                    axis: unit(*pole),
                    ref_dir: unit(*u_dir),
                    radius: radius.abs() * k,
                },
                *radius < 0.0,
            )),
            AsmSurface::Torus {
                center,
                normal,
                major,
                minor,
                u_dir,
            } => {
                if *major < 0.0 {
                    // Lemon torus: the same parametric form with a negative
                    // major radius, as a surface of revolution of the
                    // meridian circle centred at `major` along `u_dir`.
                    let z = unit(*normal);
                    let x = unit(*u_dir);
                    let circle = Curve::Ellipse {
                        center: brep::scale(brep::add(*center, brep::scale(x, *major)), k),
                        normal: brep::cross(x, z),
                        major_dir: x,
                        major: minor.abs() * k,
                        minor: minor.abs() * k,
                    };
                    return Ok((
                        Surface::Revolution {
                            curve: circle,
                            origin: brep::scale(*center, k),
                            axis: z,
                        },
                        *minor < 0.0,
                    ));
                }
                Ok((
                    Surface::Torus {
                        center: brep::scale(*center, k),
                        axis: unit(*normal),
                        ref_dir: unit(*u_dir),
                        major: major * k,
                        minor: minor.abs() * k,
                    },
                    *minor < 0.0,
                ))
            }
            AsmSurface::Spline {
                reversed,
                subtype,
                geom,
                ranges,
            } => match geom {
                SplineGeom::BSpline { surface, .. } => {
                    Ok((Surface::BSpline(self.bspline_surface(surface)?), *reversed))
                }
                SplineGeom::Extrusion { curve, dir } => {
                    let (c, kc) = self.curve(curve)?;
                    // S(u, v) = C(u) + v dir; a reversed profile flips du.
                    Ok((
                        Surface::Extrusion {
                            curve: c,
                            dir: unit(*dir),
                        },
                        *reversed != (kc < 0.0),
                    ))
                }
                SplineGeom::Revolution {
                    approx: Some(a), ..
                } => Ok((Surface::BSpline(self.bspline_surface(a)?), *reversed)),
                SplineGeom::Revolution { .. } => {
                    Err(GeomError(format!("{subtype} without approximation")))
                }
                SplineGeom::HelixLine(h) => {
                    Ok((helix_line_surface(h, ranges, &self.opt)?, *reversed))
                }
                SplineGeom::HelixCircle(h) => {
                    Ok((helix_circle_surface(h, ranges, &self.opt)?, *reversed))
                }
                SplineGeom::Unsupported(why) => Err(GeomError(format!("{subtype}: {why}"))),
            },
        }
    }

    fn surface(&mut self, record: usize) -> Result<Option<(usize, bool)>, GeomError> {
        if let Some(s) = self.surface_map.get(&record) {
            return Ok(*s);
        }
        let result = self
            .data(record)
            .and_then(|r| geom::surface_record(self.file, r))
            .and_then(|s| self.surface_geom(&s));
        let out = match result {
            Ok((s, flip)) => {
                self.body.surfaces.push(s);
                Some((self.body.surfaces.len() - 1, flip))
            }
            Err(e) => {
                let name = self.file.records[record].type_name.clone();
                self.issue(format!("surface {record} ({name}): {e}"));
                None
            }
        };
        self.surface_map.insert(record, out);
        Ok(out)
    }

    /// Converts a face; `None` when its surface is unsupported.
    fn face(&mut self, record: usize) -> Result<Option<usize>, GeomError> {
        self.expect_type(record, "face")?;
        let mut c = self.cursor(record)?;
        c.ptr()?; // next
        let first_loop = c.ptr()?;
        c.ptr()?; // shell
        c.ptr()?; // subshell
        let surface_rec = ptr_index(self.file, c.ptr()?)
            .ok_or_else(|| GeomError("face without surface".into()))?;
        let sense_reversed = c.boolean()?;
        let double_sided = c.boolean()?;
        let Some((surface, flip)) = self.surface(surface_rec)? else {
            return Ok(None);
        };
        let mut loops = Vec::new();
        let mut point_loops = Vec::new();
        let mut lp = ptr_index(self.file, first_loop);
        let mut guard = 0;
        while let Some(l) = lp {
            guard += 1;
            if guard > MAX_LIST {
                return Err(GeomError("loop list does not end".into()));
            }
            self.expect_type(l, "loop")?;
            let mut lc = self.cursor(l)?;
            let next = lc.ptr()?;
            let first = ptr_index(self.file, lc.ptr()?);
            let mut coedges = Vec::new();
            let mut point = None;
            let mut ce = first;
            let mut n = 0;
            while let Some(cr) = ce {
                n += 1;
                if n > MAX_LIST {
                    return Err(GeomError("coedge list does not end".into()));
                }
                self.expect_type(cr, "coedge")?;
                let mut cc = self.cursor(cr)?;
                let next_ce = cc.ptr()?;
                cc.ptr()?; // previous
                cc.ptr()?; // partner
                let edge_rec = ptr_index(self.file, cc.ptr()?)
                    .ok_or_else(|| GeomError("coedge without edge".into()))?;
                let coedge_reversed = cc.boolean()?;
                match self.edge(edge_rec) {
                    Ok(EdgeRef::Edge(e, along)) => coedges.push(Coedge {
                        edge: e,
                        forward: along != coedge_reversed,
                    }),
                    Ok(EdgeRef::Degenerate(v)) => point = Some(v),
                    Err(err) => {
                        let name = self.file.records[edge_rec].type_name.clone();
                        self.issue(format!("edge {edge_rec} ({name}): {err}"));
                    }
                }
                ce = ptr_index(self.file, next_ce);
                if ce == first {
                    break;
                }
            }
            match point {
                Some(v) if coedges.is_empty() => point_loops.push(v),
                _ => {
                    let (tidy, spurs, pinches) = tidy_loop(
                        coedges,
                        &self.body.edges,
                        matches!(self.body.surfaces[surface], Surface::Plane { .. }),
                    );
                    self.tidied.spurs += spurs;
                    self.tidied.pinches += pinches;
                    loops.extend(tidy);
                }
            }
            lp = ptr_index(self.file, next);
        }
        self.body.faces.push(Face {
            surface,
            reversed: sense_reversed != flip,
            double_sided,
            loops,
            point_loops,
        });
        Ok(Some(self.body.faces.len() - 1))
    }

    fn shell(&mut self, record: usize) -> Result<(Shell, usize), GeomError> {
        self.expect_type(record, "shell")?;
        let mut c = self.cursor(record)?;
        c.ptr()?; // next
        let subshell = c.ptr()?;
        let first_face = c.ptr()?;
        if subshell >= 0 {
            self.issue(format!(
                "shell {record} has subshells; their faces are not read"
            ));
        }
        let mut shell = Shell::default();
        let mut skipped = 0;
        let mut f = ptr_index(self.file, first_face);
        let mut guard = 0;
        while let Some(fr) = f {
            guard += 1;
            if guard > MAX_LIST {
                return Err(GeomError("face list does not end".into()));
            }
            match self.face(fr)? {
                Some(fi) => shell.faces.push(fi),
                None => skipped += 1,
            }
            let mut fc = self.cursor(fr)?;
            f = ptr_index(self.file, fc.ptr()?);
        }
        Ok((shell, skipped))
    }

    fn transform(&self, record: usize) -> Result<Transform, GeomError> {
        let data = self.data(record)?;
        let mut c = Cursor::new(&self.file.tokens, self.file.records[data].fields.start);
        c.ptr()?;
        c.int()?;
        let a = c.point()?;
        let b = c.point()?;
        let d = c.point()?;
        let t = c.point()?;
        let scale = c.double()?;
        // Rows are the images of the x, y and z axes (row-vector form).
        let m = [
            [a[0] * scale, b[0] * scale, d[0] * scale],
            [a[1] * scale, b[1] * scale, d[1] * scale],
            [a[2] * scale, b[2] * scale, d[2] * scale],
        ];
        Ok(Transform {
            m,
            t: brep::scale(t, self.opt.scale),
        })
    }
}

/// A line swept along a helix (`helix_spl_line`, see [`HelixLine`]) as the
/// ruled surface between the helices of the line's two ends, sampled at the
/// same parameters (the builder interpolates both, so the rulings stay
/// exact). `ranges` are the face's u and v ranges; the definition's are
/// used where they are unbounded. Both are widened a little, so that the
/// face's edges lie well inside the surface.
pub fn helix_line_surface(
    h: &HelixLine,
    ranges: &[[Option<f64>; 2]; 2],
    options: &Options,
) -> Result<Surface, GeomError> {
    if h.line[1].abs() > 1e-12 {
        return Err(GeomError(
            "helix_spl_line with a tangential line component".into(),
        ));
    }
    let bounded = |r: [Option<f64>; 2]| match r {
        [Some(a), Some(b)] if b > a => Some((a, b)),
        _ => None,
    };
    let (u0, u1) = bounded(ranges[0])
        .or_else(|| bounded(h.ranges[0]))
        .ok_or_else(|| GeomError("helix_spl_line without a line range".into()))?;
    let (v0, v1) = bounded(ranges[1])
        .or_else(|| bounded(h.ranges[1]))
        .ok_or_else(|| GeomError("helix_spl_line without a helix range".into()))?;
    let du = 0.02 * (u1 - u0);
    let dv = (0.02 * (v1 - v0)).max(0.02);
    let (u0, u1, v0, v1) = (u0 - du, u1 + du, v0 - dv, v1 + dv);
    let tau = std::f64::consts::TAU;
    let axis = unit(h.axis);
    let point = |u: f64, v: f64| {
        let radial = brep::add(brep::scale(h.major, v.cos()), brep::scale(h.minor, v.sin()));
        let grow = 1.0 + h.taper * v / tau + h.line[0] * u;
        let p = brep::add(
            brep::add(h.center, brep::scale(radial, grow)),
            brep::add(
                brep::scale(h.pitch, v / tau),
                brep::scale(axis, h.line[2] * u),
            ),
        );
        brep::scale(p, options.scale)
    };
    let turns = (v1 - v0) / tau;
    // Longer than the arc sweeps take: densely, as before.
    let n = helix_samples(turns, options).unwrap_or_else(|| {
        ((turns * options.helix_samples_per_turn as f64).ceil() as usize).clamp(8, 20_000)
    });
    let params: Vec<f64> = (0..=n)
        .map(|i| v0 + (v1 - v0) * i as f64 / n as f64)
        .collect();
    let from = params.iter().map(|&v| point(u0, v)).collect();
    let to = params.iter().map(|&v| point(u1, v)).collect();
    Ok(Surface::Ruled {
        from: Curve::Interpolated {
            params: params.clone(),
            points: from,
        },
        to: Curve::Interpolated { params, points: to },
    })
}

/// The most samples of a helix that a thread's surfaces (arc sweeps and
/// ruled flanks) and edges are built from (45 turns at the default 48 a
/// turn). OCCT's projections and booleans make a search grid of a few
/// points per knot span of a spline face for each query point: the many
/// spans of long sampled helices made the replay of parts with threads of
/// 55 and 81 turns take tens of gigabytes (one of 40 turns takes under
/// one). Longer helices are sampled less densely instead, down to
/// [`MIN_HELIX_SAMPLES_PER_TURN`].
const MAX_ARC_SWEEP_SAMPLES: usize = 2160;

/// The fewest samples per turn of a long helix: at 24 a turn the volume of
/// a part with a thread of 81 turns stays within 3e-6 of the one at 48
/// (135 turns at most).
const MIN_HELIX_SAMPLES_PER_TURN: usize = 16;

/// The samples of a helix of `turns` turns: the options' per turn, at most
/// [`MAX_ARC_SWEEP_SAMPLES`] while that keeps
/// [`MIN_HELIX_SAMPLES_PER_TURN`]; None for a longer one.
fn helix_samples(turns: f64, options: &Options) -> Option<usize> {
    let n = ((turns * options.helix_samples_per_turn as f64).ceil() as usize).max(8);
    if n <= MAX_ARC_SWEEP_SAMPLES {
        Some(n)
    } else if turns * MIN_HELIX_SAMPLES_PER_TURN as f64 <= MAX_ARC_SWEEP_SAMPLES as f64 {
        Some(MAX_ARC_SWEEP_SAMPLES)
    } else {
        None
    }
}

/// A circular arc swept along a helix (`helix_spl_circ`, see
/// [`HelixCircle`]) as arcs through the helices of the arcs' control
/// points, sampled at the same parameters (the builder interpolates them,
/// so that each sample's arc is exact). `ranges` are the face's u and v
/// ranges, the definition's where they are unbounded; the helix's range is
/// widened a little, and the arc's too where it stays below a full turn.
pub fn helix_circle_surface(
    h: &HelixCircle,
    ranges: &[[Option<f64>; 2]; 2],
    options: &Options,
) -> Result<Surface, GeomError> {
    let bounded = |r: [Option<f64>; 2]| match r {
        [Some(a), Some(b)] if b > a => Some((a, b)),
        _ => None,
    };
    let tau = std::f64::consts::TAU;
    let (u0, u1) = bounded(ranges[0])
        .or_else(|| bounded(h.ranges[0]))
        .unwrap_or((0.0, tau));
    let (v0, v1) = bounded(ranges[1])
        .or_else(|| bounded(h.ranges[1]))
        .ok_or_else(|| GeomError("helix_spl_circ without a helix range".into()))?;
    let radius = brep::norm(h.major);
    if h.radius.is_nan() || h.radius <= 0.0 || radius.is_nan() || radius <= 0.0 {
        return Err(GeomError("helix_spl_circ without a radius".into()));
    }
    let du = (0.02 * (u1 - u0)).min(0.5 * (tau - (u1 - u0)).max(0.0));
    let dv = (0.02 * (v1 - v0)).max(0.02);
    let (u0, u1, v0, v1) = (u0 - du, u1 + du, v0 - dv, v1 + dv);
    // One arc per quarter turn at most: control points at the arcs' ends
    // and, at the distance radius / cos(half the arc) on its bisector, at
    // their middles.
    let arcs = ((u1 - u0) / (0.5 * std::f64::consts::PI)).ceil().max(1.0) as usize;
    let half = 0.5 * (u1 - u0) / arcs as f64;
    let mut offsets = Vec::with_capacity(2 * arcs + 1);
    let mut weights = Vec::with_capacity(2 * arcs + 1);
    for k in 0..=2 * arcs {
        let angle = u0 + half * k as f64 + h.phase;
        let (distance, weight) = if k % 2 == 0 {
            (h.radius, 1.0)
        } else {
            (h.radius / half.cos(), half.cos())
        };
        offsets.push((distance * angle.cos(), distance * angle.sin()));
        weights.push(weight);
    }
    let axis = unit(h.axis);
    // The point at helix angle v, `inward` towards the axis and `along` it
    // from the helix.
    let point = |v: f64, inward: f64, along: f64| {
        let r = brep::add(brep::scale(h.major, v.cos()), brep::scale(h.minor, v.sin()));
        let grow = 1.0 + h.taper * v / tau;
        let p = brep::add(
            brep::add(h.center, brep::scale(r, grow - inward / radius)),
            brep::add(brep::scale(h.pitch, v / tau), brep::scale(axis, along)),
        );
        brep::scale(p, options.scale)
    };
    let turns = (v1 - v0) / tau;
    let n = helix_samples(turns, options).ok_or_else(|| {
        GeomError(format!(
            "helix_spl_circ of {turns:.0} turns: too long to sample"
        ))
    })?;
    let params: Vec<f64> = (0..=n)
        .map(|i| v0 + (v1 - v0) * i as f64 / n as f64)
        .collect();
    let sections = offsets
        .iter()
        .map(|&(inward, along)| Curve::Interpolated {
            params: params.clone(),
            points: params.iter().map(|&v| point(v, inward, along)).collect(),
        })
        .collect();
    Ok(Surface::ArcSweep { sections, weights })
}

fn is_identity(t: &Transform) -> bool {
    let id = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    (0..3).all(|i| (0..3).all(|j| (t.m[i][j] - id[i][j]).abs() < 1e-12))
        && t.t.iter().all(|v| v.abs() < 1e-12)
}

/// The curve of one edge of a body and its parameter range in the neutral
/// model, at a history state (`view`, as for [`convert_body_at`]), with the
/// body's transform. Fails for edges without a curve.
pub fn edge_curve(
    file: &AsmFile,
    body: usize,
    edge: usize,
    options: &Options,
    view: Option<&HashMap<usize, Option<usize>>>,
) -> Result<(Curve, [f64; 2], Option<Transform>), GeomError> {
    edge_curve_along(file, body, edge, options, view).map(|(c, t, tr, _)| (c, t, tr))
}

/// As [`edge_curve`], and whether the curve's increasing parameter runs
/// along the edge's own direction (from its start vertex to its end
/// vertex in the file, the direction its forward coedges run).
pub fn edge_curve_along(
    file: &AsmFile,
    body: usize,
    edge: usize,
    options: &Options,
    view: Option<&HashMap<usize, Option<usize>>>,
) -> Result<(Curve, [f64; 2], Option<Transform>, bool), GeomError> {
    let mut ctx = Ctx {
        file,
        view,
        opt: *options,
        body: Body::default(),
        issues: Vec::new(),
        vertex_map: HashMap::new(),
        edge_map: HashMap::new(),
        degenerate_edges: 0,
        tidied: LoopTidy::default(),
        surface_map: HashMap::new(),
    };
    let mut c = ctx.cursor(body)?;
    c.ptr()?; // lump
    c.ptr()?; // wire
    let transform = match ptr_index(file, c.ptr()?) {
        Some(t) => Some(ctx.transform(t)?).filter(|t| !is_identity(t)),
        None => None,
    };
    match ctx.edge(edge)? {
        EdgeRef::Edge(e, along) => {
            let e = &ctx.body.edges[e];
            Ok((ctx.body.curves[e.curve].clone(), e.t, transform, along))
        }
        EdgeRef::Degenerate(_) => Err(GeomError("the edge has no curve".into())),
    }
}

/// One face of a body in the neutral model, as [`face_surface`] gives it.
#[derive(Clone, Debug)]
pub struct FaceSurface {
    pub surface: Surface,
    /// The face's normal runs against the surface's natural normal (for a
    /// cone or cylinder: towards the axis).
    pub reversed: bool,
    /// Points along the face's edges, `samples` per edge.
    pub boundary: Vec<P3>,
    /// The body's transform (not applied to the above).
    pub transform: Option<Transform>,
}

/// The surface of one face of a body at a history state (`view`, as for
/// [`convert_body_at`]), with points along its edges. Fails for faces whose
/// surface is not supported.
pub fn face_surface(
    file: &AsmFile,
    body: usize,
    face: usize,
    options: &Options,
    view: Option<&HashMap<usize, Option<usize>>>,
    samples: usize,
) -> Result<FaceSurface, GeomError> {
    let mut ctx = Ctx {
        file,
        view,
        opt: *options,
        body: Body::default(),
        issues: Vec::new(),
        vertex_map: HashMap::new(),
        edge_map: HashMap::new(),
        degenerate_edges: 0,
        tidied: LoopTidy::default(),
        surface_map: HashMap::new(),
    };
    let mut c = ctx.cursor(body)?;
    c.ptr()?; // lump
    c.ptr()?; // wire
    let transform = match ptr_index(file, c.ptr()?) {
        Some(t) => Some(ctx.transform(t)?).filter(|t| !is_identity(t)),
        None => None,
    };
    let f = ctx
        .face(face)?
        .ok_or_else(|| GeomError("the face's surface is not supported".into()))?;
    let f = &ctx.body.faces[f];
    let n = samples.max(2);
    let boundary = f
        .loops
        .iter()
        .flatten()
        .flat_map(|ce| {
            let e = &ctx.body.edges[ce.edge];
            let curve = &ctx.body.curves[e.curve];
            (0..n).map(move |i| curve.eval(e.t[0] + (e.t[1] - e.t[0]) * i as f64 / (n - 1) as f64))
        })
        .collect();
    Ok(FaceSurface {
        surface: ctx.body.surfaces[f.surface].clone(),
        reversed: f.reversed,
        boundary,
        transform,
    })
}

/// Converts one `body` record.
pub fn convert_body(file: &AsmFile, record: usize, options: &Options) -> ConvertedBody {
    convert_body_at(file, record, options, None)
}

/// Converts one `body` record at an earlier history state: `view` maps
/// entities to the records of their data then (see
/// [`crate::asm::history::History::view`]).
pub fn convert_body_at(
    file: &AsmFile,
    record: usize,
    options: &Options,
    view: Option<&HashMap<usize, Option<usize>>>,
) -> ConvertedBody {
    let mut ctx = Ctx {
        file,
        view,
        opt: *options,
        body: Body::default(),
        issues: Vec::new(),
        vertex_map: HashMap::new(),
        edge_map: HashMap::new(),
        degenerate_edges: 0,
        tidied: LoopTidy::default(),
        surface_map: HashMap::new(),
    };
    let mut skipped_faces = 0;
    let result: Result<(), GeomError> = (|| {
        let mut c = ctx.cursor(record)?;
        let first_lump = c.ptr()?;
        let wire = c.ptr()?;
        let transform = c.ptr()?;
        if wire >= 0 {
            ctx.issue("body has wires; they are not read".into());
        }
        if let Some(t) = ptr_index(file, transform) {
            let t = ctx.transform(t)?;
            if !is_identity(&t) {
                ctx.body.transform = Some(t);
            }
        }
        let mut lp = ptr_index(file, first_lump);
        let mut guard = 0;
        while let Some(l) = lp {
            guard += 1;
            if guard > MAX_LIST {
                return Err(GeomError("lump list does not end".into()));
            }
            ctx.expect_type(l, "lump")?;
            let mut lc = ctx.cursor(l)?;
            let next = lc.ptr()?;
            let first_shell = lc.ptr()?;
            let mut lump = Lump::default();
            let mut sh = ptr_index(file, first_shell);
            while let Some(s) = sh {
                let (shell, skipped) = ctx.shell(s)?;
                skipped_faces += skipped;
                lump.shells.push(shell);
                let mut sc = ctx.cursor(s)?;
                sh = ptr_index(file, sc.ptr()?);
            }
            ctx.body.lumps.push(lump);
            lp = ptr_index(file, next);
        }
        Ok(())
    })();
    if let Err(e) = result {
        ctx.issue(format!("body {record}: {e}"));
    }
    ctx.body.split_disconnected_shells();
    let check = ctx.body.check(1e-3);
    ConvertedBody {
        record,
        top_level: file.records[record].type_name == "body" && file.top_level().contains(&record),
        body: ctx.body,
        issues: ctx.issues,
        skipped_faces,
        degenerate_edges: ctx.degenerate_edges,
        tidied: ctx.tidied,
        check,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(v0: usize, v1: usize) -> Edge {
        Edge {
            curve: 0,
            t: [0.0, 1.0],
            vertices: [v0, v1],
            tolerance: 1e-6,
        }
    }

    fn forward(edge: usize) -> Coedge {
        Coedge {
            edge,
            forward: true,
        }
    }

    #[test]
    fn slits_are_left_out_of_loops() {
        // A circle (edge 0, closed at vertex 0) with a slit out to vertex 1
        // and back (edge 1, both ways).
        let edges = [edge(0, 0), edge(0, 1)];
        let back = Coedge {
            edge: 1,
            forward: false,
        };
        let (loops, spurs, pinches) = tidy_loop(vec![back, forward(1), forward(0)], &edges, false);
        assert_eq!((loops, spurs, pinches), (vec![vec![forward(0)]], 1, 0));
        // Across the loop's end too.
        let (loops, spurs, _) = tidy_loop(vec![forward(1), forward(0), back], &edges, false);
        assert_eq!((loops, spurs), (vec![vec![forward(0)]], 1));
        // A loop across a periodic face with its seam edges is kept: the
        // seam's coedges do not follow one another.
        let edges = [edge(0, 0), edge(0, 1), edge(1, 1)];
        let seam = vec![forward(0), forward(1), forward(2), back];
        let (loops, spurs, pinches) = tidy_loop(seam.clone(), &edges, false);
        assert_eq!((loops, spurs, pinches), (vec![seam], 0, 0));
    }

    #[test]
    fn pinched_loops_are_split() {
        // A triangle 0-1-2 with a circle (edge 3) at vertex 0 between its
        // last and first edges, and one in the middle at vertex 1.
        let edges = [edge(0, 1), edge(1, 2), edge(2, 0), edge(0, 0), edge(1, 1)];
        let (loops, _, pinches) = tidy_loop(
            vec![forward(0), forward(1), forward(2), forward(3)],
            &edges,
            true,
        );
        assert_eq!(pinches, 1);
        assert_eq!(
            loops,
            vec![vec![forward(0), forward(1), forward(2)], vec![forward(3)]]
        );
        let (loops, _, pinches) = tidy_loop(
            vec![forward(0), forward(4), forward(1), forward(2)],
            &edges,
            true,
        );
        assert_eq!(pinches, 1);
        assert_eq!(
            loops,
            vec![vec![forward(0), forward(1), forward(2)], vec![forward(4)]]
        );
    }

    #[test]
    fn builds_thread_flanks_as_ruled_surfaces() {
        // A 60 degree flank: the line rises 0.5 along -axis per unit of
        // length and moves 0.866 outwards, from radius 1 cm; pitch 0.5 cm.
        let (c, s) = (30f64.to_radians().cos(), 0.5);
        let h = HelixLine {
            ranges: [
                [Some(0.0), Some(0.2)],
                [Some(0.0), Some(20.0)],
                [Some(0.0), Some(20.0)],
            ],
            center: [0.0; 3],
            major: [1.0, 0.0, 0.0],
            minor: [0.0, 1.0, 0.0],
            pitch: [0.0, 0.0, 0.5],
            taper: 0.0,
            axis: [0.0, 0.0, 1.0],
            line: [c, 0.0, -s],
        };
        let face = [[Some(0.0), Some(0.2)], [Some(1.0), Some(7.0)]];
        let Surface::Ruled {
            from:
                Curve::Interpolated {
                    params,
                    points: from,
                },
            to: Curve::Interpolated { points: to, .. },
        } = helix_line_surface(&h, &face, &Options::default()).unwrap()
        else {
            panic!("expected a ruled surface")
        };
        // The face's range, widened by 2 %.
        let (v0, v1) = (params[0], params[params.len() - 1]);
        assert!((v0 - 0.88).abs() < 1e-12 && (v1 - 7.12).abs() < 1e-12);
        let (u0, u1) = (-0.004, 0.204);
        for (k, &v) in params.iter().enumerate() {
            for (p, u) in [(from[k], u0), (to[k], u1)] {
                // Millimetres: radius 10 (1 + c u), height 5 v / 2 pi - 10 s u.
                let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
                assert!((r - 10.0 * (1.0 + c * u)).abs() < 1e-9);
                assert!((p[2] - (5.0 * v / std::f64::consts::TAU - 10.0 * s * u)).abs() < 1e-9);
                assert!((p[0] / r - v.cos()).abs() < 1e-9 && (p[1] / r - v.sin()).abs() < 1e-9);
            }
        }
        // The rulings are the flank: 0.208 cm long.
        let k = params.len() / 2;
        assert!((brep::dist(from[k], to[k]) - 2.08).abs() < 1e-9);
    }

    #[test]
    fn builds_thread_roots_as_arc_sweeps() {
        // A root of radius 0.0875 mm on a helix of radius 1.7 mm about y
        // (the layout of a thread root seen in a part file): its arc from
        // 120 to 180 degrees with the phase 180 degrees runs from 0.5 r
        // inwards and 0.866 r down the axis to r inwards.
        let h = HelixCircle {
            ranges: [[Some(0.0), Some(1.0)], [Some(0.0), Some(50.0)]],
            phase: std::f64::consts::PI,
            center: [0.0, 0.04, 0.0],
            major: [0.0, 0.0, -0.17],
            minor: [-0.17, 0.0, 0.0],
            pitch: [0.0, 0.07, 0.0],
            taper: 0.0,
            axis: [0.0, 1.0, 0.0],
            radius: 0.00875,
        };
        let (a0, a1) = (2.0 * std::f64::consts::FRAC_PI_3, std::f64::consts::PI);
        let face = [[Some(a0), Some(a1)], [Some(3.0), Some(40.0)]];
        let Surface::ArcSweep { sections, weights } =
            helix_circle_surface(&h, &face, &Options::default()).unwrap()
        else {
            panic!("expected an arc sweep")
        };
        // One arc (a sixth of a turn, widened by 2 %): three sections.
        let half = 0.5 * 1.04 * (a1 - a0);
        assert_eq!(weights, vec![1.0, half.cos(), 1.0]);
        let points = |c: &Curve| match c {
            Curve::Interpolated { params, points } => (params.clone(), points.clone()),
            _ => panic!("expected sampled sections"),
        };
        let (params, first) = points(&sections[0]);
        let (_, last) = points(&sections[2]);
        let (_, middle) = points(&sections[1]);
        let tau = std::f64::consts::TAU;
        for (k, &v) in params.iter().enumerate() {
            // The helix point (mm) and the radial unit at v.
            let x = [-v.sin(), 0.0, -v.cos()];
            let c = [1.7 * x[0], 0.4 + 0.7 * v / tau, 1.7 * x[2]];
            for (p, angle) in [
                (first[k], a0 - 0.02 * (a1 - a0)),
                (last[k], a1 + 0.02 * (a1 - a0)),
            ] {
                let w = angle + h.phase;
                let expected = brep::add(
                    c,
                    brep::add(
                        brep::scale(x, -0.0875 * w.cos()),
                        [0.0, 0.0875 * w.sin(), 0.0],
                    ),
                );
                assert!(brep::dist(p, expected) < 1e-9, "{p:?} {expected:?}");
            }
            // The middle control point on the bisector.
            assert!((brep::dist(middle[k], c) - 0.0875 / half.cos()).abs() < 1e-9);
        }
    }

    #[test]
    fn samples_long_thread_roots_less_densely() {
        let tau = std::f64::consts::TAU;
        let h = HelixCircle {
            ranges: [[Some(0.0), Some(1.0)], [Some(0.0), Some(1000.0 * tau)]],
            phase: 0.0,
            center: [0.0; 3],
            major: [0.0, 0.0, -0.17],
            minor: [-0.17, 0.0, 0.0],
            pitch: [0.0, 0.07, 0.0],
            taper: 0.0,
            axis: [0.0, 1.0, 0.0],
            radius: 0.00875,
        };
        let samples = |turns: f64| {
            let face = [[Some(0.0), Some(1.0)], [Some(0.0), Some(turns * tau)]];
            match helix_circle_surface(&h, &face, &Options::default()) {
                Ok(Surface::ArcSweep { sections, .. }) => match &sections[0] {
                    Curve::Interpolated { params, .. } => Ok(params.len() - 1),
                    _ => panic!("expected sampled sections"),
                },
                Ok(_) => panic!("expected an arc sweep"),
                Err(e) => Err(e),
            }
        };
        // 40 turns (widened by 2 % at each end) at 48 a turn; 81 at fewer, within the
        // most samples; 200 turns would need fewer than the fewest a turn.
        assert_eq!(
            samples(40.0).unwrap(),
            (40.0 * 1.04 * 48.0_f64).ceil() as usize
        );
        assert_eq!(samples(81.0).unwrap(), MAX_ARC_SWEEP_SAMPLES);
        assert!(samples(200.0).is_err());
    }
}
