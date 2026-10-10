// SPDX-License-Identifier: MIT
//! Neutral B-rep model (topology and geometry, millimetres), independent of
//! the ASM format and of OCCT, with consistency checks.
//!
//! Conventions:
//! - Every edge runs along increasing parameter of its curve, from
//!   `vertices[0]` at `t[0]` to `vertices[1]` at `t[1]` (`t[0] < t[1]`).
//! - A coedge uses its edge forward (`forward`) or reversed.
//! - Loops are oriented so that the face lies on the left of the coedges
//!   when viewed against the face's material normal. The material normal is
//!   the surface's natural normal (see [`Surface`]), negated when
//!   `Face::reversed` is set.

pub type P3 = [f64; 3];

pub fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn scale(a: P3, s: f64) -> P3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn norm(a: P3) -> f64 {
    dot(a, a).sqrt()
}
pub fn normalize(a: P3) -> P3 {
    let n = norm(a);
    if n > 0.0 { scale(a, 1.0 / n) } else { a }
}
pub fn dist(a: P3, b: P3) -> f64 {
    norm(sub(a, b))
}

/// Non-rational or rational B-spline curve in OCCT form: distinct knots with
/// multiplicities, `sum(mults) == poles.len() + degree + 1`.
#[derive(Clone, Debug, PartialEq)]
pub struct BSplineCurve {
    pub degree: usize,
    pub knots: Vec<f64>,
    pub mults: Vec<usize>,
    pub poles: Vec<P3>,
    pub weights: Option<Vec<f64>>,
    /// Closed curve whose parameter wraps around (edges may cross the
    /// seam); the poles are still those of a clamped curve.
    pub periodic: bool,
}

/// B-spline curve in a surface's parameter space.
#[derive(Clone, Debug, PartialEq)]
pub struct BSplineCurve2d {
    pub degree: usize,
    pub knots: Vec<f64>,
    pub mults: Vec<usize>,
    pub poles: Vec<[f64; 2]>,
    pub weights: Option<Vec<f64>>,
}

/// Tensor-product B-spline surface; `poles[iu * nv + iv]`.
#[derive(Clone, Debug, PartialEq)]
pub struct BSplineSurface {
    pub u_degree: usize,
    pub v_degree: usize,
    pub u_knots: Vec<f64>,
    pub u_mults: Vec<usize>,
    pub v_knots: Vec<f64>,
    pub v_mults: Vec<usize>,
    pub nu: usize,
    pub nv: usize,
    pub poles: Vec<P3>,
    pub weights: Option<Vec<f64>>,
    /// Periodic in u or v (stored clamped and closed): faces may cross
    /// the seam, as edges may on periodic curves.
    pub u_periodic: bool,
    pub v_periodic: bool,
}

fn full_knots(knots: &[f64], mults: &[usize]) -> Vec<f64> {
    let mut out = Vec::new();
    for (k, m) in knots.iter().zip(mults) {
        for _ in 0..*m {
            out.push(*k);
        }
    }
    out
}

/// De Boor evaluation of homogeneous points `cw` (x, y, z, w).
fn de_boor(degree: usize, flat: &[f64], cw: &[[f64; 4]], t: f64) -> [f64; 4] {
    let n = cw.len();
    if n == 0 {
        return [0.0; 4];
    }
    let p = degree.min(n - 1);
    // Span index k: flat[k] <= t < flat[k+1], limited to valid spans.
    let lo = p;
    let hi = n - 1;
    let mut k = lo;
    while k < hi && flat[k + 1] <= t {
        k += 1;
    }
    let mut d: Vec<[f64; 4]> = (0..=p).map(|j| cw[k + j - p]).collect();
    for r in 1..=p {
        for j in (r..=p).rev() {
            let i = k + j - p;
            let denom = flat[i + p + 1 - r] - flat[i];
            let a = if denom.abs() < 1e-300 {
                0.0
            } else {
                (t - flat[i]) / denom
            };
            let prev = d[j - 1];
            for (x, p) in d[j].iter_mut().zip(prev) {
                *x = (1.0 - a) * p + a * *x;
            }
        }
    }
    d[p]
}

