// SPDX-License-Identifier: MIT
// The Render workspace (mitcad#2013, render/RenderWorkspace.hpp) in the
// main window: its stages as the Render tab's own tab row with Render and
// Cancel at its right, the stage's actions as the tab's page, its panels as
// cards over the view (Floating) or docks (Docked), shown while the Render
// tab is; the other cards and docks hidden meanwhile and back after.
#include "MainWindow.hpp"

#include <QAction>
#include <QDockWidget>
#include <QFileInfo>
#include <QHBoxLayout>
#include <QVBoxLayout>
#include <QtLogging>

#include "OcctViewer.hpp"
#include "browser/BrowserController.hpp"
#include "browser/TimelineWidget.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/FloatingLayout.hpp"
#include "framework/GlassCard.hpp"
#include "framework/Ribbon.hpp"
#include "framework/TestSync.hpp"
#include "render/RenderMode.hpp"
#include "render/RenderWorkspace.hpp"
#include "view/ViewController.hpp"

namespace mitcad {

namespace {
const QString kRenderTab = QStringLiteral("RENDER");
} // namespace

void MainWindow::createRenderWorkspace() {
  RenderWorkspace::Hooks hooks;
  hooks.renderMode = [this] { return m_viewController->renderMode(); };
  hooks.selection = [this] { return m_selection; };
  hooks.select = [this](const Selection& items) {
    if (m_mode == Mode::Idle) {
      setSelection(items);
    }
  };
  hooks.pickFilterChanged = [this] {
    if (m_mode == Mode::Idle) {
      updateIdleView();
    }
  };
  hooks.runCommand = [this](const QString& id) {
    if (QAction* action = m_registry->action(id)) {
      action->trigger();
    }
  };
  hooks.deviceChanged = [this] {
    if (RenderMode* mode = m_viewController->renderMode()) {
      mode->deviceChanged();
    }
  };
  m_renderWorkspace = new RenderWorkspace(*this, *m_viewer, std::move(hooks), this);
  // View › Render Environment… and Render Image… show their stage while
  // the workspace is open, not a window beside it.
  m_viewController->showInRenderWorkspace = [this](const QString& stage) {
    if (m_renderWorkspace == nullptr || !m_renderWorkspace->isOpen()) {
      return false;
    }
    m_renderWorkspace->showStage(stage == QLatin1String("output") ? RenderWorkspace::Stage::Output
                                                                  : RenderWorkspace::Stage::Light);
    return true;
  };
  // The stages in place of the workspace's tabs, Render and Cancel beside
  // the search; the tab's page a slim row of the stage's actions.
  m_ribbon->setTabRow(kRenderTab, m_renderWorkspace->stageBar(), m_renderWorkspace->stageButtons());
  m_ribbon->setTabContent(kRenderTab, m_renderWorkspace->stageActions());
  // The panels are placed again when another stage shows its own.
  connect(m_renderWorkspace, &RenderWorkspace::stageChanged, this, [this] {
    if (m_floating != nullptr) {
      m_floating->requestLayout();
    }
  });
  // One header each: the cards' (the docks') titles follow the panels'.
  connect(m_renderWorkspace, &RenderWorkspace::titlesChanged, this, [this] {
    const QString titles[] = {m_renderWorkspace->leftTitle(), m_renderWorkspace->rightTitle()};
    for (int i = 0; i < 2; ++i) {
      if (i < m_renderCards.size()) {
        m_renderCards[i]->setTitle(titles[i]);
      }
      if (i < m_renderDocks.size()) {
        m_renderDocks[i]->setWindowTitle(titles[i]);
      }
    }
  });
}

void MainWindow::addRenderPanels() {
  if (m_renderWorkspace == nullptr || m_nativeUi) {
    return;
  }
  const struct {
    QString name;
    QString title;
    QWidget* content;
  } parts[] = {{QStringLiteral("renderLeftPanel"), m_renderWorkspace->leftTitle(), m_renderWorkspace->leftPanel()},
               {QStringLiteral("renderSettingsPanel"), m_renderWorkspace->rightTitle(), m_renderWorkspace->rightPanel()},
               {QStringLiteral("renderBarPanel"), QString(), m_renderWorkspace->bottomBar()}};
  if (m_floating != nullptr) {
    // Cards over the view (Floating): the stage's list at the upper left,
    // the settings of what is chosen in it at the upper right, each with
    // one header (the panel's title) and as tall as the room down to the
    // Shots bar at the bottom (their pages scroll down within). Hidden
    // until the Render tab shows.
    for (const auto& part : parts) {
      auto* card = new GlassCard;
      card->setObjectName(part.name);
      card->setTitle(part.title);
      // The Shots bar (no title) is Design's timeline bar (mitcad#1990,
      // owner on test build 10): one row, as high as the timeline, with its
      // margins and corners, at the view's bottom between the same margins.
      const bool bar = part.title.isEmpty();
      card->setCapsule(bar);
      card->setContent(part.content, bar ? GlassCard::barMargins() : QMargins(8, 2, 8, 8));
      m_renderCards << card;
    }
    const auto room = [] { return 100000; }; // all there is, the layout's limit
    m_floating->addCard(m_renderCards[0], FloatingLayout::Anchor::TopLeft, {300, room, 1.0, false});
    m_floating->addCard(m_renderCards[1], FloatingLayout::Anchor::TopRight, {340, room, 1.0, false});
    m_floating->addCard(m_renderCards[2], FloatingLayout::Anchor::Bottom,
                        {0, [this] { return m_controller->timeline()->sizeHint().height(); }, 1.0, false});
    for (const auto& part : parts) {
      m_floating->watchContent(part.content);
    }
    return;
  }
  // Docks (Docked): the same panels beside and below the view.
  QWidget* contents[] = {parts[0].content, parts[1].content, parts[2].content};
  const Qt::DockWidgetArea areas[] = {Qt::LeftDockWidgetArea, Qt::RightDockWidgetArea, Qt::BottomDockWidgetArea};
  for (int i = 0; i < 3; ++i) {
    auto* dock = new QDockWidget(parts[i].title.isEmpty() ? tr("Shots") : parts[i].title, this);
    dock->setObjectName(parts[i].name);
    dock->setWidget(contents[i]);
    dock->setFeatures(QDockWidget::DockWidgetMovable | QDockWidget::DockWidgetFloatable);
    addDockWidget(areas[i], dock);
    dock->hide();
    m_renderDocks << dock;
  }
}

void MainWindow::updateRenderWorkspace(const QString& tab) {
  if (m_renderWorkspace == nullptr) {
    return;
  }
  const bool open = tab == kRenderTab;
  if (open == m_renderWorkspace->isOpen()) {
    return;
  }
  QStringList names;
  for (GlassCard* card : std::as_const(m_renderCards)) {
    names << card->objectName();
  }
  if (open) {
    // Its contents replace Render Environment's and Render Image's windows.
    m_viewController->closeRenderWindows();
    if (m_mode == Mode::Idle && m_controller != nullptr) {
      clearSelection();
    }
    refreshRenderWorkspace();
    if (m_floating != nullptr) {
      m_floating->showOnlyCards(names);
      for (GlassCard* card : std::as_const(m_renderCards)) {
        card->setVisible(true);
      }
      // The design framed between the side cards, once the layout has
      // placed them: a body left under the Settings card showed only as a
      // thin edge beside it (a selected one as a blue line).
      TestSync::singleShot(0, this, [this] {
        if (m_renderWorkspace != nullptr && m_renderWorkspace->isOpen() && m_viewer != nullptr) {
          m_viewer->fitAll();
        }
      });
    } else if (!m_nativeUi) {
      m_renderHiddenDocks.clear();
      for (QDockWidget* dock : findChildren<QDockWidget*>(QString(), Qt::FindDirectChildrenOnly)) {
        if (!dock->isHidden() && !m_renderDocks.contains(dock)) {
          m_renderHiddenDocks << dock->objectName();
          dock->hide();
        }
      }
      for (QDockWidget* dock : std::as_const(m_renderDocks)) {
        dock->show();
      }
    }
    m_renderWorkspace->setOpen(true);
    qDebug().noquote() << QStringLiteral("Render workspace panels: %1")
                              .arg(m_floating != nullptr ? names.join(QStringLiteral(", "))
                                                         : QStringLiteral("renderLeftPanel, renderSettingsPanel, "
                                                                          "renderBarPanel (docked)"));
  } else {
    m_renderWorkspace->setOpen(false);
    if (m_floating != nullptr) {
      // A drawing that opened meanwhile keeps its Drawings card
      // (mitcad#1008, mitcad#1990).
      m_floating->showOnlyCards(drawingOpen() ? drawingCards() : QStringList());
      for (GlassCard* card : std::as_const(m_renderCards)) {
        card->setVisible(false);
      }
    } else if (!m_nativeUi) {
      for (QDockWidget* dock : std::as_const(m_renderDocks)) {
        dock->hide();
      }
      for (const QString& name : std::as_const(m_renderHiddenDocks)) {
        if (auto* dock = findChild<QDockWidget*>(name, Qt::FindDirectChildrenOnly)) {
          dock->show();
        }
      }
      m_renderHiddenDocks.clear();
    }
    if (m_mode == Mode::Idle) {
      updateIdleView(); // the idle pick filter again
    }
  }
  TestSync::singleShot(100, m_renderWorkspace, [workspace = m_renderWorkspace] { workspace->logLayout(); });
}

void MainWindow::refreshRenderWorkspace() {
  if (m_renderWorkspace == nullptr) {
    return;
  }
  m_renderWorkspace->setDocument(m_filePath.isEmpty() ? QString() : QFileInfo(m_filePath).absolutePath(),
                                 m_filePath.isEmpty() ? QString() : QFileInfo(m_filePath).completeBaseName());
  if (m_renderWorkspace->isOpen() && !modelBusy()) {
    m_renderWorkspace->refresh();
  }
}

} // namespace mitcad
