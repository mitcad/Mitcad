// SPDX-License-Identifier: MIT
// The navigation bar at the bottom of the view (mitcad#1086,
// view/NavigationBar): Look At, Fit, Visual Style, Layout Grid, Snap to
// Grid and Comment Pins, the View menu's own actions, and after a
// separator the commands the user pinned to it (mitcad#1589: from a group
// menu row's or a pinned toolbar button's Pin to Navigation Bar; Edit
// Parameters by default, mitcad#1582). In the Floating
// layout a capsule card at the bottom centre of the view, above the timeline
// (FloatingLayout::Anchor::BottomCenter); in the Docked layout in the
// status bar. It shows in every workspace and in sketches; Preferences ›
// Display › Show the navigation bar hides it. The AppKit frontend shows
// the same commands in a bar of its own (createNativeNavigationBar,
// mitcad#1099).
#include "MainWindow.hpp"

#include <QMargins>
#include <QStatusBar>

#include "framework/ChromeStyle.hpp"
#include "framework/FloatingLayout.hpp"
#include "framework/GlassCard.hpp"
#include "framework/Ribbon.hpp"
#include "view/NavigationBar.hpp"
#include "view/ViewController.hpp"

#ifdef MITCAD_MACOS_NATIVE_UI
#include <QAction>
#include <QStringList>
#include <QTimer>

#include <utility>

#include "framework/CommandRegistry.hpp"
#include "framework/Icons.hpp"
#include "platform/NativeMacWindow.hpp"
#endif

namespace mitcad {

void MainWindow::createNavigationBar() {
  if (m_nativeUi || m_viewController == nullptr) {
    return;
  }
  const QList<QAction*> styles = m_viewController->visualStyleActions();
  if (chromeStyle() == ChromeStyle::Floating && m_floating != nullptr) {
    m_navigationBar = new NavigationBar(*m_registry, styles, NavigationBar::Presentation::Card);
    m_navigationCard = new GlassCard;
    m_navigationCard->setObjectName(QStringLiteral("navigationBar"));
    m_navigationCard->setCapsule(true);
    m_navigationCard->setContent(m_navigationBar, QMargins(10, 3, 10, 3));
    m_floating->addCard(m_navigationCard, FloatingLayout::Anchor::BottomCenter);
    m_navigationBar->setHolder(m_navigationCard);
  } else {
    m_navigationBar = new NavigationBar(*m_registry, styles, NavigationBar::Presentation::StatusBar, this);
    // Left of the failed features' and the warnings' badges and the
    // indicators, at the right of the status line.
    statusBar()->insertPermanentWidget(0, m_navigationBar);
  }
  m_viewController->navigationBarChanged = [this](bool) { updateNavigationBar(); };
  // Pinned from the ribbon's menus (mitcad#1589); a pinned button's Change
  // Keyboard Shortcut… opens Tools > Keyboard Shortcuts at its command.
  if (m_ribbon != nullptr) {
    m_ribbon->setNavigationBarPins(
        [this](const QString& id) { return m_navigationBar != nullptr && m_navigationBar->isPinned(id); });
    connect(m_ribbon, &Ribbon::navigationBarPinRequested, this, [this](const QString& id, bool wanted) {
      if (m_navigationBar == nullptr) {
        return;
      }
      if (wanted) {
        m_navigationBar->pin(id);
      } else {
        m_navigationBar->unpin(id);
      }
    });
  }
  connect(m_navigationBar, &NavigationBar::shortcutRequested, this, [this](const QString& id) { editShortcuts(id); });
  updateNavigationBar();
}

void MainWindow::updateNavigationBar() {
  if (m_navigationBar == nullptr) {
    return;
  }
  const bool wanted = m_viewController->settings().navigationBar;
  if (m_navigationCard != nullptr) {
    // A drawing hides it with the other cards (FloatingLayout::showOnlyCards).
    m_navigationCard->setVisible(wanted);
    m_floating->requestLayout();
  } else {
    // Nor while the Start page covers the view (mitcad#1258).
    m_navigationBar->setVisible(wanted && !drawingOpen() && !startPageShown());
  }
}

#ifdef MITCAD_MACOS_NATIVE_UI
// The AppKit window's navigation bar (mitcad#1099): the same commands at
// the bottom centre of the view, their artwork, checks and tooltips from
// the native action snapshot, a click the command as its View menu item
// runs it. A change of their actions made outside the native window (the
// Sketch Palette's Snap to grid) refreshes the snapshot, so that the bar
// and the View menu stay in step; Preferences shows and hides it.
void MainWindow::createNativeNavigationBar() {
  if (!m_nativeWindow || m_viewController == nullptr) {
    return;
  }
  auto* refresh = new QTimer(this);
  refresh->setSingleShot(true);
  refresh->setInterval(0);
  connect(refresh, &QTimer::timeout, this, [this] { refreshNativeActions(); });
  const auto follow = [refresh](QAction* action) {
    if (action != nullptr) {
      connect(action, &QAction::changed, refresh, qOverload<>(&QTimer::start));
    }
  };
  mac::NativeMacNavigationBar bar;
  bar.style.id = QStringLiteral("view.visual_style"); // the Qt bar's button
  bar.style.label = tr("Visual Style");
  bar.style.icon = themeIcon(QStringLiteral("visual-style"));
  bar.commands = {QStringLiteral("view.look_at"), QStringLiteral("view.fit"), bar.style.id,
                  QStringLiteral("view.grid"), QStringLiteral("view.snap_grid"),
                  QStringLiteral("view.comment_pins")};
  for (const QString& id : std::as_const(bar.commands)) {
    follow(m_registry->action(id));
  }
  for (QAction* style : m_viewController->visualStyleActions()) {
    if (style == nullptr) {
      bar.styles << QString(); // a separator
      continue;
    }
    if (style->objectName().startsWith(QStringLiteral("command_"))) {
      bar.styles << style->objectName().mid(8);
      follow(style);
    }
  }
  const auto show = [this, bar] {
    mac::NativeMacNavigationBar shown = bar;
    shown.shown = m_viewController->settings().navigationBar;
    m_nativeWindow->setNavigationBar(shown);
  };
  m_viewController->navigationBarChanged = [show](bool) { show(); };
  show();
}
#endif

} // namespace mitcad
