// SPDX-License-Identifier: MIT
// Turning in the application (mitcad#1869, docs/cam.md "Turning",
// app/COMMANDS.md "The CNC group"): the turning operations' panels of the
// CNC group on a lathe setup (a setup whose machine is a lathe): Turn
// Facing, Turn Roughing, Turn Finishing, Grooving, Parting Off and
// Threading. Each is an application panel like the milling operations'
// (cam/MainWindowCam.cpp): OK runs the model's cam_operation_add (or
// cam_operation_edit when a Browser row opened it), one undo step; values
// with units go to the model as typed. Generate, Simulate and Post
// Process… work on lathe setups as they do on mills.
//
// Messages a person sees say what is wrong in words; the log lines carry
// the names and numbers UI tests read.
#include "../MainWindow.hpp"
#include "../framework/Json.hpp"

#include <cmath>
#include <initializer_list>
#include <tuple>
#include <utility>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QtLogging>

#include "../commands/CommandSupport.hpp"
#include "../framework/Command.hpp"
#include "../framework/CommandRegistry.hpp"
#include "CamToolpaths.hpp"

namespace mitcad {
namespace {

using Choices = QVector<QPair<QString, QString>>;

// The turning operations by the model's `type`, with their icons.
struct TurningKind {
  const char* type;
  const char* icon;
};

constexpr TurningKind kTurningKinds[] = {
    {"turn_face", "cnc-turn-face"},     {"turn_rough", "cnc-turn-rough"}, {"turn_finish", "cnc-turn-finish"},
    {"turn_groove", "cnc-turn-groove"}, {"turn_part_off", "cnc-turn-part"}, {"turn_thread", "cnc-turn-thread"},
};

QString turningName(const QString& type) {
  if (type == QStringLiteral("turn_face")) return MainWindow::tr("Turn Facing");
  if (type == QStringLiteral("turn_rough")) return MainWindow::tr("Turn Roughing");
  if (type == QStringLiteral("turn_finish")) return MainWindow::tr("Turn Finishing");
  if (type == QStringLiteral("turn_groove")) return MainWindow::tr("Grooving");
  if (type == QStringLiteral("turn_part_off")) return MainWindow::tr("Parting Off");
  return MainWindow::tr("Threading");
}

QString turningTooltip(const QString& type) {
  if (type == QStringLiteral("turn_face")) return MainWindow::tr("Adds facing of the part's front on a lathe");
  if (type == QStringLiteral("turn_rough"))
    return MainWindow::tr("Adds roughing of the outside or the bore on a lathe, in passes along the axis");
  if (type == QStringLiteral("turn_finish"))
    return MainWindow::tr("Adds a finishing pass along the outside or the bore on a lathe");
  if (type == QStringLiteral("turn_groove")) return MainWindow::tr("Adds a groove cut with plunges on a lathe");
  if (type == QStringLiteral("turn_part_off")) return MainWindow::tr("Adds parting the part off at its back on a lathe");
  return MainWindow::tr("Adds a thread cut in passes on a lathe");
}

bool hasSide(const QString& type) {
  return type == QStringLiteral("turn_rough") || type == QStringLiteral("turn_finish") ||
         type == QStringLiteral("turn_groove") || type == QStringLiteral("turn_thread");
}

bool picksFace(const QString& type) {
  return type == QStringLiteral("turn_groove") || type == QStringLiteral("turn_thread");
}

int idOf(const QJsonObject& object) { return object.value(QStringLiteral("id")).toInt(); }

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

bool isLathe(const QJsonObject& data, int setup) {
  return machineOf(data, str(setupOf(data, setup), "machine"))
      .value(QStringLiteral("machine"))
      .toObject()
      .contains(QStringLiteral("lathe"));
}

// Whether a turning panel's setup hands the part over to its lathe's
// sub-spindle (mitcad#2319): its turning may turn the part's back there.
bool handsOver(const CommandState& state) {
  const QJsonObject data = camData(state);
  const QJsonObject setup = setupOf(data, state.choice(QStringLiteral("setup")).toInt());
  const bool sub = machineOf(data, str(setup, "machine"))
                       .value(QStringLiteral("machine"))
                       .toObject()
                       .value(QStringLiteral("lathe"))
                       .toObject()
                       .value(QStringLiteral("axes"))
                       .toObject()
                       .value(QStringLiteral("sub_spindle"))
                       .toBool();
  return sub && setup.value(QStringLiteral("lathe")).toObject().contains(QStringLiteral("hand_over"));
}

QJsonArray toolsOf(const QJsonObject& data, int setup) {
  return machineOf(data, str(setupOf(data, setup), "machine"))
      .value(QStringLiteral("tools"))
      .toObject()
      .value(QStringLiteral("tools"))
      .toArray();
}

// The lathe setups.
Choices latheSetups(const QJsonObject& data) {
  Choices choices;
  for (const QJsonValue& value : data.value(QStringLiteral("setups")).toArray()) {
    const int id = idOf(value.toObject());
    if (isLathe(data, id)) {
      choices.append(qMakePair(QString::number(id), str(value.toObject(), "name")));
    }
  }
  return choices;
}

Choices latheTools(const QJsonObject& data, int setup) {
  Choices choices;
  for (const QJsonValue& value : toolsOf(data, setup)) {
    const QJsonObject tool = value.toObject();
    const int number = tool.value(QStringLiteral("number")).toInt();
    choices.append(qMakePair(QString::number(number), QStringLiteral("%1: %2").arg(number).arg(str(tool, "name"))));
  }
  return choices;
}

// The tool a turning operation starts with: the kind it needs, on the
// side it cuts.
QString defaultLatheTool(const QJsonObject& data, int setup, const QString& type, const QString& side) {
  const bool inside = side == QStringLiteral("inside");
  QString want = QStringLiteral("turning");
  if (type == QStringLiteral("turn_groove")) {
    want = QStringLiteral("grooving");
  } else if (type == QStringLiteral("turn_part_off")) {
    want = QStringLiteral("parting");
  } else if (type == QStringLiteral("turn_thread")) {
    want = QStringLiteral("threading");
  } else if (inside) {
    want = QStringLiteral("boring");
  }
  const QJsonArray tools = toolsOf(data, setup);
  for (const QJsonValue& value : tools) {
    const QJsonObject shape = value.toObject().value(QStringLiteral("shape")).toObject();
    if (str(shape, "kind") == want && shape.value(QStringLiteral("inside")).toBool() == (inside && want != QStringLiteral("boring"))) {
      return QString::number(value.toObject().value(QStringLiteral("number")).toInt());
    }
  }
  for (const QJsonValue& value : tools) {
    if (str(value.toObject().value(QStringLiteral("shape")).toObject(), "kind") == want) {
      return QString::number(value.toObject().value(QStringLiteral("number")).toInt());
    }
  }
  return tools.isEmpty() ? QString()
                         : QString::number(tools.first().toObject().value(QStringLiteral("number")).toInt());
}

// A lathe setup the panels start with: the current one when it is on a
// lathe, else the first on a lathe.
int latheSetup(const QJsonObject& data, int current) {
  if (isLathe(data, current)) {
    return current;
  }
  const Choices setups = latheSetups(data);
  return setups.isEmpty() ? 0 : setups.first().first.toInt();
}

// A value input as the model takes it (see MainWindowCam.cpp's camValue).
QJsonValue turningValue(const CommandState& state, const QString& id, ValueKind kind, const QString& label, QString* why) {
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

void loadTurningValue(CommandState& state, const QString& id, const QJsonValue& value, ValueKind kind,
                      const CommandContext& context) {
  if (value.isString()) {
    state.setText(id, value.toString());
  } else if (value.isDouble()) {
    state.setText(id, kind == ValueKind::Length ? cmd::lengthText(value.toDouble(), context)
                                                : cam::shortNumber(value.toDouble()));
  }
}

// The fields of cam_operation_add's `operation` (and cam_operation_edit's
// changes) of a turning `type` from its panel; empty with `why` when not
// complete.
QJsonObject turningFields(const CommandState& state, const QString& type, QString* why, QString* input) {
  const bool edit = !state.object(QStringLiteral("edited")).isEmpty();
  QJsonObject op;
  const QString name = state.text(QStringLiteral("name")).trimmed();
  if (!name.isEmpty()) {
    op.insert(QStringLiteral("name"), name);
  }
  const auto value = [&](const char* id, ValueKind kind, const QString& label) {
    QString problem;
    const QJsonValue v = turningValue(state, QString::fromLatin1(id), kind, label, &problem);
    if (!problem.isEmpty() && why->isEmpty()) {
      *why = problem;
      *input = QString::fromLatin1(id);
    }
    return v;
  };
  QJsonObject kind{{QStringLiteral("type"), type}};
  if (hasSide(type)) {
    kind.insert(QStringLiteral("side"), state.choice(QStringLiteral("side")));
  }
  // How a finish (mitcad#2503) or a roughing (mitcad#2549) goes into
  // undercuts; left out: not at all.
  if (type == QStringLiteral("turn_finish") || type == QStringLiteral("turn_rough")) {
    const QString plunge = state.choice(QStringLiteral("plunge"));
    if (!plunge.isEmpty() && plunge != QStringLiteral("none")) {
      kind.insert(QStringLiteral("plunge"), plunge);
    }
  }
  const auto kindValue = [&](const char* id, const QString& label) {
    const QJsonValue v = value(id, ValueKind::Length, label);
    if (!v.isNull()) {
      kind.insert(QLatin1String(id), v);
    }
  };
  if (type == QStringLiteral("turn_groove")) {
    kindValue("z_from", MainWindow::tr("Z From"));
    kindValue("z_to", MainWindow::tr("Z To"));
  } else if (type == QStringLiteral("turn_part_off")) {
    kindValue("z", MainWindow::tr("Parting Z"));
  } else if (type == QStringLiteral("turn_thread")) {
    const QJsonValue pitch = value("pitch", ValueKind::Length, MainWindow::tr("Pitch"));
    if (pitch.isNull() && why->isEmpty()) {
      *why = MainWindow::tr("Give the thread's pitch.");
      *input = QStringLiteral("pitch");
    }
    kind.insert(QStringLiteral("pitch"), pitch);
    kindValue("diameter", MainWindow::tr("Thread Diameter"));
    kindValue("z_start", MainWindow::tr("Thread Start"));
    kindValue("z_end", MainWindow::tr("Thread End"));
    kindValue("depth", MainWindow::tr("Thread Depth"));
  }
  op.insert(QStringLiteral("kind"), kind);
  const QString tool = state.choice(QStringLiteral("tool"));
  if (tool.isEmpty()) {
    *why = MainWindow::tr("The lathe has no tools yet: add them with Machines and Tools….");
    *input = QStringLiteral("tool");
    return {};
  }
  op.insert(QStringLiteral("tool"), tool.toInt());
  // A groove's bottom face or a thread's face, each in its occurrence.
  QJsonArray inputs;
  if (picksFace(type)) {
    for (const SelectionItem& item : state.items(QStringLiteral("face"))) {
      QJsonObject face{{QStringLiteral("kind"), QStringLiteral("face")},
                       {QStringLiteral("body"), item.owner},
                       {QStringLiteral("face"), item.name}};
      if (!item.occurrence.isEmpty()) {
        face.insert(QStringLiteral("occurrence"), item.occurrence);
      }
      inputs.append(face);
    }
  }
  if (!inputs.isEmpty() || edit) {
    op.insert(QStringLiteral("inputs"), inputs);
  }
  QJsonObject heights;
  for (const auto& [id, label] : {std::pair<const char*, QString>{"clearance", MainWindow::tr("Clearance")},
                                  std::pair<const char*, QString>{"retract", MainWindow::tr("Retract")}}) {
    const QJsonValue v = value(id, ValueKind::Length, label);
    if (!v.isNull()) {
      heights.insert(QLatin1String(id), v);
    }
  }
  if (!heights.isEmpty() || edit) {
    op.insert(QStringLiteral("heights"), heights);
  }
  QJsonObject cutting;
  const QJsonValue rpm = value("spindle_rpm", ValueKind::Unitless, MainWindow::tr("Spindle Speed (rpm)"));
  if (!rpm.isNull()) {
    cutting.insert(QStringLiteral("spindle_rpm"), rpm);
  }
  if (!cutting.isEmpty() || edit) {
    op.insert(QStringLiteral("cutting"), cutting);
  }
  QJsonObject turning;
  for (const auto& [id, kindOf, label] :
       {std::tuple<const char*, ValueKind, QString>{"surface_speed", ValueKind::Unitless,
                                                    MainWindow::tr("Surface Speed (m/min)")},
        std::tuple<const char*, ValueKind, QString>{"max_rpm", ValueKind::Unitless, MainWindow::tr("Spindle Limit (rpm)")},
        std::tuple<const char*, ValueKind, QString>{"feed_per_rev", ValueKind::Length,
                                                    MainWindow::tr("Feed per Revolution")},
        std::tuple<const char*, ValueKind, QString>{"depth_of_cut", ValueKind::Length, MainWindow::tr("Depth of Cut")},
        std::tuple<const char*, ValueKind, QString>{"peck", ValueKind::Length, MainWindow::tr("Peck")},
        std::tuple<const char*, ValueKind, QString>{"stock_to_leave_z", ValueKind::Length,
                                                    MainWindow::tr("Stock to Leave along Z")}}) {
    const QJsonValue v = value(id, kindOf, label);
    if (!v.isNull()) {
      turning.insert(QLatin1String(id), v);
    }
  }
  if (state.checked(QStringLiteral("constant_rpm"))) {
    turning.insert(QStringLiteral("constant_rpm"), true);
  }
  if (!turning.isEmpty()) {
    op.insert(QStringLiteral("turning"), turning);
  } else if (edit) {
    op.insert(QStringLiteral("turning"), QJsonValue(QJsonValue::Null));
  }
  const QJsonValue leave = value("stock_to_leave", ValueKind::Length, MainWindow::tr("Stock to Leave"));
  if (!leave.isNull() || edit) {
    op.insert(QStringLiteral("stock_to_leave"), leave);
  }
  // The part's back on the sub-spindle (mitcad#2319).
  const bool sub = handsOver(state) && state.checked(QStringLiteral("sub_spindle"));
  if (sub || edit) {
    op.insert(QStringLiteral("sub_spindle"), sub);
  }
  op.insert(QStringLiteral("spindle"), state.choice(QStringLiteral("spindle")));
  op.insert(QStringLiteral("coolant"), state.choice(QStringLiteral("coolant")));
  if (!why->isEmpty()) {
    return {};
  }
  return op;
}

} // namespace

void MainWindow::registerCamTurningCommands() {
  for (const TurningKind& kind : kTurningKinds) {
    m_registry->add(camTurningCommand(QString::fromLatin1(kind.type)));
  }
}

CommandDef MainWindow::camTurningCommand(const QString& type) {
  CommandDef def;
  def.id = QStringLiteral("make.cam_") + type;
  def.name = turningName(type);
  for (const TurningKind& kind : kTurningKinds) {
    if (type == QLatin1String(kind.type)) {
      def.icon = QString::fromLatin1(kind.icon);
    }
  }
  def.tooltip = turningTooltip(type);
  def.keywords = {QStringLiteral("cam"),   QStringLiteral("cnc"),       QStringLiteral("lathe"),
                  QStringLiteral("turn"),  QStringLiteral("turning"),   QStringLiteral("toolpath"),
                  QStringLiteral("fanuc"), QStringLiteral("operation"), type};
  if (type == QStringLiteral("turn_thread")) {
    def.keywords << QStringLiteral("thread") << QStringLiteral("g76");
  } else if (type == QStringLiteral("turn_groove")) {
    def.keywords << QStringLiteral("groove") << QStringLiteral("relief");
  } else if (type == QStringLiteral("turn_part_off")) {
    def.keywords << QStringLiteral("part") << QStringLiteral("cut off");
  } else if (type == QStringLiteral("turn_rough")) {
    def.keywords << QStringLiteral("rough") << QStringLiteral("bore") << QStringLiteral("g71");
  } else if (type == QStringLiteral("turn_finish")) {
    def.keywords << QStringLiteral("finish") << QStringLiteral("bore") << QStringLiteral("g70");
  } else {
    def.keywords << QStringLiteral("face");
  }
  def.editedKind = QStringLiteral("CAM operation");
  def.enabled = [this] { return !m_snapshot.camSetups.isEmpty(); };
  const auto setupChanged = [type](CommandState& state, const CommandContext&) {
    const int setup = state.choice(QStringLiteral("setup")).toInt();
    const Choices tools = latheTools(camData(state), setup);
    const QString tool = state.choice(QStringLiteral("tool"));
    bool kept = false;
    for (const auto& choice : tools) {
      kept = kept || choice.first == tool;
    }
    if (!kept) {
      state.setChoice(QStringLiteral("tool"),
                      defaultLatheTool(camData(state), setup, type, state.choice(QStringLiteral("side"))));
    }
  };
  const auto sideChanged = [type](CommandState& state, const CommandContext&) {
    const int setup = state.choice(QStringLiteral("setup")).toInt();
    state.setChoice(QStringLiteral("tool"),
                    defaultLatheTool(camData(state), setup, type, state.choice(QStringLiteral("side"))));
  };
  const auto fromDefaults = [](const CommandState&, const CommandContext&) { return MainWindow::tr("Auto"); };
  const auto constant = [](const CommandState& state) { return state.checked(QStringLiteral("constant_rpm")); };
  def.inputs = {
      choiceInput(QStringLiteral("setup"), tr("Setup"), {}, QString())
          .withChoices([](const CommandState& state) { return latheSetups(camData(state)); })
          .withOnChange(setupChanged)
          .withTooltip(tr("A setup on a lathe the operation belongs to")),
      textInput(QStringLiteral("name"), tr("Name")).withTooltip(tr("Empty: the kind and a number")),
  };
  if (hasSide(type)) {
    def.inputs.append(choiceInput(QStringLiteral("side"), tr("Side"),
                                  {{QStringLiteral("outside"), tr("Outside")}, {QStringLiteral("inside"), tr("Bore")}},
                                  QStringLiteral("outside"))
                          .withOnChange(sideChanged)
                          .withTooltip(tr("The outside diameters or the bore")));
  }
  if (type == QStringLiteral("turn_finish") || type == QStringLiteral("turn_rough")) {
    // Into undercuts (mitcad#2503, roughing mitcad#2549): Tool geometry
    // down walls as steep as the tool's back angle less 2 degrees (its
    // insert and holder), Free down every wall. A finish never deeper
    // than the insert's depth of cut, unless a roughing before it leaves
    // less; a roughing pocket by pocket, written as moves.
    const bool rough = type == QStringLiteral("turn_rough");
    def.inputs.append(choiceInput(QStringLiteral("plunge"), tr("Plunge"),
                                  {{QStringLiteral("none"), tr("Don't plunge")},
                                   {QStringLiteral("tool"), tr("Tool geometry")},
                                   {QStringLiteral("free"), tr("Free")}},
                                  QStringLiteral("none"))
                          .withTooltip(rough ? tr("How the passes go into undercuts: walls falling towards the chuck")
                                             : tr("How the finish goes into undercuts: walls falling towards the chuck")));
  }
  if (picksFace(type)) {
    def.inputs.append(selectionInput(QStringLiteral("face"), tr("Face"), SelectKind::Face, 0, 1)
                          .withAccepts([](const SelectionItem& item) { return item.geometry == QStringLiteral("cylinder"); })
                          .withTooltip(type == QStringLiteral("turn_groove")
                                           ? tr("The groove's bottom face; or give its Z from and to")
                                           : tr("The threaded face; or give the thread's diameter, start and end")));
  }
  def.inputs.append(choiceInput(QStringLiteral("tool"), tr("Tool"), {}, QString())
                        .withChoices([](const CommandState& state) {
                          return latheTools(camData(state), state.choice(QStringLiteral("setup")).toInt());
                        })
                        .withTooltip(tr("A tool on the lathe's turret")));
  if (type == QStringLiteral("turn_groove")) {
    def.inputs.append(valueInput(QStringLiteral("z_from"), tr("Z From"), ValueKind::Length, QString())
                          .withHeading(tr("Groove"))
                          .withAuto(fromDefaults)
                          .withTooltip(tr("The groove's wall towards the face, in the work's Z; empty: from the face")));
    def.inputs.append(valueInput(QStringLiteral("z_to"), tr("Z To"), ValueKind::Length, QString())
                          .withAuto(fromDefaults)
                          .withTooltip(tr("The groove's wall towards the chuck; empty: from the face")));
  } else if (type == QStringLiteral("turn_part_off")) {
    def.inputs.append(valueInput(QStringLiteral("z"), tr("Parting Z"), ValueKind::Length, QString())
                          .withHeading(tr("Parting"))
                          .withAuto([](const CommandState&, const CommandContext&) { return MainWindow::tr("The part's back"); })
                          .withTooltip(tr("Where the part is cut off, in the work's Z")));
  } else if (type == QStringLiteral("turn_thread")) {
    def.inputs.append(valueInput(QStringLiteral("pitch"), tr("Pitch"), ValueKind::Length, QStringLiteral("1.5 mm"))
                          .withHeading(tr("Thread"))
                          .withTooltip(tr("The thread's lead per turn")));
    def.inputs.append(valueInput(QStringLiteral("diameter"), tr("Thread Diameter"), ValueKind::Length, QString())
                          .withAuto(fromDefaults)
                          .withTooltip(tr("The major diameter outside, the minor in a bore; empty: from the face")));
    def.inputs.append(valueInput(QStringLiteral("z_start"), tr("Thread Start"), ValueKind::Length, QString())
                          .withAuto(fromDefaults)
                          .withTooltip(tr("Where the thread starts, towards the face; empty: from the face")));
    def.inputs.append(valueInput(QStringLiteral("z_end"), tr("Thread End"), ValueKind::Length, QString())
                          .withAuto(fromDefaults)
                          .withTooltip(tr("Where the thread ends, towards the chuck; empty: from the face")));
    def.inputs.append(valueInput(QStringLiteral("depth"), tr("Thread Depth"), ValueKind::Length, QString())
                          .withAuto(fromDefaults)
                          .withTooltip(tr("How deep the thread is cut on its radius; empty: from the pitch")));
  }
  def.inputs.append(valueInput(QStringLiteral("surface_speed"), tr("Surface Speed (m/min)"), ValueKind::Unitless, QString())
                        .withHeading(tr("Feeds and Speeds"))
                        .withAuto(fromDefaults)
                        .withTooltip(tr("Meters per minute at the cut, kept constant as the diameter changes")));
  def.inputs.append(valueInput(QStringLiteral("max_rpm"), tr("Spindle Limit (rpm)"), ValueKind::Unitless, QString())
                        .withAuto([](const CommandState&, const CommandContext&) { return MainWindow::tr("The machine's"); })
                        .withTooltip(tr("The fastest the spindle turns near the center")));
  if (type != QStringLiteral("turn_thread")) {
    def.inputs.append(checkInput(QStringLiteral("constant_rpm"), tr("Constant spindle speed"))
                          .withTooltip(tr("Turns at a set speed instead of a constant surface speed")));
  }
  // The part's back on the sub-spindle, after the setup's hand-over
  // (mitcad#2319).
  def.inputs.append(checkInput(QStringLiteral("sub_spindle"), tr("On the sub-spindle"))
                        .withTooltip(tr("Turns the part's back after the sub-spindle has taken it over"))
                        .withVisible([](const CommandState& state) { return handsOver(state); }));
  // On the main spindle after the sub-spindle's operations (mitcad#2392):
  // Generate refuses it there; the panel says so as a warning.
  def.inputs.append(noteInput(
      QStringLiteral("spindle_order"),
      [](const CommandState& state) {
        InputNote note;
        const QJsonObject setup = setupOf(camData(state), state.choice(QStringLiteral("setup")).toInt());
        const bool sub = handsOver(state) && state.checked(QStringLiteral("sub_spindle"));
        const QJsonObject edited = state.object(QStringLiteral("edited"));
        const int editedId = edited.value(QStringLiteral("setup_id")).toInt() == idOf(setup) ? idOf(edited) : 0;
        if (cam::mainAfterSub(setup, editedId, sub)) {
          note.notice = MainWindow::tr("On the main spindle after the sub-spindle's operations: Generate refuses it.");
          note.noticeLevel = NoticeLevel::Warning;
        }
        return note;
      },
      [](int, const CommandState&, const CommandContext&) {}));
  def.inputs.append(valueInput(QStringLiteral("spindle_rpm"), tr("Spindle Speed (rpm)"), ValueKind::Unitless, QString())
                        .withAuto(fromDefaults)
                        .withTooltip(tr("Revolutions per minute; empty: from the surface speed"))
                        .withVisible([type, constant](const CommandState& state) {
                          return type == QStringLiteral("turn_thread") || constant(state);
                        }));
  if (type != QStringLiteral("turn_thread")) {
    def.inputs.append(valueInput(QStringLiteral("feed_per_rev"), tr("Feed per Revolution"), ValueKind::Length, QString())
                          .withAuto(fromDefaults)
                          .withTooltip(tr("How far the tool moves per turn of the spindle")));
  }
  if (type == QStringLiteral("turn_face") || type == QStringLiteral("turn_rough") || type == QStringLiteral("turn_thread")) {
    def.inputs.append(valueInput(QStringLiteral("depth_of_cut"), tr("Depth of Cut"), ValueKind::Length, QString())
                          .withAuto(fromDefaults)
                          .withTooltip(type == QStringLiteral("turn_thread") ? tr("The first pass's depth")
                                                                             : tr("The deepest pass")));
  }
  if (type == QStringLiteral("turn_groove") || type == QStringLiteral("turn_part_off")) {
    def.inputs.append(valueInput(QStringLiteral("peck"), tr("Peck"), ValueKind::Length, QString())
                          .withAuto([](const CommandState&, const CommandContext&) { return MainWindow::tr("In one go"); })
                          .withTooltip(tr("How deep each plunge goes before the tool backs out")));
  }
  if (type == QStringLiteral("turn_rough")) {
    def.inputs.append(valueInput(QStringLiteral("stock_to_leave"), tr("Stock to Leave"), ValueKind::Length, QString())
                          .withAuto(fromDefaults)
                          .withTooltip(tr("Material left on the diameters for finishing")));
    def.inputs.append(valueInput(QStringLiteral("stock_to_leave_z"), tr("Stock to Leave along Z"), ValueKind::Length,
                                 QString())
                          .withAuto(fromDefaults)
                          .withTooltip(tr("Material left on the shoulders for finishing")));
  }
  def.inputs.append(valueInput(QStringLiteral("clearance"), tr("Clearance"), ValueKind::Length, QString())
                        .withHeading(tr("Approach"))
                        .withAuto(fromDefaults)
                        .withTooltip(tr("How far from the stock the tool starts and comes back")));
  if (type == QStringLiteral("turn_rough")) {
    def.inputs.append(valueInput(QStringLiteral("retract"), tr("Retract"), ValueKind::Length, QString())
                          .withAuto(fromDefaults)
                          .withTooltip(tr("How far the tool backs off after each pass")));
  }
  def.inputs.append(choiceInput(QStringLiteral("spindle"), tr("Spindle"),
                                {{QStringLiteral("cw"), tr("Clockwise")}, {QStringLiteral("ccw"), tr("Counterclockwise")}},
                                QStringLiteral("cw")));
  def.inputs.append(choiceInput(QStringLiteral("coolant"), tr("Coolant"),
                                {{QStringLiteral("off"), tr("Off")}, {QStringLiteral("flood"), tr("Flood")}},
                                QStringLiteral("off")));

  def.init = [this, type](CommandState& state, const CommandContext&) {
    state.setObject(QStringLiteral("cam"), camPanelData());
    const int setup = latheSetup(camData(state), camDefaultSetup());
    state.setChoice(QStringLiteral("setup"), setup > 0 ? QString::number(setup) : QString());
    state.setChoice(QStringLiteral("tool"), defaultLatheTool(camData(state), setup, type, QStringLiteral("outside")));
    qInfo().noquote() << QStringLiteral("CAM turning panel %1: setup %2, tool %3")
                             .arg(type, setup > 0 ? str(setupOf(camData(state), setup), "name") : QStringLiteral("none"),
                                  state.choice(QStringLiteral("tool")));
  };
  def.modelChanged = [this](CommandState& state, const CommandContext&) {
    state.setObject(QStringLiteral("cam"), camPanelData());
  };
  def.load = [this](const QJsonObject& op, CommandState& state, const CommandContext& context) {
    state.setObject(QStringLiteral("edited"), op);
    state.setObject(QStringLiteral("cam"), camPanelData());
    state.setChoice(QStringLiteral("setup"), QString::number(op.value(QStringLiteral("setup_id")).toInt()));
    state.setText(QStringLiteral("name"), str(op, "name"));
    const QJsonObject kind = op.value(QStringLiteral("kind")).toObject();
    if (kind.contains(QStringLiteral("side"))) {
      state.setChoice(QStringLiteral("side"), str(kind, "side"));
    }
    state.setChoice(QStringLiteral("plunge"),
                    kind.contains(QStringLiteral("plunge")) ? str(kind, "plunge") : QStringLiteral("none"));
    for (const char* key : {"z_from", "z_to", "z", "pitch", "diameter", "z_start", "z_end", "depth"}) {
      loadTurningValue(state, QString::fromLatin1(key), kind.value(QLatin1String(key)), ValueKind::Length, context);
    }
    Selection faces;
    for (const QJsonValue& input : op.value(QStringLiteral("inputs")).toArray()) {
      const QJsonObject picked = input.toObject();
      if (str(picked, "kind") == QStringLiteral("face")) {
        faces.append(SelectionItem{SelectKind::Face, str(picked, "body"), str(picked, "face"), QStringLiteral("cylinder"),
                                   str(picked, "occurrence")});
      }
    }
    state.setItems(QStringLiteral("face"), faces);
    state.setChoice(QStringLiteral("tool"), QString::number(op.value(QStringLiteral("tool")).toInt()));
    const QJsonObject heights = op.value(QStringLiteral("heights")).toObject();
    loadTurningValue(state, QStringLiteral("clearance"), heights.value(QStringLiteral("clearance")), ValueKind::Length,
                     context);
    loadTurningValue(state, QStringLiteral("retract"), heights.value(QStringLiteral("retract")), ValueKind::Length,
                     context);
    loadTurningValue(state, QStringLiteral("spindle_rpm"),
                     op.value(QStringLiteral("cutting")).toObject().value(QStringLiteral("spindle_rpm")),
                     ValueKind::Unitless, context);
    const QJsonObject turning = op.value(QStringLiteral("turning")).toObject();
    for (const char* key : {"surface_speed", "max_rpm"}) {
      loadTurningValue(state, QString::fromLatin1(key), turning.value(QLatin1String(key)), ValueKind::Unitless, context);
    }
    for (const char* key : {"feed_per_rev", "depth_of_cut", "peck", "stock_to_leave_z"}) {
      loadTurningValue(state, QString::fromLatin1(key), turning.value(QLatin1String(key)), ValueKind::Length, context);
    }
    state.setChecked(QStringLiteral("constant_rpm"), turning.value(QStringLiteral("constant_rpm")).toBool());
    state.setChecked(QStringLiteral("sub_spindle"), op.value(QStringLiteral("sub_spindle")).toBool());
    loadTurningValue(state, QStringLiteral("stock_to_leave"), op.value(QStringLiteral("stock_to_leave")),
                     ValueKind::Length, context);
    for (const char* key : {"spindle", "coolant"}) {
      const QString chosen = str(op, key);
      if (!chosen.isEmpty()) {
        state.setChoice(QString::fromLatin1(key), chosen);
      }
    }
  };
  def.build = [type](const CommandState& state, const CommandContext&) -> Built {
    if (setupOf(camData(state), state.choice(QStringLiteral("setup")).toInt()).isEmpty()) {
      return Built::failure(MainWindow::tr("Add a setup on a lathe first: New Setup with a lathe as its machine."),
                            QStringLiteral("setup"));
    }
    QString why;
    QString input;
    const QJsonObject fields = turningFields(state, type, &why, &input);
    if (!why.isEmpty()) {
      return Built::failure(why, input);
    }
    return Built::of(fields);
  };
  def.accept = [this, type](const CommandState& state, const CommandContext&) { return acceptCamTurning(state, type); };
  def.ended = [this](bool) {
    m_camEditedOperation = 0;
    drawCamScene();
  };
  return def;
}

bool MainWindow::acceptCamTurning(const CommandState& state, const QString& type) {
  QString why;
  QString input;
  const QJsonObject fields = turningFields(state, type, &why, &input);
  if (!why.isEmpty()) {
    showHint(why);
    return false;
  }
  const int setup = state.choice(QStringLiteral("setup")).toInt();
  if (setup <= 0) {
    showHint(tr("Add a setup on a lathe first: New Setup with a lathe as its machine."));
    return false;
  }
  const QJsonObject edited = state.object(QStringLiteral("edited"));
  if (!edited.isEmpty()) {
    const int id = idOf(edited);
    if (!runModelCommand({{QStringLiteral("cmd"), QStringLiteral("cam_operation_edit")},
                          {QStringLiteral("operation"), id},
                          {QStringLiteral("changes"), fields}})) {
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

} // namespace mitcad
