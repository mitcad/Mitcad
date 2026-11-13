// SPDX-License-Identifier: MIT
// Annotations (mitcad#702, app/COMMANDS.md "Annotations"): the Annotations
// window (View > Annotations…, the browser's Annotations folder, the
// Measure panel's Annotations…), the editor of one, and what the view's
// annotations do with the mouse: a click selects one (its row and its
// highlight, its picks highlighted in its colour), a drag of its text or
// its dimension line moves it, a double-click opens its editor, a right
// click its menu. The editor's Pick Again… and Pick… take the next click in
// the view as a pick or the plane. Every change is one undo step of the
// model's add_annotation, edit_annotation or delete_annotation, outside a
// panel's undo range as a light bulb's (runVisibilityCommands).
#include "MainWindow.hpp"

#include <exception>
#include <initializer_list>
#include <optional>

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QAction>
#include <QColor>
#include <QMenu>
#include <QTimer>
#include <QtLogging>

#include "framework/Dialogs.hpp"
#include "framework/Icons.hpp"
#include "framework/IdleCall.hpp"
#include "view/AnnotationDialogs.hpp"
#include "view/AnnotationsWindow.hpp"
#include "view/MeasurementDrawing.hpp"

namespace mitcad {
namespace {

QJsonObject annotationIn(const QJsonArray& annotations, const QString& name) {
  for (const QJsonValue& value : annotations) {
    if (value.toObject().value(QStringLiteral("name")).toString() == name) {
      return value.toObject();
    }
  }
  return {};
}

QString compactJson(const QJsonObject& object) {
  return QString::fromUtf8(QJsonDocument(object).toJson(QJsonDocument::Compact));
}

// A note, a datum label, a feature control frame, a hole callout, a
// material or process note (mitcad#1049): on a leader, without a value.
bool isLeaderKind(const QString& kind) {
  return kind == QLatin1String("note") || kind == QLatin1String("datum") || kind == QLatin1String("frame") ||
         kind == QLatin1String("hole_callout") || kind == QLatin1String("material") ||
         kind == QLatin1String("process_note");
}

// What a feature control frame can stand under: a dimension or a hole
// callout (mitcad#902).
bool takesFrameKind(const QString& kind) { return !isLeaderKind(kind) || kind == QLatin1String("hole_callout"); }

// A datum label, a feature control frame or a hole callout: it marks a
// feature (a face, an edge, a datum plane or axis), not a vertex or a
// point.
bool marksFeature(const QJsonArray& annotations, const QString& name) {
  const QString kind = annotationIn(annotations, name).value(QStringLiteral("kind")).toString();
  return kind == QLatin1String("datum") || kind == QLatin1String("frame") || kind == QLatin1String("hole_callout");
}

} // namespace

void MainWindow::connectAnnotations() {
  OcctViewer::AnnotationActions actions;
  // Only outside commands and sketches: there the view takes picks; and
  // not while a pick of an annotation's is waited for.
  actions.active = [this] { return m_mode == Mode::Idle && !m_annotationPick; };
  actions.clicked = [this](const QString& name) {
    highlightAnnotations({name});
    if (auto* window = static_cast<AnnotationsWindow*>(m_annotationsWindow.data())) {
      window->selectNames({name});
    }
  };
  // The dialogs and menus after the mouse event that asked for them, and
  // never while a job computes the model (mitcad#888).
  actions.opened = [this](const QString& name) {
    afterEventWhenIdle(*this, this, [this, name] { editAnnotationNamed(name); });
  };
  actions.menu = [this](const QString& name, const QPoint& global) {
    afterEventWhenIdle(*this, this, [this, name, global] { annotationMenu(name, global); });
  };
  actions.moved = [this](const QString& name, bool line, double alongX, double alongY) {
    annotationMoved(name, line, alongX, alongY);
  };
  m_viewer->setAnnotationActions(std::move(actions));
}

bool MainWindow::runAnnotationCommand(const QJsonObject& command, const QString& what) {
  // Showing and hiding works in every mode, as a light bulb does; other
  // changes only where the design can change.
  QStringList fields = command.keys();
  for (const char* key : {"cmd", "name", "names", "visible"}) {
    fields.removeAll(QLatin1String(key));
  }
  const bool visibilityOnly = fields.isEmpty() && command.contains(QStringLiteral("visible"));
  // In a command's panel too: the change runs outside its undo range, as
  // Keep's does (mitcad#764: the Measure panel's Annotations… opens the
  // window, whose edits were refused there).
  const bool inPanel = m_mode == Mode::Command && m_session && !windowReadOnly();
  if (!visibilityOnly && !inPanel && !canChangeModel(true)) {
    showHint(changeBlockedReason());
    qDebug().noquote() << QStringLiteral("Annotation command refused: %1").arg(changeBlockedReason());
    return false;
  }
  qDebug().noquote() << QStringLiteral("Annotation command: %1").arg(compactJson(command));
  if (!runVisibilityCommands(QJsonArray{command}, what)) {
    qDebug().noquote() << QStringLiteral("Annotation command refused: %1").arg(lastError());
    return false;
  }
  return true;
}

void MainWindow::openAnnotations() {
  auto* window = static_cast<AnnotationsWindow*>(m_annotationsWindow.data());
  if (window == nullptr) {
    window = new AnnotationsWindow(this);
    m_annotationsWindow = window;
    AnnotationsWindow::Actions actions;
    actions.selected = [this](const QStringList& names) { highlightAnnotations(names); };
    actions.edit = [this](const QString& name) {
      afterEventWhenIdle(*this, this, [this, name] { editAnnotationNamed(name); });
    };
    actions.change = [this](const QStringList& names, const QJsonObject& change) {
      QJsonObject command = change;
      command.insert(QStringLiteral("cmd"), QStringLiteral("edit_annotation"));
      command.insert(QStringLiteral("names"), QJsonArray::fromStringList(names));
      runAnnotationCommand(command, names.size() == 1 ? tr("Edit %1").arg(names.front())
                                                      : tr("Edit %n annotation(s)", nullptr, static_cast<int>(names.size())));
    };
    actions.remove = [this](const QStringList& names) {
      runAnnotationCommand({{QStringLiteral("cmd"), QStringLiteral("delete_annotation")},
                            {QStringLiteral("names"), QJsonArray::fromStringList(names)}},
                           tr("Delete Annotations"));
    };
    actions.frame = [this](const QString& dimension) {
      afterEventWhenIdle(*this, this, [this, dimension] { addFrameUnder(dimension); });
    };
    // The height of the design's annotations' texts (mitcad#894).
    actions.textHeight = [this] {
      afterEventWhenIdle(*this, this, [this] {
        QJsonObject current;
        try {
          current = query({{QStringLiteral("query"), QStringLiteral("annotation_text_height")}}).toObject();
        } catch (const std::exception& error) {
          qDebug().noquote() << QStringLiteral("Annotation command refused: %1").arg(QString::fromUtf8(error.what()));
          return;
        }
        QWidget* parent = m_annotationsWindow && m_annotationsWindow->isVisible() ? m_annotationsWindow.data()
                                                                                  : static_cast<QWidget*>(this);
        if (const std::optional<QJsonValue> height = askAnnotationTextHeight(parent, current)) {
          runAnnotationCommand({{QStringLiteral("cmd"), QStringLiteral("set_annotation_text_height")},
                                {QStringLiteral("text_height"), *height}},
                               tr("Annotation Text Height"));
        }
      });
    };
    window->setActions(std::move(actions));
    // Nothing highlighted once it closes.
    connect(window, &QDialog::finished, this, [this] { highlightAnnotations({}); });
  }
  window->setAnnotations(measuredAnnotations()); // hidden ones listed with their values too
  window->show();
  window->raise();
  window->activateWindow();
  qDebug().noquote() << QStringLiteral("Annotations window opened: %1 annotation(s)")
                            .arg(m_snapshot.measurements.size());
}

void MainWindow::editAnnotationNamed(const QString& name) {
  // An editor open is no pick waited for, of this annotation or another.
  endAnnotationPick();
  // Hidden ones too, with the kinds their picks have (mitcad#716).
  const QJsonObject annotation = annotationIn(measuredAnnotations(), name);
  if (annotation.isEmpty()) {
    return;
  }
  AnnotationEditContext context;
  for (const QJsonValue& value : std::as_const(m_snapshot.measurements)) {
    const QJsonObject other = value.toObject();
    const QString group = other.value(QStringLiteral("group")).toString();
    if (!group.isEmpty() && !context.groups.contains(group)) {
      context.groups << group;
    }
    // The dimensions (and hole callouts) a frame can stand under.
    if (takesFrameKind(other.value(QStringLiteral("kind")).toString())) {
      context.dimensions << other.value(QStringLiteral("name")).toString();
    }
  }
  context.groups.sort();
  context.datums = datumLetters();
  if (annotation.value(QStringLiteral("kind")).toString() == QLatin1String("frame")) {
    context.characteristics = geometricCharacteristics();
    // Under a dimension its pick is only an anchor: a vertex, a datum point
    // or a whole body is no feature for its leader to point at.
    if (!annotation.value(QStringLiteral("on")).toString().isEmpty()) {
      const std::optional<SelectionItem> anchor = annotationPickItem(annotation.value(QStringLiteral("a")));
      context.leaderNeedsPick = !anchor || anchor->kind == SelectKind::Vertex || anchor->kind == SelectKind::Point ||
                                anchor->kind == SelectKind::Body;
    }
  }
  context.describe = [this](const QJsonValue& pick) { return describeAnnotationPick(pick); };
  context.canPick = m_mode == Mode::Idle && !m_session;
  QWidget* parent = m_annotationsWindow && m_annotationsWindow->isVisible() ? m_annotationsWindow.data()
                                                                            : static_cast<QWidget*>(this);
  const std::optional<AnnotationEdit> edit = editAnnotation(parent, annotation, context);
  if (!edit) {
    return;
  }
  QString current = name;
  if (!edit->change.isEmpty()) {
    QJsonObject command = edit->change;
    command.insert(QStringLiteral("cmd"), QStringLiteral("edit_annotation"));
    command.insert(QStringLiteral("name"), name);
    if (!runAnnotationCommand(command, tr("Edit %1").arg(name))) {
      return;
    }
    current = edit->change.value(QStringLiteral("new_name")).toString(name);
    // A datum named by its letter takes the new letter's name.
    const QString letter = edit->change.value(QStringLiteral("letter")).toString();
    if (!letter.isEmpty() && annotationIn(m_snapshot.measurements, current).isEmpty()) {
      for (const QJsonValue& value : std::as_const(m_snapshot.measurements)) {
        if (value.toObject().value(QStringLiteral("letter")).toString() == letter) {
          current = value.toObject().value(QStringLiteral("name")).toString();
        }
      }
    }
  }
  if (!edit->pick.isEmpty()) {
    startAnnotationPick(current, edit->pick, edit->withPick);
  }
}

void MainWindow::annotationMenu(const QString& name, const QPoint& global) {
  const QJsonObject annotation = annotationIn(m_snapshot.measurements, name);
  if (annotation.isEmpty()) {
    return;
  }
  QMenu menu(this);
  QStringList entries;
  const auto add = [&menu, &entries](const QString& icon, const QString& text, std::function<void()> call) {
    QAction* action = menu.addAction(themeIcon(icon), text);
    connect(action, &QAction::triggered, &menu, std::move(call));
    entries << text;
  };
  add(QStringLiteral("edit"), tr("Edit…"), [this, name] { editAnnotationNamed(name); });
  add(QStringLiteral("eye-off"), tr("Hide"), [this, name] {
    runAnnotationCommand({{QStringLiteral("cmd"), QStringLiteral("edit_annotation")},
                          {QStringLiteral("name"), name},
                          {QStringLiteral("visible"), false}},
                         tr("Hide %1").arg(name));
  });
  add(QStringLiteral("delete"), tr("Delete"), [this, name] {
    runAnnotationCommand({{QStringLiteral("cmd"), QStringLiteral("delete_annotation")}, {QStringLiteral("name"), name}},
                         tr("Delete %1").arg(name));
  });
  add(QStringLiteral("measure"), tr("Annotations…"), [this, name] {
    openAnnotations();
    if (auto* window = static_cast<AnnotationsWindow*>(m_annotationsWindow.data())) {
      window->selectNames({name});
    }
  });
  qDebug().noquote() << QStringLiteral("Context menu: %1").arg(entries.join(QStringLiteral(" | ")));
  execMenu(&menu, global);
}

void MainWindow::annotationMoved(const QString& name, bool line, double alongX, double alongY) {
  const QJsonObject annotation = annotationIn(m_snapshot.measurements, name);
  const QJsonObject drawing = annotation.value(QStringLiteral("drawing")).toObject();
  if (drawing.isEmpty()) {
    return;
  }
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("edit_annotation")}, {QStringLiteral("name"), name}};
  if (line) {
    // Across the line: a length's line farther from or nearer its ends,
    // an angle's arc out or in.
    double offset = drawing.value(QStringLiteral("line_offset")).toDouble() + alongY;
    if (annotation.value(QStringLiteral("kind")).toString() == QLatin1String("angle") && offset <= 0.0) {
      offset = drawing.value(QStringLiteral("line_offset")).toDouble() * 0.1;
    }
    command.insert(QStringLiteral("line_offset"), offset);
  } else {
    const QJsonArray current = drawing.value(QStringLiteral("text_offset")).toArray();
    command.insert(QStringLiteral("text_offset"),
                   QJsonArray{current.at(0).toDouble() + alongX, current.at(1).toDouble() + alongY});
  }
  if (runAnnotationCommand(command, line ? tr("Move %1's Line").arg(name) : tr("Move %1's Text").arg(name))) {
    qDebug().noquote() << QStringLiteral("Annotation %1 moved: %2").arg(name, compactJson(command));
  }
}

