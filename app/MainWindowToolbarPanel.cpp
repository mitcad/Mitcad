// SPDX-License-Identifier: MIT
// View › Toolbar Panel (mitcad#2426, GitHub mitcad/Mitcad#571; app/COMMANDS.md,
// "Toolbar panel hidden"): the toolbar panel hidden for more room for the
// model, in both layouts, and shown for a while by a rest of the pointer at
// the view's top edge.
//   - One application setting, view/toolbarPanelHidden: every window and
//     design follows it, new ones too, and it is kept between starts.
//   - Floating: the panel's row of the FloatingLayout hidden; the cards and
//     the view's free area move up (FloatingLayout::setRowHidden). Docked:
//     the panel under the top bar hidden; the view takes its height. The top
//     bar (☰, the quick-access group, the document tabs) stays.
//   - The reveal (ToolbarPanelReveal): a band a few pixels high under the
//     top bar; a rest on it shows the panel over the view, which keeps its
//     place (Floating: the row over the cards; Docked: the panel taken out
//     of the top bar and placed over the view's top), until the pointer has
//     left it and nothing opened from it is open: a command started from it
//     keeps it while its command panel is open (mitcad#2472,
//     keepToolbarPanelFor).
//   - The Start page hides the panel as before; leaving it gives the user's
//     state back (MainWindowStart.cpp).
// Logged:
//   Toolbar panel hidden (Ctrl+F1)          the command, with its key
//   Toolbar panel shown
//   Toolbar panel hidden (kept setting)     a window set up with it hidden
//   Toolbar panel reveal band at x,y: wxh   the band's middle, as it moves
//   Toolbar panel revealed at top edge: panel x,y wxh
//   Toolbar panel placed again over the view: panel x,y wxh   Docked, the window resized
//   Toolbar panel hidden again (the pointer left it)
//   Toolbar panel kept (command panel Measure open)
//   Toolbar panel released (command Measure ended)
//   Toolbar panel hidden again (the command ended)
#include "MainWindow.hpp"

#include <QAction>
#include <QApplication>
#include <QKeySequence>
#include <QSettings>
#include <QToolBar>
#include <QToolButton>
#include <QVBoxLayout>
#include <QtLogging>

#include <utility>

#include "OcctViewer.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/FloatingLayout.hpp"
#include "framework/NoticeLevel.hpp"
#include "framework/PlaceWatcher.hpp"
#include "framework/Ribbon.hpp"
#include "framework/TitleBar.hpp"
#include "framework/ToolbarPanelReveal.hpp"
#include "framework/WindowStyle.hpp"

