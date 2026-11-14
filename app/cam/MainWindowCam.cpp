// SPDX-License-Identifier: MIT
// CAM in the application (mitcad#1013, docs/cam.md "UI", app/COMMANDS.md
// "The CNC group"): the Make tab's CNC group and what it drives.
//
//   - New Setup and the operations' panels (Facing, Contour, Pocket,
//     Adaptive Clearing, Rest Machining, Drill; 3D Roughing, Raster
//     Finishing, Waterline Finishing on the bodies' surfaces): application
//     panels whose
//     inputs are picked and typed as a feature's, OK running the model's
//     cam_setup_add / cam_operation_add (or _edit when a Browser row opened
//     the panel), one undo step each; values with units ("40 in" a feed of
//     40 inches per minute) go to the model as they are typed;
//   - Generate (cam_generate), Simulate (the toolpaths played back with the
//     tool drawn), Post Process… (cam_post written through the save
//     dialog) and Machines and Tools… (cam/CamMachinesDialog.hpp);
//   - the toolpaths in the 3D view (OcctViewer::setCamToolpaths) from
//     cam_toolpaths, in each setup's work frame, hidden with the setup's
//     eye in the Browser's CAM folder, the current operation highlighted;
//   - the Browser's CAM rows' actions (DocumentHost: editCamSetup, …).
//
// Messages a person sees say what is wrong in words; the log lines carry
// the names and numbers UI tests read.
#include "../MainWindow.hpp"
#include "../framework/Json.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <functional>
#include <initializer_list>
#include <memory>
#include <optional>
#include <utility>

#include <QComboBox>
#include <QDialogButtonBox>
#include <QDir>
#include <QFileInfo>
#include <QFormLayout>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QLabel>
#include <QList>
#include <QPalette>
#include <QPushButton>
#include <QSaveFile>
#include <QStringList>
#include <QVBoxLayout>
#include <QtLogging>

#include "../browser/BrowserController.hpp"
#include "../commands/CommandSupport.hpp"
#include "../framework/Command.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/DialogFolders.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/ModelMessages.hpp"
#include "../framework/NoticeLevel.hpp"
#include "../framework/UserText.hpp"
#include "CamMachinesDialog.hpp"
#include "CamSimulateDialog.hpp"
#include "CamToolpaths.hpp"

