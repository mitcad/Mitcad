// SPDX-License-Identifier: MIT
#include "mitcad/geometry/removed.hpp"

#include <algorithm>
#include <cmath>
#include <cstdlib>
#include <cstddef>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepTools.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <Precision.hxx>
#include <ShapeAnalysis_Surface.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>

#include "mitcad/geometry/cancel.hpp"
#include "mitcad/geometry/query.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

// Points asked about per face, spread over it.
constexpr std::size_t kPoints = 9;

// A region's box grows by this share of its diagonal, at least by
// kMinMargin mm, on every side; on later attempts by twice and four times
// that.
constexpr double kMargin = 0.5;
constexpr double kMinMargin = 0.5;
constexpr int kAttempts = 3;

// The boxes together take at most this share of `before`'s box; beyond it
// the whole bodies are cut (cutting them down gains little).
constexpr double kMaxShare = 0.5;

// `after` is cut down to the boxes grown by this much more (mm), so that
// no side of its boxes meets one of `before`'s.
constexpr double kApart = 1.0e-2;

// A piece this near a side of its box (mm) reaches it.
constexpr double kSide = 1.0e-4;

// Pieces that count (removed_material.hpp).
constexpr double kSliverShare = 1.0e-3;
constexpr double kSliverVolume = 1.0e-9;

// The box grown by `by` on every side, without a gap (Bnd_Box::Enlarge
// widens the gap to at least its value, it does not add to it).
Bnd_Box inflated(const Bnd_Box& box, double by) {
  const gp_Pnt low = box.CornerMin();
  const gp_Pnt high = box.CornerMax();
  Bnd_Box out;
  out.Update(low.X() - by, low.Y() - by, low.Z() - by, high.X() + by, high.Y() + by, high.Z() + by);
  return out;
}

// Points inside the face: of a coarse grid over its parameter range, of a
// fine one for a narrow face; at most kPoints, spread over the grid.
std::vector<gp_Pnt> sample(const TopoDS_Face& face) {
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  BRepTopAdaptor_FClass2d classifier(face, Precision::PConfusion());
  std::vector<gp_Pnt2d> inside;
  for (const int cells : {5, 24}) {
    for (int a = 0; a < cells; ++a) {
      for (int b = 0; b < cells; ++b) {
        const gp_Pnt2d uv(u0 + (u1 - u0) * (a + 0.5) / cells, v0 + (v1 - v0) * (b + 0.5) / cells);
        if (classifier.Perform(uv) == TopAbs_IN) {
          inside.push_back(uv);
        }
      }
    }
    if (!inside.empty()) {
      break;
    }
  }
  std::vector<gp_Pnt> points;
  const BRepAdaptor_Surface surface(face);
  const std::size_t wanted = std::min(inside.size(), kPoints);
  for (std::size_t k = 0; k < wanted; ++k) {
    const gp_Pnt2d& uv = inside[k * inside.size() / wanted];
    points.push_back(surface.Value(uv.X(), uv.Y()));
  }
  return points;
}

// The faces of a body, to ask whether points lie within the slack of one
// of them. A point is measured only against the faces whose boxes (grown
// by the slack) hold it: projected onto the face's surface and classified
// in its parameter space; a point that projects outside the face (near its
// edges) is measured against the face itself.
class Near {
public:
  Near(const Shape& body, double slack) : m_slack(slack) {
    for (int i = 0; i < body.face_count(); ++i) {
      Entry entry;
      entry.face = body.face(i);
      Bnd_Box box;
      BRepBndLib::Add(entry.face, box);
      if (box.IsVoid()) {
        continue;
      }
      entry.box = inflated(box, slack);
      m_faces.push_back(std::move(entry));
    }
  }

  bool operator()(const gp_Pnt& p) {
    for (Entry& entry : m_faces) {
      if (entry.box.IsOut(p)) {
        continue;
      }
      if (entry.surface.IsNull()) {
        entry.surface = new ShapeAnalysis_Surface(BRep_Tool::Surface(entry.face));
        entry.inside = std::make_unique<BRepTopAdaptor_FClass2d>(entry.face, Precision::Confusion());
      }
      const gp_Pnt2d uv = entry.surface->ValueOfUV(p, Precision::Confusion());
      if (p.Distance(entry.surface->Value(uv)) > m_slack) {
        continue;
      }
      if (entry.inside->Perform(uv) != TopAbs_OUT) {
        return true;
      }
      BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(p).Vertex(), entry.face);
      if (distance.IsDone() && distance.Value() <= m_slack) {
        return true;
      }
    }
    return false;
  }

