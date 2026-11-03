// SPDX-License-Identifier: MIT
// Real portable document-frame input and observations (mitcad#1172).
#include "TestDriver.hpp"
#include "GlassCard.hpp"
#include "MainWindow.hpp"
#include "DocumentFrame.hpp"
#include "WindowManager.hpp"

#include <QApplication>
#include <QContextMenuEvent>
#include <QDragEnterEvent>
#include <QDragMoveEvent>
#include <QDropEvent>
#include <QElapsedTimer>
#include <QMimeData>
#include <QMouseEvent>
#include <QOpenGLWidget>
#include <QSet>
#include <QTabBar>
#include <QtLogging>
#include <cmath>
#include <initializer_list>
#include <memory>
#include <type_traits>

namespace mitcad {
namespace {
QTabBar* tabsOf(DocumentFrame* frame) {
  return frame ? frame->findChild<QTabBar*>(QStringLiteral("documentTabs")) : nullptr;
}

bool liveBar(QTabBar* tabs) {
  return tabs && tabs->isVisible() && tabs->isEnabled() && tabs->window()->isVisible();
}

// The frame is shown, and its tab bar is shown exactly when the frame's
// policy shows tabs for its documents (one document: no tab bar).
bool barAsPolicy(DocumentFrame* frame, QTabBar* tabs) {
  if (!frame || !tabs || !frame->isVisible()) return false;
  return DocumentFrame::showsDocumentTabs(int(frame->documents().size())) ? liveBar(tabs) : tabs->isHidden();
}

template<class Event> auto dragPosition(const QPoint& point) {
  // Retain Qt 6.8 support while using the non-deprecated overload on newer Qt.
  if constexpr (std::is_constructible_v<Event, QPointF, Qt::DropActions, const QMimeData*,
                                        Qt::MouseButtons, Qt::KeyboardModifiers>) return QPointF(point);
  else return point;
}
} // namespace

bool TestDriver::runDocumentFrameStep(const QString& name, const QStringList& a, int line) {
  const bool matches = name.startsWith(QLatin1String("document-tab-")) ||
                       name.startsWith(QLatin1String("expect-document-frame-")) ||
                       name == QLatin1String("observe-document-frame-close-all") ||
                       name == QLatin1String("expect-document-hidden-quiet");
  if (!matches) return false;
  auto* current = qobject_cast<MainWindow*>(m_main.data());
  QPointer<DocumentFrame> frame = DocumentFrame::containing(current);
  QPointer<QTabBar> tabs = tabsOf(frame);
  // Expectations also hold for a frame with one document and no tab bar,
  // and a drop needs only the destination's bar; other tab input needs
  // this frame's shown bar.
  const bool ownBar = !name.startsWith(QLatin1String("expect-document-")) && name != QLatin1String("document-tab-drop");
  if (!frame || !tabs || (ownBar ? !liveBar(tabs) : !frame->isVisible())) {
    fail(line, QStringLiteral("%1 requires a visible portable document frame%2").arg(name, ownBar ? QStringLiteral(" and tab bar") : QString()));
    return true;
  }
  const auto indexOf = [this, line, tabs](const QString& text, int& index) {
    bool ok = false;
    index = text.toInt(&ok);
    if (!ok || index < 0 || !tabs || index >= tabs->count() || !tabs->isTabVisible(index) ||
        !tabs->isTabEnabled(index) || !tabs->rect().contains(tabs->tabRect(index).center())) {
      fail(line, QStringLiteral("no visible enabled document tab at index '%1'").arg(text));
      return false;
    }
    return true;
  };
  if (name == QLatin1String("observe-document-frame-close-all")) {
    QPointer<WindowManager> manager = WindowManager::instance();
    if (!manager || manager->windows().size() != frame->documents().size()) {
      fail(line, QStringLiteral("Close All process observation requires all documents in this frame")); return true;
    }
    struct Closes { QSet<MainWindow*> remaining; int count = 0; QMetaObject::Connection connection; };
    auto closes = std::make_shared<Closes>();
    for (MainWindow* document : frame->documents()) closes->remaining.insert(document);
    closes->count = int(closes->remaining.size());
    closes->connection = connect(manager, &WindowManager::windowRemoved, this,
                                [this, closes, manager, line](MainWindow* document) {
      closes->remaining.remove(document);
      if (!closes->remaining.isEmpty()) return;
      disconnect(closes->connection);
      if (!manager || !manager->windows().isEmpty()) {
        fail(line, QStringLiteral("Close All left documents in the process registry"));
      } else qInfo().noquote() << QStringLiteral("TestDriver: observed Close All: %1 accepted document closes, empty process registry")
                                     .arg(closes->count);
    });
  } else if (name == QLatin1String("expect-document-frame-tabs")) {
    bool ok = false;
    const int expected = a.at(0).toInt(&ok);
    if (!ok || expected < 1) { fail(line, QStringLiteral("expected a positive document tab count")); return true; }
    whenReady([this, expected] {
      auto* document = qobject_cast<MainWindow*>(m_main.data());
      auto* host = DocumentFrame::containing(document);
      auto* bar = tabsOf(host);
      m_waitDetail = QStringLiteral("actual bar %1 tabs, stack %2 documents")
          .arg(bar ? bar->count() : -1).arg(host ? host->documents().size() : -1);
      return barAsPolicy(host, bar) && bar->count() == expected && host->documents().size() == expected &&
             host->activeDocument() == document && bar->currentIndex() == host->documents().indexOf(document);
    }, 6, QStringLiteral("document frame did not show %1 tabs").arg(expected));
  } else if (name == QLatin1String("expect-document-frame-order")) {
    QList<QPointer<QWidget>> expected;
    for (const QString& capture : a) {
      const auto document = m_documentWindows.value(capture);
      if (!document) { fail(line, QStringLiteral("unknown or closed document capture '%1'").arg(capture)); return true; }
      expected.append(document);
    }
    whenReady([this, frame, tabs, expected] {
      if (!barAsPolicy(frame, tabs) || frame->documents().size() != expected.size() || tabs->count() != expected.size()) return false;
      const auto actual = frame->documents();
      for (qsizetype i = 0; i < expected.size(); ++i) {
        if (!expected.at(i) || actual.at(i) != expected.at(i).data() || !tabs->isTabVisible(int(i))) {
          m_waitDetail = QStringLiteral("document order differs at actual tab %1").arg(i);
          return false;
        }
      }
      return true;
    }, 6, QStringLiteral("actual document frame order differs from '%1'").arg(a.join(QLatin1Char(' '))));
  } else if (name == QLatin1String("document-tab-click") || name == QLatin1String("document-tab-context")) {
    int index = -1;
    if (!indexOf(a.at(0), index)) return true;
    const QPoint point = tabs->tabRect(index).center();
    if (name == QLatin1String("document-tab-context")) {
      queue([this, tabs, point, line] {
        if (!liveBar(tabs)) { fail(line, QStringLiteral("document tab bar disappeared before its context event")); return; }
        QContextMenuEvent event(QContextMenuEvent::Mouse, point, tabs->mapToGlobal(point));
        QCoreApplication::sendEvent(tabs, &event);
      });
    } else {
      const QString buttonName = a.size() > 1 ? a.at(1) : QStringLiteral("left");
      if (buttonName != QLatin1String("left") && buttonName != QLatin1String("middle")) {
        fail(line, QStringLiteral("document-tab-click <index> [left|middle]")); return true;
      }
      const Qt::MouseButton button = buttonName == QLatin1String("middle") ? Qt::MiddleButton : Qt::LeftButton;
      for (const bool press : {true, false}) queue([this, tabs, point, button, press, line] {
        if (!liveBar(tabs)) { fail(line, QStringLiteral("document tab bar disappeared during its click")); return; }
        QMouseEvent event(press ? QEvent::MouseButtonPress : QEvent::MouseButtonRelease,
                          QPointF(point), QPointF(tabs->mapToGlobal(point)), button,
                          press ? Qt::MouseButtons(button) : Qt::NoButton, Qt::NoModifier);
        QCoreApplication::sendEvent(tabs, &event);
      });
    }
  } else if (name == QLatin1String("document-tab-drag")) {
    int from = -1, to = -1;
    if (!indexOf(a.at(0), from) || !indexOf(a.at(1), to)) return true;
    if (from == to) { fail(line, QStringLiteral("document tab reorder requires different indices")); return true; }
    const QPoint start = tabs->tabRect(from).center();
    const QPoint end = tabs->tabRect(to).center();
    for (int step = 0; step <= 7; ++step) queue([this, tabs, start, end, step, line] {
      if (!liveBar(tabs)) { fail(line, QStringLiteral("document tab bar disappeared during its drag")); return; }
      const QPoint point = step == 7 ? end : start + (end - start) * step / 6;
      const auto type = step == 0 ? QEvent::MouseButtonPress : step == 7 ? QEvent::MouseButtonRelease : QEvent::MouseMove;
      QMouseEvent event(type, QPointF(point), QPointF(tabs->mapToGlobal(point)),
                        step == 0 || step == 7 ? Qt::LeftButton : Qt::NoButton,
                        step == 7 ? Qt::NoButton : Qt::LeftButton, Qt::NoModifier);
      QCoreApplication::sendEvent(tabs, &event);
    });
  } else if (name == QLatin1String("document-tab-drop")) {
    auto* document = qobject_cast<MainWindow*>(m_documentWindows.value(a.at(0)).data());
    auto* destination = qobject_cast<MainWindow*>(m_documentWindows.value(a.at(1)).data());
    auto* manager = WindowManager::instance();
    QPointer<QTabBar> destinationTabs = tabsOf(DocumentFrame::containing(destination));
    int index = -1;
    bool ok = false;
    index = a.at(2).toInt(&ok);
    if (!manager || !document || !destination || !liveBar(destinationTabs) || !ok || index < 0 || index >= destinationTabs->count()) {
      fail(line, QStringLiteral("document-tab-drop <document capture> <destination capture> <visible tab index>")); return true;
    }
    const QPoint point = destinationTabs->tabRect(index).center();
    auto mime = std::make_shared<QMimeData>();
    mime->setData("application/x-mitcad-document-window", manager->handle(document).toUtf8());
    queue([this, destinationTabs, point, mime, line] {
      if (!liveBar(destinationTabs)) { fail(line, QStringLiteral("destination tab bar disappeared")); return; }
      QDragEnterEvent enter(dragPosition<QDragEnterEvent>(point), Qt::MoveAction, mime.get(), Qt::LeftButton, Qt::NoModifier);
      QCoreApplication::sendEvent(destinationTabs, &enter);
      if (!enter.isAccepted()) { fail(line, QStringLiteral("real tab drag-enter refused the document MIME")); return; }
      QDragMoveEvent move(dragPosition<QDragMoveEvent>(point), Qt::MoveAction, mime.get(), Qt::LeftButton, Qt::NoModifier);
      QCoreApplication::sendEvent(destinationTabs, &move);
      QDropEvent drop(QPointF(point), Qt::MoveAction, mime.get(), Qt::LeftButton, Qt::NoModifier);
      QCoreApplication::sendEvent(destinationTabs, &drop);
      if (!drop.isAccepted()) fail(line, QStringLiteral("real tab drop refused the document MIME"));
    });
  } else if (name == QLatin1String("expect-document-hidden-quiet")) {
    auto* document = qobject_cast<MainWindow*>(m_documentWindows.value(a.at(0)).data());
    bool ok = false;
    const double seconds = a.at(1).toDouble(&ok);
    if (!document || !ok || !std::isfinite(seconds) || seconds < 0.2 || seconds > 5 ||
        document == current || document->viewerWidget()->documentVisible() || document->isVisible()) {
      fail(line, QStringLiteral("expect-document-hidden-quiet requires a hidden captured document and 0.2..5 seconds")); return true;
    }
    struct Observation {
      QPointer<MainWindow> document;
      QElapsedTimer clock;
      int frames = 0;
      QSet<QString> visibleCards;
      QMetaObject::Connection connection;
      void sampleCards() {
        if (!document) return;
        for (GlassCard* card : document->findChildren<GlassCard*>()) {
          if (card->isWindow() && card->isVisible()) visibleCards.insert(card->objectName());
        }
      }
    };
    auto observation = std::make_shared<Observation>();
    observation->document = document;
    observation->clock.start();
    observation->sampleCards();
    observation->connection = connect(document->viewerWidget(), &QOpenGLWidget::frameSwapped, this,
                                     [observation] { ++observation->frames; });
    whenReady([observation, seconds] {
      observation->sampleCards();
      return observation->clock.elapsed() >= qint64(seconds * 1000);
    }, seconds + 2,
              QStringLiteral("hidden document observation did not complete"), [this, observation, line] {
      disconnect(observation->connection);
      if (!observation->document || observation->frames != 0 || observation->document->viewerWidget()->documentVisible() ||
          observation->document->isVisible()) {
        fail(line, QStringLiteral("hidden document rendered %1 actual frameSwapped events").arg(observation->frames));
      } else if (!observation->visibleCards.isEmpty()) {
        fail(line, QStringLiteral("hidden document showed native card windows: %1")
                       .arg(QStringList(observation->visibleCards.values()).join(QStringLiteral(", "))));
      } else qInfo().noquote() << QStringLiteral("TestDriver: hidden document quiet: %1 ms, 0 frameSwapped events, 0 visible native card windows")
                                     .arg(observation->clock.elapsed());
    });
  } else return false;
  return true;
}
} // namespace mitcad
