// SPDX-License-Identifier: MIT
//! The geometry classes of the format as the file stores them: curves,
//! surfaces, B-reps, extrusions, meshes and block instances. Each object is
//! a class chunk (`TCODE_CLASS`) with the class's UUID and its
//! data; a class this reader does not know is kept by its name, so that
//! the import can say what it left out.
//!
//! The values the conversion relies on are checked here, so that a
//! damaged file is refused object by object instead of reaching the
//! B-spline code: knot vectors (finite, non-decreasing, no knot of a
//! higher multiplicity than the degree inside, a domain that is not
//! empty), the weights of rational control points (finite and not 0), the
//! angles of arcs (at most a full turn) and the parameters of polylines
//! and polycurves (increasing). Arrays are allocated only for as many
//! items as the bytes left in their chunk can hold.

use crate::archive::{
    Reader, Result, TCODE_ANONYMOUS_CHUNK, TCODE_CLASS, TCODE_CLASS_DATA, TCODE_CLASS_UUID, Uuid,
    err, err_at,
};

pub type P3 = [f64; 3];

/// A plane: origin and axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    pub origin: P3,
    pub x: P3,
    pub y: P3,
    pub z: P3,
}

/// A NURBS curve or the rows of a surface: the format's knot vector (two
/// knots fewer than the usual one, `order + count - 2`) and its control
/// points, homogeneous (`x w, y w, z w, w`) when rational.
#[derive(Clone, Debug, PartialEq)]
pub struct NurbsCurve {
    pub dim: usize,
    pub rational: bool,
    pub order: usize,
    pub knots: Vec<f64>,
    /// `dim` (+1 when rational) values each; 2D curves have dim 2.
    pub cvs: Vec<[f64; 4]>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Curve {
    Nurbs(NurbsCurve),
    Line {
        from: P3,
        to: P3,
        domain: [f64; 2],
    },
    /// A circular arc: the circle's plane and radius, its angles, and the
    /// curve's domain, which maps linearly onto the angles.
    Arc {
        plane: Plane,
        radius: f64,
        angles: [f64; 2],
        domain: [f64; 2],
    },
    Polyline {
        points: Vec<P3>,
        params: Vec<f64>,
    },
    /// Segments joined end to end; segment i runs over
    /// `params[i]..params[i + 1]` of the polycurve.
    Poly {
        segments: Vec<Curve>,
        params: Vec<f64>,
    },
    /// A class this reader does not decode.
    Other(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct NurbsSurface {
    pub dim: usize,
    pub rational: bool,
    pub order: [usize; 2],
    pub count: [usize; 2],
    pub knots: [Vec<f64>; 2],
    /// `cvs[i * count[1] + j]`, i in u.
    pub cvs: Vec<[f64; 4]>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Surface {
    Nurbs(NurbsSurface),
    /// A plane; the surface's (s, t) map linearly from `domain` onto
    /// `extents`, the coordinates along the plane's x and y axes.
    Plane {
        plane: Plane,
        domain: [[f64; 2]; 2],
        extents: [[f64; 2]; 2],
    },
    /// A curve turned about an axis: u the angle, v the curve's parameter,
    /// or swapped when `transposed`.
    Revolution {
        axis: [P3; 2],
        angles: [f64; 2],
        transposed: bool,
        curve: Box<Curve>,
    },
    /// `curves[0](s) + curves[1](t) + base`.
    Sum {
        base: P3,
        curves: Box<[Curve; 2]>,
    },
    Other(String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrepVertex {
    pub point: P3,
    pub tolerance: f64,
}

/// An edge: part of a 3D curve (`curve` index, `proxy` its parameters),
/// reversed or not, from `vertices[0]` to `vertices[1]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrepEdge {
    pub curve: i32,
    pub reversed: bool,
    pub proxy: [f64; 2],
    pub vertices: [i32; 2],
    pub tolerance: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrimType {
    Unknown,
    Boundary,
    Mated,
    Seam,
    Singular,
    CurveOnSurface,
    PointOnSurface,
    Slit,
}

/// A trim: a use of an edge by a loop (`edge` -1 for a singular trim at a
/// pole); `reversed_3d` when it runs against the edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrepTrim {
    pub edge: i32,
    pub vertices: [i32; 2],
    pub reversed_3d: bool,
    pub kind: TrimType,
    pub face_loop: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopType {
    Unknown,
    Outer,
    Inner,
    Slit,
    CurveOnSurface,
    PointOnSurface,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BrepLoop {
    pub trims: Vec<i32>,
    pub kind: LoopType,
    pub face: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BrepFace {
    pub loops: Vec<i32>,
    pub surface: i32,
    /// The face's normal is against the surface's.
    pub reversed: bool,
}

/// A boundary representation as the file keeps it: its 3D curves (the
/// 2D trim curves are not read; OCCT computes its own), surfaces and
/// topology, and the render meshes saved with its faces.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Brep {
    pub curves: Vec<Option<Box<Curve>>>,
    pub surfaces: Vec<Option<Box<Surface>>>,
    pub vertices: Vec<BrepVertex>,
    pub edges: Vec<BrepEdge>,
    pub trims: Vec<BrepTrim>,
    pub loops: Vec<BrepLoop>,
    pub faces: Vec<BrepFace>,
    /// 0 not known or not a solid, 1 a solid, 2 a solid whose normals
    /// point in.
    pub is_solid: i32,
    /// Each face's render mesh, where the file has one.
    pub render_meshes: Vec<Option<Mesh>>,
}

/// A mesh: its vertices and its faces (quads, or triangles whose last two
/// indices are equal).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Mesh {
    pub vertices: Vec<P3>,
    pub faces: Vec<[u32; 4]>,
}

impl Mesh {
    /// The faces as triangles (quads split along their first diagonal);
    /// faces with an index past the vertices are left out.
    pub fn triangles(&self) -> Vec<[u32; 3]> {
        let n = u32::try_from(self.vertices.len()).unwrap_or(u32::MAX);
        let mut out = Vec::with_capacity(self.faces.len() * 2);
        for f in &self.faces {
            if f.iter().any(|&i| i >= n) {
                continue;
            }
            out.push([f[0], f[1], f[2]]);
            if f[2] != f[3] {
                out.push([f[0], f[2], f[3]]);
            }
        }
        out.retain(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]);
        out
    }
}

/// A lightweight extrusion: planar profiles moved along a line, with caps
/// where `cap` says so. The profiles are in the plane whose z axis is the
/// path's direction and whose y axis is `up` (x = up × z), at the path's
/// start; more than one profile (an outer one, then holes) are the
/// segments of a polycurve.
#[derive(Clone, Debug, PartialEq)]
pub struct Extrusion {
    pub profile: Curve,
    pub profile_count: usize,
    pub path: [P3; 2],
    /// The part of the path, as parameters of the line (0 at its start, 1
    /// at its end).
    pub t: [f64; 2],
    pub up: P3,
    /// The end planes' normals (in the profile's frame) where an end is
    /// mitered.
    pub miter: [Option<P3>; 2],
    pub cap: [bool; 2],
}

/// A block instance: its definition's id and placement (rows of a 4x4
/// matrix applied to column vectors).
#[derive(Clone, Debug, PartialEq)]
pub struct InstanceRef {
    pub definition: Uuid,
    pub xform: [[f64; 4]; 4],
}

/// An object of the file's object table, by what it is.
#[derive(Clone, Debug, PartialEq)]
pub enum Geometry {
    Brep(Box<Brep>),
    Extrusion(Box<Extrusion>),
    Surface(Box<Surface>),
    Mesh(Box<Mesh>),
    Curve(Box<Curve>),
    Instance(InstanceRef),
    /// A class not taken (points, annotations, SubD, …), by name.
    Other(String),
}

pub mod class {
    use crate::archive::Uuid;

    pub const NURBS_CURVE: Uuid = Uuid::parse("4ED7D4DD-E947-11d3-BFE5-0010830122F0");
    pub const NURBS_CURVE_TL: Uuid = Uuid::parse("5EAF1119-0B51-11d4-BFFE-0010830122F0");
    pub const NURBS_CURVE_OLD: Uuid = Uuid::parse("76A709D5-1550-11d4-8000-0010830122F0");
    pub const LINE_CURVE: Uuid = Uuid::parse("4ED7D4DB-E947-11d3-BFE5-0010830122F0");
    pub const ARC_CURVE: Uuid = Uuid::parse("CF33BE2A-09B4-11d4-BFFB-0010830122F0");
    pub const POLY_CURVE: Uuid = Uuid::parse("4ED7D4E0-E947-11d3-BFE5-0010830122F0");
    pub const POLY_CURVE_OLD: Uuid = Uuid::parse("EF638317-154B-11d4-8000-0010830122F0");
    pub const POLYLINE_CURVE: Uuid = Uuid::parse("4ED7D4E6-E947-11d3-BFE5-0010830122F0");
    pub const NURBS_SURFACE: Uuid = Uuid::parse("4ED7D4DE-E947-11d3-BFE5-0010830122F0");
    pub const NURBS_SURFACE_TL: Uuid = Uuid::parse("4760C817-0BE3-11d4-BFFE-0010830122F0");
    pub const NURBS_SURFACE_OLD: Uuid = Uuid::parse("FA4FD4B5-1613-11d4-8000-0010830122F0");
    pub const PLANE_SURFACE: Uuid = Uuid::parse("4ED7D4DF-E947-11d3-BFE5-0010830122F0");
    pub const CLIPPING_PLANE_SURFACE: Uuid = Uuid::parse("DBC5A584-CE3F-4170-98A8-497069CA5C36");
    pub const REV_SURFACE: Uuid = Uuid::parse("A16220D3-163B-11d4-8000-0010830122F0");
    pub const REV_SURFACE_TL: Uuid = Uuid::parse("0A8401B6-4D34-4b99-8615-1B4E723DC4E5");
    pub const SUM_SURFACE: Uuid = Uuid::parse("C4CD5359-446D-4690-9FF5-29059732472B");
    pub const SUM_SURFACE_TL: Uuid = Uuid::parse("665F6331-2A66-4cce-81D0-B5EEBD9B5417");
    pub const BREP: Uuid = Uuid::parse("60B5DBC5-E660-11d3-BFE4-0010830122F0");
    pub const BREP_TL: Uuid = Uuid::parse("F06FC243-A32A-4608-9DD8-A7D2C4CE2A36");
    pub const EXTRUSION: Uuid = Uuid::parse("36F53175-72B8-4d47-BF1F-B4E6FC24F4B9");
    pub const MESH: Uuid = Uuid::parse("4ED7D4E4-E947-11d3-BFE5-0010830122F0");
    pub const INSTANCE_REF: Uuid = Uuid::parse("F9CFB638-B9D4-4340-87E3-C56E7865D96A");
    pub const INSTANCE_DEFINITION: Uuid = Uuid::parse("26F8BFF6-2618-417f-A158-153D64A94989");
    pub const LAYER: Uuid = Uuid::parse("95809813-E985-11d3-BFE5-0010830122F0");

    /// The names of the classes the import reports when it leaves them
    /// out.
    pub const NAMES: &[(&str, &str)] = &[
        ("C3101A1D-F157-11d3-BFE7-0010830122F0", "point"),
        ("2488F347-F8FA-11d3-BFEC-0010830122F0", "point cloud"),
        ("4ED7D4E5-E947-11d3-BFE5-0010830122F0", "point grid"),
        ("F09BA4D9-455B-42C3-BA3B-E6CCACEF853B", "SubD"),
        ("0559733B-5332-49d1-A936-0532AC76ADE5", "hatch"),
        ("74198302-CDF4-4f95-9609-6D684F22AB37", "text dot"),
        ("57376349-62A9-4A16-B411-A46BCD544790", "text"),
        ("945BF594-6FF9-4F5C-BFC0-B3AF528F29D2", "leader"),
        ("E550882B-F44D-4154-A1EF-6E50CBBBF543", "linear dimension"),
        ("D417786B-F6CD-4F12-9E1F-063F414DBEB6", "angular dimension"),
        ("FC749C2F-4C00-41FD-9840-26D94F047AD3", "radial dimension"),
        ("03124828-4C9B-4D28-9A82-664DDDE7A14F", "ordinate dimension"),
        ("D46767BA-7E8F-4D9D-9A92-66050219A5B9", "center mark"),
        ("85A08513-F383-11d3-BFE7-0010830122F0", "light"),
        ("C8C66EFA-B3CB-4e00-9440-2AD66203379E", "detail view"),
        ("D379E6D8-7C31-4407-A913-E3B7040D034A", "morph control"),
        ("4ED7D4D8-E947-11d3-BFE5-0010830122F0", "curve on surface"),
        ("4ED7D4D9-E947-11d3-BFE5-0010830122F0", "curve proxy"),
        ("4ED7D4E2-E947-11d3-BFE5-0010830122F0", "surface proxy"),
        ("00C61749-D430-4ecc-83A8-29130A20CF9C", "offset surface"),
    ];

    /// What an object of the class is, for the report: the kinds
    /// `crate::kind_name` gives the objects read (`B-rep`, `extrusion`,
    /// `surface`, `mesh`, `curve`, `block instance`), else [`name`]. Used
    /// for an object whose data could not be read.
    pub fn kind(id: Uuid) -> String {
        let is = |ids: &[Uuid]| ids.contains(&id);
        if is(&[BREP, BREP_TL]) {
            "B-rep".to_owned()
        } else if id == EXTRUSION {
            "extrusion".to_owned()
        } else if id == MESH {
            "mesh".to_owned()
        } else if id == INSTANCE_REF {
            "block instance".to_owned()
        } else if is(&[
            NURBS_SURFACE,
            NURBS_SURFACE_TL,
            NURBS_SURFACE_OLD,
            PLANE_SURFACE,
            CLIPPING_PLANE_SURFACE,
            REV_SURFACE,
            REV_SURFACE_TL,
            SUM_SURFACE,
            SUM_SURFACE_TL,
        ]) {
            "surface".to_owned()
        } else if is(&[
            NURBS_CURVE,
            NURBS_CURVE_TL,
            NURBS_CURVE_OLD,
            LINE_CURVE,
            ARC_CURVE,
            POLY_CURVE,
            POLY_CURVE_OLD,
            POLYLINE_CURVE,
        ]) {
            "curve".to_owned()
        } else {
            name(id)
        }
    }

    /// The class's name for the report; a class not in [`NAMES`] is "an
    /// object of another class" (its id is no text for a person).
    pub fn name(id: Uuid) -> String {
        let text = id.to_string();
        NAMES
            .iter()
            .find(|(u, _)| u.eq_ignore_ascii_case(&text))
            .map_or_else(
                || "object of another class".to_owned(),
                |(_, n)| (*n).to_owned(),
            )
    }
}

/// A class chunk: its UUID and the reader of its data, or None for a null
/// object.
pub fn class_chunk<'a>(r: &mut Reader<'a>) -> Result<Option<(Uuid, Reader<'a>)>> {
    let mut c = r.expect_chunk(TCODE_CLASS, "an object")?;
    let mut id = c.data.expect_chunk(TCODE_CLASS_UUID, "an object's class")?;
    let uuid = id.data.uuid()?;
    if uuid.is_nil() {
        return Ok(None);
    }
    let data = c.data.expect_chunk(TCODE_CLASS_DATA, "an object's data")?;
    Ok(Some((uuid, data.data)))
}

/// A curve object (a class chunk).
pub fn read_curve_object(r: &mut Reader) -> Result<Option<Curve>> {
    let Some((uuid, mut data)) = class_chunk(r)? else {
        return Ok(None);
    };
    read_curve(uuid, &mut data).map(Some)
}

/// A surface object (a class chunk).
pub fn read_surface_object(r: &mut Reader) -> Result<Option<Surface>> {
    let Some((uuid, mut data)) = class_chunk(r)? else {
        return Ok(None);
    };
    read_surface(uuid, &mut data).map(Some)
}

fn plane(r: &mut Reader) -> Result<Plane> {
    let origin = r.point()?;
    let x = r.point()?;
    let y = r.point()?;
    let z = r.point()?;
    r.skip(32)?; // the plane equation
    Ok(Plane { origin, x, y, z })
}

/// The highest B-spline order taken: OCCT's B-splines have a degree of 25
/// at most, and the conversion's work grows with the square of the order.
pub const MAX_ORDER: usize = 26;

/// Whether a knot vector of the format (`order + count - 2` knots, so
/// `count >= order`) is valid: every knot finite, non-decreasing, no knot
/// repeated more than `order - 1` times (`knots[i] < knots[i + order -
/// 1]`), and a domain (`knots[order - 2]` to `knots[count - 1]`) that is
/// not empty.
pub fn valid_knots(knots: &[f64], order: usize) -> bool {
    order >= 2
        && knots.len() >= 2 * order - 2
        && knots.iter().all(|k| k.is_finite())
        && knots.windows(2).all(|w| w[0] <= w[1])
        && knots.windows(order).all(|w| w[0] < w[order - 1])
        && knots[order - 2] < knots[knots.len() + 1 - order]
}

/// The control points read (homogeneous, the weight last), checked: the
/// coordinates finite, a rational point's weight finite and not 0.
fn control_points(
    r: &mut Reader,
    n: usize,
    dim: usize,
    rational: bool,
    what: &str,
) -> Result<Vec<[f64; 4]>> {
    let size = dim + usize::from(rational);
    if n.saturating_mul(size * 8) > r.remaining() {
        return err(format!("{what} longer than its chunk"));
    }
    let mut cvs = Vec::with_capacity(n);
    for _ in 0..n {
        let mut cv = [0.0; 4];
        for v in cv.iter_mut().take(size) {
            *v = r.f64()?;
        }
        if !rational {
            cv[3] = 1.0;
        } else if dim < 3 {
            // The weight comes after the coordinates.
            cv[3] = cv[dim];
            cv[dim] = 0.0;
        }
        if cv.iter().any(|v| !v.is_finite()) || cv[3] == 0.0 {
            return err(format!(
                "{what} with a control point that is not a number or a weight of 0"
            ));
        }
        cvs.push(cv);
    }
    Ok(cvs)
}

/// Whether the parameters are finite and increasing.
fn increasing(params: &[f64]) -> bool {
    params.iter().all(|p| p.is_finite()) && params.windows(2).all(|w| w[0] < w[1])
}

pub fn read_curve(uuid: Uuid, r: &mut Reader) -> Result<Curve> {
    use class::*;
    if uuid == NURBS_CURVE || uuid == NURBS_CURVE_TL || uuid == NURBS_CURVE_OLD {
        return nurbs_curve(r).map(Curve::Nurbs);
    }
    if uuid == LINE_CURVE {
        let (major, _) = r.chunk_version()?;
        if major != 1 {
            return err_at(
                "a line curve of a version MitCad does not read",
                format!("version {major}"),
            );
        }
        let from = r.point()?;
        let to = r.point()?;
        let domain = r.interval()?;
        if !increasing(&domain) {
            return err("a line whose domain is not valid");
        }
        return Ok(Curve::Line { from, to, domain });
    }
    if uuid == ARC_CURVE {
        let (major, _) = r.chunk_version()?;
        if major != 1 {
            return err_at(
                "an arc curve of a version MitCad does not read",
                format!("version {major}"),
            );
        }
        let plane = plane(r)?;
        let radius = r.f64()?;
        r.skip(72)?; // three points of the circle, obsolete
        let angles = r.interval()?;
        let domain = r.interval()?;
        // An arc turns at most once, in the direction of its angles, over
        // an increasing domain.
        let sweep = angles[1] - angles[0];
        if !(radius.is_finite()
            && sweep.is_finite()
            && sweep > 0.0
            && sweep <= std::f64::consts::TAU * (1.0 + 1e-9)
            && increasing(&domain))
        {
            return err("an arc whose radius, angles or domain are not valid");
        }
        return Ok(Curve::Arc {
            plane,
            radius,
            angles,
            domain,
        });
    }
    if uuid == POLYLINE_CURVE {
        let (major, _) = r.chunk_version()?;
        if major != 1 {
            return err_at(
                "a polyline curve of a version MitCad does not read",
                format!("version {major}"),
            );
        }
        let points = r.point_array()?;
        let params = r.f64_array()?;
        if params.len() != points.len() || !increasing(&params) {
            return err("a polyline whose parameters do not match its points");
        }
        return Ok(Curve::Polyline { points, params });
    }
    if uuid == POLY_CURVE || uuid == POLY_CURVE_OLD {
        let (_major, _) = r.chunk_version()?;
        // A segment is a class chunk: three chunk headers and its id at
        // least.
        let count = r.count(36)?;
        r.skip(8)?; // reserved
        r.skip(48)?; // bounding box
        let params = r.f64_array()?;
        if count == 0 || params.len() != count + 1 || !increasing(&params) {
            return err("a polycurve whose parameters do not match its segments");
        }
        let mut segments = Vec::with_capacity(count);
        for _ in 0..count {
            match read_curve_object(r)? {
                Some(c) => segments.push(c),
                None => return err("a polycurve with an empty segment"),
            }
        }
        if params.len() != segments.len() + 1 {
            return err("a polycurve whose parameters do not match its segments");
        }
        return Ok(Curve::Poly { segments, params });
    }
    Ok(Curve::Other(class::name(uuid)))
}

fn nurbs_curve(r: &mut Reader) -> Result<NurbsCurve> {
    let (major, _) = r.chunk_version()?;
    if major != 1 {
        return err_at(
            "a NURBS curve of a version MitCad does not read",
            format!("version {major}"),
        );
    }
    let dim = r.i32()?;
    let rational = r.i32()? != 0;
    let order = r.i32()?;
    let count = r.i32()?;
    r.skip(8)?; // reserved
    r.skip(48)?; // bounding box
    if !(1..=3).contains(&dim) || order < 2 || order as usize > MAX_ORDER || count < order {
        return err_at(
            "a NURBS curve whose dimension, order or number of control points is not valid",
            format!("dimension {dim}, order {order}, {count} control points"),
        );
    }
    let (dim, order, count) = (dim as usize, order as usize, count as usize);
    let knots = r.f64_array()?;
    if knots.len() != order + count - 2 {
        return err_at(
            "a NURBS curve whose knot count does not match",
            format!(
                "order {order}, {count} control points, {} knots",
                knots.len()
            ),
        );
    }
    if !valid_knots(&knots, order) {
        return err("a NURBS curve whose knots are not valid");
    }
    let n = r.count(0)?;
    if n != count {
        return err("a NURBS curve whose control point count does not match");
    }
    let cvs = control_points(r, count, dim, rational, "a NURBS curve")?;
    Ok(NurbsCurve {
        dim,
        rational,
        order,
        knots,
        cvs,
    })
}

pub fn read_surface(uuid: Uuid, r: &mut Reader) -> Result<Surface> {
    use class::*;
    if uuid == NURBS_SURFACE || uuid == NURBS_SURFACE_TL || uuid == NURBS_SURFACE_OLD {
        return nurbs_surface(r).map(Surface::Nurbs);
    }
    if uuid == PLANE_SURFACE || uuid == CLIPPING_PLANE_SURFACE {
        let (major, minor) = r.chunk_version()?;
        if major != 1 {
            return err_at(
                "a plane surface of a version MitCad does not read",
                format!("version {major}"),
            );
        }
        let plane = plane(r)?;
        let domain = [r.interval()?, r.interval()?];
        let extents = if minor >= 1 {
            [r.interval()?, r.interval()?]
        } else {
            domain
        };
        return Ok(Surface::Plane {
            plane,
            domain,
            extents,
        });
    }
    if uuid == REV_SURFACE || uuid == REV_SURFACE_TL {
        let (major, _) = r.chunk_version()?;
        if major != 1 && major != 2 {
            return err_at(
                "a surface of revolution of a version MitCad does not read",
                format!("version {major}"),
            );
        }
        let axis = [r.point()?, r.point()?];
        let angles = r.interval()?;
        if major == 2 {
            r.skip(16)?; // the angle's parameters
        }
        r.skip(48)?; // bounding box
        let transposed = r.i32()? != 0;
        let has_curve = r.u8()? != 0;
        let curve = if has_curve {
            read_curve_object(r)?
        } else {
            None
        };
        let Some(curve) = curve else {
            return err("a surface of revolution without its curve");
        };
        return Ok(Surface::Revolution {
            axis,
            angles,
            transposed,
            curve: Box::new(curve),
        });
    }
    if uuid == SUM_SURFACE || uuid == SUM_SURFACE_TL {
        let (major, _) = r.chunk_version()?;
        if major != 1 {
            return err_at(
                "a sum surface of a version MitCad does not read",
                format!("version {major}"),
            );
        }
        let base = r.point()?;
        r.skip(48)?; // bounding box
        let a = read_curve_object(r)?;
        let b = read_curve_object(r)?;
        let (Some(a), Some(b)) = (a, b) else {
            return err("a sum surface without its curves");
        };
        return Ok(Surface::Sum {
            base,
            curves: Box::new([a, b]),
        });
    }
    Ok(Surface::Other(class::name(uuid)))
}

fn nurbs_surface(r: &mut Reader) -> Result<NurbsSurface> {
    let (major, _) = r.chunk_version()?;
    if major != 1 {
        return err_at(
            "a NURBS surface of a version MitCad does not read",
            format!("version {major}"),
        );
    }
    let dim = r.i32()?;
    let rational = r.i32()? != 0;
    let order = [r.i32()?, r.i32()?];
    let count = [r.i32()?, r.i32()?];
    r.skip(8)?; // reserved
    r.skip(48)?; // bounding box
    if !(1..=3).contains(&dim)
        || (0..2).any(|k| order[k] < 2 || order[k] as usize > MAX_ORDER || count[k] < order[k])
    {
        return err_at(
            "a NURBS surface whose dimension, orders or numbers of control points are not valid",
            format!("dimension {dim}, orders {order:?}, {count:?} control points"),
        );
    }
    let dim = dim as usize;
    let order = order.map(|o| o as usize);
    let count = count.map(|c| c as usize);
    let mut knots: [Vec<f64>; 2] = [Vec::new(), Vec::new()];
    for k in 0..2 {
        knots[k] = r.f64_array()?;
        if knots[k].len() != order[k] + count[k] - 2 {
            return err("a NURBS surface whose knot count does not match");
        }
        if !valid_knots(&knots[k], order[k]) {
            return err("a NURBS surface whose knots are not valid");
        }
    }
    let n = r.count(0)?;
    if Some(n) != count[0].checked_mul(count[1]) {
        return err("a NURBS surface whose control point count does not match");
    }
    let cvs = control_points(r, n, dim, rational, "a NURBS surface")?;
    Ok(NurbsSurface {
        dim,
        rational,
        order,
        count,
        knots,
        cvs,
    })
}

/// An array of curves or surfaces: an anonymous chunk with a count and,
/// for each item, a flag and the object. The items are boxed, so that an
/// array of empty items (4 bytes each in the file) takes no more memory
/// than a pointer each.
fn object_array<T>(
    r: &mut Reader,
    what: &str,
    read: impl Fn(&mut Reader) -> Result<Option<T>>,
) -> Result<Vec<Option<Box<T>>>> {
    let (major, _, mut a) = r.anonymous(what)?;
    if major != 1 {
        return err_at(
            format!("{what} of a version MitCad does not read"),
            format!("version {major}"),
        );
    }
    let n = a.count(4)?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        if a.i32()? == 1 {
            out.push(read(&mut a)?.map(Box::new));
        } else {
            out.push(None);
        }
    }
    Ok(out)
}

fn trim_type(i: i32) -> TrimType {
    match i {
        1 => TrimType::Boundary,
        2 => TrimType::Mated,
        3 => TrimType::Seam,
        4 => TrimType::Singular,
        5 => TrimType::CurveOnSurface,
        6 => TrimType::PointOnSurface,
        7 => TrimType::Slit,
        _ => TrimType::Unknown,
    }
}

fn loop_type(i: i32) -> LoopType {
    match i {
        1 => LoopType::Outer,
        2 => LoopType::Inner,
        3 => LoopType::Slit,
        4 => LoopType::CurveOnSurface,
        5 => LoopType::PointOnSurface,
        _ => LoopType::Unknown,
    }
}

/// The items of one of a B-rep's topology arrays, each `item_size` bytes
/// in the file at least.
fn topology<T>(
    r: &mut Reader,
    what: &str,
    item_size: usize,
    read: impl Fn(&mut Reader) -> Result<T>,
) -> Result<Vec<T>> {
    let (major, _, mut a) = r.anonymous(what)?;
    if major != 1 {
        return err_at(
            format!("{what} of a version MitCad does not read"),
            format!("version {major}"),
        );
    }
    let n = a.count(item_size)?;
    (0..n).map(|_| read(&mut a)).collect()
}

fn read_brep(r: &mut Reader) -> Result<Brep> {
    let (major, minor) = r.chunk_version()?;
    if major != 3 {
        return err_at(
            "a B-rep of a version MitCad does not read (files of format versions 1 and 2)",
            format!("version {major}.{minor}"),
        );
    }
    // The 2D curves: skipped with their chunk (OCCT computes the faces'
    // parameter curves).
    r.expect_chunk(TCODE_ANONYMOUS_CHUNK, "the B-rep's 2D curves")?;
    let curves = object_array(r, "the B-rep's 3D curves", read_curve_object)?;
    let surfaces = object_array(r, "the B-rep's surfaces", read_surface_object)?;
    let new_trims = r.writer >= 200_206_180;
    // Each item at least: index, point, an empty array, tolerance.
    let vertices = topology(r, "the B-rep's vertices", 40, |r| {
        r.i32()?;
        let point = r.point()?;
        r.i32_array()?; // its edges
        let tolerance = r.f64()?;
        Ok(BrepVertex { point, tolerance })
    })?;
    let edges = topology(r, "the B-rep's edges", 48, |r| {
        r.i32()?;
        let curve = r.i32()?;
        let reversed = r.i32()? != 0;
        let proxy = r.interval()?;
        let vertices = [r.i32()?, r.i32()?];
        r.i32_array()?; // its trims
        let tolerance = r.f64()?;
        if new_trims {
            r.interval()?; // the edge's own domain
        }
        Ok(BrepEdge {
            curve,
            reversed,
            proxy,
            vertices,
            tolerance,
        })
    })?;
    let trims = topology(r, "the B-rep's trims", 132, |r| {
        r.i32()?;
        r.i32()?; // its 2D curve
        r.interval()?;
        let edge = r.i32()?;
        let vertices = [r.i32()?, r.i32()?];
        let reversed_3d = r.i32()? != 0;
        let kind = trim_type(r.i32()?);
        r.i32()?; // iso
        let face_loop = r.i32()?;
        r.skip(16)?; // tolerances
        if new_trims {
            r.skip(16 + 8 + 24)?; // domain and proxy flags
        } else {
            r.skip(48)?; // two points
        }
        r.skip(16)?; // legacy tolerances
        Ok(BrepTrim {
            edge,
            vertices,
            reversed_3d,
            kind,
            face_loop,
        })
    })?;
    let loops = topology(r, "the B-rep's loops", 16, |r| {
        r.i32()?;
        let trims = r.i32_array()?;
        let kind = loop_type(r.i32()?);
        let face = r.i32()?;
        Ok(BrepLoop { trims, kind, face })
    })?;
    let faces = topology(r, "the B-rep's faces", 20, |r| {
        r.i32()?;
        let loops = r.i32_array()?;
        let surface = r.i32()?;
        let reversed = r.i32()? != 0;
        r.i32()?; // material channel
        Ok(BrepFace {
            loops,
            surface,
            reversed,
        })
    })?;
    r.skip(48)?; // bounding box
    let mut brep = Brep {
        curves,
        surfaces,
        vertices,
        edges,
        trims,
        loops,
        faces,
        is_solid: 0,
        render_meshes: Vec::new(),
    };
    if minor >= 1 {
        // The faces' render meshes, then their analysis meshes.
        let mut meshes = r.expect_chunk(TCODE_ANONYMOUS_CHUNK, "the B-rep's render meshes")?;
        for _ in 0..brep.faces.len() {
            if meshes.data.at_end() {
                break;
            }
            if meshes.data.u8()? != 0 {
                let mesh = class_chunk(&mut meshes.data)?
                    .and_then(|(uuid, mut d)| (uuid == class::MESH).then(|| read_mesh(&mut d)));
                brep.render_meshes.push(mesh.transpose().ok().flatten());
            } else {
                brep.render_meshes.push(None);
            }
        }
        r.expect_chunk(TCODE_ANONYMOUS_CHUNK, "the B-rep's analysis meshes")?;
        if minor >= 2 && !r.at_end() {
            brep.is_solid = r.i32()?;
            // 3 and more: not known (as the format has it).
            if !(0..3).contains(&brep.is_solid) {
                brep.is_solid = 0;
            }
        }
    }
    Ok(brep)
}

/// A compressed buffer of `size` bytes: its CRC, a method byte and the
/// bytes, as they are (0) or deflated in an anonymous chunk (1).
fn compressed(r: &mut Reader, size: usize) -> Result<Vec<u8>> {
    if size == 0 {
        return Ok(Vec::new());
    }
    let crc = r.u32()?;
    let method = r.u8()?;
    let bytes = match method {
        0 => r.bytes(size)?.to_vec(),
        1 => {
            let mut c = r.expect_chunk(TCODE_ANONYMOUS_CHUNK, "a compressed buffer")?;
            let n = c.data.remaining();
            let deflated = c.data.bytes(n)?;
            miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(deflated, size).map_err(
                |e| {
                    crate::archive::Error::at(
                        "a compressed buffer is damaged",
                        format!("{:?}", e.status),
                    )
                },
            )?
        }
        m => {
            return err_at(
                "a compressed buffer of a method MitCad does not read",
                format!("method {m}"),
            );
        }
    };
    if bytes.len() != size {
        return err_at(
            "a compressed buffer of the wrong size",
            format!("{} bytes where {size} were expected", bytes.len()),
        );
    }
    if crate::archive::crc32(0, &bytes) != crc {
        return err("a compressed buffer is damaged (its CRC does not match)");
    }
    Ok(bytes)
}

pub fn read_mesh(r: &mut Reader) -> Result<Mesh> {
    let (major, _) = r.chunk_version()?;
    if major != 1 && major != 3 {
        return err_at(
            "a mesh of a version MitCad does not read",
            format!("version {major}"),
        );
    }
    let vcount = r.count(0)?;
    let fcount = r.count(0)?;
    r.skip(4 * 16 + 16)?; // texture and surface domains, surface scale
    r.skip(24 + 24 + 16)?; // float boxes of the vertices, normals, textures
    r.i32()?; // closed
    if r.u8()? != 0 {
        r.expect_chunk(TCODE_ANONYMOUS_CHUNK, "a mesh's parameters")?;
    }
    for _ in 0..4 {
        if r.u8()? != 0 {
            r.expect_chunk(TCODE_ANONYMOUS_CHUNK, "a mesh's curvature statistics")?;
        }
    }
    let index_size = r.i32()?;
    let mut faces = Vec::new();
    if fcount > 0 {
        let width = match index_size {
            1 | 2 | 4 => index_size as usize,
            s => {
                return err_at(
                    "a mesh whose vertex indices have a size MitCad does not read",
                    format!("{s}-byte vertex indices"),
                );
            }
        };
        if fcount.saturating_mul(4 * width) > r.remaining() {
            return err("a mesh whose faces run past its chunk");
        }
        faces.reserve_exact(fcount);
        for _ in 0..fcount {
            let f = match width {
                1 => {
                    let b = r.bytes(4)?;
                    [b[0], b[1], b[2], b[3]].map(u32::from)
                }
                2 => [r.u16()?, r.u16()?, r.u16()?, r.u16()?].map(u32::from),
                _ => [r.u32()?, r.u32()?, r.u32()?, r.u32()?],
            };
            faces.push(f);
        }
    }
    let mut vertices = Vec::new();
    if major == 1 {
        let n = r.count(12)?;
        vertices.reserve_exact(n);
        for _ in 0..n {
            vertices.push([
                f64::from(r.f32()?),
                f64::from(r.f32()?),
                f64::from(r.f32()?),
            ]);
        }
    } else if vcount > 0 {
        // The vertices as a compressed buffer of three floats each: its
        // size, at most 12 bytes per byte of the file left, is the limit
        // of what is inflated.
        let size = r.u32()? as usize;
        if size != 0 && Some(size) != vcount.checked_mul(12) {
            return err("a mesh whose vertex buffer has the wrong size");
        }
        let bytes = compressed(r, size)?;
        let (chunks, _) = bytes.as_chunks::<12>();
        vertices.reserve_exact(chunks.len());
        for c in chunks {
            let f = |k: usize| f64::from(f32::from_le_bytes([c[k], c[k + 1], c[k + 2], c[k + 3]]));
            vertices.push([f(0), f(4), f(8)]);
        }
    }
    Ok(Mesh { vertices, faces })
}

fn read_extrusion(r: &mut Reader) -> Result<Extrusion> {
    let (major, minor, mut d) = r.versioned_chunk("an extrusion")?;
    if major != 1 {
        return err_at(
            "an extrusion of a version MitCad does not read",
            format!("version {major}"),
        );
    }
    let Some(profile) = read_curve_object(&mut d)? else {
        return err("an extrusion without its profile");
    };
    let path = [d.point()?, d.point()?];
    let t = d.interval()?;
    let up = d.point()?;
    let have = [d.bool()?, d.bool()?];
    let n = [d.point()?, d.point()?];
    d.interval()?; // the path's domain
    d.bool()?; // transposed
    let mut profile_count = 1;
    if minor >= 1 {
        profile_count = usize::try_from(d.i32()?).unwrap_or(1).max(1);
    }
    let cap = if minor >= 2 {
        [d.bool()?, d.bool()?]
    } else {
        let closed = crate::convert::curve_is_closed(&profile);
        [closed, closed]
    };
    let mitered = |k: usize| {
        let v = n[k];
        (have[k] && v[2] > 1e-6 && (v[0] != 0.0 || v[1] != 0.0)).then_some(v)
    };
    Ok(Extrusion {
        profile,
        profile_count,
        path,
        t,
        up,
        miter: [mitered(0), mitered(1)],
        cap,
    })
}

fn read_instance_ref(r: &mut Reader) -> Result<InstanceRef> {
    let (major, _) = r.chunk_version()?;
    if major != 1 {
        return err_at(
            "a block instance of a version MitCad does not read",
            format!("version {major}"),
        );
    }
    let definition = r.uuid()?;
    let mut xform = [[0.0; 4]; 4];
    for row in &mut xform {
        for v in row.iter_mut() {
            *v = r.f64()?;
        }
    }
    Ok(InstanceRef { definition, xform })
}

/// An object of the object table, by its class.
pub fn read_geometry(uuid: Uuid, r: &mut Reader) -> Result<Geometry> {
    use class::*;
    Ok(if uuid == BREP || uuid == BREP_TL {
        Geometry::Brep(Box::new(read_brep(r)?))
    } else if uuid == EXTRUSION {
        Geometry::Extrusion(Box::new(read_extrusion(r)?))
    } else if uuid == MESH {
        Geometry::Mesh(Box::new(read_mesh(r)?))
    } else if uuid == INSTANCE_REF {
        Geometry::Instance(read_instance_ref(r)?)
    } else {
        match read_surface(uuid, r)? {
            Surface::Other(_) => match read_curve(uuid, r)? {
                Curve::Other(name) => Geometry::Other(name),
                c => Geometry::Curve(Box::new(c)),
            },
            s => Geometry::Surface(Box::new(s)),
        }
    })
}
