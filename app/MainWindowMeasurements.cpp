// SPDX-License-Identifier: MIT
// Measurement objects (mitcad#649), now annotations (mitcad#702;
// app/COMMANDS.md "Measurement objects" and "Annotations"): Keep as
// Dimension in the Measure panel and in a history row's menu adds the
// picks as a dimension of the design (the model's add_annotation, whose
// default kind and name the model chooses, in the plane and with the
// places the view suggests), the Annotate menu's Note…, Datum Label…,
// Frame… and Hole Callout a leader note, a datum label, a feature control
// frame or a hole callout on the one pick shown; all outside the panel's undo
// range, as a light bulb's change is (CommandSession::runVisibility). The
// shown ones are drawn in their planes in the view after every refresh
// (OcctViewer::setMeasurements) and listed in the Annotations window
// (MainWindowAnnotations.cpp).
#include "MainWindow.hpp"

#include <cmath>
#include <initializer_list>
#include <optional>
#include <utility>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QStringList>
#include <QtLogging>

#include "framework/Command.hpp"
#include "framework/CommandSession.hpp"
#include "view/AnnotationDialogs.hpp"
#include "view/AnnotationsWindow.hpp"
#include "view/MeasurementDrawing.hpp"

namespace mitcad {

// Sketch curves and points are measured by the panel itself.
QJsonValue MainWindow::measurementPick(const SelectionItem& item) {
  QJsonObject pick;
  switch (item.kind) {
  case SelectKind::Plane:
  case SelectKind::Axis:
  case SelectKind::Point:
    pick.insert(QStringLiteral("datum"), item.owner);
    break;
  case SelectKind::Body:
  case SelectKind::Face:
  case SelectKind::Edge:
  case SelectKind::Vertex: {
    const QJsonValue reference = item.reference();
    if (!reference.isObject()) {
      return QJsonValue();
    }
    pick = reference.toObject();
    break;
  }
  default:
    return QJsonValue();
  }
  if (!item.occurrence.isEmpty()) {
    pick.insert(QStringLiteral("occurrence"), item.occurrence);
  }
  return pick;
}

namespace {

const SelectionItem* firstOf(const QHash<QString, Selection>& inputs, std::initializer_list<const char*> keys) {
  for (const char* key : keys) {
    const auto found = inputs.constFind(QString::fromLatin1(key));
    if (found != inputs.constEnd() && !found->isEmpty()) {
      return &found->first();
    }
  }
  return nullptr;
}

QJsonArray vectorOf(const gp_Dir& d) { return QJsonArray{d.X() + 0.0, d.Y() + 0.0, d.Z() + 0.0}; }

} // namespace

bool MainWindow::measurementPicks(const QHash<QString, Selection>& inputs, QJsonObject& command, QString& why) {
  const SelectionItem* a = firstOf(inputs, {"a", "body_a", "sketch_a"});
  const SelectionItem* b = firstOf(inputs, {"b", "body_b", "sketch_b"});
  if (a == nullptr) {
    why = tr("Pick what to measure first.");
    return false;
  }
  const QJsonValue pickA = measurementPick(*a);
  const QJsonValue pickB = b != nullptr ? measurementPick(*b) : QJsonValue();
  if (pickA.isNull() || (b != nullptr && pickB.isNull())) {
    // Also Keep's tooltip while it is greyed out (mitcad#869): one short
    // line.
    why = tr("Sketch geometry cannot be kept as an annotation: pick the body's corners or edges");
    return false;
  }
  command.insert(QStringLiteral("a"), pickA);
  if (b != nullptr) {
    command.insert(QStringLiteral("b"), pickB);
  }
  return true;
}

bool MainWindow::notePick(const QHash<QString, Selection>& inputs, QJsonObject& pick, QJsonValue& at) {
  const SelectionItem* a = firstOf(inputs, {"a", "body_a", "sketch_a"});
  if (a == nullptr || firstOf(inputs, {"b", "body_b", "sketch_b"}) != nullptr) {
    return false;
  }
  // A face, edge or vertex of a body, a body clicked, or a datum plane,
  // axis or point.
  if (a->kind != SelectKind::Face && a->kind != SelectKind::Edge && a->kind != SelectKind::Vertex &&
      a->kind != SelectKind::Body && a->kind != SelectKind::Plane && a->kind != SelectKind::Axis &&
      a->kind != SelectKind::Point) {
    return false;
  }
  // A whole body gives no point of its own: only where it was clicked.
  if (a->kind == SelectKind::Body && !a->at) {
    return false;
  }
  const QJsonValue value = measurementPick(*a);
  if (!value.isObject()) {
    return false;
  }
  pick = value.toObject();
  // Where the click hit it: the arrow points at the pick's point nearest
  // it.
  at = a->at ? QJsonValue(QJsonArray{(*a->at)[0], (*a->at)[1], (*a->at)[2]}) : QJsonValue();
  return true;
}

bool MainWindow::markablePick(const QHash<QString, Selection>& inputs) {
  const SelectionItem* a = firstOf(inputs, {"a", "body_a", "sketch_a"});
  return a != nullptr && a->kind != SelectKind::Vertex && a->kind != SelectKind::Point;
}

QJsonObject MainWindow::annotationView() const {
  QJsonObject view{{QStringLiteral("direction"), vectorOf(m_viewer->viewDirection())},
                   {QStringLiteral("up"), vectorOf(m_viewer->viewUp())}};
  // A pixel's size at the view's centre: its pixels for a millimetre
  // along the up direction there.
  const gp_Pnt centre = m_viewer->viewCamera()->Center();
  const gp_Dir up = m_viewer->viewUp();
  const QPointF here = m_viewer->toWidgetF(centre);
  const QPointF there = m_viewer->toWidgetF(centre.Translated(gp_Vec(up)));
  const double pixels = std::hypot(there.x() - here.x(), there.y() - here.y());
  if (pixels > 1e-9 && std::isfinite(pixels)) {
    view.insert(QStringLiteral("pixel"), 1.0 / pixels);
  }
  return view;
}

void MainWindow::keepMeasurement(const QHash<QString, Selection>& inputs, const QString& what) {
  if (!m_session || m_session->def().id != QStringLiteral("inspect.measure")) {
    return;
  }
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("add_annotation")},
                      {QStringLiteral("view"), annotationView()}};
  QString why;
  if (!measurementPicks(inputs, command, why)) {
    showHint(why);
    qDebug().noquote() << QStringLiteral("Measurement not kept (%1): %2").arg(what, why);
    return;
  }
  // Listed once the model has it (followMeasurements); a light bulb's way
  // outside the panel's undo range, so that closing the panel keeps it.
  m_measurementKept = true;
  if (!m_session->runVisibility(QJsonArray{command}, tr("Keep Measurement"))) {
    m_measurementKept = false;
    qDebug().noquote() << QStringLiteral("Measurement not kept (%1): %2").arg(what, lastError());
  }
}

