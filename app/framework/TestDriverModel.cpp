// SPDX-License-Identifier: MIT
// Test steps for the model and its jobs: numbers, parameters and bodies the model reports,
// commands sent to it, the undo depth, held previews and edits, a job
// cancelled, failed or given input while it runs.
// Split from TestDriver.cpp (mitcad#2173), so that a UI test depends on the
// step areas it uses; the steps are described in TestDriver.hpp.
#include "TestDriver.hpp"
#include "TestDriverSupport.hpp"
#include "Diagnostics.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdlib>
#include <limits>
#include <memory>
#include <unordered_set>
#include <utility>
#include <vector>

#include <BRep_Tool.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Poly_Polygon3D.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_TShape.hxx>

#include <QAbstractButton>
#include <QAbstractItemView>
#include <QAction>
#include <QAbstractScrollArea>
#include <QApplication>
#include <QClipboard>
#include <QColor>
#include <QComboBox>
#include <QContextMenuEvent>
#include <QDir>
#include <QDockWidget>
#include <QEnterEvent>
#include <QEvent>
#include <QFile>
#include <QFileInfo>
#include <QFormLayout>
#include <QHash>
#include <QHeaderView>
#include <QIcon>
#include <QImage>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QKeySequence>
#include <QLabel>
#include <QLayout>
#include <QLineEdit>
#include <QPlainTextEdit>
#include <QMenu>
#include <QMenuBar>
#include <QMessageBox>
#include <QMouseEvent>
#include <QNativeGestureEvent>
#include <QOpenGLWidget>
#include <QPainter>
#include <QPixmap>
#include <QPointingDevice>
#include <QRegion>
#include <QRegularExpression>
#include <QScreen>
#include <QScrollArea>
#include <QSettings>
#include <QShortcut>
#include <QStyleHints>
#include <QThread>
#include <QTimer>
#include <QToolBar>
#include <QToolButton>
#include <QToolTip>
#include <QTreeWidget>
#include <QTreeWidgetItemIterator>
#include <QTextDocument>
#include <QWheelEvent>
#include <QWidget>
#include <QWindow>
#include <QtGlobal>
#include <QtLogging>

#include "../platform/MacChrome.hpp"
#include "../platform/MacTestApplication.hpp"
#include "../platform/NativeTestInput.hpp"
#include "../OcctViewer.hpp"
#include "../DocumentFrame.hpp"
#include "../MainWindow.hpp"
#include "../WindowManager.hpp"
#include "../update/UpdateController.hpp"
#include "../view/SpaceMouseController.hpp"
#include "../view/ViewController.hpp"
#include "../view/ViewEdges.hpp"
#include "../sketch/SketchController.hpp"
#include "../sketch/SketchOverlay.hpp"
#include "../sketch/SketchTool.hpp"
#include "../browser/TimelineWidget.hpp"
#include "GlassCard.hpp"
#include "Ribbon.hpp"
#include "TestSync.hpp"
#include "HangWatchdog.hpp"
#include "SessionLog.hpp"
#include "Dialogs.hpp"
#include "DisplayMeshes.hpp"
#include "Icons.hpp"
#include "ComputeProgress.hpp"
#include "Theme.hpp"
#include "TreeItemDelegate.hpp"
#include "mitcad/geometry/geometry.hpp"
#include "mitcad/io/mesh.hpp"

