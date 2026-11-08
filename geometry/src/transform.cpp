// SPDX-License-Identifier: MIT
#include "mitcad/geometry/transform.hpp"

#include <algorithm>
#include <cmath>
#include <memory>
#include <optional>
#include <stdexcept>
#include <utility>
#include <vector>

#include <BRepBuilderAPI_GTransform.hxx>
#include <BRepBuilderAPI_ModifyShape.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepTools_Modification.hxx>
#include <BRep_Tool.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_BezierCurve.hxx>
#include <Geom_BezierSurface.hxx>
#include <Geom_Circle.hxx>
#include <Geom_Ellipse.hxx>
#include <Geom_Line.hxx>
#include <Geom_Plane.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_SphericalSurface.hxx>
#include <Geom_SurfaceOfRevolution.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_DataMap.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_GTrsf.hxx>
#include <gp_Mat.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>
#include <gp_Vec2d.hxx>
#include <gp_XYZ.hxx>

#include "history.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

// Relative tolerance of the similarity check.
constexpr double kSimilar = 1.0e-9;

// True when `linear` is a rotation or a mirror times a uniform scale, which
// gp_Trsf represents.
bool is_similarity(const Affine& map) {
  const auto& m = map.linear;
  double product[3][3];
  for (int r = 0; r < 3; ++r) {
    for (int c = 0; c < 3; ++c) {
      product[r][c] = 0.0;
      for (int k = 0; k < 3; ++k) {
        product[r][c] += m[static_cast<std::size_t>(k)][static_cast<std::size_t>(r)] *
                         m[static_cast<std::size_t>(k)][static_cast<std::size_t>(c)];
      }
    }
  }
  const double scale = product[0][0];
  if (!(scale > 0.0)) {
    return false;
  }
  for (int r = 0; r < 3; ++r) {
    for (int c = 0; c < 3; ++c) {
      const double expected = r == c ? scale : 0.0;
      if (std::abs(product[r][c] - expected) > kSimilar * scale) {
        return false;
      }
    }
  }
  return true;
}

double determinant(const Affine& map) {
  const auto& m = map.linear;
  return m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) -
         m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) +
         m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
}

// True when a face of the shape has no surface: the triangles of a mesh
// body (STL, OBJ).
bool has_mesh_faces(const TopoDS_Shape& shape) {
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    TopLoc_Location location;
    if (BRep_Tool::Surface(TopoDS::Face(it.Current()), location).IsNull()) {
      return true;
    }
  }
  return false;
}

bool is_identity(const Affine& map) {
  for (std::size_t r = 0; r < 3; ++r) {
    for (std::size_t c = 0; c < 3; ++c) {
      if (map.linear[r][c] != (r == c ? 1.0 : 0.0)) {
        return false;
      }
    }
    if (map.translation[r] != 0.0) {
      return false;
    }
  }
  return true;
}

// OCCT's checker on the moved shape where it is a B-rep: it reports a face
// without a surface as BRepCheck_NoSurface, and a mesh body's triangles
// have nothing else for it to check. A compound's parts are checked one by
// one, so B-rep bodies next to a mesh still are.
void require_valid_brep(const TopoDS_Shape& shape) {
  if (!has_mesh_faces(shape)) {
    detail::require_valid(shape, "the transform");
  } else if (shape.ShapeType() == TopAbs_COMPOUND) {
    for (TopoDS_Iterator it(shape); it.More(); it.Next()) {
      require_valid_brep(it.Value());
    }
  }
}

// True when `map` is a rotation (no mirror) without a scale: a motion a
// location can carry (OCCT refuses locations that scale).
bool is_rigid(const Affine& map) {
  if (!is_similarity(map) || !(determinant(map) > 0.0)) {
    return false;
  }
  double column = 0.0;
  for (const auto& row : map.linear) {
    column += row[0] * row[0];
  }
  return std::abs(column - 1.0) <= kSimilar;
}

// Exact images of a general affine map (mitcad#1504). OCCT's
// BRepBuilderAPI_GTransform converts every face and edge to a B-spline
// first, a sphere's seam p-curves by an approximate projection, and moves
// the poles; a sphere flattened along its axis by 0.1 or 0.15 came out with
// a face the checker found unorientable, and the boolean of a plate with
// many such B-spline faces ended alike after 2.5 GB. A sphere whose axes the
// map keeps at right angles, the two about the axis equally long, is mapped
// to the ellipsoid of revolution it is, with the same parameters (its
// p-curves stay as they are, moved along v into the meridian's range), as
// are the edges' circles (to circles or ellipses), lines that keep their
// length, isometric planes and B-splines (their poles). A shape with a
// sphere whose faces and edges all map so is mapped exactly; any other goes
// through BRepBuilderAPI_GTransform as before.

