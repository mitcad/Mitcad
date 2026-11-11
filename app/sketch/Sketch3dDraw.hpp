// SPDX-License-Identifier: MIT
#pragma once

// The drawing tools of a 3D sketch without the view (mitcad#1525): where a
// pointer's ray places a point on the temporary XY, YZ or ZX plane through
// the anchor, along an axis or along the plane's normal; which snap wins
// among the candidates under the pointer (discrete points before points on
// curves before guidance, the nearest on the screen, ties by kind); the
// typed values of a segment (Offsets: signed Delta X, Y and Z; Axis and
// Length) and what they contradict; the arcs' geometry from their points;
// and the `sketch3d.*` commands a tool sends, with the follow-up relations
// and dimensions that go into the same undo step. Everything is in the
// sketch's local coordinates (mm). Plain Qt Core, so the unit tests need
// no view.

#include <array>
#include <optional>
#include <utility>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>
#include <QStringList>

#include "Sketch3dModel.hpp"

namespace mitcad::sketch {

// Local vectors.
Xyz add(const Xyz& a, const Xyz& b);
Xyz sub(const Xyz& a, const Xyz& b);
Xyz scaled(const Xyz& a, double factor);
double dot(const Xyz& a, const Xyz& b);
Xyz cross(const Xyz& a, const Xyz& b);
double norm(const Xyz& a);
double distance(const Xyz& a, const Xyz& b);
// Of unit length; none for a vector of (nearly) no length.
std::optional<Xyz> unit(const Xyz& a);
bool finite(const Xyz& a);
// `a` turned by `angle` (radians, counter-clockwise) about the unit `axis`.
Xyz rotated(const Xyz& a, const Xyz& axis, double angle);
// The angle from `a` to `b` counter-clockwise about the unit `axis`, in
// [0, 2π).
double angleAbout(const Xyz& a, const Xyz& b, const Xyz& axis);

enum class Axis3 { X, Y, Z };
enum class Plane3 { XY, YZ, ZX };

Xyz axisVector(Axis3 axis);
// "x", "y", "z": the commands' axis names.
QString axisKey(Axis3 axis);
// "X", "Y", "Z".
QString axisLabel(Axis3 axis);
// The frame axis a temporary plane is square to: XY's Z, YZ's X, ZX's Y.
Axis3 planeNormalAxis(Plane3 plane);
Xyz planeNormal(Plane3 plane);
// The two frame axes in a temporary plane: XY's X and Y, YZ's Y and Z,
// ZX's Z and X.
std::array<Axis3, 2> planeAxes(Plane3 plane);
// "XY", "YZ", "ZX".
QString planeLabel(Plane3 plane);

// The manipulator's free rotation (mitcad#1661): the temporary frame's X,
// Y and Z in the sketch's local coordinates, of unit length, square to
// each other and right-handed. It is the sketch's own frame until the
// rotation handles turn it; turning it guides the placement only and never
// changes the sketch's stored frame.
using Frame3 = std::array<Xyz, 3>;
Frame3 sketchFrame();
// Whether a frame is the sketch's own (each axis within 1e-9).
bool isSketchFrame(const Frame3& frame);
// `frame` turned by `angle` (radians, counter-clockwise seen from the
// axis' positive end) about its own axis `about`; made square again, and
// a component within 1e-12 of 0 or ±1 made exact, so that quarter turns
// give the sketch's axes again.
Frame3 turnedFrame(const Frame3& frame, Axis3 about, double angle);
// A local vector's coordinates along a frame's X, Y and Z, and the local
// vector of such coordinates (the typed values on a rotated plane).
Xyz inFrame(const Frame3& frame, const Xyz& v);
Xyz outOfFrame(const Frame3& frame, const Xyz& v);
// A rotated plane's frame as the commands take it (mitcad#1661):
// {"x": its X, "normal": its Z}, the model's y its Y.
QJsonObject frameJson(const Frame3& frame);

// A pointer's ray in local coordinates: from the eye (or the near plane)
// along a unit direction.
struct Ray3 {
  Xyz from{};
  Xyz direction{0.0, 0.0, -1.0};
};

// Below this sine of the angle between a ray and a plane, or above the
// cosine (1 - its square) between a ray and a line, a place found by the
// pointer would jump: the plane is seen edge-on, the line end-on.
constexpr double kEdgeOnSine = 0.05;

// Where the ray meets the plane through `through` square to the unit
// `normal`; none when the plane is seen edge-on or lies behind the eye.
std::optional<Xyz> onPlane(const Ray3& ray, const Xyz& through, const Xyz& normal);
// The point of the line through `through` along the unit `direction`
// nearest to the ray; none when the line is seen (nearly) end-on.
std::optional<Xyz> onLine(const Ray3& ray, const Xyz& through, const Xyz& direction);

// How the pointer places the next point (the temporary manipulator): on
// the active plane through the anchor, along an axis through the anchor,
// or, Normal, along the plane's normal through an in-plane place held when
// Normal was chosen (the elevation changes, the in-plane coordinates stay).
// The plane, the axes and the normal are those of `frame`: the sketch's
// own, or as the rotation handles turned them (mitcad#1661).
// Nothing of this is stored: it guides, it adds no relation.
struct Placement {
  enum class Mode { Plane, Axis, Normal };
  Plane3 plane = Plane3::XY;
  Mode mode = Mode::Plane;
  Axis3 axis = Axis3::X;
  std::optional<Xyz> normalBase; // Normal: the in-plane place held
  Frame3 frame = sketchFrame();  // the manipulator's rotation

