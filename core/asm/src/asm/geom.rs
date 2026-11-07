// SPDX-License-Identifier: MIT
//! Decoding of ASM geometry records and subtype objects into ASM-level
//! geometry (model units, ASM conventions). [`crate::convert`] maps these to
//! the neutral model.

use std::fmt;

use super::file::AsmFile;
use super::token::Token;
use crate::brep::P3;

#[derive(Clone, Debug, PartialEq)]
pub struct GeomError(pub String);

impl fmt::Display for GeomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for GeomError {}

pub type Result<T> = std::result::Result<T, GeomError>;

fn err<T>(msg: impl Into<String>) -> Result<T> {
    Err(GeomError(msg.into()))
}

/// Reads typed values from a token slice.
#[derive(Clone)]
pub struct Cursor<'a> {
    pub toks: &'a [Token],
    pub pos: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(toks: &'a [Token], pos: usize) -> Self {
        Cursor { toks, pos }
    }

    pub fn peek(&self) -> Option<&'a Token> {
        self.toks.get(self.pos)
    }

    pub fn next_token(&mut self) -> Result<&'a Token> {
        let t = self
            .toks
            .get(self.pos)
            .ok_or_else(|| GeomError("unexpected end of data".into()))?;
        self.pos += 1;
        Ok(t)
    }

    fn unexpected<T>(&self, what: &str, t: &Token) -> Result<T> {
        err(format!(
            "expected {what}, got {t} at token {}",
            self.pos - 1
        ))
    }

    pub fn int(&mut self) -> Result<i64> {
        match self.next_token()? {
            Token::Int(v) => Ok(*v),
            t => self.unexpected("an integer", t),
        }
    }

    /// An enumeration value; the text form (`asm::text`) writes them as
    /// words.
    pub fn enumeration(&mut self) -> Result<i64> {
        match self.next_token()? {
            Token::Enum(v) => Ok(*v),
            Token::Ident(word) => match super::text::enumeration(word) {
                Some(v) => Ok(v),
                None => self.unexpected("an enum", &Token::Ident(word.clone())),
            },
            t => self.unexpected("an enum", t),
        }
    }

    pub fn double(&mut self) -> Result<f64> {
        match self.next_token()? {
            Token::Double(v) => Ok(*v),
            Token::Int(v) => Ok(*v as f64),
            t => self.unexpected("a double", t),
        }
    }

    pub fn boolean(&mut self) -> Result<bool> {
        match self.next_token()? {
            Token::True => Ok(true),
            Token::False => Ok(false),
            t => self.unexpected("a boolean", t),
        }
    }

    pub fn ptr(&mut self) -> Result<i64> {
        match self.next_token()? {
            Token::Ptr(v) => Ok(*v),
            t => self.unexpected("a pointer", t),
        }
    }

    /// A position or vector; the text form (`asm::text`) writes them as
    /// three numbers.
    pub fn point(&mut self) -> Result<P3> {
        match self.next_token()? {
            Token::Pos(p) | Token::Vector(p) => Ok(*p),
            first @ (Token::Double(_) | Token::Int(_)) => {
                let x = match first {
                    Token::Double(v) => *v,
                    Token::Int(v) => *v as f64,
                    _ => unreachable!(),
                };
                Ok([x, self.double()?, self.double()?])
            }
            t => self.unexpected("a position or vector", t),
        }
    }

    pub fn ident(&mut self) -> Result<&'a str> {
        match self.next_token()? {
            Token::Ident(s) => Ok(s),
            t => self.unexpected("an identifier", t),
        }
    }

    pub fn string(&mut self) -> Result<&'a str> {
        match self.next_token()? {
            Token::Str(s) | Token::Literal(s) => Ok(s),
            t => self.unexpected("a string", t),
        }
    }

    /// One end of a parameter interval: `F` (unbounded) or `T <double>`.
    pub fn interval_end(&mut self) -> Result<Option<f64>> {
        if self.boolean()? {
            Ok(Some(self.double()?))
        } else {
            Ok(None)
        }
    }

    pub fn interval(&mut self) -> Result<[Option<f64>; 2]> {
        Ok([self.interval_end()?, self.interval_end()?])
    }

    /// Skips a balanced `{ ... }` starting at the current `{`.
    pub fn skip_subtype(&mut self) -> Result<()> {
        if self.next_token()? != &Token::SubStart {
            return err("expected '{'");
        }
        let mut depth = 1;
        while depth > 0 {
            match self.next_token()? {
                Token::SubStart => depth += 1,
                Token::SubEnd => depth -= 1,
                _ => {}
            }
        }
        Ok(())
    }

    /// Index one past the `}` matching the `{` at `self.pos`.
    pub fn subtype_end(&self) -> Result<usize> {
        let mut c = self.clone();
        c.skip_subtype()?;
        Ok(c.pos)
    }
}

/// B-spline curve as stored: ASM multiplicities (end knots have
/// multiplicity `degree`), control points with weights (1 if polynomial).
#[derive(Clone, Debug, PartialEq)]
pub struct Bs3Curve {
    pub degree: usize,
    pub rational: bool,
    /// 0 open, 1 closed, 2 periodic.
    pub closure: i64,
    pub knots: Vec<f64>,
    pub mults: Vec<usize>,
    pub ctrl: Vec<[f64; 4]>,
}

/// 2D B-spline (pcurves); points are (u, v, w).
#[derive(Clone, Debug, PartialEq)]
pub struct Bs2Curve {
    pub degree: usize,
    pub rational: bool,
    pub closure: i64,
    pub knots: Vec<f64>,
    pub mults: Vec<usize>,
    pub ctrl: Vec<[f64; 3]>,
}

/// B-spline surface as stored; `ctrl[iv * nu + iu]` (u varies fastest).
#[derive(Clone, Debug, PartialEq)]
pub struct Bs3Surface {
    pub u_degree: usize,
    pub v_degree: usize,
    pub rational: bool,
    pub u_closure: i64,
    pub v_closure: i64,
    pub u_knots: Vec<f64>,
    pub u_mults: Vec<usize>,
    pub v_knots: Vec<f64>,
    pub v_mults: Vec<usize>,
    pub nu: usize,
    pub nv: usize,
    pub ctrl: Vec<[f64; 4]>,
}

fn knot_vector(c: &mut Cursor) -> Result<(Vec<f64>, Vec<usize>)> {
    let n = c.int()?;
    knots_of_count(c, n)
}

/// `n` pairs of knot value and multiplicity.
fn knots_of_count(c: &mut Cursor, n: i64) -> Result<(Vec<f64>, Vec<usize>)> {
    if !(1..=100_000).contains(&n) {
        return err(format!("bad knot count {n}"));
    }
    let mut knots = Vec::with_capacity(n as usize);
    let mut mults = Vec::with_capacity(n as usize);
    for _ in 0..n {
        knots.push(c.double()?);
        let m = c.int()?;
        if !(1..=64).contains(&m) {
            return err(format!("bad knot multiplicity {m}"));
        }
        mults.push(m as usize);
    }
    Ok((knots, mults))
}

fn pole_count(mults: &[usize], degree: usize) -> Result<usize> {
    let sum: usize = mults.iter().sum();
    if sum < degree {
        return err("knot vector too short for the degree");
    }
    Ok(sum + 1 - degree)
}

fn read_degree(c: &mut Cursor) -> Result<usize> {
    let d = c.int()?;
    if !(1..=25).contains(&d) {
        return err(format!("bad degree {d}"));
    }
    Ok(d as usize)
}

/// Reads `nubs|nurbs|nullbs ...`; `None` for `nullbs`.
pub fn bs3_curve(c: &mut Cursor) -> Result<Option<Bs3Curve>> {
    let kind = c.ident()?;
    let rational = match kind {
        "nullbs" => return Ok(None),
        "nubs" => false,
        "nurbs" => true,
        other => return err(format!("expected a B-spline curve, got {other}")),
    };
    let degree = read_degree(c)?;
    let closure = c.enumeration()?;
    let (knots, mults) = knot_vector(c)?;
    let n = pole_count(&mults, degree)?;
    let mut ctrl = Vec::with_capacity(n);
    for _ in 0..n {
        let p = [c.double()?, c.double()?, c.double()?];
        let w = if rational { c.double()? } else { 1.0 };
        ctrl.push([p[0], p[1], p[2], w]);
    }
    Ok(Some(Bs3Curve {
        degree,
        rational,
        closure,
        knots,
        mults,
        ctrl,
    }))
}

pub fn bs2_curve(c: &mut Cursor) -> Result<Option<Bs2Curve>> {
    let kind = c.ident()?;
    let rational = match kind {
        "nullbs" => return Ok(None),
        "nubs" => false,
        "nurbs" => true,
        other => return err(format!("expected a 2D B-spline, got {other}")),
    };
    let degree = read_degree(c)?;
    let closure = c.enumeration()?;
    let (knots, mults) = knot_vector(c)?;
    let n = pole_count(&mults, degree)?;
    let mut ctrl = Vec::with_capacity(n);
    for _ in 0..n {
        let p = [c.double()?, c.double()?];
        let w = if rational { c.double()? } else { 1.0 };
        ctrl.push([p[0], p[1], w]);
    }
    Ok(Some(Bs2Curve {
        degree,
        rational,
        closure,
        knots,
        mults,
        ctrl,
    }))
}