void MainWindow::keepMeasurementRow(int number) {
  const MeasureHistory::Row* row = m_measureHistory.row(number);
  if (row == nullptr) {
    return;
  }
  // A copy: keeping refreshes the rows.
  const QHash<QString, Selection> inputs = row->inputs;
  keepMeasurement(inputs, QStringLiteral("row %1").arg(number));
}

void MainWindow::keepNote(const QHash<QString, Selection>& inputs) { keepLeader(inputs, QStringLiteral("note")); }

void MainWindow::keepLeader(const QHash<QString, Selection>& inputs, const QString& kind) {
  if (!m_session || m_session->def().id != QStringLiteral("inspect.measure")) {
    return;
  }
  QJsonObject pick;
  QJsonValue at;
  if (!notePick(inputs, pick, at)) {
    showHint(tr("Pick one face, edge or vertex of a body or a datum, or click a body where the leader points."));
    return;
  }
  // A datum label or a frame marks a feature, not a point (the model
  // refuses that too); a hole callout a hole.
  if (kind != QLatin1String("note") && !markablePick(inputs)) {
    showHint(kind == QLatin1String("hole_callout")
                 ? tr("A hole callout points at a hole: pick its round face or edge.")
                 : tr("A datum label or a feature control frame marks a face, an edge, an axis or a plane, not a "
                      "point."));
    return;
  }
  const SelectionItem* item = firstOf(inputs, {"a", "body_a", "sketch_a"});
  const QString what = item != nullptr ? item->describe() : QString();
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("add_annotation")},
                      {QStringLiteral("kind"), kind},
                      {QStringLiteral("a"), pick},
                      {QStringLiteral("view"), annotationView()}};
  QString label;
  if (kind == QLatin1String("datum")) {
    // A datum label: the model's next free letter offered (ISO 5459); kept,
    // the model chooses it again (no letter sent).
    const QString suggested =
        query({{QStringLiteral("query"), QStringLiteral("next_datum_letter")}}).toObject().value(QStringLiteral("letter")).toString();
    const std::optional<QString> letter = askDatumLetter(this, suggested, what);
    if (!letter) {
      return;
    }
    if (*letter != suggested) {
      command.insert(QStringLiteral("letter"), *letter);
    }
    label = tr("Add Datum Label");
  } else if (kind == QLatin1String("frame")) {
    const std::optional<QJsonObject> frame =
        askFeatureControlFrame(this, geometricCharacteristics(), QJsonObject(), datumLetters(), what);
    if (!frame) {
      return;
    }
    command.insert(QStringLiteral("frame"), *frame);
    label = tr("Add Feature Control Frame");
  } else if (kind == QLatin1String("hole_callout")) {
    // Its text is the hole's (mitcad#902): no dialog; the model refuses a
    // pick that is no hole, with the reason.
    label = tr("Add Hole Callout");
  } else {
    const std::optional<QString> text = askNoteText(this, what);
    if (!text) {
      return;
    }
    command.insert(QStringLiteral("text"), *text);
    label = tr("Add Note");
  }
  if (!at.isNull()) {
    command.insert(QStringLiteral("at"), at);
  }
  m_measurementKept = true;
  if (!m_session->runVisibility(QJsonArray{command}, label)) {
    m_measurementKept = false;
    qDebug().noquote() << QStringLiteral("Leader %1 not added: %2").arg(kind, lastError());
  }
}