  bool rotated() const { return !isSketchFrame(frame); }
};

// The placement's directions in the sketch's local coordinates: an axis
// of its frame, the active plane's normal and the plane's two axes.
Xyz placementAxis(const Placement& placement, Axis3 axis);
Xyz placementNormal(const Placement& placement);
std::array<Xyz, 2> placementPlaneAxes(const Placement& placement);

// "on the XY plane", "along X", "along the normal of XY"; rotated: "on
// the rotated XY plane", "along the rotated X", "along the normal of the
// rotated XY" (translated).
QString placementText(const Placement& placement);
// Where the pointer's ray places a point through `anchor`; none when that
// place is unstable (an edge-on plane, a view-parallel axis).
std::optional<Xyz> placePoint(const Ray3& ray, const Xyz& anchor, const Placement& placement);

// The ring of a rotation handle about the frame's axis `about` through
// `center` (mitcad#1661): `segments` + 1 points of radius `radius`,
// starting on the frame's next axis (Y for X, Z for Y, X for Z).
std::vector<Xyz> handleRing(const Xyz& center, const Frame3& frame, Axis3 about, double radius, int segments);
// How far a rotation handle about the unit `axis` through `center` has
// turned from `grabbed` (a place in the ring's plane) to where the
// pointer's ray meets that plane: radians in (-π, π], counter-clockwise
// about the axis; none when the plane is seen edge-on, lies behind the eye
// or a place is on the center.
std::optional<double> handleTurn(const Ray3& ray, const Xyz& center, const Xyz& axis, const Xyz& grabbed);
// An angle as a handle's drag takes it: to whole degrees (radians).
double wholeDegrees(double radians);
// Radians in degrees, as the fields and the log show them: "30°", "-12.5°".
QString degreesText(double radians);

// What a point snaps to, in the order of the specification's tie-break.
enum class SnapKind {
  Free,        // no snap: where the placement puts it
  Point,       // an existing point of the sketch: shared
  Origin,      // the frame's origin: an origin attachment
  LinkedPoint, // a body vertex or edge end, another sketch's point: attached
  Center,      // an arc's or circle's centre point of the sketch: shared
  Midpoint,    // the middle of an edge or a curve
  OnCurve,     // a point on a curve of a sketch or an Intersection Curve
  OnEdge,      // a point on a body edge
  Guide,       // an axis through the anchor on the active plane: guidance only
  OnFace,      // a point on a face of a body (mitcad#1549): the lowest
};

// What the pointer would snap to now, and what a click stores for it.
struct SnapTarget {
  SnapKind kind = SnapKind::Free;
  Xyz at{};          // local coordinates
  double pixels = 0; // on the screen from the pointer
  // An existing point of the sketch (Point, Center): the point is shared.
  QString point;
  // A linked source (Origin, LinkedPoint, a Midpoint or On Curve of
  // another sketch or an edge, OnEdge, OnFace): the point is attached to
  // it, put on its reference curve or on the face (commands.md "Linked
  // references").
  QJsonValue source;
  // A curve of this sketch (OnCurve, Midpoint): Point on Curve or Midpoint.
  QString curve;
  // Of this sketch: wins ties among the points on curves.
  bool own = false;
  // The badge's source for agents and the log, with its ids: "vertex V{…}
  // of F2.b0 in Component1", "c3"; a guide's axis, "X".
  QString what;
  // The same as the badge shows it, in words (mitcad#1653): "a vertex of
  // Box1 in Bracket", "Line 1"; empty for none.
  QString words;

