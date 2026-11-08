// SPDX-License-Identifier: MIT
// The Configuration panel in the window (mitcad#1020): the Configuration
// bar (mitcad#807, browser/ConfigurationBar) in a left panel of its own,
// between the Projects panel and the browser, instead of at the top of the
// browser: a card in the Floating layout, a dock in the Docked one. It is
// shown while the design has options or rows, and scrolls inside when the
// left column gives it less than its height. The AppKit frontend presents
// the same bar (MainWindow::refreshNativeWindow).
#include "../MainWindow.hpp"

#include <algorithm>
#include <functional>
#include <memory>
#include <utility>

#include <QAction>
#include <QDockWidget>
#include <QEvent>
#include <QFrame>
#include <QMargins>
#include <QScrollArea>
#include <QSettings>
#include <QTimer>
#include <QToolButton>
#include <QtLogging>

#include "../framework/FloatingLayout.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/PlaceWatcher.hpp"
#include "BrowserController.hpp"
#include "ConfigurationBar.hpp"

namespace mitcad {
namespace {

// The settings key of the Floating card's collapsed state.
const QString kCollapsedKey = QStringLiteral("configuration/panelCollapsed");

// The bar in a scroll area of its own: the panel scrolls when it is lower
// than the bar. `transparent`: on a card's material.
QScrollArea* configurationScroll(ConfigurationBar* bar, bool transparent) {
  auto* scroll = new QScrollArea;
  scroll->setObjectName(QStringLiteral("configurationScroll"));
  scroll->setWidgetResizable(true);
  scroll->setFrameShape(QFrame::NoFrame);
  scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  // setWidget shows the bar and fills its background: it stays hidden
  // while the design has no options, on the card's material.
  scroll->setWidget(bar);
  bar->setVisible(bar->isPresent());
  if (transparent) {
    scroll->setAutoFillBackground(false);
    scroll->viewport()->setAutoFillBackground(false);
    bar->setAutoFillBackground(false);
  }
  return scroll;
}

// Calls `changed` when the widget's layout asks for another size (the
// Docked panel follows the bar's height).
class LayoutFollower : public QObject {
public:
  LayoutFollower(QWidget* widget, std::function<void()> changed) : QObject(widget), m_changed(std::move(changed)) {
    widget->installEventFilter(this);
  }

protected:
  bool eventFilter(QObject* watched, QEvent* event) override {
    if (event->type() == QEvent::LayoutRequest) {
      m_changed();
    }
    return QObject::eventFilter(watched, event);
  }

private:
  std::function<void()> m_changed;
};

void logShown(bool present) {
  qInfo().noquote() << QStringLiteral("Configuration panel: %1")
                           .arg(present ? QStringLiteral("shown") : QStringLiteral("hidden"));
}

} // namespace

void MainWindow::addConfigurationCard() {
  ConfigurationBar* bar = m_controller != nullptr ? m_controller->configurationBar() : nullptr;
  if (bar == nullptr || m_floating == nullptr) {
    return;
  }
  // The card's header reads "Configuration": the row combo's caption is
  // "Row".
  bar->setHeaderTitled(true);
  auto* card = new GlassCard;
  card->setObjectName(QStringLiteral("configurationDock"));
  card->setTitle(tr("Configuration"));
  card->setCollapsible(true);
  card->setContent(configurationScroll(bar, true), QMargins(6, 0, 6, 8));
  // Collapsed as the user left it.
  const bool collapsed = QSettings().value(kCollapsedKey, false).toBool();
  if (collapsed) {
    card->setCollapsed(true);
    qInfo().noquote() << QStringLiteral("Configuration panel: collapsed (remembered)");
  }
  // Where its chevron is, for UI tests: "Configuration panel chevron at
  // x,y" (main window coordinates) when it moved.
  if (auto* chevron = card->findChild<QToolButton*>(QStringLiteral("cardChevron"))) {
    new PlaceWatcher(chevron, [chevron, logged = QString()]() mutable {
      if (!chevron->isVisible()) {
        return;
      }
      const QPoint at = GlassCard::mapToHost(chevron, chevron->rect().center());
      const QString line = QStringLiteral("Configuration panel chevron at %1,%2").arg(at.x()).arg(at.y());
      if (line != logged) {
        logged = line;
        qDebug().noquote() << line;
      }
    });
  }
  connect(card, &GlassCard::collapsedChanged, this, [](bool now) {
    QSettings settings;
    if (settings.value(kCollapsedKey, false).toBool() != now) {
      settings.setValue(kCollapsedKey, now);
      qInfo().noquote() << QStringLiteral("Configuration panel: %1, remembered")
                               .arg(now ? QStringLiteral("collapsed") : QStringLiteral("expanded"));
    }
  });
  // Between the Projects card and the browser's (FloatingLayout stacks the
  // upper left cards in the order they were added), as high as the bar.
  m_floating->addCard(card, FloatingLayout::Anchor::TopLeft,
                      {260, [bar] { return bar->isHidden() ? 0 : bar->sizeHint().height(); }, 1.0,
                       bar->isPresent()});
  m_floating->watchContent(bar);
  connect(bar, &ConfigurationBar::presenceChanged, card, [card](bool present) {
    card->setVisible(present);
    logShown(present);
  });
  m_configurationContainer = card;
}

QDockWidget* MainWindow::dockConfigurationPanel(QDockWidget* browserDock) {
  ConfigurationBar* bar = m_controller != nullptr ? m_controller->configurationBar() : nullptr;
  if (bar == nullptr) {
    return browserDock;
  }
  bar->setHeaderTitled(true);
  auto* dock = new QDockWidget(tr("Configuration"), this);
  dock->setObjectName(QStringLiteral("configurationDock"));
  // Shown with the design's options, not closed by hand (no View menu
  // entry brings it back).
  dock->setFeatures(QDockWidget::DockWidgetMovable | QDockWidget::DockWidgetFloatable);
  // Nor from the window's menu of its panels (a right-click on a dock's
  // title or the toolbar), which would hide it with nothing to bring it
  // back while the design has options (mitcad#1455).
  dock->toggleViewAction()->setVisible(false);
  dock->setWidget(configurationScroll(bar, false));
  addDockWidget(Qt::LeftDockWidgetArea, dock);
  // Above the browser and below the Projects dock (split before), in the
  // same column: the view keeps its width. The split puts the browser's
  // dock right after this one. Hidden by createPanels until the design has
  // options, once the column is made.
  splitDockWidget(dock, browserDock, Qt::Vertical);
  m_configurationContainer = dock;
  // As high as the bar (with the dock's title), at most half of what it
  // shares with the browser: when it shows, and again when the bar's
  // height changes (an option added or taken away, a notice), not when
  // only the user's drag of the separator changed the dock.
  const auto fitted = std::make_shared<int>(-1); // the bar's height the dock was fitted to
  const auto fit = [this, dock, browserDock, bar, fitted] {
    if (dock->isHidden() || dock->isFloating() || browserDock->isHidden() || dock->widget() == nullptr) {
      return;
    }
    const int hint = bar->sizeHint().height();
    if (hint == *fitted) {
      return;
    }
    *fitted = hint;
    const int total = dock->height() + browserDock->height();
    const int title = std::max(0, dock->height() - dock->widget()->height());
    const int wanted = std::min(total / 2, hint + title + 2);
    resizeDocks({dock, browserDock}, {wanted, total - wanted}, Qt::Vertical);
  };
  new LayoutFollower(bar, [dock, fit] { QTimer::singleShot(0, dock, fit); });
  connect(bar, &ConfigurationBar::presenceChanged, dock, [dock, fit, fitted](bool present) {
    dock->setVisible(present);
    logShown(present);
    *fitted = -1;
    if (present) {
      QTimer::singleShot(0, dock, fit);
    }
  });
  return dock;
}

} // namespace mitcad
