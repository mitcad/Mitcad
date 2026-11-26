// SPDX-License-Identifier: MIT
// The timeline's History list in the window (mitcad#2427, GitHub
// mitcad/Mitcad#581; docs/user-guide.md "The History list"):
// Preferences › Display › Timeline shows the timeline as the strip at the
// bottom (the default) or as the History list in the left column under the
// Browser. The panel that holds the timeline's place (the timeline, or the
// sketch's items while a sketch is edited) moves with it: Docked, the
// "timelineDock" dock is split under the Browser's dock in the Browser's
// column (the split dragged and kept with the docks' arrangement, its View ›
// Panels entry and close button as before) and back to the bottom; Floating,
// the "timelineDock" card becomes a card of the left column under the
// Browser's, with its title and chevron, as wide as the Browser's card and
// sharing the column's height (the column's dragged split kept under
// floating/leftSplit), and back to the bottom bar. TimelineWidget shows the
// list or the strip (TimelineWidget::setPresentation).
#include "../MainWindow.hpp"

#include <algorithm>

#include <QDockWidget>
#include <QMargins>
#include <QPointer>
#include <QStackedWidget>
#include <QtLogging>

#include "../framework/FloatingLayout.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/TestSync.hpp"
#include "../sketch/SketchController.hpp"
#include "../sketch/SketchItemList.hpp"
#include "BrowserBackground.hpp"
#include "BrowserController.hpp"
#include "BrowserPreferences.hpp"
#include "TimelineWidget.hpp"