pub fn bs3_surface(c: &mut Cursor) -> Result<Option<Bs3Surface>> {
    let kind = c.ident()?;
    let rational = match kind {
        "nullbs" => return Ok(None),
        "nubs" => false,
        "nurbs" => true,
        other => return err(format!("expected a B-spline surface, got {other}")),
    };
    let u_degree = read_degree(c)?;
    let v_degree = read_degree(c)?;
    if rational {
        // Rational in "both", "u" or "v"; weights are stored for every pole.
        if let Some(Token::Ident(_)) = c.peek() {
            c.next_token()?;
        }
    }
    let u_closure = c.enumeration()?;
    let v_closure = c.enumeration()?;
    let _u_singularity = c.enumeration()?;
    let _v_singularity = c.enumeration()?;
    let nku = c.int()?;
    let nkv = c.int()?;
    let (u_knots, u_mults) = knots_of_count(c, nku)?;
    let (v_knots, v_mults) = knots_of_count(c, nkv)?;
    let nu = pole_count(&u_mults, u_degree)?;
    let nv = pole_count(&v_mults, v_degree)?;
    if nu * nv > 10_000_000 {
        return err("too many control points");
    }
    let mut ctrl = Vec::with_capacity(nu * nv);
    for _ in 0..nu * nv {
        let p = [c.double()?, c.double()?, c.double()?];
        let w = if rational { c.double()? } else { 1.0 };
        ctrl.push([p[0], p[1], p[2], w]);
    }
    Ok(Some(Bs3Surface {
        u_degree,
        v_degree,
        rational,
        u_closure,
        v_closure,
        u_knots,
        u_mults,
        v_knots,
        v_mults,
        nu,
        nv,
        ctrl,
    }))
}

