// SPDX-License-Identifier: MIT
#include "DocumentFrame.hpp"
#include "MainWindow.hpp"
#include "WindowManager.hpp"
#include "framework/Dialogs.hpp"
#include "framework/InterfaceSize.hpp"

#include <QApplication>
#include <QCloseEvent>
#include <QDesktopServices>
#include <QCursor>
#include <QDrag>
#include <QDragEnterEvent>
#include <QDropEvent>
#include <QFileInfo>
#include <QKeyEvent>
#include <QMenu>
#include <QMenuBar>
#include <QMimeData>
#include <QMouseEvent>
#include <QSettings>
#include <QStackedWidget>
#include <QTabBar>
#include <QTimer>
#include <QUrl>
#include <QVBoxLayout>
#include <memory>

namespace mitcad {
namespace {
constexpr char kDocumentMime[] = "application/x-mitcad-document-window";

class DocumentTabs final : public QTabBar {
public:
  DocumentTabs(DocumentFrame& frame, WindowManager& manager) : m_frame(frame), m_manager(&manager) {
    setObjectName(QStringLiteral("documentTabs"));
    setMovable(true);
    setTabsClosable(true);
    setExpanding(false);
    setElideMode(Qt::ElideMiddle);
    setAcceptDrops(true);
  }
protected:
  bool eventFilter(QObject* watched, QEvent* event) override {
    if (event->type() == QEvent::KeyPress && static_cast<QKeyEvent*>(event)->key() == Qt::Key_Escape) m_dragCancelled = true;
    return QTabBar::eventFilter(watched, event);
  }
  void mousePressEvent(QMouseEvent* event) override {
    const int index = tabAt(event->position().toPoint());
    const auto pages = m_frame.documents();
    m_pressedDocument = index >= 0 && index < pages.size() ? pages[index] : nullptr;
    m_start = event->position().toPoint();
    QTabBar::mousePressEvent(event);
  }
  void mouseReleaseEvent(QMouseEvent* event) override {
    if (event->button() == Qt::MiddleButton) {
      const int index = tabAt(event->position().toPoint());
      if (index >= 0) emit tabCloseRequested(index);
      event->accept();
      return;
    }
    m_pressedDocument.clear();
    QTabBar::mouseReleaseEvent(event);
  }
  void mouseMoveEvent(QMouseEvent* event) override {
    // Reordering remains QTabBar's ordinary drag. Crossing the bar's outer
    // margin starts a transferable drag that may end in another frame.
    if (m_pressedDocument && (event->buttons() & Qt::LeftButton) &&
        !rect().adjusted(-16, -16, 16, 16).contains(event->position().toPoint()) &&
        (event->position().toPoint() - m_start).manhattanLength() >= QApplication::startDragDistance()) {
      QPointer<MainWindow> document = m_pressedDocument;
      m_pressedDocument.clear();
      if (!m_manager || DocumentFrame::containing(document) != &m_frame) return;
      // End QTabBar's internal move before entering QDrag's nested loop.
      QMouseEvent release(QEvent::MouseButtonRelease, event->position(), event->globalPosition(),
                          Qt::LeftButton, Qt::NoButton, event->modifiers());
      QTabBar::mouseReleaseEvent(&release);
      auto* mime = new QMimeData;
      mime->setData(kDocumentMime, m_manager->handle(document).toUtf8());
      m_dragCancelled = false;
      QPointer<DocumentTabs> self = this;
      QPointer<DocumentFrame> frame = &m_frame;
      frame->setProperty("documentTabDragActive", true);
      frame->setAttribute(Qt::WA_DeleteOnClose, false);
      qApp->installEventFilter(this);
      // QDrag keeps its source for event routing, but the nested tracking
      // loop must not let that source's QObject parent delete a stack drag.
      auto drag = std::make_unique<QDrag>(this);
      drag->setParent(nullptr);
      drag->setMimeData(mime);
      const auto result = drag->exec(Qt::MoveAction);
      if (!self || !frame) return;
      qApp->removeEventFilter(self);
      frame->setProperty("documentTabDragActive", false);
      frame->setAttribute(Qt::WA_DeleteOnClose, true);
      if (result == Qt::IgnoreAction && document && !self->m_dragCancelled && self->m_manager) {
        // Escape cancels; a release outside all tab bars detaches.
        if (!(QApplication::keyboardModifiers() & Qt::ControlModifier) &&
            !QApplication::mouseButtons().testFlag(Qt::LeftButton)) {
          frame->moveDocument(document);
        }
      }
      if (frame && (!self || !self->m_manager || frame->documents().isEmpty())) frame->deleteLater();
      return;
    }
    QTabBar::mouseMoveEvent(event);
  }
  void dragEnterEvent(QDragEnterEvent* event) override {
    if (m_manager && m_manager->findByHandle(QString::fromUtf8(event->mimeData()->data(kDocumentMime)))) {
      event->setDropAction(Qt::MoveAction);
      event->accept();
    }
  }
  void dragMoveEvent(QDragMoveEvent* event) override {
    if (event->mimeData()->hasFormat(kDocumentMime)) event->acceptProposedAction();
  }
  void dropEvent(QDropEvent* event) override {
    if (!m_manager) return;
    auto* document = m_manager->findByHandle(QString::fromUtf8(event->mimeData()->data(kDocumentMime)));
    if (!document) return;
    if (auto* source = DocumentFrame::containing(document)) {
      source->moveDocument(document, &m_frame, tabAt(event->position().toPoint()));
      event->setDropAction(Qt::MoveAction);
      event->accept();
    }
  }
private:
  DocumentFrame& m_frame;
  QPointer<WindowManager> m_manager;
  QPoint m_start;
  QPointer<MainWindow> m_pressedDocument;
  bool m_dragCancelled = false;
};
}

DocumentFrame::DocumentFrame(WindowManager& manager)
    : m_manager(&manager), m_tabs(new DocumentTabs(*this, manager)), m_stack(new QStackedWidget) {
  // Frames are presentation siblings, but their event filters and manager
  // reference must end before QApplication outlives its document manager.
  connect(&manager, &QObject::destroyed, this, [this] {
    if (property("documentTabDragActive").toBool()) {
      QDrag::cancel();
      hide(); // the guarded drag continuation finishes deleting this frame
    } else delete this;
  });
  setObjectName(QStringLiteral("documentFrame"));
  setAttribute(Qt::WA_DeleteOnClose);
  auto* root = new QWidget;
  auto* layout = new QVBoxLayout(root);
  layout->setContentsMargins(0, 0, 0, 0);
  layout->setSpacing(0);
  layout->addWidget(m_tabs);
  layout->addWidget(m_stack, 1);
  setCentralWidget(root);
  // Within the screen at a larger Interface size (mitcad#1495).
  resize(startWindowSize());
  connect(m_tabs, &QTabBar::currentChanged, this, [this](int index) {
    m_stack->setCurrentIndex(index);
    updateActiveDocument();
  });
  connect(m_tabs, &QTabBar::tabMoved, this, [this](int from, int to) {
    QWidget* page = m_stack->widget(from);
    m_stack->removeWidget(page);
    m_stack->insertWidget(to, page);
    m_stack->setCurrentIndex(m_tabs->currentIndex());
    updateActiveDocument();
  });
  connect(m_tabs, &QTabBar::tabCloseRequested, this, [this](int index) {
    if (auto* document = qobject_cast<MainWindow*>(m_stack->widget(index)); document && m_manager) m_manager->closeWindow(document);
  });
  m_tabs->setContextMenuPolicy(Qt::CustomContextMenu);
  connect(m_tabs, &QWidget::customContextMenuRequested, this, [this](const QPoint& point) {
    showTabMenu(m_tabs->tabAt(point), m_tabs->mapToGlobal(point));
  });
  connect(&manager, &WindowManager::windowRemoved, this, [this](MainWindow* document) {
    const auto before = m_pendingClose.size();
    m_pendingClose.removeIf([document](const auto& page) { return !page || page == document; });
    if (before != m_pendingClose.size()) scheduleClosing();
  });
  connect(&manager, &WindowManager::closeCancelled, this, [this](MainWindow* document) {
    if (m_pendingClose.contains(document)) m_pendingClose.clear();
  });
  qApp->installEventFilter(this);
}

void DocumentFrame::install(WindowManager& manager) {
  connect(&manager, &WindowManager::windowAdded, &manager,
          [](MainWindow* document, MainWindow*, bool separate) {
    document->setProperty("mitcadSeparateWindow", separate);
  });
#ifdef MITCAD_MAC_UI
  // Exercise the portable frame in a focused Qt test without changing the
  // macOS product default or introducing a user-facing alternate tab bar.
  if (qEnvironmentVariableIntValue("MITCAD_TEST_DOCUMENT_FRAMES") != 1 ||
      qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT")) return;
#endif
  connect(&manager, &WindowManager::windowAdded, &manager,
          [&manager](MainWindow* document, MainWindow* source, bool separate) {
    if (document->usesNativeUi()) return;
    DocumentFrame* frame = !separate ? containing(source) : nullptr;
    const bool tabs = QSettings().value(QStringLiteral("documents/openDesignsIn"), QStringLiteral("tabs")).toString() != QStringLiteral("windows");
    if (!tabs || separate) frame = nullptr;
    if (!frame) frame = new DocumentFrame(manager);
    frame->addDocument(document);
  });
  connect(&manager, &WindowManager::windowRemoved, &manager, [](MainWindow* document) {
    if (auto* frame = containing(document)) frame->removeDocument(document);
  });
  manager.setPresenter([](MainWindow* document) {
    if (auto* frame = containing(document)) {
      frame->activateDocument(document);
      frame->show();
      frame->raise();
      frame->activateWindow();
    } else document->present();
  });
}

DocumentFrame* DocumentFrame::containing(MainWindow* document) {
  for (QWidget* parent = document ? document->parentWidget() : nullptr; parent; parent = parent->parentWidget()) {
    if (auto* frame = qobject_cast<DocumentFrame*>(parent)) return frame;
  }
  return nullptr;
}

QList<MainWindow*> DocumentFrame::documents() const {
  QList<MainWindow*> result;
  for (int index = 0; index < m_stack->count(); ++index) result.append(qobject_cast<MainWindow*>(m_stack->widget(index)));
  return result;
}
MainWindow* DocumentFrame::activeDocument() const { return qobject_cast<MainWindow*>(m_stack->currentWidget()); }

void DocumentFrame::addDocument(MainWindow* document, int index) {
  if (!document || m_stack->indexOf(document) >= 0) return;
  if (auto* frame = containing(document)) frame->removeDocument(document);
  document->setParent(m_stack, Qt::Widget);
  document->menuBar()->setNativeMenuBar(false);
  // updateChrome shows its own menu bar again while it is alone here.
  document->menuBar()->hide();
  index = index < 0 ? m_stack->count() : qBound(0, index, m_stack->count());
  m_stack->insertWidget(index, document);
  m_tabs->insertTab(index, QString());
  document->installEventFilter(this);
  connect(document, &QWidget::windowTitleChanged, this, [this, document] { updateDocument(document); });
  updateDocument(document);
  activateDocument(document);
}
void DocumentFrame::removeDocument(MainWindow* document) {
  const int index = m_stack->indexOf(document);
  if (index < 0) return;
  document->removeEventFilter(this);
  disconnect(document, nullptr, this, nullptr);
  document->setDocumentVisible(false);
  m_stack->removeWidget(document);
  m_tabs->removeTab(index);
  if (m_active == document) m_active.clear();
  updateActiveDocument();
  if (!m_stack->count()) {
    hide();
    if (!property("documentTabDragActive").toBool()) deleteLater();
  }
}
void DocumentFrame::activateDocument(MainWindow* document) {
  const int index = m_stack->indexOf(document);
  if (index < 0) return;
  m_tabs->setCurrentIndex(index);
  m_stack->setCurrentIndex(index);
  updateActiveDocument();
  QWidget* focus = QApplication::focusWidget();
  if (!focus || (focus != document && !document->isAncestorOf(focus))) document->viewerWidget()->setFocus(Qt::OtherFocusReason);
}
void DocumentFrame::updateDocument(MainWindow* document) {
  const int index = m_stack->indexOf(document);
  if (index < 0) return;
  const QString path = document->documentFilePath();
  QString title = path.isEmpty() ? tr("Untitled") : QFileInfo(path).completeBaseName();
  if (document->isWindowModified()) title += QStringLiteral(" •");
  m_tabs->setTabText(index, title);
  m_tabs->setTabToolTip(index, path);
  if (document == activeDocument()) {
    setWindowTitle(document->windowTitle());
    setWindowModified(document->isWindowModified());
  }
}
void DocumentFrame::updateActiveDocument() {
  MainWindow* document = activeDocument();
  for (MainWindow* page : documents()) page->setDocumentVisible(page == document);
  m_active = document;
  menuBar()->clear();
  updateChrome();
  if (!document) return;
  for (QAction* action : document->menuBar()->actions()) menuBar()->addAction(action);
  setWindowTitle(document->windowTitle());
  setWindowModified(document->isWindowModified());
  if (m_manager) m_manager->setActiveWindow(document);
}
bool DocumentFrame::showsDocumentTabs(int documentCount) { return documentCount >= 2; }
void DocumentFrame::updateChrome() {
  // With the tab bar the frame's menu bar follows the shown document above
  // the tabs; without it the one document keeps its own menu bar, so its
  // window coordinates are the frame's and its layout is unchanged.
  const bool tabs = showsDocumentTabs(int(m_stack->count()));
  m_tabs->setVisible(tabs);
  menuBar()->setVisible(tabs);
  const MainWindow* shown = activeDocument();
  // The system menu bar holds the frame's menus (a native menu bar); the
  // document's in-window copy then follows the preference.
  const bool inWindow = !menuBar()->isNativeMenuBar() || windowMenuBarShown();
  for (MainWindow* page : documents()) page->menuBar()->setVisible(!tabs && page == shown && inWindow);
}
namespace {
QString windowMenuBarKey() { return QStringLiteral("view/windowMenuBar"); }
} // namespace
bool DocumentFrame::windowMenuBarShown() {
#ifdef Q_OS_MACOS
  return QSettings().value(windowMenuBarKey(), false).toBool();
#else
  return true;
#endif
}
bool DocumentFrame::setWindowMenuBarShown(bool shown) {
  if (windowMenuBarShown() == shown) return false;
  QSettings().setValue(windowMenuBarKey(), shown);
  for (QWidget* top : QApplication::topLevelWidgets()) {
    if (auto* frame = qobject_cast<DocumentFrame*>(top)) frame->updateChrome();
  }
  return true;
}
void DocumentFrame::moveDocument(MainWindow* document, DocumentFrame* destination, int index) {
  if (!m_manager || !document || containing(document) != this) return;
  if (destination == this) {
    const int from = m_stack->indexOf(document);
    if (index >= 0 && index != from) m_tabs->moveTab(from, index);
    return;
  }
  if (!destination) {
    destination = new DocumentFrame(*m_manager);
    destination->resize(size());
    destination->move(QCursor::pos());
  }
  removeDocument(document);
  destination->addDocument(document, index);
  destination->show();
  destination->raise();
  destination->activateWindow();
}
void DocumentFrame::closeDocuments(const QList<MainWindow*>& pages) {
  m_pendingClose.clear();
  for (MainWindow* page : pages) m_pendingClose.append(page);
  continueClosing();
}
void DocumentFrame::scheduleClosing() {
  if (m_closePosted || m_pendingClose.isEmpty()) return;
  m_closePosted = true;
  QTimer::singleShot(0, this, [this] { m_closePosted = false; continueClosing(); });
}
void DocumentFrame::continueClosing() {
  if (m_continuingClose || !m_manager) return;
  while (!m_pendingClose.isEmpty() &&
         (!m_pendingClose.front() || !m_manager->windows().contains(m_pendingClose.front()))) m_pendingClose.removeFirst();
  if (m_pendingClose.isEmpty()) return;
  const QPointer<MainWindow> document = m_pendingClose.front();
  QPointer<DocumentFrame> frame = this;
  m_continuingClose = true;
  const bool accepted = m_manager->closeWindow(document);
  if (!frame) return;
  m_continuingClose = false;
  if (accepted) {
    m_pendingClose.removeAll(document);
    scheduleClosing();
  }
  // A pending command/job resumes through windowRemoved; a declined save
  // or lock question clears this batch through closeCancelled.
}
void DocumentFrame::showTabMenu(int index, const QPoint& point) {
  auto* document = qobject_cast<MainWindow*>(m_stack->widget(index));
  if (!document) return;
  QPointer<MainWindow> guard = document;
  QPointer<DocumentFrame> frame = this;
  QMenu menu;
  QAction* close = menu.addAction(tr("Close"));
  QAction* others = menu.addAction(tr("Close Others"));
  QAction* all = menu.addAction(tr("Close All"));
  menu.addSeparator();
  QAction* folder = menu.addAction(tr("Show in Folder"));
  folder->setEnabled(!document->documentFilePath().isEmpty());
  QAction* detach = menu.addAction(tr("Move to New Window"));
  QAction* chosen = execMenu(&menu, point);
  if (!frame || !guard) return;
  if (chosen == close) { if (m_manager) m_manager->closeWindow(document); }
  else if (chosen == others) { auto pages = documents(); pages.removeAll(document); closeDocuments(pages); }
  else if (chosen == all) closeDocuments(documents());
  else if (chosen == folder) QDesktopServices::openUrl(QUrl::fromLocalFile(QFileInfo(document->documentFilePath()).absolutePath()));
  else if (chosen == detach) moveDocument(document);
}
void DocumentFrame::closeEvent(QCloseEvent* event) {
  if (documents().isEmpty()) { event->accept(); return; }
  event->ignore();
  closeDocuments(documents());
}
bool DocumentFrame::eventFilter(QObject* watched, QEvent* event) {
  if (auto* document = qobject_cast<MainWindow*>(watched)) {
    if (event->type() == QEvent::ModifiedChange) updateDocument(document);
  }
  if (m_manager && event->type() == QEvent::WindowActivate && watched == this && activeDocument()) m_manager->setActiveWindow(activeDocument());
  // Ctrl+Tab and Ctrl+W act on the tabs only while they are shown: with one
  // document Ctrl+W is File › Close (an empty Untitled design stays).
  if ((event->type() == QEvent::KeyPress || event->type() == QEvent::ShortcutOverride) &&
      showsDocumentTabs(int(m_stack->count()))) {
    auto* widget = qobject_cast<QWidget*>(watched);
    auto* key = static_cast<QKeyEvent*>(event);
    const auto modifiers = key->modifiers();
    const bool tab = key->key() == Qt::Key_Tab || key->key() == Qt::Key_Backtab;
    const bool close = key->key() == Qt::Key_W;
    if (widget && widget->window() == this && (modifiers & Qt::ControlModifier) &&
        !(modifiers & (Qt::AltModifier | Qt::MetaModifier)) && (tab || close)) {
      if (event->type() == QEvent::KeyPress) {
        if (close) { if (auto* document = activeDocument(); document && m_manager) m_manager->closeWindow(document); }
        else if (m_tabs->count()) {
          const int delta = modifiers & Qt::ShiftModifier ? -1 : 1;
          m_tabs->setCurrentIndex((m_tabs->currentIndex() + delta + m_tabs->count()) % m_tabs->count());
        }
      }
      event->accept();
      return true;
    }
  }
  return QMainWindow::eventFilter(watched, event);
}
} // namespace mitcad
