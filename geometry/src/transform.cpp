// SPDX-License-Identifier: MIT
#include "mitcad/geometry/transform.hpp"

#include <cmath>
#include <memory>
#include <optional>
#include <stdexcept>
#include <vector>

#include <BRepBuilderAPI_GTransform.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRep_Tool.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Iterator.hxx>
#include <gp_GTrsf.hxx>
#include <gp_Mat.hxx>
#include <gp_Trsf.hxx>

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

// Whether the shape is valid as OCCT's checker sees it, checked once per
// shape: a similarity moves its geometry exactly and keeps a valid shape
// valid, so the copies of a pattern need not be checked one by one (the
// check was four fifths of their time).
bool valid_once(const Shape& shape) {
  if (const std::optional<bool> known = shape.checked_valid()) {
    return *known;
  }
  const bool valid = detail::is_valid(shape.occt());
  shape.set_checked_valid(valid);
  return valid;
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

} // namespace

ShapePtr transform_shape(const Shape& shape, const Affine& map, const std::string& rename) {
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
    const bool known_valid = similarity && !mesh && valid_once(shape);
    if (known_valid && is_identity(map)) {
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
      return std::make_shared<Shape>(shape.occt(), faces);
    }
    std::unique_ptr<BRepBuilderAPI_ModifyShape> operation;
    if (similarity) {
      gp_Trsf trsf;
      trsf.SetValues(m[0][0], m[0][1], m[0][2], t[0], m[1][0], m[1][1], m[1][2], t[1], m[2][0],
                     m[2][1], m[2][2], t[2]);
      operation = std::make_unique<BRepBuilderAPI_Transform>(shape.occt(), trsf, true, mesh);
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
    namer.finish();
    if (!known_valid) {
      require_valid_brep(namer.result());
    }
    return namer.shape();
  });
}

} // namespace mitcad::geometry
