// SPDX-License-Identifier: MIT
// Test steps for files, the clipboard and the session log.
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
#include <QDragEnterEvent>
#include <QDragMoveEvent>
#include <QDropEvent>
#include <QMimeData>
#include <QUrl>
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
    {"expect-session-log", 1, 2, TestDriver::kPlain},
    {"expect-file", 1, 2, TestDriver::kPlain},
    {"expect-files", 3, 4, TestDriver::kPlain},
    // The clipboard's text, and a saved file holding the same text
    // (mitcad#1641: the BUILD lists' Copy as Text and Save as Text…).
    {"expect-clipboard", 1, 2, TestDriver::kPlain},
    {"expect-clipboard-file", 1, 2, TestDriver::kPlain},
    // Files dropped from the file manager (mitcad#2204: onto a Projects
    // panel row).
    {"drop-files-logged", 2, -1, TestDriver::kInputOfMain},
    // The same onto the middle of a widget (mitcad#2281: the 3D view while
    // a command is open).
    {"drop-files-on-widget", 2, -1, TestDriver::kInputOfMain},
};

// Files dropped from the file manager at `at` (the main window's logical
// pixels), as the system's drag and drop delivers them: the enter, a move
// and the drop, each to the widget under the place that takes drops (an
// item view's viewport), with the files' URLs. Empty, or why it was not
// dropped.
QString dropFilesAt(QWidget* main, const QPoint& at, const QList<QUrl>& urls) {
  QWidget* target = main != nullptr ? main->childAt(at) : nullptr;
  if (target == nullptr && main != nullptr && main->rect().contains(at) && main->acceptDrops()) {
    target = main;
  }
  while (target != nullptr && !target->acceptDrops()) {
    target = target->parentWidget();
  }
  if (target == nullptr) {
    return QStringLiteral("nothing at %1,%2 takes drops").arg(at.x()).arg(at.y());
  }
  const QPoint local = target->mapFrom(main, at);
  const Qt::DropActions actions = Qt::CopyAction | Qt::MoveAction | Qt::LinkAction;
  QMimeData data;
  data.setUrls(urls);
  QDragEnterEvent enter(QPointF(local), actions, &data, Qt::LeftButton, Qt::NoModifier);
  QApplication::sendEvent(target, &enter);
  if (!enter.isAccepted()) {
    return QStringLiteral("%1 refuses the files").arg(target->objectName());
  }
  QDragMoveEvent move(QPointF(local), actions, &data, Qt::LeftButton, Qt::NoModifier);
  // As Qt delivers it (QWidgetWindow::handleDragMoveEvent): the move that
  // follows an accepted enter comes accepted, with the enter's drop action,
  // so a widget that only handles the enter and the drop (the main window
  // over the 3D view) keeps the files; a move built here starts ignored.
  move.setDropAction(enter.dropAction());
  move.setAccepted(enter.isAccepted());
  QApplication::sendEvent(target, &move);
  if (!move.isAccepted()) {
    return QStringLiteral("%1 refuses the files there").arg(target->objectName());
  }
  QDropEvent drop(QPointF(local), actions, &data, Qt::LeftButton, Qt::NoModifier);
  QApplication::sendEvent(target, &drop);
  qInfo().noquote() << QStringLiteral("TestDriver: dropped %1 file(s) at %2,%3").arg(urls.size()).arg(at.x()).arg(at.y());
  return QString();
}

} // namespace

const TestDriver::StepArea TestDriver::kFileSteps{
    kStepSpecs, std::size(kStepSpecs), &TestDriver::runFileStep};