void MainWindow::followMeasurements() {
  if (!m_viewer->annotationActions().clicked) {
    connectAnnotations();
  }
  const QString unit = m_snapshot.lengthUnit;
  const double millimetres = lengthUnitMillimetres(unit);
  m_viewer->setMeasurements(measurementDisplays(
      m_snapshot.measurements, [unit, millimetres](const QString& kind, double value) {
        return measurementValueText(kind, value, unit, millimetres);
      }));
  // For UI tests: each one's state when they change, and one just kept.
  QStringList names;
  QStringList states;
  for (const QJsonValue& value : std::as_const(m_snapshot.measurements)) {
    const QJsonObject measurement = value.toObject();
    const QString name = measurement.value(QStringLiteral("name")).toString();
    names << name;
    const QString error = measurement.value(QStringLiteral("error")).toString();
    const QString group = measurement.value(QStringLiteral("group")).toString();
    states << (error.isEmpty() ? QStringLiteral("%1 %2%3%4")
                                     .arg(name, measurement.value(QStringLiteral("kind")).toString(),
                                          measurement.value(QStringLiteral("visible")).toBool(true)
                                              ? QString()
                                              : QStringLiteral(" (hidden)"),
                                          group.isEmpty() ? QString() : QStringLiteral(" in ") + group)
                               : QStringLiteral("%1 broken: %2").arg(name, error));
  }
  const QString logged = QStringLiteral("Annotations: %1")
                             .arg(states.isEmpty() ? QStringLiteral("none") : states.join(QStringLiteral(" | ")));
  if (logged != m_loggedMeasurements) {
    m_loggedMeasurements = logged;
    qDebug().noquote() << logged;
  }
  if (m_measurementKept) {
    for (const QString& name : std::as_const(names)) {
      if (!m_measurementNames.contains(name)) {
        qDebug().noquote() << QStringLiteral("Annotation kept: %1").arg(name);
        m_measurementKept = false;
      }
    }
  }
  m_measurementNames = names;
  followAnnotationPick(); // a pick of an annotation that is gone ends
  // The Annotations window follows the model.
  if (auto* window = static_cast<AnnotationsWindow*>(m_annotationsWindow.data())) {
    window->setAnnotations(window->isVisible() ? measuredAnnotations() : m_snapshot.measurements);
  }
}

} // namespace mitcad
