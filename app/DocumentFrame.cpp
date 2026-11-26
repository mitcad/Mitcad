// SPDX-License-Identifier: MIT
#include "DocumentFrame.hpp"
#include "MainWindow.hpp"
#include "WindowManager.hpp"
#include "framework/Dialogs.hpp"
#include "framework/FitToScreen.hpp"
#include "framework/Icons.hpp"
#include "framework/InterfaceSize.hpp"
#include "framework/MenuBarReveal.hpp"
#include "framework/PlaceWatcher.hpp"
#include "framework/Theme.hpp"
#include "framework/TitleBar.hpp"
#include "framework/UiSizes.hpp"

#include <QAbstractButton>
#include <QApplication>
#include <QCloseEvent>
#include <QDesktopServices>
#include <QCursor>
#include <QDrag>
#include <QDragEnterEvent>
#include <QtLogging>
#include <QDropEvent>
#include <QEnterEvent>
#include <QFileInfo>
#include <QKeyEvent>
#include <QMenu>
#include <QMenuBar>
#include <QMimeData>
#include <QMouseEvent>
#include <QPainter>
#include <QPixmap>
#include <QPointer>
#include <QSettings>
#include <QStackedWidget>
#include <QStyle>
#include <QStyleOptionTab>
#include <QTabBar>
#include <QTimer>
#include <QUrl>
#include <QVBoxLayout>
#include <algorithm>
#include <memory>