  bool discrete() const;
  bool continuous() const;
  // Where the point is fixed by the snap alone (a shared, linked or origin
  // point, a midpoint): typed values must agree with it.
  bool fixesPlace() const { return discrete(); }
};

// The badge's kind: "Point", "Origin", "Linked Point", "Center",
// "Midpoint", "On Curve", "On Edge", "Along X", "On Face" (translated);
// empty for a free place.
QString snapKindText(const SnapTarget& target);
// The badge, in words (mitcad#1653, translated): "Linked Point: a vertex of
// Box1", "On Curve: Line 2", "Origin", "Along X". No ids, topological
// names or coordinates.
QString snapBadge(const SnapTarget& target);
// The same for agents and the log, with the ids: "Linked Point: vertex
// V{…} of F2.b0", "On Curve: c3".
QString snapDetail(const SnapTarget& target);
// A source a snap offers (an edge, a vertex, a face, another sketch's point
// or curve, an Intersection Curve; with `occurrence` for another
// component's, as the tools' sources carry it) in the badge's words: "an
// edge of Box1", "the start of an edge of Box1", "a point of Sketch1 in
// Bracket". A middle is left to the badge's kind (Midpoint).
QString snapSourceWords(const QJsonValue& source, const SourceNames& names);
// The places the view paints for a snap (mitcad#1653): its source outside
// the sketch (sourcePlaces); none for the sketch's own points and curves,
// the origin, a guide or a free place.
QJsonArray snapPlaces(const SnapTarget& target);
// The winner among the candidates within `tolerance` pixels: discrete
// points before points on curves before guidance before a face; in a
// class the nearest on the screen; at equal distance (within half a pixel)
// a point of the sketch, then the origin, a linked point, a centre, a
// midpoint; among points on curves this sketch's first. None when no
// candidate is near.
std::optional<SnapTarget> chooseSnap(const std::vector<SnapTarget>& candidates, double tolerance);
// Whether a snap would put the next point on the start it continues from
// (mitcad#1661): a snap that fixes the place (the origin, a point) where
// the pending start is, as when the pointer rests where the start was
// clicked. It is no place for the next point (a segment of no length), so
// it is no candidate: the typed offsets and the placement put the point.
bool snapsOntoStart(const SnapTarget& snap, const Xyz& start);

// A point a tool has placed: where, and what it snapped to.
struct PlacedPoint {
  Xyz at{};
  SnapTarget snap;
  // Offsets typed for it from the tool's previous point (or the anchor):
  // the driving offset dimensions added with it, an empty expression for
  // an axis not typed.
  std::array<QString, 3> offsets;
  // The frame they were typed along: the sketch's, or a rotated plane's
  // (mitcad#1661: Offset Along dimensions).
  Frame3 offsetFrame = sketchFrame();
};

// The point input of a placed point: an existing point's id, or [x, y, z].
QJsonValue pointInput(const PlacedPoint& point);
// The source a placed point is attached to or put on (start_source,
// end_source, add_point's source); null when none.
QJsonValue pointSource(const PlacedPoint& point);
// The relation a snap to this sketch's own curve stores on the point made
// for it (`point_on` or `midpoint`, as sketch3d.add_constraint takes it);
// empty for other snaps.
QJsonObject ownCurveRelation(const PlacedPoint& point, const QString& pointId);

// A value typed in a tool's field.
struct TypedValue {
  bool edited = false; // typed: a driving value; else a live measurement
  QString text;        // as typed
  QString expression;  // as the model takes it ("40 mm", "d1 * 2")
  double value = 0.0;  // mm, when valid
  bool valid = true;
  QString problem; // why it is not valid
  void clear() { *this = TypedValue(); }
};

// The typed input of a segment or of a point a tool places.
struct SegmentInput {
  enum class Mode { Offsets, AxisLength };
  Mode mode = Mode::Offsets;
  std::array<TypedValue, 3> offsets; // Delta X, Y, Z
  Axis3 axis = Axis3::X;             // Axis and Length
  bool negative = false;
  TypedValue length;