// ---------------------------------------------------------------------------
// Datums and feature control frames (mitcad#702 phase 2)

QStringList MainWindow::datumLetters() const {
  QStringList letters;
  for (const QJsonValue& value : std::as_const(m_snapshot.measurements)) {
    const QJsonObject annotation = value.toObject();
    if (annotation.value(QStringLiteral("kind")).toString() == QLatin1String("datum")) {
      letters << annotation.value(QStringLiteral("letter")).toString();
    }
  }
  letters.sort();
  return letters;
}

QJsonArray MainWindow::geometricCharacteristics() const {
  return query({{QStringLiteral("query"), QStringLiteral("geometric_characteristics")}}).toArray();
}

QJsonArray MainWindow::measuredAnnotations() const {
  bool unmeasured = false;
  for (const QJsonValue& value : std::as_const(m_snapshot.measurements)) {
    unmeasured = unmeasured || value.toObject().value(QStringLiteral("measured")) == QJsonValue(false);
  }
  if (!unmeasured) {
    return m_snapshot.measurements;
  }
  // The model keeps what it measured: only hidden ones whose bodies
  // changed are measured again.
  try {
    return query({{QStringLiteral("query"), QStringLiteral("annotations")}, {QStringLiteral("measure_hidden"), true}})
        .toArray();
  } catch (const std::exception& error) {
    qDebug().noquote() << QStringLiteral("Annotations not measured: %1").arg(QString::fromUtf8(error.what()));
    return m_snapshot.measurements;
  }
}