impl BSplineCurve {
    pub fn is_valid(&self) -> bool {
        self.knots.len() == self.mults.len()
            && self.knots.len() >= 2
            && self.mults.iter().sum::<usize>() == self.poles.len() + self.degree + 1
            && self.knots.windows(2).all(|w| w[0] < w[1])
            && self
                .weights
                .as_ref()
                .is_none_or(|w| w.len() == self.poles.len())
    }

    /// Parameter range where the curve is defined.
    pub fn range(&self) -> (f64, f64) {
        let flat = full_knots(&self.knots, &self.mults);
        (flat[self.degree], flat[flat.len() - 1 - self.degree])
    }

    pub fn eval(&self, t: f64) -> P3 {
        let flat = full_knots(&self.knots, &self.mults);
        let (a, b) = (flat[self.degree], flat[flat.len() - 1 - self.degree]);
        let t = if self.periodic && b > a && !(a..=b).contains(&t) {
            a + (t - a).rem_euclid(b - a)
        } else {
            t
        };
        let cw: Vec<[f64; 4]> = self
            .poles
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let w = self.weights.as_ref().map_or(1.0, |w| w[i]);
                [p[0] * w, p[1] * w, p[2] * w, w]
            })
            .collect();
        let h = de_boor(self.degree, &flat, &cw, t);
        [h[0] / h[3], h[1] / h[3], h[2] / h[3]]
    }
}

impl BSplineCurve2d {
    pub fn is_valid(&self) -> bool {
        self.knots.len() == self.mults.len()
            && self.knots.len() >= 2
            && self.mults.iter().sum::<usize>() == self.poles.len() + self.degree + 1
            && self.knots.windows(2).all(|w| w[0] < w[1])
    }
}

impl BSplineSurface {
    pub fn is_valid(&self) -> bool {
        self.u_knots.len() == self.u_mults.len()
            && self.v_knots.len() == self.v_mults.len()
            && self.u_knots.len() >= 2
            && self.v_knots.len() >= 2
            && self.u_mults.iter().sum::<usize>() == self.nu + self.u_degree + 1
            && self.v_mults.iter().sum::<usize>() == self.nv + self.v_degree + 1
            && self.poles.len() == self.nu * self.nv
            && self.u_knots.windows(2).all(|w| w[0] < w[1])
            && self.v_knots.windows(2).all(|w| w[0] < w[1])
            && self
                .weights
                .as_ref()
                .is_none_or(|w| w.len() == self.poles.len())
    }

    pub fn u_range(&self) -> (f64, f64) {
        let f = full_knots(&self.u_knots, &self.u_mults);
        (f[self.u_degree], f[f.len() - 1 - self.u_degree])
    }

    pub fn v_range(&self) -> (f64, f64) {
        let f = full_knots(&self.v_knots, &self.v_mults);
        (f[self.v_degree], f[f.len() - 1 - self.v_degree])
    }

    pub fn eval(&self, u: f64, v: f64) -> P3 {
        let fu = full_knots(&self.u_knots, &self.u_mults);
        let fv = full_knots(&self.v_knots, &self.v_mults);
        let mut col = Vec::with_capacity(self.nu);
        for iu in 0..self.nu {
            let row: Vec<[f64; 4]> = (0..self.nv)
                .map(|iv| {
                    let i = iu * self.nv + iv;
                    let p = self.poles[i];
                    let w = self.weights.as_ref().map_or(1.0, |w| w[i]);
                    [p[0] * w, p[1] * w, p[2] * w, w]
                })
                .collect();
            col.push(de_boor(self.v_degree, &fv, &row, v));
        }
        let h = de_boor(self.u_degree, &fu, &col, u);
        [h[0] / h[3], h[1] / h[3], h[2] / h[3]]
    }

    /// Normal du x dv by central differences.
    pub fn normal(&self, u: f64, v: f64) -> P3 {
        let (u0, u1) = self.u_range();
        let (v0, v1) = self.v_range();
        let hu = (u1 - u0) * 1e-5;
        let hv = (v1 - v0) * 1e-5;
        let du = sub(
            self.eval((u + hu).min(u1), v),
            self.eval((u - hu).max(u0), v),
        );
        let dv = sub(
            self.eval(u, (v + hv).min(v1)),
            self.eval(u, (v - hv).max(v0)),
        );
        normalize(cross(du, dv))
    }
}