  // Whether a value of the current mode is typed.
  bool anyEdited() const;
  // The typed values of the other mode are dropped (a mode switch).
  void clearOtherMode();
  // Every typed value is dropped.
  void clearAll();
};

// The end the typed values put a segment from `start`, with `pointer`
// (where the pointer places it) for what is not typed: Offsets replace
// the typed coordinates' differences; Axis and Length puts it along the
// axis in its sense at the typed length, or as far along as the pointer.
// On a rotated plane (`frame`, mitcad#1661) the axes and offsets are its.
Xyz typedEnd(const Xyz& start, const Xyz& pointer, const SegmentInput& input, const Frame3& frame = sketchFrame());
// Why the typed values cannot make a segment from `start` to `end`: a
// value not valid, a length not above zero, a segment of at most 1e-6 mm,
// Keep Axis against a typed offset; empty when they can.
QString inputProblem(const Xyz& start, const Xyz& end, const SegmentInput& input, std::optional<Axis3> keepAxis);
// Why a snap that fixes the place contradicts the typed values (it does
// not satisfy them within 1e-6 mm), naming both; empty when it does not.
QString snapConflict(const Xyz& start, const SnapTarget& snap, const SegmentInput& input,
                     std::optional<Axis3> keepAxis, const Frame3& frame = sketchFrame());
// The model's numbers as the fields show them: "40.00".
QString formatLength(double millimetres, double unitMillimetres);

// sketch3d.add_line from `start` to `end` with what was typed: Offsets'
// typed fields as `offsets` (a field not typed null, a typed zero kept),
// Axis and Length's `axis`, `sense` and a typed `length`, Keep Axis's
// alignment with the sense the segment runs; the snaps' sources. On a
// rotated plane (mitcad#1661) with its `frame`: the axis and the offsets
// are along the frame's axes.
QJsonObject lineCommand(const QString& sketch, const PlacedPoint& start, const PlacedPoint& end,
                        const SegmentInput& input, std::optional<Axis3> keepAxis, bool construction,
                        const Frame3& frame = sketchFrame());
// sketch3d.add_point with its source and the place.
QJsonObject pointCommand(const QString& sketch, const PlacedPoint& point);

// An arc's circle and sweep: counter-clockwise about `normal` from start.
struct ArcShape {
  Xyz center{};
  Xyz normal{0.0, 0.0, 1.0};
  double radius = 0.0;
  double sweep = 0.0;
};
// Through start, `through` and end, in that order; none for three points
// on one line or two that meet.
std::optional<ArcShape> threePointArc(const Xyz& start, const Xyz& end, const Xyz& through);
// Around `center` from `start` about the unit `normal`: `endHint` turned
// onto the circle (`end` gets it); none for a start on the centre or an
// end hint on the axis.
std::optional<ArcShape> centerArc(const Xyz& center, const Xyz& start, const Xyz& endHint, const Xyz& normal,
                                  Xyz* end);
// Leaving `start` along the unit `tangent` to `end`, in the plane of the
// tangent and the end; none when the end lies on the tangent's line.
std::optional<ArcShape> tangentArc(const Xyz& start, const Xyz& tangent, const Xyz& end);
// The same arc with a typed radius and arc length (each optional): the
// end where they put it, toward `endHint`.
std::optional<ArcShape> tangentArcSized(const Xyz& start, const Xyz& tangent, const Xyz& endHint,
                                        std::optional<double> radius, std::optional<double> arcLength, Xyz* end);
// `segments` + 1 points along the arc from `start`.
std::vector<Xyz> arcPoints(const ArcShape& arc, const Xyz& start, int segments);
// Keep Plane: the axis and sense of the plane's normal the arc's normal
// lies along; none when it lies along none of them.
std::optional<std::pair<Axis3, bool>> keptNormal(const Xyz& normal, Plane3 plane);

// sketch3d.add_arc (mode three_point, center or tangent); `through`,
// `center`, `normal` and `from` as the mode takes them; Keep Plane's
// normal axis and sense, a typed radius and arc length.
struct ArcRequest {
  QString mode; // three_point, center, tangent
  PlacedPoint start;
  PlacedPoint end;
  PlacedPoint center;   // center
  Xyz through{};        // three_point
  Xyz normal{0.0, 0.0, 1.0}; // center
  QString from;         // tangent: the curve continued
  std::optional<std::pair<Axis3, bool>> keepPlane;
  QString radius;    // typed expressions, empty when not typed
  QString arcLength;
  bool construction = false;
};
QJsonObject arcCommand(const QString& sketch, const ArcRequest& arc);
// sketch3d.add_spline through (or by) the placed points: a fit-point
// spline (closed: periodic through the first) or a control-point spline
// of degree min(3, n - 1).
QJsonObject splineCommand(const QString& sketch, const std::vector<PlacedPoint>& points, bool controlPoints,
                          bool closed, bool construction);
// The commands that follow a made curve in the same undo step: for each
// placed point (its id in `ids`, in the same order; an empty id for one
// that is not stored), an attachment to a linked source when the curve's
// command did not take it (`attachSources`), a relation with this
// sketch's curve it snapped to, and the driving offset dimensions typed
// for it from the point before it (`previous`: the id before the first,
// the anchor's, or empty).
QJsonArray followUps(const QString& sketch, const std::vector<PlacedPoint>& points, const QStringList& ids,
                     const QString& previous, bool attachSources);

// Sample points of curves for the preview: a cubic through the fit
// points (closed: periodic), or a clamped B-spline of degree min(3, n - 1)
// by the control points.
std::vector<Xyz> fitSplinePreview(const std::vector<Xyz>& points, bool closed);
std::vector<Xyz> controlSplinePreview(const std::vector<Xyz>& control);

// The direction a curve of the sketch leaves `point` along when it is
// continued there: a line's from its other end, an arc's tangent, a
// spline's first or last control leg; none when `point` is not one of its
// ends or the curve has no direction there.
std::optional<Xyz> endTangent(const Sketch3dModel& model, const QString& curve, const QString& point);
// The curves of the sketch that end at `point` and a Tangent Arc can
// continue: lines, arcs and open splines, not construction-only reference
// curves; the free end's curve first.
QStringList continuableCurves(const Sketch3dModel& model, const QString& point);

// --- Plane-bound shapes (mitcad#1550): circles, ellipses, rectangles,
// polygons and texts drawn on the active XY, YZ or ZX plane through their
// first point, as the SKETCH tab's planar tools draw them.

// "xy", "yz", "zx": the commands' plane names.
QString planeKey(Plane3 plane);

// The plane of a plane-bound shape: XY, YZ or ZX of the manipulator's
// frame, the sketch's own or rotated (mitcad#1661). A Plane3 alone is the
// sketch's own.
struct ShapePlane {
  Plane3 plane = Plane3::XY;
  Frame3 frame = sketchFrame();