namespace mitcad {

using namespace testdriver;

namespace {

const TestDriver::StepSpec kStepSpecs[] = {
    {"capture-model-number", 3, 3, TestDriver::kIdle},
    {"expect-document-cache-sharing", 1, 1, TestDriver::kIdle},
    {"qt-cancel-progress", 1, 1, TestDriver::kPlain},
    {"cancel-model-job", 1, 1, TestDriver::kPlain},
    {"native-click-on-job", 2, 2, TestDriver::kIdle},
    {"native-history-drag-on-job", 2, 2, TestDriver::kIdle},
    {"fail-next-recovery-command", 1, 1, TestDriver::kIdle},
    {"expect-picking-on-job", 1, 1, TestDriver::kIdle},
    {"expect-model-busy", 1, 1, TestDriver::kPlain},
    {"hold-model-preview", 1, 1, TestDriver::kPlain},
    {"hold-model-edit", 1, 1, TestDriver::kPlain},
    {"hold-model-display", 1, 1, TestDriver::kPlain},
    {"capture-undo-depth", 1, 1, TestDriver::kIdle},
    {"capture-sketch", 2, 2, TestDriver::kIdle},
    {"expect-undo-depth", 2, 2, TestDriver::kIdle},
    {"expect-model-bodies", 2, 2, TestDriver::kIdle},
    // Whether the design has unsaved changes (yes|no, mitcad#2168).
    {"expect-document-modified", 1, 1, TestDriver::kIdle},
    {"expect-model-parameter", 2, 3, TestDriver::kIdle},
    {"model-command", 1, 1, TestDriver::kIdle},
    // A command the model refuses: its refusal shown as the user sees it
    // (mitcad#1284: the model's message in the interface language).
    {"model-command-refused", 1, 1, TestDriver::kIdle},
};

} // namespace

const TestDriver::StepArea TestDriver::kModelSteps{
    kStepSpecs, std::size(kStepSpecs), &TestDriver::runModelStep};

void TestDriver::runModelStep(const QString& name, const QStringList& a, int line) {
  QWidget* mainWindow = m_main.data();
  if (name == QLatin1String("capture-model-number")) {
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    const QJsonDocument query = QJsonDocument::fromJson(a.at(1).toUtf8());
    if (!window || !query.isObject()) { fail(line, QStringLiteral("capture-model-number requires a document and JSON query")); return; }
    QJsonValue value = window->query(query.object());
    // A number in the path indexes an array (`parts.0.length`, mitcad#1814).
    for (const QString& part : a.at(2).split(QLatin1Char('.'))) {
      bool isPathIndex = false;
      const int pathIndex = part.toInt(&isPathIndex);
      value = isPathIndex && value.isArray() ? value.toArray().at(pathIndex) : value.toObject().value(part);
    }
    if (!value.isDouble() || !std::isfinite(value.toDouble())) {
      fail(line, QStringLiteral("model query path '%1' is not a finite number").arg(a.at(2))); return;
    }
    m_vars.insert(a.at(0), QString::number(value.toDouble(), 'g', 17));
  } else if (name == QLatin1String("expect-document-cache-sharing")) {
    double total = 0;
    if (!toNumber(a.at(0), total) || total <= 0) { fail(line, QStringLiteral("expected a positive process cache budget")); return; }
    const QPointer<WindowManager> manager = WindowManager::instance();
    whenReady([manager, total] {
      if (!manager || manager->windows().isEmpty()) return false;
      const auto windows = manager->windows();
      double sum = 0;
      for (MainWindow* window : windows) {
        if (window->modelBusy()) return false;
        const double budget = window->query(QJsonObject{{QStringLiteral("query"), QStringLiteral("cache")}})
            .toObject().value(QStringLiteral("memory")).toObject().value(QStringLiteral("budget")).toDouble(-1);
        if (budget <= 0 || budget > total || (windows.size() > 1 && budget >= total)) return false;
        sum += budget;
      }
      return sum <= total + 1 && (windows.size() > 1 || std::abs(sum - total) <= 1);
    }, 10, QStringLiteral("the actual document caches did not share the process budget"));
  } else if (name == QLatin1String("expect-picking-on-job")) {
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    auto* session = window ? window->findChild<DocumentSession*>() : nullptr;
    if (!session || window->modelBusy() || !m_jobPickLabel.isEmpty()) {
      fail(line, QStringLiteral("expect-picking-on-job requires an idle document and no armed pick")); return;
    }
    auto& viewer = window->viewer();
    const auto points = viewer.pickPoints(SelectKind::Face | SelectKind::Edge | SelectKind::Body);
    if (points.empty() || !viewer.itemAt(points.front().second)) {
      fail(line, QStringLiteral("expect-picking-on-job requires a pickable body")); return;
    }
    const QPoint point = points.front().second;
    const QString label = a.at(0);
    m_jobPickLabel = label;
    m_jobPickLine = line;
    const QPointer<MainWindow> guard(window);
    m_jobPickConnection = connect(session, &DocumentSession::jobStarted, this,
        [this, guard, point, label, line](const QString& started, const QString&) {
          if (started != label) return;
          QObject::disconnect(m_jobPickConnection);
          m_jobPickLabel.clear();
          if (!guard || !guard->modelBusy()) {
            fail(line, QStringLiteral("the armed pick did not observe a busy job")); return;
          }
          // Same OCCT MoveTo/filter path as a hover inside paintGL, with
          // deterministic timing before the job can finish.
          if (guard->viewer().itemAt(point) || !guard->viewer().itemsAt(point).empty() ||
              !guard->viewer().pickPoints(SelectKind::Face | SelectKind::Edge | SelectKind::Body).empty()) {
            fail(line, QStringLiteral("a model item remained pickable during a job")); return;
          }
          qInfo().noquote() << QStringLiteral("Picking suspended during %1 (busy)").arg(label);
        });
  } else if (name == QLatin1String("cancel-model-job")) {
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    auto* session = window ? window->findChild<DocumentSession*>() : nullptr;
    if (!session || !session->busy() || !session->activeJob() ||
        session->activeJob()->stage() != tr("Preview of %1").arg(a.at(0))) {
      fail(line, QStringLiteral("cancel-model-job requires the named command's running preview")); return;
    }
    session->cancelJob();
    qInfo().noquote() << QStringLiteral("Test preview cancellation requested: %1").arg(a.at(0));
  } else if (name == QLatin1String("qt-cancel-progress")) {
    if (mac::test::active() || a.at(0).isEmpty()) {
      fail(line, QStringLiteral("qt-cancel-progress requires the Qt frontend and a job label")); return;
    }
    const QString label = a.at(0);
    const auto ownsProgress = [](const MainWindow* window, const ComputeProgress* progress) {
      if (!window || !progress) return false;
      // A window-modal QDialog is a top-level widget. QWidget ancestry
      // stops at that window boundary, but QObject ownership retains the
      // document that constructed this particular progress dialog.
      for (const QObject* owner = progress->parent(); owner; owner = owner->parent()) {
        if (owner == window) return true;
      }
      return false;
    };
    whenReady([this, label, ownsProgress] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      QWidget* modal = QApplication::activeModalWidget();
      auto* progress = qobject_cast<ComputeProgress*>(modal);
      // The synchronous Qt wait logs its current job before showing this
      // modal dialog. Match the newest start, so a preceding preview cannot
      // satisfy an edit_feature cancellation request.
      static const QRegularExpression started(QStringLiteral("^Computing (\\S+) started$"));
      QRegularExpressionMatch match;
      const int newest = findLastLine(started, m_mark, nullptr, &match);
      QStringList candidates;
      if (window) {
        for (const auto* candidate : window->findChildren<ComputeProgress*>()) {
          candidates << QStringLiteral("%1:visible=%2,modal=%3,owned=%4")
              .arg(candidate->objectName()).arg(candidate->isVisible()).arg(int(candidate->windowModality()))
              .arg(ownsProgress(window, candidate));
        }
      }
      const QString detail = QStringLiteral("busy=%1, appState=%2, activeModal=%3/%4, visible=%5, progress=%6, "
          "cancelling=%7, owned=%8, newestJob=%9, candidates=[%10]")
          .arg(window && window->modelBusy()).arg(int(QGuiApplication::applicationState()))
          .arg(modal ? QString::fromLatin1(modal->metaObject()->className()) : QStringLiteral("none"))
          .arg(modal ? modal->objectName() : QStringLiteral("none")).arg(modal && modal->isVisible())
          .arg(progress != nullptr).arg(progress && progress->isCancelling())
          .arg(ownsProgress(window, progress))
          .arg(newest >= 0 ? match.captured(1) : QStringLiteral("none")).arg(candidates.join(QStringLiteral("; ")));
      if (detail != m_waitDetail && window && window->modelBusy()) {
        qInfo().noquote() << QStringLiteral("TestDriver: Qt progress readiness: %1").arg(detail);
      }
      m_waitDetail = detail;
      if (!window || !window->modelBusy() || !progress || !progress->isVisible() || progress->isCancelling() ||
          !ownsProgress(window, progress) || newest < 0 || match.captured(1) != label) return false;
      m_found = progress;
      return true;
    }, 10, QStringLiteral("no live Qt progress dialog for '%1'").arg(label), [this, label, line, ownsProgress] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      auto* progress = qobject_cast<ComputeProgress*>(m_found.data());
      if (!window || !window->modelBusy() || !progress || QApplication::activeModalWidget() != progress ||
          !progress->isVisible() || progress->isCancelling() || !ownsProgress(window, progress)) {
        fail(line, QStringLiteral("the Qt progress dialog lost its live job before cancellation")); return;
      }
      const auto buttons = progress->findChildren<QAbstractButton*>();
      for (QAbstractButton* button : buttons) {
        if (plainText(button->text()) == QStringLiteral("Cancel") && button->isVisible() && button->isEnabled()) {
          button->click();
          if (!progress->isCancelling()) {
            fail(line, QStringLiteral("the real Qt progress Cancel button did not request cancellation")); return;
          }
          qInfo().noquote() << QStringLiteral("Qt test: clicked progress Cancel during %1 (busy)").arg(label);
          return;
        }
      }
      fail(line, QStringLiteral("the live Qt progress dialog has no enabled Cancel button"));
    });
  } else if (name == QLatin1String("native-click-on-job")) {
    if (!mac::test::active()) { fail(line, QStringLiteral("native-click-on-job requires the native macOS UI")); return; }
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    auto* session = window ? window->findChild<DocumentSession*>() : nullptr;
    if (!session || window->modelBusy() || a.at(0).isEmpty() || a.at(1).isEmpty() || !m_nativeJobClickLabel.isEmpty()) {
      fail(line, QStringLiteral("native-click-on-job requires an idle document and no existing armed click")); return;
    }
    m_nativeJobClickLabel = a.at(0);
    m_nativeJobClickLine = line;
    const QString label = a.at(0);
    const QString identifier = a.at(1);
    const QPointer<MainWindow> guard(window);
    m_nativeJobClickConnection = QObject::connect(session, &DocumentSession::jobStarted, this,
        [this, guard, label, identifier, line](const QString& started, const QString&) {
          if (started != label) return;
          QObject::disconnect(m_nativeJobClickConnection);
          m_nativeJobClickLabel.clear();
          if (!guard || !guard->modelBusy()) {
            fail(line, QStringLiteral("the armed click did not observe an actual busy '%1' job").arg(label)); return;
          }
          QString error;
          if (!mac::test::clickImmediately(identifier, error)) { fail(line, error); return; }
          qInfo().noquote() << QStringLiteral("Native test: clicked %1 during %2 (busy)").arg(identifier, label);
        });
  } else if (name == QLatin1String("native-history-drag-on-job")) {
    if (!mac::test::active()) { fail(line, QStringLiteral("native-history-drag-on-job requires the native macOS UI")); return; }
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    auto* session = window ? window->findChild<DocumentSession*>() : nullptr;
    double index = -1;
    if (!session || window->modelBusy() || a.at(0).isEmpty() || !m_nativeJobHistoryDragLabel.isEmpty() ||
        !toNumber(a.at(1), index) || index < 0 || index != std::floor(index)) {
      fail(line, QStringLiteral("native-history-drag-on-job requires an idle document, a final index and no armed drag"));
      return;
    }
    m_nativeJobHistoryDragLabel = a.at(0);
    m_nativeJobHistoryDragLine = line;
    const QString label = a.at(0);
    const QPointer<MainWindow> guard(window);
    m_nativeJobHistoryDragConnection = QObject::connect(session, &DocumentSession::jobStarted, this,
        [this, guard, label, index, line](const QString& started, const QString&) {
          if (started != label) return;
          QObject::disconnect(m_nativeJobHistoryDragConnection);
          m_nativeJobHistoryDragLabel.clear();
          for (const QString& phase : {QStringLiteral("to"), QStringLiteral("end")}) {
            if (!guard || !guard->modelBusy()) {
              fail(line, QStringLiteral("the armed history drag did not observe busy '%1' before %2").arg(label, phase));
              return;
            }
            QString error;
            if (!mac::test::historyMarkerEvent(phase, int(index), error)) { fail(line, error); return; }
            if (!guard || !guard->modelBusy()) {
              fail(line, QStringLiteral("the armed history drag did not observe busy '%1' after %2").arg(label, phase));
              return;
            }
          }
          qInfo().noquote() << QStringLiteral("Native test: history marker moved to %1 and released during %2 (busy before/after both inputs)")
              .arg(int(index)).arg(label);
        });
  } else if (name == QLatin1String("fail-next-recovery-command")) {
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    auto* session = window ? window->findChild<DocumentSession*>() : nullptr;
    if (!mac::test::active() || !session || window->modelBusy() || !session->state().activeCommand ||
        !session->m_testFailCommand.isEmpty() ||
        (a.at(0) != QLatin1String("restore_command") && a.at(0) != QLatin1String("discard_command"))) {
      fail(line, QStringLiteral("fail-next-recovery-command requires an idle native command and restore_command|discard_command"));
      return;
    }
    session->m_testFailCommand = a.at(0);
  } else if (name == QLatin1String("model-command") || name == QLatin1String("model-command-refused")) {
    const bool refusal = name == QLatin1String("model-command-refused");
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    QJsonParseError error;
    const QJsonDocument document = QJsonDocument::fromJson(a.at(0).toUtf8(), &error);
    if (!window || error.error != QJsonParseError::NoError || !document.isObject() ||
        !document.object().value(QStringLiteral("cmd")).isString()) {
      fail(line, QStringLiteral("%1 requires a document and one JSON command object").arg(name));
      return;
    }
    const bool done =
        window->runVisibilityCommands(QJsonArray{document.object()}, QStringLiteral("UI test model command"));
    if (!done && !refusal) {
      fail(line, QStringLiteral("model-command was refused: %1").arg(a.at(0)));
      return;
    }
    if (done && refusal) {
      fail(line, QStringLiteral("model-command-refused was not refused: %1").arg(a.at(0)));
      return;
    }
    const QString logged = refusal ? QStringLiteral("TestDriver model command refused: %1")
                                   : QStringLiteral("TestDriver model command: %1");
    qInfo().noquote() << logged.arg(document.object().value(QStringLiteral("cmd")).toString());
  } else if (name == QLatin1String("capture-undo-depth")) {
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    if (!window || window->modelBusy()) { fail(line, QStringLiteral("capture-undo-depth requires an idle document")); return; }
    const QJsonValue depth = window->query({{QStringLiteral("query"), QStringLiteral("document")}})
                                 .toObject().value(QStringLiteral("undo_depth"));
    if (!depth.isDouble() || !std::isfinite(depth.toDouble()) || depth.toDouble() < 0 ||
        depth.toDouble() != std::floor(depth.toDouble())) {
      fail(line, QStringLiteral("the document query returned no valid numeric undo depth")); return;
    }
    m_vars[a.at(0)] = QString::number(depth.toInteger());
  } else if (name == QLatin1String("expect-undo-depth")) {
    double base = 0;
    double delta = 0;
    if (!toNumber(a.at(0), base) || !toNumber(a.at(1), delta) || base != std::floor(base) ||
        delta != std::floor(delta) || base + delta < 0) {
      fail(line, QStringLiteral("expect-undo-depth <base-depth> <integer-change>")); return;
    }
    whenReady([this, base, delta] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window || window->modelBusy()) return false;
      const int depth = window->query({{QStringLiteral("query"), QStringLiteral("document")}})
                            .toObject().value(QStringLiteral("undo_depth")).toInt(-1);
      m_waitDetail = QStringLiteral("actual undo depth %1").arg(depth);
      return depth == base + delta;
    }, 6, QStringLiteral("undo depth did not change by %1 from %2").arg(delta).arg(base));
  } else if (name == QLatin1String("capture-sketch")) {
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    if (!window || window->modelBusy()) {
      fail(line, QStringLiteral("capture-sketch requires an idle document")); return;
    }
    const QJsonValue sketch = window->query({{QStringLiteral("query"), QStringLiteral("sketch")},
                                             {QStringLiteral("uid"), a.at(0)}});
    if (!sketch.isObject() || !sketch.toObject().value(QStringLiteral("entities")).isArray()) {
      fail(line, QStringLiteral("the sketch query returned no entities")); return;
    }
    const QByteArray json = QJsonDocument(sketch.toObject()).toJson();
    QDir().mkpath(QFileInfo(a.at(1)).absolutePath());
    QFile output(a.at(1));
    if (!output.open(QIODevice::WriteOnly) || output.write(json) != json.size()) {
      fail(line, QStringLiteral("cannot write the sketch query snapshot"));
    }
  } else if (name == QLatin1String("expect-model-parameter")) {
    const QString parameter = a.at(0);
    const QString kind = a.size() == 3 ? a.at(2) : QStringLiteral("user");
    if (kind != QLatin1String("user") && kind != QLatin1String("model") && kind != QLatin1String("reference")) {
      fail(line, QStringLiteral("expect-model-parameter <name> <value|absent> [user|model|reference]"));
      return;
    }
    const bool absent = a.at(1) == QStringLiteral("absent");
    double expected = 0;
    if (!absent && !toNumber(a.at(1), expected)) { return; }
    whenReady([this, parameter, kind, absent, expected] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window || window->modelBusy()) { return false; }
      for (const QJsonValue& value : window->queryArray(QStringLiteral("parameters"))) {
        const QJsonObject current = value.toObject();
        if (current.value(QStringLiteral("name")).toString() != parameter) { continue; }
        return !absent && current.value(QStringLiteral("kind")).toString() == kind &&
            std::abs(current.value(QStringLiteral("value")).toDouble() - expected) < 1e-6;
      }
      return absent;
    }, 6, QStringLiteral("parameter %1 did not match %2").arg(parameter, a.at(1)));
  } else if (name == QLatin1String("expect-model-bodies")) {
    double count = 0;
    double volume = 0;
    if (!toNumber(a.at(0), count) || !toNumber(a.at(1), volume) || count < 0 || count != std::floor(count) || volume < 0) {
      fail(line, QStringLiteral("expect-model-bodies <count> <total-volume-mm3>")); return;
    }
    whenReady([this, count, volume] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window || window->modelBusy()) return false;
      const QJsonValue result = window->query({{QStringLiteral("query"), QStringLiteral("bodies")},
                                              {QStringLiteral("volumes"), true}});
      if (!result.isArray()) {
        m_waitDetail = QStringLiteral("the bodies query returned a non-array result");
        return false;
      }
      const QJsonArray bodies = result.toArray();
      double actual = 0;
      for (const QJsonValue& body : bodies) {
        const QJsonValue amount = body.toObject().value(QStringLiteral("volume"));
        if (!amount.isDouble() || !std::isfinite(amount.toDouble()) || amount.toDouble() < 0) {
          m_waitDetail = QStringLiteral("a body has no finite numeric volume");
          return false;
        }
        actual += amount.toDouble();
      }
      if (!std::isfinite(actual)) { m_waitDetail = QStringLiteral("the total body volume is not finite"); return false; }
      m_waitDetail = QStringLiteral("%1 bodies, total volume %2 mm3").arg(bodies.size()).arg(actual, 0, 'g', 12);
      return bodies.size() == count && std::abs(actual - volume) <= std::max(1e-5, volume * 1e-6);
    }, 6, QStringLiteral("expected %1 bodies with total volume %2 mm3").arg(count).arg(volume));
  } else if (name == QLatin1String("expect-document-modified")) {
    if (a.at(0) != QLatin1String("yes") && a.at(0) != QLatin1String("no")) {
      fail(line, QStringLiteral("expect-document-modified yes|no")); return;
    }
    const bool wanted = a.at(0) == QLatin1String("yes");
    whenReady([this, wanted] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window || window->modelBusy()) return false;
      // As the model marks it (MainWindow::isModified), and as the title shows it.
      const bool model = window->query({{QStringLiteral("query"), QStringLiteral("document")}})
                             .toObject()
                             .value(QStringLiteral("modified"))
                             .toBool();
      const bool title = window->isWindowModified();
      m_waitDetail = QStringLiteral("modified: the model %1, the title %2")
                         .arg(model ? QStringLiteral("yes") : QStringLiteral("no"),
                              title ? QStringLiteral("yes") : QStringLiteral("no"));
      return model == wanted && title == wanted;
    }, 6, wanted ? QStringLiteral("expected the design to have unsaved changes")
                 : QStringLiteral("expected the design to have no unsaved changes"));
  } else if (name == QLatin1String("hold-model-preview") || name == QLatin1String("hold-model-edit") ||
             name == QLatin1String("hold-model-display")) {
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    if (!window || (a.at(0) != QStringLiteral("true") && a.at(0) != QStringLiteral("false"))) {
      fail(line, QStringLiteral("%1 true|false requires a document window").arg(name)); return;
    }
    auto& session = window->documentSession();
    const auto gate = name == QLatin1String("hold-model-preview") ? session.m_testHoldPreview
                      : name == QLatin1String("hold-model-edit")  ? session.m_testHoldEdit
                                                                   : session.m_testHoldDisplay;
    gate->store(a.at(0) == QStringLiteral("true"));
  } else if (name == QLatin1String("expect-model-busy")) {
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    if (!window || (a.at(0) != QStringLiteral("true") && a.at(0) != QStringLiteral("false"))) {
      fail(line, QStringLiteral("expect-model-busy true|false requires a document window")); return;
    }
    if (window->modelBusy() != (a.at(0) == QStringLiteral("true"))) {
      fail(line, QStringLiteral("the document was not busy=%1 at this input step").arg(a.at(0)));
    }
  }
}

} // namespace mitcad
