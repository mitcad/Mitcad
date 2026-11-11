// SPDX-License-Identifier: MIT
#pragma once

// A 3D sketch as its editing context shows it (mitcad#1524): the
// `sketch3d` query's answer read into plain values, and the texts the
// Sketch Palette, the status line and the logs give of it (its freedoms,
// what is determined, redundant or failed, its in-plane relations and its
// linked references with their sources). The model computes everything;
// this only reads and words it. Plain Qt Core, so the unit tests read it
// without a view.

#include <array>
#include <functional>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>
#include <QStringList>

namespace mitcad::sketch {

using Xyz = std::array<double, 3>;

struct Sketch3dPoint {
  QString id;     // p2
  Xyz at{};       // local coordinates, where the last solve left it
  Xyz position{}; // in the component (the frame applied once)
  bool hasPosition = false;
  bool determined = false;
  bool onSketchPlane = false; // kept in the plane by On Sketch Plane
};

struct Sketch3dCurve {
  QString id;   // c3
  QString type; // line, arc, circle, spline, fitted_spline, ellipse, elliptical_arc, reference
  bool construction = false;
  bool planeBound = false; // a converted sketch's arc or circle bound to the sketch plane
  bool determined = false;
  QStringList points; // the points that define it
  QJsonObject json;   // as the query gives it (definition and `geometry`)
};

// A relation or a dimension (both numbered k…).
struct Sketch3dItem {
  QString id;
  QString type;
  bool dimension = false;
  bool inPlane = false;
  bool redundant = false;
  bool driven = false;
  QStringList entities; // the points and curves it names
  QJsonObject json;
};

// A linked reference record: an attachment (k…), a reference curve (c…) or
// a point on a face (k…, mitcad#1549).
struct Sketch3dReference {
  QString id;
  QString kind;   // point, curve, face
  QString mode;   // include, project
  QString status;  // ok, failed, unknown
  QString message; // why it failed, in plain words (mitcad#1644)
  QString detail;  // the same for agents, with the ids
  QString body;
  QString occurrence;
  QJsonValue source;
  QStringList users;
};

struct Sketch3dModel {
  QString uid;
  QString name;
  QJsonValue support;
  bool hasFrame = false;
  Xyz origin{};
  Xyz xAxis{1.0, 0.0, 0.0};
  Xyz yAxis{0.0, 1.0, 0.0};
  Xyz normal{0.0, 0.0, 1.0};
  std::vector<Sketch3dPoint> points;
  std::vector<Sketch3dCurve> curves;
  std::vector<Sketch3dItem> items; // relations, then dimensions
  std::vector<Sketch3dReference> references;
  int texts = 0; // a converted sketch's
  bool evaluated = false;
  // The solve's status; none (dof -1) when it did not solve.
  bool solved = false;
  int dof = -1;
  bool fullyConstrained = false;
  QStringList redundant;
  // Why the last evaluation failed (kind empty: it did not).
  QString failureKind;
  QString failureMessage; // in plain words (mitcad#1644)
  QString failureDetail;  // the agents' text, with the ids
  QStringList failureItems; // its relations and entities
  QStringList warnings;
  // mitcad#1550: it all lies in its sketch plane (it can become a 2D
  // sketch), what lies out of it, and the profile regions it bounds there.
  bool flat = false;
  QStringList offPlane;      // the agents' texts, with the ids
  QStringList offPlaneShown; // the same in plain words (mitcad#1644)
  QStringList regions;

  // Reads the `sketch3d` query's answer; an empty model for anything else.
  static Sketch3dModel read(const QJsonObject& sketch);