// Relative tolerance of "at right angles" and "as long" for the exact
// images: rounding only.
constexpr double kExactImage = 1.0e-12;
constexpr double kHalfTurn = 3.14159265358979323846;

// A general affine map applied to points and vectors.
class AffineImage {
public:
  explicit AffineImage(const Affine& map) {
    const auto& m = map.linear;
    m_linear = gp_Mat(m[0][0], m[0][1], m[0][2], m[1][0], m[1][1], m[1][2], m[2][0], m[2][1], m[2][2]);
    m_translation = gp_XYZ(map.translation[0], map.translation[1], map.translation[2]);
    // The Frobenius norm bounds how much the map lengthens any distance:
    // tolerances grow by it (never shrink).
    double squares = 0.0;
    for (const auto& row : m) {
      for (double value : row) {
        squares += value * value;
      }
    }
    m_stretch = std::max(1.0, std::sqrt(squares));
    m_mirror = determinant(map) < 0.0;
  }

  gp_Pnt point(const gp_Pnt& p) const {
    gp_XYZ xyz = p.XYZ();
    xyz.Multiply(m_linear);
    xyz.Add(m_translation);
    return gp_Pnt(xyz);
  }
  gp_Vec vector(const gp_Vec& v) const {
    gp_XYZ xyz = v.XYZ();
    xyz.Multiply(m_linear);
    return gp_Vec(xyz);
  }
  double stretch() const { return m_stretch; }
  bool mirror() const { return m_mirror; }

private:
  gp_Mat m_linear;
  gp_XYZ m_translation;
  double m_stretch = 1.0;
  bool m_mirror = false;
};

bool at_right_angles(const gp_Vec& a, const gp_Vec& b) {
  return std::abs(a.Dot(b)) <= kExactImage * a.Magnitude() * b.Magnitude();
}

bool as_long(double a, double b) { return std::abs(a - b) <= kExactImage * std::max(a, b); }

// The image of a conic centre + r1 cos(t) x + r2 sin(t) y with the same
// parameter: a circle, or an ellipse whose major axis is x's image. None
// when the images of x and y are not at right angles, or y's is the longer
// (the major axis would shift the parameter by a quarter turn).
occ::handle<Geom_Curve> mapped_conic(const AffineImage& map, const gp_Ax2& frame, double r1, double r2) {
  const gp_Vec x = map.vector(gp_Vec(frame.XDirection())) * r1;
  const gp_Vec y = map.vector(gp_Vec(frame.YDirection())) * r2;
  const double lx = x.Magnitude();
  const double ly = y.Magnitude();
  if (!(lx > 0.0) || !(ly > 0.0) || !at_right_angles(x, y)) {
    return {};
  }
  const gp_Ax2 axes(map.point(frame.Location()), gp_Dir(x.Crossed(y)), gp_Dir(x));
  if (as_long(lx, ly)) {
    return occ::handle<Geom_Curve>(new Geom_Circle(axes, lx));
  }
  if (lx > ly) {
    return occ::handle<Geom_Curve>(new Geom_Ellipse(axes, lx, ly));
  }
  return {};
}