namespace mitcad {
namespace {
constexpr char kDocumentMime[] = "application/x-mitcad-document-window";

// The × of a design's tab in the top bar's row (mitcad#2426): the shared
// close mark (`dismiss`), on the shown design's accent block in the color
// for text on the accent, elsewhere in the outline's; a round tint under the
// pointer. The style's own close button drew its dark mark on the accent.
class RowTabClose final : public QAbstractButton {
public:
  explicit RowTabClose(QTabBar* bar) : QAbstractButton(bar), m_bar(bar) {
    setObjectName(QStringLiteral("documentTabClose"));
    setFocusPolicy(Qt::NoFocus);
    setAttribute(Qt::WA_Hover);
    // Named as the style's close button of a tab ("Close Tab"): never a bare
    // "Close", which is a command panel's button (the UI tests' `button
    // '^Close$'` takes a button without text by its accessible name).
    setToolTip(DocumentFrame::tr("Close the design"));
    setAccessibleName(DocumentFrame::tr("Close Tab"));
    resize(sizeHint());
    connect(this, &QAbstractButton::clicked, this, [this] {
      const int index = tabIndex();
      if (m_bar != nullptr && index >= 0) emit m_bar->tabCloseRequested(index);
    });
  }
  QSize sizeHint() const override {
    const int side = uiIconSize(18);
    return {side, side};
  }

protected:
  void paintEvent(QPaintEvent*) override {
    if (m_bar == nullptr) return;
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QPalette pal = m_bar->palette();
    const bool shown = tabIndex() == m_bar->currentIndex();
    const QColor onAccent = themeColor(ColorRole::AccentText, pal);
    if (underMouse() || isDown()) {
      QColor fill = shown ? onAccent : themeColor(ColorRole::Text, pal);
      fill.setAlpha(isDown() ? 70 : 40);
      painter.setPen(Qt::NoPen);
      painter.setBrush(fill);
      painter.drawEllipse(QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5));
    }
    const int side = uiIconSize(14);
    const QRect mark((width() - side) / 2, (height() - side) / 2, side, side);
    QPixmap pixmap = themeIcon(QStringLiteral("dismiss")).pixmap(QSize(side, side), devicePixelRatioF());
    if (shown) {
      QPainter tint(&pixmap);
      tint.setCompositionMode(QPainter::CompositionMode_SourceIn);
      tint.fillRect(QRectF(0, 0, pixmap.width(), pixmap.height()), onAccent);
    }
    painter.drawPixmap(mark, pixmap);
  }
  void enterEvent(QEnterEvent* event) override {
    QAbstractButton::enterEvent(event);
    update();
  }
  void leaveEvent(QEvent* event) override {
    QAbstractButton::leaveEvent(event);
    update();
  }

private:
  int tabIndex() const {
    if (m_bar == nullptr) return -1;
    for (int i = 0; i < m_bar->count(); ++i) {
      if (m_bar->tabButton(i, QTabBar::RightSide) == this) return i;
    }
    return -1;
  }
  QPointer<QTabBar> m_bar;
};

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
  // In a Floating title row (mitcad#1990): each design a flat tab on the
  // bar, its name (a • after it while edited) and its ×, the shown one on a
  // block of the accent, as the toolbar panel's shown tab (mitcad#2426: a
  // tint of the text read as little more than the hovered tab's fill),
  // without the style's base line; else the style's tabs in the frame's
  // own row.
  void setRowStyle(bool row) {
    if (row == m_rowStyle) return;
    m_rowStyle = row;
    setDrawBase(!row);
    // The row's × (RowTabClose) instead of the style's, and back.
    if (row) {
      for (int i = 0; i < count(); ++i) useRowClose(i);
    } else {
      useStyleClose();
    }
    // In the row the tabs share its room with the other controls: when it
    // is short they are narrowed to their names elided (minimumTabSizeHint),
    // every design's tab in sight, rather than scrolled with arrows, which
    // left the first tabs out of sight beside the shown one. The style's
    // own choice again in the frame's row.
    if (row) {
      m_frameScrollButtons = usesScrollButtons();
    }
    setUsesScrollButtons(row ? false : m_frameScrollButtons);
    if (row) {
      // The tab under the pointer highlighted (paintEvent).
      setAttribute(Qt::WA_Hover);
      setMouseTracking(true);
    } else {
      setMinimumHeight(0);
      setMaximumHeight(QWIDGETSIZE_MAX);
    }
    updateGeometry();
    update();
  }
protected:
  QSize tabSizeHint(int index) const override {
    if (!m_rowStyle) return QTabBar::tabSizeHint(index);
    // The name, the room around it and the close button's; long names
    // elided in the middle at 240 px. The name as wide as in semibold, as
    // the shown design's is painted (paintEvent): measured in the regular
    // weight only, it was elided in its own tab where the semibold face is
    // wider ("Un…ed" in Segoe UI Variable, the Docked layout's font,
    // mitcad#2082), and a tab keeps its width when it is shown.
    // As high as the bar (TitleBar::hostDocumentTabs fixes its height to
    // the buttons' beside it), so that the name, the close button and the
    // block share the bar's centre line with Home, File, Undo and Redo.
    return QSize(std::min(nameRoom(index), 240) + closeRoom(index),
                 std::max(fontMetrics().height() + 10, minimumHeight()));
  }
  // How narrow a tab may be squeezed when the row is short. A short name
  // (whole at its full width) keeps that width while the long names,
  // narrowed to their least, leave it room in the bar: the long name gives
  // up the room and short names are not elided (mitcad#2060; QTabBar shared
  // the shortfall between the tabs, and "Untitled" lost 2 px beside a long
  // name that still had 76 px to give, in the Windows VM's 1028 px window).
  // Only when even that does not fit are short names narrowed too.
  QSize minimumTabSizeHint(int index) const override {
    if (!m_rowStyle) return QTabBar::minimumTabSizeHint(index);
    const QSize full = tabSizeHint(index);
    if (nameRoom(index) <= 240) {
      int least = 0;
      for (int i = 0; i < count(); ++i) {
        least += nameRoom(i) <= 240 ? tabSizeHint(i).width() : squeezedWidth(i);
      }
      if (least <= width()) return full;
    }
    return QSize(squeezedWidth(index), full.height());
  }
  // The name's width in the wider of its two weights and the room around
  // it; the close button's room beside it; the least width of a tab.
  int nameRoom(int index) const {
    return std::max(QFontMetrics(nameFont(false)).horizontalAdvance(tabText(index)),
                    QFontMetrics(nameFont(true)).horizontalAdvance(tabText(index))) +
           24;
  }
  int closeRoom(int index) const {
    const QWidget* close = tabButton(index, QTabBar::RightSide);
    return close != nullptr ? close->sizeHint().width() + 6 : 0;
  }
  int squeezedWidth(int index) const {
    return std::min(std::min(nameRoom(index), 240) + closeRoom(index), 64 + closeRoom(index));
  }
  void leaveEvent(QEvent* event) override {
    QTabBar::leaveEvent(event);
    if (m_rowStyle) update();
  }
  void paintEvent(QPaintEvent* event) override {
    QStringList shown;
    if (!m_rowStyle) {
      QTabBar::paintEvent(event);
      // The names as the style drew them (QTabBar elides them in its
      // style option).
      for (int i = 0; i < count(); ++i) shown.append(shownName(i));
      logShownNames(shown);
      return;
    }
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QPalette pal = palette();
    QColor text = pal.color(QPalette::WindowText);
    QColor quiet = text;
    quiet.setAlpha(190);
    const int hovered = tabAt(mapFromGlobal(QCursor::pos()));
    for (int i = 0; i < count(); ++i) {
      const QRect tab = tabRect(i);
      const QRectF block = rowBlock(tab);
      const bool selected = i == currentIndex();
      if (selected || (i == hovered && isEnabled())) {
        // The shown design a flat block of the accent (the chosen tab of
        // docs/ui-conventions.md), no line round it; a hovered one on the
        // chrome's hover fill.
        painter.setPen(Qt::NoPen);
        painter.setBrush(selected ? accentColor(pal) : themeColor(ColorRole::HoverFill, pal));
        painter.drawRoundedRect(block, kControlRadius, kControlRadius);
      }
      // The name left of the close button, elided in the middle; on the
      // accent in the color for text on it.
      const QRect name = nameRect(i);
      painter.setFont(nameFont(selected));
      painter.setPen(selected ? themeColor(ColorRole::AccentText, pal) : quiet);
      const QString elided = shownName(i);
      painter.drawText(name, Qt::AlignVCenter | Qt::AlignLeft, elided);
      shown.append(elided);
    }
    logShownNames(shown);
  }
  // The names as shown, for the UI tests (mitcad#2060): logged when they
  // change, e.g. "Document tabs shown: 'Bracket' 'Untitled'".
  void logShownNames(const QStringList& shown) {
    if (shown == m_loggedNames) return;
    m_loggedNames = shown;
    QString line = QStringLiteral("Document tabs shown:");
    for (const QString& name : shown) line += QStringLiteral(" '%1'").arg(name);
    qDebug().noquote() << line;
  }
  // A tab's name as it is drawn: in the row elided in the middle to the
  // room left of its close button, in the shown weight; else as the style
  // elides it in its option.
  QString shownName(int index) const {
    if (!m_rowStyle) {
      QStyleOptionTab option;
      initStyleOption(&option, index);
      return option.text;
    }
    return QFontMetrics(nameFont(index == currentIndex()))
        .elidedText(tabText(index), Qt::ElideMiddle, nameRect(index).width());
  }
  // Where a tab's name is painted in the row: left of its close button.
  QRect nameRect(int index) const {
    const QRect tab = tabRect(index);
    const QWidget* close = tabButton(index, QTabBar::RightSide);
    const int right = close != nullptr && !close->isHidden() ? close->geometry().left() - 4 : tab.right() - 10;
    return QRect(tab.left() + 12, tab.top(), std::max(0, right - tab.left() - 12), tab.height());
  }
  // A tab inserted in the row gets the row's × too; also once the tab bar
  // is done with it, should it add the style's button after this.
  void tabInserted(int index) override {
    QTabBar::tabInserted(index);
    if (!m_rowStyle) return;
    useRowClose(index);
    QTimer::singleShot(0, this, [this] {
      if (!m_rowStyle) return;
      for (int i = 0; i < count(); ++i) useRowClose(i);
    });
  }
  // The row's × at the right of a tab (where the row's name room expects
  // it, nameRect), the style's button gone from its side.
  void useRowClose(int index) {
    if (dynamic_cast<RowTabClose*>(tabButton(index, QTabBar::RightSide)) != nullptr) return;
    const auto side =
        static_cast<QTabBar::ButtonPosition>(style()->styleHint(QStyle::SH_TabBar_CloseButtonPosition, nullptr, this));
    for (const QTabBar::ButtonPosition at : {side, QTabBar::RightSide}) {
      if (QWidget* own = tabButton(index, at)) {
        setTabButton(index, at, nullptr);
        own->deleteLater();
      }
    }
    setTabButton(index, QTabBar::RightSide, new RowTabClose(this));
  }
  // The style's close buttons again (the frame's own row): the row's go,
  // the tab bar makes its own for every tab.
  void useStyleClose() {
    for (int i = 0; i < count(); ++i) {
      if (QWidget* mine = dynamic_cast<RowTabClose*>(tabButton(i, QTabBar::RightSide))) {
        setTabButton(i, QTabBar::RightSide, nullptr);
        mine->deleteLater();
      }
    }
    setTabsClosable(false);
    setTabsClosable(true);
  }
  // The shown design's name in semibold.
  QFont nameFont(bool selected) const {
    QFont font = this->font();
    font.setWeight(selected ? QFont::DemiBold : QFont::Normal);
    return font;
  }
public:
  // The names as they are drawn, logged also where the bar is never
  // painted (Windows session 0, where paintEvent's line never came): from
  // DocumentFrame::logShownTab as the tabs change or move.
  void logNamesAsShown() {
    QStringList shown;
    for (int i = 0; i < count(); ++i) shown.append(shownName(i));
    logShownNames(shown);
  }
  // The tabs in the row for the UI tests (mitcad#2082): each one's width,
  // the width it asks for, and whether its name is elided.
  QString rowWidths() const {
    if (!m_rowStyle) return {};
    QStringList tabs;
    for (int i = 0; i < count(); ++i) {
      const bool elided =
          QFontMetrics(nameFont(i == currentIndex())).horizontalAdvance(tabText(i)) > nameRect(i).width();
      tabs << QStringLiteral("%1 %2 of %3 elided %4")
                  .arg(tabText(i))
                  .arg(tabRect(i).width())
                  .arg(tabSizeHint(i).width())
                  .arg(elided ? QStringLiteral("yes") : QStringLiteral("no"));
    }
    return QStringLiteral("Title row tabs: bar %1; %2").arg(width()).arg(tabs.join(QStringLiteral("; ")));
  }
protected:
  // The shown or hovered tab's block in the row: as high as the row
  // buttons' hover fill (the icon and 10, less a pixel at each edge),
  // centred on the bar's centre line like them.
  QRectF rowBlock(const QRect& tab) const {
    const qreal high = std::min<qreal>(tab.height() - 2, uiIconSize(18) + 8);
    const qreal top = tab.top() + (tab.height() - high) / 2.0;
    return QRectF(tab.left() + 1.5, top, tab.width() - 3.0, high);
  }
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
  bool m_rowStyle = false;
  QStringList m_loggedNames; // logShownNames' last names
  bool m_frameScrollButtons = true; // the style's, kept while in a title row
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
  m_root = root;
  m_rootLayout = layout;
  // Its menu bar (with the tabs) hidden as the documents' own while
  // Preferences › Display › Show the menu bar is cleared (mitcad#1620).
  MenuBarReveal::instance().watch(menuBar());
  // Within the screen at a larger Interface size (mitcad#1495).
  resize(startWindowSize());
  connect(m_tabs, &QTabBar::currentChanged, this, [this](int index) {
    m_stack->setCurrentIndex(index);
    updateActiveDocument();
    logShownTab();
  });
  // A click on the shown design's tab leaves the Start page for the
  // design: in the top bar's title row (mitcad#1990, both layouts since
  // mitcad#2082; the owner, 17.11.2026: the Start page has no workspace
  // panel to leave it by) and in the macOS Docked tab row under the menu
  // bar, which hides its toolbar over the page while the tabs show
  // (MainWindow::updateStartToolbar).
  connect(m_tabs, &QTabBar::tabBarClicked, this, [this](int index) {
    if (index >= 0 && index == m_tabs->currentIndex()) {
      if (MainWindow* document = activeDocument()) {
        document->leaveStartPageForDesign();
      }
    }
  });
  // Where the shown design's tab is, for the UI tests (ui_click_logged).
  new PlaceWatcher(m_tabs, [this] { logShownTab(); });
  connect(m_tabs, &QTabBar::tabMoved, this, [this](int from, int to) {
    QWidget* page = m_stack->widget(from);
    m_stack->removeWidget(page);
    m_stack->insertWidget(to, page);
    m_stack->setCurrentIndex(m_tabs->currentIndex());
    updateActiveDocument();
  });
  // A tab's × (or a middle click) closes its design: the last one to the
  // Start page or by quitting, as Preferences › General says (mitcad#2142).
  connect(m_tabs, &QTabBar::tabCloseRequested, this, [this](int index) {
    if (auto* document = qobject_cast<MainWindow*>(m_stack->widget(index)); document && m_manager) {
      qInfo().noquote() << QStringLiteral("Document tab closed: %1").arg(m_tabs->tabText(index));
      m_manager->closeDesign(document);
    }
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

DocumentFrame::~DocumentFrame() {
  // The tab bar back from a design's title row (mitcad#1990), so that it
  // goes with the frame and not before it with that design.
  placeTabs(nullptr);
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
    if (auto* frame = containing(document)) {
#ifdef Q_OS_MACOS
      // The closed page's native actions can hide shared application menu
      // roles when Qt destroys them. Refresh the focused frame afterwards.
      connect(document, &QObject::destroyed, qApp, [] {
        if (auto* activeFrame = qobject_cast<DocumentFrame*>(QApplication::activeWindow())) {
          activeFrame->updateActiveDocument();
        }
      }, Qt::QueuedConnection);
#endif
      frame->removeDocument(document);
    }
  });
  manager.setPresenter([](MainWindow* document) {
    if (auto* frame = containing(document)) {
      frame->activateDocument(document);
      const bool shownBefore = frame->isVisible();
      frame->show();
      // Its frame within the screen once it is known (mitcad#1556).
      if (!shownBefore) capNewWindowToScreen(frame);
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
  // Disable the page's native menu bar before reparenting. On macOS Qt
  // otherwise attaches it to this frame, taking over the system menu bar.
  // Destroying it then leaves the frame without a registered native bar.
  document->menuBar()->setNativeMenuBar(false);
  document->setParent(m_stack, Qt::Widget);
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
  // Not in its title row when it goes (mitcad#1990).
  if (document->isAncestorOf(m_tabs)) placeTabs(nullptr);
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
void DocumentFrame::logShownTab() {
  const int index = m_tabs->currentIndex();
  if (index < 0 || !m_tabs->isVisible()) {
    return;
  }
  const QPoint at = m_tabs->mapTo(m_tabs->window(), m_tabs->tabRect(index).center());
  const QString line = QStringLiteral("Document tab shown at %1,%2").arg(at.x()).arg(at.y());
  if (line != m_loggedTab) {
    m_loggedTab = line;
    qDebug().noquote() << line;
  }
  // The names and widths only once the row has given the bar the room it
  // asks for at least: while the row is still laid out (the bar 0 wide in
  // the Docked top bar) every tab is squeezed and its name elided only for
  // that moment.
  if (m_tabs->width() < m_tabs->minimumSizeHint().width()) {
    return;
  }
  static_cast<DocumentTabs*>(m_tabs)->logNamesAsShown();
  const QString widths = static_cast<const DocumentTabs*>(m_tabs)->rowWidths();
  if (!widths.isEmpty() && widths != m_loggedWidths) {
    m_loggedWidths = widths;
    qDebug().noquote() << widths;
  }
}
bool DocumentFrame::showsDocumentTabs(int documentCount) { return documentCount >= 2; }
bool DocumentFrame::tabsInTitleRow() const {
  const MainWindow* shown = activeDocument();
  const TitleBar* row = shown ? shown->findChild<TitleBar*>() : nullptr;
  return row && row->isFloatingRow();
}
bool DocumentFrame::showsTabs() const {
  // No tab for a window on the Start page with no design open (mitcad#2142).
  if (m_stack->count() == 1) {
    const MainWindow* only = activeDocument();
    if (only != nullptr && !only->hasDesign()) return false;
  }
  return tabsInTitleRow() ? m_stack->count() >= 1 : showsDocumentTabs(int(m_stack->count()));
}
void DocumentFrame::documentShownChanged(MainWindow* document) {
  if (m_stack->indexOf(document) < 0) return;
  updateDocument(document);
  updateChrome();
}
void DocumentFrame::placeTabs(MainWindow* host) {
  auto* tabs = static_cast<DocumentTabs*>(m_tabs);
  if (host) {
    if (auto* row = host->findChild<TitleBar*>()) {
      tabs->setRowStyle(true);
      row->hostDocumentTabs(m_tabs);
      return;
    }
  }
  tabs->setRowStyle(false);
  if (m_root && m_tabs->parentWidget() != m_root) {
    m_tabs->hide();
    m_tabs->setParent(m_root);
    m_rootLayout->insertWidget(0, m_tabs);
  }
}
void DocumentFrame::updateChrome() {
  // With the tab bar the frame's menu bar follows the shown document above
  // the tabs; without it the one document keeps its own menu bar, so its
  // window coordinates are the frame's and its layout is unchanged. In a
  // Floating title row (mitcad#1990) the tabs are the shown document's,
  // and neither bar shows: the row's File, ? and ☰ have the menus.
  MainWindow* shown = activeDocument();
  const bool inRow = tabsInTitleRow();
  placeTabs(inRow ? shown : nullptr);
  const bool tabs = showsTabs();
  m_tabs->setVisible(tabs);
  // A document whose menus are its top bar's (the Floating layout of
  // Windows and Linux, mitcad#1990) has no menu bar row, neither its own
  // nor the frame's copy.
  const MenuBarReveal& reveal = MenuBarReveal::instance();
  const bool noBarRow = shown != nullptr && reveal.keepsHidden(shown->menuBar());
  menuBar()->setVisible(tabs && !inRow && !noBarRow);
  // The system menu bar holds the frame's menus (a native menu bar); the
  // document's in-window copy then follows the preference.
  const bool inWindow = !menuBar()->isNativeMenuBar() || windowMenuBarShown();
  for (MainWindow* page : documents()) {
    page->menuBar()->setVisible((!tabs || inRow) && page == shown && inWindow && !reveal.keepsHidden(page->menuBar()));
    // The Docked toolbar over the Start page follows the tabs (mitcad#2082).
    page->updateStartToolbar();
  }
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
  if (chosen == close) { if (m_manager) m_manager->closeDesign(document); }
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
  // document Ctrl+W is File › Close. Either closes the design as its tab's
  // × does (WindowManager::closeDesign, mitcad#2142).
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
        if (close) { if (auto* document = activeDocument(); document && m_manager) m_manager->closeDesign(document); }
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