  ShapePlane(Plane3 p) : plane(p) {} // a frame plane of the sketch
  ShapePlane(Plane3 p, const Frame3& f) : plane(p), frame(f) {}
  bool rotated() const { return !isSketchFrame(frame); }
  // Its first and second in-plane axes and its normal (unit, local).
  Xyz u() const;
  Xyz v() const;
  Xyz normal() const;
};
// The commands' plane: "xy", "yz", "zx", or a rotated plane's
// {"x": u, "normal": normal}.
QJsonValue planeValue(const ShapePlane& plane);
// A point's planar coordinates in a plane (along its first and second
// axis), and the point of planar coordinates at an elevation along the
// plane's normal.
std::array<double, 2> planarOf(const Xyz& p, const ShapePlane& plane);
Xyz fromPlanar(const std::array<double, 2>& uv, double elevation, const ShapePlane& plane);
// `p` moved along the plane's normal into the plane through `through`.
Xyz ontoPlane(const Xyz& p, const Xyz& through, const ShapePlane& plane);

struct CircleShape {
  Xyz center{};
  double radius = 0.0;
};
// Through the ends of a diameter, or through three points, in the plane
// through the first; none for points that meet or lie on one line.
std::optional<CircleShape> twoPointCircle(const Xyz& a, const Xyz& b, const ShapePlane& plane);
std::optional<CircleShape> threePointCircle(const Xyz& a, const Xyz& b, const Xyz& c, const ShapePlane& plane);
// `segments` + 1 points around a circle in the plane through its centre.
std::vector<Xyz> circlePoints(const Xyz& center, double radius, const ShapePlane& plane, int segments);
// An ellipse's minor radius a point gives: its distance from the major
// axis, in the plane through the centre.
double ellipseMinor(const Xyz& center, const Xyz& major, const Xyz& at, const ShapePlane& plane);
// `segments` + 1 points around an ellipse.
std::vector<Xyz> ellipsePoints(const Xyz& center, const Xyz& major, double minor, const ShapePlane& plane, int segments);

enum class RectangleMode { TwoPoint, Center, ThreePoint };
// "two_point", "center", "three_point".
QString rectangleKey(RectangleMode mode);
// A rectangle's corners counter-clockwise about the plane's normal from
// its first, as the model numbers them, from its clicks (opposite
// corners; the centre and a corner; a side and the opposite side's
// place), in the plane through the first click; none without area.
std::optional<std::array<Xyz, 4>> rectangleCorners(RectangleMode mode, const std::vector<Xyz>& clicks, const ShapePlane& plane);
// The corner each click is (0 to 3), as the model joins an existing point
// given there; -1 for a click that is no corner (a centre, a side's place).
std::vector<int> rectangleClickCorners(RectangleMode mode, const std::vector<Xyz>& clicks, const ShapePlane& plane);
// A regular polygon's corners: around `center` with the first corner at
// `vertex` (inscribed) or the middle of the first side there
// (circumscribed).
std::vector<Xyz> polygonCorners(const Xyz& center, const Xyz& vertex, int sides, bool inscribed, const ShapePlane& plane);
// An Edge Polygon's centre: on the side of the edge from `a` to `b` where
// `side` is, in the plane through `a`; none for an edge of no length.
std::optional<Xyz> edgePolygonCenter(const Xyz& a, const Xyz& b, const Xyz& side, int sides, const ShapePlane& plane);

// sketch3d.add_circle on the plane around `center` (an existing point's
// id or a new place) with the radius it is drawn with; `typed` a driving
// radius (or diameter) expression, else the drawn value drives; `through`
// existing points held on it.
QJsonObject circleCommand(const QString& sketch, const PlacedPoint& center, double radius, const ShapePlane& plane,
                          const QString& typed, bool diameter, const QStringList& through, bool construction);
// sketch3d.add_ellipse around `center` to `major` with the drawn minor
// radius; the typed radii as its dimensions.
QJsonObject ellipseCommand(const QString& sketch, const PlacedPoint& center, const PlacedPoint& major, double minor,
                           const ShapePlane& plane, const QString& majorTyped, const QString& minorTyped, bool construction);
// sketch3d.add_rectangle of the mode from its clicks (a 3-point
// rectangle's third a place), the typed width and height.
QJsonObject rectangleCommand(const QString& sketch, RectangleMode mode, const std::vector<PlacedPoint>& clicks,
                             const ShapePlane& plane, const QString& width, const QString& height, bool construction);
// sketch3d.add_polygon around `center` through `vertex`.
QJsonObject polygonCommand(const QString& sketch, const PlacedPoint& center, const PlacedPoint& vertex, int sides,
                           bool inscribed, const ShapePlane& plane, const QString& radius, bool construction);
// sketch3d.add_text at `at` (in a frame to `frame` when given).
QJsonObject textCommand(const QString& sketch, const QString& text, const Xyz& at, const std::optional<Xyz>& frame,
                        const ShapePlane& plane, double height);

} // namespace mitcad::sketch
