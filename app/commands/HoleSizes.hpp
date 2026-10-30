// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <limits>

#include <QHash>
#include <QJsonObject>
#include <QPair>
#include <QString>
#include <QStringList>
#include <QVector>

#include "../framework/Command.hpp"

namespace mitcad::cmd {

// The Hole panel's fields that follow a screw size (mitcad#903,
// mitcad#980). Hole Tap Type Clearance takes a screw size and an ISO 273
// fit: the hole's Diameter and its counterbore's, countersink's and
// counterdrill's values come from the model's `hole_sizes` table. A tapped
// hole's counterbore and counterdrill come from the same table for the
// screw of its thread's nominal size (DIN 974-1; an ISO metric size the
// table has a row for), its countersink from its thread's major diameter.
// A field follows the size until the user types in it (or it holds an
// expression): it is then kept when the size changes and marked, its mark
// (FieldMark) saying "Typed: does not follow the size" with the action
// Reset to size. A size that has no value for a field (M24's countersink)
// clears it and marks it "No standard value for M24", never leaving the
// last size's value. Depth follows a counterbore or counterdrill the size
// fills while it is not typed: it is raised to the recess's depth plus the
// drill point when the recess would reach it. The definition keeps the
// values as plain values and names a clearance hole's size and fit
// (`clearance`). Logged for UI tests (app/COMMANDS.md, Hole).

// Hole Tap Type Clearance, and its Screw Size and Fit inputs.
extern const QString kClearance;
extern const QString kScrewSize;
extern const QString kFit;

// A custom thread (mitcad#893): Thread Type Custom, and its Major Diameter
// input.
extern const QString kCustom;
extern const QString kThreadMajor;

bool customThread(const CommandState& state);

// A size of the `hole_sizes` table: its clearance holes by fit, its
// counterbore (a counterdrill's too) and countersink; NaN where it has
// none.
struct HoleSize {
  QString size;
  QHash<QString, double> clearance;
  double cbDiameter = std::numeric_limits<double>::quiet_NaN();
  double cbDepth = std::numeric_limits<double>::quiet_NaN();
  double cdAngle = std::numeric_limits<double>::quiet_NaN();
  double csDiameter = std::numeric_limits<double>::quiet_NaN();
  double csAngle = std::numeric_limits<double>::quiet_NaN();
};

struct HoleSizeTable {
  QVector<HoleSize> sizes;
  QVector<QPair<QString, QString>> fits{{QStringLiteral("fine"), QStringLiteral("Fine")},
                                        {QStringLiteral("normal"), QStringLiteral("Normal")},
                                        {QStringLiteral("coarse"), QStringLiteral("Coarse")}};
  QString defaultFit = QStringLiteral("normal");
  QString defaultSize = QStringLiteral("M6");
};

// The table, read once.
const HoleSizeTable& holeSizeTable(const CommandContext& context);
const HoleSize* holeSizeOf(const CommandContext& context, const QString& size);

// What the size gives the fields in a state: the values (mm and degrees)
// of a clearance hole's Diameter and each recess's (all of them, the
// hidden types' too), a tapped hole's countersink and, where its thread's
// size has a screw, its counterbore and counterdrill; none for a simple
// hole. `missing`: the fields the size has no value for; `size` names it
// ("M24"); `bore`: a tapped hole's bore (its thread's minor diameter, NaN
// when unknown).
struct SizedFields {
  QVector<QPair<QString, double>> values;
  QStringList missing;
  QString size;
  double bore = std::numeric_limits<double>::quiet_NaN();
};
SizedFields sizedFields(const CommandState& state, const CommandContext& model);

// Fills the fields the size gives but those typed and those holding an
// expression (both then marked), clears and marks those it has no value
// for, and raises an untyped Depth to fit the recess; the log says which.
void fillSized(CommandState& state, const CommandContext& model);
// A new Hole Tap Type: every field follows the size again (but
// expressions) and is filled.
void followSizeAgain(CommandState& state, const CommandContext& model);
// A value typed in a field the size fills: it stays when the size changes.
std::function<void(CommandState&, const CommandContext&)> markTyped(const QString& id);
// A Depth typed (or dragged): kept when a recess would reach it.
void markDepthTyped(CommandState& state, const CommandContext& model);
// A hole loaded for editing: the fields whose stored values its size does
// not give (or expressions) count as typed, and so does its Depth.
void markLoaded(const QJsonObject& feature, CommandState& state, const CommandContext& context);

// A field's mark in a state: typed, or no standard value; none while it
// follows the size or the size does not fill it.
FieldMark sizeMark(const CommandState& state, const QString& id);
// Reset to size: the field follows the size again and is filled from it.
void resetToSize(const QString& id, CommandState& state, const CommandContext& model);

// A value input the size fills: marked typed when typed in, its mark and
// Reset to size.
InputDef sizedValueInput(const QString& id, const QString& label, ValueKind kind, const QString& defaultText);

} // namespace mitcad::cmd