namespace mitcad {
namespace {

using Choices = QVector<QPair<QString, QString>>;

const QString kMake = QStringLiteral("MAKE");
const QString kCnc = QStringLiteral("CNC");
// A choice that stands for no machine, and for the machine's controller.
const QString kNoMachine = QStringLiteral("__none");
const QString kMachineDialect = QStringLiteral("__machine");

int idOf(const QJsonObject& object) { return object.value(QStringLiteral("id")).toInt(); }

bool editing(const CommandState& state) { return !state.object(QStringLiteral("edited")).isEmpty(); }

// The operation kinds, by the model's `type`: their commands, names,
// icons and tooltips.
struct OperationKind {
  const char* type;
  const char* icon;
};

constexpr OperationKind kOperationKinds[] = {
    {"facing", "cnc-facing"}, {"contour", "cnc-contour"}, {"pocket", "cnc-pocket"},
    {"adaptive", "cnc-adaptive"}, {"rest", "cnc-rest"},   {"drill", "cnc-drill"},
    // 3-axis machining of the bodies' surfaces (mitcad#1013, phase 3).
    {"roughing_3d", "cnc-roughing-3d"}, {"raster_finish", "cnc-raster"}, {"waterline_finish", "cnc-waterline"},
};

QString operationName(const QString& type) {
  if (type == QStringLiteral("facing")) return MainWindow::tr("Facing");
  if (type == QStringLiteral("contour")) return MainWindow::tr("Contour");
  if (type == QStringLiteral("pocket")) return MainWindow::tr("Pocket");
  if (type == QStringLiteral("adaptive")) return MainWindow::tr("Adaptive Clearing");
  if (type == QStringLiteral("rest")) return MainWindow::tr("Rest Machining");
  if (type == QStringLiteral("roughing_3d")) return MainWindow::tr("3D Roughing");
  if (type == QStringLiteral("raster_finish")) return MainWindow::tr("Raster Finishing");
  if (type == QStringLiteral("waterline_finish")) return MainWindow::tr("Waterline Finishing");
  return MainWindow::tr("Drill");
}

QString operationTooltip(const QString& type) {
  if (type == QStringLiteral("facing")) return MainWindow::tr("Adds rows that face the stock's top flat");
  if (type == QStringLiteral("contour"))
    return MainWindow::tr("Adds a cut along outlines: outside, inside or on the line");
  if (type == QStringLiteral("pocket")) return MainWindow::tr("Adds offset loops that clear an area with its islands");
  if (type == QStringLiteral("adaptive")) return MainWindow::tr("Adds clearing that keeps the tool's engagement even");
  if (type == QStringLiteral("rest")) return MainWindow::tr("Adds clearing of what a bigger tool left in an area");
  if (type == QStringLiteral("roughing_3d"))
    return MainWindow::tr("Adds clearing of the stock round the bodies, level by level");
  if (type == QStringLiteral("raster_finish"))
    return MainWindow::tr("Adds parallel finishing passes over the bodies' surfaces");
  if (type == QStringLiteral("waterline_finish"))
    return MainWindow::tr("Adds finishing passes round the bodies' steep walls, level by level");
  return MainWindow::tr("Adds holes at points, in one go or in pecks");
}

QString commandOf(const QString& type) { return QStringLiteral("make.cam_") + type; }

bool clears(const QString& type) {
  return type == QStringLiteral("pocket") || type == QStringLiteral("adaptive") || type == QStringLiteral("rest");
}

// The 3-axis operations: they machine the bodies' surfaces (the model's
// `body` inputs) within a boundary of faces or profiles.
bool machinesSurfaces(const QString& type) {
  return type == QStringLiteral("roughing_3d") || type == QStringLiteral("raster_finish") ||
         type == QStringLiteral("waterline_finish");
}

// The panels' data on the design (CamPanelData): its setups, machines with
// their tools and the dialects, kept in the state as "cam".
QJsonObject camData(const CommandState& state) { return state.object(QStringLiteral("cam")); }

QJsonObject setupOf(const QJsonObject& data, int id) {
  for (const QJsonValue& value : data.value(QStringLiteral("setups")).toArray()) {
    if (idOf(value.toObject()) == id) {
      return value.toObject();
    }
  }
  return {};
}

QJsonObject machineOf(const QJsonObject& data, const QString& id) {
  for (const QJsonValue& value : data.value(QStringLiteral("machines")).toArray()) {
    if (str(value.toObject(), "id") == id) {
      return value.toObject();
    }
  }
  return {};
}

QJsonArray toolsOf(const QJsonObject& data, int setup) {
  return machineOf(data, str(setupOf(data, setup), "machine"))
      .value(QStringLiteral("tools"))
      .toObject()
      .value(QStringLiteral("tools"))
      .toArray();
}

Choices setupChoices(const QJsonObject& data) {
  Choices choices;
  for (const QJsonValue& value : data.value(QStringLiteral("setups")).toArray()) {
    choices.append(qMakePair(QString::number(idOf(value.toObject())), str(value.toObject(), "name")));
  }
  return choices;
}

QString toolLabel(const QJsonObject& tool) {
  return QStringLiteral("%1: %2").arg(tool.value(QStringLiteral("number")).toInt()).arg(str(tool, "name"));
}

Choices toolChoices(const QJsonObject& data, int setup) {
  Choices choices;
  for (const QJsonValue& value : toolsOf(data, setup)) {
    choices.append(qMakePair(QString::number(value.toObject().value(QStringLiteral("number")).toInt()), toolLabel(value.toObject())));
  }
  return choices;
}

// The tool an operation starts with: a drill for drilling, a ball end mill
// for raster and waterline finishing, an end mill (flat, bull nose, ball;
// not a ball for 3D roughing) for the others; else the first.
QString defaultTool(const QJsonObject& data, int setup, const QString& type) {
  const QJsonArray tools = toolsOf(data, setup);
  if (type == QStringLiteral("raster_finish") || type == QStringLiteral("waterline_finish")) {
    for (const QJsonValue& value : tools) {
      if (str(value.toObject().value(QStringLiteral("shape")).toObject(), "kind") == QStringLiteral("ball")) {
        return QString::number(value.toObject().value(QStringLiteral("number")).toInt());
      }
    }
  }
  const bool ballAllowed = type != QStringLiteral("roughing_3d");
  for (const QJsonValue& value : tools) {
    const QString kind = str(value.toObject().value(QStringLiteral("shape")).toObject(), "kind");
    const bool drill = kind == QStringLiteral("drill");
    const bool mill = kind == QStringLiteral("flat") || kind == QStringLiteral("bull") ||
                      (ballAllowed && kind == QStringLiteral("ball"));
    if (type == QStringLiteral("drill") ? drill : mill) {
      return QString::number(value.toObject().value(QStringLiteral("number")).toInt());
    }
  }
  return tools.isEmpty() ? QString()
                         : QString::number(tools.first().toObject().value(QStringLiteral("number")).toInt());
}

Choices machineChoices(const QJsonObject& data) {
  Choices choices{{kNoMachine, MainWindow::tr("None")}};
  for (const QJsonValue& value : data.value(QStringLiteral("machines")).toArray()) {
    const QJsonObject machine = value.toObject();
    const QJsonObject body = machine.value(QStringLiteral("machine")).toObject();
    choices.append(qMakePair(str(machine, "id"), QStringLiteral("%1 (%2)").arg(str(body, "name"), str(body, "controller"))));
  }
  return choices;
}

Choices dialectChoices(const QJsonObject& data) {
  Choices choices{{kMachineDialect, MainWindow::tr("The machine's controller")}};
  for (const QJsonValue& value : data.value(QStringLiteral("dialects")).toArray()) {
    choices.append(qMakePair(str(value.toObject(), "id"), str(value.toObject(), "label")));
  }
  return choices;
}

// The dialect a setup panel's program is written in: the one chosen, else
// the machine's controller.
QString effectiveDialect(const CommandState& state) {
  const QString dialect = state.choice(QStringLiteral("dialect"));
  if (!dialect.isEmpty() && dialect != kMachineDialect) {
    return dialect;
  }
  const QJsonObject machine = machineOf(camData(state), state.choice(QStringLiteral("machine")));
  return str(machine.value(QStringLiteral("machine")).toObject(), "controller");
}

// A value input as the model takes it (a CamValue): an expression with its
// unit as typed ("40 in", "d1 + 2 mm"), a plain number for a unitless one;
// null when left empty. Why not, when it does not evaluate.
QJsonValue camValue(const CommandState& state, const QString& id, ValueKind kind, const QString& label, QString* why) {
  if (state.text(id).trimmed().isEmpty()) {
    return QJsonValue(QJsonValue::Null);
  }
  const double value = state.value(id);
  if (!std::isfinite(value)) {
    *why = MainWindow::tr("%1: check the value.").arg(label);
    return QJsonValue(QJsonValue::Null);
  }
  if (kind == ValueKind::Unitless) {
    return value;
  }
  return state.expression(id);
}

// A value of the model's JSON in a value input: an expression as it is, a
// number (millimetres, degrees) in the design's unit.
void loadCamValue(CommandState& state, const QString& id, const QJsonValue& value, ValueKind kind,
                  const CommandContext& context) {
  if (value.isString()) {
    state.setText(id, value.toString());
  } else if (value.isDouble()) {
    switch (kind) {
    case ValueKind::Length:
      state.setText(id, cmd::lengthText(value.toDouble(), context));
      break;
    case ValueKind::Angle:
      state.setText(id, cam::shortNumber(value.toDouble()) + QStringLiteral(" deg"));
      break;
    case ValueKind::Unitless:
      state.setText(id, cam::shortNumber(value.toDouble()));
      break;
    }
  }
}

// A model input or reference with the occurrence its pick was in (the
// model's `occurrence`, "O1/O4"; left out in the root, mitcad#1013), so
// that a pick inside a placed component is computed where it is placed.
QJsonObject withOccurrence(QJsonObject input, const QString& occurrence, const char* key = "occurrence") {
  if (!occurrence.isEmpty()) {
    input.insert(QLatin1String(key), occurrence);
  }
  return input;
}

// An operation's geometric inputs (the model's `inputs`) from its picks:
// a face, a sketch profile, the edges of each body, the curves of each
// sketch (each in its occurrence: a component placed twice gives two
// inputs); for drilling, each pick a point.
QJsonArray camInputs(const Selection& items, bool points) {
  QJsonArray inputs;
  if (points) {
    for (const SelectionItem& item : items) {
      inputs.append(withOccurrence(
          QJsonObject{{QStringLiteral("kind"), QStringLiteral("point")}, {QStringLiteral("at"), item.reference()}},
          item.occurrence));
    }
    return inputs;
  }
  // The bodies of edges and the sketches of curves, each with its
  // occurrence, in the order first picked.
  QList<std::pair<QString, QString>> edgeBodies;
  QList<std::pair<QString, QString>> curveSketches;
  for (const SelectionItem& item : items) {
    const std::pair<QString, QString> owner{item.owner, item.occurrence};
    if (item.kind == SelectKind::Face) {
      inputs.append(withOccurrence(QJsonObject{{QStringLiteral("kind"), QStringLiteral("face")},
                                               {QStringLiteral("body"), item.owner},
                                               {QStringLiteral("face"), item.name}},
                                   item.occurrence));
    } else if (item.kind == SelectKind::Profile) {
      inputs.append(withOccurrence(QJsonObject{{QStringLiteral("kind"), QStringLiteral("region")},
                                               {QStringLiteral("sketch"), item.owner},
                                               {QStringLiteral("region"), item.name}},
                                   item.occurrence));
    } else if (item.kind == SelectKind::Edge && !edgeBodies.contains(owner)) {
      edgeBodies << owner;
    } else if (item.kind == SelectKind::SketchCurve && !curveSketches.contains(owner)) {
      curveSketches << owner;
    }
  }
  for (const auto& [body, occurrence] : std::as_const(edgeBodies)) {
    QJsonArray edges;
    for (const SelectionItem& item : items) {
      if (item.kind == SelectKind::Edge && item.owner == body && item.occurrence == occurrence) {
        edges.append(item.name);
      }
    }
    inputs.append(withOccurrence(QJsonObject{{QStringLiteral("kind"), QStringLiteral("edges")},
                                             {QStringLiteral("body"), body},
                                             {QStringLiteral("edges"), edges}},
                                 occurrence));
  }
  for (const auto& [sketch, occurrence] : std::as_const(curveSketches)) {
    QJsonArray curves;
    for (const SelectionItem& item : items) {
      if (item.kind == SelectKind::SketchCurve && item.owner == sketch && item.occurrence == occurrence) {
        curves.append(item.name);
      }
    }
    inputs.append(withOccurrence(QJsonObject{{QStringLiteral("kind"), QStringLiteral("curves")},
                                             {QStringLiteral("sketch"), sketch},
                                             {QStringLiteral("curves"), curves}},
                                 occurrence));
  }
  return inputs;
}

// A reference's pick (a point, a plane, an axis) as an item in its
// occurrence; an invalid item when it names nothing.
SelectionItem camItemOf(const QJsonValue& at, const QString& occurrence, const CommandContext& context) {
  SelectionItem item = cmd::itemOf(at, context);
  if (item.isValid()) {
    item.occurrence = occurrence;
  }
  return item;
}

// The picks of an operation's inputs, to edit it (the stock has none),
// each in the occurrence it was picked in.
Selection camItems(const QJsonArray& inputs, const CommandContext& context) {
  Selection items;
  for (const QJsonValue& value : inputs) {
    const QJsonObject input = value.toObject();
    const QString kind = str(input, "kind");
    const QString occurrence = str(input, "occurrence");
    if (kind == QStringLiteral("face")) {
      items.append(SelectionItem{SelectKind::Face, str(input, "body"), str(input, "face"), QStringLiteral("plane"),
                                 occurrence});
    } else if (kind == QStringLiteral("region")) {
      items.append(SelectionItem{SelectKind::Profile, str(input, "sketch"), str(input, "region"),
                                 QStringLiteral("plane"), occurrence});
    } else if (kind == QStringLiteral("edges")) {
      for (const QJsonValue& edge : input.value(QStringLiteral("edges")).toArray()) {
        items.append(SelectionItem{SelectKind::Edge, str(input, "body"), edge.toString(), QString(), occurrence});
      }
    } else if (kind == QStringLiteral("curves")) {
      for (const QJsonValue& curve : input.value(QStringLiteral("curves")).toArray()) {
        items.append(
            SelectionItem{SelectKind::SketchCurve, str(input, "sketch"), curve.toString(), QString(), occurrence});
      }
    } else if (kind == QStringLiteral("point")) {
      const QJsonValue at = input.value(QStringLiteral("at"));
      const QJsonObject ref = at.toObject();
      // A face's or an edge's reference stays a face or an edge (a hole's
      // cylinder, a circular edge's centre); itemOf names the rest.
      if (ref.contains(QStringLiteral("face"))) {
        items.append(SelectionItem{SelectKind::Face, str(ref, "body"), str(ref, "face"), QStringLiteral("cylinder"),
                                   occurrence});
      } else if (ref.contains(QStringLiteral("edge"))) {
        items.append(SelectionItem{SelectKind::Edge, str(ref, "body"), str(ref, "edge"), QStringLiteral("circle"),
                                   occurrence});
      } else {
        const SelectionItem item = camItemOf(at, occurrence, context);
        if (item.isValid()) {
          items.append(item);
        }
      }
    }
  }
  return items;
}

// The fields of cam_setup_add (and cam_setup_edit's changes) from a setup
// panel; empty with `why` (and its input) when they are not complete.
QJsonObject setupFields(const CommandState& state, QString* why, QString* input) {
  const bool edit = editing(state);
  QJsonObject setup;
  const QString name = state.text(QStringLiteral("name")).trimmed();
  if (!name.isEmpty()) {
    setup.insert(QStringLiteral("name"), name);
  }
  // Each body as its occurrence places it: the id alone in the root.
  QJsonArray bodies;
  for (const SelectionItem& item : state.items(QStringLiteral("bodies"))) {
    if (item.occurrence.isEmpty()) {
      bodies.append(item.owner);
    } else {
      bodies.append(withOccurrence(QJsonObject{{QStringLiteral("body"), item.owner}}, item.occurrence));
    }
  }
  if (!bodies.isEmpty() || edit) {
    setup.insert(QStringLiteral("bodies"), bodies);
  }
  const auto value = [&](const char* id, ValueKind kind, const QString& label) {
    QString problem;
    const QJsonValue v = camValue(state, QString::fromLatin1(id), kind, label, &problem);
    if (!problem.isEmpty() && why->isEmpty()) {
      *why = problem;
      *input = QString::fromLatin1(id);
    }
    return v;
  };
  if (state.choice(QStringLiteral("stock")) == QStringLiteral("body")) {
    const Selection& body = state.items(QStringLiteral("stock_body"));
    if (body.isEmpty()) {
      *why = MainWindow::tr("Pick the body the stock is.");
      *input = QStringLiteral("stock_body");
      return {};
    }
    setup.insert(QStringLiteral("stock"), withOccurrence(QJsonObject{{QStringLiteral("kind"), QStringLiteral("body")},
                                                                     {QStringLiteral("body"), body.first().owner}},
                                                         body.first().occurrence));
  } else {
    QJsonObject stock{{QStringLiteral("kind"), QStringLiteral("around_bodies")}};
    const std::pair<const char*, QString> margins[] = {{"side", MainWindow::tr("Side Margin")},
                                                       {"top", MainWindow::tr("Top Margin")},
                                                       {"bottom", MainWindow::tr("Bottom Margin")}};
    for (const auto& [key, label] : margins) {
      const QJsonValue margin = value(key, ValueKind::Length, label);
      if (!margin.isNull()) {
        stock.insert(QLatin1String(key), margin);
      }
    }
    setup.insert(QStringLiteral("stock"), stock);
  }
  QJsonObject wcs;
  if (state.choice(QStringLiteral("origin")) == QStringLiteral("point")) {
    const Selection& point = state.items(QStringLiteral("origin_point"));
    if (point.isEmpty()) {
      *why = MainWindow::tr("Pick the work zero: a vertex, a point or a circular edge's center.");
      *input = QStringLiteral("origin_point");
      return {};
    }
    wcs.insert(QStringLiteral("origin"), withOccurrence(QJsonObject{{QStringLiteral("kind"), QStringLiteral("point")},
                                                                    {QStringLiteral("at"), point.first().reference()}},
                                                        point.first().occurrence));
  } else {
    wcs.insert(QStringLiteral("origin"), QJsonObject{{QStringLiteral("kind"), QStringLiteral("stock")},
                                                     {QStringLiteral("corner"), state.choice(QStringLiteral("origin"))}});
  }
  // The axes' picks with their occurrences (`z_occurrence`, `x_occurrence`).
  if (!state.items(QStringLiteral("z")).isEmpty()) {
    const SelectionItem& z = state.items(QStringLiteral("z")).first();
    wcs.insert(QStringLiteral("z"), z.reference());
    wcs = withOccurrence(wcs, z.occurrence, "z_occurrence");
  }
  if (!state.items(QStringLiteral("x")).isEmpty()) {
    const SelectionItem& x = state.items(QStringLiteral("x")).first();
    wcs.insert(QStringLiteral("x"), x.reference());
    wcs = withOccurrence(wcs, x.occurrence, "x_occurrence");
  }
  setup.insert(QStringLiteral("wcs"), wcs);
  setup.insert(QStringLiteral("work_offset"), std::clamp(state.choice(QStringLiteral("work_offset")).toInt(), 1, 6));
  const QJsonValue safe = value("safe_height", ValueKind::Length, MainWindow::tr("Safe Height"));
  if (!safe.isNull()) {
    setup.insert(QStringLiteral("safe_height"), safe);
  }
  const QString machine = state.choice(QStringLiteral("machine"));
  if (machine.isEmpty() || machine == kNoMachine) {
    if (edit) {
      setup.insert(QStringLiteral("machine"), QJsonValue(QJsonValue::Null));
    }
  } else {
    setup.insert(QStringLiteral("machine"), machine);
  }
  const QString dialect = state.choice(QStringLiteral("dialect"));
  if (dialect.isEmpty() || dialect == kMachineDialect) {
    if (edit) {
      setup.insert(QStringLiteral("dialect"), QJsonValue(QJsonValue::Null));
    }
  } else {
    setup.insert(QStringLiteral("dialect"), dialect);
  }
  // The post settings as kept (arcs, line numbers, …), the units chosen.
  QJsonObject post = state.object(QStringLiteral("post"));
  post.insert(QStringLiteral("units"), state.choice(QStringLiteral("units")) == QStringLiteral("inches")
                                           ? QStringLiteral("inches")
                                           : QStringLiteral("millimetres"));
  setup.insert(QStringLiteral("post"), post);
  setup.insert(QStringLiteral("probe_work_zero"),
               effectiveDialect(state) == QStringLiteral("millenniumos") && state.checked(QStringLiteral("probe")));
  if (!why->isEmpty()) {
    return {};
  }
  return setup;
}

// The fields of cam_operation_add's `operation` (and cam_operation_edit's
// changes) of `type` from its panel; empty with `why` when not complete.
QJsonObject operationFields(const CommandState& state, const QString& type, QString* why, QString* input) {
  const bool edit = editing(state);
  const bool drill = type == QStringLiteral("drill");
  const bool contour = type == QStringLiteral("contour");
  const bool surface = machinesSurfaces(type);
  const bool raster = type == QStringLiteral("raster_finish");
  const bool roughing = type == QStringLiteral("roughing_3d");
  QJsonObject op;
  const QString name = state.text(QStringLiteral("name")).trimmed();
  if (!name.isEmpty()) {
    op.insert(QStringLiteral("name"), name);
  }
  QJsonObject kind{{QStringLiteral("type"), type}};
  if (contour) {
    kind.insert(QStringLiteral("side"), state.choice(QStringLiteral("side")));
  } else if (drill) {
    kind.insert(QStringLiteral("peck"), state.checked(QStringLiteral("peck")));
    kind.insert(QStringLiteral("canned_cycle"), state.checked(QStringLiteral("canned")));
  } else if (type == QStringLiteral("rest")) {
    const QString from = state.choice(QStringLiteral("from_tool"));
    if (!from.isEmpty()) {
      kind.insert(QStringLiteral("from_tool"), from.toInt());
    }
  } else if (roughing && state.checked(QStringLiteral("adaptive"))) {
    kind.insert(QStringLiteral("adaptive"), true);
  }
  op.insert(QStringLiteral("kind"), kind);
  const QString tool = state.choice(QStringLiteral("tool"));
  if (tool.isEmpty()) {
    *why = MainWindow::tr("The setup's machine has no tools yet: add them with Machines and Tools….");
    *input = QStringLiteral("tool");
    return {};
  }
  op.insert(QStringLiteral("tool"), tool.toInt());
  const Selection& picks = state.items(QStringLiteral("geometry"));
  QJsonArray inputs = camInputs(picks, drill);
  // The bodies a 3-axis operation machines (none: the setup's), each in
  // its occurrence; its picks are its boundary (none: the stock).
  if (surface) {
    for (const SelectionItem& item : state.items(QStringLiteral("bodies"))) {
      inputs.append(withOccurrence(
          QJsonObject{{QStringLiteral("kind"), QStringLiteral("body")}, {QStringLiteral("body"), item.owner}},
          item.occurrence));
    }
  }
  if (inputs.isEmpty() && !surface) {
    if (type != QStringLiteral("facing")) {
      *why = drill ? MainWindow::tr("Pick the holes: points, vertices, round edges or holes' faces.")
                   : MainWindow::tr("Pick what to cut: faces, sketch profiles, edges or sketch curves.");
      *input = QStringLiteral("geometry");
      return {};
    }
    inputs.append(QJsonObject{{QStringLiteral("kind"), QStringLiteral("stock")}});
  }
  op.insert(QStringLiteral("inputs"), inputs);
  const auto value = [&](const char* id, ValueKind valueKind, const QString& label) {
    QString problem;
    const QJsonValue v = camValue(state, QString::fromLatin1(id), valueKind, label, &problem);
    if (!problem.isEmpty() && why->isEmpty()) {
      *why = problem;
      *input = QString::fromLatin1(id);
    }
    return v;
  };
  // An object of the values given; an edit sends it whole (its empty
  // values back to their defaults).
  const auto group = [&](const char* key, std::initializer_list<std::pair<const char*, std::pair<ValueKind, QString>>> fields) {
    QJsonObject out;
    for (const auto& [id, about] : fields) {
      const QJsonValue v = value(id, about.first, about.second);
      if (!v.isNull()) {
        out.insert(QLatin1String(id), v);
      }
    }
    if (!out.isEmpty() || edit) {
      op.insert(QLatin1String(key), out);
    }
  };
  group("depths", {{"top", {ValueKind::Length, MainWindow::tr("Top")}},
                   {"bottom", {ValueKind::Length, MainWindow::tr("Bottom")}},
                   {"step_down", {ValueKind::Length, MainWindow::tr("Step Down")}}});
  group("heights", {{"clearance", {ValueKind::Length, MainWindow::tr("Clearance Height")}},
                    {"retract", {ValueKind::Length, MainWindow::tr("Retract Height")}}});
  group("cutting", {{"spindle_rpm", {ValueKind::Unitless, MainWindow::tr("Spindle Speed (rpm)")}},
                    {"feed", {ValueKind::Length, MainWindow::tr("Feed per Minute")}},
                    {"plunge_feed", {ValueKind::Length, MainWindow::tr("Plunge Feed per Minute")}}});
  const auto optional = [&](const char* id, bool used, const QString& label) {
    const QJsonValue v = used ? value(id, ValueKind::Length, label) : QJsonValue(QJsonValue::Null);
    if (!v.isNull() || edit) {
      op.insert(QLatin1String(id), v);
    }
  };
  optional("stepover", type == QStringLiteral("facing") || clears(type) || roughing || raster,
           MainWindow::tr("Stepover"));
  optional("stock_to_leave", contour || clears(type) || surface, MainWindow::tr("Stock to Leave"));
  if (!drill && !raster) {
    op.insert(QStringLiteral("direction"), state.choice(QStringLiteral("direction")));
  }
  // The finishing kinds' angles: the passes' direction and the slope that
  // splits the surfaces between them (left out: every slope).
  if (surface && !roughing) {
    QJsonObject finish = op.value(QStringLiteral("kind")).toObject();
    if (raster) {
      const QJsonValue passAngle = value("angle", ValueKind::Angle, MainWindow::tr("Pass Angle"));
      if (!passAngle.isNull()) {
        finish.insert(QStringLiteral("angle"), passAngle);
      }
    }
    const QJsonValue slope =
        value("slope", ValueKind::Angle, raster ? MainWindow::tr("Steepest Slope") : MainWindow::tr("Shallowest Slope"));
    if (!slope.isNull()) {
      finish.insert(QStringLiteral("slope"), slope);
    }
    op.insert(QStringLiteral("kind"), finish);
  }
  op.insert(QStringLiteral("spindle"), state.choice(QStringLiteral("spindle")));
  op.insert(QStringLiteral("coolant"), state.choice(QStringLiteral("coolant")));
  if (contour || clears(type) || roughing) {
    const QString entry = state.choice(QStringLiteral("entry"));
    QJsonObject out{{QStringLiteral("kind"), entry.isEmpty() ? QStringLiteral("plunge") : entry}};
    if (entry == QStringLiteral("ramp")) {
      out.insert(QStringLiteral("angle"), value("ramp_angle", ValueKind::Angle, MainWindow::tr("Ramp Angle")));
    } else if (entry == QStringLiteral("helix")) {
      out.insert(QStringLiteral("angle"), value("helix_angle", ValueKind::Angle, MainWindow::tr("Helix Angle")));
      const QJsonValue radius = value("helix_radius", ValueKind::Length, MainWindow::tr("Helix Radius"));
      if (!radius.isNull()) {
        out.insert(QStringLiteral("radius"), radius);
      }
    }
    op.insert(QStringLiteral("entry"), out);
  } else if (edit) {
    op.insert(QStringLiteral("entry"), QJsonValue(QJsonValue::Null));
  }
  if (contour && state.checked(QStringLiteral("tabs"))) {
    const QJsonValue count = value("tab_count", ValueKind::Unitless, MainWindow::tr("Tab Count"));
    op.insert(QStringLiteral("tabs"),
              QJsonObject{{QStringLiteral("count"), std::max(1, static_cast<int>(std::lround(count.toDouble(4))))},
                          {QStringLiteral("width"), value("tab_width", ValueKind::Length, MainWindow::tr("Tab Width"))},
                          {QStringLiteral("height"), value("tab_height", ValueKind::Length, MainWindow::tr("Tab Height"))}});
  } else if (edit) {
    op.insert(QStringLiteral("tabs"), QJsonValue(QJsonValue::Null));
  }
  if (contour && state.checked(QStringLiteral("lead"))) {
    op.insert(QStringLiteral("lead"),
              QJsonObject{{QStringLiteral("radius"), value("lead_radius", ValueKind::Length, MainWindow::tr("Lead Radius"))}});
  } else if (edit) {
    op.insert(QStringLiteral("lead"), QJsonValue(QJsonValue::Null));
  }
  if (!why->isEmpty()) {
    return {};
  }
  return op;
}

} // namespace

// ---------------------------------------------------------------------------
// Registration

void MainWindow::registerCamCommands() {
  m_registry->add(camSetupCommand());
  for (const OperationKind& kind : kOperationKinds) {
    m_registry->add(camOperationCommand(QString::fromLatin1(kind.type)));
  }
  const auto action = [](const char* id, const QString& name, const char* icon, const QString& tooltip,
                         const char* section, std::function<void()> run) {
    CommandDef def;
    def.id = QString::fromLatin1(id);
    def.name = name;
    def.icon = QString::fromLatin1(icon);
    def.tooltip = tooltip;
    def.kind = CommandDef::Kind::Action;
    def.mode = CommandDef::Mode::Model;
    def.tab = kMake;
    def.group = kCnc;
    def.section = QString::fromLatin1(section);
    def.run = std::move(run);
    return def;
  };
  const auto hasSetups = [this] { return !m_snapshot.camSetups.isEmpty(); };
  CommandDef generate = action("make.cam_generate", tr("Generate"), "cnc-generate",
                               tr("Computes the toolpaths of the setups' operations"), "toolpaths",
                               [this] { generateCam(0); });
  generate.pinned = true;
  generate.enabled = hasSetups;
  generate.keywords = {QStringLiteral("cam"), QStringLiteral("cnc"), QStringLiteral("toolpath"),
                       QStringLiteral("generate"), QStringLiteral("compute")};
  m_registry->add(generate);
  CommandDef simulate = action("make.cam_simulate", tr("Simulate"), "cnc-simulate",
                               tr("Plays a setup's toolpaths back with the tool drawn where it is"), "toolpaths",
                               [this] { simulateCam(0); });
  simulate.enabled = hasSetups;
  simulate.keywords = {QStringLiteral("cam"), QStringLiteral("cnc"), QStringLiteral("toolpath"),
                       QStringLiteral("playback"), QStringLiteral("simulation")};
  m_registry->add(simulate);
  CommandDef post = action("make.cam_post", tr("Post Process…"), "cnc-post",
                           tr("Writes a setup's program as G-code for its machine's controller"), "toolpaths",
                           [this] { postProcessCam(0); });
  post.enabled = hasSetups;
  post.keywords = {QStringLiteral("cam"),  QStringLiteral("cnc"),   QStringLiteral("g-code"), QStringLiteral("gcode"),
                   QStringLiteral("post"), QStringLiteral("grbl"),  QStringLiteral("nc"),     QStringLiteral("program"),
                   QStringLiteral("export")};
  m_registry->add(post);
  CommandDef machines = action("make.cam_machines", tr("Machines and Tools…"), "cnc-machines",
                               tr("Edits the design's CNC machines and their tool libraries"), "machines",
                               [this] { openCamMachines(); });
  machines.keywords = {QStringLiteral("cam"),  QStringLiteral("cnc"),     QStringLiteral("machine"),
                       QStringLiteral("tool"), QStringLiteral("library"), QStringLiteral("end mill"),
                       QStringLiteral("router")};
  m_registry->add(machines);
}

QJsonObject MainWindow::camPanelData() const {
  QJsonArray setups;
  for (const QJsonValue& value : m_snapshot.camSetups) {
    const QJsonObject setup = value.toObject();
    setups.append(QJsonObject{{QStringLiteral("id"), idOf(setup)},
                              {QStringLiteral("name"), str(setup, "name")},
                              {QStringLiteral("machine"), str(setup, "machine")}});
  }
  QJsonArray machines;
  QJsonArray dialects;
  try {
    machines = query({{QStringLiteral("query"), QStringLiteral("cam_machines")}})
                   .toObject()
                   .value(QStringLiteral("machines"))
                   .toArray();
    dialects = query({{QStringLiteral("query"), QStringLiteral("cam_dialects")}})
                   .toObject()
                   .value(QStringLiteral("dialects"))
                   .toArray();
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("CAM machines not read: %1").arg(QString::fromUtf8(e.what()));
  }
  return {{QStringLiteral("setups"), setups},
          {QStringLiteral("machines"), machines},
          {QStringLiteral("dialects"), dialects},
          {QStringLiteral("current"), camDefaultSetup()}};
}

int MainWindow::camDefaultSetup() const {
  if (m_camCurrentSetup > 0 && !m_snapshot.camSetup(m_camCurrentSetup).isEmpty()) {
    return m_camCurrentSetup;
  }
  return m_snapshot.camSetups.isEmpty() ? 0 : idOf(m_snapshot.camSetups.last().toObject());
}

// ---------------------------------------------------------------------------
// New Setup

CommandDef MainWindow::camSetupCommand() {
  CommandDef def;
  def.id = QStringLiteral("make.cam_setup");
  def.name = tr("New Setup");
  def.icon = QStringLiteral("cnc-setup");
  def.tooltip = tr("Adds a CNC setup: the bodies, the stock, the work zero and the machine");
  def.tab = kMake;
  def.group = kCnc;
  def.section = QStringLiteral("setup");
  def.pinned = true;
  def.keywords = {QStringLiteral("cam"),   QStringLiteral("cnc"),       QStringLiteral("setup"),
                  QStringLiteral("stock"), QStringLiteral("work zero"), QStringLiteral("wcs"),
                  QStringLiteral("mill"),  QStringLiteral("router"),    QStringLiteral("g54")};
  // An edit logs "Editing CAM setup Setup 1 with New Setup".
  def.editedKind = QStringLiteral("CAM setup");
  const auto around = [](const CommandState& state) { return state.choice(QStringLiteral("stock")) != QStringLiteral("body"); };
  const auto onBody = [](const CommandState& state) { return state.choice(QStringLiteral("stock")) == QStringLiteral("body"); };
  const auto atPoint = [](const CommandState& state) {
    return state.choice(QStringLiteral("origin")) == QStringLiteral("point");
  };
  const Choices origins = {
      {QStringLiteral("top_front_left"), tr("Stock top, front left corner")},
      {QStringLiteral("top_front_right"), tr("Stock top, front right corner")},
      {QStringLiteral("top_back_left"), tr("Stock top, back left corner")},
      {QStringLiteral("top_back_right"), tr("Stock top, back right corner")},
      {QStringLiteral("top_center"), tr("Middle of the stock's top")},
      {QStringLiteral("bottom_front_left"), tr("Stock bottom, front left corner")},
      {QStringLiteral("bottom_center"), tr("Middle of the stock's bottom")},
      {QStringLiteral("point"), tr("A point picked")},
  };
  // The work offsets by their codes, the same in every language; listed
  // for the catalogues as the other fixed choices are (CommandPanel's
  // choiceLabel looks them up in the "Choice" context).
  static const char* const kWorkOffsets[] = {
      QT_TRANSLATE_NOOP("Choice", "G54"), QT_TRANSLATE_NOOP("Choice", "G55"), QT_TRANSLATE_NOOP("Choice", "G56"),
      QT_TRANSLATE_NOOP("Choice", "G57"), QT_TRANSLATE_NOOP("Choice", "G58"), QT_TRANSLATE_NOOP("Choice", "G59"),
  };
  Choices offsets;
  for (int g = 1; g <= 6; ++g) {
    offsets.append(qMakePair(QString::number(g), QString::fromLatin1(kWorkOffsets[g - 1])));
  }
  def.inputs = {
      textInput(QStringLiteral("name"), tr("Name")).withTooltip(tr("Empty: Setup and a number")),
      selectionInput(QStringLiteral("bodies"), tr("Bodies"), SelectKind::Body, 0, 0)
          .withTooltip(tr("The bodies it machines; none: every body")),
      choiceInput(QStringLiteral("stock"), tr("Stock"),
                  {{QStringLiteral("around"), tr("Around the bodies")}, {QStringLiteral("body"), tr("A body")}},
                  QStringLiteral("around"))
          .withHeading(tr("Stock")),
      valueInput(QStringLiteral("side"), tr("Side Margin"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(tr("Stock added on the four sides of the bodies"))
          .withVisible(around),
      valueInput(QStringLiteral("top"), tr("Top Margin"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(tr("Stock added above the bodies"))
          .withVisible(around),
      valueInput(QStringLiteral("bottom"), tr("Bottom Margin"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(tr("Stock added below the bodies"))
          .withVisible(around),
      selectionInput(QStringLiteral("stock_body"), tr("Stock Body"), SelectKind::Body, 0, 1)
          .withTooltip(tr("A body of the design that is the stock"))
          .withVisible(onBody),
      choiceInput(QStringLiteral("origin"), tr("Work Zero"), origins, QStringLiteral("top_front_left"))
          .withHeading(tr("Work Zero"))
          .withTooltip(tr("Where X, Y and Z are zero for the machine")),
      selectionInput(QStringLiteral("origin_point"), tr("Zero Point"),
                     SelectKind::Vertex | SelectKind::SketchPoint | SelectKind::Point | SelectKind::Edge, 0, 1)
          .withTooltip(tr("A vertex, a point or a circular edge's center"))
          .withVisible(atPoint),
      selectionInput(QStringLiteral("z"), tr("Z Axis"),
                     SelectKind::Face | SelectKind::Plane | SelectKind::Axis | SelectKind::Edge, 0, 1)
          .withAccepts([](const SelectionItem& item) {
            return item.kind != SelectKind::Face || item.geometry == QStringLiteral("plane") ||
                   item.geometry == QStringLiteral("cylinder");
          })
          .withTooltip(tr("A planar face, a plane or an axis the tool points along; none: the design's Z")),
      selectionInput(QStringLiteral("x"), tr("X Axis"), SelectKind::Edge | SelectKind::Axis, 0, 1)
          .withTooltip(tr("A straight edge or an axis along X; none: the plane's own")),
      choiceInput(QStringLiteral("work_offset"), tr("Work Offset"), offsets, QStringLiteral("1"))
          .withTooltip(tr("The controller's work coordinate system the program uses")),
      valueInput(QStringLiteral("safe_height"), tr("Safe Height"), ValueKind::Length, QStringLiteral("15 mm"))
          .withTooltip(tr("How far above the stock the tool travels and changes")),
      choiceInput(QStringLiteral("machine"), tr("Machine"), {}, kNoMachine)
          .withChoices([](const CommandState& state) { return machineChoices(camData(state)); })
          .withHeading(tr("Machine"))
          .withTooltip(tr("The machine with the tools; Machines and Tools… adds one")),
      choiceInput(QStringLiteral("dialect"), tr("Controller"), {}, kMachineDialect)
          .withChoices([](const CommandState& state) { return dialectChoices(camData(state)); })
          .withTooltip(tr("The G-code the program is written in")),
      choiceInput(QStringLiteral("units"), tr("Program Units"),
                  {{QStringLiteral("millimetres"), tr("Millimeters")}, {QStringLiteral("inches"), tr("Inches")}},
                  QStringLiteral("millimetres"))
          .withTooltip(tr("The units of the written program")),
      checkInput(QStringLiteral("probe"), tr("Probe the work zero first"))
          .withTooltip(tr("The program starts by probing the work zero on the stock"))
          .withVisible([](const CommandState& state) { return effectiveDialect(state) == QStringLiteral("millenniumos"); }),
  };
  def.init = [this](CommandState& state, const CommandContext&) {
    state.setObject(QStringLiteral("cam"), camPanelData());
    // The design's first machine, its only one usually.
    const Choices machinesNow = machineChoices(camData(state));
    state.setChoice(QStringLiteral("machine"), machinesNow.size() > 1 ? machinesNow.at(1).first : kNoMachine);
    qInfo().noquote() << QStringLiteral("CAM setup panel: machine %1").arg(state.choice(QStringLiteral("machine")));
  };
  def.modelChanged = [this](CommandState& state, const CommandContext&) {
    state.setObject(QStringLiteral("cam"), camPanelData());
  };
  def.load = [this](const QJsonObject& setup, CommandState& state, const CommandContext& context) {
    state.setObject(QStringLiteral("edited"), setup);
    state.setObject(QStringLiteral("cam"), camPanelData());
    state.setObject(QStringLiteral("post"), setup.value(QStringLiteral("post")).toObject());
    state.setText(QStringLiteral("name"), str(setup, "name"));
    // Each body in its occurrence: the id alone, or {body, occurrence}.
    Selection bodies;
    for (const QJsonValue& body : setup.value(QStringLiteral("bodies")).toArray()) {
      const QJsonObject placed = body.toObject();
      bodies.append(body.isString() ? SelectionItem{SelectKind::Body, body.toString(), QString(), QString()}
                                    : SelectionItem{SelectKind::Body, str(placed, "body"), QString(), QString(),
                                                    str(placed, "occurrence")});
    }
    state.setItems(QStringLiteral("bodies"), bodies);
    const QJsonObject stock = setup.value(QStringLiteral("stock")).toObject();
    if (str(stock, "kind") == QStringLiteral("body")) {
      state.setChoice(QStringLiteral("stock"), QStringLiteral("body"));
      state.setItems(QStringLiteral("stock_body"), {SelectionItem{SelectKind::Body, str(stock, "body"), QString(),
                                                                  QString(), str(stock, "occurrence")}});
    } else {
      state.setChoice(QStringLiteral("stock"), QStringLiteral("around"));
      for (const char* key : {"side", "top", "bottom"}) {
        loadCamValue(state, QString::fromLatin1(key), stock.value(QLatin1String(key)), ValueKind::Length, context);
      }
    }
    const QJsonObject wcs = setup.value(QStringLiteral("wcs")).toObject();
    const QJsonObject origin = wcs.value(QStringLiteral("origin")).toObject();
    if (str(origin, "kind") == QStringLiteral("point")) {
      state.setChoice(QStringLiteral("origin"), QStringLiteral("point"));
      const SelectionItem point =
          camItemOf(origin.value(QStringLiteral("at")), str(origin, "occurrence"), context);
      if (point.isValid()) {
        state.setItems(QStringLiteral("origin_point"), {point});
      }
    } else {
      const QString corner = str(origin, "corner");
      state.setChoice(QStringLiteral("origin"), corner.isEmpty() ? QStringLiteral("top_front_left") : corner);
    }
    for (const char* axis : {"z", "x"}) {
      const QString occurrenceKey = QString::fromLatin1(axis) + QStringLiteral("_occurrence");
      const SelectionItem item = camItemOf(wcs.value(QLatin1String(axis)),
                                           wcs.value(occurrenceKey).toString(), context);
      if (item.isValid()) {
        state.setItems(QString::fromLatin1(axis), {item});
      }
    }
    state.setChoice(QStringLiteral("work_offset"), QString::number(setup.value(QStringLiteral("work_offset")).toInt(1)));
    loadCamValue(state, QStringLiteral("safe_height"), setup.value(QStringLiteral("safe_height")), ValueKind::Length,
                 context);
    const QString machine = str(setup, "machine");
    state.setChoice(QStringLiteral("machine"), machine.isEmpty() ? kNoMachine : machine);
    const QString dialect = str(setup, "dialect");
    state.setChoice(QStringLiteral("dialect"), dialect.isEmpty() ? kMachineDialect : dialect);
    state.setChoice(QStringLiteral("units"),
                    str(setup.value(QStringLiteral("post")).toObject(), "units") == QStringLiteral("inches")
                        ? QStringLiteral("inches")
                        : QStringLiteral("millimetres"));
    state.setChecked(QStringLiteral("probe"), setup.value(QStringLiteral("probe_work_zero")).toBool());
  };
  def.build = [](const CommandState& state, const CommandContext&) -> Built {
    QString why;
    QString input;
    const QJsonObject fields = setupFields(state, &why, &input);
    if (!why.isEmpty()) {
      return Built::failure(why, input);
    }
    return Built::of(fields);
  };
  def.accept = [this](const CommandState& state, const CommandContext&) { return acceptCamSetup(state); };
  return def;
}

bool MainWindow::acceptCamSetup(const CommandState& state) {
  QString why;
  QString input;
  const QJsonObject fields = setupFields(state, &why, &input);
  if (!why.isEmpty()) {
    showHint(why);
    return false;
  }
  const QJsonObject edited = state.object(QStringLiteral("edited"));
  const QJsonObject command =
      edited.isEmpty()
          ? QJsonObject{{QStringLiteral("cmd"), QStringLiteral("cam_setup_add")}, {QStringLiteral("setup"), fields}}
          : QJsonObject{{QStringLiteral("cmd"), QStringLiteral("cam_setup_edit")},
                        {QStringLiteral("setup"), idOf(edited)},
                        {QStringLiteral("changes"), fields}};
  QJsonObject result;
  if (!runModelCommand(command, &result)) {
    qInfo().noquote() << QStringLiteral("CAM setup refused: %1").arg(m_lastError);
    return false;
  }
  const int id = edited.isEmpty() ? result.value(QStringLiteral("setup")).toInt() : idOf(edited);
  m_camCurrentSetup = id;
  const QString name = str(m_snapshot.camSetup(id), "name");
  qInfo().noquote() << (edited.isEmpty() ? QStringLiteral("CAM setup added: %1") : QStringLiteral("CAM setup edited: %1"))
                           .arg(name);
  return true;
}

// ---------------------------------------------------------------------------
// The operations' panels

CommandDef MainWindow::camOperationCommand(const QString& type) {
  const bool drill = type == QStringLiteral("drill");
  const bool contour = type == QStringLiteral("contour");
  const bool facing = type == QStringLiteral("facing");
  const bool clearing = clears(type);
  const bool surface = machinesSurfaces(type);
  const bool roughing = type == QStringLiteral("roughing_3d");
  const bool raster = type == QStringLiteral("raster_finish");
  CommandDef def;
  def.id = commandOf(type);
  def.name = operationName(type);
  for (const OperationKind& kind : kOperationKinds) {
    if (type == QLatin1String(kind.type)) {
      def.icon = QString::fromLatin1(kind.icon);
    }
  }
  def.tooltip = operationTooltip(type);
  def.tab = kMake;
  def.group = kCnc;
  def.section = QStringLiteral("operations");
  def.pinned = contour;
  def.keywords = {QStringLiteral("cam"), QStringLiteral("cnc"), QStringLiteral("toolpath"), QStringLiteral("operation"),
                  QStringLiteral("mill"), type};
  if (drill) {
    def.keywords << QStringLiteral("hole") << QStringLiteral("peck");
  } else if (contour) {
    def.keywords << QStringLiteral("profile") << QStringLiteral("outline") << QStringLiteral("cut out");
  } else if (facing) {
    def.keywords << QStringLiteral("face") << QStringLiteral("surface");
  } else if (surface) {
    def.keywords << QStringLiteral("3d") << QStringLiteral("3-axis") << QStringLiteral("surface")
                 << QStringLiteral("z-level");
    if (roughing) {
      def.keywords << QStringLiteral("rough") << QStringLiteral("clear");
    } else {
      def.keywords << QStringLiteral("finish") << QStringLiteral("ball") << (raster ? QStringLiteral("parallel")
                                                                                 : QStringLiteral("steep"));
    }
  } else {
    def.keywords << QStringLiteral("clear") << QStringLiteral("rough") << QStringLiteral("pocket");
  }
  // An edit logs "Editing CAM operation Contour 1 with Contour".
  def.editedKind = QStringLiteral("CAM operation");
  def.enabled = [this] { return !m_snapshot.camSetups.isEmpty(); };

  // What is picked: faces and sketch profiles (areas), outlines for a
  // contour, points for drilling.
  SelectFilter picks = SelectKind::Face | SelectKind::Profile;
  QString pickLabel = tr("Areas");
  QString pickTip = tr("Planar faces or sketch profiles whose areas are cleared");
  int least = 1;
  if (facing) {
    pickLabel = tr("Area");
    pickTip = tr("Planar faces or sketch profiles to face; none: the stock's top");
    least = 0;
  } else if (contour) {
    picks = picks | SelectKind::Edge | SelectKind::SketchCurve;
    pickLabel = tr("Outlines");
    pickTip = tr("Faces, sketch profiles, edges or sketch curves the tool goes along");
  } else if (drill) {
    picks = SelectKind::Vertex | SelectKind::SketchPoint | SelectKind::Point | SelectKind::Face | SelectKind::Edge;
    pickLabel = tr("Holes");
    pickTip = tr("Points, vertices, round edges or holes' faces");
  } else if (surface) {
    pickLabel = tr("Boundary");
    pickTip = tr("Planar faces or sketch profiles the tool's center stays within; none: the stock");
    least = 0;
  }
  const auto accepts = [drill](const SelectionItem& item) {
    if (item.kind == SelectKind::Face) {
      return drill ? item.geometry == QStringLiteral("cylinder") : item.geometry == QStringLiteral("plane");
    }
    if (drill && item.kind == SelectKind::Edge) {
      return item.geometry == QStringLiteral("circle") || item.geometry == QStringLiteral("arc");
    }
    return true;
  };
  const auto setupChanged = [type](CommandState& state, const CommandContext&) {
    // Another setup's machine: its own tools.
    const int setup = state.choice(QStringLiteral("setup")).toInt();
    const Choices tools = toolChoices(camData(state), setup);
    const QString tool = state.choice(QStringLiteral("tool"));
    if (std::none_of(tools.begin(), tools.end(), [&tool](const QPair<QString, QString>& c) { return c.first == tool; })) {
      state.setChoice(QStringLiteral("tool"), defaultTool(camData(state), setup, type));
    }
  };
  const auto entryIs = [](const char* kind) {
    return [kind](const CommandState& state) { return state.choice(QStringLiteral("entry")) == QLatin1String(kind); };
  };
  const auto none = [](const CommandState&, const CommandContext&) { return MainWindow::tr("Auto"); };
  const auto fromTool = [](const CommandState&, const CommandContext&) { return MainWindow::tr("From the tool"); };

  def.inputs = {
      choiceInput(QStringLiteral("setup"), tr("Setup"), {}, QString())
          .withChoices([](const CommandState& state) { return setupChoices(camData(state)); })
          .withOnChange(setupChanged)
          .withTooltip(tr("The setup the operation belongs to")),
      textInput(QStringLiteral("name"), tr("Name")).withTooltip(tr("Empty: the kind and a number")),
  };
  if (surface) {
    def.inputs.append(selectionInput(QStringLiteral("bodies"), tr("Bodies"), SelectKind::Body, 0, 0)
                          .withTooltip(tr("The bodies whose surfaces are machined; none: the setup's bodies")));
  }
  def.inputs.append(
      selectionInput(QStringLiteral("geometry"), pickLabel, picks, least, 0).withAccepts(accepts).withTooltip(pickTip));
  if (contour) {
    def.inputs.append(choiceInput(QStringLiteral("side"), tr("Side"),
                                  {{QStringLiteral("outside"), tr("Outside")},
                                   {QStringLiteral("inside"), tr("Inside")},
                                   {QStringLiteral("on"), tr("On the Line")}},
                                  QStringLiteral("outside"))
                          .withTooltip(tr("Which side of the outline the tool runs on")));
  }
  def.inputs.append(choiceInput(QStringLiteral("tool"), tr("Tool"), {}, QString())
                        .withChoices([](const CommandState& state) {
                          return toolChoices(camData(state), state.choice(QStringLiteral("setup")).toInt());
                        })
                        .withTooltip(tr("A tool of the setup's machine")));
  if (type == QStringLiteral("rest")) {
    def.inputs.append(choiceInput(QStringLiteral("from_tool"), tr("Bigger Tool"), {}, QString())
                          .withChoices([](const CommandState& state) {
                            Choices choices{{QString(), MainWindow::tr("Any bigger tool")}};
                            choices += toolChoices(camData(state), state.choice(QStringLiteral("setup")).toInt());
                            return choices;
                          })
                          .withTooltip(tr("The tool that cleared the area before")));
  }
  if (roughing) {
    def.inputs.append(checkInput(QStringLiteral("adaptive"), tr("Adaptive clearing"))
                          .withTooltip(tr("Clears each level keeping the tool's engagement even")));
  } else if (surface) {
    const auto everySlope = [](const CommandState&, const CommandContext&) { return MainWindow::tr("Every slope"); };
    if (raster) {
      def.inputs.append(valueInput(QStringLiteral("angle"), tr("Pass Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
                            .withHeading(tr("Surfaces"))
                            .withTooltip(tr("The passes' direction from the work's X axis")));
      def.inputs.append(valueInput(QStringLiteral("slope"), tr("Steepest Slope"), ValueKind::Angle, QString())
                            .withAuto(everySlope)
                            .withTooltip(tr("Cuts only where the surface is at most this steep; empty: every slope")));
    } else {
      def.inputs.append(valueInput(QStringLiteral("slope"), tr("Shallowest Slope"), ValueKind::Angle, QString())
                            .withHeading(tr("Surfaces"))
                            .withAuto(everySlope)
                            .withTooltip(tr("Cuts only where the surface is at least this steep; empty: every slope")));
    }
  }
  def.inputs.append(valueInput(QStringLiteral("top"), tr("Top"), ValueKind::Length, QString())
                        .withHeading(tr("Depths"))
                        .withAuto([](const CommandState&, const CommandContext&) { return MainWindow::tr("Stock top"); })
                        .withTooltip(tr("Where the cut starts, in the work's Z; empty: the stock's top")));
  def.inputs.append(valueInput(QStringLiteral("bottom"), tr("Bottom"), ValueKind::Length, QString())
                        .withAuto(none)
                        .withTooltip(tr("How deep the cut goes, in the work's Z; empty: from what is picked")));
  // Raster finishing has no levels.
  if (!raster) {
    QString stepTip = tr("The deepest pass; empty: the flutes or the diameter");
    if (drill) {
      stepTip = tr("The depth of each peck; empty: from the tool");
    } else if (type == QStringLiteral("waterline_finish")) {
      stepTip = tr("The height between levels; empty: a tenth of the diameter");
    }
    def.inputs.append(valueInput(QStringLiteral("step_down"), tr("Step Down"), ValueKind::Length, QString())
                          .withAuto(fromTool)
                          .withTooltip(stepTip));
  }
  def.inputs.append(valueInput(QStringLiteral("clearance"), tr("Clearance Height"), ValueKind::Length, QString())
                        .withHeading(tr("Heights"))
                        .withAuto(none)
                        .withTooltip(tr("Where the tool travels between cuts; empty: the setup's safe height")));
  def.inputs.append(valueInput(QStringLiteral("retract"), tr("Retract Height"), ValueKind::Length, QString())
                        .withAuto(none)
                        .withTooltip(tr("Where rapids end before a cut; empty: a little above the top")));
  def.inputs.append(valueInput(QStringLiteral("spindle_rpm"), tr("Spindle Speed (rpm)"), ValueKind::Unitless, QString())
                        .withHeading(tr("Feeds and Speeds"))
                        .withAuto(fromTool)
                        .withTooltip(tr("Revolutions per minute; empty: the tool's")));
  def.inputs.append(valueInput(QStringLiteral("feed"), tr("Feed per Minute"), ValueKind::Length, QString())
                        .withAuto(fromTool)
                        .withTooltip(tr("A length per minute: 1200 mm or 40 in; empty: the tool's")));
  def.inputs.append(valueInput(QStringLiteral("plunge_feed"), tr("Plunge Feed per Minute"), ValueKind::Length, QString())
                        .withAuto(fromTool)
                        .withTooltip(tr("A length per minute for going down; empty: the tool's")));
  if (facing || clearing || roughing || raster) {
    def.inputs.append(valueInput(QStringLiteral("stepover"), tr("Stepover"), ValueKind::Length, QString())
                          .withAuto(none)
                          .withTooltip(tr("The distance between rows or loops; empty: a share of the diameter")));
  }
  if (contour || clearing || surface) {
    def.inputs.append(valueInput(QStringLiteral("stock_to_leave"), tr("Stock to Leave"), ValueKind::Length, QString())
                          .withAuto(none)
                          .withTooltip(tr("Material left on the walls for a finishing pass")));
  }
  if (!drill && !raster) {
    def.inputs.append(choiceInput(QStringLiteral("direction"), tr("Direction"),
                                  {{QStringLiteral("climb"), tr("Climb")},
                                   {QStringLiteral("conventional"), tr("Conventional")}},
                                  QStringLiteral("climb"))
                          .withTooltip(tr("Climb or conventional milling")));
  }
  def.inputs.append(choiceInput(QStringLiteral("spindle"), tr("Spindle"),
                                {{QStringLiteral("cw"), tr("Clockwise")}, {QStringLiteral("ccw"), tr("Counterclockwise")}},
                                QStringLiteral("cw")));
  def.inputs.append(choiceInput(QStringLiteral("coolant"), tr("Coolant"),
                                {{QStringLiteral("off"), tr("Off")},
                                 {QStringLiteral("flood"), tr("Flood")},
                                 {QStringLiteral("mist"), tr("Mist")},
                                 {QStringLiteral("air"), tr("Air Blast")}},
                                QStringLiteral("off")));
  if (contour || clearing || roughing) {
    def.inputs.append(choiceInput(QStringLiteral("entry"), tr("Entry"),
                                  {{QStringLiteral("plunge"), tr("Plunge")},
                                   {QStringLiteral("ramp"), tr("Ramp")},
                                   {QStringLiteral("helix"), tr("Helix")}},
                                  QStringLiteral("plunge"))
                          .withHeading(tr("Entry"))
                          .withTooltip(tr("How the tool goes down into the material")));
    def.inputs.append(valueInput(QStringLiteral("ramp_angle"), tr("Ramp Angle"), ValueKind::Angle, QStringLiteral("3 deg"))
                          .withVisible(entryIs("ramp")));
    def.inputs.append(valueInput(QStringLiteral("helix_radius"), tr("Helix Radius"), ValueKind::Length, QString())
                          .withAuto(none)
                          .withTooltip(tr("Empty: a quarter of the tool's diameter"))
                          .withVisible(entryIs("helix")));
    def.inputs.append(valueInput(QStringLiteral("helix_angle"), tr("Helix Angle"), ValueKind::Angle, QStringLiteral("3 deg"))
                          .withVisible(entryIs("helix")));
  }
  if (contour) {
    const auto checked = [](const char* id) {
      return [id](const CommandState& state) { return state.checked(QLatin1String(id)); };
    };
    def.inputs.append(checkInput(QStringLiteral("lead"), tr("Lead in and out"))
                          .withTooltip(tr("Arcs into and out of the wall")));
    def.inputs.append(valueInput(QStringLiteral("lead_radius"), tr("Lead Radius"), ValueKind::Length, QStringLiteral("2 mm"))
                          .withVisible(checked("lead")));
    def.inputs.append(checkInput(QStringLiteral("tabs"), tr("Holding tabs"))
                          .withHeading(tr("Tabs"))
                          .withTooltip(tr("Leaves bridges that hold the part in the stock")));
    def.inputs.append(valueInput(QStringLiteral("tab_count"), tr("Tab Count"), ValueKind::Unitless, QStringLiteral("4"))
                          .withVisible(checked("tabs")));
    def.inputs.append(valueInput(QStringLiteral("tab_width"), tr("Tab Width"), ValueKind::Length, QStringLiteral("6 mm"))
                          .withVisible(checked("tabs")));
    def.inputs.append(valueInput(QStringLiteral("tab_height"), tr("Tab Height"), ValueKind::Length, QStringLiteral("2 mm"))
                          .withVisible(checked("tabs")));
  }
  if (drill) {
    def.inputs.append(checkInput(QStringLiteral("peck"), tr("Peck drilling"))
                          .withHeading(tr("Drilling"))
                          .withTooltip(tr("Back out to clear the chips after each step down")));
    def.inputs.append(checkInput(QStringLiteral("canned"), tr("Canned cycle"))
                          .withTooltip(tr("Writes the controller's drilling cycle where its G-code has one")));
  }

  def.init = [this, type](CommandState& state, const CommandContext&) {
    state.setObject(QStringLiteral("cam"), camPanelData());
    const int setup = camDefaultSetup();
    state.setChoice(QStringLiteral("setup"), QString::number(setup));
    state.setChoice(QStringLiteral("tool"), defaultTool(camData(state), setup, type));
    qInfo().noquote() << QStringLiteral("CAM operation panel %1: setup %2, tool %3")
                             .arg(type, str(setupOf(camData(state), setup), "name"), state.choice(QStringLiteral("tool")));
  };
  def.modelChanged = [this](CommandState& state, const CommandContext&) {
    state.setObject(QStringLiteral("cam"), camPanelData());
  };
  def.load = [this](const QJsonObject& op, CommandState& state, const CommandContext& context) {
    state.setObject(QStringLiteral("edited"), op);
    state.setObject(QStringLiteral("cam"), camPanelData());
    state.setChoice(QStringLiteral("setup"), QString::number(op.value(QStringLiteral("setup_id")).toInt()));
    state.setText(QStringLiteral("name"), str(op, "name"));
    state.setItems(QStringLiteral("geometry"), camItems(op.value(QStringLiteral("inputs")).toArray(), context));
    // The bodies of a 3-axis operation, each in its occurrence.
    Selection bodies;
    for (const QJsonValue& input : op.value(QStringLiteral("inputs")).toArray()) {
      const QJsonObject picked = input.toObject();
      if (str(picked, "kind") == QStringLiteral("body")) {
        bodies.append(
            SelectionItem{SelectKind::Body, str(picked, "body"), QString(), QString(), str(picked, "occurrence")});
      }
    }
    state.setItems(QStringLiteral("bodies"), bodies);
    const QJsonObject kind = op.value(QStringLiteral("kind")).toObject();
    state.setChecked(QStringLiteral("adaptive"), kind.value(QStringLiteral("adaptive")).toBool());
    loadCamValue(state, QStringLiteral("angle"), kind.value(QStringLiteral("angle")), ValueKind::Angle, context);
    loadCamValue(state, QStringLiteral("slope"), kind.value(QStringLiteral("slope")), ValueKind::Angle, context);
    if (kind.contains(QStringLiteral("side"))) {
      state.setChoice(QStringLiteral("side"), str(kind, "side"));
    }
    state.setChecked(QStringLiteral("peck"), kind.value(QStringLiteral("peck")).toBool());
    state.setChecked(QStringLiteral("canned"), kind.value(QStringLiteral("canned_cycle")).toBool());
    if (kind.contains(QStringLiteral("from_tool"))) {
      state.setChoice(QStringLiteral("from_tool"), QString::number(kind.value(QStringLiteral("from_tool")).toInt()));
    }
    state.setChoice(QStringLiteral("tool"), QString::number(op.value(QStringLiteral("tool")).toInt()));
    const QJsonObject depths = op.value(QStringLiteral("depths")).toObject();
    const QJsonObject heights = op.value(QStringLiteral("heights")).toObject();
    const QJsonObject cutting = op.value(QStringLiteral("cutting")).toObject();
    loadCamValue(state, QStringLiteral("top"), depths.value(QStringLiteral("top")), ValueKind::Length, context);
    loadCamValue(state, QStringLiteral("bottom"), depths.value(QStringLiteral("bottom")), ValueKind::Length, context);
    loadCamValue(state, QStringLiteral("step_down"), depths.value(QStringLiteral("step_down")), ValueKind::Length, context);
    loadCamValue(state, QStringLiteral("clearance"), heights.value(QStringLiteral("clearance")), ValueKind::Length, context);
    loadCamValue(state, QStringLiteral("retract"), heights.value(QStringLiteral("retract")), ValueKind::Length, context);
    loadCamValue(state, QStringLiteral("spindle_rpm"), cutting.value(QStringLiteral("spindle_rpm")), ValueKind::Unitless,
                 context);
    loadCamValue(state, QStringLiteral("feed"), cutting.value(QStringLiteral("feed")), ValueKind::Length, context);
    loadCamValue(state, QStringLiteral("plunge_feed"), cutting.value(QStringLiteral("plunge_feed")), ValueKind::Length,
                 context);
    loadCamValue(state, QStringLiteral("stepover"), op.value(QStringLiteral("stepover")), ValueKind::Length, context);
    loadCamValue(state, QStringLiteral("stock_to_leave"), op.value(QStringLiteral("stock_to_leave")), ValueKind::Length,
                 context);
    for (const char* key : {"direction", "spindle", "coolant"}) {
      const QString chosen = str(op, key);
      if (!chosen.isEmpty()) {
        state.setChoice(QString::fromLatin1(key), chosen);
      }
    }
    const QJsonObject entry = op.value(QStringLiteral("entry")).toObject();
    if (!entry.isEmpty()) {
      state.setChoice(QStringLiteral("entry"), str(entry, "kind"));
      const QString angleKey = str(entry, "kind") == QStringLiteral("ramp") ? QStringLiteral("ramp_angle")
                                                                             : QStringLiteral("helix_angle");
      loadCamValue(state, angleKey, entry.value(QStringLiteral("angle")), ValueKind::Angle, context);
      loadCamValue(state, QStringLiteral("helix_radius"), entry.value(QStringLiteral("radius")), ValueKind::Length, context);
    }
    const QJsonObject tabs = op.value(QStringLiteral("tabs")).toObject();
    state.setChecked(QStringLiteral("tabs"), !tabs.isEmpty());
    if (!tabs.isEmpty()) {
      loadCamValue(state, QStringLiteral("tab_count"), tabs.value(QStringLiteral("count")), ValueKind::Unitless, context);
      loadCamValue(state, QStringLiteral("tab_width"), tabs.value(QStringLiteral("width")), ValueKind::Length, context);
      loadCamValue(state, QStringLiteral("tab_height"), tabs.value(QStringLiteral("height")), ValueKind::Length, context);
    }
    const QJsonObject lead = op.value(QStringLiteral("lead")).toObject();
    state.setChecked(QStringLiteral("lead"), !lead.isEmpty());
    if (!lead.isEmpty()) {
      loadCamValue(state, QStringLiteral("lead_radius"), lead.value(QStringLiteral("radius")), ValueKind::Length, context);
    }
  };
  def.build = [type](const CommandState& state, const CommandContext&) -> Built {
    if (setupOf(camData(state), state.choice(QStringLiteral("setup")).toInt()).isEmpty()) {
      return Built::failure(MainWindow::tr("Add a setup first: New Setup."), QStringLiteral("setup"));
    }
    QString why;
    QString input;
    const QJsonObject fields = operationFields(state, type, &why, &input);
    if (!why.isEmpty()) {
      return Built::failure(why, input);
    }
    return Built::of(fields);
  };
  def.accept = [this, type](const CommandState& state, const CommandContext&) {
    return acceptCamOperation(state, type);
  };
  def.ended = [this](bool) {
    m_camEditedOperation = 0;
    drawCamScene();
  };
  return def;
}

bool MainWindow::acceptCamOperation(const CommandState& state, const QString& type) {
  QString why;
  QString input;
  const QJsonObject fields = operationFields(state, type, &why, &input);
  if (!why.isEmpty()) {
    showHint(why);
    return false;
  }
  const int setup = state.choice(QStringLiteral("setup")).toInt();
  const QJsonObject edited = state.object(QStringLiteral("edited"));
  if (!edited.isEmpty()) {
    // Its changes, and into another setup (last there) when one was chosen:
    // one undo step.
    const int id = idOf(edited);
    QJsonArray commands;
    commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("cam_operation_edit")},
                                {QStringLiteral("operation"), id},
                                {QStringLiteral("changes"), fields}});
    if (setup != edited.value(QStringLiteral("setup_id")).toInt()) {
      commands.append(QJsonObject{
          {QStringLiteral("cmd"), QStringLiteral("cam_operation_move")},
          {QStringLiteral("operation"), id},
          {QStringLiteral("setup"), setup},
          {QStringLiteral("index"),
           static_cast<int>(m_snapshot.camSetup(setup).value(QStringLiteral("operations")).toArray().size())}});
    }
    if (!runModelCommands(commands, tr("Edit CAM Operation"))) {
      qInfo().noquote() << QStringLiteral("CAM operation refused: %1").arg(m_lastError);
      return false;
    }
    m_camCurrentSetup = setup;
    qInfo().noquote() << QStringLiteral("CAM operation edited: %1 in %2")
                             .arg(str(m_snapshot.camOperation(id), "name"), str(m_snapshot.camSetup(setup), "name"));
    return true;
  }
  QJsonObject result;
  if (!runModelCommand({{QStringLiteral("cmd"), QStringLiteral("cam_operation_add")},
                        {QStringLiteral("setup"), setup},
                        {QStringLiteral("operation"), fields}},
                       &result)) {
    qInfo().noquote() << QStringLiteral("CAM operation refused: %1").arg(m_lastError);
    return false;
  }
  m_camCurrentSetup = setup;
  const int id = result.value(QStringLiteral("operation")).toInt();
  qInfo().noquote() << QStringLiteral("CAM operation added: %1 to %2")
                           .arg(str(m_snapshot.camOperation(id), "name"), str(m_snapshot.camSetup(setup), "name"));
  return true;
}

// ---------------------------------------------------------------------------
// The Browser's CAM rows

void MainWindow::editCamSetup(int setup) {
  if (m_mode != Mode::Idle) {
    return;
  }
  const QJsonObject json = m_snapshot.camSetup(setup);
  const CommandDef* def = m_registry->find(QStringLiteral("make.cam_setup"));
  if (json.isEmpty() || def == nullptr) {
    return;
  }
  m_camCurrentSetup = setup;
  startFeatureCommand(*def, QString(), json);
}

void MainWindow::editCamOperation(int operation) {
  if (m_mode != Mode::Idle) {
    return;
  }
  QJsonObject setup;
  QJsonObject json = m_snapshot.camOperation(operation, &setup);
  const QString type = str(json.value(QStringLiteral("kind")).toObject(), "type");
  const CommandDef* def = m_registry->find(commandOf(type));
  if (json.isEmpty() || def == nullptr) {
    return;
  }
  json.insert(QStringLiteral("setup_id"), idOf(setup));
  // Its toolpath is the current one while the panel is open.
  m_camEditedOperation = operation;
  drawCamScene();
  startFeatureCommand(*def, QString(), json);
}

void MainWindow::runCamAction(const QString& action, int setup) {
  if (action == QStringLiteral("generate")) {
    generateCam(setup);
  } else if (action == QStringLiteral("simulate")) {
    simulateCam(setup);
  } else if (action == QStringLiteral("post")) {
    postProcessCam(setup);
  }
}

void MainWindow::setCurrentCamOperation(int operation) {
  if (operation == m_camCurrentOperation) {
    return;
  }
  m_camCurrentOperation = operation;
  qDebug().noquote() << QStringLiteral("CAM current operation: %1")
                            .arg(operation > 0 ? str(m_snapshot.camOperation(operation), "name") : QStringLiteral("none"));
  drawCamScene();
}

// ---------------------------------------------------------------------------
// Generate

void MainWindow::generateCam(int setup) {
  if (m_snapshot.camSetups.isEmpty()) {
    showHint(tr("Add a setup first: New Setup."));
    return;
  }
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("cam_generate")}};
  if (setup > 0) {
    command.insert(QStringLiteral("setup"), setup);
  }
  QJsonObject result;
  if (!runModelCommand(command, &result)) {
    qInfo().noquote() << QStringLiteral("CAM generate refused: %1").arg(m_lastError);
    return;
  }
  int done = 0;
  int failed = 0;
  QString firstFailure;
  QString firstFinding;
  for (const QJsonValue& value : result.value(QStringLiteral("setups")).toArray()) {
    const QJsonObject generated = value.toObject();
    const QString setupName = str(generated, "name");
    QStringList parts;
    for (const QJsonValue& entry : generated.value(QStringLiteral("operations")).toArray()) {
      const QJsonObject op = entry.toObject();
      const QString state = str(op, "state");
      if (state == QStringLiteral("up_to_date")) {
        ++done;
        parts << QStringLiteral("%1 up_to_date, %2 moves, %3 mm, %4 min")
                     .arg(str(op, "name"))
                     .arg(op.value(QStringLiteral("moves")).toInt())
                     .arg(cam::shortNumber(op.value(QStringLiteral("cut_length")).toDouble()),
                          cam::shortNumber(op.value(QStringLiteral("duration")).toDouble()));
      } else if (state == QStringLiteral("failed")) {
        ++failed;
        parts << QStringLiteral("%1 failed: %2").arg(str(op, "name"), str(op, "error"));
        if (firstFailure.isEmpty()) {
          firstFailure = QStringLiteral("%1: %2").arg(str(op, "name"), modelMessage(userText(str(op, "error"))));
        }
      } else {
        parts << QStringLiteral("%1 %2").arg(str(op, "name"), state);
      }
    }
    qInfo().noquote() << QStringLiteral("CAM generate %1: %2")
                             .arg(setupName, parts.isEmpty() ? QStringLiteral("no operations")
                                                             : parts.join(QStringLiteral("; ")));
    if (generated.contains(QStringLiteral("error"))) {
      ++failed;
      qInfo().noquote() << QStringLiteral("CAM generate %1 failed: %2").arg(setupName, str(generated, "error"));
      if (firstFailure.isEmpty()) {
        firstFailure = QStringLiteral("%1: %2").arg(setupName, modelMessage(userText(str(generated, "error"))));
      }
    }
    // The machine check's findings: what the machine cannot do, in words
    // (their places go to the log only).
    for (const QJsonValue& finding : generated.value(QStringLiteral("findings")).toArray()) {
      const QString message = str(finding.toObject(), "message");
      qInfo().noquote() << QStringLiteral("CAM finding %1: %2").arg(setupName, message);
      if (firstFinding.isEmpty()) {
        firstFinding = modelMessage(message);
      }
    }
  }
  if (failed > 0) {
    showWarning(tr("Could not generate every toolpath. %1").arg(firstFailure));
  } else if (!firstFinding.isEmpty()) {
    showWarning(tr("The machine check found a problem. %1").arg(firstFinding));
  } else {
    showStatus(tr("Generated %n toolpath(s).", nullptr, done), NoticeLevel::Information);
  }
}

// ---------------------------------------------------------------------------
// The toolpaths in the view

void MainWindow::followCam() {
  // The operations' states, for UI tests.
  QStringList setups;
  for (const QJsonValue& value : std::as_const(m_snapshot.camSetups)) {
    const QJsonObject setup = value.toObject();
    QStringList operations;
    for (const QJsonValue& entry : setup.value(QStringLiteral("operations")).toArray()) {
      operations << QStringLiteral("%1 %2").arg(str(entry.toObject(), "name"), str(entry.toObject(), "state"));
    }
    setups << QStringLiteral("%1: %2").arg(str(setup, "name"), operations.isEmpty() ? QStringLiteral("no operations")
                                                                                    : operations.join(QStringLiteral(", ")));
  }
  const QString line = QStringLiteral("CAM states: %1").arg(setups.isEmpty() ? QStringLiteral("none")
                                                                            : setups.join(QStringLiteral("; ")));
  if (line != m_loggedCamStates) {
    m_loggedCamStates = line;
    qInfo().noquote() << line;
  }
  // The toolpaths again when the setups, their states or their eyes (each
  // setup's `visible`) changed.
  const QByteArray key = QJsonDocument(m_snapshot.camSetups).toJson(QJsonDocument::Compact);
  if (key != m_camDrawnKey) {
    m_camDrawnKey = key;
    updateCamToolpaths();
  }
}

void MainWindow::updateCamToolpaths() {
  m_camLines.clear();
  int operations = 0;
  int moves = 0;
  for (const QJsonValue& value : std::as_const(m_snapshot.camSetups)) {
    const QJsonObject setup = value.toObject();
    const int id = idOf(setup);
    // Its eye off: its toolpaths are not drawn.
    if (!setup.value(QStringLiteral("visible")).toBool(true)) {
      continue;
    }
    // The operations generated: up to date, or out of date (drawn muted).
    QJsonArray ids;
    QList<int> stale;
    for (const QJsonValue& entry : setup.value(QStringLiteral("operations")).toArray()) {
      const QString state = str(entry.toObject(), "state");
      if (state == QStringLiteral("up_to_date") || state == QStringLiteral("out_of_date")) {
        ids.append(idOf(entry.toObject()));
        if (state == QStringLiteral("out_of_date")) {
          stale << idOf(entry.toObject());
        }
      }
    }
    if (ids.isEmpty()) {
      continue;
    }
    QJsonObject answer;
    try {
      answer = query({{QStringLiteral("query"), QStringLiteral("cam_toolpaths")},
                      {QStringLiteral("setup"), id},
                      {QStringLiteral("operations"), ids}})
                   .toObject();
    } catch (const std::exception& e) {
      qWarning().noquote() << QStringLiteral("CAM toolpaths of %1 not read: %2")
                                  .arg(str(setup, "name"), QString::fromUtf8(e.what()));
      continue;
    }
    const cam::WorkFrame frame = cam::WorkFrame::of(answer.value(QStringLiteral("work")).toObject());
    if (!frame.valid) {
      continue;
    }
    std::optional<cam::Point3> at;
    for (const QJsonValue& entry : answer.value(QStringLiteral("operations")).toArray()) {
      const QJsonObject op = entry.toObject();
      const QJsonArray path = op.value(QStringLiteral("toolpath")).toObject().value(QStringLiteral("moves")).toArray();
      if (path.isEmpty()) {
        continue;
      }
      CamLines lines;
      lines.setup = id;
      lines.operation = idOf(op);
      lines.stale = stale.contains(lines.operation);
      lines.lines = cam::pathLines(path, frame, at);
      ++operations;
      moves += lines.lines.moves;
      qDebug().noquote() << QStringLiteral("CAM toolpath %1").arg(cam::pathLinesLog(str(op, "name"), lines.lines));
      m_camLines.push_back(std::move(lines));
    }
  }
  qInfo().noquote() << QStringLiteral("CAM toolpaths drawn: %1 operation(s), %2 moves").arg(operations).arg(moves);
  drawCamScene();
}

void MainWindow::drawCamScene() {
  using Look = cam::ToolpathScene::Look;
  cam::ToolpathScene scene;
  for (const Look look : {Look::Rapid, Look::Feed, Look::Entry, Look::Stale, Look::Current, Look::CurrentRapid}) {
    scene.groups.push_back({look, {}});
  }
  const auto group = [&scene](Look look) -> std::vector<float>& {
    return scene.groups.at(static_cast<std::size_t>(look)).segments;
  };
  const auto append = [](std::vector<float>& to, const std::vector<float>& from) {
    to.insert(to.end(), from.begin(), from.end());
  };
  for (const CamLines& entry : m_camLines) {
    const bool current = entry.operation > 0 &&
                         (entry.operation == m_camCurrentOperation || entry.operation == m_camEditedOperation);
    if (current) {
      append(group(Look::CurrentRapid), entry.lines.rapids);
      append(group(Look::Current), entry.lines.feeds);
      append(group(Look::Current), entry.lines.entries);
    } else if (entry.stale) {
      append(group(Look::Stale), entry.lines.rapids);
      append(group(Look::Stale), entry.lines.feeds);
      append(group(Look::Stale), entry.lines.entries);
    } else {
      append(group(Look::Rapid), entry.lines.rapids);
      append(group(Look::Feed), entry.lines.feeds);
      append(group(Look::Entry), entry.lines.entries);
    }
  }
  scene.tool = m_camTool;
  m_viewer->setCamToolpaths(scene);
}

// ---------------------------------------------------------------------------
// Simulate

void MainWindow::simulateCam(int setup) {
  const int id = setup > 0 ? setup : camDefaultSetup();
  const QJsonObject json = m_snapshot.camSetup(id);
  if (json.isEmpty()) {
    showHint(tr("Add a setup first: New Setup."));
    return;
  }
  QJsonObject answer;
  try {
    answer = query({{QStringLiteral("query"), QStringLiteral("cam_toolpaths")}, {QStringLiteral("setup"), id}}).toObject();
  } catch (const std::exception& e) {
    showWarning(tr("Could not simulate %1: %2").arg(str(json, "name"), modelMessage(userText(QString::fromUtf8(e.what())))));
    return;
  }
  const cam::WorkFrame frame = cam::WorkFrame::of(answer.value(QStringLiteral("work")).toObject());
  m_camPlayback.clear();
  m_camPlaybackOperations.clear();
  std::optional<cam::Point3> at;
  for (const QJsonValue& entry : answer.value(QStringLiteral("operations")).toArray()) {
    const QJsonObject op = entry.toObject();
    const QJsonArray moves = op.value(QStringLiteral("toolpath")).toObject().value(QStringLiteral("moves")).toArray();
    if (moves.isEmpty() || !frame.valid) {
      continue;
    }
    CamPlaybackOperation played;
    played.id = idOf(op);
    played.name = str(op, "name");
    played.diameter = op.value(QStringLiteral("tool")).toObject().value(QStringLiteral("diameter")).toDouble(6.0);
    played.moves = static_cast<int>(moves.size());
    cam::appendPlayback(moves, frame, static_cast<int>(m_camPlaybackOperations.size()), at, m_camPlayback);
    m_camPlaybackOperations.push_back(played);
  }
  if (m_camPlayback.empty()) {
    showHint(tr("No toolpath to play back: Generate computes them."));
    qInfo().noquote() << QStringLiteral("CAM simulate %1: no toolpaths").arg(str(json, "name"));
    return;
  }
  m_camPlaybackAxis = frame.z;
  if (m_camSimulate == nullptr) {
    m_camSimulate = new CamSimulateDialog(this);
    connect(m_camSimulate.data(), &CamSimulateDialog::positionChanged, this, [this](int index) { camSimulateAt(index); });
    connect(m_camSimulate.data(), &QDialog::finished, this, [this] {
      m_camTool.reset();
      m_camCurrentOperation = 0;
      clearCamStock();
      qDebug().noquote() << QStringLiteral("CAM simulate closed");
      drawCamScene();
    });
  }
  // The stock as cut along the playback (cam/MainWindowCamStock.cpp).
  loadCamStock(id, str(json, "name"), frame);
  qInfo().noquote() << QStringLiteral("CAM simulate: %1, %2 operations, %3 points")
                           .arg(str(json, "name"))
                           .arg(m_camPlaybackOperations.size())
                           .arg(m_camPlayback.size());
  m_camSimulate->setPlayback(static_cast<int>(m_camPlayback.size()));
  showDialog(m_camSimulate);
  m_camSimulate->raise();
}

void MainWindow::camSimulateAt(int index) {
  if (index < 0 || static_cast<std::size_t>(index) >= m_camPlayback.size()) {
    return;
  }
  const cam::PlaybackPoint& point = m_camPlayback.at(static_cast<std::size_t>(index));
  const CamPlaybackOperation& op = m_camPlaybackOperations.at(static_cast<std::size_t>(point.operation));
  cam::ToolpathScene::Tool tool;
  tool.tip = point.at;
  tool.axis = m_camPlaybackAxis;
  tool.diameter = op.diameter;
  tool.length = std::max(10.0, op.diameter * 3.0);
  m_camTool = tool;
  m_camCurrentOperation = op.id;
  if (m_camSimulate != nullptr) {
    m_camSimulate->setPositionText(tr("%1, move %2 of %3").arg(op.name).arg(point.move + 1).arg(op.moves));
  }
  qDebug().noquote() << QStringLiteral("CAM simulate at %1 of %2: %3, move %4")
                            .arg(index + 1)
                            .arg(m_camPlayback.size())
                            .arg(op.name)
                            .arg(point.move + 1);
  showCamStock(index);
  drawCamScene();
}

// ---------------------------------------------------------------------------
// Post Process…

void MainWindow::postProcessCam(int setup) {
  if (m_snapshot.camSetups.isEmpty()) {
    showHint(tr("Add a setup first: New Setup."));
    return;
  }
  QJsonArray dialects;
  try {
    dialects = query({{QStringLiteral("query"), QStringLiteral("cam_dialects")}})
                   .toObject()
                   .value(QStringLiteral("dialects"))
                   .toArray();
  } catch (const std::exception&) {
  }
  // The setup, its controller and the units, its own by default.
  QDialog dialog(this);
  dialog.setObjectName(QStringLiteral("camPostDialog"));
  dialog.setWindowTitle(tr("Post Process"));
  auto* form = new QFormLayout;
  auto* setups = new QComboBox;
  setups->setObjectName(QStringLiteral("camPostSetup"));
  for (const QJsonValue& value : std::as_const(m_snapshot.camSetups)) {
    setups->addItem(str(value.toObject(), "name"), idOf(value.toObject()));
  }
  setups->setCurrentIndex(std::max(0, setups->findData(setup > 0 ? setup : camDefaultSetup())));
  form->addRow(tr("Setup"), setups);
  auto* dialect = new QComboBox;
  dialect->setObjectName(QStringLiteral("camPostDialect"));
  for (const QJsonValue& value : std::as_const(dialects)) {
    dialect->addItem(str(value.toObject(), "label"), str(value.toObject(), "id"));
  }
  dialect->setToolTip(tr("The controller the G-code is written for"));
  form->addRow(tr("Controller"), dialect);
  auto* units = new QComboBox;
  units->setObjectName(QStringLiteral("camPostUnits"));
  units->addItem(tr("Millimeters"), QStringLiteral("millimetres"));
  units->addItem(tr("Inches"), QStringLiteral("inches"));
  form->addRow(tr("Units"), units);
  const auto defaults = [this, setups, dialect, units] {
    const QJsonObject json = m_snapshot.camSetup(setups->currentData().toInt());
    dialect->setCurrentIndex(std::max(0, dialect->findData(str(json, "effective_dialect"))));
    units->setCurrentIndex(
        std::max(0, units->findData(str(json.value(QStringLiteral("post")).toObject(), "units"))));
  };
  defaults();
  connect(setups, &QComboBox::currentIndexChanged, &dialog, defaults);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  buttons->button(QDialogButtonBox::Ok)->setText(tr("Post…"));
  connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  // The owner's order of 13.11.2026 (mitcad#1013): the machine code is
  // written only after this notice (the program starts with it too), a
  // warning in its highlight box (docs/ui-conventions.md, "One
  // presentation per level").
  auto* notice = new QLabel(noticeLevelText(
      NoticeLevel::Warning, tr("MitCad CAM is highly experimental: treat the machine code with caution. "
                               "Check every toolpath in a simulator, then run it first without stock, at a "
                               "low feed, with a hand on the emergency stop, before cutting.")));
  notice->setObjectName(QStringLiteral("camPostNotice"));
  notice->setWordWrap(true);
  notice->setMargin(8);
  notice->setAutoFillBackground(true);
  QPalette noticePalette = notice->palette();
  noticePalette.setColor(QPalette::Window, noticeFill(NoticeLevel::Warning, noticePalette));
  notice->setPalette(noticePalette);
  auto* layout = new QVBoxLayout(&dialog);
  layout->addLayout(form);
  layout->addWidget(notice);
  layout->addWidget(buttons);
  qInfo().noquote() << QStringLiteral("CAM post dialog: the notice that CAM is highly experimental is shown");
  qInfo().noquote() << QStringLiteral("CAM post dialog: setup %1, %2, %3")
                           .arg(setups->currentText(), dialect->currentData().toString(), units->currentData().toString());
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    return;
  }
  const int id = setups->currentData().toInt();
  const QString setupName = setups->currentText();
  QJsonObject posted;
  try {
    posted = query({{QStringLiteral("query"), QStringLiteral("cam_post")},
                    {QStringLiteral("setup"), id},
                    {QStringLiteral("dialect"), dialect->currentData().toString()},
                    {QStringLiteral("units"), units->currentData().toString()}})
                 .toObject();
  } catch (const std::exception& e) {
    const QString error = QString::fromUtf8(e.what());
    qInfo().noquote() << QStringLiteral("CAM post refused: %1").arg(error);
    sheetWarning(this, tr("Post Process"),
                 tr("Could not write the program of %1:\n%2").arg(setupName, modelMessage(userText(error))));
    return;
  }
  const QString extension = str(posted, "extension");
  const QString base = m_filePath.isEmpty() ? setupName : QFileInfo(m_filePath).completeBaseName() + QLatin1Char('-') + setupName;
  const QString suggested = QDir(dialogFolder(DialogGroup::Export)).filePath(base + QLatin1Char('.') + extension);
  const QString filter = tr("G-code for %1 (*.%2)").arg(str(posted, "label"), extension);
  QString path = dialogSaveFileName(this, tr("Save G-code"), suggested, filter);
  if (path.isEmpty()) {
    return;
  }
  rememberDialogFolder(DialogGroup::Export, path);
  if (QFileInfo(path).suffix().isEmpty()) {
    path += QLatin1Char('.') + extension;
  }
  const QString text = str(posted, "text");
  QSaveFile file(path);
  if (!file.open(QIODevice::WriteOnly | QIODevice::Text) || file.write(text.toUtf8()) < 0 || !file.commit()) {
    qInfo().noquote() << QStringLiteral("CAM post not written: %1: %2").arg(path, file.errorString());
    sheetCritical(this, tr("Post Process"),
                  tr("Could not write %1:\n%2").arg(QDir::toNativeSeparators(path), file.errorString()));
    return;
  }
  const int lines = static_cast<int>(text.count(QLatin1Char('\n')));
  qInfo().noquote() << QStringLiteral("CAM post written: %1: %2, %3, %4 lines")
                           .arg(path, str(posted, "dialect"), str(posted, "units"))
                           .arg(lines);
  showStatus(tr("Wrote the program of %1 to %2.").arg(setupName, QDir::toNativeSeparators(path)), NoticeLevel::Information);
}

// ---------------------------------------------------------------------------
// Machines and Tools…

void MainWindow::openCamMachines() {
  CamMachinesDialog::Host host;
  host.query = [this](const QJsonObject& asked) { return query(asked); };
  host.run = [this](const QJsonObject& command, QJsonObject* result) { return runModelCommand(command, result); };
  host.lastError = [this] { return m_lastError; };
  host.lengthText = [this](double millimetres) { return cmd::lengthText(millimetres, *this); };
  host.projectFolder = projectFolder();
  CamMachinesDialog dialog(std::move(host), this);
  qInfo().noquote() << QStringLiteral("CAM machines dialog opened");
  prepareModal(&dialog);
  execDialog(&dialog);
  qInfo().noquote() << QStringLiteral("CAM machines dialog closed");
}

} // namespace mitcad