private:
  struct Entry {
    TopoDS_Face face;
    Bnd_Box box;
    Handle(ShapeAnalysis_Surface) surface;
    std::unique_ptr<BRepTopAdaptor_FClass2d> inside;
  };
  std::vector<Entry> m_faces;
  double m_slack;
};

// The faces of `of` with a point farther than `slack` from the faces of
// `from` (with `every`: with every point that far, a face gone).
std::vector<int> far_faces(const Shape& of, const Shape& from, double slack, bool every) {
  std::vector<int> out;
  Near near_from(from, slack);
  for (int i = 0; i < of.face_count(); ++i) {
    throw_if_cancelled();
    const std::vector<gp_Pnt> points = sample(of.face(i));
    const bool far = every ? !points.empty() && std::none_of(points.begin(), points.end(),
                                                             [&](const gp_Pnt& p) { return near_from(p); })
                           : std::any_of(points.begin(), points.end(), [&](const gp_Pnt& p) { return !near_from(p); });
    if (far) {
      out.push_back(i);
    }
  }
  return out;
}

// Adds the boxes of the faces to `regions`.
void add_boxes(const Shape& body, const std::vector<int>& faces, std::vector<Bnd_Box>& regions) {
  for (const int i : faces) {
    Bnd_Box box;
    BRepBndLib::Add(body.face(i), box);
    if (!box.IsVoid()) {
      regions.push_back(inflated(box, 0.0));
    }
  }
}

// The boxes of the faces where the bodies differ (removed.hpp).
std::vector<Bnd_Box> changed_regions(const Shape& before, const Shape& after, double slack) {
  std::vector<Bnd_Box> regions;
  add_boxes(after, far_faces(after, before, slack, false), regions);
  add_boxes(before, far_faces(before, after, slack, true), regions);
  return regions;
}

double box_volume(const Bnd_Box& box) {
  const gp_XYZ size = box.CornerMax().XYZ() - box.CornerMin().XYZ();
  return size.X() * size.Y() * size.Z();
}

// The boxes grown, those that come within `apart` of each other united,
// until none do.
std::vector<Bnd_Box> grown(const std::vector<Bnd_Box>& regions, double factor, double apart) {
  std::vector<Bnd_Box> boxes;
  for (const Bnd_Box& box : regions) {
    boxes.push_back(inflated(box, factor * std::max(kMinMargin, kMargin * std::sqrt(box.SquareExtent()))));
  }
  for (bool joined = true; joined;) {
    joined = false;
    for (std::size_t i = 0; i < boxes.size() && !joined; ++i) {
      const Bnd_Box near = inflated(boxes[i], apart);
      for (std::size_t j = i + 1; j < boxes.size() && !joined; ++j) {
        if (!near.IsOut(boxes[j])) {
          boxes[i].Add(boxes[j]);
          boxes.erase(boxes.begin() + static_cast<std::ptrdiff_t>(j));
          joined = true;
        }
      }
    }
  }
  return boxes;
}

// The boxes as solids of one shape, each grown by `by`.
Shape box_solids(const std::vector<Bnd_Box>& boxes, double by) {
  BRep_Builder builder;
  TopoDS_Compound compound;
  builder.MakeCompound(compound);
  for (const Bnd_Box& box : boxes) {
    const Bnd_Box solid = inflated(box, by);
    builder.Add(compound, BRepPrimAPI_MakeBox(solid.CornerMin(), solid.CornerMax()).Shape());
  }
  return Shape(compound);
}

