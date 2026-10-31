// SPDX-License-Identifier: MIT
#include "MainWindow.hpp"

#include <algorithm>
#include <functional>
#include <stdexcept>
#include <utility>

#include <QAbstractItemView>
#include <QAction>
#include <QApplication>
#include <QCloseEvent>
#include <QCryptographicHash>
#include <QElapsedTimer>
#include <QEvent>
#include <QFileInfo>
#include <QLabel>
#include <QMenu>
#include <QMenuBar>
#include <QModelIndex>
#include <QObject>
#include <QPalette>
#include <QSet>
#include <QSettings>
#include <QStackedWidget>
#include <QTimer>
#include <QTabBar>
#include <QToolBar>
#include <QToolButton>
#include <QtLogging>

#include "browser/BrowserController.hpp"
#include "browser/ConfigurationBar.hpp"
#include "browser/TimelineWidget.hpp"
#include "files/ProjectIndicator.hpp"
#include "files/LockController.hpp"
#include "framework/AppSettings.hpp"
#include "framework/AutoselectPanel.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/ComputeProgress.hpp"
#include "framework/Dialogs.hpp"
#include "framework/Icons.hpp"
#include "framework/Ribbon.hpp"
#include "mcp/AgentIndicator.hpp"
#include "platform/NativeDialogs.hpp"
#include "platform/NativeForms.hpp"
#include "projects/ProjectsPanel.hpp"
#include "platform/NativeMacWindow.hpp"
#include "report/ReportCenter.hpp"
#include "sketch/SketchPalette.hpp"

namespace mitcad {
namespace {

// AppKit holds rendered icon images, so a Qt palette change must replace
// those images after Qt finishes updating the application's palette.
class NativePaletteWatcher : public QObject {
public:
  NativePaletteWatcher(QWidget* root, std::function<void()> refresh)
      : QObject(root), m_root(root), m_refresh(std::move(refresh)), m_palette(QApplication::palette()) {
    root->installEventFilter(this);
    qApp->installEventFilter(this);
  }