// The exact image of a curve with the same parameter, or none.
occ::handle<Geom_Curve> mapped_curve(const occ::handle<Geom_Curve>& curve, const AffineImage& map) {
  if (curve.IsNull()) {
    return {};
  }
  if (const occ::handle<Geom_TrimmedCurve> trimmed = occ::down_cast<Geom_TrimmedCurve>(curve); !trimmed.IsNull()) {
    const occ::handle<Geom_Curve> basis = mapped_curve(trimmed->BasisCurve(), map);
    if (basis.IsNull()) {
      return {};
    }
    return occ::handle<Geom_Curve>(
        new Geom_TrimmedCurve(basis, trimmed->FirstParameter(), trimmed->LastParameter(), true, false));
  }
  if (const occ::handle<Geom_BSplineCurve> spline = occ::down_cast<Geom_BSplineCurve>(curve); !spline.IsNull()) {
    const occ::handle<Geom_BSplineCurve> copy = occ::down_cast<Geom_BSplineCurve>(spline->Copy());
    for (int i = 1; i <= copy->NbPoles(); ++i) {
      copy->SetPole(i, map.point(copy->Pole(i)));
    }
    return copy;
  }
  if (const occ::handle<Geom_BezierCurve> bezier = occ::down_cast<Geom_BezierCurve>(curve); !bezier.IsNull()) {
    const occ::handle<Geom_BezierCurve> copy = occ::down_cast<Geom_BezierCurve>(bezier->Copy());
    for (int i = 1; i <= copy->NbPoles(); ++i) {
      copy->SetPole(i, map.point(copy->Pole(i)));
    }
    return copy;
  }
  if (const occ::handle<Geom_Line> line = occ::down_cast<Geom_Line>(curve); !line.IsNull()) {
    // A line's parameter is its length: the same only where the map keeps
    // the line's length.
    const gp_Vec direction = map.vector(gp_Vec(line->Position().Direction()));
    if (!as_long(direction.Magnitude(), 1.0)) {
      return {};
    }
    return occ::handle<Geom_Curve>(new Geom_Line(map.point(line->Position().Location()), gp_Dir(direction)));
  }
  if (const occ::handle<Geom_Circle> circle = occ::down_cast<Geom_Circle>(curve); !circle.IsNull()) {
    return mapped_conic(map, circle->Position(), circle->Radius(), circle->Radius());
  }
  if (const occ::handle<Geom_Ellipse> ellipse = occ::down_cast<Geom_Ellipse>(curve); !ellipse.IsNull()) {
    return mapped_conic(map, ellipse->Position(), ellipse->MajorRadius(), ellipse->MinorRadius());
  }
  return {};
}

// The exact image of a surface with the same parameters, but for v moved by
// `v_shift`, or none.
struct SurfaceImage {
  occ::handle<Geom_Surface> surface;
  double v_shift = 0.0;
};

// A sphere centre + r (cos(v) (cos(u) x + sin(u) y) + sin(v) z) whose x, y
// and z the map keeps at right angles, x and y as long (a): the surface of
// revolution about x' × y' of the meridian centre' + r (a cos(v) x^ + c
// sin(v) z^) (c the length of z's image). Its meridian is half of an
// ellipse, trimmed within the ellipse's period, so the surface's v is the
// sphere's moved by half a turn (by a quarter where z's image is the longer).
std::optional<SurfaceImage> mapped_sphere(const Geom_SphericalSurface& sphere, const AffineImage& map) {
  const gp_Ax3& frame = sphere.Position();
  const double r = sphere.Radius();
  const gp_Vec x = map.vector(gp_Vec(frame.XDirection()));
  const gp_Vec y = map.vector(gp_Vec(frame.YDirection()));
  const gp_Vec z = map.vector(gp_Vec(frame.Direction()));
  const double a = x.Magnitude();
  const double c = z.Magnitude();
  if (!at_right_angles(x, y) || !at_right_angles(x, z) || !at_right_angles(y, z) || !as_long(a, y.Magnitude()) ||
      as_long(a, c)) {
    return std::nullopt;
  }
  const gp_Pnt centre = map.point(frame.Location());
  const gp_Dir x_dir(x);
  const gp_Dir z_dir(z);
  // The meridian's plane holds x' and z'; its ellipse's y axis is -z' (x'
  // the major axis) or x' (z' the major axis).
  const gp_Dir normal(x.Crossed(z));
  occ::handle<Geom_Ellipse> ellipse;
  double first = 0.0;
  SurfaceImage image;
  if (c < a) {
    // t = v + π: centre + a r cos(t) (-x') + c r sin(t) (-z').
    ellipse = new Geom_Ellipse(gp_Ax2(centre, normal, x_dir.Reversed()), a * r, c * r);
    first = kHalfTurn / 2;
    image.v_shift = kHalfTurn;
  } else {
    // t = v + π/2: centre + c r cos(t) (-z') + a r sin(t) x'.
    ellipse = new Geom_Ellipse(gp_Ax2(centre, normal, z_dir.Reversed()), c * r, a * r);
    first = 0.0;
    image.v_shift = kHalfTurn / 2;
  }
  const occ::handle<Geom_TrimmedCurve> meridian = new Geom_TrimmedCurve(ellipse, first, first + kHalfTurn, true, false);
  image.surface = new Geom_SurfaceOfRevolution(meridian, gp_Ax1(centre, gp_Dir(x.Crossed(y))));
  return image;
}

