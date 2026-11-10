// SPDX-License-Identifier: MIT
// Print Analysis (mitcad#620, app/COMMANDS.md "Print Analysis"): the
// overhangs and thin walls of the analysis shown in the document drawn over
// the bodies, with the thinnest point and the legend; the panel's own while
// it is open. Overhangs and walls may take long on large meshes: they run
// on the model's worker as jobs that can be cancelled, and the model keeps
// their results by the bodies' content and the settings, so showing an
// analysis again does not compute it again.
#include "MainWindow.hpp"

#include <exception>
#include <memory>

#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QtLogging>

#include "commands/PrintAnalysis.hpp"
#include "framework/Json.hpp"
#include "framework/ModelWorker.hpp"
#include "framework/TestSync.hpp"
#include "framework/Theme.hpp"

namespace mitcad {

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
  // not the widgets' palette (mitcad#1041).
  const bool dark = m_viewer->backgroundDark();
  m_viewer->setPrintOverlay(printOverlayOf(overlay, viewColor(ColorRole::PrintOverhang, dark),
                                           viewColor(ColorRole::PrintThin, dark),
                                           viewColor(ColorRole::PrintGap, dark)));
}

void MainWindow::followPrintAnalysis() {
  const QJsonObject shown = m_snapshot.shownAnalysis();
  const bool print = shown.value(QStringLiteral("type")).toString() == QStringLiteral("print");
  // The analysis as the model lists it (its settings, its error), the
  // design's structure, and the shapes of its bodies: a change of any of
  // them computes it again.
  QByteArray key;
  if (print) {
    key = compactJson(shown) + m_snapshot.key;
    for (const QJsonValue& uid : shown.value(QStringLiteral("bodies")).toArray()) {
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
  if (!print) {
    showPrintOverlay(QJsonObject());
    return;
  }
  const QString name = shown.value(QStringLiteral("name")).toString();
  const QString error = shown.value(QStringLiteral("error")).toString();
  if (!error.isEmpty()) {
    // A body or the bed face gone: red in the browser with the reason.
    showPrintOverlay(QJsonObject());
    qInfo().noquote() << QStringLiteral("Print analysis %1: failed: %2").arg(name, error);
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
  if (shown.value(QStringLiteral("type")).toString() != QStringLiteral("print")) {
    return;
  }
  const QString name = shown.value(QStringLiteral("name")).toString();
  QHash<QString, QString> names;
  for (auto body = m_snapshot.bodies.cbegin(); body != m_snapshot.bodies.cend(); ++body) {
    names.insert(body.key(), body->name);
  }
  const BodyNames bodyName = [names](const QString& uid) { return names.value(uid, uid); };
  const QString stage = tr("Print analysis of %1").arg(name);

  if (asynchronousCommands()) {
    // The native window: the whole analysis as one job on the model's
    // worker, which the progress cancels.
    const bool started = modelInspectionAsync(
        tr("Print Analysis"), this,
        [shown, name, bodyName](const Document& document) {
          Inspection result;
          PrintAnswers answers;
          result.error = runPrintAnalysis(
              shown,
              [&document](const QJsonObject& request, bool) {
                return parseObject(document.query(rustStr(compactJson(request))));
              },
              answers);
          if (result.error.isEmpty()) {
            result.log = printAnalysisLog(answers, bodyName);
            result.printOverlay = printOverlayJson(shown, answers, name);
          }
          return result;
        },
        [this, generation, name](const Inspection& result, const CommandHost::AsyncResult& outcome) {
          if (generation != m_printGeneration) {
            return;
          }
          if (outcome.cancelled) {
            showPrintOverlay(QJsonObject());
            qInfo().noquote() << QStringLiteral("Print analysis %1: cancelled").arg(name);
            return;
          }
          presentPrintAnalysis(name, result, outcome.error);
        });
    if (!started) {
      whenIdle(this, [this, generation] { computePrintAnalysis(generation); });
    }
    return;
  }

  Inspection result;
  PrintAnswers answers;
  try {
    result.error = runPrintAnalysis(
        shown,
        [this, &stage](const QJsonObject& request, bool slow) {
          return slow ? jobQuery(request, stage) : queryObject(request);
        },
        answers);
  } catch (const ComputationCancelled&) {
    if (generation == m_printGeneration) {
      showPrintOverlay(QJsonObject());
      qInfo().noquote() << QStringLiteral("Print analysis %1: cancelled").arg(name);
    }
    return;
  } catch (const std::exception& e) {
    result.error = errorText(e);
  }
  if (generation != m_printGeneration) {
    return; // changed while the jobs ran
  }
  if (result.error.isEmpty()) {
    result.log = printAnalysisLog(answers, bodyName);
    result.printOverlay = printOverlayJson(shown, answers, name);
  }
  presentPrintAnalysis(name, result, QString());
}

void MainWindow::presentPrintAnalysis(const QString& name, const Inspection& result, const QString& error) {
  const QString failure = error.isEmpty() ? result.error : error;
  if (!failure.isEmpty()) {
    showPrintOverlay(QJsonObject());
    qInfo().noquote() << QStringLiteral("Print analysis %1: failed: %2").arg(name, failure);
    return;
  }
  showPrintOverlay(result.printOverlay);
  // For UI tests (app/COMMANDS.md, "Logs that UI tests read").
  qInfo().noquote() << QStringLiteral("Print analysis %1: %2").arg(name, result.log);
}

} // namespace mitcad
