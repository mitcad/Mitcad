// SPDX-License-Identifier: MIT
// Print Analysis (mitcad#620, app/COMMANDS.md "Print Analysis"): the
// overhangs and thin walls of the analysis shown in the document drawn over
// the bodies, with the thinnest point and the legend; the panel's own while
// it is open. Overhangs and walls may take long on large meshes: they run
// on the model's worker as jobs that can be cancelled, and the model keeps
// their results by the bodies' content and the settings, so showing an
// analysis again does not compute it again. A shown Draft Analysis
// (mitcad#2166, app/COMMANDS.md "Draft Analysis") is followed the same way:
// its three classes drawn over the bodies with their legend; a shown
// Minimum Radius (mitcad#2518) too: its smallest radius marked and its
// faces under the limit drawn; a shown Minimum Distance (mitcad#2519,
// app/COMMANDS.md "Minimum Distance"): its line between the closest points
// with the distance; and the surface quality analyses
// (mitcad#2517, SurfaceAnalysis.hpp): a Zebra's stripes, a Curvature Map's
// colors, an Isocurve Analysis' curves and an Environment Map's
// reflection, computed again when the model changes.
#include "MainWindow.hpp"

#include <array>
#include <exception>
#include <initializer_list>
#include <memory>
#include <vector>

#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QObject>
#include <QtLogging>

#include "commands/DraftAnalysis.hpp"
#include "commands/MinRadiusAnalysis.hpp"
#include "commands/MinimumDistance.hpp"
#include "commands/PointCloudDeviation.hpp"
#include "commands/PrintAnalysis.hpp"
#include "commands/SurfaceAnalysis.hpp"
#include "framework/Json.hpp"
#include "framework/ModelWorker.hpp"
#include "framework/TestSync.hpp"
#include "framework/Theme.hpp"

