// SPDX-License-Identifier: MIT
// Forms (mitcad#1757; commands.md "Form: subdivision surfaces from a
// control cage", app/COMMANDS.md "Form"):
//
// - Form (the SOLID tab's CREATE menu, a section of its own): a new Form
//   from a box, a cylinder, a sphere, a plane or a planar face, with its
//   sizes, divisions, a symmetry plane and its operation (New Body, Join,
//   Cut, Intersect with its Objects, New Component). Its preview is `form.create`
//   applied and taken back (CommandDef::sketchCommand, previewAsNew), OK
//   keeps it as one undo step.
// - Edit Form (no button: a Form's double-click in the timeline or the
//   Browser, Edit Feature; MainWindow::editForm): the cage drawn over the
//   view (view/FormCageOverlay.hpp), its faces, edges or vertices picked
//   there, and one edit at a time chosen in the panel, previewed as the
//   edited cage and its limit surface's outline (`form.*` with `preview`,
//   the lines over the view) and applied by Apply (Enter) as one undo step
//   (CommandDef::apply, repeats): Move, Rotate and Scale with their
//   handles, Extrude Faces, Insert Edge Loop, Delete Edge Loop, Crease
//   (sharp or semi-sharp), Uncrease, Delete Faces and Symmetry (about X, Y,
//   Z or a picked plane); a vertex dragged in the view is put there at
//   once, one undo step too (mitcad#1810). Close keeps the edits; Undo Last
//   Edit and Undo All Edits take this panel's edits back.
#include <cmath>
#include <exception>
#include <functional>
#include <initializer_list>
#include <limits>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include <QHash>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QObject>
#include <QString>
#include <QtLogging>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "../browser/DocumentHost.hpp"
#include "../framework/CommandRegistry.hpp"
#include "CommandFactories.hpp"
#include "CommandSupport.hpp"
#include "Commands.hpp"
#include "FormCage.hpp"

