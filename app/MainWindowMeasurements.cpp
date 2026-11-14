// SPDX-License-Identifier: MIT
// Measurement objects (mitcad#649), now annotations (mitcad#702;
// app/COMMANDS.md "Measurement objects" and "Annotations"): the picks of a
// panel as add_annotation takes them, for the Make tab's Annotate group
// (mitcad#1049, MainWindowMake.cpp: Dimension, whose default kind and name
// the model chooses, in the plane and with the places the view suggests;
// Note, Datum Label, Feature Control Frame, Hole Callout, Material and
// Process Note on one pick), each outside the panel's undo range, as a
// light bulb's change is; before Add, each panel's preview of what it adds,
// placed by the model as Add will place it (add_annotation with "preview",
// mitcad#1097) and drawn dimmed (OcctViewer::setMeasurementPreview). The
// shown ones are drawn in their planes in the view after every refresh
// (OcctViewer::setMeasurements) and listed in the Annotations window
// (MainWindowAnnotations.cpp).
#include "MainWindow.hpp"

#include <cmath>
#include <exception>
#include <initializer_list>
#include <optional>
#include <utility>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QStringList>
#include <QtLogging>

#include "framework/Command.hpp"
#include "framework/CommandSession.hpp"
#include "framework/Json.hpp"
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

// Whether two views of annotationView look along the same direction with
// the same up (a pan or a zoom places an annotation no differently).
bool sameDirection(const QJsonObject& a, const QJsonObject& b) {
  for (const char* key : {"direction", "up"}) {
    const QJsonArray u = a.value(QLatin1String(key)).toArray();
    const QJsonArray v = b.value(QLatin1String(key)).toArray();
    if (u.size() != 3 || v.size() != 3) {
      return false;
    }
    double along = 0.0;
    for (int i = 0; i < 3; ++i) {
      along += u.at(i).toDouble() * v.at(i).toDouble();
    }
    if (along < 1.0 - 1e-12) {
      return false;
    }
  }
  return true;
}

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

QJsonObject MainWindow::annotationCommand(const CommandState& state, const QString& kind, const QJsonObject& extra,
                                          QString* why) {
  QHash<QString, Selection> inputs;
  inputs.insert(QStringLiteral("a"), state.items(QStringLiteral("a")));
  if (!state.items(QStringLiteral("b")).isEmpty()) {
    inputs.insert(QStringLiteral("b"), state.items(QStringLiteral("b")));
  }
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("add_annotation")}};
  if (kind == QLatin1String("dimension")) {
    // The kind the model chooses for the picks, as Keep chose it.
    QString reason;
    if (!measurementPicks(inputs, command, reason)) {
      if (why != nullptr) {
        *why = reason;
      }
      return QJsonObject();
    }
  } else {
    QJsonObject pick;
    QJsonValue at;
    if (!notePick(inputs, pick, at)) {
      if (why != nullptr) {
        *why = tr("Pick one face, edge or vertex of a body or a datum, or click a body where the leader points.");
      }
      return QJsonObject();
    }
    command.insert(QStringLiteral("kind"), kind);
    command.insert(QStringLiteral("a"), pick);
    if (!at.isNull()) {
      command.insert(QStringLiteral("at"), at);
    }
  }
  for (auto it = extra.begin(); it != extra.end(); ++it) {
    command.insert(it.key(), it.value());
  }
  return command;
}

QString MainWindow::previewAnnotation(const QJsonObject& asked) {
  m_annotationPreview = asked;
  const auto logged = [this](const QString& line) {
    if (line != m_loggedAnnotationPreview) {
      m_loggedAnnotationPreview = line;
      qDebug().noquote() << line;
    }
  };
  if (asked.isEmpty()) {
    m_annotationPreviewView = QJsonObject();
    m_viewer->setMeasurementPreview({});
    logged(QStringLiteral("Annotation preview: none"));
    return QString();
  }
  // Placed by the model as Add will place it: in the view as it is, with
  // nothing stored (commands.md, "Annotations": a preview).
  const QJsonObject view = annotationView();
  QJsonObject request = asked;
  request.insert(QStringLiteral("view"), view);
  request.insert(QStringLiteral("preview"), true);
  m_annotationPreviewView = view;
  QJsonObject answer;
  try {
    answer = command(request);
  } catch (const std::exception& error) {
    const QString why = errorText(error);
    m_viewer->setMeasurementPreview({});
    logged(QStringLiteral("Annotation preview: none: %1").arg(why));
    return why;
  }
  // The annotation as the annotations query will list it, without the
  // command's own status fields.
  for (const char* key : {"recomputed", "restored", "restored_packed", "failed_features", "notices"}) {
    answer.remove(QLatin1String(key));
  }
  if (answer.value(QStringLiteral("error")).isNull()) {
    answer.remove(QStringLiteral("error"));
  }
  const QString unit = m_snapshot.lengthUnit;
  const double millimetres = lengthUnitMillimetres(unit);
  const std::vector<MeasurementDisplay> shown =
      measurementDisplays(QJsonArray{answer}, [unit, millimetres](const QString& kind, double value) {
        return measurementValueText(kind, value, unit, millimetres);
      });
  m_viewer->setMeasurementPreview(shown);
  const QString name = answer.value(QStringLiteral("name")).toString();
  if (shown.empty()) {
    logged(QStringLiteral("Annotation preview: none drawn: %1").arg(name));
  } else {
    logged(QStringLiteral("Annotation preview: %1 %2").arg(name, answer.value(QStringLiteral("kind")).toString()));
  }
  return QString();
}

void MainWindow::followAnnotationPreviewView() {
  if (m_annotationPreview.isEmpty() || !m_session || modelBusy()) {
    return;
  }
  if (!sameDirection(annotationView(), m_annotationPreviewView)) {
    // Computed again through the panel, which shows why if it is refused.
    m_session->previewAgain();
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
                           // Rolled back with the features its picks are
                           // on (mitcad#1756), not broken.
                           : measurement.value(QStringLiteral("rolled_back")).toBool()
                               ? QStringLiteral("%1 rolled back: %2").arg(name, error)
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