namespace mitcad {
namespace {

bool isDraft(const QJsonObject& analysis) {
  return analysis.value(QStringLiteral("type")).toString() == QStringLiteral("draft");
}

// Point Cloud Deviation (mitcad#2240).
bool isDeviation(const QJsonObject& analysis) {
  return analysis.value(QStringLiteral("type")).toString() == QStringLiteral("point_cloud_deviation");
}

// Minimum Radius (mitcad#2518).
bool isMinRadius(const QJsonObject& analysis) {
  return analysis.value(QStringLiteral("type")).toString() == QStringLiteral("min_radius");
}

// Minimum Distance (mitcad#2519).
bool isDistance(const QJsonObject& analysis) {
  return analysis.value(QStringLiteral("type")).toString() == QStringLiteral("minimum_distance");
}

// Surface quality analyses (mitcad#2517).
bool isSurface(const QJsonObject& analysis) {
  return isSurfaceAnalysis(analysis.value(QStringLiteral("type")).toString());
}

// The kinds of analysis drawn over the bodies (mitcad#620, mitcad#2166,
// mitcad#2518), Point Cloud Deviation's points (mitcad#2240), Minimum
// Distance's line (mitcad#2519) and the surface quality analyses
// (mitcad#2517).
bool drawnOverBodies(const QJsonObject& analysis) {
  const QString type = analysis.value(QStringLiteral("type")).toString();
  return type == QStringLiteral("print") || type == QStringLiteral("draft") || isDeviation(analysis) ||
         isMinRadius(analysis) || isDistance(analysis) || isSurface(analysis);
}

// The log's words of the kind: "Print analysis", "Draft analysis",
// "Point cloud deviation", "Minimum radius", "Minimum distance", "Zebra
// analysis", ….
QString kindOf(const QJsonObject& analysis) {
  if (isSurface(analysis)) {
    return surfaceAnalysisKind(analysis.value(QStringLiteral("type")).toString());
  }
  return isDraft(analysis)       ? QStringLiteral("Draft analysis")
         : isDeviation(analysis) ? QStringLiteral("Point cloud deviation")
         : isMinRadius(analysis) ? QStringLiteral("Minimum radius")
         : isDistance(analysis)  ? QStringLiteral("Minimum distance")
                                 : QStringLiteral("Print analysis");
}

// The panel's and the progress' words of the kind.
QString titleOf(const QJsonObject& analysis) {
  if (isSurface(analysis)) {
    return surfaceAnalysisTitle(analysis.value(QStringLiteral("type")).toString());
  }
  return isDraft(analysis)       ? QObject::tr("Draft Analysis")
         : isDeviation(analysis) ? QObject::tr("Point Cloud Deviation")
         : isMinRadius(analysis) ? QObject::tr("Minimum Radius")
         : isDistance(analysis)  ? QObject::tr("Minimum Distance")
                                 : QObject::tr("Print Analysis");
}

// The shown analysis' numbers in the log's words and its overlay, or its
// error: a Print Analysis' queries or a Draft Analysis' one.
Inspection analyseShown(const QJsonObject& shown, const QString& name, const BodyNames& bodyName,
                        const PrintQuery& query) {
  Inspection result;
  if (isMinRadius(shown)) {
    QJsonObject answer;
    result.error = runMinRadiusAnalysis(shown, query, answer);
    if (result.error.isEmpty()) {
      result.log = minRadiusAnalysisLog(answer, bodyName);
      result.printOverlay = minRadiusOverlayJson(shown, answer, name);
    }
    return result;
  }
  // Minimum Distance (mitcad#2519): its line and the distance.
  if (isDistance(shown)) {
    QJsonObject answer;
    result.error = runMinimumDistance(shown, query, answer);
    if (result.error.isEmpty()) {
      result.log = minimumDistanceLog(answer);
      result.printOverlay = minimumDistanceOverlayJson(answer, name);
    }
    return result;
  }
  // The surface quality analyses (mitcad#2517).
  if (isSurface(shown)) {
    QJsonObject answer;
    result.error = runSurfaceAnalysis(shown, query, answer);
    if (result.error.isEmpty()) {
      result.log = surfaceAnalysisLog(answer, bodyName);
      result.printOverlay = surfaceOverlayJson(shown, answer, name);
    }
    return result;
  }
  if (isDeviation(shown)) {
    QJsonObject answer;
    std::vector<float> places;
    result.error = runPointCloudDeviation(shown, query, answer, places);
    if (result.error.isEmpty()) {
      const QString body = bodyName(shown.value(QStringLiteral("body")).toString());
      result.log = pointCloudDeviationLog(answer, body);
      result.printOverlay = pointCloudDeviationOverlayJson(shown, answer, places, name);
    }
    return result;
  }
  if (isDraft(shown)) {
    QJsonObject answer;
    result.error = runDraftAnalysis(shown, query, answer);
    if (result.error.isEmpty()) {
      result.log = draftAnalysisLog(answer, bodyName);
      result.printOverlay = draftOverlayJson(shown, answer, name);
    }
    return result;
  }
  PrintAnswers answers;
  result.error = runPrintAnalysis(shown, query, answers);
  if (result.error.isEmpty()) {
    result.log = printAnalysisLog(answers, bodyName);
    result.printOverlay = printOverlayJson(shown, answers, name);
  }
  return result;
}

} // namespace

QJsonObject MainWindow::jobQuery(const QJsonObject& request, const QString& stage) {
  Document& document = *m_document;
  const QByteArray json = compactJson(request);
  rust::String answer;
  const bool done = runJob(
      request.value(QStringLiteral("query")).toString(), stage,
      [&document, &json, &answer](ModelJob& job) {
        const AttachedJob attached(document, job);
        answer = document.query(rustStr(json));
      },
      JobLog::WhenSlow);
  if (!done) {
    throw ComputationCancelled();
  }
  return parseObject(answer);
}

void MainWindow::showPrintAnalysis(const QJsonObject& overlay) {
  // The panel's own while it is open; the analysis shown in the document
  // again once it closes (followPrintAnalysis: no key equals this one).
  m_printKey = QByteArrayLiteral("panel");
  ++m_printGeneration;
  showPrintOverlay(overlay);
}

void MainWindow::showPrintOverlay(const QJsonObject& overlay) {
  // Overhangs, walls too thin to print and narrow gaps (mitcad#1519) are
  // categories with a legend, not warnings or errors: violet, magenta and
  // teal for the view's background,
  // not the widgets' palette (mitcad#1041); Draft Analysis' classes
  // (mitcad#2166) in their own roles.
  const bool dark = m_viewer->backgroundDark();
  const std::array<QColor, 3> draft = {viewColor(ColorRole::DraftPositive, dark),
                                       viewColor(ColorRole::DraftUnder, dark),
                                       viewColor(ColorRole::DraftNegative, dark)};
  // Point Cloud Deviation's scale (mitcad#2240).
  const std::array<QColor, 3> deviation = {viewColor(ColorRole::DeviationInside, dark),
                                           viewColor(ColorRole::DeviationNeutral, dark),
                                           viewColor(ColorRole::DeviationOutside, dark)};
  // Minimum Distance's line (mitcad#2519): what an inspection points at,
  // and a problem it found when closer than the clearance.
  const std::array<QColor, 2> distance = {viewColor(ColorRole::PreviewTarget, dark),
                                          viewColor(ColorRole::Interference, dark)};
  PrintOverlay drawn =
      printOverlayOf(overlay, viewColor(ColorRole::PrintOverhang, dark), viewColor(ColorRole::PrintThin, dark),
                     viewColor(ColorRole::PrintGap, dark), draft, deviation,
                     viewColor(ColorRole::DeviationUnmeasured, dark), distance);
  // A surface quality analysis' stripes, colors, curves or mirrored faces
  // (mitcad#2517).
  addSurfaceOverlay(overlay, surfaceColors(), drawn);
  m_viewer->setPrintOverlay(drawn);
  // A point cloud a shown deviation colours: its measured points are the
  // deviation's (mitcad#2240), the others drawn dimmed (mitcad#2265).
  const DeviationSample sample = deviationSampleOf(overlay);
  if (sample != m_deviationSample) {
    m_deviationSample = sample;
    drawPointClouds();
  }
}

void MainWindow::followPrintAnalysis() {
  const QJsonObject shown = m_snapshot.shownAnalysis();
  const bool drawn = drawnOverBodies(shown);
  // The analysis as the model lists it (its settings, its error), the
  // design's structure, and the shapes of its bodies: a change of any of
  // them computes it again.
  QByteArray key;
  if (drawn) {
    key = compactJson(shown) + m_snapshot.key;
    QJsonArray uids = shown.value(QStringLiteral("bodies")).toArray();
    // A deviation's one body (mitcad#2240).
    if (shown.value(QStringLiteral("body")).isString()) {
      uids.append(shown.value(QStringLiteral("body")));
    }
    // A Minimum Radius' faces' bodies too (mitcad#2518).
    if (isMinRadius(shown)) {
      uids = minRadiusBodiesOf(shown);
    }
    // A minimum distance's picked bodies (mitcad#2519); a component's
    // bodies change the snapshot's key.
    for (const char* side : {"first", "second"}) {
      for (const QJsonValue& pick : shown.value(QLatin1String(side)).toArray()) {
        const QJsonValue body = pick.toObject().value(QStringLiteral("body"));
        if (body.isString()) {
          uids.append(body);
        }
      }
    }
    // The bodies of a surface analysis' faces (mitcad#2517).
    if (isSurface(shown)) {
      for (const QJsonValue& face : shown.value(QStringLiteral("faces")).toArray()) {
        if (face.toObject().value(QStringLiteral("body")).isString()) {
          uids.append(face.toObject().value(QStringLiteral("body")));
        }
      }
    }
    for (const QJsonValue& uid : uids) {
      std::shared_ptr<geometry::Shape> shape;
      try {
        shape = bodyShape(uid.toString());
      } catch (const std::exception&) {
        // gone: the model's error says so
      }
      key += ' ' + QByteArray::number(static_cast<qulonglong>(reinterpret_cast<quintptr>(shape.get())));
    }
  }
  if (key == m_printKey) {
    return;
  }
  m_printKey = key;
  const quint64 generation = ++m_printGeneration;
  if (!drawn) {
    showPrintOverlay(QJsonObject());
    return;
  }
  const QString name = shown.value(QStringLiteral("name")).toString();
  const QString error = shown.value(QStringLiteral("error")).toString();
  if (!error.isEmpty()) {
    // A body, the bed face or the pull reference gone: red in the browser
    // with the reason.
    showPrintOverlay(QJsonObject());
    qInfo().noquote() << QStringLiteral("%1 %2: failed: %3").arg(kindOf(shown), name, error);
    return;
  }
  // Not inside the refresh: the queries run as model jobs.
  TestSync::singleShot(0, this, [this, generation] { computePrintAnalysis(generation); });
}

void MainWindow::computePrintAnalysis(quint64 generation) {
  if (generation != m_printGeneration || (m_session && m_session->def().inspect)) {
    return; // a newer one, or a panel that shows its own
  }
  if (modelBusy()) {
    whenIdle(this, [this, generation] { computePrintAnalysis(generation); });
    return;
  }
  const QJsonObject shown = m_snapshot.shownAnalysis();
  if (!drawnOverBodies(shown)) {
    return;
  }
  const QString name = shown.value(QStringLiteral("name")).toString();
  const QString kind = kindOf(shown);
  QHash<QString, QString> names;
  for (auto body = m_snapshot.bodies.cbegin(); body != m_snapshot.bodies.cend(); ++body) {
    names.insert(body.key(), body->name);
  }
  const BodyNames bodyName = [names](const QString& uid) { return names.value(uid, uid); };
  const QString stage = isSurface(shown)     ? QStringLiteral("%1 %2").arg(titleOf(shown), name) // mitcad#2517
                        : isDraft(shown)     ? tr("Draft analysis of %1").arg(name)
                        : isDeviation(shown) ? tr("Point cloud deviation %1").arg(name)
                        : isMinRadius(shown) ? tr("Minimum radius %1").arg(name)
                        : isDistance(shown)  ? tr("Minimum distance %1").arg(name)
                                             : tr("Print analysis of %1").arg(name);

  if (asynchronousCommands()) {
    // The native window: the whole analysis as one job on the model's
    // worker, which the progress cancels.
    const bool started = modelInspectionAsync(
        titleOf(shown), this,
        [shown, name, bodyName](const Document& document) {
          return analyseShown(shown, name, bodyName, [&document](const QJsonObject& request, bool) {
            return parseObject(document.query(rustStr(compactJson(request))));
          });
        },
        [this, generation, kind, name](const Inspection& result, const CommandHost::AsyncResult& outcome) {
          if (generation != m_printGeneration) {
            return;
          }
          if (outcome.cancelled) {
            showPrintOverlay(QJsonObject());
            qInfo().noquote() << QStringLiteral("%1 %2: cancelled").arg(kind, name);
            return;
          }
          presentPrintAnalysis(kind, name, result, outcome.error);
        });
    if (!started) {
      whenIdle(this, [this, generation] { computePrintAnalysis(generation); });
    }
    return;
  }

  Inspection result;
  try {
    result = analyseShown(shown, name, bodyName, [this, &stage](const QJsonObject& request, bool slow) {
      return slow ? jobQuery(request, stage) : queryObject(request);
    });
  } catch (const ComputationCancelled&) {
    if (generation == m_printGeneration) {
      showPrintOverlay(QJsonObject());
      qInfo().noquote() << QStringLiteral("%1 %2: cancelled").arg(kind, name);
    }
    return;
  } catch (const std::exception& e) {
    result = Inspection();
    result.error = errorText(e);
  }
  if (generation != m_printGeneration) {
    return; // changed while the jobs ran
  }
  presentPrintAnalysis(kind, name, result, QString());
}

void MainWindow::presentPrintAnalysis(const QString& kind, const QString& name, const Inspection& result,
                                      const QString& error) {
  const QString failure = error.isEmpty() ? result.error : error;
  if (!failure.isEmpty()) {
    showPrintOverlay(QJsonObject());
    qInfo().noquote() << QStringLiteral("%1 %2: failed: %3").arg(kind, name, failure);
    return;
  }
  showPrintOverlay(result.printOverlay);
  // For UI tests (app/COMMANDS.md, "Logs that UI tests read").
  qInfo().noquote() << QStringLiteral("%1 %2: %3").arg(kind, name, result.log);
}

} // namespace mitcad
