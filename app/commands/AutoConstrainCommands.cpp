// SPDX-License-Identifier: MIT
// Auto Constrain (mitcad#2019): the sketch's Constraints group's command
// that makes a sketch fully constrained, first with the relations its
// geometry has (within the panel's tolerances), then with dimensions of the
// values it has for what is left free. It acts on the curves and points
// selected, or on the whole sketch when nothing is. Its definition is
// `sketch.auto_constrain` (core/model-api/src/commands.md, "Auto Constrain
// of a sketch"): the preview runs it, so the panel says what it would add
// before OK, and OK keeps it as one undo step; the status line then says
// what it added.
#include "Commands.hpp"

#include <cmath>
#include <memory>

#include <QDebug>
#include <QJsonArray>
#include <QJsonObject>
#include <QObject>

#include "../framework/Command.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/NoticeLevel.hpp"
#include "../sketch/SketchController.hpp"

namespace mitcad {
namespace {

using sketch::SketchController;

constexpr double kPi = 3.14159265358979323846;

// What a preview's answer added, in words: "12 constraints, 5 dimensions;
// the sketch is fully constrained", or the degrees of freedom left. Empty
// for no answer.
QString summaryOf(const QJsonObject& answer) {
  if (answer.isEmpty()) {
    return {};
  }
  const int relations = answer.value(QStringLiteral("relations")).toInt();
  const int dimensions = static_cast<int>(answer.value(QStringLiteral("dimensions")).toArray().size());
  const int dof = answer.value(QStringLiteral("dof")).toInt();
  const QString constraints = QObject::tr("%n constraint(s)", nullptr, relations);
  const QString dimensioned = QObject::tr("%n dimension(s)", nullptr, dimensions);
  const QString left = dof == 0 ? QObject::tr("the sketch is fully constrained")
                                : QObject::tr("%n degree(s) of freedom left", nullptr, dof);
  return QObject::tr("%1, %2; %3", "Auto Constrain: constraints, dimensions; what is left")
      .arg(constraints, dimensioned, left);
}

// The same for the log, in English and with the types: "Auto Constrain
// added: coincident 4, horizontal 2, length 2 (dof 0)".
QString logOf(const QJsonObject& answer) {
  QStringList parts;
  const QJsonObject added = answer.value(QStringLiteral("added")).toObject();
  for (auto it = added.begin(); it != added.end(); ++it) {
    parts << QStringLiteral("%1 %2").arg(it.key()).arg(it.value().toInt());
  }
  return QStringLiteral("Auto Constrain added: %1 (dof %2)")
      .arg(parts.isEmpty() ? QStringLiteral("nothing") : parts.join(QStringLiteral(", ")))
      .arg(answer.value(QStringLiteral("dof")).toInt());
}

void addAutoConstrain(CommandRegistry& registry, SketchController& c) {
  CommandDef def;
  def.id = QStringLiteral("sketch.auto_constrain");
  def.name = QObject::tr("Auto Constrain");
  def.icon = QStringLiteral("auto-constrain");
  def.kind = CommandDef::Kind::Feature;
  def.mode = CommandDef::Mode::Sketch;
  def.sketchCommand = true;
  def.tooltip = QObject::tr("Adds the relations the sketch has, then dimensions until it is fully constrained");
  def.keywords = {QStringLiteral("constraint"), QStringLiteral("automatic"), QStringLiteral("fully constrained"),
                  QStringLiteral("dimension"), QStringLiteral("infer")};
  const auto relationsOn = [](const CommandState& s) { return s.checked(QStringLiteral("relations")); };
  def.inputs = {
      selectionInput(QStringLiteral("objects"), QObject::tr("Objects"),
                     SelectKind::SketchCurve | SelectKind::SketchPoint, 0, 0)
          .withAccepts([&c](const SelectionItem& item) {
            return item.owner == c.uid() && c.model().text(item.name) == nullptr;
          })
          .withTooltip(QObject::tr("The curves and points to constrain; none: the whole sketch")),
      checkInput(QStringLiteral("relations"), QObject::tr("Relations"), true)
          .withTooltip(QObject::tr("Adds the relations the geometry already has within the tolerances")),
      checkInput(QStringLiteral("dimensions"), QObject::tr("Dimensions"), true)
          .withTooltip(QObject::tr("Adds dimensions with the current values for what is still free")),
      valueInput(QStringLiteral("length_tolerance"), QObject::tr("Length Tolerance"), ValueKind::Length,
                 QStringLiteral("0.01 mm"))
          .withVisible(relationsOn)
          .withTooltip(QObject::tr("How far apart two places may be and still count as one, and how far a "
                                   "point may move")),
      valueInput(QStringLiteral("angle_tolerance"), QObject::tr("Angle Tolerance"), ValueKind::Angle,
                 QStringLiteral("0.5 deg"))
          .withVisible(relationsOn)
          .withTooltip(QObject::tr("How far from level, square or parallel a direction may be")),
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    const bool relations = s.checked(QStringLiteral("relations"));
    const bool dimensions = s.checked(QStringLiteral("dimensions"));
    if (!relations && !dimensions) {
      return Built::failure(QObject::tr("Check Relations, Dimensions or both."), QStringLiteral("relations"));
    }
    QJsonObject cmd{{QStringLiteral("cmd"), QStringLiteral("sketch.auto_constrain")},
                    {QStringLiteral("sketch"), c.uid()},
                    {QStringLiteral("relations"), relations},
                    {QStringLiteral("dimensions"), dimensions}};
    QStringList objects;
    for (const SelectionItem& item : s.items(QStringLiteral("objects"))) {
      objects << item.name;
    }
    if (!objects.isEmpty()) {
      cmd.insert(QStringLiteral("entities"), QJsonArray::fromStringList(objects));
    }
    if (relations) {
      const double length = s.value(QStringLiteral("length_tolerance"));
      if (!(length > 0.0)) {
        return Built::failure(QObject::tr("The length tolerance must be greater than zero."),
                              QStringLiteral("length_tolerance"));
      }
      // The panel's angle is in radians; the command takes degrees.
      const double degrees = s.value(QStringLiteral("angle_tolerance")) * 180.0 / kPi;
      if (!(degrees > 0.0 && degrees < 45.0)) {
        return Built::failure(QObject::tr("The angle tolerance must be greater than zero and less than 45 degrees."),
                              QStringLiteral("angle_tolerance"));
      }
      cmd.insert(QStringLiteral("length_tolerance"), length);
      cmd.insert(QStringLiteral("angle_tolerance"), degrees);
    }
    return Built::of(cmd);
  };
  // The last answer of a preview that applied: the panel's note says what
  // it adds, and the status line says it again after OK. Kept while the
  // preview is taken back (before OK applies the command again).
  const auto shown = std::make_shared<QJsonObject>();
  def.sketchPreviewShown = [shown](const QJsonObject& answer) {
    if (!answer.isEmpty()) {
      *shown = answer;
      qDebug().noquote() << QStringLiteral("Preview %1").arg(logOf(answer));
    }
  };
  def.note = [shown](const CommandState&, const CommandContext&) { return summaryOf(*shown); };
  def.describe = [shown](const CommandState&, const CommandContext&) { return logOf(*shown); };
  def.ended = [&c, shown](bool committed) {
    if (committed && !shown->isEmpty()) {
      c.host().showStatus(summaryOf(*shown), NoticeLevel::Success);
    }
    *shown = QJsonObject();
  };
  registry.add(def);
}

} // namespace

void registerAutoConstrainCommands(CommandRegistry& registry, sketch::SketchController& controller) {
  addAutoConstrain(registry, controller);
}

} // namespace mitcad