void MainWindow::addFrameUnder(const QString& dimension) {
  const QJsonObject annotation = annotationIn(m_snapshot.measurements, dimension);
  if (annotation.isEmpty() || !takesFrameKind(annotation.value(QStringLiteral("kind")).toString())) {
    return;
  }
  QWidget* parent = m_annotationsWindow && m_annotationsWindow->isVisible() ? m_annotationsWindow.data()
                                                                            : static_cast<QWidget*>(this);
  const std::optional<QJsonObject> frame =
      askFeatureControlFrame(parent, geometricCharacteristics(), QJsonObject(), datumLetters(), dimension);
  if (!frame) {
    return;
  }
  // In the dimension's place under its line, of its colour and group; its
  // pick is the dimension's first.
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("add_annotation")},
                      {QStringLiteral("kind"), QStringLiteral("frame")},
                      {QStringLiteral("a"), annotation.value(QStringLiteral("a"))},
                      {QStringLiteral("frame"), *frame},
                      {QStringLiteral("on"), dimension},
                      {QStringLiteral("color"), annotation.value(QStringLiteral("color"))}};
  if (const QString group = annotation.value(QStringLiteral("group")).toString(); !group.isEmpty()) {
    command.insert(QStringLiteral("group"), group);
  }
  if (runAnnotationCommand(command, tr("Add Frame under %1").arg(dimension))) {
    qDebug().noquote() << QStringLiteral("Frame added under %1").arg(dimension);
  }
}