/// 3D curve. Parameterisation as in OCCT: lines by arc length, ellipses by
/// angle from the major axis towards `normal x major`.
#[derive(Clone, Debug, PartialEq)]
pub enum Curve {
    Line {
        origin: P3,
        dir: P3,
    },
    /// Circle when `minor == major`.
    Ellipse {
        center: P3,
        normal: P3,
        major_dir: P3,
        major: f64,
        minor: f64,
    },
    BSpline(BSplineCurve),
    /// Curve known by points at parameters; the builder interpolates it
    /// (used for helices).
    Interpolated {
        params: Vec<f64>,
        points: Vec<P3>,
    },
}

impl Curve {
    pub fn eval(&self, t: f64) -> P3 {
        match self {
            Curve::Line { origin, dir } => add(*origin, scale(*dir, t)),
            Curve::Ellipse {
                center,
                normal,
                major_dir,
                major,
                minor,
            } => {
                let y = cross(*normal, *major_dir);
                add(
                    *center,
                    add(
                        scale(*major_dir, major * t.cos()),
                        scale(y, minor * t.sin()),
                    ),
                )
            }
            Curve::BSpline(b) => b.eval(t),
            Curve::Interpolated { params, points } => {
                // Piecewise linear, enough for checks.
                let i = params
                    .partition_point(|&p| p <= t)
                    .clamp(1, params.len() - 1);
                let (a, b) = (params[i - 1], params[i]);
                let s = if b > a { (t - a) / (b - a) } else { 0.0 };
                add(points[i - 1], scale(sub(points[i], points[i - 1]), s))
            }
        }
    }

    /// Parameter range of bounded curves (B-splines, interpolated curves).
    pub fn range(&self) -> Option<(f64, f64)> {
        match self {
            Curve::BSpline(b) => Some(b.range()),
            Curve::Interpolated { params, .. } => Some((*params.first()?, *params.last()?)),
            _ => None,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Curve::Line { .. } => "line",
            Curve::Ellipse { major, minor, .. } if (major - minor).abs() <= 1e-12 * major.abs() => {
                "circle"
            }
            Curve::Ellipse { .. } => "ellipse",
            Curve::BSpline(_) => "bspline",
            Curve::Interpolated { .. } => "interpolated",
        }
    }
}

/// Surface. The natural normal of each kind:
/// - `Plane`: `normal`.
/// - `Cone`: away from the axis (OCCT `Geom_ConicalSurface` with a direct
///   frame); `half_angle` in (-pi/2, pi/2), radius grows along `axis` when
///   it is positive. Cylinder when `half_angle == 0`.
/// - `Sphere`, `Torus`: outward (OCCT convention, direct frame).
/// - `BSpline`: du x dv.
/// - `Extrusion`: dcurve/du x dir.
/// - `Revolution`: OCCT `Geom_SurfaceOfRevolution` (du x dv, u around the
///   axis, v along the curve).
/// - `Ruled`: du x dv of `S(u, v) = (1 - u) from(v) + u to(v)`, `u` in
///   [0, 1].
/// - `ArcSweep`: du x dv, `u` in [0, 1] along the arcs.
#[derive(Clone, Debug, PartialEq)]
pub enum Surface {
    Plane {
        origin: P3,
        normal: P3,
        u_dir: P3,
    },
    Cone {
        origin: P3,
        axis: P3,
        ref_dir: P3,
        radius: f64,
        half_angle: f64,
    },
    Sphere {
        center: P3,
        axis: P3,
        ref_dir: P3,
        radius: f64,
    },
    Torus {
        center: P3,
        axis: P3,
        ref_dir: P3,
        major: f64,
        minor: f64,
    },
    BSpline(BSplineSurface),
    Extrusion {
        curve: Curve,
        dir: P3,
    },
    Revolution {
        curve: Curve,
        origin: P3,
        axis: P3,
    },
    /// Straight lines between corresponding points of two curves with the
    /// same parameter range (used for thread flanks, between two helices
    /// sampled at the same parameters).
    Ruled {
        from: Curve,
        to: Curve,
    },
    /// Circular arcs through corresponding points of curves with the same
    /// parameter range: for each v, the rational quadratic B-spline (in u,
    /// knots spaced evenly in [0, 1], one arc per two spans) whose control
    /// points are the curves' points at v and whose weights are `weights`
    /// (used for the rounded roots of threads, between helices sampled at
    /// the same parameters).
    ArcSweep {
        sections: Vec<Curve>,
        weights: Vec<f64>,
    },
}