namespace mitcad {
namespace cmd {
namespace {

// --- Form (a new one)

const QString kStart = QStringLiteral("start");
const QString kFace = QStringLiteral("face");

bool startsFrom(const CommandState& state, std::initializer_list<const char*> kinds) {
  const QString start = state.choice(kStart);
  for (const char* kind : kinds) {
    if (start == QLatin1String(kind)) {
      return true;
    }
  }
  return false;
}

// A size the primitive needs: positive, else why not.
bool positiveSize(const CommandState& state, const QString& key, const QString& label, Built& error) {
  const double value = state.value(key);
  if (!std::isfinite(value) || value <= 0.0) {
    error = Built::failure(QObject::tr("%1 must be more than zero.").arg(label), key);
    return false;
  }
  return true;
}

// A count of divisions: a whole number from `least`.
std::optional<int> divisions(const CommandState& state, const QString& key, int least) {
  const double value = state.value(key);
  if (!std::isfinite(value) || value < least || std::abs(value - std::round(value)) > 1e-9 || value > 64) {
    return std::nullopt;
  }
  return static_cast<int>(std::lround(value));
}

QJsonArray centerJson(const CommandState& state) {
  QJsonArray center;
  for (const char* key : {"center_x", "center_y", "center_z"}) {
    const double value = state.value(QString::fromLatin1(key));
    center.append(std::isfinite(value) ? value : 0.0);
  }
  return center;
}

// For agents and UI tests: the bodies a Form joined, cut or intersected
// (its Objects; every body when none were picked), as the model has them
// now (mitcad#1810).
void logOperationBodies(const CommandState& state, const CommandContext& context, const QString& operation) {
  const QJsonArray participants = bodyUids(state.items(QStringLiteral("objects")));
  QJsonArray bodies;
  try {
    bodies = context.query({{QStringLiteral("query"), QStringLiteral("bodies")}, {QStringLiteral("properties"), true}})
                 .toArray();
  } catch (const std::exception&) {
    return;
  }
  for (const QJsonValue& value : bodies) {
    const QJsonObject body = value.toObject();
    const QString uid = body.value(QStringLiteral("uid")).toString();
    if (!participants.isEmpty() && !participants.contains(QJsonValue(uid))) {
      continue;
    }
    qDebug().noquote() << QStringLiteral("Form %1: body %2 %3, volume %4 mm3, %5 faces")
                              .arg(operation, uid, body.value(QStringLiteral("kind")).toString())
                              .arg(body.value(QStringLiteral("volume")).toDouble(), 0, 'f', 3)
                              .arg(body.value(QStringLiteral("faces")).toInt());
  }
}

} // namespace

CommandDef formCommand(const CommandContext& context) {
  Q_UNUSED(context);
  CommandDef def;
  def.id = QStringLiteral("solid.form");
  def.name = QObject::tr("Form");
  def.icon = QStringLiteral("form");
  def.tooltip = QObject::tr("A smooth body shaped by a control cage that you edit");
  def.group = QStringLiteral("CREATE");
  def.section = QStringLiteral("form"); // a section of its own (mitcad#1757)
  def.keywords = {QStringLiteral("subdivision"), QStringLiteral("cage"), QStringLiteral("organic"),
                  QStringLiteral("smooth"), QStringLiteral("freeform"), QStringLiteral("sculpt")};
  // form.create applied for the preview and taken back; OK keeps it.
  def.sketchCommand = true;
  def.previewAsNew = true;
  const auto boxOrPlane = [](const CommandState& state) { return startsFrom(state, {"box", "plane"}); };
  const auto rounded = [](const CommandState& state) { return startsFrom(state, {"cylinder", "sphere"}); };
  const auto tall = [](const CommandState& state) { return startsFrom(state, {"box", "cylinder"}); };
  const auto onFace = [](const CommandState& state) { return startsFrom(state, {"face"}); };
  const auto placed = [](const CommandState& state) { return !startsFrom(state, {"face"}); };
  def.inputs = {
      choiceInput(kStart, QObject::tr("Start From"),
                  {{QStringLiteral("box"), QStringLiteral("Box")},
                   {QStringLiteral("cylinder"), QStringLiteral("Cylinder")},
                   {QStringLiteral("sphere"), QStringLiteral("Sphere")},
                   {QStringLiteral("plane"), QStringLiteral("Plane")},
                   {QStringLiteral("face"), QStringLiteral("Face")}},
                  QStringLiteral("box"))
          .withTooltip(QObject::tr("The cage the Form starts from")),
      selectionInput(kFace, QObject::tr("Face"), SelectKind::Face, 1, 1)
          .withAccepts([](const SelectionItem& item) { return isPlanar(item); })
          .withoutMeshes()
          .withVisible(onFace)
          .withTooltip(QObject::tr("A planar face: its outline is the cage")),
      valueInput(QStringLiteral("thickness"), QObject::tr("Thickness"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(onFace)
          .withTooltip(QObject::tr("0 makes a surface on the face; more makes a solid standing on it")),
      valueInput(QStringLiteral("length"), QObject::tr("Length"), ValueKind::Length, QStringLiteral("40 mm"))
          .withStepMinimum(0.0, true)
          .withVisible(boxOrPlane),
      valueInput(QStringLiteral("width"), QObject::tr("Width"), ValueKind::Length, QStringLiteral("40 mm"))
          .withStepMinimum(0.0, true)
          .withVisible(boxOrPlane),
      valueInput(QStringLiteral("height"), QObject::tr("Height"), ValueKind::Length, QStringLiteral("40 mm"))
          .withStepMinimum(0.0, true)
          .withVisible(tall),
      valueInput(QStringLiteral("diameter"), QObject::tr("Diameter"), ValueKind::Length, QStringLiteral("40 mm"))
          .withStepMinimum(0.0, true)
          .withVisible(rounded),
      valueInput(QStringLiteral("divisions"), QObject::tr("Divisions"), ValueKind::Unitless, QStringLiteral("2"))
          .withStepMinimum(1.0)
          .withVisible(placed)
          .withTooltip(QObject::tr("How many cage faces along each side; a cylinder's rows")),
      valueInput(QStringLiteral("around"), QObject::tr("Around"), ValueKind::Unitless, QStringLiteral("8"))
          .withStepMinimum(3.0)
          .withVisible([](const CommandState& state) { return startsFrom(state, {"cylinder"}); })
          .withTooltip(QObject::tr("How many cage faces round the cylinder")),
      valueInput(QStringLiteral("center_x"), QObject::tr("Center X"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(placed),
      valueInput(QStringLiteral("center_y"), QObject::tr("Center Y"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(placed),
      valueInput(QStringLiteral("center_z"), QObject::tr("Center Z"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(placed),
      choiceInput(QStringLiteral("symmetry"), QObject::tr("Symmetry"),
                  {{QStringLiteral("none"), QStringLiteral("None")},
                   {QStringLiteral("x"), QStringLiteral("X")},
                   {QStringLiteral("y"), QStringLiteral("Y")},
                   {QStringLiteral("z"), QStringLiteral("Z")}},
                  QStringLiteral("none"))
          .withVisible(placed)
          .withTooltip(QObject::tr("A mirror plane through the center: an edit on one side is mirrored on the other")),
  };
  // New Body, Join, Cut, Intersect (with the Objects they work on) and New
  // Component, as the other body features (mitcad#1810).
  def.inputs += operationInputs();

  def.build = [](const CommandState& state, const CommandContext&) -> Built {
    Built error;
    QJsonObject primitive;
    const QString start = state.choice(kStart);
    const std::optional<int> parts = divisions(state, QStringLiteral("divisions"), 1);
    if (start != QStringLiteral("face") && !parts) {
      return Built::failure(QObject::tr("Divisions must be a whole number from 1 to 64."), QStringLiteral("divisions"));
    }
    if (start == QStringLiteral("box")) {
      if (!positiveSize(state, QStringLiteral("length"), QObject::tr("Length"), error) ||
          !positiveSize(state, QStringLiteral("width"), QObject::tr("Width"), error) ||
          !positiveSize(state, QStringLiteral("height"), QObject::tr("Height"), error)) {
        return error;
      }
      primitive = {{QStringLiteral("type"), QStringLiteral("box")},
                   {QStringLiteral("size"), QJsonArray{state.value(QStringLiteral("length")),
                                                       state.value(QStringLiteral("width")),
                                                       state.value(QStringLiteral("height"))}},
                   {QStringLiteral("segments"), QJsonArray{*parts, *parts, *parts}}};
    } else if (start == QStringLiteral("cylinder")) {
      const std::optional<int> around = divisions(state, QStringLiteral("around"), 3);
      if (!around) {
        return Built::failure(QObject::tr("Around must be a whole number from 3 to 64."), QStringLiteral("around"));
      }
      if (!positiveSize(state, QStringLiteral("diameter"), QObject::tr("Diameter"), error) ||
          !positiveSize(state, QStringLiteral("height"), QObject::tr("Height"), error)) {
        return error;
      }
      primitive = {{QStringLiteral("type"), QStringLiteral("cylinder")},
                   {QStringLiteral("diameter"), state.value(QStringLiteral("diameter"))},
                   {QStringLiteral("height"), state.value(QStringLiteral("height"))},
                   {QStringLiteral("segments"), *around},
                   {QStringLiteral("rows"), *parts}};
    } else if (start == QStringLiteral("sphere")) {
      if (!positiveSize(state, QStringLiteral("diameter"), QObject::tr("Diameter"), error)) {
        return error;
      }
      primitive = {{QStringLiteral("type"), QStringLiteral("sphere")},
                   {QStringLiteral("diameter"), state.value(QStringLiteral("diameter"))},
                   {QStringLiteral("segments"), *parts}};
    } else if (start == QStringLiteral("plane")) {
      if (!positiveSize(state, QStringLiteral("length"), QObject::tr("Length"), error) ||
          !positiveSize(state, QStringLiteral("width"), QObject::tr("Width"), error)) {
        return error;
      }
      primitive = {{QStringLiteral("type"), QStringLiteral("plane")},
                   {QStringLiteral("size"), QJsonArray{state.value(QStringLiteral("length")),
                                                       state.value(QStringLiteral("width"))}},
                   {QStringLiteral("segments"), QJsonArray{*parts, *parts}}};
    } else {
      const Selection& faces = state.items(kFace);
      if (faces.isEmpty()) {
        return Built::failure(QObject::tr("Pick a planar face."), kFace);
      }
      const SelectionItem& face = faces.first();
      if (!face.occurrence.isEmpty()) {
        // form.create links no occurrence: the face's own component only.
        return Built::failure(QObject::tr("Pick a face of the active component's own bodies."), kFace);
      }
      primitive = {{QStringLiteral("type"), QStringLiteral("face")},
                   {QStringLiteral("body"), face.owner},
                   {QStringLiteral("face"), face.name}};
      const double thickness = state.value(QStringLiteral("thickness"));
      if (std::isfinite(thickness) && thickness > 0.0) {
        primitive.insert(QStringLiteral("thickness"), thickness);
      } else if (!std::isfinite(thickness) || thickness < 0.0) {
        return Built::failure(QObject::tr("Thickness must be zero or more."), QStringLiteral("thickness"));
      }
    }
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("form.create")},
                        {QStringLiteral("primitive"), primitive}};
    addOperation(command, state); // `operation`, and `participants` for Join, Cut, Intersect
    if (start != QStringLiteral("face")) {
      command.insert(QStringLiteral("center"), centerJson(state));
      const QString symmetry = state.choice(QStringLiteral("symmetry"));
      if (symmetry == QStringLiteral("x") || symmetry == QStringLiteral("y") || symmetry == QStringLiteral("z")) {
        command.insert(QStringLiteral("symmetry"), symmetry);
      }
    }
    return Built::of(command);
  };
  def.describe = [](const CommandState& state, const CommandContext& context) {
    const QString operation = state.choice(QStringLiteral("operation"));
    if (operation == QStringLiteral("join") || operation == QStringLiteral("cut") ||
        operation == QStringLiteral("intersect")) {
      logOperationBodies(state, context, operation);
    }
    return QStringLiteral("Added Form from a %1").arg(state.choice(kStart));
  };
  return def;
}

namespace {

// --- Edit Form

const QString kForm = QStringLiteral("form");     // hidden: the Form's uid
const QString kPicked = QStringLiteral("picked"); // hidden: the picks (forms::pickedText)
const QString kCage = QStringLiteral("cage");     // the form_cage answer (state object)
const QString kAction = QStringLiteral("action");
const QString kPick = QStringLiteral("pick");
const QString kAxis = QStringLiteral("axis");
const QString kPlane = QStringLiteral("plane");
const QString kDrag = QStringLiteral("drag");               // hidden: a vertex dragged (forms::dragText)
const QString kCreaseType = QStringLiteral("crease_type");  // Sharp or Semi-Sharp
const QString kSharpness = QStringLiteral("sharpness");
const QString kMirror = QStringLiteral("mirror");           // a picked plane or planar face
const QString kMirrorPlane = QStringLiteral("mirror_plane"); // it resolved (state object)

forms::Action actionIn(const CommandState& state) {
  return forms::actionOf(state.choice(kAction)).value_or(forms::Action::Move);
}

// The kind a click picks now: the edit's only one, else the Select choice's.
forms::Kind kindIn(const CommandState& state) {
  const forms::Action action = actionIn(state);
  if (const std::optional<forms::Kind> only = forms::onlyKind(action)) {
    return *only;
  }
  const forms::Kind chosen = forms::kindOf(state.choice(kPick));
  if (forms::takesKind(action, chosen)) {
    return chosen;
  }
  return action == forms::Action::Crease || action == forms::Action::Uncrease ? forms::Kind::Edges
                                                                             : forms::Kind::Faces;
}

forms::Picked pickedIn(const CommandState& state) { return forms::pickedOf(state.text(kPicked)); }

forms::Cage cageIn(const CommandState& state) { return forms::cageOf(state.object(kCage)); }

double valueOr(const CommandState& state, const QString& key, double fallback) {
  const double value = state.value(key);
  return std::isfinite(value) ? value : fallback;
}

forms::Edit editIn(const CommandState& state) {
  forms::Edit edit;
  edit.action = actionIn(state);
  edit.move = {valueOr(state, QStringLiteral("move_x"), 0.0), valueOr(state, QStringLiteral("move_y"), 0.0),
               valueOr(state, QStringLiteral("move_z"), 0.0)};
  const QString axis = state.choice(kAxis);
  edit.axis = axis == QStringLiteral("x") ? 0 : axis == QStringLiteral("y") ? 1 : 2;
  edit.angle = valueOr(state, QStringLiteral("angle"), 0.0);
  edit.factor = valueOr(state, QStringLiteral("factor"), 1.0);
  edit.distance = valueOr(state, QStringLiteral("distance"), 0.0);
  edit.at = valueOr(state, QStringLiteral("at"), 0.5);
  if (edit.action == forms::Action::Crease && state.choice(kCreaseType) == QStringLiteral("semi")) {
    edit.sharpness = state.value(kSharpness); // NaN: refused in words
  }
  edit.plane = state.choice(kPlane);
  const QJsonObject mirror = state.object(kMirrorPlane);
  if (edit.plane == QStringLiteral("picked") && !mirror.isEmpty()) {
    const QJsonArray origin = mirror.value(QStringLiteral("origin")).toArray();
    const QJsonArray normal = mirror.value(QStringLiteral("normal")).toArray();
    if (origin.size() == 3 && normal.size() == 3) {
      edit.planePicked = true;
      for (int k = 0; k < 3; ++k) {
        edit.planeOrigin[static_cast<std::size_t>(k)] = origin.at(k).toDouble();
        edit.planeNormal[static_cast<std::size_t>(k)] = normal.at(k).toDouble();
      }
    }
  }
  return edit;
}

// The picked mirror plane (a plane or a planar face, mitcad#1810) in the
// Form's component's coordinates: the model's `section_plane` gives it in
// the design's, `placement` (the component's first occurrence; none: the
// root's) takes it back. An empty object when there is none or it cannot
// be found.
QJsonObject mirrorPlaneOf(const CommandState& state, const CommandContext& context,
                          const std::optional<gp_Trsf>& placement) {
  const Selection& items = state.items(kMirror);
  if (items.isEmpty()) {
    return {};
  }
  const SelectionItem& item = items.first();
  QJsonObject request{{QStringLiteral("query"), QStringLiteral("section_plane")},
                      {QStringLiteral("plane"), item.reference()}};
  if (!item.occurrence.isEmpty()) {
    request.insert(QStringLiteral("occurrence"), item.occurrence);
  }
  QJsonObject answer;
  try {
    answer = context.queryObject(request);
  } catch (const std::exception& failure) {
    qDebug().noquote() << QStringLiteral("Form edit mirror plane %1 not found: %2")
                              .arg(item.logDescription(), QString::fromUtf8(failure.what()));
    return {};
  }
  const QJsonArray o = answer.value(QStringLiteral("origin")).toArray();
  const QJsonArray n = answer.value(QStringLiteral("normal")).toArray();
  if (o.size() != 3 || n.size() != 3) {
    return {};
  }
  gp_Pnt origin(o.at(0).toDouble(), o.at(1).toDouble(), o.at(2).toDouble());
  gp_Vec normal(n.at(0).toDouble(), n.at(1).toDouble(), n.at(2).toDouble());
  if (normal.Magnitude() < 1e-12) {
    return {};
  }
  if (placement) {
    const gp_Trsf back = placement->Inverted();
    origin.Transform(back);
    normal.Transform(back);
  }
  normal.Normalize();
  const QJsonObject plane{{QStringLiteral("origin"), QJsonArray{origin.X(), origin.Y(), origin.Z()}},
                          {QStringLiteral("normal"), QJsonArray{normal.X(), normal.Y(), normal.Z()}}};
  qDebug().noquote() << QStringLiteral("Form edit mirror plane %1: %2")
                            .arg(item.logDescription(),
                                 QString::fromUtf8(QJsonDocument(plane).toJson(QJsonDocument::Compact)));
  return plane;
}

gp_Pnt pnt(const forms::Point& p) { return gp_Pnt(p[0], p[1], p[2]); }

// The values the edits start from again after one was applied.
void resetValues(CommandState& state) {
  const std::vector<std::pair<QString, QString>> starts{
      {QStringLiteral("move_x"), QStringLiteral("0 mm")}, {QStringLiteral("move_y"), QStringLiteral("0 mm")},
      {QStringLiteral("move_z"), QStringLiteral("0 mm")}, {QStringLiteral("angle"), QStringLiteral("0 deg")},
      {QStringLiteral("factor"), QStringLiteral("1")}};
  for (const auto& [key, text] : starts) {
    state.setText(key, text);
    state.setExpression(key, text);
    state.setValue(key, std::numeric_limits<double>::quiet_NaN()); // evaluated again
  }
}

// What the edits of this panel share: how many it applied (Undo Last Edit
// and Undo All Edits take them back), and the last preview's lines.
struct Shared {
  int edits = 0;
  QByteArray previewKey;
  std::vector<std::vector<gp_Pnt>> previewLines;
  QString previewError;
};

// The cage, the picks and what a click picks to the view.
void publish(const FormEditHooks& hooks, const CommandState& state) {
  if (!hooks.show) {
    return;
  }
  const forms::Action action = actionIn(state);
  hooks.show(state.object(kCage), state.text(kPicked), forms::kindKey(kindIn(state)),
             action != forms::Action::Symmetry);
}

// The cage read again (after an edit, an undo): the picks it no longer has
// go.
void readCage(CommandState& state, const CommandContext& context) {
  const QString form = state.text(kForm);
  QJsonObject cage;
  try {
    cage = context.queryObject({{QStringLiteral("query"), QStringLiteral("form_cage")}, {QStringLiteral("uid"), form}});
  } catch (const std::exception& failure) {
    qWarning().noquote() << QStringLiteral("Form edit: the cage of %1 was not read: %2")
                                .arg(form, QString::fromUtf8(failure.what()));
  }
  state.setObject(kCage, cage);
  const forms::Cage read = forms::cageOf(cage);
  state.setText(kPicked, forms::pickedText(forms::keptIn(pickedIn(state), read)));
  qDebug().noquote() << QStringLiteral("Form edit %1: cage of %2 vertices, %3 edges, %4 faces%5%6")
                            .arg(read.name)
                            .arg(read.vertices.size())
                            .arg(read.edges.size())
                            .arg(read.faces.size())
                            .arg(read.closed ? QStringLiteral(", closed") : QStringLiteral(", open"))
                            .arg(read.symmetric ? QStringLiteral(", symmetric") : QString());
}

// For agents and UI tests: the Form's body after an edit, whether it is a
// solid, and on a symmetric cage whether the two halves are equal (the
// body's box even about a plane square to an axis).
void logBody(const CommandState& state, const CommandContext& context) {
  const QString form = state.text(kForm);
  const forms::Cage cage = cageIn(state);
  QJsonArray bodies;
  try {
    bodies = context.query({{QStringLiteral("query"), QStringLiteral("bodies")}, {QStringLiteral("properties"), true}})
                 .toArray();
  } catch (const std::exception&) {
    return;
  }
  bool any = false;
  for (const QJsonValue& value : bodies) {
    const QJsonObject body = value.toObject();
    const QString uid = body.value(QStringLiteral("uid")).toString();
    if (!uid.startsWith(form + QLatin1Char('.'))) {
      continue;
    }
    any = true;
    const QJsonObject box = body.value(QStringLiteral("bbox")).toObject();
    const QJsonArray low = box.value(QStringLiteral("min")).toArray();
    const QJsonArray high = box.value(QStringLiteral("max")).toArray();
    QString halves;
    if (cage.symmetric && low.size() == 3 && high.size() == 3) {
      for (int k = 0; k < 3; ++k) {
        if (std::abs(std::abs(cage.symmetryNormal[static_cast<std::size_t>(k)]) - 1.0) > 1e-9) {
          continue;
        }
        const double middle = (low.at(k).toDouble() + high.at(k).toDouble()) / 2.0;
        const bool equal = std::abs(middle - cage.symmetryOrigin[static_cast<std::size_t>(k)]) <= 1e-6;
        halves = equal ? QStringLiteral("; halves equal about the symmetry plane")
                       : QStringLiteral("; halves not equal about the symmetry plane");
      }
    }
    qDebug().noquote() << QStringLiteral("Form edit %1: body %2 %3, volume %4 mm3, %5 faces%6")
                              .arg(cage.name, uid, body.value(QStringLiteral("kind")).toString())
                              .arg(body.value(QStringLiteral("volume")).toDouble(), 0, 'f', 3)
                              .arg(body.value(QStringLiteral("faces")).toInt())
                              .arg(halves);
  }
  if (!any) {
    qDebug().noquote() << QStringLiteral("Form edit %1: no body").arg(cage.name);
  }
}

// An arrow at the picks' middle (in the Form's component's coordinates;
// handlesSeenAt places them).
std::optional<Manipulator> pickArrow(const CommandState& state, const gp_Dir& direction) {
  const forms::Cage cage = cageIn(state);
  const std::optional<forms::Point> middle = forms::centerOf(cage, pickedIn(state));
  if (!middle) {
    return std::nullopt;
  }
  Manipulator handle = arrow(pnt(*middle), direction);
  handle.valueBox = true;
  handle.boxAtZero = false;
  handle.scale = forms::sizeOf(cage, pickedIn(state));
  return handle;
}

std::function<std::optional<Manipulator>(const CommandState&, const CommandContext&)> moveHandle(int axis) {
  return [axis](const CommandState& state, const CommandContext&) -> std::optional<Manipulator> {
    std::optional<Manipulator> handle =
        pickArrow(state, gp_Dir(axis == 0 ? 1 : 0, axis == 1 ? 1 : 0, axis == 2 ? 1 : 0));
    if (handle) {
      handle->color = axis == 0 ? ColorRole::AxisX : axis == 1 ? ColorRole::AxisY : ColorRole::AxisZ;
    }
    return handle;
  };
}

CommandDef formEditCommand(DocumentHost& host, const FormEditHooks& hooks) {
  CommandDef def;
  def.id = QString::fromLatin1(kFormEditCommand);
  def.name = QObject::tr("Edit Form");
  def.icon = QStringLiteral("form");
  def.tooltip = QObject::tr("Changes a Form's control cage");
  def.tab.clear(); // a Form's double-click or Edit Feature opens it
  def.group = QStringLiteral("CREATE");
  def.keywords = {QStringLiteral("cage"), QStringLiteral("subdivision")};
  def.okLabel = QObject::tr("Apply");
  def.repeats = true; // each edit one undo step; Close keeps them
  def.enabled = [hooks] { return hooks.form && !hooks.form().isEmpty(); };
  const auto shared = std::make_shared<Shared>();
  const auto is = [](forms::Action action) {
    return [action](const CommandState& state) { return actionIn(state) == action; };
  };
  def.inputs = {
      textInput(kForm, QObject::tr("Form")).withVisible([](const CommandState&) { return false; }),
      textInput(kPicked, QObject::tr("Picked")).withVisible([](const CommandState&) { return false; })
          .withOnChange([hooks](CommandState& state, const CommandContext&) {
            qDebug().noquote() << QStringLiteral("Form edit picked: %1").arg(state.text(kPicked));
            publish(hooks, state);
          }),
      // A vertex dragged in the view (mitcad#1810; the main window sets it
      // when the drag ends): put there by `form.set_positions`, one undo
      // step that Undo Last Edit takes back as the panel's other edits.
      textInput(kDrag, QObject::tr("Drag")).withVisible([](const CommandState&) { return false; })
          .withOnChange([&host, hooks, shared](CommandState& state, const CommandContext& context) {
            const std::optional<forms::Drag> drag = forms::dragOf(state.text(kDrag));
            state.setText(kDrag, QString());
            if (!drag) {
              return;
            }
            const QJsonObject command = forms::dragCommand(state.text(kForm), *drag, false);
            const QString name = cageIn(state).name;
            if (host.runModelCommand(command)) {
              ++shared->edits;
              qDebug().noquote() << QStringLiteral("Form edit Drag in %1: done: %2")
                                        .arg(name, QString::fromUtf8(QJsonDocument(command).toJson(QJsonDocument::Compact)));
              readCage(state, context);
              logBody(state, context);
            } else {
              qDebug().noquote() << QStringLiteral("Form edit Drag in %1: refused").arg(name);
              readCage(state, context); // the cage drawn where it is
            }
            publish(hooks, state);
          }),
      choiceInput(kAction, QObject::tr("Edit"),
                  {{QStringLiteral("move"), QStringLiteral("Move")},
                   {QStringLiteral("rotate"), QStringLiteral("Rotate")},
                   {QStringLiteral("scale"), QStringLiteral("Scale")},
                   {QStringLiteral("extrude"), QStringLiteral("Extrude Faces")},
                   {QStringLiteral("insert_loop"), QStringLiteral("Insert Edge Loop")},
                   {QStringLiteral("delete_loop"), QStringLiteral("Delete Edge Loop")},
                   {QStringLiteral("crease"), QStringLiteral("Crease")},
                   {QStringLiteral("uncrease"), QStringLiteral("Uncrease")},
                   {QStringLiteral("delete_faces"), QStringLiteral("Delete Faces")},
                   {QStringLiteral("symmetry"), QStringLiteral("Symmetry")}},
                  QStringLiteral("move"))
          .withTooltip(QObject::tr("What Apply does to the picks"))
          .withOnChange([hooks](CommandState& state, const CommandContext&) {
            // The picks of a kind the edit does not take go.
            const forms::Kind kind = kindIn(state);
            state.setChoice(kPick, forms::kindKey(kind));
            forms::Picked picked = pickedIn(state);
            forms::Picked kept;
            if (kind == forms::Kind::Faces) {
              kept.faces = picked.faces;
            } else if (kind == forms::Kind::Edges) {
              kept.edges = picked.edges;
            } else {
              kept.vertices = picked.vertices;
            }
            if (actionIn(state) == forms::Action::Symmetry) {
              kept = forms::Picked();
            }
            state.setText(kPicked, forms::pickedText(kept));
            publish(hooks, state);
          }),
      choiceInput(kPick, QObject::tr("Select"),
                  {{QStringLiteral("faces"), QStringLiteral("Faces")},
                   {QStringLiteral("edges"), QStringLiteral("Edges")},
                   {QStringLiteral("vertices"), QStringLiteral("Vertices")}},
                  QStringLiteral("faces"))
          .withChoices([](const CommandState& state) {
            const forms::Action action = actionIn(state);
            Choices options;
            if (forms::takesKind(action, forms::Kind::Faces)) {
              options.append({QStringLiteral("faces"), QStringLiteral("Faces")});
            }
            options.append({QStringLiteral("edges"), QStringLiteral("Edges")});
            options.append({QStringLiteral("vertices"), QStringLiteral("Vertices")});
            return options;
          })
          .withVisible([](const CommandState& state) {
            const forms::Action action = actionIn(state);
            return !forms::onlyKind(action) && action != forms::Action::Symmetry;
          })
          .withTooltip(QObject::tr("What a click on the cage picks"))
          .withOnChange([hooks](CommandState& state, const CommandContext&) {
            state.setText(kPicked, forms::pickedText(forms::Picked()));
            publish(hooks, state);
          }),
      noteInput(
          QStringLiteral("picks"),
          [](const CommandState& state) {
            InputNote note;
            const forms::Picked picked = pickedIn(state);
            if (actionIn(state) == forms::Action::Symmetry) {
              return note;
            }
            if (picked.isEmpty()) {
              note.text = forms::editRefusal(editIn(state), picked);
              if (note.text.isEmpty()) {
                note.text = QObject::tr("Click the cage in the view to pick; Ctrl+click adds.");
              }
            } else {
              note.text = QObject::tr("Picked: %1").arg(forms::pickedWords(picked));
              note.actions = {QObject::tr("Clear")};
            }
            return note;
          },
          [&host](int, const CommandState&, const CommandContext&) {
            host.chooseInCommand(QString::fromLatin1(kFormEditCommand), kPicked, forms::pickedText(forms::Picked()));
          }),
      valueInput(QStringLiteral("move_x"), QObject::tr("X Distance"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(is(forms::Action::Move))
          .withManipulator(moveHandle(0)),
      valueInput(QStringLiteral("move_y"), QObject::tr("Y Distance"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(is(forms::Action::Move))
          .withManipulator(moveHandle(1)),
      valueInput(QStringLiteral("move_z"), QObject::tr("Z Distance"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(is(forms::Action::Move))
          .withManipulator(moveHandle(2)),
      choiceInput(kAxis, QObject::tr("Axis"),
                  {{QStringLiteral("x"), QStringLiteral("X")},
                   {QStringLiteral("y"), QStringLiteral("Y")},
                   {QStringLiteral("z"), QStringLiteral("Z")}},
                  QStringLiteral("z"))
          .withVisible(is(forms::Action::Rotate))
          .withTooltip(QObject::tr("The axis through the picks' middle they turn about")),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withVisible(is(forms::Action::Rotate))
          .withManipulator([](const CommandState& state, const CommandContext&) -> std::optional<Manipulator> {
            const forms::Cage cage = cageIn(state);
            const std::optional<forms::Point> middle = forms::centerOf(cage, pickedIn(state));
            if (!middle) {
              return std::nullopt;
            }
            const int axis = editIn(state).axis;
            const gp_Dir direction(axis == 0 ? 1 : 0, axis == 1 ? 1 : 0, axis == 2 ? 1 : 0);
            Manipulator handle = ring(pnt(*middle), direction);
            handle.reference = axis == 0 ? gp_Dir(0, 1, 0) : gp_Dir(1, 0, 0);
            handle.valueBox = true;
            handle.boxAtZero = false;
            return handle;
          }),
      valueInput(QStringLiteral("factor"), QObject::tr("Scale Factor"), ValueKind::Unitless, QStringLiteral("1"))
          .withStepMinimum(0.0, true)
          .withVisible(is(forms::Action::Scale))
          .withManipulator([](const CommandState& state, const CommandContext&) -> std::optional<Manipulator> {
            std::optional<Manipulator> handle = pickArrow(state, gp_Dir(1, 0, 0));
            if (handle) {
              // The knob a size's distance out at a factor of 1.
              handle->factor = forms::sizeOf(cageIn(state), pickedIn(state));
              handle->minimum = 0.05;
              handle->scale = 0.0;
              handle->boxAtZero = true;
            }
            return handle;
          }),
      valueInput(QStringLiteral("distance"), QObject::tr("Distance"), ValueKind::Length, QStringLiteral("5 mm"))
          .withVisible(is(forms::Action::Extrude))
          .withTooltip(QObject::tr("Along the faces' normals; negative goes inwards"))
          .withManipulator([](const CommandState& state, const CommandContext&) -> std::optional<Manipulator> {
            const std::optional<forms::Point> normal = forms::normalOf(cageIn(state), pickedIn(state));
            if (!normal) {
              return std::nullopt;
            }
            return pickArrow(state, gp_Dir((*normal)[0], (*normal)[1], (*normal)[2]));
          }),
      valueInput(QStringLiteral("at"), QObject::tr("Position"), ValueKind::Unitless, QStringLiteral("0.5"))
          .withVisible(is(forms::Action::InsertLoop))
          .withTooltip(QObject::tr("Where the loop crosses the picked edge: 0 at one end, 1 at the other"))
          .withManipulator([](const CommandState& state, const CommandContext&) -> std::optional<Manipulator> {
            const forms::Cage cage = cageIn(state);
            const forms::Picked picked = pickedIn(state);
            if (picked.edges.size() != 1) {
              return std::nullopt;
            }
            const forms::Vertex* a = cage.vertex(picked.edges.first()[0]);
            const forms::Vertex* b = cage.vertex(picked.edges.first()[1]);
            if (a == nullptr || b == nullptr) {
              return std::nullopt;
            }
            const gp_Pnt from = pnt(a->at);
            const gp_Pnt to = pnt(b->at);
            const double length = from.Distance(to);
            if (length < 1e-9) {
              return std::nullopt;
            }
            Manipulator handle = arrow(from, gp_Dir(gp_Vec(from, to)));
            handle.factor = length;
            handle.minimum = 0.0;
            return handle;
          }),
      // Crease: sharp for good, or semi-sharp for a number of subdivision
      // steps, which rounds the edge a little (mitcad#1810).
      choiceInput(kCreaseType, QObject::tr("Type"),
                  {{QStringLiteral("sharp"), QStringLiteral("Sharp")},
                   {QStringLiteral("semi"), QStringLiteral("Semi-Sharp")}},
                  QStringLiteral("sharp"))
          .withVisible(is(forms::Action::Crease))
          .withTooltip(QObject::tr("Sharp keeps the edges sharp; Semi-Sharp rounds them a little")),
      valueInput(kSharpness, QObject::tr("Sharpness"), ValueKind::Unitless, QStringLiteral("2"))
          .withStepMinimum(0.0, true)
          .withVisible([](const CommandState& state) {
            return actionIn(state) == forms::Action::Crease && state.choice(kCreaseType) == QStringLiteral("semi");
          })
          .withTooltip(QObject::tr("How many subdivision steps the edges stay sharp; more is sharper")),
      choiceInput(kPlane, QObject::tr("Plane"),
                  {{QStringLiteral("none"), QStringLiteral("None")},
                   {QStringLiteral("x"), QStringLiteral("X")},
                   {QStringLiteral("y"), QStringLiteral("Y")},
                   {QStringLiteral("z"), QStringLiteral("Z")},
                   {QStringLiteral("picked"), QStringLiteral("Picked Plane")}},
                  QStringLiteral("x"))
          .withVisible(is(forms::Action::Symmetry))
          .withTooltip(QObject::tr("A plane square to this axis through the cage's middle, or one you pick; None frees both")),
      // Symmetry about any plane (mitcad#1810): an origin or construction
      // plane, or a planar face; the half on the side its normal points to
      // is kept.
      selectionInput(kMirror, QObject::tr("Mirror Plane"), SelectKind::Plane | SelectKind::Face, 1, 1)
          .withAccepts([](const SelectionItem& item) { return item.kind != SelectKind::Face || isPlanar(item); })
          .withoutMeshes()
          .withVisible([](const CommandState& state) {
            return actionIn(state) == forms::Action::Symmetry && state.choice(kPlane) == QStringLiteral("picked");
          })
          .withTooltip(QObject::tr("The plane the cage is mirrored about: the half on the side it faces is kept"))
          .withOnChange([hooks](CommandState& state, const CommandContext& context) {
            state.setObject(kMirrorPlane,
                            mirrorPlaneOf(state, context, hooks.placement ? hooks.placement() : std::nullopt));
          }),
      noteInput(
          QStringLiteral("edits"),
          [shared](const CommandState&) {
            InputNote note;
            if (shared->edits > 0) {
              note.text = QObject::tr("%n edit(s) applied in this panel", nullptr, shared->edits);
              note.actions = {QObject::tr("Undo Last Edit"), QObject::tr("Undo All Edits")};
            }
            return note;
          },
          [shared, &host](int action, const CommandState&, const CommandContext&) {
            const int count = action == 0 ? 1 : shared->edits;
            for (int i = 0; i < count && shared->edits > 0; ++i) {
              if (!host.runModelCommand({{QStringLiteral("cmd"), QStringLiteral("undo")}})) {
                break;
              }
              --shared->edits;
            }
            qDebug().noquote() << QStringLiteral("Form edit undone: %1 edit(s) left").arg(shared->edits);
          }),
  };

  def.init = [hooks, shared](CommandState& state, const CommandContext& context) {
    *shared = Shared();
    state.setText(kForm, hooks.form ? hooks.form() : QString());
    state.setText(kPicked, forms::pickedText(forms::Picked()));
    readCage(state, context);
    publish(hooks, state);
  };
  // An undo or another change of the model while the panel is open.
  def.modelChanged = [hooks](CommandState& state, const CommandContext& context) {
    readCage(state, context);
    publish(hooks, state);
  };
  def.handlesSeenAt = [hooks](const CommandState&, const CommandContext&) -> std::optional<gp_Trsf> {
    return hooks.placement ? hooks.placement() : std::nullopt;
  };

  // What Apply would do, from the inputs alone (it may run on the model's
  // worker): the panel's line and the hint over the view.
  def.inspect = [](const CommandState& state, const CommandContext&) {
    Inspection result;
    const forms::Edit edit = editIn(state);
    const forms::Picked picked = pickedIn(state);
    const QString refusal = forms::editRefusal(edit, picked);
    const QJsonObject command = forms::editCommand(state.text(kForm), edit, picked, false);
    if (!refusal.isEmpty()) {
      result.text = refusal;
      result.viewHint = refusal;
    } else if (command.isEmpty()) {
      result.text = QObject::tr("Drag a handle or type a value, then Apply.");
    } else {
      result.text = QObject::tr("Apply makes this edit; Close keeps the edits made.");
    }
    result.log = command.isEmpty() ? QStringLiteral("nothing to apply")
                                   : QString::fromUtf8(QJsonDocument(command).toJson(QJsonDocument::Compact));
    return result;
  };
  // The edited cage and its limit surface's outline, previewed by the
  // model (`preview`: nothing changes), as lines over the view.
  def.viewLines = [hooks, shared](const CommandState& state,
                                  const CommandContext&) -> std::vector<std::vector<gp_Pnt>> {
    const QJsonObject command = forms::editCommand(state.text(kForm), editIn(state), pickedIn(state), true);
    if (command.isEmpty() || !hooks.preview) {
      return {};
    }
    // Asked again with every handle update: the model only when the edit
    // or the cage changed.
    const QByteArray key = QJsonDocument(command).toJson(QJsonDocument::Compact) +
                           QJsonDocument(state.object(kCage)).toJson(QJsonDocument::Compact);
    if (key == shared->previewKey) {
      return shared->previewLines;
    }
    shared->previewKey = key;
    shared->previewLines.clear();
    try {
      const QJsonObject answer = hooks.preview(command);
      forms::Lines lines = forms::cageLines(forms::cageOf(answer.value(QStringLiteral("cage")).toObject()));
      const forms::Lines outline = forms::limitOutline(answer.value(QStringLiteral("limit")).toObject());
      lines.insert(lines.end(), outline.begin(), outline.end());
      for (const std::vector<forms::Point>& line : lines) {
        std::vector<gp_Pnt> points;
        for (const forms::Point& p : line) {
          points.push_back(pnt(p));
        }
        shared->previewLines.push_back(std::move(points));
      }
      shared->previewError.clear();
      qDebug().noquote() << QStringLiteral("Form edit preview: %1 lines").arg(shared->previewLines.size());
    } catch (const std::exception& failure) {
      const QString error = QString::fromUtf8(failure.what());
      if (error != shared->previewError) {
        shared->previewError = error;
        qDebug().noquote() << QStringLiteral("Form edit preview refused: %1").arg(error);
      }
    }
    return shared->previewLines;
  };
  def.apply = [&host, hooks, shared](CommandState& state, const CommandContext& context) {
    const forms::Edit edit = editIn(state);
    const forms::Picked picked = pickedIn(state);
    const QString refusal = forms::editRefusal(edit, picked);
    if (!refusal.isEmpty()) {
      host.showStatus(refusal, NoticeLevel::Hint);
      return false;
    }
    const QJsonObject command = forms::editCommand(state.text(kForm), edit, picked, false);
    if (command.isEmpty()) {
      host.showStatus(QObject::tr("Nothing to apply: drag a handle or type a value first."), NoticeLevel::Hint);
      return false;
    }
    const QString name = cageIn(state).name;
    if (!host.runModelCommand(command)) {
      qDebug().noquote() << QStringLiteral("Form edit %1 in %2: refused").arg(forms::stepName(edit.action), name);
      return false;
    }
    ++shared->edits;
    qDebug().noquote() << QStringLiteral("Form edit %1 in %2: done: %3")
                              .arg(forms::stepName(edit.action), name,
                                   QString::fromUtf8(QJsonDocument(command).toJson(QJsonDocument::Compact)));
    readCage(state, context);
    logBody(state, context);
    resetValues(state);
    publish(hooks, state);
    return true;
  };
  def.ended = [hooks](bool) {
    if (hooks.ended) {
      hooks.ended();
    }
  };
  def.describe = [shared](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Form edits in %1: %2").arg(cageIn(state).name).arg(shared->edits);
  };
  return def;
}

} // namespace
} // namespace cmd

void registerFormEditCommand(CommandRegistry& registry, const CommandContext& context, DocumentHost& host,
                             FormEditHooks hooks) {
  Q_UNUSED(context);
  registry.add(cmd::formEditCommand(host, hooks));
}

} // namespace mitcad