void TestDriver::runFileStep(const QString& name, const QStringList& a, int line) {
  // A regular expression argument.
  const auto regex = [&](const QString& pattern, QRegularExpression& re) {
    re = QRegularExpression(pattern);
    if (!re.isValid()) {
      fail(line, QStringLiteral("bad regular expression '%1': %2").arg(pattern, re.errorString()));
      return false;
    }
    return true;
  };
  // Seconds from an optional argument.
  const auto seconds = [&](int index, double fallback, double& value) {
    value = fallback;
    if (index < a.size() && !toNumber(a.at(index), value)) {
      fail(line, QStringLiteral("'%1' is not a number of seconds").arg(a.at(index)));
      return false;
    }
    return true;
  };
  if (name == QLatin1String("expect-session-log")) {
    // A line of this run's session log file (mitcad#1896).
    QRegularExpression re;
    double limit = 6;
    if (!regex(a.at(0), re) || !seconds(1, 6, limit)) {
      return;
    }
    whenReady(
        [this, re] {
          const QString path = SessionLog::instance().currentFile();
          if (path.isEmpty()) {
            m_waitDetail = QStringLiteral("no session log is written");
            return false;
          }
          QFile file(path);
          if (!file.open(QIODevice::ReadOnly)) {
            m_waitDetail = QStringLiteral("%1 cannot be read").arg(path);
            return false;
          }
          const QStringList lines = QString::fromUtf8(file.readAll()).split(QLatin1Char('\n'));
          for (const QString& text : lines) {
            if (re.match(text).hasMatch()) {
              return true;
            }
          }
          m_waitDetail = QStringLiteral("%1 lines in %2").arg(lines.size()).arg(path);
          return false;
        },
        limit, QStringLiteral("'%1' is not in the session log").arg(a.at(0)));
  } else if (name == QLatin1String("expect-file")) {
    double limit = 6;
    if (!seconds(1, 6, limit)) {
      return;
    }
    const QString path = a.at(0);
    whenReady([path] { return QFileInfo::exists(path); }, limit, QStringLiteral("'%1' does not exist").arg(path));
  } else if (name == QLatin1String("expect-files")) {
    double limit = 6;
    bool ok = false;
    const int count = a.at(2).toInt(&ok);
    if (!ok || !seconds(3, 6, limit)) {
      fail(line, QStringLiteral("expect-files <dir> <glob> <count> [seconds]"));
      return;
    }
    const QString dir = a.at(0);
    const QString glob = a.at(1);
    whenReady(
        [this, dir, glob, count] {
          const QStringList names =
              QDir(dir).entryList(QStringList{glob}, QDir::Files | QDir::Hidden | QDir::NoDotAndDotDot);
          m_waitDetail = QStringLiteral("found %1: %2").arg(names.size()).arg(names.join(QStringLiteral(", ")));
          return names.size() == count;
        },
        limit, QStringLiteral("not %1 files '%2' in %3").arg(count).arg(glob, dir));
  } else if (name == QLatin1String("expect-clipboard")) {
    // The clipboard's whole text matches (mitcad#1641); line ends are as
    // the application put them (\r\n in a regular expression).
    QRegularExpression re;
    double limit = 6;
    if (!regex(a.at(0), re) || !seconds(1, 6, limit)) {
      return;
    }
    whenReady(
        [this, re] {
          const QString text = QApplication::clipboard()->text();
          QString shown = text.left(400);
          shown.replace(QLatin1Char('\r'), QStringLiteral("\\r")).replace(QLatin1Char('\n'), QStringLiteral("\\n"));
          m_waitDetail = QStringLiteral("the clipboard has %1 character(s): %2").arg(text.size()).arg(shown);
          return re.match(text).hasMatch();
        },
        limit, QStringLiteral("the clipboard does not match '%1'").arg(a.at(0)));
  } else if (name == QLatin1String("expect-clipboard-file")) {
    // The file holds exactly the clipboard's text in UTF-8 (mitcad#1641: Save
    // as Text writes what Copy as Text copies).
    double limit = 6;
    if (!seconds(1, 6, limit)) {
      return;
    }
    const QString path = a.at(0);
    whenReady(
        [this, path] {
          QFile file(path);
          if (!file.open(QIODevice::ReadOnly)) {
            m_waitDetail = QStringLiteral("cannot read it: %1").arg(file.errorString());
            return false;
          }
          const QString saved = QString::fromUtf8(file.readAll());
          const QString copied = QApplication::clipboard()->text();
          qsizetype at = 0;
          while (at < saved.size() && at < copied.size() && saved.at(at) == copied.at(at)) {
            ++at;
          }
          m_waitDetail = QStringLiteral("%1 character(s) saved, %2 on the clipboard, the first difference at %3")
                             .arg(saved.size())
                             .arg(copied.size())
                             .arg(at);
          return !copied.isEmpty() && saved == copied;
        },
        limit, QStringLiteral("'%1' does not hold the clipboard's text").arg(path));
  } else if (name == QLatin1String("drop-files-logged")) {
    // As the system's drag and drop delivers files from the file manager:
    // the enter, a move and the drop, each to the widget under the place
    // that takes drops (an item view's viewport), with the files' URLs.
    QRegularExpression re;
    if (!regex(a.at(0), re)) {
      return;
    }
    QList<QUrl> urls;
    for (const QString& path : a.mid(1)) {
      urls << QUrl::fromLocalFile(QFileInfo(path).absoluteFilePath());
    }
    const auto at = std::make_shared<QPoint>();
    whenReady([this, re, at] { return loggedPoint(re, 0, *at); }, 6,
              QStringLiteral("no place logged for '%1'").arg(a.at(0)), [this, at, urls, line] {
                queue([this, at, urls, line] {
                  const QString refused = dropFilesAt(m_main.data(), *at, urls);
                  if (!refused.isEmpty()) {
                    fail(line, refused);
                  }
                });
              });
  } else if (name == QLatin1String("drop-files-on-widget")) {
    // The same at the middle of the main window's widget of that object
    // name, once it is shown.
    const QString widget = a.at(0);
    QList<QUrl> urls;
    for (const QString& path : a.mid(1)) {
      urls << QUrl::fromLocalFile(QFileInfo(path).absoluteFilePath());
    }
    const auto at = std::make_shared<QPoint>();
    whenReady(
        [this, widget, at] {
          QWidget* main = m_main.data();
          QWidget* found = main != nullptr ? main->findChild<QWidget*>(widget) : nullptr;
          if (found == nullptr || !found->isVisible()) {
            return false;
          }
          *at = found->mapTo(main, found->rect().center());
          return true;
        },
        6, QStringLiteral("no shown widget '%1'").arg(widget), [this, at, urls, line] {
          queue([this, at, urls, line] {
            const QString refused = dropFilesAt(m_main.data(), *at, urls);
            if (!refused.isEmpty()) {
              fail(line, refused);
            }
          });
        });
  }
}

} // namespace mitcad