// The body cut down to the boxes, each grown by `by`: the pieces of the
// intersection, the body itself when a box holds all of it (the
// intersection leaves it as it is, without pieces), none when the boxes
// miss it.
std::vector<ShapePtr> clipped(const Shape& body, const std::vector<Bnd_Box>& boxes, double by) {
  const BooleanResult common = boolean(BooleanOp::Intersect, {&body}, box_solids(boxes, by));
  std::vector<ShapePtr> pieces;
  if (common.touched.front()) {
    for (const BooleanPiece& piece : common.pieces) {
      pieces.push_back(piece.shape);
    }
    return pieces;
  }
  Bnd_Box whole;
  BRepBndLib::Add(body.occt(), whole);
  const bool held = std::any_of(boxes.begin(), boxes.end(), [&](const Bnd_Box& box) {
    const Bnd_Box grown_box = inflated(box, by);
    for (int axis = 1; axis <= 3; ++axis) {
      if (whole.CornerMin().Coord(axis) < grown_box.CornerMin().Coord(axis) ||
          whole.CornerMax().Coord(axis) > grown_box.CornerMax().Coord(axis)) {
        return false;
      }
    }
    return true;
  });
  if (held) {
    pieces.push_back(std::make_shared<Shape>(body));
  }
  return pieces;
}

// The pieces that count (not slivers).
std::vector<const BooleanPiece*> counting(const BooleanResult& result) {
  std::vector<std::pair<double, const BooleanPiece*>> measured;
  double largest = 0.0;
  for (const BooleanPiece& piece : result.pieces) {
    const double v = volume(*piece.shape);
    largest = std::max(largest, v);
    measured.emplace_back(v, &piece);
  }
  std::vector<const BooleanPiece*> out;
  for (const auto& [v, piece] : measured) {
    if (v > kSliverShare * largest && v > kSliverVolume) {
      out.push_back(piece);
    }
  }
  return out;
}

// Whether every face of the piece lies within the slack of `near`'s
// faces at the points sampled: a sliver between near-coincident faces.
bool thin(const Shape& piece, Near& near) {
  for (int i = 0; i < piece.face_count(); ++i) {
    const std::vector<gp_Pnt> points = sample(piece.face(i));
    if (!std::all_of(points.begin(), points.end(), [&](const gp_Pnt& p) { return near(p); })) {
      return false;
    }
  }
  return true;
}

// `before` minus `after` within the boxes (apart from each other); none
// when a piece that counts reaches the side of its box. With `slivers`
// (`after`'s faces), the pieces whose faces all lie near `after`'s faces
// are left out, wherever they are.
std::optional<BooleanResult> cut_within(const Shape& before, const Shape& after,
                                        const std::vector<Bnd_Box>& boxes, Near* slivers = nullptr) {
  BooleanResult result;
  result.touched = {false};
  const std::vector<ShapePtr> within = clipped(before, boxes, 0.0);
  if (within.empty()) {
    return result;
  }
  throw_if_cancelled();
  const std::vector<ShapePtr> around = clipped(after, boxes, kApart);
  throw_if_cancelled();
  std::vector<const Shape*> targets;
  for (const ShapePtr& piece : within) {
    targets.push_back(piece.get());
  }
  BooleanResult cut;
  if (around.empty()) {
    cut.touched.assign(targets.size(), false);
  } else {
    BRep_Builder builder;
    TopoDS_Compound compound;
    builder.MakeCompound(compound);
    for (const ShapePtr& piece : around) {
      builder.Add(compound, piece->occt());
    }
    cut = boolean(BooleanOp::Cut, targets, Shape(compound));
  }
  // A part of `before` in a box that `after` does not reach is removed
  // as it is.
  for (std::size_t i = 0; i < targets.size(); ++i) {
    if (!cut.touched[i]) {
      cut.pieces.push_back({within[i], {i}});
    }
  }
  if (slivers != nullptr) {
    std::vector<BooleanPiece> kept;
    for (BooleanPiece& piece : cut.pieces) {
      throw_if_cancelled();
      if (!thin(*piece.shape, *slivers)) {
        kept.push_back(std::move(piece));
      }
    }
    cut.pieces = std::move(kept);
  }
  for (const BooleanPiece* piece : counting(cut)) {
    Bnd_Box box;
    BRepBndLib::Add(piece->shape->occt(), box);
    const bool inside = std::any_of(boxes.begin(), boxes.end(), [&](const Bnd_Box& region) {
      for (int axis = 1; axis <= 3; ++axis) {
        if (box.CornerMin().Coord(axis) <= region.CornerMin().Coord(axis) + kSide ||
            box.CornerMax().Coord(axis) >= region.CornerMax().Coord(axis) - kSide) {
          return false;
        }
      }
      return true;
    });
    if (!inside) {
      return std::nullopt;
    }
  }
  for (BooleanPiece& piece : cut.pieces) {
    piece.sources = {0};
  }
  result.pieces = std::move(cut.pieces);
  result.touched = {!result.pieces.empty()};
  return result;
}