/// Analytic or procedural curve.
#[derive(Clone, Debug, PartialEq)]
pub enum AsmCurve {
    /// `root + t * dir`.
    Straight { root: P3, dir: P3 },
    /// `center + major cos t + ratio (normal x major) sin t`.
    Ellipse {
        center: P3,
        normal: P3,
        major: P3,
        ratio: f64,
    },
    /// Procedural curve: the subtype name and its geometry. With
    /// `reversed` the curve runs against its definition: parameter `t`
    /// corresponds to `-t` of the definition.
    Int {
        reversed: bool,
        subtype: String,
        geom: IntGeom,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum IntGeom {
    /// B-spline: exact (`exact_int_cur`) or the stored approximation.
    BSpline(Bs3Curve),
    /// `helix_int_cur`: `center + major cos t + minor sin t + pitch t/(2 pi)`.
    Helix {
        range: [f64; 2],
        center: P3,
        major: P3,
        minor: P3,
        pitch: P3,
        taper: f64,
    },
    /// A curve stored without its approximation: in the summary form (the
    /// approximation's knots and fit tolerance only) or with none (form 2).
    /// It is then its support surface along its parameter curve,
    /// `S(p(t))`, at the parameter curve's parameter (`par_int_cur`,
    /// `blend_int_cur` and the other surface curves, mitcad#589).
    OnSurface {
        chart: Chart,
        pcurve: Bs2Curve,
        /// The approximation's knots (summary form; empty otherwise).
        params: Vec<f64>,
    },
    /// A `blend_int_cur` stored with its approximation on a rolling-ball
    /// blend: the approximation, how far it may be from the curve (its fit
    /// tolerance, model units), and the curve as the blend along its
    /// parameter curve ([`IntGeom::OnSurface`]), for where the
    /// approximation is too far off (mitcad#622).
    Approximated {
        approx: Bs3Curve,
        fit: f64,
        exact: Box<IntGeom>,
    },
    Unsupported(String),
}

/// A support surface in the parameters its parameter curves use (model
/// units).
#[derive(Clone, Debug, PartialEq)]
pub enum Chart {
    /// `root + u u_dir + v v_dir`: `v_dir` is `normal x u_dir`, negated
    /// when the plane's `reverse_v` is set.
    Plane { root: P3, u_dir: P3, v_dir: P3 },
    /// `u` the generator distance normalised by `u_scale`, `v` the angle
    /// from `major` towards `minor`: at the axial distance `d = u dir cos
    /// u_scale` (`dir` -1 when `sin cos < 0`) the base ellipse scaled by
    /// `1 + d sin / (cos |major|)`, moved by `d axis`.
    Cone {
        center: P3,
        axis: P3,
        major: P3,
        minor: P3,
        sin: f64,
        cos: f64,
        u_scale: f64,
    },
    /// A spline surface's B-spline (exact or its stored approximation), in
    /// its own parameters.
    Spline(Box<Bs3Surface>),
    /// A procedural surface stored without its approximation, which the
    /// builder makes (a rolling-ball blend, mitcad#621): curves on it are
    /// passed on as the surface along their parameter curve.
    Procedural(Box<AsmSurface>),
}

impl Chart {
    /// The point at surface parameters `(u, v)` (model units) of an
    /// analytic chart; `None` for a spline, whose B-spline the converter
    /// evaluates.
    pub fn eval(&self, u: f64, v: f64) -> Option<P3> {
        Some(match self {
            Chart::Plane { root, u_dir, v_dir } => [
                root[0] + u * u_dir[0] + v * v_dir[0],
                root[1] + u * u_dir[1] + v * v_dir[1],
                root[2] + u * u_dir[2] + v * v_dir[2],
            ],
            Chart::Cone {
                center,
                axis,
                major,
                minor,
                sin,
                cos,
                u_scale,
            } => {
                let dir = if sin * cos < 0.0 { -1.0 } else { 1.0 };
                let d = u * dir * cos * u_scale;
                let r = (major[0] * major[0] + major[1] * major[1] + major[2] * major[2]).sqrt();
                let grow = if r > 0.0 {
                    1.0 + d * sin / (cos * r)
                } else {
                    1.0
                };
                let (s, c) = v.sin_cos();
                std::array::from_fn(|k| {
                    center[k] + d * axis[k] + grow * (major[k] * c + minor[k] * s)
                })
            }
            Chart::Spline(_) | Chart::Procedural(_) => return None,
        })
    }
}

/// Analytic or procedural surface.
#[derive(Clone, Debug, PartialEq)]
pub enum AsmSurface {
    Plane {
        root: P3,
        normal: P3,
        u_dir: P3,
    },
    /// Base ellipse (center, normal, major axis vector, ratio) and the sine
    /// and cosine of the half angle.
    Cone {
        center: P3,
        normal: P3,
        major: P3,
        ratio: f64,
        sin: f64,
        cos: f64,
    },
    Sphere {
        center: P3,
        radius: f64,
        u_dir: P3,
        pole: P3,
    },
    Torus {
        center: P3,
        normal: P3,
        major: f64,
        minor: f64,
        u_dir: P3,
    },
    /// Procedural surface; with `reversed` its normal is negated. `ranges`
    /// are the u and v parameter ranges of the record (`None` ends are
    /// unbounded, as are both when the record does not give them).
    Spline {
        reversed: bool,
        subtype: String,
        geom: SplineGeom,
        ranges: [[Option<f64>; 2]; 2],
    },
}

/// `helix_spl_line`: a straight line swept along a helix (thread flanks).
/// With `r(v) = major cos v + minor sin v` and `a` the unit axis, the
/// surface is `S(u, v) = center + (1 + taper v / 2pi + line[0] u) r(v) +
/// pitch v / 2pi + line[2] u a`. So `u` is the arc length along the line
/// (whose direction has the radial component `line[0] |major|` and the
/// axial component `line[2]`) and `v` the helix angle. `line[1]` (a
/// tangential component, presumably) is 0 in every file seen.
#[derive(Clone, Debug, PartialEq)]
pub struct HelixLine {
    /// The three intervals at the start of the definition: the line's
    /// range and twice the helix's (as seen; the meaning of the third is
    /// assumed).
    pub ranges: [[Option<f64>; 2]; 3],
    pub center: P3,
    pub major: P3,
    pub minor: P3,
    /// Advance per turn.
    pub pitch: P3,
    pub taper: f64,
    pub axis: P3,
    pub line: P3,
}

/// `helix_spl_circ`: a circular arc swept along a helix (the rounded root
/// or crest of a thread or coil). With `r(v) = major cos v + minor sin v`
/// (radius `R`), the radial unit `x(v) = r(v) / R`, `a` the unit axis and
/// `w = u + phase`, the surface is `S(u, v) = center + (1 + taper v / 2pi)
/// r(v) + pitch v / 2pi + radius (-cos w x(v) + sin w a)`: the arc lies in
/// the plane of the axis, `u` is its angle and `v` the helix angle
/// *(verified on the faces of two such surfaces of different phases: their
/// edges are the helices of the arc's ends)*.
#[derive(Clone, Debug, PartialEq)]
pub struct HelixCircle {
    /// The arc's range and the helix's (the definition's first two
    /// intervals; a third repeats the helix's).
    pub ranges: [[Option<f64>; 2]; 2],
    pub phase: f64,
    pub center: P3,
    pub major: P3,
    pub minor: P3,
    /// Advance per turn.
    pub pitch: P3,
    pub taper: f64,
    pub axis: P3,
    pub radius: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SplineGeom {
    /// B-spline: exact (`exact_spl_sur`) or the stored approximation.
    BSpline {
        surface: Box<Bs3Surface>,
        exact: bool,
    },
    /// `cyl_spl_sur`: profile curve swept along `dir`: `C(u) + v dir`.
    Extrusion {
        curve: Box<AsmCurve>,
        dir: P3,
    },
    /// `rot_spl_sur`: profile curve revolved about an axis.
    Revolution {
        curve: Box<AsmCurve>,
        root: P3,
        axis: P3,
        approx: Option<Box<Bs3Surface>>,
    },
    /// `helix_spl_line`: a line swept along a helix.
    HelixLine(Box<HelixLine>),
    /// `helix_spl_circ`: a circular arc swept along a helix.
    HelixCircle(Box<HelixCircle>),
    /// `rb_blend_spl_sur` (revision-gated layout): the rolling ball's
    /// definition (mitcad#621), with its stored approximation if any
    /// (mitcad#622).
    RollingBall(Box<RollingBall>),
    /// `sum_spl_sur` stored without its approximation: `S(u, v) = a(u) +
    /// b(v) - origin` (mitcad#621).
    Sum {
        curves: Box<[AsmCurve; 2]>,
        origin: P3,
    },
    /// `off_spl_sur` (revision-gated layout) stored without its
    /// approximation (mitcad_private#198): the support moved by `distance`
    /// (model units) along the `du x dv` of its stored parameters, in the
    /// support's parameters. `against`: that `du x dv` runs against the
    /// normal of a plane support (its `v` reversed); a spline support's is
    /// its B-spline's `du x dv`.
    Offset {
        support: Box<AsmSurface>,
        distance: f64,
        against: bool,
    },
    Unsupported(String),
}

/// What a rolling ball rolls on, on one side.
#[derive(Clone, Debug, PartialEq)]
pub enum BlendSupport {
    /// `blend_support_surface`: the ball touches the surface.
    Surface(AsmSurface),
    /// `blend_support_curve`: the ball touches the curve (an edge).
    Curve(AsmCurve),
}

/// One side of a rolling-ball blend: its support and, when stored, the
/// contact track as a parameter curve in the support surface's chart.
#[derive(Clone, Debug, PartialEq)]
pub struct BlendSide {
    pub support: BlendSupport,
    pub contact: Option<Bs2Curve>,
}

/// `rb_blend_spl_sur` (revision-gated layout): a ball of radius `|offsets[0]|` whose centre runs along the `spine` (the
/// slice curve) touching both supports. The surface's `u` runs across the
/// section from the first side's contact (0) to the second's (1), its `v`
/// is the spine's parameter.
#[derive(Clone, Debug, PartialEq)]
pub struct RollingBall {
    pub sides: [BlendSide; 2],
    pub spine: AsmCurve,
    /// The two signed offsets (the radius on each side).
    pub offsets: [f64; 2],
    /// The spine parameters the surface spans: the v knots of the stored
    /// approximation or of its summary, or the v interval of a record
    /// stored with neither (tail form 2).
    pub v_range: [f64; 2],
    /// The stored approximation (tail form 0), if any, and how far it may
    /// be from the surface (its fit tolerance, model units; mitcad#622).
    pub approx: Option<Box<Bs3Surface>>,
    pub fit: f64,
}

/// Upper bound for nested `ref` and embedded definitions.
const MAX_DEPTH: usize = 32;

/// Position of the `{` that defines the subtype at `pos` (follows `ref`).
fn resolve_subtype(file: &AsmFile, pos: usize, depth: usize) -> Result<usize> {
    if depth > MAX_DEPTH {
        return err("subtype references nest too deeply");
    }
    if file.tokens.get(pos) != Some(&Token::SubStart) {
        return err("expected '{'");
    }
    match file.tokens.get(pos + 1) {
        Some(Token::Ident(name)) if name == "ref" => {
            let n = match file.tokens.get(pos + 2) {
                Some(Token::Int(n)) => *n,
                _ => return err("ref without an index"),
            };
            let target = usize::try_from(n)
                .ok()
                .and_then(|i| file.subtypes.get(i).copied())
                .ok_or_else(|| GeomError(format!("ref {n} outside the subtype table")))?;
            if target >= pos {
                return err(format!("ref {n} points forward"));
            }
            resolve_subtype(file, target, depth + 1)
        }
        Some(Token::Ident(_)) => Ok(pos),
        _ => err("subtype without a name"),
    }
}

/// Finds the first B-spline surface written directly in the subtype at
/// `def` (not inside nested subtypes).
fn find_bs3_surface(file: &AsmFile, def: usize) -> Result<Option<Bs3Surface>> {
    let end = Cursor::new(&file.tokens, def).subtype_end()?;
    let toks = &file.tokens;
    let mut depth = 0usize;
    let mut i = def + 2;
    while i + 2 < end {
        match &toks[i] {
            Token::SubStart => depth += 1,
            Token::SubEnd => depth = depth.saturating_sub(1),
            // A surface starts `nubs <u degree> <v degree>`; a curve has an
            // enum after its degree.
            Token::Ident(name)
                if depth == 0
                    && (name == "nubs" || name == "nurbs")
                    && matches!(toks[i + 1], Token::Int(_))
                    && matches!(toks[i + 2], Token::Int(_)) =>
            {
                let mut c = Cursor::new(toks, i);
                return bs3_surface(&mut c);
            }
            _ => {}
        }
        i += 1;
    }
    Ok(None)
}

/// Skips the version integer and save-level enum that start most subtypes.
fn skip_version(c: &mut Cursor) {
    if let Some(Token::Int(_)) = c.peek() {
        c.pos += 1;
    }
    if let Some(Token::Enum(_)) = c.peek() {
        c.pos += 1;
    }
}

/// Decodes the int_cur subtype starting at `pos` (`{` or `{ ref N }`).
pub fn int_curve_subtype(file: &AsmFile, pos: usize, depth: usize) -> Result<(String, IntGeom)> {
    let def = resolve_subtype(file, pos, depth)?;
    let mut c = Cursor::new(&file.tokens, def + 1);
    let name = c.ident()?.to_string();
    let geom = match name.as_str() {
        "helix_int_cur" => {
            if let Some(Token::Int(_)) = c.peek() {
                c.pos += 1;
            }
            let range = c.interval()?;
            let center = c.point()?;
            let major = c.point()?;
            let minor = c.point()?;
            let pitch = c.point()?;
            let taper = c.double()?;
            match range {
                [Some(a), Some(b)] => IntGeom::Helix {
                    range: [a, b],
                    center,
                    major,
                    minor,
                    pitch,
                    taper,
                },
                _ => IntGeom::Unsupported("unbounded helix".into()),
            }
        }
        _ => {
            // An optional serializer revision, then the approximation's
            // form: 0 the B-spline and its fit tolerance, 1 the summary
            // (the B-spline's knots and fit tolerance, and its closure),
            // 2 none (the parameter interval and the closure).
            if let Some(Token::Int(_)) = c.peek() {
                c.pos += 1;
            }
            let form = match c.peek() {
                Some(Token::Enum(e)) => {
                    c.pos += 1;
                    Some(*e)
                }
                _ => None,
            };
            match (form, c.peek()) {
                (_, Some(Token::Ident(k))) if k == "nubs" || k == "nurbs" => {
                    match bs3_curve(&mut c)? {
                        Some(b) if name == "blend_int_cur" || name == "bldcur" => {
                            blend_curve(file, &mut c, depth, &name, b)
                        }
                        Some(b) => IntGeom::BSpline(b),
                        None => IntGeom::Unsupported(format!("{name} without a B-spline")),
                    }
                }
                (_, Some(Token::Ident(k))) if k == "nullbs" => {
                    IntGeom::Unsupported(format!("{name} without a B-spline"))
                }
                (Some(1), _) => {
                    let n = usize::try_from(c.int()?)
                        .ok()
                        .filter(|&n| n <= 1_000_000)
                        .ok_or_else(|| GeomError(format!("{name}: bad knot count")))?;
                    let params = (0..n).map(|_| c.double()).collect::<Result<Vec<f64>>>()?;
                    c.double()?; // fit tolerance
                    c.enumeration()?; // closure
                    curve_on_surface(file, &mut c, depth, &name, params)?
                }
                (Some(2), _) => {
                    c.interval()?;
                    c.enumeration()?; // closure
                    curve_on_surface(file, &mut c, depth, &name, Vec::new())?
                }
                _ => IntGeom::Unsupported(format!("{name}: unknown layout")),
            }
        }
    };
    Ok((name, geom))
}

/// The support surfaces and parameter curves of a surface curve stored
/// without its approximation: the curve is the first support along its
/// parameter curve (a `par_int_cur` has one of them, in either slot).
fn curve_on_surface(
    file: &AsmFile,
    c: &mut Cursor,
    depth: usize,
    name: &str,
    params: Vec<f64>,
) -> Result<IntGeom> {
    let s1 = embedded_surface(file, c, depth + 1)?;
    let s2 = embedded_surface(file, c, depth + 1)?;
    let p1 = bs2_curve(c)?;
    let p2 = bs2_curve(c)?;
    let pair = match (s1, p1, s2, p2) {
        (Some(s), Some(p), _, _) | (_, _, Some(s), Some(p)) => Some((s, p)),
        _ => None,
    };
    Ok(match pair {
        Some((Ok(chart), pcurve)) => IntGeom::OnSurface {
            chart,
            pcurve,
            params,
        },
        Some((Err(why), _)) => IntGeom::Unsupported(format!("{name} on {why}")),
        None => IntGeom::Unsupported(format!(
            "{name} without an approximation, a support surface and its parameter curve"
        )),
    })
}

/// A `blend_int_cur` stored with its approximation `approx` (the cursor
/// after it): with its fit tolerance and, where its support is a
/// rolling-ball blend and its parameter curve on it is stored, the curve as
/// that blend along the parameter curve (a contact curve of the ball, an
/// iso-parameter line of the blend; mitcad#622). Otherwise, or where the
/// rest does not read, the approximation alone.
fn blend_curve(
    file: &AsmFile,
    c: &mut Cursor,
    depth: usize,
    name: &str,
    approx: Bs3Curve,
) -> IntGeom {
    let fit = match c.double() {
        Ok(fit) => fit,
        Err(_) => return IntGeom::BSpline(approx),
    };
    match curve_on_surface(file, c, depth, name, Vec::new()) {
        Ok(
            exact @ IntGeom::OnSurface {
                chart: Chart::Procedural(_),
                ..
            },
        ) => IntGeom::Approximated {
            approx,
            fit,
            exact: Box::new(exact),
        },
        _ => IntGeom::BSpline(approx),
    }
}

fn unit3(v: P3) -> P3 {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if n > 0.0 {
        [v[0] / n, v[1] / n, v[2] / n]
    } else {
        v
    }
}

fn cross3(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Reads a support surface written inside a curve's subtype: `None` for
/// `null_surface`, else its chart, or why it has none the reader can
/// evaluate. Analytic surfaces end with their `reverse_v` flag and their
/// u and v ranges, splines with their ranges.
fn embedded_surface(
    file: &AsmFile,
    c: &mut Cursor,
    depth: usize,
) -> Result<Option<std::result::Result<Chart, String>>> {
    Ok(embedded_surface_with_chart(file, c, depth)?.map(|(_, chart)| chart))
}

/// [`embedded_surface`] with the surface itself (as a surface record
/// decodes) beside its chart.
fn embedded_surface_with_chart(
    file: &AsmFile,
    c: &mut Cursor,
    depth: usize,
) -> Result<Option<(AsmSurface, std::result::Result<Chart, String>)>> {
    if depth > MAX_DEPTH {
        return err("embedded geometry nests too deeply");
    }
    let kind = c.ident()?;
    let (surface, chart) = match kind {
        "null_surface" => return Ok(None),
        "plane" => {
            let root = c.point()?;
            let normal = c.point()?;
            let u_dir = c.point()?;
            let reverse_v = c.boolean()?;
            let v = cross3(unit3(normal), u_dir);
            (
                AsmSurface::Plane {
                    root,
                    normal,
                    u_dir,
                },
                Ok(Chart::Plane {
                    root,
                    u_dir,
                    v_dir: if reverse_v { v.map(|x| -x) } else { v },
                }),
            )
        }
        "cone" => {
            let center = c.point()?;
            let normal = c.point()?;
            let major = c.point()?;
            let ratio = c.double()?;
            c.interval()?;
            let sin = c.double()?;
            let cos = c.double()?;
            let u_scale = c.double()?;
            c.boolean()?;
            let axis = unit3(normal);
            (
                AsmSurface::Cone {
                    center,
                    normal,
                    major,
                    ratio,
                    sin,
                    cos,
                },
                Ok(Chart::Cone {
                    center,
                    axis,
                    major,
                    minor: cross3(axis, major).map(|x| x * ratio),
                    sin,
                    cos,
                    u_scale,
                }),
            )
        }
        "sphere" => {
            let center = c.point()?;
            let radius = c.double()?;
            let u_dir = c.point()?;
            let pole = c.point()?;
            c.boolean()?;
            (
                AsmSurface::Sphere {
                    center,
                    radius,
                    u_dir,
                    pole,
                },
                Err("a sphere".to_string()),
            )
        }
        "torus" => {
            let center = c.point()?;
            let normal = c.point()?;
            let major = c.double()?;
            let minor = c.double()?;
            let u_dir = c.point()?;
            c.boolean()?;
            (
                AsmSurface::Torus {
                    center,
                    normal,
                    major,
                    minor,
                    u_dir,
                },
                Err("a torus".to_string()),
            )
        }
        "spline" => {
            let reversed = c.boolean()?;
            let (subtype, geom) = spline_surface_subtype(file, c.pos, depth + 1)?;
            c.skip_subtype()?;
            let chart = match &geom {
                SplineGeom::BSpline { surface, .. }
                | SplineGeom::Revolution {
                    approx: Some(surface),
                    ..
                } => Ok(Chart::Spline(surface.clone())),
                _ => Err(format!("a {subtype} without a B-spline")),
            };
            let ranges = [c.interval()?, c.interval()?];
            let surface = AsmSurface::Spline {
                reversed,
                subtype,
                geom,
                ranges,
            };
            let chart = match (chart, &surface) {
                (
                    Err(_),
                    AsmSurface::Spline {
                        geom: SplineGeom::RollingBall(_),
                        ..
                    },
                ) => Ok(Chart::Procedural(Box::new(surface.clone()))),
                // An offset of a spline, in the spline's parameters
                // (mitcad_private#198).
                (
                    Err(_),
                    AsmSurface::Spline {
                        geom: SplineGeom::Offset { support, .. },
                        ..
                    },
                ) if matches!(**support, AsmSurface::Spline { .. }) => {
                    Ok(Chart::Procedural(Box::new(surface.clone())))
                }
                (chart, _) => chart,
            };
            return Ok(Some((surface, chart)));
        }
        other => return err(format!("unknown embedded surface {other}")),
    };
    c.interval()?;
    c.interval()?;
    Ok(Some((surface, chart)))
}

/// Reads a procedural surface's definition (revision-gated layout, after
/// its revision integer): `None` when the record stores its approximation,
/// which is then the surface.
type ProceduralReader = fn(&AsmFile, &mut Cursor<'_>, usize) -> Result<Option<SplineGeom>>;

/// The procedural spline-surface families whose definitions the reader
/// evaluates where a record stores no approximation (mitcad#621,
/// mitcad_private#198), by subtype name (with the legacy name). The order
/// does not matter; a family read is one row more.
const PROCEDURAL_SURFACES: &[(&str, &str, ProceduralReader)] = &[
    ("rb_blend_spl_sur", "rbblnsur", rolling_ball_surface),
    ("sum_spl_sur", "sumsur", sum_surface),
    ("off_spl_sur", "offsur", offset_surface),
];

/// [`rolling_ball`] as a [`ProceduralReader`].
fn rolling_ball_surface(
    file: &AsmFile,
    c: &mut Cursor,
    depth: usize,
) -> Result<Option<SplineGeom>> {
    rolling_ball(file, c, depth).map(|rb| rb.map(|rb| SplineGeom::RollingBall(Box::new(rb))))
}

/// The reader of a procedural surface family's definition by its subtype
/// name ([`PROCEDURAL_SURFACES`]).
fn procedural_surface(name: &str) -> Option<ProceduralReader> {
    PROCEDURAL_SURFACES
        .iter()
        .find(|(modern, legacy, _)| name == *modern || name == *legacy)
        .map(|&(_, _, read)| read)
}

/// Reads a revision-gated `off_spl_sur` after its revision integer
/// (mitcad_private#198, the layout of the published format notes): the
/// support surface with its bounds, the offset distance, the pair of
/// booleans in the sense slots (the first negates the direction of the
/// offset), the two booleans of the extension prefix (the second gates an
/// extension run) and the shared surface tail. `None` when it stores its
/// approximation (tail form 0). Supports: planes and splines with a
/// B-spline, whose parameters' `du x dv` is known.
fn offset_surface(file: &AsmFile, c: &mut Cursor, depth: usize) -> Result<Option<SplineGeom>> {
    let (support, chart) = embedded_surface_with_chart(file, c, depth + 1)?
        .ok_or_else(|| GeomError("off_spl_sur without its support".into()))?;
    let distance = c.double()?;
    let negated = c.boolean()?;
    c.boolean()?;
    c.boolean()?;
    if c.boolean()? {
        // The extension run: a boolean, six integers, a boolean, a curve
        // with its two parameter ends, two booleans, a tolerance and four
        // integers.
        c.boolean()?;
        for _ in 0..6 {
            c.int()?;
        }
        c.boolean()?;
        embedded_curve(file, c, depth + 1)?;
        c.boolean()?;
        c.boolean()?;
        c.double()?;
        for _ in 0..4 {
            c.int()?;
        }
    }
    if c.enumeration()? == 0 {
        return Ok(None);
    }
    let against = match (&chart, &support) {
        (Ok(Chart::Plane { u_dir, v_dir, .. }), AsmSurface::Plane { normal, .. }) => {
            let n = cross3(*u_dir, *v_dir);
            n[0] * normal[0] + n[1] * normal[1] + n[2] * normal[2] < 0.0
        }
        (Ok(Chart::Spline(_)), _) => false,
        _ => return err("off_spl_sur of a support whose parameters' normal is not known"),
    };
    Ok(Some(SplineGeom::Offset {
        support: Box::new(support),
        distance: if negated { -distance } else { distance },
        against,
    }))
}

/// Reads a revision-gated `sum_spl_sur` after its revision integer: the two
/// curves (each with its parameter interval), the origin and the shared
/// surface tail. `None` when it stores its approximation (tail form 0).
fn sum_surface(file: &AsmFile, c: &mut Cursor, depth: usize) -> Result<Option<SplineGeom>> {
    let a = embedded_curve(file, c, depth + 1)?;
    let b = embedded_curve(file, c, depth + 1)?;
    let origin = c.point()?;
    match (c.enumeration()?, a, b) {
        (0, ..) => Ok(None),
        (_, Some(a), Some(b)) => Ok(Some(SplineGeom::Sum {
            curves: Box::new([a, b]),
            origin,
        })),
        _ => err("sum_spl_sur without its curves"),
    }
}

/// Reads a revision-gated `rb_blend_spl_sur` after its revision integer
/// (the layout of the published format notes): two sides, the slice
/// (spine) curve, the two offsets, the radius selector, the u and v
/// ranges, the shape fields and the shared surface tail: its stored
/// approximation and fit tolerance (tail form 0), the summary of one (1)
/// or neither (2). `None` when tail form 0 holds no approximation.
fn rolling_ball(file: &AsmFile, c: &mut Cursor, depth: usize) -> Result<Option<RollingBall>> {
    let side = |c: &mut Cursor| -> Result<BlendSide> {
        let kind = c.string()?.to_string();
        let surface = embedded_surface_with_chart(file, c, depth + 1)?;
        let curve = embedded_curve(file, c, depth + 1)?;
        let contact = bs2_curve(c)?;
        c.point()?; // location
        bs2_curve(c)?;
        if let Some(Token::Int(_)) = c.peek() {
            c.int()?;
            bs2_curve(c)?;
        }
        let support = match (kind.as_str(), surface, curve) {
            ("blend_support_surface", Some((s, _)), _) => BlendSupport::Surface(s),
            ("blend_support_curve", _, Some(curve)) => BlendSupport::Curve(curve),
            (kind, ..) => return err(format!("a rolling-ball side of {kind} without its support")),
        };
        Ok(BlendSide { support, contact })
    };
    let first = side(c)?;
    let second = side(c)?;
    let spine = embedded_curve(file, c, depth + 1)?
        .ok_or_else(|| GeomError("a rolling-ball blend without its slice curve".into()))?;
    let offsets = [c.double()?, c.double()?];
    // The radius selector: enum -1 (none) or a double.
    c.next_token()?;
    c.interval()?; // u range
    c.interval()?; // v range
    c.int()?;
    c.double()?;
    c.double()?;
    c.int()?;
    let mut approx = None;
    let mut fit = 0.0;
    let v_range = match c.enumeration()? {
        // The approximation and its fit tolerance (mitcad#622: it may be
        // a tenth of a millimetre off the surface).
        0 => {
            let Some(s) = bs3_surface(c)? else {
                return Ok(None);
            };
            fit = c.double()?;
            let range = [s.v_knots[0], s.v_knots[s.v_knots.len() - 1]];
            approx = Some(Box::new(s));
            range
        }
        // The summary: the approximation's u and v knots, its fit
        // tolerance, closures and singularities.
        1 => {
            let knots = |c: &mut Cursor| -> Result<Vec<f64>> {
                let n = usize::try_from(c.int()?)
                    .ok()
                    .filter(|&n| (2..=1_000_000).contains(&n))
                    .ok_or_else(|| GeomError("rb_blend_spl_sur: bad knot count".into()))?;
                (0..n).map(|_| c.double()).collect()
            };
            knots(c)?;
            let v = knots(c)?;
            [v[0], v[v.len() - 1]]
        }
        2 => {
            c.interval()?;
            match c.interval()? {
                [Some(a), Some(b)] => [a, b],
                _ => return err("rb_blend_spl_sur: an unbounded v interval"),
            }
        }
        form => return err(format!("rb_blend_spl_sur: unknown tail form {form}")),
    };
    Ok(Some(RollingBall {
        sides: [first, second],
        spine,
        offsets,
        v_range,
        approx,
        fit,
    }))
}

/// Reads an embedded curve (`intcurve`, `straight`, `ellipse`, `null_curve`)
/// with its parameter interval.
pub fn embedded_curve(file: &AsmFile, c: &mut Cursor, depth: usize) -> Result<Option<AsmCurve>> {
    if depth > MAX_DEPTH {
        return err("embedded geometry nests too deeply");
    }
    let kind = c.ident()?;
    let curve = match kind {
        "null_curve" => return Ok(None),
        "intcurve" => {
            let reversed = c.boolean()?;
            let (subtype, geom) = int_curve_subtype(file, c.pos, depth + 1)?;
            c.skip_subtype()?;
            c.interval()?;
            AsmCurve::Int {
                reversed,
                subtype,
                geom,
            }
        }
        "straight" => {
            let root = c.point()?;
            let dir = c.point()?;
            c.interval()?;
            AsmCurve::Straight { root, dir }
        }
        "ellipse" => {
            let center = c.point()?;
            let normal = c.point()?;
            let major = c.point()?;
            let ratio = c.double()?;
            c.interval()?;
            AsmCurve::Ellipse {
                center,
                normal,
                major,
                ratio,
            }
        }
        other => return err(format!("unknown embedded curve {other}")),
    };
    Ok(Some(curve))
}

/// Decodes the spl_sur subtype at `pos`.
pub fn spline_surface_subtype(
    file: &AsmFile,
    pos: usize,
    depth: usize,
) -> Result<(String, SplineGeom)> {
    let def = resolve_subtype(file, pos, depth)?;
    let mut c = Cursor::new(&file.tokens, def + 1);
    let name = c.ident()?.to_string();
    let geom = match name.as_str() {
        "exact_spl_sur" => {
            skip_version(&mut c);
            match bs3_surface(&mut c)? {
                Some(s) => SplineGeom::BSpline {
                    surface: Box::new(s),
                    exact: true,
                },
                None => SplineGeom::Unsupported("exact_spl_sur without a B-spline".into()),
            }
        }
        "cyl_spl_sur" => {
            if let Some(Token::Int(_)) = c.peek() {
                c.pos += 1;
            }
            match embedded_curve(file, &mut c, depth + 1)? {
                Some(curve) => {
                    let dir = c.point()?;
                    SplineGeom::Extrusion {
                        curve: Box::new(curve),
                        dir,
                    }
                }
                None => SplineGeom::Unsupported("cyl_spl_sur without a profile".into()),
            }
        }
        "rot_spl_sur" => {
            if let Some(Token::Int(_)) = c.peek() {
                c.pos += 1;
            }
            match embedded_curve(file, &mut c, depth + 1)? {
                Some(curve) => {
                    let root = c.point()?;
                    let axis = c.point()?;
                    let approx = find_bs3_surface(file, def)?.map(Box::new);
                    SplineGeom::Revolution {
                        curve: Box::new(curve),
                        root,
                        axis,
                        approx,
                    }
                }
                None => SplineGeom::Unsupported("rot_spl_sur without a profile".into()),
            }
        }
        "helix_spl_line" => {
            // version, three intervals, the helix as in helix_int_cur
            // (center, major, minor, pitch, taper, axis), two null
            // surfaces and two null B-splines, then the line vector.
            if let Some(Token::Int(_)) = c.peek() {
                c.pos += 1;
            }
            let ranges = [c.interval()?, c.interval()?, c.interval()?];
            let center = c.point()?;
            let major = c.point()?;
            let minor = c.point()?;
            let pitch = c.point()?;
            let taper = c.double()?;
            let axis = c.point()?;
            let mut nulls = 0;
            while let Some(Token::Ident(k)) = c.peek() {
                if k != "null_surface" && k != "nullbs" {
                    break;
                }
                c.pos += 1;
                nulls += 1;
            }
            match c.peek() {
                Some(Token::Vector(_) | Token::Pos(_)) if nulls == 4 => {
                    SplineGeom::HelixLine(Box::new(HelixLine {
                        ranges,
                        center,
                        major,
                        minor,
                        pitch,
                        taper,
                        axis,
                        line: c.point()?,
                    }))
                }
                _ => SplineGeom::Unsupported("helix_spl_line: unknown layout".into()),
            }
        }
        "helix_spl_circ" => {
            // version, the arc's and the helix's intervals, the phase, the
            // helix's interval again, the helix (center, major, minor,
            // pitch, taper, axis), two null surfaces and two null
            // B-splines, then the arc's radius.
            if let Some(Token::Int(_)) = c.peek() {
                c.pos += 1;
            }
            let ranges = [c.interval()?, c.interval()?];
            let phase = c.double()?;
            c.interval()?;
            let center = c.point()?;
            let major = c.point()?;
            let minor = c.point()?;
            let pitch = c.point()?;
            let taper = c.double()?;
            let axis = c.point()?;
            let mut nulls = 0;
            while let Some(Token::Ident(k)) = c.peek() {
                if k != "null_surface" && k != "nullbs" {
                    break;
                }
                c.pos += 1;
                nulls += 1;
            }
            match c.peek() {
                Some(Token::Double(_) | Token::Int(_)) if nulls == 4 => {
                    SplineGeom::HelixCircle(Box::new(HelixCircle {
                        ranges,
                        phase,
                        center,
                        major,
                        minor,
                        pitch,
                        taper,
                        axis,
                        radius: c.double()?,
                    }))
                }
                _ => SplineGeom::Unsupported("helix_spl_circ: unknown layout".into()),
            }
        }
        _ => {
            // A rolling ball (revision-gated layout): its definition
            // (mitcad#621), with its approximation where stored
            // (mitcad#622); a sum surface stored without its
            // approximation.
            let mut definition = Ok(None);
            if let Some(Token::Int(_)) = c.peek()
                && let Some(read) = procedural_surface(&name)
            {
                c.pos += 1;
                definition = read(file, &mut c, depth);
            }
            match (definition, find_bs3_surface(file, def)?) {
                (Ok(Some(geom)), _) => geom,
                (_, Some(s)) => SplineGeom::BSpline {
                    surface: Box::new(s),
                    exact: false,
                },
                (Err(e), None) => {
                    SplineGeom::Unsupported(format!("{name} without a stored approximation: {e}"))
                }
                (Ok(None), None) => {
                    SplineGeom::Unsupported(format!("{name} without a stored approximation"))
                }
            }
        }
    };
    Ok((name, geom))
}

/// Decodes a curve record (`straight-curve`, `ellipse-curve`,
/// `intcurve-curve`); the cursor is after the three header fields.
pub fn curve_record(file: &AsmFile, record: usize) -> Result<AsmCurve> {
    let r = &file.records[record];
    let mut c = Cursor::new(&file.tokens, r.fields.start);
    c.ptr()?; // attribute
    c.int()?; // history id
    c.ptr()?;
    match r.sub_type() {
        "straight" => Ok(AsmCurve::Straight {
            root: c.point()?,
            dir: c.point()?,
        }),
        "ellipse" => Ok(AsmCurve::Ellipse {
            center: c.point()?,
            normal: c.point()?,
            major: c.point()?,
            ratio: c.double()?,
        }),
        "intcurve" => {
            let reversed = c.boolean()?;
            let (subtype, geom) = int_curve_subtype(file, c.pos, 0)?;
            Ok(AsmCurve::Int {
                reversed,
                subtype,
                geom,
            })
        }
        other => err(format!("unsupported curve type {other}")),
    }
}

/// Decodes a surface record (`plane-surface`, `cone-surface`, ...).
pub fn surface_record(file: &AsmFile, record: usize) -> Result<AsmSurface> {
    let r = &file.records[record];
    let mut c = Cursor::new(&file.tokens, r.fields.start);
    c.ptr()?;
    c.int()?;
    c.ptr()?;
    match r.sub_type() {
        "plane" => Ok(AsmSurface::Plane {
            root: c.point()?,
            normal: c.point()?,
            u_dir: c.point()?,
        }),
        "cone" => {
            let center = c.point()?;
            let normal = c.point()?;
            let major = c.point()?;
            let ratio = c.double()?;
            c.interval()?; // base ellipse range
            let sin = c.double()?;
            let cos = c.double()?;
            Ok(AsmSurface::Cone {
                center,
                normal,
                major,
                ratio,
                sin,
                cos,
            })
        }
        "sphere" => Ok(AsmSurface::Sphere {
            center: c.point()?,
            radius: c.double()?,
            u_dir: c.point()?,
            pole: c.point()?,
        }),
        "torus" => Ok(AsmSurface::Torus {
            center: c.point()?,
            normal: c.point()?,
            major: c.double()?,
            minor: c.double()?,
            u_dir: c.point()?,
        }),
        "spline" => {
            let reversed = c.boolean()?;
            let (subtype, geom) = spline_surface_subtype(file, c.pos, 0)?;
            c.skip_subtype()?;
            let ranges = match (c.interval(), c.interval()) {
                (Ok(u), Ok(v)) => [u, v],
                _ => [[None; 2]; 2],
            };
            Ok(AsmSurface::Spline {
                reversed,
                subtype,
                geom,
                ranges,
            })
        }
        other => err(format!("unsupported surface type {other}")),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::asm::writer::Writer;

    fn file_with(build: impl FnOnce(&mut Writer)) -> AsmFile {
        let mut w = Writer::new(1, 2);
        w.record("asmheader")
            .ptr(-1)
            .int(-1)
            .str("231.6.3.65535")
            .end();
        build(&mut w);
        AsmFile::parse(&w.finish()).unwrap()
    }

    #[test]
    fn decodes_exact_int_cur_and_refs() {
        let f = file_with(|w| {
            w.record("intcurve-curve")
                .head()
                .bool(true)
                .sub_start()
                .ident("exact_int_cur")
                .int(23100)
                .en(0);
            // nubs, degree 1, open, knots 0 (1), 2 (1): two poles.
            w.ident("nubs")
                .int(1)
                .en(0)
                .int(2)
                .dbl(0.0)
                .int(1)
                .dbl(2.0)
                .int(1);
            w.dbl(0.0).dbl(0.0).dbl(0.0).dbl(2.0).dbl(0.0).dbl(0.0);
            w.dbl(0.0)
                .ident("null_surface")
                .ident("null_surface")
                .ident("nullbs")
                .ident("nullbs");
            w.sub_end().bool(false).bool(false).end();
            w.record("intcurve-curve")
                .head()
                .bool(false)
                .sub_start()
                .ident("ref")
                .int(0)
                .sub_end();
            w.bool(false).bool(false).end();
        });
        let c = curve_record(&f, 1).unwrap();
        let AsmCurve::Int {
            reversed,
            subtype,
            geom: IntGeom::BSpline(b),
        } = c
        else {
            panic!("{c:?}")
        };
        assert!(reversed);
        assert_eq!(subtype, "exact_int_cur");
        assert_eq!(b.degree, 1);
        assert_eq!(b.ctrl.len(), 2);
        assert_eq!(b.ctrl[1], [2.0, 0.0, 0.0, 1.0]);
        let c2 = curve_record(&f, 2).unwrap();
        assert!(matches!(
            c2,
            AsmCurve::Int {
                reversed: false,
                geom: IntGeom::BSpline(_),
                ..
            }
        ));
    }

    #[test]
    fn decodes_analytic_surfaces() {
        let f = file_with(|w| {
            w.record("cone-surface")
                .head()
                .pos([0.0; 3])
                .vec([0.0, 0.0, 1.0])
                .vec([2.0, 0.0, 0.0])
                .dbl(1.0);
            w.bool(false).bool(false).dbl(0.0).dbl(1.0).dbl(2.0);
            w.bool(false)
                .bool(false)
                .bool(false)
                .bool(false)
                .bool(false)
                .end();
            w.record("torus-surface")
                .head()
                .pos([0.0; 3])
                .vec([0.0, 0.0, 1.0])
                .dbl(5.0)
                .dbl(-1.0);
            w.vec([1.0, 0.0, 0.0])
                .bool(false)
                .bool(false)
                .bool(false)
                .bool(false)
                .bool(false)
                .end();
        });
        assert_eq!(
            surface_record(&f, 1).unwrap(),
            AsmSurface::Cone {
                center: [0.0; 3],
                normal: [0.0, 0.0, 1.0],
                major: [2.0, 0.0, 0.0],
                ratio: 1.0,
                sin: 0.0,
                cos: 1.0
            }
        );
        assert!(
            matches!(surface_record(&f, 2).unwrap(), AsmSurface::Torus { minor, .. } if minor == -1.0)
        );
    }

    #[test]
    fn decodes_helix_spl_line_with_the_face_ranges() {
        // Layout as in .f3d files (values of an M3 thread flank).
        let f = file_with(|w| {
            w.record("spline-surface")
                .head()
                .bool(false)
                .sub_start()
                .ident("helix_spl_line")
                .int(22601);
            for hi in [0.032, 50.0, 50.0] {
                w.bool(true).dbl(0.0).bool(true).dbl(hi);
            }
            w.pos([0.0, 0.0, 1.0])
                .vec([0.12645, 0.0, 0.0])
                .vec([0.0, 0.12645, 0.0])
                .vec([0.0, 0.0, 0.05])
                .dbl(0.0)
                .vec([0.0, 0.0, 1.0]);
            w.ident("null_surface")
                .ident("null_surface")
                .ident("nullbs")
                .ident("nullbs")
                .vec([6.8488, 0.0, -0.5])
                .sub_end();
            w.bool(true).dbl(0.0).bool(true).dbl(0.032);
            w.bool(true).dbl(13.3).bool(true).dbl(39.1);
            w.bool(false).bool(false).bool(false).end();
        });
        let s = surface_record(&f, 1).unwrap();
        let AsmSurface::Spline {
            reversed,
            subtype,
            geom: SplineGeom::HelixLine(h),
            ranges,
        } = s
        else {
            panic!("{s:?}")
        };
        assert!(!reversed);
        assert_eq!(subtype, "helix_spl_line");
        assert_eq!(ranges, [[Some(0.0), Some(0.032)], [Some(13.3), Some(39.1)]]);
        assert_eq!(h.ranges[1], [Some(0.0), Some(50.0)]);
        assert_eq!(h.center, [0.0, 0.0, 1.0]);
        assert_eq!(h.pitch, [0.0, 0.0, 0.05]);
        assert_eq!(h.line, [6.8488, 0.0, -0.5]);
    }

    #[test]
    fn decodes_helix_spl_circ() {
        // Layout as in a part file (a thread root of radius 0.0875 mm).
        let f = file_with(|w| {
            w.record("spline-surface")
                .head()
                .bool(false)
                .sub_start()
                .ident("helix_spl_circ")
                .int(22502);
            w.bool(true).dbl(2.0944).bool(true).dbl(3.125);
            w.bool(true).dbl(0.0).bool(true).dbl(50.2);
            w.dbl(3.125);
            w.bool(true).dbl(0.0).bool(true).dbl(50.2);
            w.pos([0.0, 0.0413, 0.0])
                .vec([0.0, 0.0, -0.17086])
                .vec([-0.17086, 0.0, 0.0])
                .vec([0.0, 0.07007, 0.0])
                .int(0)
                .vec([0.0, 1.0, 0.0]);
            w.ident("null_surface")
                .ident("null_surface")
                .ident("nullbs")
                .ident("nullbs")
                .dbl(0.00875)
                .sub_end();
            w.bool(true).dbl(2.0944).bool(true).dbl(3.125);
            w.bool(true).dbl(3.47).bool(true).dbl(40.59);
            w.bool(false).bool(false).bool(false).end();
        });
        let s = surface_record(&f, 1).unwrap();
        let AsmSurface::Spline {
            subtype,
            geom: SplineGeom::HelixCircle(h),
            ranges,
            ..
        } = s
        else {
            panic!("{s:?}")
        };
        assert_eq!(subtype, "helix_spl_circ");
        assert_eq!(
            ranges,
            [[Some(2.0944), Some(3.125)], [Some(3.47), Some(40.59)]]
        );
        assert_eq!(h.ranges[1], [Some(0.0), Some(50.2)]);
        assert_eq!(h.phase, 3.125);
        assert_eq!(h.minor, [-0.17086, 0.0, 0.0]);
        assert_eq!(h.taper, 0.0);
        assert_eq!(h.radius, 0.00875);
    }

    #[test]
    fn finds_the_approximation_of_a_procedural_surface() {
        let f = file_with(|w| {
            w.record("spline-surface")
                .head()
                .bool(false)
                .sub_start()
                .ident("rb_blend_spl_sur")
                .int(23100);
            // A nested subtype with its own surface must be skipped.
            w.ident("spline")
                .bool(false)
                .sub_start()
                .ident("exact_spl_sur")
                .int(1)
                .en(0);
            w.ident("nubs")
                .int(1)
                .int(1)
                .en(0)
                .en(0)
                .en(0)
                .en(0)
                .int(2)
                .int(2);
            w.dbl(0.0)
                .int(1)
                .dbl(1.0)
                .int(1)
                .dbl(0.0)
                .int(1)
                .dbl(1.0)
                .int(1);
            for p in [[9.0, 9.0, 9.0]; 4] {
                w.dbl(p[0]).dbl(p[1]).dbl(p[2]);
            }
            w.sub_end();
            w.en(0)
                .ident("nubs")
                .int(1)
                .int(1)
                .en(0)
                .en(0)
                .en(0)
                .en(0)
                .int(2)
                .int(2);
            w.dbl(0.0)
                .int(1)
                .dbl(1.0)
                .int(1)
                .dbl(0.0)
                .int(1)
                .dbl(1.0)
                .int(1);
            for p in [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
            ] {
                w.dbl(p[0]).dbl(p[1]).dbl(p[2]);
            }
            w.sub_end()
                .bool(false)
                .bool(false)
                .bool(false)
                .bool(false)
                .end();
        });
        let s = surface_record(&f, 1).unwrap();
        let AsmSurface::Spline {
            geom: SplineGeom::BSpline { surface, exact },
            ..
        } = s
        else {
            panic!("{s:?}")
        };
        assert!(!exact);
        assert_eq!(surface.ctrl[0], [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(surface.nu * surface.nv, 4);
    }

    /// A `par_int_cur` in the summary form (mitcad#589): the knots of its
    /// approximation, the fit tolerance and the closure instead of the
    /// B-spline, then a plane (with `reverse_v`), no second surface, the
    /// plane's parameter curve (a straight 2D B-spline) and none for the
    /// second, and the tail.
    fn summary_par_int_cur(w: &mut Writer, support: impl FnOnce(&mut Writer)) {
        w.record("intcurve-curve")
            .head()
            .bool(false)
            .sub_start()
            .ident("par_int_cur")
            .en(1)
            .int(3)
            .dbl(0.0)
            .dbl(0.5)
            .dbl(1.0)
            .dbl(1e-6)
            .en(0);
        support(w);
        w.ident("null_surface");
        // nubs, degree 1, open, knots 0 (1), 1 (1): (1, 2) to (3, 2).
        w.ident("nubs")
            .int(1)
            .en(0)
            .int(2)
            .dbl(0.0)
            .int(1)
            .dbl(1.0)
            .int(1);
        w.dbl(1.0).dbl(2.0).dbl(3.0).dbl(2.0);
        w.ident("nullbs")
            .bool(false)
            .bool(false)
            .int(0)
            .int(0)
            .int(0)
            .int(0)
            .bool(true);
        w.sub_end().bool(false).bool(false).end();
    }

    #[test]
    fn decodes_surface_curves_stored_without_their_approximation() {
        let f = file_with(|w| {
            summary_par_int_cur(w, |w| {
                w.ident("plane")
                    .pos([10.0, 0.0, 0.0])
                    .vec([0.0, 0.0, 1.0])
                    .vec([1.0, 0.0, 0.0])
                    .bool(true)
                    .bool(false)
                    .bool(false)
                    .bool(false)
                    .bool(false);
            });
        });
        let c = curve_record(&f, 1).unwrap();
        let AsmCurve::Int {
            subtype,
            geom:
                IntGeom::OnSurface {
                    chart,
                    pcurve,
                    params,
                },
            ..
        } = c
        else {
            panic!("{c:?}")
        };
        assert_eq!(subtype, "par_int_cur");
        assert_eq!(params, vec![0.0, 0.5, 1.0]);
        assert_eq!(pcurve.ctrl, vec![[1.0, 2.0, 1.0], [3.0, 2.0, 1.0]]);
        // v runs against normal x u_dir (+y) with reverse_v.
        assert_eq!(
            chart,
            Chart::Plane {
                root: [10.0, 0.0, 0.0],
                u_dir: [1.0, 0.0, 0.0],
                v_dir: [0.0, -1.0, 0.0],
            }
        );
        assert_eq!(chart.eval(1.0, 2.0), Some([11.0, -2.0, 0.0]));
        // On a support the reader cannot evaluate: said so.
        let f = file_with(|w| {
            summary_par_int_cur(w, |w| {
                w.ident("torus")
                    .pos([0.0; 3])
                    .vec([0.0, 0.0, 1.0])
                    .dbl(5.0)
                    .dbl(1.0)
                    .vec([1.0, 0.0, 0.0])
                    .bool(false)
                    .bool(false)
                    .bool(false)
                    .bool(false)
                    .bool(false);
            });
        });
        let c = curve_record(&f, 1).unwrap();
        assert!(
            matches!(&c, AsmCurve::Int { geom: IntGeom::Unsupported(why), .. } if why == "par_int_cur on a torus"),
            "{c:?}"
        );
    }

    /// An embedded plane support: root, normal, u direction, `reverse_v`
    /// and the two unbounded ranges.
    fn embedded_plane(w: &mut Writer, root: P3, normal: P3, u_dir: P3) {
        w.ident("plane")
            .pos(root)
            .vec(normal)
            .vec(u_dir)
            .bool(false);
        w.bool(false).bool(false).bool(false).bool(false);
    }

    /// One side of a rolling-ball record (revision-gated layout): the kind,
    /// the support, no contact pcurve, the location, no secondary pcurve,
    /// the extension integer and no tertiary pcurve.
    fn rolling_ball_side(w: &mut Writer, support: impl FnOnce(&mut Writer)) {
        support(w);
        w.ident("nullbs")
            .pos([0.0; 3])
            .ident("nullbs")
            .int(0)
            .ident("nullbs");
    }

    /// A `rb_blend_spl_sur` stored without its approximation (as user
    /// drawings of mitcad#621 store them): two plane supports (the
    /// floor z = 0 and the wall x = 0), the slice x = z = 2 along y, offsets
    /// -2 and -2, no radius selector, the u and v ranges, the shape fields
    /// and the tail in the given form (1 the summary, 2 none), then the
    /// face's ranges.
    fn summary_rolling_ball(w: &mut Writer, form: i32, first_side: impl FnOnce(&mut Writer)) {
        w.record("spline-surface").head().bool(false);
        rolling_ball_subtype(w, form, first_side);
        w.bool(true).dbl(0.0).bool(true).dbl(1.0);
        w.bool(true).dbl(2.0).bool(true).dbl(7.0);
        w.bool(false).bool(false).bool(false).end();
    }

    /// The subtype of [`summary_rolling_ball`] (braces included); form 0
    /// stores the approximation (the bilinear patch over u in [0, 1], v in
    /// [0, 10] of the poles (0, 0, 2) (2, 0, 0) (0, 10, 2) (2, 10, 0)) and
    /// its fit tolerance 0.0043 (mitcad#622).
    fn rolling_ball_subtype(w: &mut Writer, form: i32, first_side: impl FnOnce(&mut Writer)) {
        rolling_ball_subtype_with(w, form, [-2.0, -2.0], first_side);
    }

    /// A file of a face (record 2, without loops) on a rolling-ball blend
    /// (record 1) stored with its approximation, of fit tolerance 0.0043
    /// (0.043 mm in cm), with these offsets (mitcad#622).
    pub(crate) fn coarse_blend_face_file(offsets: [f64; 2]) -> AsmFile {
        coarse_blend_face_file_with(offsets, floor_side)
    }

    /// [`coarse_blend_face_file`] with another first side (the floor
    /// tilted: no longer a cylinder, mitcad_private#198).
    pub(crate) fn coarse_blend_face_file_with(
        offsets: [f64; 2],
        first_side: impl FnOnce(&mut Writer),
    ) -> AsmFile {
        file_with(|w| {
            w.record("spline-surface").head().bool(false);
            rolling_ball_subtype_with(w, 0, offsets, first_side);
            w.bool(false).bool(false).bool(false).bool(false);
            w.bool(false).bool(false).bool(false).end();
            w.record("face")
                .head()
                .ptr(-1)
                .ptr(-1)
                .ptr(-1)
                .ptr(-1)
                .ptr(1)
                .bool(false)
                .bool(false)
                .end();
        })
    }

    fn rolling_ball_subtype_with(
        w: &mut Writer,
        form: i32,
        offsets: [f64; 2],
        first_side: impl FnOnce(&mut Writer),
    ) {
        w.sub_start().ident("rb_blend_spl_sur").int(20900);
        rolling_ball_side(w, first_side);
        rolling_ball_side(w, |w| {
            w.str("blend_support_surface");
            embedded_plane(w, [0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
            w.ident("null_curve");
        });
        w.ident("straight")
            .pos([2.0, 0.0, 2.0])
            .vec([0.0, 1.0, 0.0])
            .bool(false)
            .bool(false);
        w.dbl(offsets[0]).dbl(offsets[1]).en(-1);
        w.bool(true)
            .dbl(0.0)
            .bool(true)
            .dbl(1.0)
            .bool(false)
            .bool(false);
        w.int(1).dbl(0.05).dbl(0.01).int(1).en(form);
        if form == 0 {
            w.ident("nubs")
                .int(1)
                .int(1)
                .en(0)
                .en(0)
                .en(0)
                .en(0)
                .int(2)
                .int(2);
            w.dbl(0.0)
                .int(1)
                .dbl(1.0)
                .int(1)
                .dbl(0.0)
                .int(1)
                .dbl(10.0)
                .int(1);
            for p in [
                [0.0, 0.0, 2.0],
                [2.0, 0.0, 0.0],
                [0.0, 10.0, 2.0],
                [2.0, 10.0, 0.0],
            ] {
                w.dbl(p[0]).dbl(p[1]).dbl(p[2]);
            }
            w.dbl(0.0043);
        } else if form == 1 {
            w.int(3).dbl(0.0).dbl(0.5).dbl(1.0);
            w.int(3).dbl(0.0).dbl(5.0).dbl(10.0);
            w.dbl(0.01);
        } else {
            w.bool(true).dbl(0.0).bool(true).dbl(1.0);
            w.bool(true).dbl(0.0).bool(true).dbl(10.0);
        }
        if form != 0 {
            w.en(0).en(0).en(0).en(0);
        }
        for _ in 0..6 {
            w.int(0);
        }
        w.bool(false).int(0).int(0).int(0).sub_end();
    }

    #[test]
    fn decodes_rolling_balls_stored_without_their_approximation() {
        for form in [1, 2] {
            let f = file_with(|w| {
                summary_rolling_ball(w, form, |w| {
                    w.str("blend_support_surface");
                    embedded_plane(w, [0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
                    w.ident("null_curve");
                });
            });
            let s = surface_record(&f, 1).unwrap();
            let AsmSurface::Spline {
                subtype,
                geom: SplineGeom::RollingBall(rb),
                ranges,
                ..
            } = s
            else {
                panic!("{s:?}")
            };
            assert_eq!(subtype, "rb_blend_spl_sur");
            assert_eq!(ranges, [[Some(0.0), Some(1.0)], [Some(2.0), Some(7.0)]]);
            assert_eq!(rb.offsets, [-2.0, -2.0]);
            assert_eq!(rb.v_range, [0.0, 10.0]);
            assert_eq!(
                rb.spine,
                AsmCurve::Straight {
                    root: [2.0, 0.0, 2.0],
                    dir: [0.0, 1.0, 0.0]
                }
            );
            assert!(matches!(
                &rb.sides[0].support,
                BlendSupport::Surface(AsmSurface::Plane { normal, .. }) if *normal == [0.0, 0.0, 1.0]
            ));
            assert!(matches!(
                &rb.sides[1].support,
                BlendSupport::Surface(AsmSurface::Plane { normal, .. }) if *normal == [1.0, 0.0, 0.0]
            ));
            assert!(rb.sides.iter().all(|s| s.contact.is_none()));
            assert!(rb.approx.is_none());
        }
        // A ball rolling on an edge (a circle) on its first side.
        let f = file_with(|w| {
            summary_rolling_ball(w, 1, |w| {
                w.str("blend_support_curve").ident("null_surface");
                w.ident("ellipse")
                    .pos([0.0, 0.0, 3.0])
                    .vec([0.0, 0.0, 1.0])
                    .vec([10.0, 0.0, 0.0])
                    .dbl(1.0)
                    .bool(false)
                    .bool(false);
            });
        });
        let s = surface_record(&f, 1).unwrap();
        let AsmSurface::Spline {
            geom: SplineGeom::RollingBall(rb),
            ..
        } = s
        else {
            panic!("{s:?}")
        };
        assert!(matches!(
            &rb.sides[0].support,
            BlendSupport::Curve(AsmCurve::Ellipse { ratio, .. }) if *ratio == 1.0
        ));
    }

    /// A revision-gated `off_spl_sur` of the floor z = 0 (its `v` reversed
    /// when `reverse_v`) by `distance`, the first sense boolean `negated`,
    /// with the tail in the given form (0 with an approximation, 2
    /// without), then the face's ranges.
    fn offset_surface_record(
        w: &mut Writer,
        distance: f64,
        negated: bool,
        reverse_v: bool,
        form: i32,
    ) {
        w.record("spline-surface").head().bool(false);
        w.sub_start().ident("off_spl_sur").int(20900);
        w.ident("plane")
            .pos([0.0; 3])
            .vec([0.0, 0.0, 1.0])
            .vec([1.0, 0.0, 0.0])
            .bool(reverse_v);
        w.bool(false).bool(false).bool(false).bool(false);
        w.dbl(distance).bool(negated).bool(false);
        w.bool(false).bool(false);
        w.en(form);
        if form == 0 {
            w.ident("nubs")
                .int(1)
                .int(1)
                .en(0)
                .en(0)
                .en(0)
                .en(0)
                .int(2)
                .int(2);
            w.dbl(0.0)
                .int(1)
                .dbl(1.0)
                .int(1)
                .dbl(0.0)
                .int(1)
                .dbl(1.0)
                .int(1);
            for p in [
                [0.0, 0.0, distance],
                [1.0, 0.0, distance],
                [0.0, 1.0, distance],
                [1.0, 1.0, distance],
            ] {
                w.dbl(p[0]).dbl(p[1]).dbl(p[2]);
            }
            w.dbl(0.0);
        } else {
            w.bool(true).dbl(0.0).bool(true).dbl(1.0);
            w.bool(true).dbl(0.0).bool(true).dbl(1.0);
            w.en(0).en(0).en(0).en(0);
        }
        for _ in 0..6 {
            w.int(0);
        }
        w.bool(false).sub_end();
        w.bool(false).bool(false).bool(false).bool(false).end();
    }

    #[test]
    fn decodes_offset_surfaces_stored_without_their_approximation() {
        // mitcad_private#198: the support, the distance (negated by the
        // first sense boolean) and whether the support's du x dv runs
        // against its normal.
        let f = file_with(|w| offset_surface_record(w, 0.5, false, false, 2));
        let s = surface_record(&f, 1).unwrap();
        let AsmSurface::Spline {
            geom:
                SplineGeom::Offset {
                    support,
                    distance,
                    against,
                },
            ..
        } = s
        else {
            panic!("{s:?}")
        };
        assert!(matches!(*support, AsmSurface::Plane { normal, .. } if normal == [0.0, 0.0, 1.0]));
        assert_eq!((distance, against), (0.5, false));
        let f = file_with(|w| offset_surface_record(w, 0.5, true, true, 2));
        let s = surface_record(&f, 1).unwrap();
        assert!(
            matches!(&s, AsmSurface::Spline { geom: SplineGeom::Offset { distance, against, .. }, .. } if *distance == -0.5 && *against),
            "{s:?}"
        );
        // With its approximation: that.
        let f = file_with(|w| offset_surface_record(w, 0.5, false, false, 0));
        let s = surface_record(&f, 1).unwrap();
        assert!(
            matches!(
                &s,
                AsmSurface::Spline {
                    geom: SplineGeom::BSpline { exact: false, .. },
                    ..
                }
            ),
            "{s:?}"
        );
    }

    fn floor_side(w: &mut Writer) {
        w.str("blend_support_surface");
        embedded_plane(w, [0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
        w.ident("null_curve");
    }

    /// The floor tilted about x: the slice no longer runs along the
    /// supports' intersection, so the blend is no cylinder.
    pub(crate) fn tilted_floor_side(w: &mut Writer) {
        w.str("blend_support_surface");
        embedded_plane(w, [0.0; 3], [0.0, 0.1, 1.0], [1.0, 0.0, 0.0]);
        w.ident("null_curve");
    }

    #[test]
    fn decodes_rolling_balls_with_their_approximation() {
        // mitcad#622: the definition, and the approximation with its fit
        // tolerance; the surface spans the approximation's v.
        let f = file_with(|w| summary_rolling_ball(w, 0, floor_side));
        let s = surface_record(&f, 1).unwrap();
        let AsmSurface::Spline {
            geom: SplineGeom::RollingBall(rb),
            ranges,
            ..
        } = s
        else {
            panic!("{s:?}")
        };
        assert_eq!(ranges, [[Some(0.0), Some(1.0)], [Some(2.0), Some(7.0)]]);
        assert_eq!(rb.offsets, [-2.0, -2.0]);
        assert_eq!(rb.v_range, [0.0, 10.0]);
        assert_eq!(rb.fit, 0.0043);
        let approx = rb.approx.expect("the approximation");
        assert_eq!((approx.nu, approx.nv), (2, 2));
        assert_eq!(approx.ctrl[1], [2.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn decodes_contact_curves_of_rolling_balls_with_their_approximation() {
        // mitcad#622: a blend_int_cur stored with its approximation (fit
        // tolerance 0.0043) on such a blend, whose parameter curve on it
        // (u = 1 over v in [2, 7]) is stored: the approximation, its fit
        // tolerance and the curve as the blend along the parameter curve.
        let f = file_with(|w| {
            w.record("intcurve-curve")
                .head()
                .bool(false)
                .sub_start()
                .ident("blend_int_cur")
                .int(20900)
                .en(0);
            w.ident("nubs")
                .int(1)
                .en(0)
                .int(2)
                .dbl(2.0)
                .int(1)
                .dbl(7.0)
                .int(1);
            w.dbl(2.0).dbl(2.0).dbl(0.0).dbl(2.0).dbl(7.0).dbl(0.0);
            w.dbl(0.0043);
            w.ident("spline").bool(false);
            rolling_ball_subtype(w, 0, floor_side);
            w.bool(false).bool(false).bool(false).bool(false);
            w.ident("null_surface");
            w.ident("nubs")
                .int(1)
                .en(0)
                .int(2)
                .dbl(2.0)
                .int(1)
                .dbl(7.0)
                .int(1);
            w.dbl(1.0).dbl(2.0).dbl(1.0).dbl(7.0);
            w.ident("nullbs")
                .bool(false)
                .bool(false)
                .int(0)
                .int(0)
                .int(0)
                .int(0)
                .bool(true);
            w.sub_end().bool(false).bool(false).end();
        });
        let c = curve_record(&f, 1).unwrap();
        let AsmCurve::Int {
            subtype,
            geom: IntGeom::Approximated { approx, fit, exact },
            ..
        } = c
        else {
            panic!("{c:?}")
        };
        assert_eq!(subtype, "blend_int_cur");
        assert_eq!(fit, 0.0043);
        assert_eq!(approx.ctrl[1], [2.0, 7.0, 0.0, 1.0]);
        let IntGeom::OnSurface { chart, pcurve, .. } = *exact else {
            panic!("{exact:?}")
        };
        assert!(matches!(
            chart,
            Chart::Procedural(s) if matches!(&*s, AsmSurface::Spline { geom: SplineGeom::RollingBall(_), .. })
        ));
        assert_eq!(pcurve.ctrl, vec![[1.0, 2.0, 1.0], [1.0, 7.0, 1.0]]);
    }

    #[test]
    fn decodes_sum_surfaces_stored_without_their_approximation() {
        // sum_spl_sur (revision-gated): two curves with their intervals, the
        // origin and the tail without the approximation (form 2).
        let f = file_with(|w| {
            w.record("spline-surface")
                .head()
                .bool(false)
                .sub_start()
                .ident("sum_spl_sur")
                .int(20900);
            w.ident("straight")
                .pos([0.0, 0.0, 0.0])
                .vec([1.0, 0.0, 0.0])
                .bool(false)
                .bool(false);
            w.ident("straight")
                .pos([0.0, 5.0, 0.0])
                .vec([0.0, 0.0, 2.0])
                .bool(false)
                .bool(false);
            w.pos([0.0, 5.0, 0.0]).en(2);
            w.bool(true).dbl(0.0).bool(true).dbl(1.0);
            w.bool(true).dbl(0.0).bool(true).dbl(1.0);
            w.en(0).en(0).en(0).en(0);
            for _ in 0..6 {
                w.int(0);
            }
            w.bool(false).sub_end();
            w.bool(false).bool(false).bool(false).bool(false).end();
        });
        let s = surface_record(&f, 1).unwrap();
        let AsmSurface::Spline {
            geom: SplineGeom::Sum { curves, origin },
            ..
        } = s
        else {
            panic!("{s:?}")
        };
        assert_eq!(origin, [0.0, 5.0, 0.0]);
        assert_eq!(
            curves[1],
            AsmCurve::Straight {
                root: [0.0, 5.0, 0.0],
                dir: [0.0, 0.0, 2.0]
            }
        );
    }

    #[test]
    fn cone_charts_follow_the_generator_and_the_angle() {
        // A cone of base radius 2 opening at 45 degrees along +z, u scaled
        // by 2: u = 1 is 2 cos(45) along the axis, where the radius is
        // 2 + 2 cos(45) tan(45).
        let (s, c) = std::f64::consts::FRAC_PI_4.sin_cos();
        let cone = Chart::Cone {
            center: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            major: [2.0, 0.0, 0.0],
            minor: [0.0, 2.0, 0.0],
            sin: s,
            cos: c,
            u_scale: 2.0,
        };
        let d = 2.0 * c;
        let p = cone.eval(1.0, std::f64::consts::FRAC_PI_2).unwrap();
        for (got, want) in p.iter().zip([0.0, 2.0 + d, d]) {
            assert!((got - want).abs() < 1e-12, "{p:?}");
        }
        // A cylinder: u in radii along the axis.
        let cylinder = Chart::Cone {
            center: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            major: [2.0, 0.0, 0.0],
            minor: [0.0, 2.0, 0.0],
            sin: 0.0,
            cos: 1.0,
            u_scale: 2.0,
        };
        let p = cylinder.eval(0.5, 0.0).unwrap();
        for (got, want) in p.iter().zip([2.0, 0.0, 1.0]) {
            assert!((got - want).abs() < 1e-12, "{p:?}");
        }
    }
}