std::optional<SurfaceImage> mapped_surface(const occ::handle<Geom_Surface>& surface, const AffineImage& map) {
  if (surface.IsNull()) {
    return std::nullopt;
  }
  if (const occ::handle<Geom_RectangularTrimmedSurface> trimmed =
          occ::down_cast<Geom_RectangularTrimmedSurface>(surface);
      !trimmed.IsNull()) {
    const std::optional<SurfaceImage> basis = mapped_surface(trimmed->BasisSurface(), map);
    if (!basis || basis->v_shift != 0.0) {
      return std::nullopt;
    }
    double u1 = 0.0;
    double u2 = 0.0;
    double v1 = 0.0;
    double v2 = 0.0;
    trimmed->Bounds(u1, u2, v1, v2);
    SurfaceImage image;
    image.surface = new Geom_RectangularTrimmedSurface(basis->surface, u1, u2, v1, v2);
    return image;
  }
  if (const occ::handle<Geom_BSplineSurface> spline = occ::down_cast<Geom_BSplineSurface>(surface);
      !spline.IsNull()) {
    const occ::handle<Geom_BSplineSurface> copy = occ::down_cast<Geom_BSplineSurface>(spline->Copy());
    for (int i = 1; i <= copy->NbUPoles(); ++i) {
      for (int j = 1; j <= copy->NbVPoles(); ++j) {
        copy->SetPole(i, j, map.point(copy->Pole(i, j)));
      }
    }
    SurfaceImage image;
    image.surface = copy;
    return image;
  }
  if (const occ::handle<Geom_BezierSurface> bezier = occ::down_cast<Geom_BezierSurface>(surface);
      !bezier.IsNull()) {
    const occ::handle<Geom_BezierSurface> copy = occ::down_cast<Geom_BezierSurface>(bezier->Copy());
    for (int i = 1; i <= copy->NbUPoles(); ++i) {
      for (int j = 1; j <= copy->NbVPoles(); ++j) {
        copy->SetPole(i, j, map.point(copy->Pole(i, j)));
      }
    }
    SurfaceImage image;
    image.surface = copy;
    return image;
  }
  if (const occ::handle<Geom_Plane> plane = occ::down_cast<Geom_Plane>(surface); !plane.IsNull()) {
    // A plane's parameters are lengths: the same only where the map keeps
    // the plane's lengths and right angle.
    const gp_Ax3& frame = plane->Position();
    const gp_Vec x = map.vector(gp_Vec(frame.XDirection()));
    const gp_Vec y = map.vector(gp_Vec(frame.YDirection()));
    if (!as_long(x.Magnitude(), 1.0) || !as_long(y.Magnitude(), 1.0) || !at_right_angles(x, y)) {
      return std::nullopt;
    }
    SurfaceImage image;
    image.surface = new Geom_Plane(gp_Ax3(map.point(frame.Location()), gp_Dir(x.Crossed(y)), gp_Dir(x)));
    return image;
  }
  if (const occ::handle<Geom_SphericalSurface> sphere = occ::down_cast<Geom_SphericalSurface>(surface);
      !sphere.IsNull()) {
    return mapped_sphere(*sphere, map);
  }
  return std::nullopt;
}

// The exact images of a shape's faces, edges and vertices, for
// BRepTools_Modifier.
class ExactImage : public BRepTools_Modification {
public:
  explicit ExactImage(const AffineImage& map) : m_map(map) {}