// ---------------------------------------------------------------------------
// An annotation's picks: described, highlighted, picked again

std::optional<SelectionItem> MainWindow::annotationPickItem(const QJsonValue& pick) const {
  const QJsonObject fields = pick.toObject();
  SelectionItem item;
  item.occurrence = fields.value(QStringLiteral("occurrence")).toString();
  if (fields.contains(QStringLiteral("datum"))) {
    // xy, xz, yz, x, y, z and origin, or a construction feature's uid.
    const QString uid = fields.value(QStringLiteral("datum")).toString();
    const DocumentSnapshot::Feature* feature = m_snapshot.feature(uid);
    const QString type = feature != nullptr ? feature->type : uid;
    item.owner = uid;
    item.kind = type == QLatin1String("construction_axis") || type.size() == 1 ? SelectKind::Axis
                : type == QLatin1String("construction_point") || type == QLatin1String("origin")
                    ? SelectKind::Point
                    : SelectKind::Plane;
    return item;
  }
  item.owner = fields.value(QStringLiteral("body")).toString();
  if (item.owner.isEmpty()) {
    return std::nullopt;
  }
  const std::pair<const char*, SelectKind> named[] = {
      {"face", SelectKind::Face}, {"edge", SelectKind::Edge}, {"vertex", SelectKind::Vertex}};
  item.kind = SelectKind::Body;
  for (const auto& [key, kind] : named) {
    if (fields.contains(QLatin1String(key))) {
      item.kind = kind;
      item.name = fields.value(QLatin1String(key)).toString();
    }
  }
  return item;
}