namespace mitcad {

void MainWindow::followTimelinePresentation() {
  if (m_controller == nullptr || m_timelinePlace == nullptr) {
    return;
  }
  // Every window follows a change at once.
  connect(&BrowserPreferencesNotifier::instance(), &BrowserPreferencesNotifier::timelinePresentationChanged, this,
          [this] { applyTimelinePresentation(); });
  if (m_floating != nullptr) {
    // The card as high as the list's rows, sharing the column.
    connect(m_controller->timeline(), &TimelineWidget::listHeightChanged, m_floating, &FloatingLayout::requestLayout);
    applyTimelinePresentation();
    return;
  }
  // Docked: also once the kept arrangement of the docks is restored
  // (restoreDockArrangement places it again), after the default
  // arrangement was taken with the dock at the bottom.
  TestSync::singleShot(0, this, [this] { applyTimelinePresentation(); });
}

int MainWindow::timelineCardContentHeight() const {
  if (m_timelinePlace == nullptr || m_controller == nullptr) {
    return 0;
  }
  if (m_sketchItems != nullptr && m_timelinePlace->currentWidget() == m_sketchItems) {
    // The sketch's items in rows as wide as the card.
    const QSize grid = m_sketchItems->gridSize();
    const int perRow = std::max(1, m_sketchItems->viewport()->width() / std::max(1, grid.width()));
    const int rows = std::max(1, (m_sketchItems->count() + perRow - 1) / perRow);
    return rows * grid.height() + 4;
  }
  return m_controller->timeline()->listContentHeight();
}

void MainWindow::applyTimelinePresentation() {
  if (m_controller == nullptr || m_timelinePlace == nullptr) {
    return;
  }
  const bool list = timelinePresentation() == TimelinePresentation::HistoryList;
  m_controller->timeline()->setPresentation(list ? TimelineWidget::Presentation::List
                                                 : TimelineWidget::Presentation::Strip);
  QString panel;
  if (m_floating != nullptr && m_timelineCard != nullptr) {
    GlassCard* card = m_timelineCard;
    // Moved only when it is not where the choice puts it.
    const bool moved = card->property("timelineHistoryList").toBool() != list;
    card->setProperty("timelineHistoryList", list);
    if (moved && list) {
      // A card of the left column under the Browser's: its title and
      // chevron, the Browser card's margins.
      card->setCapsule(false);
      card->setTitle(tr("Timeline"));
      card->setCollapsible(true);
      card->setContentMargins(QMargins(6, 0, 6, 8));
      FloatingLayout::Sizing sizing;
      const int remembered = savedBrowserCardWidth();
      sizing.width = remembered > 0 ? remembered : kBrowserCardWidth;
      sizing.contentHeight = [this] { return timelineCardContentHeight(); };
      // As wide as the Browser's card (its width follows its rows or the
      // user's drag).
      if (auto* browser = findChild<GlassCard*>(QStringLiteral("browserDock"))) {
        sizing.widthFor = [browser = QPointer<GlassCard>(browser)](int) {
          return browser != nullptr && browser->isShown() ? browser->slot().width() : 0;
        };
      }
      m_floating->setCardAnchor(card, FloatingLayout::Anchor::TopLeft, sizing);
    } else if (moved) {
      // The bottom bar again.
      card->setCollapsed(false);
      card->setCollapsible(false);
      card->setTitle(QString());
      card->setCapsule(true);
      card->setContentMargins(GlassCard::barMargins());
      m_floating->setCardAnchor(card, FloatingLayout::Anchor::Bottom, FloatingLayout::Sizing());
    }
    panel = list ? QStringLiteral("history list under the Browser") : QStringLiteral("strip at the bottom");
    if (moved) {
      sizeTimelinePlace(m_sketch->isActive());
    }
  } else if (m_floating == nullptr) {
    auto* dock = findChild<QDockWidget*>(QStringLiteral("timelineDock"), Qt::FindDirectChildrenOnly);
    auto* browser = findChild<QDockWidget*>(QStringLiteral("browserDock"), Qt::FindDirectChildrenOnly);
    if (dock == nullptr || browser == nullptr) {
      return;
    }
    const Qt::DockWidgetArea browserArea = dockWidgetArea(browser);
    const Qt::DockWidgetArea area = dockWidgetArea(dock);
    // A floating dock stays where the user put it; a hidden one (the Start
    // page, a drawing, its close button) stays hidden where it goes.
    const bool hidden = dock->isHidden();
    if (!dock->isFloating() && list && browserArea != Qt::NoDockWidgetArea && area != browserArea) {
      // Under the Browser, in its column: two fifths of it.
      addDockWidget(browserArea, dock);
      splitDockWidget(browser, dock, Qt::Vertical);
      dock->setHidden(hidden);
      TestSync::singleShot(0, this, [this, dock = QPointer<QDockWidget>(dock), browser = QPointer<QDockWidget>(browser)] {
        if (dock == nullptr || browser == nullptr || dock->isHidden() || browser->isHidden() ||
            dockWidgetArea(dock) != dockWidgetArea(browser)) {
          return;
        }
        const int total = browser->height() + dock->height();
        resizeDocks({browser.data(), dock.data()}, {total - total * 2 / 5, total * 2 / 5}, Qt::Vertical);
      });
    } else if (!dock->isFloating() && !list && area == browserArea) {
      addDockWidget(Qt::BottomDockWidgetArea, dock);
      dock->setHidden(hidden);
      // As high as the strip again, not as the column's share it had.
      TestSync::singleShot(0, this, [this, dock = QPointer<QDockWidget>(dock)] {
        if (dock != nullptr && !dock->isHidden() && dockWidgetArea(dock) == Qt::BottomDockWidgetArea) {
          resizeDocks({dock.data()}, {dock->sizeHint().height()}, Qt::Vertical);
        }
      });
    }
    panel = list ? QStringLiteral("history list under the Browser") : QStringLiteral("strip at the bottom");
  }
  const QString logged = QStringLiteral("Timeline panel: %1").arg(panel);
  if (!panel.isEmpty() && logged != m_loggedTimelinePanel) {
    m_loggedTimelinePanel = logged;
    qDebug().noquote() << logged;
  }
}

} // namespace mitcad