// Whether MITCAD_NO_NEAR_COPIES is set: near copies are joined whole, for
// comparisons.
bool near_copies_off() {
#ifdef _MSC_VER
  char* value = nullptr;
  std::size_t length = 0;
  if (_dupenv_s(&value, &length, "MITCAD_NO_NEAR_COPIES") != 0 || value == nullptr) {
    return false;
  }
  std::free(value);
  return true;
#else
  return std::getenv("MITCAD_NO_NEAR_COPIES") != nullptr;
#endif
}

} // namespace

BooleanResult removed_material(const Shape& before, const Shape& after, double slack) {
  return detail::run("removed material", [&] {
    const std::vector<Bnd_Box> regions = changed_regions(before, after, slack);
    if (regions.empty()) {
      BooleanResult none;
      none.touched = {false};
      return none;
    }
    Bnd_Box body;
    BRepBndLib::Add(before.occt(), body);
    if (!body.IsVoid()) {
      for (int attempt = 0; attempt < kAttempts; ++attempt) {
        const std::vector<Bnd_Box> boxes = grown(regions, static_cast<double>(1 << attempt), 2.0 * kApart);
        double share = 0.0;
        for (const Bnd_Box& box : boxes) {
          share += box_volume(box);
        }
        if (share > kMaxShare * box_volume(body)) {
          break;
        }
        if (std::optional<BooleanResult> result = cut_within(before, after, boxes)) {
          return std::move(*result);
        }
      }
    }
    return boolean(BooleanOp::Cut, {&before}, after);
  });
}

std::optional<BooleanResult> join_near_copy(const Shape& body, const Shape& copy, double slack) {
  static const bool off = near_copies_off();
  if (off) {
    return std::nullopt;
  }
  return detail::run("join of a near copy", [&]() -> std::optional<BooleanResult> {
    // A near copy: at most half the faces of each differ from the other's.
    const std::vector<int> far_copy = far_faces(copy, body, slack, false);
    if (2 * far_copy.size() > static_cast<std::size_t>(copy.face_count())) {
      return std::nullopt;
    }
    const std::vector<int> far_body = far_faces(body, copy, slack, false);
    if (2 * far_body.size() > static_cast<std::size_t>(body.face_count())) {
      return std::nullopt;
    }
    BooleanResult same;
    same.touched = {true};
    same.pieces.push_back({std::make_shared<Shape>(body), {0}});
    std::vector<Bnd_Box> regions;
    add_boxes(copy, far_copy, regions);
    add_boxes(body, far_body, regions);
    if (regions.empty()) {
      return same;
    }
    Bnd_Box whole;
    BRepBndLib::Add(body.occt(), whole);
    if (whole.IsVoid()) {
      return std::nullopt;
    }
    // Slivers between the copy's faces and the body's within a box are
    // material within the slack: left out, wherever they reach.
    Near slivers(body, slack);
    for (int attempt = 0; attempt < kAttempts; ++attempt) {
      const std::vector<Bnd_Box> boxes = grown(regions, static_cast<double>(1 << attempt), 2.0 * kApart);
      double share = 0.0;
      for (const Bnd_Box& box : boxes) {
        share += box_volume(box);
      }
      if (share > kMaxShare * box_volume(whole)) {
        break;
      }
      // What the copy adds: the copy minus the body, within the boxes.
      const std::optional<BooleanResult> extra = cut_within(copy, body, boxes, &slivers);
      if (!extra) {
        continue;
      }
      const std::vector<const BooleanPiece*> pieces = counting(*extra);
      if (pieces.empty()) {
        return same;
      }
      BRep_Builder builder;
      TopoDS_Compound compound;
      builder.MakeCompound(compound);
      for (const BooleanPiece* piece : pieces) {
        builder.Add(compound, piece->shape->occt());
      }
      return boolean(BooleanOp::Join, {&body}, Shape(compound));
    }
    return std::nullopt;
  });
}

} // namespace mitcad::geometry