QString MainWindow::describeAnnotationPick(const QJsonValue& pick) const {
  // In words (mitcad#1741): the annotation dialog shows it.
  if (const std::optional<SelectionItem> item = annotationPickItem(pick)) {
    return pickListText(*item, m_snapshot);
  }
  return pick.isNull() || pick.isUndefined() ? QString() : tr("A pick no longer in the design");
}

void MainWindow::highlightAnnotations(const QStringList& names) {
  const bool changed = names != m_viewer->highlightedAnnotations();
  m_viewer->setHighlightedAnnotations(names);
  if (changed) {
    updateIdleView(); // their picks
  }
}

void MainWindow::addAnnotationHighlights(std::vector<HighlightDisplay>& /*highlights*/) {
  QStringList logged;
  if (m_mode == Mode::Idle && !m_annotationPick) {
    for (const QString& name : m_viewer->highlightedAnnotations()) {
      const QJsonObject annotation = annotationIn(m_snapshot.measurements, name);
      // A frame under a dimension: that dimension's picks (its own copy
      // is only an anchor and may be stale).
      const QString under = annotation.value(QStringLiteral("on")).toString();
      const QJsonObject picked = under.isEmpty() ? annotation : annotationIn(m_snapshot.measurements, under);
      // Its picks and its plane's pick.
      const QJsonValue planePick = picked.value(QStringLiteral("plane")).toObject().value(QStringLiteral("pick"));
      QStringList picks;
      for (const QJsonValue& pick : {picked.value(QStringLiteral("a")), picked.value(QStringLiteral("b")), planePick}) {
        const std::optional<SelectionItem> item = pick.isObject() ? annotationPickItem(pick) : std::nullopt;
        if (!item) {
          continue;
        }
        TopoDS_Shape shape;
        try {
          shape = itemShape(*item);
        } catch (const std::exception&) {
          continue; // a pick that is gone: the annotation is broken
        }
        if (shape.IsNull()) {
          continue;
        }
        // Not drawn (mitcad#902: a selection recolours only the
        // annotation's own thin drawing, no filled faces); listed for the
        // tests' log.
        picks << item->logDescription();
      }
      if (!picks.isEmpty()) {
        logged << QStringLiteral("%1: %2").arg(name, picks.join(QStringLiteral("; ")));
      }
    }
  }
  const QString text = logged.isEmpty() ? QStringLiteral("none") : logged.join(QStringLiteral(" | "));
  if (text != m_loggedAnnotationPicks) {
    m_loggedAnnotationPicks = text;
    qDebug().noquote() << QStringLiteral("Annotation picks highlighted: %1").arg(text);
  }
}