  bool empty() const { return points.empty() && curves.empty() && texts == 0; }
  const Sketch3dPoint* point(const QString& id) const;
  const Sketch3dCurve* curve(const QString& id) const;
  const Sketch3dItem* item(const QString& id) const;
  // The record of an attachment or a reference curve; null for others.
  const Sketch3dReference* reference(const QString& id) const;
  // The reference curve a relation or point uses, or the attachment of a
  // point; null when it uses none.
  const Sketch3dReference* referenceOf(const QString& id) const;
  int determinedPoints() const;
  int determinedCurves() const;
  // The relations and dimensions kept in plane from a converted sketch.
  QStringList inPlaneItems() const;
  int onSketchPlanePoints() const;
  QStringList failedReferences() const;
};

// What names the features and components a source is in (mitcad#1652): a
// feature's name by its uid ("Extrude2" for F2; a body F2.b0 is named by
// its feature), a component's by an occurrence path. Either may be unset or
// give nothing: the words then say "a body", "another sketch", "another
// component", never an id.
struct SourceNames {
  std::function<QString(const QString& feature)> feature;
  std::function<QString(const QString& occurrence)> component;
};

// An entity's name in the palette's words (mitcad#1652, translated): its
// kind and running number among the sketch's entities of that kind, in the
// sketch's order, as the timeline numbers features: "Point 3", "Line 2",
// "Reference curve 1", "Coincident 2", "Length 1". Empty for an id the
// sketch lacks. The ids themselves are the agents' (the query, the logs).
QString entityName(const Sketch3dModel& model, const QString& id);
// Several entities' names, ", " between them; ids the sketch lacks are left
// out.
QString entityNames(const Sketch3dModel& model, const QStringList& ids);

// A source of a linked reference in words (mitcad#1652, translated): "an
// edge of Extrude2", "the middle of an edge of Box1", "a vertex of a body",
// "a face of Box1", "a point of Sketch1", "the end of a curve of Sketch1",
// "Intersection Curve1", "the origin", and "… in Bracket" for another
// component's. No topological names, ids or coordinates.
QString sourceText(const QJsonValue& source, const SourceNames& names = {});
// The same for agents and the log, in English with its names and ids:
// "edge E{…} of F2.b0 in O1", "vertex … of …", "the middle of edge …",
// "point p2 of F3", "curve c4 of F3", "the origin", "Intersection Curve F5".
QString sourceDetail(const QJsonValue& source);

// The places a source is (mitcad#1652), as the view paints a notice's
// places while the pointer is over the palette's row that uses it: a body's
// vertex, edge or face ({kind, name, body}, with `occurrence` for another
// component's), another sketch's point or curve ({kind: sketch_point or
// sketch_curve, sketch, name}) or an Intersection Curve ({kind: feature,
// name}). None for the origin.
QJsonArray sourcePlaces(const QJsonValue& source);
// The places of a point, a curve, a relation, a dimension or a linked
// reference record of the sketch: itself (its points and curves: kind
// sketch_point or sketch_curve of the sketch's uid), then the sources of
// the reference records it is or uses. Empty for an id the sketch lacks.
QJsonArray entityPlaces(const Sketch3dModel& model, const QString& id);

// A relation's or dimension's kind (translated): "Coincident", "Length",
// "Point on Curve" (mitcad#1526: the palette's list).
QString itemKindName(const QString& type, bool dimension);
// The same in English, for agents and the log.
QString itemKindEnglish(const QString& type, bool dimension);

// The palette's status line: "Fully constrained", "3 degrees of freedom",
// "Not solved: <why>"; translated.
QString statusText(const Sketch3dModel& model);
// The palette's detail lines (translated): determined points and curves,
// redundant relations, in-plane relations and On Sketch Plane points,
// references and failed ones, and that a 3D sketch bounds no profiles.
QStringList detailLines(const Sketch3dModel& model);
// One point's, curve's, relation's or dimension's description for the
// palette and the hover, in words (mitcad#1652, translated): "Point 2;
// determined; linked to the origin"; "Reference curve 1 (an edge of
// Extrude2); included in 3D"; "Coincident 1 on Point 2, Point 5;
// redundant". No ids, topological names or coordinates; the view paints
// where it is. Empty for an id the sketch lacks.
QString describeEntity(const Sketch3dModel& model, const QString& id, const SourceNames& names = {});
// The same for agents and the log, in English with the ids and the local
// coordinates in `unit` (`millimetres` per unit): "Point p2 at 40.00,
// 0.00, 0.00 mm; determined"; "Reference curve c7: edge E{…} of F2.b0;
// included in 3D". Empty for an id the sketch lacks.
QString entityDetail(const Sketch3dModel& model, const QString& id, const QString& unit, double millimetres);
// The log line of the status, in English for UI tests: "3D sketch F3
// (3DSketch1): 5 points, 3 curves, dof 2, determined 4 points 2 curves,
// redundant none, in plane none, references 1 (failed none), failure none".
QString statusLog(const Sketch3dModel& model);

} // namespace mitcad::sketch