impl Surface {
    pub fn kind(&self) -> &'static str {
        match self {
            Surface::Ruled { .. } => "ruled",
            Surface::ArcSweep { .. } => "arc sweep",
            Surface::Plane { .. } => "plane",
            Surface::Cone { half_angle, .. } if *half_angle == 0.0 => "cylinder",
            Surface::Cone { .. } => "cone",
            Surface::Sphere { .. } => "sphere",
            Surface::Torus { .. } => "torus",
            Surface::BSpline(_) => "bspline",
            Surface::Extrusion { .. } => "extrusion",
            Surface::Revolution { .. } => "revolution",
        }
    }

    /// Distance from a point to the (untrimmed) surface, when cheap to
    /// compute; `None` for free-form surfaces.
    pub fn distance(&self, p: P3) -> Option<f64> {
        match self {
            Surface::Plane { origin, normal, .. } => Some(dot(sub(p, *origin), *normal).abs()),
            Surface::Cone {
                origin,
                axis,
                radius,
                half_angle,
                ..
            } => {
                let d = sub(p, *origin);
                let h = dot(d, *axis);
                let rho = norm(sub(d, scale(*axis, h)));
                // Distance in the meridian half-plane to the line
                // r = radius + h tan(a).
                let (s, c) = half_angle.sin_cos();
                Some(((rho - radius) * c - h * s).abs())
            }
            Surface::Sphere { center, radius, .. } => Some((dist(p, *center) - radius).abs()),
            Surface::Torus {
                center,
                axis,
                major,
                minor,
                ..
            } => {
                let d = sub(p, *center);
                let h = dot(d, *axis);
                let rho = norm(sub(d, scale(*axis, h)));
                Some(((rho - major).hypot(h) - minor).abs())
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Vertex {
    pub point: P3,
    pub tolerance: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub curve: usize,
    pub t: [f64; 2],
    pub vertices: [usize; 2],
    pub tolerance: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Coedge {
    pub edge: usize,
    pub forward: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub surface: usize,
    pub reversed: bool,
    pub double_sided: bool,
    pub loops: Vec<Vec<Coedge>>,
    /// Loops that are a single point: vertices where the face closes up
    /// at a singular point of its surface (a cone apex, a sphere pole).
    pub point_loops: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Shell {
    pub faces: Vec<usize>,
    /// Edges of the shell that bound no face (wire parts).
    pub wire_edges: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Lump {
    pub shells: Vec<Shell>,
}

/// Affine map: `p' = m * p + t` (rows of `m`), already in millimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub m: [[f64; 3]; 3],
    pub t: P3,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Body {
    pub lumps: Vec<Lump>,
    pub faces: Vec<Face>,
    pub edges: Vec<Edge>,
    pub vertices: Vec<Vertex>,
    pub curves: Vec<Curve>,
    pub surfaces: Vec<Surface>,
    pub transform: Option<Transform>,
}

/// Result of [`Body::check`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CheckReport {
    /// Edges used by one coedge only: the boundary of a sheet.
    pub free_edges: usize,
    /// Edges used more often in one direction than in the other (outside
    /// double-sided faces): inconsistent orientation.
    pub unpaired_coedges: usize,
    /// Loops whose coedges do not connect end to start.
    pub open_loops: usize,
    /// Edge ends farther than the tolerance from their vertex.
    pub vertex_mismatches: usize,
    /// Largest vertex-to-curve-end distance (mm).
    pub max_vertex_gap: f64,
    /// Vertices farther than the tolerance from an analytic face surface.
    pub off_surface_vertices: usize,
    pub max_surface_gap: f64,
    pub messages: Vec<String>,
}

impl CheckReport {
    pub fn is_clean(&self) -> bool {
        self.unpaired_coedges == 0 && self.open_loops == 0 && self.vertex_mismatches == 0
    }
}

impl Coedge {
    fn start_end(&self, body: &Body) -> (usize, usize) {
        let e = &body.edges[self.edge];
        if self.forward {
            (e.vertices[0], e.vertices[1])
        } else {
            (e.vertices[1], e.vertices[0])
        }
    }
}

impl Body {
    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    /// Per edge of the shell: (forward uses, reversed uses).
    fn edge_uses(&self, shell: &Shell) -> std::collections::HashMap<usize, (usize, usize)> {
        let mut uses: std::collections::HashMap<usize, (usize, usize)> =
            std::collections::HashMap::new();
        for &fi in &shell.faces {
            for c in self.faces[fi].loops.iter().flatten() {
                let u = uses.entry(c.edge).or_insert((0, 0));
                if c.forward {
                    u.0 += 1;
                } else {
                    u.1 += 1;
                }
            }
        }
        uses
    }

    /// The shell bounds a volume: it has faces, every edge is used as often
    /// in one direction as in the other, and no face is double-sided. An
    /// edge is mostly used once each way; ASM also puts the seam of a full
    /// cylinder on the line where it touches planar faces (a hole tangent to
    /// a wall), an edge used twice each way.
    pub fn shell_is_closed(&self, shell: &Shell) -> bool {
        !shell.faces.is_empty()
            && shell.faces.iter().all(|&f| !self.faces[f].double_sided)
            && self
                .edge_uses(shell)
                .values()
                .all(|&(a, b)| a == b && a >= 1)
    }

    /// Splits shells whose faces fall into groups that share no edge (ASM
    /// allows that in sheet bodies, OCCT does not). The groups of a lump's
    /// only shell are lumps of their own: pieces side by side, not one
    /// inside another (a lump's other shells are its voids, which ASM keeps
    /// as shells of their own).
    pub fn split_disconnected_shells(&mut self) {
        let mut pieces = Vec::new();
        for li in 0..self.lumps.len() {
            let single = self.lumps[li].shells.len() == 1;
            let mut out = Vec::new();
            for shell in std::mem::take(&mut self.lumps[li].shells) {
                let n = shell.faces.len();
                let mut parent: Vec<usize> = (0..n).collect();
                fn find(p: &mut [usize], mut i: usize) -> usize {
                    while p[i] != i {
                        p[i] = p[p[i]];
                        i = p[i];
                    }
                    i
                }
                let mut by_edge: std::collections::HashMap<usize, usize> =
                    std::collections::HashMap::new();
                for (k, &fi) in shell.faces.iter().enumerate() {
                    for c in self.faces[fi].loops.iter().flatten() {
                        if let Some(&other) = by_edge.get(&c.edge) {
                            let (a, b) = (find(&mut parent, k), find(&mut parent, other));
                            parent[a] = b;
                        } else {
                            by_edge.insert(c.edge, k);
                        }
                    }
                }
                let mut groups: Vec<(usize, Shell)> = Vec::new();
                for (k, &fi) in shell.faces.iter().enumerate() {
                    let root = find(&mut parent, k);
                    match groups.iter_mut().find(|(r, _)| *r == root) {
                        Some((_, s)) => s.faces.push(fi),
                        None => groups.push((
                            root,
                            Shell {
                                faces: vec![fi],
                                wire_edges: Vec::new(),
                            },
                        )),
                    }
                }
                if groups.len() <= 1 {
                    out.push(shell);
                } else if single {
                    let mut groups = groups.into_iter().map(|(_, s)| s);
                    out.extend(groups.next());
                    pieces.extend(groups.map(|s| Lump { shells: vec![s] }));
                } else {
                    out.extend(groups.into_iter().map(|(_, s)| s));
                }
            }
            self.lumps[li].shells = out;
        }
        self.lumps.extend(pieces);
    }

    /// Every shell of every lump is closed: the body is a solid.
    pub fn is_solid(&self) -> bool {
        !self.lumps.is_empty()
            && self
                .lumps
                .iter()
                .all(|l| l.shells.iter().all(|s| self.shell_is_closed(s)))
    }

    /// Checks topology and geometry consistency. `tolerance` (mm) is the
    /// allowed vertex gap in addition to the vertex and edge tolerances.
    pub fn check(&self, tolerance: f64) -> CheckReport {
        let mut r = CheckReport::default();
        let note = |r: &mut CheckReport, m: String| {
            if r.messages.len() < 20 {
                r.messages.push(m);
            }
        };
        for (fi, f) in self.faces.iter().enumerate() {
            for (li, lp) in f.loops.iter().enumerate() {
                let n = lp.len();
                let mut open = false;
                for i in 0..n {
                    let (_, end) = lp[i].start_end(self);
                    let (start, _) = lp[(i + 1) % n].start_end(self);
                    if end != start {
                        let gap = dist(self.vertices[end].point, self.vertices[start].point);
                        if gap > tolerance {
                            open = true;
                        }
                    }
                }
                if open {
                    r.open_loops += 1;
                    note(&mut r, format!("face {fi} loop {li} is open"));
                }
            }
        }
        // An inner edge of a shell is used as often in each direction (once,
        // or twice at a cylinder's seam on planar faces it touches).
        for lump in &self.lumps {
            for shell in &lump.shells {
                let uses = self.edge_uses(shell);
                let sheet = shell.faces.iter().any(|&f| self.faces[f].double_sided);
                r.free_edges += uses.values().filter(|(a, b)| a + b == 1).count();
                if !sheet {
                    let mut bad: Vec<(usize, (usize, usize))> = uses
                        .iter()
                        .filter(|(_, (a, b))| a + b >= 2 && a != b)
                        .map(|(e, u)| (*e, *u))
                        .collect();
                    bad.sort_unstable();
                    r.unpaired_coedges += bad.len();
                    if !bad.is_empty() {
                        let first: Vec<String> = bad
                            .iter()
                            .take(4)
                            .map(|(e, (a, b))| format!("edge {e} ({a} forward, {b} reversed)"))
                            .collect();
                        // The faces at the first such edge.
                        let at: Vec<String> = shell
                            .faces
                            .iter()
                            .flat_map(|&fi| {
                                self.faces[fi]
                                    .loops
                                    .iter()
                                    .flatten()
                                    .filter(|c| c.edge == bad[0].0)
                                    .map(move |c| (fi, c.forward))
                            })
                            .map(|(fi, forward)| {
                                format!(
                                    "face {fi} ({}{}{})",
                                    self.surfaces[self.faces[fi].surface].kind(),
                                    if self.faces[fi].reversed {
                                        ", reversed"
                                    } else {
                                        ""
                                    },
                                    if forward { "" } else { ", coedge reversed" }
                                )
                            })
                            .collect();
                        note(
                            &mut r,
                            format!(
                                "{} edges used inconsistently: {}; at edge {}: {}",
                                bad.len(),
                                first.join(", "),
                                bad[0].0,
                                at.join(", ")
                            ),
                        );
                    }
                }
            }
        }
        for (ei, e) in self.edges.iter().enumerate() {
            let c = &self.curves[e.curve];
            for k in 0..2 {
                let v = &self.vertices[e.vertices[k]];
                let gap = dist(c.eval(e.t[k]), v.point);
                r.max_vertex_gap = r.max_vertex_gap.max(gap);
                if gap > tolerance + v.tolerance + e.tolerance {
                    r.vertex_mismatches += 1;
                    note(
                        &mut r,
                        format!(
                            "edge {ei} ({}) end {k} is {gap:.3e} mm from its vertex",
                            c.kind()
                        ),
                    );
                }
            }
        }
        for (fi, f) in self.faces.iter().enumerate() {
            let s = &self.surfaces[f.surface];
            for c in f.loops.iter().flatten() {
                for &vi in &self.edges[c.edge].vertices {
                    let v = &self.vertices[vi];
                    if let Some(d) = s.distance(v.point) {
                        r.max_surface_gap = r.max_surface_gap.max(d);
                        if d > tolerance + v.tolerance {
                            r.off_surface_vertices += 1;
                            note(
                                &mut r,
                                format!("face {fi} ({}) vertex {vi} is {d:.3e} mm off", s.kind()),
                            );
                        }
                    }
                }
            }
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One closed edge: two faces use it once each way, a third twice (its
    /// seam), as ASM stores a full cylinder touching a wall.
    fn seam_on_a_wall(third: [bool; 2]) -> Body {
        let face = |loops: Vec<Vec<Coedge>>| Face {
            surface: 0,
            reversed: false,
            double_sided: false,
            loops,
            point_loops: Vec::new(),
        };
        let use_ = |forward: bool| Coedge { edge: 0, forward };
        Body {
            lumps: vec![Lump {
                shells: vec![Shell {
                    faces: vec![0, 1, 2],
                    wire_edges: Vec::new(),
                }],
            }],
            faces: vec![
                face(vec![vec![use_(true)]]),
                face(vec![vec![use_(false)]]),
                face(vec![vec![use_(third[0])], vec![use_(third[1])]]),
            ],
            edges: vec![Edge {
                curve: 0,
                t: [0.0, 0.0],
                vertices: [0, 0],
                tolerance: 0.0,
            }],
            vertices: vec![Vertex {
                point: [0.0; 3],
                tolerance: 0.0,
            }],
            curves: vec![Curve::Line {
                origin: [0.0; 3],
                dir: [1.0, 0.0, 0.0],
            }],
            surfaces: vec![Surface::Plane {
                origin: [0.0; 3],
                normal: [0.0, 0.0, 1.0],
                u_dir: [1.0, 0.0, 0.0],
            }],
            transform: None,
        }
    }

    #[test]
    fn edges_used_as_often_each_way_close_a_shell() {
        let balanced = seam_on_a_wall([true, false]);
        assert!(balanced.is_solid());
        assert!(balanced.check(1e-3).is_clean());
        let unbalanced = seam_on_a_wall([true, true]);
        assert!(!unbalanced.is_solid());
        let report = unbalanced.check(1e-3);
        assert_eq!(report.unpaired_coedges, 1);
        assert!(report.messages[0].contains("edge 0 (3 forward, 1 reversed)"));
    }

    #[test]
    fn evaluates_clamped_cubic_bspline() {
        let b = BSplineCurve {
            degree: 3,
            knots: vec![0.0, 1.0],
            mults: vec![4, 4],
            poles: vec![
                [0.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [2.0, 1.0, 0.0],
                [3.0, 0.0, 0.0],
            ],
            weights: None,
            periodic: false,
        };
        assert!(b.is_valid());
        assert_eq!(b.eval(0.0), [0.0, 0.0, 0.0]);
        assert_eq!(b.eval(1.0), [3.0, 0.0, 0.0]);
        let m = b.eval(0.5);
        assert!((m[0] - 1.5).abs() < 1e-12 && (m[1] - 0.75).abs() < 1e-12);
    }

    #[test]
    fn evaluates_rational_quarter_circle() {
        let w = std::f64::consts::FRAC_1_SQRT_2;
        let b = BSplineCurve {
            degree: 2,
            knots: vec![0.0, 1.0],
            mults: vec![3, 3],
            poles: vec![[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
            weights: Some(vec![1.0, w, 1.0]),
            periodic: false,
        };
        for i in 0..=10 {
            let p = b.eval(i as f64 / 10.0);
            assert!((norm(p) - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn surface_distances() {
        let cyl = Surface::Cone {
            origin: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            ref_dir: [1.0, 0.0, 0.0],
            radius: 2.0,
            half_angle: 0.0,
        };
        assert!((cyl.distance([3.0, 0.0, 5.0]).unwrap() - 1.0).abs() < 1e-12);
        let cone = Surface::Cone {
            origin: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            ref_dir: [1.0, 0.0, 0.0],
            radius: 1.0,
            half_angle: std::f64::consts::FRAC_PI_4,
        };
        assert!(cone.distance([2.0, 0.0, 1.0]).unwrap() < 1e-12);
        let torus = Surface::Torus {
            center: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            ref_dir: [1.0, 0.0, 0.0],
            major: 5.0,
            minor: 1.0,
        };
        assert!(torus.distance([6.0, 0.0, 0.0]).unwrap() < 1e-12);
    }
}