  // The images of every face and edge of `shape`: true when each has one
  // and a face is a sphere's (the only surface GTransform cannot map
  // exactly that this maps).
  bool prepare(const TopoDS_Shape& shape) {
    bool sphere = false;
    try {
      for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
        const TopoDS_Face& face = TopoDS::Face(it.Current());
        if (m_faces.IsBound(face)) {
          continue;
        }
        TopLoc_Location location;
        const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face, location);
        if (surface.IsNull()) {
          return false;
        }
        sphere = sphere || surface->IsKind(STANDARD_TYPE(Geom_SphericalSurface));
        const std::optional<SurfaceImage> image = mapped_surface(
            occ::down_cast<Geom_Surface>(surface->Transformed(location.Transformation())), m_map);
        if (!image) {
          return false;
        }
        m_faces.Bind(face, *image);
      }
      for (TopExp_Explorer it(shape, TopAbs_EDGE); it.More(); it.Next()) {
        const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
        if (BRep_Tool::Degenerated(edge) || m_curves.IsBound(edge)) {
          continue;
        }
        TopLoc_Location location;
        double first = 0.0;
        double last = 0.0;
        const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, location, first, last);
        if (curve.IsNull()) {
          return false;
        }
        const occ::handle<Geom_Curve> image =
            mapped_curve(occ::down_cast<Geom_Curve>(curve->Transformed(location.Transformation())), m_map);
        if (image.IsNull()) {
          return false;
        }
        // Over the edge's own range (a periodic curve's unmoved).
        m_curves.Bind(edge, occ::handle<Geom_Curve>(new Geom_TrimmedCurve(image, first, last, true, false)));
      }
    } catch (const Standard_Failure&) {
      return false;
    }
    return sphere;
  }

  bool NewSurface(const TopoDS_Face& F, occ::handle<Geom_Surface>& S, TopLoc_Location& L, double& Tol,
                  bool& RevWires, bool& RevFace) override {
    const SurfaceImage* image = m_faces.Seek(F);
    if (image == nullptr) {
      return false;
    }
    S = image->surface;
    L = TopLoc_Location();
    Tol = BRep_Tool::Tolerance(F) * m_map.stretch();
    RevWires = false;
    // The same parameters: a mirror turns the surface's normal inward.
    RevFace = m_map.mirror();
    return true;
  }

  bool NewCurve(const TopoDS_Edge& E, occ::handle<Geom_Curve>& C, TopLoc_Location& L, double& Tol) override {
    L = TopLoc_Location();
    Tol = BRep_Tool::Tolerance(E) * m_map.stretch();
    if (BRep_Tool::Degenerated(E)) {
      C.Nullify();
      return true;
    }
    const occ::handle<Geom_Curve>* image = m_curves.Seek(E);
    if (image == nullptr) {
      return false;
    }
    C = *image;
    return true;
  }

  bool NewPoint(const TopoDS_Vertex& V, gp_Pnt& P, double& Tol) override {
    P = m_map.point(BRep_Tool::Pnt(V));
    Tol = BRep_Tool::Tolerance(V) * m_map.stretch();
    return true;
  }

  bool NewCurve2d(const TopoDS_Edge& E, const TopoDS_Face& F, const TopoDS_Edge&, const TopoDS_Face&,
                  occ::handle<Geom2d_Curve>& C, double& Tol) override {
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(E, F, first, last);
    if (pcurve.IsNull()) {
      return false;
    }
    C = occ::down_cast<Geom2d_Curve>(pcurve->Copy());
    const SurfaceImage* image = m_faces.Seek(F);
    if (image != nullptr && image->v_shift != 0.0) {
      C->Translate(gp_Vec2d(0.0, image->v_shift));
    }
    Tol = BRep_Tool::Tolerance(E) * m_map.stretch();
    return true;
  }

  bool NewParameter(const TopoDS_Vertex& V, const TopoDS_Edge& E, double& P, double& Tol) override {
    if (BRep_Tool::Degenerated(E)) {
      return false;
    }
    P = BRep_Tool::Parameter(V, E);
    Tol = BRep_Tool::Tolerance(V) * m_map.stretch();
    return true;
  }

  GeomAbs_Shape Continuity(const TopoDS_Edge& E, const TopoDS_Face& F1, const TopoDS_Face& F2, const TopoDS_Edge&,
                           const TopoDS_Face&, const TopoDS_Face&) override {
    return BRep_Tool::Continuity(E, F1, F2);
  }

private:
  AffineImage m_map;
  NCollection_DataMap<TopoDS_Shape, SurfaceImage, TopTools_ShapeMapHasher> m_faces;
  NCollection_DataMap<TopoDS_Shape, occ::handle<Geom_Curve>, TopTools_ShapeMapHasher> m_curves;
};

// BRepBuilderAPI_ModifyShape's history over ExactImage's modification.
class ExactTransform final : public BRepBuilderAPI_ModifyShape {
public:
  ExactTransform(const TopoDS_Shape& shape, const occ::handle<BRepTools_Modification>& modification)
      : BRepBuilderAPI_ModifyShape(modification) {
    DoModif(shape);
  }
};