  bool eventFilter(QObject* watched, QEvent* event) override {
    if ((watched == m_root || watched == qApp) && !m_pending &&
        (event->type() == QEvent::ApplicationPaletteChange || event->type() == QEvent::PaletteChange)) {
      m_pending = true;
      QTimer::singleShot(0, this, [this] {
        m_pending = false;
        const QPalette palette = QApplication::palette();
        if (palette == m_palette) {
          return;
        }
        m_palette = palette;
        m_refresh();
      });
    }
    return false;
  }

private:
  QWidget* m_root;
  std::function<void()> m_refresh;
  QPalette m_palette;
  bool m_pending = false;
};

QString menuLabel(QString text) {
  text.remove(QLatin1Char('&'));
  return text;
}

QString symbolForAction(const QString& id) {
  static const QHash<QString, QString> symbols = {
      {QStringLiteral("edit.undo"), QStringLiteral("arrow.uturn.backward")},
      {QStringLiteral("edit.redo"), QStringLiteral("arrow.uturn.forward")},
      {QStringLiteral("file.new"), QStringLiteral("doc.badge.plus")},
      {QStringLiteral("file.open"), QStringLiteral("folder")},
      {QStringLiteral("file.save"), QStringLiteral("square.and.arrow.down")},
      {QStringLiteral("view.fit"), QStringLiteral("arrow.up.left.and.arrow.down.right")},
      {QStringLiteral("view.home"), QStringLiteral("house")},
      {QStringLiteral("tools.search"), QStringLiteral("magnifyingglass")},
      {QStringLiteral("tools.preferences"), QStringLiteral("gearshape")},
      {QStringLiteral("sketch.create"), QStringLiteral("pencil.and.outline")},
      {QStringLiteral("sketch.finish"), QStringLiteral("checkmark")}};
  return symbols.value(id);
}

QString symbolForNode(const BrowserNode& node) {
  switch (node.type) {
  case BrowserNode::Type::Body: return QStringLiteral("cube");
  case BrowserNode::Type::Component: return QStringLiteral("square.stack.3d.up");
  case BrowserNode::Type::Sketch: return QStringLiteral("pencil.and.outline");
  case BrowserNode::Type::Settings:
  case BrowserNode::Type::Units: return QStringLiteral("gearshape");
  case BrowserNode::Type::NamedView: return QStringLiteral("viewfinder");
  case BrowserNode::Type::Joint: return QStringLiteral("link");
  default: return node.isFolder() ? QStringLiteral("folder") : QStringLiteral("square.dashed");
  }
}

// The side cards' widths dragged by their edges (mitcad#818): Projects's
// in the Floating card's setting (projects/cardWidth, mitcad#529), so that
// both frontends open it as wide; Browser's and the inspector's in
// browser/cardWidth and inspector/cardWidth.
void rememberNativeCardWidth(const QString& card, int width) {
  if (card == QLatin1String("projects")) {
    ProjectsSettings settings = ProjectsSettings::load();
    settings.cardWidth = width;
    settings.save();
  } else {
    QSettings().setValue(card + QStringLiteral("/cardWidth"), width);
  }
  qInfo().noquote() << QStringLiteral("Native %1 card: width %2 remembered").arg(card).arg(width);
}

void restoreNativeCardWidths(NativeMacWindow& window) {
  const QSettings settings;
  const int projects = ProjectsSettings::load().cardWidth;
  const int browser = std::max(0, settings.value(QStringLiteral("browser/cardWidth")).toInt());
  const int inspector = std::max(0, settings.value(QStringLiteral("inspector/cardWidth")).toInt());
  window.setCardWidth(QStringLiteral("projects"), projects);
  window.setCardWidth(QStringLiteral("browser"), browser);
  window.setCardWidth(QStringLiteral("inspector"), inspector);
  if (projects > 0 || browser > 0 || inspector > 0) {
    qInfo().noquote() << QStringLiteral("Native card widths: projects %1, browser %2, inspector %3 (remembered)")
                             .arg(projects).arg(browser).arg(inspector);
  }
}

} // namespace

void MainWindow::createNativeWindow() {
  m_viewer->setNativeControls(true);
  mac::NativeMacWindowCallbacks callbacks;
  callbacks.cancelPointerInteraction = [this] { m_viewer->cancelPointerInteraction(); };
  callbacks.action = [this](const QString& id) {
    // AppKit may be dispatching an action from a menu that a command
    // rebuilds. Run it after that dispatch and the viewport's paint.
    QTimer::singleShot(0, this, [this, id] { nativeAction(id); });
  };
  callbacks.confirm = [this] {
    if (!m_confirmAction->isEnabled()) {
      return false;
    }
    QTimer::singleShot(0, this, [this] {
      if (!m_session && modelBusy()) {
        whenIdle(this, [this] { confirm(); });
      } else {
        confirm();
      }
    });
    return true;
  };
  callbacks.cancel = [this] {
    QTimer::singleShot(0, this, [this] { cancel(); });
    return true;
  };
  callbacks.selection = [this](const QStringList& keys) {
    Selection items;
    QStringList features;
    for (const QString& key : keys) {
      const BrowserNode node = m_nativeNodes.value(key);
      if (node.type == BrowserNode::Type::Feature) {
        if (!features.contains(node.uid)) features.append(node.uid);
        continue;
      }
      const SelectionItem item = node.item();
      if (item.isValid()) {
        items.append(item);
      }
    }
    emit m_controller->browser()->rowsPicked(items, features);
  };
  callbacks.activate = [this](const QString& key) {
    const BrowserNode node = m_nativeNodes.value(key);
    if (node.isValid()) {
      emit m_controller->browser()->doubleClicked(node);
    }
  };
  callbacks.rename = [this](const QString& key, const QString& name) {
    const BrowserNode node = m_nativeNodes.value(key);
    if (node.isValid()) {
      emit m_controller->browser()->renamed(node, name);
    }
  };
  callbacks.treeBindings = [this](const QString& key, const QRect& anchor, bool pinned) {
    const BrowserNode node = m_nativeNodes.value(key);
    if (node.isValid()) {
      QTimer::singleShot(0, this, [this, node, anchor, pinned] {
        emit m_controller->browser()->bindingsRequested(node, anchor, pinned);
      });
    }
  };
  callbacks.historyBindings = [this](const QString& uid, const QRect& anchor, bool pinned) {
    QTimer::singleShot(0, this, [this, uid, anchor, pinned] {
      emit m_controller->timeline()->bindingsRequested(uid, anchor, pinned);
    });
  };
  callbacks.treeVisibility = [this](const QString& key) {
    const BrowserNode node = m_nativeNodes.value(key);
    if (node.hasEye) {
      emit m_controller->browser()->eyeClicked(node);
    }
  };
  callbacks.componentActivated = [this](const QString& key) {
    const BrowserNode node = m_nativeNodes.value(key);
    if (node.isValid()) emit m_controller->browser()->activateClicked(node);
  };
  callbacks.treeExpanded = [this](const QString& key, bool expanded) {
    m_controller->browser()->setPresentationExpanded(key, expanded);
  };
  callbacks.browserIssues = [this](bool warnings, const QPoint& position) {
    m_controller->showIssues(warnings, position);
    QTimer::singleShot(0, this, [this] { refreshNativeWindow(); });
  };
  callbacks.treeContextMenu = [this](const QString& key, const QPoint& point) {
    const BrowserNode node = m_nativeNodes.value(key);
    if (node.isValid()) {
      emit m_controller->browser()->contextMenuRequested(node, point);
    }
  };
  callbacks.treeDelete = [this](const QString& key) {
    const BrowserNode node = m_nativeNodes.value(key);
    if (node.isValid() && !modelBusy() && canChangeModel()) {
      emit m_controller->browser()->deleteRequested(node);
    }
  };
  callbacks.historyDelete = [this](const QString& uid) {
    if (!modelBusy() && canChangeModel()) emit m_controller->timeline()->deleteRequested(uid);
  };
  callbacks.historyRename = [this](const QString& uid) {
    QTimer::singleShot(0, this, [this, uid] {
      const auto* feature = m_snapshot.feature(uid);
      if (!feature || modelBusy() || !canChangeModel()) return;
      bool accepted = false;
      const QString name = dialogText(this, tr("Rename Feature"), tr("Name:"), feature->name, &accepted).trimmed();
      if (accepted && !name.isEmpty() && !modelBusy() && canChangeModel()) {
        emit m_controller->timeline()->renamed(uid, name);
      }
    });
  };
  callbacks.historyActivated = [this](const QString& uid) {
    emit m_controller->timeline()->editRequested(uid);
  };
  callbacks.historySelection = [this](const QStringList& uids) {
    m_controller->timeline()->setSelectedFeatures(uids);
    emit m_controller->timeline()->featuresClicked(uids);
  };
  callbacks.historyMarkerChanged = [this](int marker) {
    emit m_controller->timeline()->markerRequested(marker);
  };
  callbacks.historyCanChange = [this] { return !modelBusy() && canChangeModel(); };
  callbacks.historyMarkerDragStarted = [this] {
    if (!modelBusy() && canChangeModel()) {
      emit m_controller->timeline()->markerDragStarted();
    }
  };
  callbacks.historyMarkerDragged = [this](int marker) {
    emit m_controller->timeline()->markerDragged(marker);
  };
  callbacks.historyMarkerDragFinished = [this](int marker) {
    emit m_controller->timeline()->markerDragFinished(marker);
  };
  callbacks.historyReordered = [this](const QString& uid, int position) {
    emit m_controller->timeline()->reorderRequested(uid, position);
  };
  callbacks.historyContextMenu = [this](const QString& uid, const QPoint& point) {
    emit m_controller->timeline()->contextMenuRequested(uid, point);
  };
  callbacks.historyFilterMenu = [this](const QPoint& point) {
    m_controller->timeline()->showFilterMenu(point);
  };
  callbacks.timelineSearchOpened = [this] { m_controller->timeline()->beginExternalSearch(); };
  callbacks.timelineSearchChanged = [this](const QString& text) { m_controller->timeline()->setSearchText(text); };
  callbacks.timelineSearchStepped = [this](int delta) { m_controller->timeline()->stepSearch(delta); };
  callbacks.timelineSearchSelected = [this](int index) { m_controller->timeline()->goToMatch(index); };
  callbacks.timelineSearchClosed = [this] { m_controller->timeline()->closeSearch(); };
  callbacks.timelineSearchFilterMenu = [this](const QPoint& point) { m_controller->timeline()->showSearchMenu(point); };
  callbacks.projectMenu = [this](const QPoint& point) {
    if (!m_indicator || !m_indicator->menu()) {
      return;
    }
    execMenu(m_indicator->menu(), point);
  };
  // The agent indicator (mitcad#710): a click connects an assistant while
  // none is connected, else (connected or unavailable) opens the menu, as
  // the Qt indicator does.
  callbacks.agentClicked = [this](const QPoint& point) {
    if (!m_agentIndicator) {
      return;
    }
    if (m_agentIndicator->state() == mcp::AgentIndicator::State::NoAgent) {
      m_agentIndicator->openConnect();
    } else {
      execMenu(m_agentIndicator->agentMenu(), point);
    }
  };
  callbacks.agentMenu = [this](const QPoint& point) {
    if (m_agentIndicator) {
      execMenu(m_agentIndicator->agentMenu(), point);
    }
  };
  callbacks.workspaceChanged = [this](int index) {
    m_ribbon->tabBar()->setCurrentIndex(index);
    refreshNativeWindow();
  };
  callbacks.requestClose = [this] { return requestNativeClose(); };
  callbacks.closed = [] { QCoreApplication::quit(); };
  callbacks.openFile = [this](const QString& path) {
    whenIdle(this, [this, path] { openExternalFile(path); });
  };
  callbacks.viewportOverlays = [this](const QMargins& insets, const std::vector<QRect>& covered) {
    m_viewer->setOverlayInsets(insets);
    m_viewer->setCoveredRects(covered);
  };
  callbacks.cubeArrow = [this](const QString& id) {
    QTimer::singleShot(0, this, [this, id] { m_viewer->pressCubeArrow(id); });
  };
  callbacks.cardWidthChosen = rememberNativeCardWidth;
  // The focused Projects outline's keys are the shared panel's (mitcad#816).
  callbacks.projectsKey = [this](int key) { return m_projectsPanel != nullptr && m_projectsPanel->treeKey(key); };
  m_nativeWindow = std::make_unique<NativeMacWindow>(this, std::move(callbacks));
  if (!m_nativeWindow->isEmbedded()) {
    throw std::runtime_error(m_nativeWindow->embeddingError().toStdString());
  }
  restoreNativeCardWidths(*m_nativeWindow);
  // The orientation cube's arrows are AppKit buttons at the hidden Qt
  // arrows' places.
  const auto showCubeArrows = [this] {
    QList<mac::NativeMacCubeArrow> arrows;
    for (const OcctViewer::CubeArrow& arrow : m_viewer->shownCubeArrows()) {
      arrows.append({arrow.id, arrow.toolTip, arrow.symbol,
                     QRect(m_viewer->mapTo(this, arrow.geometry.topLeft()), arrow.geometry.size())});
    }
    m_nativeWindow->setCubeArrows(arrows);
  };
  connect(m_viewer, &OcctViewer::cubeArrowsChanged, this, showCubeArrows);
  showCubeArrows();
  setNativeDialogWindow(m_nativeWindow->nativeWindow());
  connect(m_controller->browser(), &BrowserPanel::contentHeightChanged, this, [this] {
    m_nativeSnapshotKey.clear();
    QTimer::singleShot(0, this, [this] { refreshNativeWindow(); });
  });
  createProjectsPanel();
  m_projectsPanel->setParent(this);
  m_projectsPanel->hide();
  m_projectsContainer = m_projectsPanel;
  m_nativeProjectsShown = projectsPanelShownAtStart();
  m_projectsPanel->setNativePresented(m_nativeProjectsShown);
  if (QAction* action = m_registry->action(QStringLiteral("view.projects"))) action->setChecked(m_nativeProjectsShown);
  // AppKit expansion/model notifications must finish before measuring rows.
  connect(m_projectsPanel, &ProjectsPanel::contentHeightChanged, this,
          &MainWindow::refreshNativeProjects, Qt::QueuedConnection);
  // New Design, New Folder and Rename name the row in the visible AppKit
  // outline, and its rows move by dragging, with the panel's own rules
  // (mitcad#816). Registered before the outline is first presented.
  const QPointer<ProjectsPanel> projects(m_projectsPanel);
  m_projectsPanel->setNativeEditor(
      [this, projects](const QModelIndex& index, const QString& text) {
        // The row the panel added or renames must be in the outline first.
        refreshNativeProjects();
        const QString identifier = index.data(NativeControlIdentifierRole).toString();
        return m_nativeWindow != nullptr &&
               m_nativeWindow->editProjectsRow(identifier, text, [projects](const QString& name, bool accepted) {
                 if (projects != nullptr) projects->finishNativeEditing(name, accepted);
               });
      },
      [this] {
        if (m_nativeWindow != nullptr) m_nativeWindow->closeProjectsEditor();
      });
  if (auto* tree = m_projectsPanel->findChild<QAbstractItemView*>(QStringLiteral("projectsTree"))) {
    NativeRowMoves moves;
    moves.draggable = [projects](const QModelIndex& row) { return projects != nullptr && projects->canDragRow(row); };
    moves.allowed = [projects](const QModelIndex& dragged, const QModelIndex& over) {
      return projects != nullptr && projects->canDropRow(dragged, over);
    };
    moves.drop = [projects](const QModelIndex& dragged, const QModelIndex& over) {
      if (projects != nullptr) projects->dropRow(dragged, over);
    };
    moves.dragging = [projects](const QModelIndex& dragged, bool dragging) {
      if (projects != nullptr) projects->setNativeDragging(dragged, dragging);
    };
    setNativeRowMoves(tree, std::move(moves));
  }
  if (QMenu* view = findChild<QMenu*>(QStringLiteral("viewMenu"))) {
    QAction* browser = view->addAction(tr("Browser"));
    browser->setCheckable(true);
    browser->setChecked(true);
    connect(browser, &QAction::toggled, this, [this](bool shown) { m_nativeWindow->showSidebar(shown); });
    QAction* timeline = view->addAction(tr("Timeline"));
    timeline->setCheckable(true);
    timeline->setChecked(true);
    connect(timeline, &QAction::toggled, this, [this](bool shown) { m_nativeWindow->showHistory(shown); });
  }

  new NativePaletteWatcher(this, [this] {
    m_nativeSnapshotKey.clear();
    refreshNativeWindow(); // A busy document refreshes from busyChanged after it becomes idle.
  });
  connect(m_controller->timeline(), &TimelineWidget::filterChanged, this, [this] {
    m_nativeSnapshotKey.clear();
    refreshNativeWindow();
  }, Qt::QueuedConnection);
  // Search navigation changes the shared selection without a model snapshot.
  // Refresh after BrowserController's queued pick has updated the browser too.
  connect(m_controller->timeline(), &TimelineWidget::featuresClicked, this, [this] {
    refreshNativeWindow();
  }, Qt::QueuedConnection);
  connect(m_controller->timeline(), &TimelineWidget::searchChanged, this, [this] {
    auto* timeline = m_controller->timeline();
    QList<mac::NativeMacHistoryItem> results;
    for (const QString& uid : timeline->searchMatches()) {
      if (const auto* feature = m_snapshot.feature(uid)) {
        mac::NativeMacHistoryItem item;
        item.id = uid;
        item.title = feature->name;
        item.detail = m_snapshot.componentName(feature->component);
        item.icon = themeIcon(featureIcon(feature->type));
        results.append(item);
      }
    }
    m_nativeWindow->setTimelineSearchResults(timeline->searchText(), timeline->searchCountText(), results,
                                             timeline->currentSearchMatch());
  });
  connect(m_controller->timeline(), &TimelineWidget::searchStarted, this,
          [this] { m_nativeWindow->saveTimelineSearchScroll(); });
  // BrowserController's queued restoration runs first, then the native
  // scroll positions are restored after its selection refresh.
  connect(m_controller->timeline(), &TimelineWidget::searchRestored, this, [this] {
    refreshNativeWindow();
    m_nativeWindow->restoreTimelineSearchScroll();
  }, Qt::QueuedConnection);
  connect(m_indicator, &ProjectIndicator::presentationChanged, this, [this] {
    if (m_nativeWindow) {
      m_nativeWindow->setProjectStatus(m_indicator->shownText(), m_indicator->toolTip());
    }
  });
  m_nativeWindow->setProjectStatus(m_indicator->shownText(), m_indicator->toolTip());
  const auto agentStatus = [this] {
    if (m_nativeWindow) {
      const auto state = m_agentIndicator->state();
      m_nativeWindow->setAgentStatus(m_agentIndicator->shownText(), m_agentIndicator->toolTip(),
                                     state == mcp::AgentIndicator::State::Connected,
                                     state != mcp::AgentIndicator::State::Unavailable);
    }
  };
  connect(m_agentIndicator, &mcp::AgentIndicator::presentationChanged, this, agentStatus);
  agentStatus();
  // Notices remain shared state, but their Qt toolbar does not enter
  // AppKit's viewport. The native status and menus expose their actions.
  auto* noticeStore = new QWidget(this);
  noticeStore->hide();
  // These retained Qt presenters must never paint beside the native card.
  for (QToolButton* badge : {m_controller->failures(), m_controller->warnings(), m_controller->findButton()}) {
    const bool shown = !badge->isHidden();
    badge->setParent(noticeStore);
    badge->setVisible(shown);
  }
  for (auto* toolbar : findChildren<QToolBar*>(QString(), Qt::FindDirectChildrenOnly)) {
    removeToolBar(toolbar);
    toolbar->setParent(noticeStore);
    toolbar->hide();
  }
  connect(m_documentSession, &DocumentSession::busyChanged, this, [this](bool busy) {
    refreshNativeActions();
    if (!busy) {
      QTimer::singleShot(0, this, [this] {
        if (!modelBusy()) {
          refreshNativeWindow();
        }
      });
    }
  });
  connect(m_documentSession, &DocumentSession::jobStarted, this,
          [this](const QString& label, const QString& stage) {
    if (!m_documentSession->asynchronousJob() || label == QStringLiteral("preview") ||
        (label == QStringLiteral("inspection") && m_session)) {
      return;
    }
    const quint64 generation = ++m_nativeJobGeneration;
    QTimer::singleShot(400, this, [this, generation, label, stage] {
      if (generation != m_nativeJobGeneration || !m_documentSession->asynchronousJob()) {
        return;
      }
      m_nativeProgress = new ComputeProgress(this);
      connect(m_nativeProgress, &ComputeProgress::cancelRequested, m_documentSession,
              &DocumentSession::cancelJob);
      if (ModelJob* job = m_documentSession->activeJob()) {
        m_nativeProgress->showProgress(stage, job->control().progress(), 400);
      }
      showDialog(m_nativeProgress);
      qDebug().noquote() << QStringLiteral("Progress dialog shown: %1").arg(label);
    });
  });
  connect(m_documentSession, &DocumentSession::jobProgress, this,
          [this](const QString& stage, quint64, quint64, qint64 elapsed) {
    if (m_nativeProgress) {
      if (ModelJob* job = m_documentSession->activeJob()) {
        m_nativeProgress->showProgress(stage, job->control().progress(), elapsed);
      }
    }
  });
  connect(m_documentSession, &DocumentSession::jobFinished, this, [this] {
    ++m_nativeJobGeneration;
    if (m_nativeProgress) {
      delete m_nativeProgress.data();
    }
  });
  refreshNativeWindow();
  qInfo() << "Native UI: AppKit shell with embedded OCCT viewport";
}

void MainWindow::nativeAction(const QString& id) {
  if (id == QStringLiteral("tools.search")) {
    m_nativeWindow->showCommandSearch();
    return;
  }
  if (id == QStringLiteral("native.quit")) {
    m_nativeWindow->close();
    return;
  }
  if (modelBusy()) {
    whenIdle(this, [this, id] { nativeAction(id); });
    return;
  }
  if (const QPointer<QAction> action = m_nativeActions.value(id); action && action->isEnabled()) {
    action->trigger();
  } else if (m_registry->find(id)) {
    trigger(id);
  }
  if (!modelBusy()) {
    refreshNativeWindow();
  }
}

void MainWindow::refreshNativeActions() {
  if (!m_nativeWindow) {
    return;
  }
  QList<mac::NativeMacAction> actions;
  QSet<QAction*> assigned;
  QHash<QString, int> commandOrder;
  int order = 0;
  for (const auto& def : m_registry->commands()) {
    commandOrder.insert(def->id, order++);
  }
  m_nativeActions.clear();
  const auto append = [this, &actions, &assigned, &commandOrder](QAction* action, const QString& menu) {
    if (action->isSeparator()) {
      mac::NativeMacAction separator;
      separator.menu = menu;
      separator.separator = true;
      actions.append(separator);
      return;
    }
    if (!action->isVisible()) {
      return;
    }
    if (action->menuRole() == QAction::QuitRole) {
      assigned.insert(action);
      return; // AppKit's Quit action passes through the native close policy.
    }
    mac::NativeMacAction item;
    if (action->objectName().startsWith(QStringLiteral("command_"))) {
      item.id = action->objectName().mid(8);
    } else {
      item.id = action->property("mitcadNativeActionId").toString();
      if (item.id.isEmpty()) {
        const QString key = !action->objectName().isEmpty()
                                ? QStringLiteral("named/") + action->objectName()
                                : action->menu() && !action->menu()->objectName().isEmpty()
                                ? QStringLiteral("menu/") + action->menu()->objectName()
                                : menu + QLatin1Char('/') + menuLabel(action->text());
        item.id = QStringLiteral("native.menu.") + QString::fromLatin1(
            QCryptographicHash::hash(key.toUtf8(), QCryptographicHash::Sha256).toHex());
        // A changing presentation (for example Undo's caption) must not
        // change an action's identity during this process either.
        action->setProperty("mitcadNativeActionId", item.id);
      }
    }
    item.label = menuLabel(action->text());
    item.menu = menu;
    if (action->menu()) item.submenu = QStringLiteral("ribbon.popup.") + item.id;
    const bool fileMenu = menu == QStringLiteral("File") || menu.startsWith(QStringLiteral("File/"));
    item.enabled = action->isEnabled() && !modelBusy() && !(m_session && fileMenu);
    item.checked = action->isChecked();
    item.tooltip = action->toolTip();
    // The ribbon and Sketch Palette share the same coloured CAD artwork,
    // including its palette-dependent light/dark rendering.
    item.icon = action->icon();
    item.iconTemplate = false;
    item.symbol = (item.id == QStringLiteral("edit.undo") || item.id == QStringLiteral("edit.redo"))
                      ? symbolForAction(item.id) : QString();
    item.shortcut = action->shortcut().toString(QKeySequence::PortableText);
    for (const QKeySequence& key : action->shortcuts()) item.shortcuts.append(key.toString(QKeySequence::PortableText));
    if (const CommandDef* def = m_registry->find(item.id)) {
      item.group = def->group;
      item.workspace = def->tab;
      item.keywords = def->keywords;
      item.toolbarPinned = def->pinned;
      item.toolbarOrder = commandOrder.value(def->id);
      item.section = def->section;
      if (!item.workspace.isEmpty()) {
        item.symbol.clear(); // Keep the familiar CAD command drawing.
      }
      item.shortcuts.clear();
      for (const QKeySequence& key : m_registry->effectiveShortcuts(item.id)) {
        item.shortcuts.append(key.toString(QKeySequence::PortableText));
      }
      item.shortcut = item.shortcuts.value(0);
    }
    if (action->menuRole() == QAction::PreferencesRole || action->menuRole() == QAction::AboutRole ||
        action->menuRole() == QAction::AboutQtRole) {
      item.menu = QStringLiteral("MitCad");
    }
    m_nativeActions.insert(item.id, action);
    actions.append(item);
    assigned.insert(action);
  };
  const auto addMenu = [&append](const auto& self, QMenu* menu, const QString& path) -> void {
    for (QAction* action : menu->actions()) {
      if (action->menu()) {
        self(self, action->menu(), path + QLatin1Char('/') + menuLabel(action->text()));
      } else {
        append(action, path);
      }
    }
  };
  for (QAction* top : menuBar()->actions()) {
    if (top->menu() && menuLabel(top->text()) != QStringLiteral("Window")) {
      addMenu(addMenu, top->menu(), menuLabel(top->text()));
    }
  }
  for (const auto& def : m_registry->commands()) {
    QAction* action = m_registry->action(def->id);
    if (action && !assigned.contains(action)) {
      append(action, QString());
    }
  }
  // Read the actual Qt ribbon rather than maintaining an AppKit command list.
  // Menu order, section separators, subgroup defaults and extra filters now
  // have one source in both frontends.
  const auto actionId = [&](QAction* entry) {
    if (!assigned.contains(entry)) append(entry, QString());
    return entry->objectName().startsWith(QStringLiteral("command_"))
               ? entry->objectName().mid(8) : entry->property("mitcadNativeActionId").toString();
  };
  const auto menuItems = [&](const auto& self, QMenu* menu) -> QList<mac::NativeMacMenuItem> {
    QList<mac::NativeMacMenuItem> items;
    for (QAction* entry : menu->actions()) {
      if (!entry->isVisible()) continue;
      mac::NativeMacMenuItem item;
      item.separator = entry->isSeparator();
      item.title = menuLabel(entry->text());
      item.icon = entry->icon();
      item.submenu = entry->menu() != nullptr;
      if (!item.separator) item.action = actionId(entry);
      if (entry->menu()) item.children = self(self, entry->menu());
      items.append(item);
    }
    return items;
  };
  QList<mac::NativeMacRibbonGroup> groups;
  QStringList workspaces;
  QTabBar* tabs = m_ribbon->tabBar();
  for (int tab = 0; tab < tabs->count(); ++tab) workspaces.append(tabs->tabText(tab).toUpper());
  // Keep the hidden sketch page available before its tab is added.
  for (const QString& workspace : {QStringLiteral("SOLID"), QStringLiteral("BUILD"), QStringLiteral("SKETCH")}) {
    for (const QString& name : Ribbon::groups(workspace)) {
      auto* caption = m_ribbon->findChild<QToolButton*>(QStringLiteral("ribbonMenu_") + workspace + QLatin1Char('/') + name);
      if (!caption || !caption->menu()) continue;
      mac::NativeMacRibbonGroup group;
      group.workspace = workspace;
      group.name = name;
      group.title = displayName(name);
      const auto widths = m_ribbon->groupWidths(workspace, name);
      group.fullWidth = widths.first;
      group.collapsedWidth = widths.second;
      group.menu = menuItems(menuItems, caption->menu());
      for (QToolButton* button : caption->parentWidget()->findChildren<QToolButton*>()) {
        if (!button->defaultAction() || button->parentWidget()->parentWidget() != caption->parentWidget()) continue;
        mac::NativeMacRibbonButton item;
        item.action = actionId(button->defaultAction());
        item.width = button->sizeHint().width();
        item.iconSize = button->iconSize().width();
        item.menuOnly = button->popupMode() == QToolButton::InstantPopup;
        if (button->menu()) {
          item.menu = menuItems(menuItems, button->menu());
        }
        group.buttons.append(item);
      }
      // Extras such as Selection Filters use the same native popup as in Qt.
      groups.append(group);
    }
  }
  m_nativeWindow->setActions(actions);
  m_nativeWindow->setRibbon(groups, workspaces, tabs->currentIndex());
}

void MainWindow::refreshNativeWindow() {
  if (!m_nativeWindow || modelBusy()) {
    return;
  }
  const auto* failed = m_controller->failures();
  const auto* warned = m_controller->warnings();
  m_nativeWindow->setBrowserIssues(failed->isHidden() ? QString() : failed->text(), failed->toolTip(),
                                  warned->isHidden() ? QString() : warned->text(), warned->toolTip());
  ConfigurationBar* configuration = m_controller->configurationBar();
  if (configuration != nullptr && !configuration->isHidden()) {
    // The bounded browser can be narrower than the shared grid's reading
    // hint. Only this card opts in to placing its row cells on extra lines.
    if (!configuration->property("nativeResponsiveLayout").toBool()) {
      configuration->setProperty("nativeResponsiveLayout", true);
    }
    if (!m_nativeConfigurationForm) m_nativeConfigurationForm = createNativeWidgetView(configuration);
    else m_nativeConfigurationForm->refresh();
    m_nativeWindow->setConfigurationView(m_nativeConfigurationForm ? m_nativeConfigurationForm->nativeView() : nullptr);
  } else {
    m_nativeWindow->setConfigurationView(nullptr);
  }
  refreshNativeActions();
  m_nativeWindow->setDocument(documentName(), m_filePath, isModified());
  m_nativeWindow->setSketchMode(m_sketch->isActive());
  m_nativeWindow->setHistoryFilter(m_controller->timeline()->filterLabel(),
                                 !m_controller->timeline()->filter().isEmpty());
  const bool canRename = canChangeModel();
  const bool canToggleVisibility = canChangeModel(true);
  if (m_nativeSnapshotKey != m_snapshot.key || m_nativeNodes.isEmpty() || m_nativeCanRename != canRename ||
      m_nativeCanToggleVisibility != canToggleVisibility) {
    QList<mac::NativeMacTreeItem> rows;
    m_nativeNodes.clear();
    for (const auto& row : m_controller->browser()->presentationRows()) {
      mac::NativeMacTreeItem item;
      item.id = row.key;
      item.parentId = row.parent;
      item.title = row.node.name;
      item.detail = row.tooltip;
      item.symbol = symbolForNode(row.node);
      item.icon = row.icon;
      item.expanded = row.expanded;
      item.hasActivation = row.hasActivation;
      item.active = row.active;
      item.kind = row.node.isFolder() ? QStringLiteral("folder") : QString::number(static_cast<int>(row.node.type));
      item.editable = canRename && (row.node.type == BrowserNode::Type::Body || row.node.type == BrowserNode::Type::Sketch ||
                                   row.node.type == BrowserNode::Type::Component || row.node.type == BrowserNode::Type::Datum ||
                                   row.node.type == BrowserNode::Type::Analysis || row.node.type == BrowserNode::Type::Joint ||
                                   row.node.type == BrowserNode::Type::Feature ||
                                   row.node.type == BrowserNode::Type::Drawing ||
                                   row.node.type == BrowserNode::Type::ImageReference ||
                                   (row.node.type == BrowserNode::Type::NamedView && row.node.uid.startsWith(QStringLiteral("named:"))));
      item.hasVisibility = row.node.hasEye;
      // Bodies, sketches, construction and components in every mode (mitcad#295).
      item.canToggleVisibility =
          row.node.hasEye && (canToggleVisibility || BrowserController::visibilityInEveryMode(row.node));
      item.visible = row.node.visible;
      item.checked = row.node.visible;
      item.bindings = row.bindings;
      rows.append(item);
      m_nativeNodes.insert(row.key, row.node);
    }
    m_nativeWindow->setModelItems(rows);
    QList<mac::NativeMacHistoryItem> history;
    const QSet<QString> filtered = m_controller->timeline()->filteredFeatures();
    for (const auto& feature : m_snapshot.features) {
      mac::NativeMacHistoryItem item;
      item.id = feature.uid;
      item.title = feature.name;
      QStringList detail = {feature.name, feature.type};
      if (!feature.error.isEmpty()) detail.append(feature.error);
      if (feature.suppressed) detail.append(tr("Suppressed"));
      else if (feature.index >= m_snapshot.marker || feature.status == QStringLiteral("rolled_back")) {
        detail.append(tr("Rolled back"));
      } else if (feature.status == QStringLiteral("pending")) {
        detail.append(tr("Pending"));
      }
      item.detail = detail.join(QLatin1Char('\n'));
      item.symbol = feature.isSketch() ? QStringLiteral("pencil.and.outline") : QStringLiteral("cube");
      item.icon = themeIcon(featureIcon(feature.type));
      item.status = feature.status;
      item.noted = !feature.importNote.isEmpty();
      item.suppressed = feature.suppressed;
      item.filtered = filtered.contains(feature.uid);
      item.bindings = feature.suppressedWhen.isEmpty() ? 0 : 1;
      history.append(item);
    }
    m_nativeWindow->setHistory(history, m_snapshot.marker);
    m_nativeSnapshotKey = m_snapshot.key;
    m_nativeCanRename = canRename;
    m_nativeCanToggleVisibility = canToggleVisibility;
  }
  QStringList selected;
  for (auto node = m_nativeNodes.cbegin(); node != m_nativeNodes.cend(); ++node) {
    if (m_selection.contains(node.value().item()) ||
        (node.value().type == BrowserNode::Type::Feature && m_controller->timeline()->selectedFeatures().contains(node.value().uid))) {
      selected.append(node.key());
    }
  }
  m_nativeWindow->setSelection(selected);
  m_nativeWindow->setHistorySelection(m_controller->timeline()->selectedFeatures());
  refreshNativeInspector();
  refreshNativeProjects();
}

void MainWindow::refreshNativeProjects() {
  if (!m_nativeWindow || !m_projectsPanel) return;
  if (!m_nativeProjectsForm) m_nativeProjectsForm = createNativeWidgetView(m_projectsPanel);
  else m_nativeProjectsForm->refresh();
  m_nativeWindow->setProjectsView(m_nativeProjectsForm ? m_nativeProjectsForm->nativeView() : nullptr,
                                 m_nativeProjectsShown);
}

void MainWindow::refreshNativeInspector() {
  if (m_nativeForm || !m_nativeWindow) {
    return;
  }
  QWidget* page = m_mode == Mode::Sketch ? static_cast<QWidget*>(m_palette) : m_idlePage;
  m_nativeWindow->showInspector(m_mode == Mode::Sketch || !m_selection.isEmpty() ||
                                (m_mode == Mode::Idle && m_autoselect->available()));
  if (m_nativeIdlePage != page || !m_nativeIdleForm) {
    m_nativeIdleForm = createNativeWidgetView(page);
    m_nativeIdlePage = page;
  } else {
    m_nativeIdleForm->refresh();
  }
  m_nativeWindow->setInspectorView(m_nativeIdleForm ? m_nativeIdleForm->nativeView() : nullptr);
}

bool MainWindow::requestNativeClose() {
  if (m_session) {
    if (!m_nativeClosingCommand) {
      m_nativeClosingCommand = true;
      deferForNativeCommand([this] { m_nativeWindow->close(); });
    }
    return false;
  }
  QCloseEvent event;
  closeEvent(&event);
  return event.isAccepted();
}

bool MainWindow::modelCommandAsync(const QJsonObject& request, QObject* context, AsyncCompletion completion) {
  if (!m_nativeUi) {
    return false;
  }
  const QString name = request.value(QStringLiteral("cmd")).toString();
  // Native jobs bypass command(), so apply the same edit-lock policy
  // before giving the worker access to the document. A refusal completes
  // asynchronously rather than looking like a busy job to its caller.
  if (windowReadOnly() && !m_writeAnyway && !readOnlyModelCommand(name)) {
    const QString error = readOnlyReason();
    m_lastError = error;
    m_lastCancelled = false;
    qInfo().noquote() << QStringLiteral("Read-only: refused %1").arg(name);
    QTimer::singleShot(0, context ? context : this, [error, completion = std::move(completion)] {
      completion({{}, error, false});
    });
    return true;
  }
  if (m_reports) {
    m_reports->noteAction(QStringLiteral("model ") + name);
  }
  QElapsedTimer clock;
  clock.start();
  return m_documentSession->commandAsync(request, context, [this, clock, name, completion = std::move(completion)](
                                                               const DocumentSession::AsyncResult& result) {
    if (result.succeeded()) {
      reportRecompute(result.answer, static_cast<double>(clock.nsecsElapsed()) / 1.0e6);
      notePlacement(result.answer);
      m_locks->activity();
    } else if (result.cancelled) {
      showHint(tr("Computation cancelled."));
      qInfo().noquote() << QStringLiteral("Command %1 cancelled").arg(name);
    }
    completion({result.answer, result.error, result.cancelled});
  });
}

bool MainWindow::modelInspectionAsync(const QString& name, QObject* context, InspectionTask task,
                                     InspectionCompletion completion) {
  if (!m_nativeUi) {
    return false;
  }
  return m_documentSession->inspectionAsync(name, context, std::move(task),
      [completion = std::move(completion)](const Inspection& inspection,
                                          const DocumentSession::AsyncResult& result) {
        completion(inspection, {result.answer, result.error, result.cancelled});
      });
}

} // namespace mitcad
