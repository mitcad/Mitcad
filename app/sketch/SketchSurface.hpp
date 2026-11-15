// SPDX-License-Identifier: MIT
#pragma once

// A sketch on a cylindrical or conical face (mitcad#1768, commands.md
// "Sketches on cylindrical and conical faces", app/COMMANDS.md "Sketches on
// curved faces"). The sketch lies in the face's developed coordinates: on a
// cylinder the length around it and the distance along its axis, on a cone
// its development. Sketch mode keeps working in those flat coordinates
// (tools, snaps, constraints, dimensions); only the view's picture of them
// lies on the face:
//
// - SketchSurface is the face as the `sketch` query's `surface` describes
//   it, with the map from a sketch point onto the face that the view draws
//   points, highlights and previews with. It is the model's own map
//   (core/model-base/src/developed.rs, Development::point and ::outward),
//   so what the view draws lies where the model puts the sketch's curves.
// - The pointer goes back into the sketch through the model's
//   `sketch_surface` query (its rays: SketchController::sketchAt), the
//   sketch's curves are drawn from the same query's polylines
//   (MainWindow::sketchDisplay), and its profiles are laid onto the face
//   from their flat faces (wrappedFace).
// - Create Sketch and Redefine Sketch Plane take such a face as
//   `{"face", "body", "developed": {"angle"}}`, the angle putting the
//   sketch's origin where the face was clicked (developedPlane).

#include <functional>
#include <optional>
#include <vector>

#include <QJsonObject>
#include <QJsonValue>
#include <QString>

#include <TopoDS_Shape.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>

#include "SketchGeometry.hpp"
#include "mitcad/geometry/profile.hpp"

namespace mitcad {
struct SelectionItem;
}

namespace mitcad::sketch {

class SketchSurface {
public:
  enum class Kind { Cylinder, Cone };

  // The `surface` of the `sketch` or `sketch_surface` query; none for a
  // sketch on a plane (null, or `kind` plane) and for a description it
  // cannot read.
  static std::optional<SketchSurface> fromJson(const QJsonValue& surface);

  Kind kind() const { return m_kind; }
  // The material lies outside the surface (a bore's wall).
  bool internal() const { return m_internal; }
  // The radius at the sketch's origin (a cylinder's radius).
  double radius() const { return m_radius; }
  // The carrier plane: the sketch's frame, through its origin on the face,
  // the face's outward normal there its normal (the `sketch` query's
  // `frame`).
  const geometry::Frame& frame() const { return m_frame; }
  // A point of a cylinder's axis, or a cone's apex; the axis's direction
  // (a cone's from its apex where its radius grows).
  const gp_Pnt& axisOrigin() const { return m_origin; }
  const gp_Dir& axis() const { return m_axis; }

  // Moved by a placement: where an occurrence shows the sketch's component
  // (SketchHost::sketchPlacement, mitcad#505). The sketch coordinates stay.
  void transform(const gp_Trsf& placement);
  // The point of the face a sketch point goes to.
  gp_Pnt point(const V2& p) const;
  // The face's normal out of the material at a sketch point.
  gp_Dir outward(const V2& p) const;
  // The longest piece of a sketch line the view draws straight: what turns
  // about a twentieth of a radian on the face (at least 0.05 mm).
  double step() const;
  // "cylinder, radius 20.000 mm" or "cone, radius 12.000 mm, half angle
  // 30.000 deg" (", inside" for a bore's wall), for logs and UI tests.
  QString describe() const;
  // The surface's description as compact text: a key that changes with the
  // face's size, place and the sketch's origin on it.
  QString signature() const { return m_signature; }

private:
  // The model's wrap axes (Development::wrap): the centre on the axis at
  // the carrier origin's height (a cylinder's at the axis origin), the
  // radial direction to the carrier origin and the direction around, and
  // a cone's distance along the generator from the apex to the origin.
  void prepare();

  Kind m_kind = Kind::Cylinder;
  gp_Pnt m_origin;
  gp_Dir m_axis{0.0, 0.0, 1.0};
  double m_radius = 0.0;
  double m_halfAngle = 0.0;
  bool m_internal = false;
  geometry::Frame m_frame;
  QString m_signature;
  gp_Pnt m_centre;
  gp_Dir m_x{1.0, 0.0, 0.0};
  gp_Dir m_y{0.0, 1.0, 0.0};
  double m_rho0 = 0.0;
};

// The points of a polyline in the sketch with points put between them, at
// most `step` mm apart, so that its picture on a curved face bends with
// the face; `closed` adds the piece back to the first point.
std::vector<V2> densified(const std::vector<V2>& points, double step, bool closed = false);

// A flat face, or a compound of them, in the sketch's carrier plane (a
// profile of the sketch, in the coordinates the surface is in) laid onto
// the face as triangles without a surface, fine enough to follow it, for
// the view only. Null when it has no triangles.
TopoDS_Shape wrappedFace(const TopoDS_Shape& flat, const SketchSurface& surface);

// Whether a picked face is one a sketch can lie on in its developed
// coordinates: a cylindrical or conical face.
bool isDevelopedFace(const SelectionItem& item);

// The angle about a face's axis (radians) of the place `at`, as
// `sketch.create`'s `developed.angle` takes it: from the reference
// direction across the axis (the design's X axis across it, else its Y
// axis) right-handed about the axis counted towards +Z (else +Y, else
// +X); `axisPoint` a point of the axis.
double developedAngle(const gp_Dir& axis, const gp_Pnt& axisPoint, const gp_Pnt& at);

// The plane of `sketch.create` and `sketch.redefine_plane` for a picked
// cylindrical or conical face: `{"face", "body", "developed": {"angle"}}`,
// the angle putting the sketch's origin where the face was clicked
// (`item.at`, with the face's axis from the `faces` query `query` answers),
// else 0.
QJsonObject developedPlane(const SelectionItem& item, const std::function<QJsonValue(const QJsonObject&)>& query);

} // namespace mitcad::sketch
