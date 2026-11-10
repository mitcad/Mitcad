// SPDX-License-Identifier: MIT
#pragma once

// Move/Copy's motion as its inputs give it (mitcad#838), for its
// provisional drag motion (CommandDef::dragMotion): the same motion as the
// transform its definition gives the model, worked out without the model;
// and the axis Rotate's ring runs about, Move/Copy's and Move Face's
// (mitcad#854).

#include <functional>
#include <optional>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>

#include <gp_Ax1.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_XYZ.hxx>

#include "../framework/Command.hpp"

namespace mitcad::cmd {

// A free move's rotation: about X, then Y, then Z (radians).
gp_Trsf freeRotation(double rx, double ry, double rz);

// The motion of Move/Copy's inputs: Free Move turns about `pivot` (the
// middle of what moves, or an edit's kept pivot) and then moves by its X,
// Y and Z distances; Translate moves by its distances. None for the other
// move types, whose motions depend on picked geometry, and while a value
// is not a number.
std::optional<gp_Trsf> moveMotion(const CommandState& state, const gp_Pnt& pivot);

// Free Move's handles (mitcad#1580): its rings turn about the moved
// middle, `pivot` moved by the X, Y and Z distances; none while one of them
// is not a number (a minus sign typed alone). An arrow along `axis` (0, 1,
// 2: X, Y, Z) starts from that middle without its own distance, so that its
// knob, at the start plus its value, is at the moved middle as well, where
// all three arrows and the rings meet; none while a distance is not a
// number.
std::optional<gp_Pnt> freeMovedMiddle(const CommandState& state, const gp_Pnt& pivot);
std::optional<gp_Pnt> freeArrowOrigin(const CommandState& state, const gp_Pnt& pivot, int axis);

// A free move as the panel holds it (mitcad#1583): its X, Y and Z
// distances (mm) and its angles about X, then Y, then Z (radians), about a
// pivot. The model's matrix (`free`'s three rows of a rotation and a
// translation, commands.md) is p -> R (p - c) + c + t about the pivot c.
struct FreeMove {
  gp_XYZ moved{0.0, 0.0, 0.0};
  double rx = 0.0;
  double ry = 0.0;
  double rz = 0.0;
};
// The matrix split about a pivot: any pivot gives the same matrix back
// (freeMatrixOf), but the handles sit at the pivot moved by `moved`, so an
// edit splits it about the middle of what moves as the feature finds it.
FreeMove freeMoveOf(const QJsonArray& rows, const gp_Pnt& pivot);
QJsonArray freeMatrixOf(const FreeMove& move, const gp_Pnt& pivot);

// The motion of Rotate, Point to Point and Point to Position (mitcad#838)
// from what the model worked it out of, an exact preview's `motion`
// (commands.md): Rotate turns right-handed about the reported axis by its
// Angle, Point to Point moves from the reported point to the other, Point
// to Position the reported point to its X, Y and Z. None when the basis is
// of another type than the inputs, or a value is not a number.
std::optional<gp_Trsf> basisMotion(const CommandState& state, const QJsonObject& basis);

// Rotate's axis as the model turned a shown preview about it, kept for its
// ring (mitcad#838): `kept` is {"basis": the preview's `motion`, "axis":
// the Axis input's reference, "occurrence": the occurrence it was picked
// in}. The reported axis taken into the design by `frame` (the placement of
// the move's component) while `kept` is of a Rotate and of the axis picked
// now (`axis`, `occurrence`); none otherwise, when the ring takes the
// picked geometry's own axis until a preview reports one. The ring's angle
// then runs right-handed about the axis the result turns about.
std::optional<gp_Ax1> keptRotateAxis(const QJsonObject& kept, const QJsonValue& axis, const QString& occurrence,
                                     const gp_Trsf& frame);

// What Move/Copy and Move Face keep of a preview's report for Rotate's ring
// (CommandDef::previewReported, mitcad#838, mitcad#854): `kept` for
// keptRotateAxis, the report's `motion` with the Axis input's reference
// (`axis`) and the occurrence it was picked in; empty unless the inputs
// are a Rotate (`type`) and the report has a rotate `motion`.
QJsonObject rotateBasisToKeep(const QJsonObject& report, const QString& type, const QJsonValue& axis,
                              const QString& occurrence);

// The axis Rotate's ring runs about, Move/Copy's and Move Face's alike
// (mitcad#838, mitcad#854), through the foot of `center` (the middle of
// what moves) on it: the axis kept from a preview's report
// (keptRotateAxis) while it is of the axis picked now, else `picked()`,
// the picked geometry's own axis (asked only then); none without either.
std::optional<gp_Ax1> rotateRingAxis(const QJsonObject& kept, const QJsonValue& axis, const QString& occurrence,
                                     const gp_Trsf& frame, const std::function<std::optional<gp_Ax1>()>& picked,
                                     const gp_Pnt& center);

} // namespace mitcad::cmd