// transform_shape and move_shape: `by_location` moves a rigid motion of a
// B-rep as a location on the same sub-shapes instead of a copy.
ShapePtr mapped(const Shape& shape, const Affine& map, const std::string& rename, bool by_location) {
  for (const auto& row : map.linear) {
    for (double value : row) {
      detail::require_finite("a transform", value);
    }
  }
  for (double value : map.translation) {
    detail::require_finite("a transform", value);
  }
  if (std::abs(determinant(map)) < 1.0e-12) {
    throw std::invalid_argument("the transform flattens the shape");
  }
  return detail::run("transform", [&] {
    const auto& m = map.linear;
    const auto& t = map.translation;
    // A face without a surface is only its triangulation: a rigid motion
    // or uniform scale moves it only with a copy of the triangulation
    // (otherwise the face stays where it was). GTransform always copies.
    const bool mesh = has_mesh_faces(shape.occt());
    const bool similarity = is_similarity(map);
    if (similarity && !mesh && is_identity(map)) {
      // Only the names change (a tool a pattern rebuilt in its place): the
      // same B-rep, as results share their unchanged parts with inputs.
      std::vector<Shape::NamedFace> faces;
      for (int i = 0; i < shape.face_count(); ++i) {
        NameList names;
        for (const std::string& name : shape.face_names(i)) {
          names.push_back(rename.empty() ? name : rename + '(' + name + ')');
        }
        if (!names.empty()) {
          faces.push_back({shape.face(i), names});
        }
      }
      auto result = std::make_shared<Shape>(shape.occt(), faces);
      if (shape.has_open_edge_names()) {
        std::vector<Shape::NamedOpenEdge> edges;
        for (int i = 0; i < shape.edge_count(); ++i) {
          NameList names = shape.open_edge_names(i);
          for (std::string& name : names) name = rename_open_edge_name(name, rename);
          if (!names.empty()) edges.push_back({shape.edge(i), std::move(names)});
        }
        result->set_open_edge_names(edges);
      }
      return result;
    }
    std::unique_ptr<BRepBuilderAPI_ModifyShape> operation;
    if (similarity) {
      gp_Trsf trsf;
      trsf.SetValues(m[0][0], m[0][1], m[0][2], t[0], m[1][0], m[1][1], m[1][2], t[1], m[2][0],
                     m[2][1], m[2][2], t[2]);
      // A rigid motion of a B-rep by location (move_shape, mitcad#838): the
      // result is the input's sub-shapes with the motion as their location,
      // so a face keeps its surface and the triangulation it has, and the
      // view draws the moved body from the meshes it has. Nothing of the
      // input is written. Otherwise a moved copy.
      const bool located = by_location && !mesh && is_rigid(map);
      operation = std::make_unique<BRepBuilderAPI_Transform>(shape.occt(), trsf, !located, mesh);
    } else if (const occ::handle<ExactImage> exact = new ExactImage(AffineImage(map));
               !mesh && exact->prepare(shape.occt())) {
      // A sphere's body mapped exactly (mitcad#1504).
      operation = std::make_unique<ExactTransform>(shape.occt(), exact);
    } else {
      gp_GTrsf trsf;
      trsf.SetVectorialPart(
          gp_Mat(m[0][0], m[0][1], m[0][2], m[1][0], m[1][1], m[1][2], m[2][0], m[2][1], m[2][2]));
      trsf.SetTranslationPart(gp_XYZ(t[0], t[1], t[2]));
      operation = std::make_unique<BRepBuilderAPI_GTransform>(shape.occt(), trsf, true);
    }
    if (!operation->IsDone()) {
      throw std::runtime_error("the shape could not be transformed");
    }
    detail::FaceNamer namer(operation->Shape());
    for (int i = 0; i < shape.face_count(); ++i) {
      const TopoDS_Shape image = operation->ModifiedShape(shape.face(i));
      for (const std::string& name : shape.face_names(i)) {
        namer.add(image, rename.empty() ? name : rename + '(' + name + ')');
      }
    }
    namer.carry_open_edges(*operation, shape, nullptr, rename);
    namer.finish();
    // A similarity moves the geometry exactly and keeps the shape as valid
    // as it was, so its result is not checked (the copies of a pattern
    // need not be checked one by one: the check was four fifths of their
    // time). A shape OCCT's checker finds invalid (an imported body whose
    // shell is not connected) is moved as it is, not refused (mitcad#537:
    // Reorient Design's Move of such a body and its occurrences'
    // placements failed). Only a general affine map can break a shape.
    if (!similarity) {
      require_valid_brep(namer.result());
    } else if (const std::optional<bool> valid = shape.checked_valid()) {
      namer.set_valid(*valid);
    }
    return namer.shape();
  });
}

} // namespace

ShapePtr transform_shape(const Shape& shape, const Affine& map, const std::string& rename) {
  return mapped(shape, map, rename, false);
}

ShapePtr move_shape(const Shape& shape, const Affine& map, const std::string& rename) {
  return mapped(shape, map, rename, true);
}

} // namespace mitcad::geometry
