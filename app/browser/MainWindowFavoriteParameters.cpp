// SPDX-License-Identifier: MIT
// The Favorite Parameters panel in the window (mitcad#2203, GitHub
// mitcad/Mitcad#547; app/COMMANDS.md "Favorite Parameters"): MANAGE ›
// Parameters › Favorite Parameters shows or hides it, and so do its own ×
// (the card's header, the dock's title); the choice is kept per user
// (favoriteParameters/shown). Floating: a card in the left column between
// the Configuration card and the browser's (its collapsed state kept),
// sharing the column's height as they do; its header has the chevron, the
// title, a button that opens the Parameters window and ×. Docked: a
// movable dock of its own above the browser's. The content is
// browser/FavoriteParametersCard, which follows the model.
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
#include <QSignalBlocker>
#include <QTimer>
#include <QToolButton>
#include <QtLogging>

#include "../framework/CommandRegistry.hpp"
#include "../framework/FloatingLayout.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/Icons.hpp"
#include "../framework/PlaceWatcher.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/UiSizes.hpp"
#include "BrowserController.hpp"
#include "FavoriteParametersCard.hpp"

namespace mitcad {
namespace {

const QString kCommandId = QStringLiteral("manage.favorite_parameters");
const QString kPanelName = QStringLiteral("favoriteParametersDock");
// The user's choice: shown or hidden at the next start (hidden until
// chosen), and the Floating card's collapsed state.
const QString kShownKey = QStringLiteral("favoriteParameters/shown");
const QString kCollapsedKey = QStringLiteral("favoriteParameters/collapsed");

// The card's rows in a scroll area of their own: the panel scrolls when
// it is lower than its rows. `transparent`: on a card's material.
QScrollArea* favoritesScroll(FavoriteParametersCard* content, bool transparent) {
  auto* scroll = new QScrollArea;
  scroll->setObjectName(QStringLiteral("favoriteParametersScroll"));
  scroll->setWidgetResizable(true);
  scroll->setFrameShape(QFrame::NoFrame);
  scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  scroll->setWidget(content);
  content->show();
  if (transparent) {
    scroll->setAutoFillBackground(false);
    scroll->viewport()->setAutoFillBackground(false);
    content->setAutoFillBackground(false);
  }
  return scroll;
}

// A small flat button of the card's header (docs/ui-conventions.md,
// "Icons": the close mark at 16, a header's other buttons at 18, on the
// cards' 24 px flat button).
QToolButton* headerButton(const QString& name, const QString& icon, int iconSize, const QString& tip) {
  auto* button = new QToolButton;
  button->setObjectName(name);
  setFlatCardButton(button);
  button->setToolButtonStyle(Qt::ToolButtonIconOnly);
  button->setIcon(themeIcon(icon));
  button->setIconSize(uiIconSize(iconSize, iconSize));
  button->setFixedSize(uiRoomSize(24), uiRoomSize(24));
  button->setToolTip(tip);
  button->setAccessibleName(tip);
  button->setFocusPolicy(Qt::NoFocus);
  return button;
}

// "<what> at x,y" (main window coordinates) when it moved, for UI tests.
void logPlace(QWidget* widget, const QString& what) {
  new PlaceWatcher(widget, [widget, what, logged = QString()]() mutable {
    if (!widget->isVisible()) {
      return;
    }
    const QPoint at = GlassCard::mapToHost(widget, widget->rect().center());
    const QString line = QStringLiteral("%1 at %2,%3").arg(what).arg(at.x()).arg(at.y());
    if (line != logged) {
      logged = line;
      qDebug().noquote() << line;
    }
  });
}

// Calls `closed` when the dock is closed by its own button (not when the
// window hides it: a page over the view, a drawing).
class DockClosed : public QObject {
public:
  DockClosed(QDockWidget* dock, std::function<void()> closed) : QObject(dock), m_closed(std::move(closed)) {
    dock->installEventFilter(this);
  }

protected:
  bool eventFilter(QObject* watched, QEvent* event) override {
    if (event->type() == QEvent::Close) {
      m_closed();
    }
    return QObject::eventFilter(watched, event);
  }

private:
  std::function<void()> m_closed;
};

} // namespace

void MainWindow::registerFavoriteParametersCommands() {
  CommandDef panel;
  panel.id = kCommandId;
  panel.name = tr("Favorite Parameters");
  panel.icon = QStringLiteral("favorite-parameters");
  panel.tooltip = tr("Shows or hides a card that edits the starred parameters' values");
  panel.kind = CommandDef::Kind::Action;
  panel.mode = CommandDef::Mode::Any;
  panel.duringCommands = true;
  panel.keywords = {QStringLiteral("favorite"), QStringLiteral("favourite"), QStringLiteral("star"),
                    QStringLiteral("parameters"), QStringLiteral("slider"), QStringLiteral("quick edit")};
  panel.run = [this] { setFavoriteParametersShown(!m_favoriteParametersShown); };
  m_registry->add(panel);
}

bool MainWindow::favoriteParametersShownAtStart() {
  const QVariant chosen = QSettings().value(kShownKey);
  const bool shown = chosen.isValid() && chosen.toBool();
  qInfo().noquote() << QStringLiteral("Favorite parameters panel at the start: %1")
                           .arg(!chosen.isValid() ? QStringLiteral("hidden (no choice yet)")
                                : shown           ? QStringLiteral("shown (as chosen)")
                                                  : QStringLiteral("hidden (as chosen)"));
  return shown;
}

void MainWindow::followFavoriteParametersAction(bool shown) {
  m_favoriteParametersShown = shown;
  if (QAction* action = m_registry->action(kCommandId)) {
    const QSignalBlocker blocker(action);
    action->setCheckable(true);
    action->setChecked(shown);
  }
}

void MainWindow::addFavoriteParametersCard() {
  FavoriteParametersCard* content = m_controller != nullptr ? m_controller->favoriteParameters() : nullptr;
  if (content == nullptr || m_floating == nullptr) {
    return;
  }
  auto* card = new GlassCard;
  card->setObjectName(kPanelName);
  card->setTitle(tr("Favorite Parameters"));
  card->setCollapsible(true);
  QToolButton* window = headerButton(QStringLiteral("favoriteParametersWindow"), QStringLiteral("parameters"), 18,
                                     tr("Open the Parameters window"));
  connect(window, &QToolButton::clicked, this, [this] {
    qDebug().noquote() << QStringLiteral("Favorite parameters: the Parameters window from the header");
    trigger(QStringLiteral("solid.parameters"));
  });
  card->addHeaderWidget(window);
  QToolButton* close =
      headerButton(QStringLiteral("favoriteParametersClose"), QStringLiteral("dismiss"), 16, tr("Hide the card"));
  connect(close, &QToolButton::clicked, this, [this] {
    qDebug().noquote() << QStringLiteral("Favorite parameters: closed by its button");
    setFavoriteParametersShown(false);
  });
  card->addHeaderWidget(close);
  card->setContent(favoritesScroll(content, true), QMargins(8, 0, 8, 8));
  // Collapsed as the user left it.
  if (QSettings().value(kCollapsedKey, false).toBool()) {
    card->setCollapsed(true);
    qInfo().noquote() << QStringLiteral("Favorite parameters panel: collapsed (remembered)");
  }
  connect(card, &GlassCard::collapsedChanged, this, [](bool now) {
    QSettings settings;
    if (settings.value(kCollapsedKey, false).toBool() != now) {
      settings.setValue(kCollapsedKey, now);
      qInfo().noquote() << QStringLiteral("Favorite parameters panel: %1, remembered")
                               .arg(now ? QStringLiteral("collapsed") : QStringLiteral("expanded"));
    }
  });
  if (auto* chevron = card->findChild<QToolButton*>(QStringLiteral("cardChevron"))) {
    logPlace(chevron, QStringLiteral("Favorite parameters chevron"));
  }
  logPlace(window, QStringLiteral("Favorite parameters window"));
  logPlace(close, QStringLiteral("Favorite parameters close"));
  connect(content, &FavoriteParametersCard::openParametersRequested, this,
          [this] { trigger(QStringLiteral("solid.parameters")); });
  const bool shown = favoriteParametersShownAtStart();
  // Below the Configuration card and above the browser's (FloatingLayout
  // stacks the upper left cards in the order they were added), as high as
  // its rows, sharing the column's height with them.
  m_floating->addCard(card, FloatingLayout::Anchor::TopLeft,
                      {250, [content] { return content->contentHeight(); }, 1.0, shown});
  m_floating->watchContent(content);
  connect(content, &FavoriteParametersCard::contentChanged, m_floating, &FloatingLayout::requestLayout);
  m_favoriteParametersContainer = card;
  followFavoriteParametersAction(shown);
}

void MainWindow::dockFavoriteParametersPanel(QDockWidget* browserDock) {
  FavoriteParametersCard* content = m_controller != nullptr ? m_controller->favoriteParameters() : nullptr;
  if (content == nullptr || browserDock == nullptr) {
    return;
  }
  auto* dock = new QDockWidget(tr("Favorite Parameters"), this);
  dock->setObjectName(kPanelName);
  // Movable, and closed by its own × (the user's choice, as MANAGE's
  // toggle); not in the window's menu of its panels, whose entry would
  // be a second toggle besides MANAGE's.
  dock->setFeatures(QDockWidget::DockWidgetMovable | QDockWidget::DockWidgetFloatable |
                    QDockWidget::DockWidgetClosable);
  dock->toggleViewAction()->setVisible(false);
  dock->setWidget(favoritesScroll(content, false));
  addDockWidget(Qt::LeftDockWidgetArea, dock);
  // Above the browser, below the Configuration dock (made before it), in
  // the same column: the view keeps its width.
  splitDockWidget(dock, browserDock, Qt::Vertical);
  m_favoriteParametersContainer = dock;
  connect(content, &FavoriteParametersCard::openParametersRequested, this,
          [this] { trigger(QStringLiteral("solid.parameters")); });
  // As high as its rows (with the dock's title), at most half of what it
  // shares with the browser, when it shows and when its rows change.
  const auto fitted = std::make_shared<int>(-1);
  const auto fit = [this, dock, browserDock, content, fitted] {
    if (dock->isHidden() || dock->isFloating() || browserDock->isHidden() || dock->widget() == nullptr ||
        dockWidgetArea(dock) != dockWidgetArea(browserDock)) {
      return;
    }
    const int hint = content->contentHeight();
    if (hint == *fitted) {
      return;
    }
    *fitted = hint;
    const int total = dock->height() + browserDock->height();
    const int title = std::max(0, dock->height() - dock->widget()->height());
    const int wanted = std::min(total / 2, hint + title + 2);
    resizeDocks({dock, browserDock}, {wanted, total - wanted}, Qt::Vertical);
  };
  connect(content, &FavoriteParametersCard::contentChanged, dock, [dock, fit] { QTimer::singleShot(0, dock, fit); });
  connect(dock, &QDockWidget::visibilityChanged, dock, [dock, fit, fitted](bool visible) {
    if (visible) {
      *fitted = -1;
      QTimer::singleShot(0, dock, fit);
    }
  });
  new DockClosed(dock, [this] {
    if (isVisible()) {
      qDebug().noquote() << QStringLiteral("Favorite parameters: closed by its button");
      setFavoriteParametersShown(false);
    }
  });
  const bool shown = favoriteParametersShownAtStart();
  dock->setVisible(shown);
  followFavoriteParametersAction(shown);
}

void MainWindow::setFavoriteParametersShown(bool shown) {
  QSettings settings;
  if (settings.value(kShownKey).toBool() != shown || !settings.contains(kShownKey)) {
    settings.setValue(kShownKey, shown);
    qInfo().noquote() << QStringLiteral("Favorite parameters panel: %1 at the next start")
                             .arg(shown ? QStringLiteral("shown") : QStringLiteral("hidden"));
  }
  followFavoriteParametersAction(shown);
  if (m_favoriteParametersContainer == nullptr) {
    return;
  }
  if (auto* card = qobject_cast<GlassCard*>(m_favoriteParametersContainer.data())) {
    card->setVisible(shown);
    if (shown && card->isCollapsed()) {
      card->setCollapsed(false); // asked for: its rows in sight
    }
    if (m_floating != nullptr) {
      m_floating->requestLayout();
    }
  } else if (auto* dock = qobject_cast<QDockWidget*>(m_favoriteParametersContainer.data())) {
    if (shown) {
      // Not over a page that hides the docks (the Start page, Render, a
      // drawing): shown again with them.
      showDocksAgain({kPanelName});
    } else {
      m_startHiddenDocks.removeAll(kPanelName);
      m_renderHiddenDocks.removeAll(kPanelName);
      m_drawingHiddenDocks.removeAll(kPanelName);
      dock->hide();
    }
  }
  qInfo().noquote() << QStringLiteral("Favorite parameters panel: %1")
                           .arg(shown ? QStringLiteral("shown") : QStringLiteral("hidden"));
  if (shown && m_controller != nullptr) {
    m_controller->favoriteParameters()->refresh();
  }
}

} // namespace mitcad
