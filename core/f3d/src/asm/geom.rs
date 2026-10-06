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

    pub fn enumeration(&mut self) -> Result<i64> {
        match self.next_token()? {
            Token::Enum(v) => Ok(*v),
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

    pub fn point(&mut self) -> Result<P3> {
        match self.next_token()? {
            Token::Pos(p) | Token::Vector(p) => Ok(*p),
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
    Unsupported(String),
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
    Unsupported(String),
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
            skip_version(&mut c);
            match c.peek() {
                Some(Token::Ident(k)) if k == "nubs" || k == "nurbs" => match bs3_curve(&mut c)? {
                    Some(b) => IntGeom::BSpline(b),
                    None => IntGeom::Unsupported(format!("{name} without a B-spline")),
                },
                Some(Token::Ident(k)) if k == "nullbs" => {
                    IntGeom::Unsupported(format!("{name} without a B-spline"))
                }
                _ => IntGeom::Unsupported(format!("{name}: unknown layout")),
            }
        }
    };
    Ok((name, geom))
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
        _ => match find_bs3_surface(file, def)? {
            Some(s) => SplineGeom::BSpline {
                surface: Box::new(s),
                exact: false,
            },
            None => SplineGeom::Unsupported(format!("{name} without a stored approximation")),
        },
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
mod tests {
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
}