namespace mitcad {
namespace {

const QString kCommandId = QStringLiteral("view.toolbar_panel");
// The setting every window follows, and whether the way back was told.
const QString kHiddenKey = QStringLiteral("view/toolbarPanelHidden");
const QString kToldKey = QStringLiteral("view/toolbarPanelHiddenTold");

bool hiddenSetting() { return QSettings().value(kHiddenKey, false).toBool(); }

// The Docked layout's top bar holder (createDockedTopBar), whose column
// holds the title row and, under it, the panel.
const QString kDockedHolder = QStringLiteral("dockedTopBar");

// The Docked panel revealed: out of the top bar, over the view's top, as
// wide as the holder, down to the gap the bar keeps below it. False when
// it was there already.
bool placeOverView(QWidget* window, const QWidget* holder, QWidget* panel) {
  const QPoint at = holder->mapTo(window, QPoint(0, holder->height()));
  const int height = panel->sizeHint().height() + panel::kGap;
  const QRect place(at, QSize(holder->width(), height));
  if (panel->geometry() == place) {
    return false;
  }
  panel->setGeometry(place);
  return true;
}

} // namespace

void MainWindow::registerToolbarPanelCommands() {
  CommandDef panel;
  panel.id = kCommandId;
  panel.name = tr("Toolbar Panel");
  panel.icon = QStringLiteral("toolbar-panel");
  panel.tooltip = tr("Shows or hides the toolbar panel; hidden, the view's top edge shows it");
  panel.kind = CommandDef::Kind::Action;
  panel.mode = CommandDef::Mode::Any;
  panel.duringCommands = true;
  // Cmd+F1 on macOS (Qt's Control is Command there).
  panel.shortcut = QKeySequence(Qt::CTRL | Qt::Key_F1);
  panel.keywords = {QStringLiteral("toolbar"), QStringLiteral("ribbon"),     QStringLiteral("panel"),
                    QStringLiteral("hide"),    QStringLiteral("show"),       QStringLiteral("room"),
                    QStringLiteral("space"),   QStringLiteral("full screen"), QStringLiteral("maximize")};
  // The AppKit frontend shows its own toolbar.
  panel.enabled = [this] { return !m_nativeUi; };
  panel.run = [this] { setToolbarPanelHiddenEverywhere(!m_toolbarPanelHidden); };
  m_registry->add(panel);
}

QWidget* MainWindow::toolbarPanelWidget() const {
  if (m_nativeUi || m_ribbon == nullptr) {
    return nullptr;
  }
  if (m_floating != nullptr && m_titleRow != nullptr) {
    return m_ribbon; // the panel's row of the Floating layout
  }
  return m_dockedPanel;
}

void MainWindow::setUpToolbarPanelHiding() {
  if (QAction* action = m_registry->action(kCommandId)) {
    action->setCheckable(true);
  }
  m_toolbarPanelHidden = hiddenSetting();
  if (toolbarPanelWidget() != nullptr) {
    // The band in the Floating layout under its title row, in the Docked
    // layout at the view's top under the top bar.
    QWidget* bandParent = m_floating != nullptr ? static_cast<QWidget*>(m_floating) : static_cast<QWidget*>(this);
    ToolbarPanelReveal::Hooks hooks;
    hooks.panel = [this] { return toolbarPanelWidget(); };
    hooks.band = [this] { return toolbarPanelBand(); };
    hooks.canReveal = [this] { return m_toolbarPanelHidden && !startPageShown() && isVisible(); };
    hooks.reveal = [this] { revealToolbarPanel(); };
    hooks.conceal = [this](const QString& how) { concealToolbarPanel(how); };
    // Docked: the panel over the view follows the window's width (made
    // narrower, its right part and its » menu were cut off until the
    // pointer left it, mitcad#2527); the Floating row is placed again by
    // every layout. Not while it takes its room in the top bar for a
    // command (keepToolbarPanelFor).
    hooks.place = [this] {
      QWidget* panel = toolbarPanelWidget();
      if ((m_floating != nullptr && m_titleRow != nullptr) || m_toolbarPanelRoom || panel == nullptr ||
          panel->parentWidget() != this) {
        return;
      }
      const auto* holder = findChild<QWidget*>(kDockedHolder);
      if (holder != nullptr && placeOverView(this, holder, panel)) {
        const QPoint at = panel->mapTo(window(), QPoint(0, 0));
        qInfo().noquote() << QStringLiteral("Toolbar panel placed again over the view: panel %1,%2 %3x%4")
                                 .arg(at.x())
                                 .arg(at.y())
                                 .arg(panel->width())
                                 .arg(panel->height());
      }
    };
    m_toolbarPanelReveal = new ToolbarPanelReveal(bandParent, std::move(hooks));
    m_toolbarPanelReveal->watch(m_viewer);
    m_toolbarPanelReveal->watch(m_titleRow);
    if (QWidget* central = centralWidget()) {
      m_toolbarPanelReveal->watch(central);
    }
    if (auto* bar = findChild<QToolBar*>(QStringLiteral("ribbonToolbar"), Qt::FindDirectChildrenOnly)) {
      m_toolbarPanelReveal->watch(bar);
    }
    // The revealed Docked panel's width and place (hooks.place).
    if (auto* holder = findChild<QWidget*>(kDockedHolder)) {
      m_toolbarPanelReveal->watch(holder);
    }
    if (m_floating == nullptr && m_dockedPanel != nullptr) {
      // The Docked top bar's line ("view top N") again once the view has
      // moved up or down for the panel, which settles after the bar.
      new PlaceWatcher(m_viewer, [this] { logDockedTopBar(); });
    }
  }
  if (m_toolbarPanelHidden) {
    qInfo().noquote() << QStringLiteral("Toolbar panel hidden (kept setting)");
  }
  applyToolbarPanelHidden();
}

void MainWindow::setToolbarPanelHiddenEverywhere(bool hidden) {
  QSettings settings;
  settings.setValue(kHiddenKey, hidden);
  const QAction* action = m_registry->action(kCommandId);
  const QKeySequence key = action != nullptr ? action->shortcut() : QKeySequence();
  if (hidden) {
    qInfo().noquote() << QStringLiteral("Toolbar panel hidden (%1)")
                             .arg(key.isEmpty() ? QStringLiteral("no key") : key.toString(QKeySequence::PortableText));
  } else {
    qInfo().noquote() << QStringLiteral("Toolbar panel shown");
  }
  // This window first, then every other one (a document frame's designs,
  // other windows).
  applyToolbarPanelHidden();
  const QList<QWidget*> widgets = QApplication::allWidgets();
  for (QWidget* widget : widgets) {
    auto* other = qobject_cast<MainWindow*>(widget);
    if (other != nullptr && other != this) {
      other->applyToolbarPanelHidden();
    }
  }
  // The way back, said once (mitcad#1451's trap: nothing brought a hidden
  // toolbar back): the message band (Floating), the status bar (Docked).
  if (hidden && !settings.value(kToldKey, false).toBool()) {
    settings.setValue(kToldKey, true);
    showStatus(key.isEmpty() ? tr("Toolbar panel hidden. Choose View › Toolbar Panel or move the pointer to the top "
                                  "edge to show it.")
                             : tr("Toolbar panel hidden. Press %1 or move the pointer to the top edge to show it.")
                                   .arg(key.toString(QKeySequence::NativeText)),
               NoticeLevel::Information);
  }
}

void MainWindow::applyToolbarPanelHidden() {
  m_toolbarPanelHidden = hiddenSetting();
  if (QAction* action = m_registry != nullptr ? m_registry->action(kCommandId) : nullptr) {
    action->setChecked(!m_toolbarPanelHidden);
  }
  // A reveal ends: the panel shown for good, hidden again, or the Start
  // page over it.
  if (m_toolbarPanelReveal != nullptr && m_toolbarPanelReveal->revealed() &&
      (!m_toolbarPanelHidden || startPageShown())) {
    m_toolbarPanelReveal->concealNow(m_toolbarPanelHidden ? QStringLiteral("the Start page")
                                                          : QStringLiteral("shown for good"));
  }
  if (m_nativeUi || m_ribbon == nullptr) {
    return;
  }
  if (!toolbarPanelRevealed()) {
    if (m_floating != nullptr && m_titleRow != nullptr) {
      m_floating->setRowHidden(m_ribbon, m_toolbarPanelHidden);
    } else if (m_header != nullptr) {
      // The macOS layout with its header above the view (MITCAD_MAC_UI,
      // for comparison): the ribbon under the title bar, no reveal.
      m_ribbon->setVisible(!m_toolbarPanelHidden);
    } else {
      updateStartToolbar(); // the Docked panel or toolbar, with the Start page
    }
  }
  if (m_toolbarPanelReveal != nullptr) {
    m_toolbarPanelReveal->updateBand();
  }
}

bool MainWindow::toolbarPanelRevealed() const {
  return m_toolbarPanelReveal != nullptr && m_toolbarPanelReveal->revealed();
}

QRect MainWindow::toolbarPanelBand() const {
  if (!m_toolbarPanelHidden || startPageShown() || toolbarPanelWidget() == nullptr) {
    return {};
  }
  if (m_floating != nullptr && m_titleRow != nullptr) {
    // Under the title row, across the view; the cards start a gap lower.
    if (m_titleRow->isHidden()) {
      return {};
    }
    const QPoint view = m_viewer->mapTo(m_floating, QPoint(0, 0));
    const int top = m_titleRow->mapTo(m_floating, QPoint(0, m_titleRow->height())).y();
    return QRect(view.x(), top, m_viewer->width(), ToolbarPanelReveal::kBandHeight);
  }
  // The Docked layout: the view's top edge, under the top bar.
  const QWidget* central = centralWidget();
  if (central == nullptr || !central->isVisible()) {
    return {};
  }
  return QRect(central->mapTo(this, QPoint(0, 0)), QSize(central->width(), ToolbarPanelReveal::kBandHeight));
}

void MainWindow::revealToolbarPanel() {
  QWidget* panel = toolbarPanelWidget();
  if (panel == nullptr) {
    return;
  }
  if (m_floating != nullptr && m_titleRow != nullptr) {
    // At its place over the view and the cards, which keep theirs.
    m_floating->setRowHidden(m_ribbon, true, true);
  } else if (auto* holder = findChild<QWidget*>(kDockedHolder)) {
    // Out of the top bar, over the view's top: the view keeps its place.
    // On the window's color, as under the top bar, down to the gap the bar
    // keeps below it; placed again as the window is resized (the reveal's
    // place hook, setUpToolbarPanelHiding).
    panel->setParent(this);
    panel->setAutoFillBackground(true);
    placeOverView(this, holder, panel);
    panel->show();
    panel->raise();
  }
  if (m_toolbarPanelReveal != nullptr && m_toolbarPanelReveal->band() != nullptr) {
    m_toolbarPanelReveal->band()->raise();
  }
  QWidget* host = window();
  const QPoint at = panel->mapTo(host, QPoint(0, 0));
  // Over everything at its buttons (the cards under it, the view): "on
  // top", else the first button covered and by what.
  QString cover = QStringLiteral("on top");
  for (const QToolButton* button : m_ribbon->findChildren<QToolButton*>()) {
    if (!button->isVisible()) {
      continue;
    }
    const QPoint middle = button->mapTo(host, button->rect().center());
    const QWidget* hit = host->childAt(middle);
    if (hit != button && !button->isAncestorOf(hit)) {
      const QWidget* named = hit;
      while (named != nullptr && named->objectName().isEmpty()) {
        named = named->parentWidget();
      }
      cover = QStringLiteral("%1 covered at %2,%3 by %4")
                  .arg(button->objectName())
                  .arg(middle.x())
                  .arg(middle.y())
                  .arg(named != nullptr ? named->objectName() : QStringLiteral("nothing"));
      break;
    }
  }
  qInfo().noquote() << QStringLiteral("Toolbar panel revealed at top edge: panel %1,%2 %3x%4, %5")
                           .arg(at.x())
                           .arg(at.y())
                           .arg(panel->width())
                           .arg(panel->height())
                           .arg(cover);
  // Its buttons' places for the UI tests (they click a command in it).
  m_ribbon->logLayout();
}

void MainWindow::keepToolbarPanelFor(CommandSession& session) {
  if (m_toolbarPanelReveal == nullptr || !m_toolbarPanelReveal->revealed()) {
    return;
  }
  m_toolbarPanelReveal->keepFor(&session, session.def().name);
  if (m_toolbarPanelReveal->kept()) {
    // Over the cards and docks it would cover the top of the command's own
    // panel (the Floating Command card under the rows, the Docked command
    // dock): while kept it takes its room as when shown, the command's
    // panel below it, until it goes.
    if (!m_toolbarPanelRoom) {
      m_toolbarPanelRoom = true;
      QWidget* panel = toolbarPanelWidget();
      if (m_floating != nullptr && m_titleRow != nullptr) {
        m_floating->setRowHidden(m_ribbon, false);
      } else if (auto* holder = findChild<QWidget*>(kDockedHolder); holder != nullptr && panel != nullptr) {
        panel->setAutoFillBackground(false);
        if (auto* column = qobject_cast<QVBoxLayout*>(holder->layout())) {
          column->insertWidget(1, panel);
        }
        panel->show();
      }
      if (m_toolbarPanelReveal->band() != nullptr) {
        m_toolbarPanelReveal->band()->raise();
      }
      qInfo().noquote() << QStringLiteral("Toolbar panel takes its room while kept (%1)")
                               .arg(m_floating != nullptr ? QStringLiteral("Floating") : QStringLiteral("Docked"));
    }
    // OK, Cancel or Esc: its end lets the panel go (the session is
    // deleted later, so it is still the owner compared here).
    ToolbarPanelReveal* reveal = m_toolbarPanelReveal;
    connect(&session, &CommandSession::finished, reveal,
            [reveal, owner = QPointer<CommandSession>(&session)] { reveal->release(owner.data()); });
  }
}

void MainWindow::concealToolbarPanel(const QString& how) {
  QWidget* panel = toolbarPanelWidget();
  if (panel == nullptr) {
    return;
  }
  // Taking its room for a command's panel (keepToolbarPanelFor): already
  // back in the top bar.
  const bool room = std::exchange(m_toolbarPanelRoom, false);
  if (m_floating != nullptr && m_titleRow != nullptr) {
    m_floating->setRowHidden(m_ribbon, m_toolbarPanelHidden);
  } else if (auto* holder = findChild<QWidget*>(kDockedHolder)) {
    // Back under the title row in the top bar, hidden or shown as the
    // setting and the Start page have it (updateStartToolbar).
    panel->hide();
    panel->setAutoFillBackground(false);
    if (auto* column = qobject_cast<QVBoxLayout*>(holder->layout()); column != nullptr && !room) {
      column->insertWidget(1, panel);
    }
    panel->hide(); // explicitly: the layout shows only what updateStartToolbar asks for
    updateStartToolbar();
  }
  qInfo().noquote() << QStringLiteral("Toolbar panel %1 (%2)")
                           .arg(m_toolbarPanelHidden ? QStringLiteral("hidden again") : QStringLiteral("back in place"),
                                how);
}

} // namespace mitcad
