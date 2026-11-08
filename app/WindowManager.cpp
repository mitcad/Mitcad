// SPDX-License-Identifier: MIT
#include "WindowManager.hpp"
#include "MainWindow.hpp"
#include "framework/AppSettings.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/InterfaceSize.hpp"
#include "platform/MacChrome.hpp"

#include <QAction>
#include <QApplication>
#include <QEvent>
#include <QFileInfo>
#include <QKeySequence>
#include <QMenu>
#include <QMenuBar>
#include <QScopedValueRollback>
#include <QSettings>
#include <QTimer>
#include <QtLogging>
#include <utility>

namespace mitcad {
namespace {
QPointer<WindowManager> manager;
QString fileKey(const QString& path) {
  if (path.isEmpty()) return {};
  const QFileInfo info(path);
  QString key = info.canonicalFilePath();
  if (key.isEmpty()) key = info.absoluteFilePath();
#ifdef Q_OS_WIN
  key = key.toCaseFolded();
#endif
  return key;
}
} // namespace

WindowManager::WindowManager(bool nativeUi, QObject* parent) : QObject(parent), m_nativeUi(nativeUi) {
  Q_ASSERT(manager == nullptr);
  manager = this;
}

WindowManager::~WindowManager() {
  // At process shutdown, finish every session before QApplication's teardown.
  const auto kept = m_windows;
  m_windows.clear();
  for (const auto& window : kept) {
    if (!window) continue;
    disconnect(window, nullptr, this, nullptr);
    disconnect(this, nullptr, window, nullptr);
    delete window.data();
  }
  if (manager == this) manager.clear();
}

WindowManager* WindowManager::instance() { return manager; }

QList<MainWindow*> WindowManager::windows() const {
  QList<MainWindow*> result;
  for (const auto& window : m_windows) if (window) result.append(window.data());
  return result;
}

MainWindow* WindowManager::activeWindow() const {
  return m_active ? m_active.data() : (windows().isEmpty() ? nullptr : windows().last());
}

void WindowManager::setActiveWindow(MainWindow* window) {
  if (window == nullptr || !windows().contains(window) || m_active == window) return;
  m_active = window;
  emit activeWindowChanged(window);
  emit windowsChanged();
}

MainWindow* WindowManager::createWindow(bool demo, MainWindow* source, bool separate) {
  QPointer<MainWindow> sourceGuard = source;
  auto* window = new MainWindow(demo, nullptr, m_nativeUi);
  if (!m_nativeUi) {
    // Floating chrome has a small layout minimum. Every new document needs
    // the same usable viewport as its source, not that minimum-sized shell.
    // Within the screen at a larger Interface size (mitcad#1495).
    window->resize(sourceGuard ? sourceGuard->window()->size() : startWindowSize());
  }
  window->m_windowManager = this;
  window->setProperty("documentWindowHandle", QStringLiteral("window-%1").arg(m_nextHandle++));
  m_windows.append(window);
  window->installEventFilter(this);
  connect(window, &QWidget::windowTitleChanged, this, &WindowManager::windowsChanged);
  connect(window, &QObject::destroyed, this, [this] {
    m_windows.removeIf([](const auto& entry) { return entry.isNull(); });
    emit windowsChanged();
  });
  installWindowMenu(window);
  emit windowAdded(window, sourceGuard, separate);
  emit windowsChanged();
  qInfo().noquote() << QStringLiteral("Document window created: %1 (%2 open)").arg(handle(window)).arg(m_windows.size());
  return window;
}

void WindowManager::setPresenter(std::function<void(MainWindow*)> presenter) { m_presenter = std::move(presenter); }

void WindowManager::present(MainWindow* window) {
  if (window == nullptr || !windows().contains(window)) return;
  setActiveWindow(window);
  if (m_presenter) m_presenter(window);
  else {
    if (!window->usesNativeUi() && window->isMinimized()) window->showNormal();
    window->present();
    if (!window->usesNativeUi()) { window->raise(); window->activateWindow(); }
  }
  qInfo().noquote() << QStringLiteral("Document window active: %1").arg(handle(window));
}

bool WindowManager::reusable(MainWindow* window) const {
  if (window == nullptr || window->modelBusy() || !window->m_filePath.isEmpty() || window->isModified()) return false;
  const QJsonObject document = window->queryObject({{QStringLiteral("query"), QStringLiteral("document")}});
  return document.value(QStringLiteral("features")).toInt() == 0 &&
         document.value(QStringLiteral("bodies")).toInt() == 0;
}

MainWindow* WindowManager::newWindow(MainWindow* source) {
  MainWindow* window = createWindow(false, source, true);
  present(window);
  return window;
}

MainWindow* WindowManager::newDocument(MainWindow* source) {
  const bool current = QSettings().value(QStringLiteral("documents/openInCurrentWindow"), false).toBool();
  if (source != nullptr && (reusable(source) || current)) {
    if (source->deferForNativeCommand([this, source] { newDocument(source); })) return nullptr;
    if (source->modelBusy()) {
      QPointer<MainWindow> waiting = source;
      source->whenIdle(this, [this, waiting] { if (waiting) newDocument(waiting); });
      return nullptr;
    }
    QPointer<MainWindow> guarded = source;
    if (!source->maybeSave() || !guarded || !guarded->installEmptyDocument(QString()) || !guarded) return nullptr;
    present(guarded);
    qInfo() << "New document";
    // The new design's own hint, as New gave it before several documents
    // (a new window shows its start hint itself).
    guarded->showHint(tr("Create a sketch to start modeling. S searches for commands."));
    return source;
  }
  MainWindow* window = createWindow(false, source, false);
  // The new window's untitled design is in the default units for new
  // designs (mitcad#1036), as a reused window's (installEmptyDocument).
  qInfo().noquote() << QStringLiteral("New design in %1").arg(GeneralSettings::load().newDesignUnits);
  present(window);
  qInfo() << "New document";
  return window;
}

MainWindow* WindowManager::findByPath(const QString& path) const {
  const QString key = fileKey(path);
  if (key.isEmpty()) return nullptr;
  for (MainWindow* window : windows()) if (fileKey(window->documentFilePath()) == key) return window;
  return nullptr;
}

MainWindow* WindowManager::findByHandle(const QString& wanted) const {
  for (MainWindow* window : windows()) if (handle(window) == wanted) return window;
  return nullptr;
}

MainWindow* WindowManager::windowForDocument(const QString& id) const {
  for (MainWindow* window : windows()) if (window->mcpDocumentId() == id) return window;
  return nullptr;
}

QString WindowManager::handle(MainWindow* window) const {
  return window != nullptr ? window->property("documentWindowHandle").toString() : QString();
}

MainWindow* WindowManager::openFile(MainWindow* source, const QString& path, QString& error, bool replaceAsked) {
  error.clear();
  if (MainWindow* existing = findByPath(path); existing != nullptr && !(replaceAsked && existing == source)) {
    present(existing);
    // Opened again while open: its linked files that changed since are
    // offered with Update (mitcad#1416); the design is not read again.
    existing->checkLinkedFiles();
    return existing;
  }
  const bool current =
      replaceAsked || QSettings().value(QStringLiteral("documents/openInCurrentWindow"), false).toBool();
  const bool readOnlyRequest = source != nullptr && source->m_openingReadOnly;
  const bool newDesignRequest = source != nullptr && source->m_openingNewDesign;
  QPointer<MainWindow> target = source;
  const bool created = target == nullptr || (!reusable(target) && !current);
  if (created) target = createWindow(false, source, false);
  else {
    if (target->modelBusy()) {
      QPointer<MainWindow> waiting = target;
      const bool readOnly = source != nullptr && source->m_openingReadOnly;
      const bool newDesign = source != nullptr && source->m_openingNewDesign;
      target->whenIdle(this, [this, waiting, path, readOnly, newDesign, replaceAsked] {
        if (!waiting) return;
        waiting->m_openingReadOnly = readOnly;
        waiting->m_openingNewDesign = newDesign;
        QString failure;
        if (!openFile(waiting, path, failure, replaceAsked) && waiting && !failure.isEmpty()) waiting->showError(failure);
        if (waiting) { waiting->m_openingReadOnly = false; waiting->m_openingNewDesign = false; }
      });
      return nullptr;
    }
    if (!replaceAsked && (!target->maybeSave() || !target)) return nullptr;
  }
  target->m_askImportChoices = true;
  target->m_openingNewDesign = newDesignRequest;
  target->m_openingReadOnly = readOnlyRequest;
  const bool opened = target->openFile(path, error);
  if (!target) return nullptr;
  target->m_askImportChoices = false;
  target->m_openingNewDesign = false;
  target->m_openingReadOnly = false;
  if (!opened) {
    if (created) { windowClosed(target); }
    return nullptr;
  }
  present(target);
  return target;
}

MainWindow* WindowManager::openFileBeside(MainWindow* source, const QString& path, QString& error) {
  error.clear();
  if (MainWindow* existing = findByPath(path)) {
    present(existing);
    existing->checkLinkedFiles();
    return existing;
  }
  QPointer<MainWindow> target = createWindow(false, source, false);
  const bool opened = target->openFile(path, error);
  if (!target) return nullptr;
  if (!opened) {
    windowClosed(target);
    return nullptr;
  }
  present(target);
  return target;
}

void WindowManager::designSaved(MainWindow* source, const QString& path) {
  for (MainWindow* window : windows()) {
    if (window != source) window->linkedDesignSaved(path);
  }
}

bool WindowManager::closeWindow(MainWindow* window) {
  if (window == nullptr || !windows().contains(window)) return true;
  QPointer<MainWindow> guarded = window;
  if (window->m_session) {
    connect(window->m_session, &CommandSession::cancelFinished, this, [this, guarded](bool succeeded) {
      if (!succeeded) cancelQuit(guarded);
    }, Qt::SingleShotConnection);
  }
  if (window->deferForNativeCommand([guarded] { if (guarded) guarded->closeDocumentWindow(); })) return false;
  return guarded && guarded->closeDocumentWindow();
}

void WindowManager::windowClosed(MainWindow* window) {
  bool found = false;
  for (auto it = m_windows.begin(); it != m_windows.end();) {
    if (it->isNull() || it->data() == window) { it = m_windows.erase(it); found = true; }
    else ++it;
  }
  if (!found) return;
  const bool wasActive = m_active == window;
  if (wasActive) m_active.clear();
  emit windowRemoved(window);
  emit windowsChanged();
  if (window != nullptr) window->deleteLater();
  if (windows().isEmpty()) {
    qInfo() << "Document windows closed: empty process registry";
    QTimer::singleShot(0, qApp, &QCoreApplication::quit);
  } else if (m_quitting) {
    if (!m_quitRunning) QTimer::singleShot(0, this, [this] { if (m_quitting) quit(); });
  } else if (wasActive) present(activeWindow());
}

void WindowManager::quit() {
  // A window-modal save/lock question runs a nested event loop. Closing
  // another document there must not start a second close of the document
  // whose question is still open. Pending asynchronous closes resume only
  // after this invocation has returned.
  if (m_quitRunning) return;
  const QScopedValueRollback<bool> running(m_quitRunning, true);
  m_quitting = true;
  QList<QPointer<MainWindow>> guarded;
  for (MainWindow* window : windows()) guarded.append(window);
  for (const auto& window : guarded) {
    // Another document's question can cancel Quit in that nested loop.
    if (!m_quitting) return;
    if (!window || !windows().contains(window)) continue;
    if (!closeWindow(window)) {
      const bool pending = window && (window->modelBusy() || window->m_closePending ||
                                      bool(window->m_nativeCommandContinuation));
      if (!pending) m_quitting = false; // a save or lock question was cancelled
      return;
    }
  }
  m_quitting = false;
}

bool WindowManager::eventFilter(QObject* object, QEvent* event) {
  if (auto* window = qobject_cast<MainWindow*>(object)) {
    if (event->type() == QEvent::WindowActivate && window->isWindow()) setActiveWindow(window);
    if (event->type() == QEvent::ModifiedChange) emit windowsChanged();
  }
  return QObject::eventFilter(object, event);
}

void WindowManager::installWindowMenu(MainWindow* window) {
  QMenu* menu = window->menuBar()->findChild<QMenu*>(QStringLiteral("windowMenu"));
  if (menu == nullptr) {
    menu = new QMenu(tr("&Window"), window->menuBar());
    const auto menus = window->menuBar()->actions();
    if (menus.isEmpty()) window->menuBar()->addMenu(menu);
    else window->menuBar()->insertMenu(menus.last(), menu);
    menu->setObjectName(QStringLiteral("windowMenu"));
  }
  menu->clear();
  const auto rebuild = [this, window, menu] {
    menu->clear();
    menu->addAction(window->m_registry->action(QStringLiteral("window.new")));
    auto* minimize = menu->addAction(tr("Minimize"));
    minimize->setShortcut(QKeySequence(Qt::CTRL | Qt::Key_M));
    connect(minimize, &QAction::triggered, window, [window] { window->minimizeDocumentWindow(); });
    menu->addSeparator();
    for (MainWindow* document : windows()) {
      QAction* action = menu->addAction(document->documentName());
      action->setCheckable(true);
      action->setChecked(document == activeWindow());
      action->setProperty("mitcadNativeActionId", QStringLiteral("window.activate.") + handle(document));
      connect(action, &QAction::triggered, this, [this, document] { present(document); });
    }
#ifdef MITCAD_MAC_UI
    if (!window->usesNativeUi()) mac::addDocumentWindowTabActions(menu, window);
#endif
#ifdef MITCAD_MACOS_NATIVE_UI
    if (window->usesNativeUi()) window->refreshNativeWindow();
#endif
  };
  connect(menu, &QMenu::aboutToShow, window, rebuild);
  connect(this, &WindowManager::windowsChanged, window, [window, rebuild] {
    // Activation may come from one of the menu's own actions. Replace its
    // list after that QAction has finished dispatching, not underneath it.
    QTimer::singleShot(0, window, rebuild);
  });
  rebuild();
}
} // namespace mitcad