void MainWindow::startAnnotationPick(const QString& name, const QString& field, const QJsonObject& with) {
  if (m_mode != Mode::Idle || m_session) {
    showHint(changeBlockedReason());
    return;
  }
  // A frame under a dimension stands in its place: nothing to pick, unless
  // the pick takes it onto its leader.
  if (const QString under = annotationIn(m_snapshot.measurements, name).value(QStringLiteral("on")).toString();
      !under.isEmpty() && !(field == QLatin1String("a") && with.contains(QStringLiteral("on")))) {
    showHint(tr("%1 stands under %2: it has nothing of its own to pick.").arg(name, under));
    return;
  }
  m_annotationPick = AnnotationPick{name, field, with};
  setSelection({}); // the view's filter and the banner (updateIdleView)
  // The keyboard back to the view (the editor that asked for the pick has
  // just closed): Esc cancels the pick.
  activateWindow();
  m_viewer->setFocus(Qt::OtherFocusReason);
  showHint(annotationPickText());
  qDebug().noquote() << QStringLiteral("Annotation pick started: %1 %2%3")
                            .arg(name, field,
                                 with.isEmpty() ? QString() : QStringLiteral(" with %1").arg(compactJson(with)));
  // For UI tests: where a click picks what the pick takes, as "Pick face
  // F2.b0/F2:end(…) at x,y" in the window's coordinates.
  if (qEnvironmentVariableIsSet("MITCAD_LOG_PICKS")) {
    qDebug().noquote() << "Pick places:";
    SelectFilter kinds = SelectFilter(SelectKind::Face | SelectKind::Edge | SelectKind::Vertex);
    if (field == QLatin1String("plane")) {
      kinds = SelectFilter(SelectKind::Face);
    } else if (marksFeature(m_snapshot.measurements, name)) {
      kinds = SelectFilter(SelectKind::Face | SelectKind::Edge);
    }
    for (const auto& [item, at] : m_viewer->pickPoints(kinds)) {
      const QPoint point = m_viewer->mapTo(window(), at);
      qDebug().noquote() << QStringLiteral("Pick %1 %2/%3%4 at %5,%6")
                                .arg(SelectionItem::kindName(item.kind), item.owner, item.name,
                                     item.occurrence.isEmpty() ? QString() : QStringLiteral(" in ") + item.occurrence)
                                .arg(point.x())
                                .arg(point.y());
    }
  }
}

QString MainWindow::annotationPickText() const {
  if (!m_annotationPick || m_mode != Mode::Idle) {
    return QString();
  }
  if (m_annotationPick->field == QLatin1String("plane")) {
    return tr("Edit %1: pick a planar face or a construction plane for its plane. Esc cancels.").arg(m_annotationPick->name);
  }
  if (marksFeature(m_snapshot.measurements, m_annotationPick->name)) {
    return tr("Edit %1: pick a face, edge, construction plane or construction axis again. Esc cancels.").arg(m_annotationPick->name);
  }
  return tr("Edit %1: pick a face, edge, vertex or datum again. Esc cancels.").arg(m_annotationPick->name);
}

void MainWindow::applyAnnotationPickFilter() {
  if (!m_annotationPick || m_mode != Mode::Idle) {
    return;
  }
  if (m_annotationPick->field == QLatin1String("plane")) {
    m_viewer->setPickFilter(SelectKind::Plane | SelectKind::Face, [](const SelectionItem& item) {
      return item.kind == SelectKind::Plane || item.geometry == QLatin1String("plane");
    });
    return;
  }
  // A datum label or a frame marks a feature: no vertex, no point.
  if (marksFeature(m_snapshot.measurements, m_annotationPick->name)) {
    m_viewer->setPickFilter(SelectKind::Face | SelectKind::Edge | SelectKind::Plane | SelectKind::Axis);
    return;
  }
  m_viewer->setPickFilter(SelectKind::Face | SelectKind::Edge | SelectKind::Vertex | SelectKind::Plane |
                          SelectKind::Axis | SelectKind::Point);
}

bool MainWindow::annotationPicked(const Selection& items) {
  if (!m_annotationPick || m_mode != Mode::Idle) {
    return false;
  }
  const AnnotationPick waiting = *m_annotationPick;
  const QJsonValue pick = items.isEmpty() ? QJsonValue() : measurementPick(items.first());
  if (!pick.isObject()) {
    showHint(annotationPickText());
    qDebug().noquote() << QStringLiteral("Annotation pick still waited for: %1 %2").arg(waiting.name, waiting.field);
    return true;
  }
  const SelectionItem& item = items.first();
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("edit_annotation")},
                      {QStringLiteral("name"), waiting.name}};
  if (waiting.field == QLatin1String("plane")) {
    command.insert(QStringLiteral("plane"), QJsonObject{{QStringLiteral("pick"), pick}});
  } else {
    command.insert(waiting.field, pick);
    // A leader points where the click hit its pick.
    const QString kind = annotationIn(m_snapshot.measurements, waiting.name).value(QStringLiteral("kind")).toString();
    if (isLeaderKind(kind) && waiting.field == QLatin1String("a")) {
      command.insert(QStringLiteral("at"), item.at ? QJsonValue(QJsonArray{(*item.at)[0], (*item.at)[1], (*item.at)[2]})
                                                   : QJsonValue(QJsonValue::Null));
    }
  }
  // What goes with the pick (a frame's `on: null`), in the same edit.
  for (auto field = waiting.with.constBegin(); field != waiting.with.constEnd(); ++field) {
    if (!command.contains(field.key())) {
      command.insert(field.key(), field.value());
    }
  }
  qDebug().noquote() << QStringLiteral("Annotation picked: %1 %2: %3").arg(waiting.name, waiting.field, item.logDescription());
  if (!runAnnotationCommand(command, tr("Pick %1 Again").arg(waiting.name))) {
    return true; // still waiting: another pick, or Esc
  }
  m_annotationPick.reset();
  updateIdleView();
  afterEventWhenIdle(*this, this, [this, name = waiting.name] { editAnnotationNamed(name); });
  return true;
}

bool MainWindow::cancelAnnotationPick() {
  if (!m_annotationPick) {
    return false;
  }
  const QString name = m_annotationPick->name;
  m_annotationPick.reset();
  updateIdleView();
  showHint(tr("Pick canceled."));
  qDebug().noquote() << QStringLiteral("Annotation pick cancelled: %1").arg(name);
  afterEventWhenIdle(*this, this, [this, name] { editAnnotationNamed(name); });
  return true;
}

void MainWindow::endAnnotationPick() {
  if (!m_annotationPick) {
    return;
  }
  qDebug().noquote() << QStringLiteral("Annotation pick ended: %1").arg(m_annotationPick->name);
  m_annotationPick.reset();
  updateModeBanner();
  if (m_mode == Mode::Idle) {
    updateIdleView(); // the view picks as it did
  }
}

void MainWindow::followAnnotationPick() {
  if (m_annotationPick && annotationIn(m_snapshot.measurements, m_annotationPick->name).isEmpty()) {
    endAnnotationPick(); // its annotation is gone (undo, another window's change)
  }
}

void MainWindow::dropAnnotationHighlight() {
  if (m_viewer->highlightedAnnotations().isEmpty()) {
    return;
  }
  // The view's first, so that the window's rows deselected change nothing
  // more (highlightAnnotations sees no change).
  m_viewer->setHighlightedAnnotations({});
  if (auto* window = static_cast<AnnotationsWindow*>(m_annotationsWindow.data())) {
    window->selectNames({});
  }
}

} // namespace mitcad
