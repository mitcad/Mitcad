// SPDX-License-Identifier: MIT
#include "ProjectsPanel.hpp"

#include <algorithm>
#include <functional>
#include <utility>

#include <QAbstractItemDelegate>
#include <QAction>
#include <QApplication>
#include <QDateTime>
#include <QDir>
#include <QDockWidget>
#include <QEvent>
#include <QFileInfo>
#include <QFont>
#include <QGuiApplication>
#include <QHeaderView>
#include <QHideEvent>
#include <QKeyEvent>
#include <QLineEdit>
#include <QMainWindow>
#include <QMetaObject>
#include <QMenu>
#include <QMouseEvent>
#include <QPainter>
#include <QPixmap>
#include <QScrollBar>
#include <QStyle>
#include <QStyleOptionViewItem>
#include <QTimer>
#include <QToolBar>
#include <QToolButton>
#include <QTreeWidget>
#include <QTreeWidgetItemIterator>
#include <QVBoxLayout>
#include <QtLogging>

#include "../files/RemoteTask.hpp"
#include "../framework/AppSettings.hpp"
#include "../framework/ChromeStyle.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/Icons.hpp"
#include "../framework/PanelTree.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/WindowStyle.hpp"
#include "../platform/NativeDialogs.hpp"
#include "ProjectRowText.hpp"
#include "ProjectRowDrag.hpp"
#include "../framework/UiSizes.hpp"

namespace mitcad {
namespace {

constexpr int kName = 0;
constexpr int kState = 1;
constexpr int kTypeRole = Qt::UserRole;
constexpr int kRootRole = Qt::UserRole + 1;
constexpr int kPathRole = Qt::UserRole + 2;
constexpr int kKeyRole = Qt::UserRole + 3;
constexpr int kTipRole = Qt::UserRole + 4;   // a project row's tooltip lines without its state
constexpr int kMarksRole = Qt::UserRole + 5; // the row's marks as the log shows them (RowMarks::logged)
constexpr int kDefaultWidth = 240;

const QString kSuffix = QStringLiteral(".mitcad");

// A design's name as the panel shows it: its file's name without .mitcad.
QString designName(const QString& path) { return designDisplayName(path); }

// What a name typed in place is for, as the log names it.
QString namingWhat(bool isNew, bool folder, const QString& from) {
  if (isNew) {
    return folder ? QStringLiteral("new folder") : QStringLiteral("new design");
  }
  return QStringLiteral("rename of %1").arg(from);
}

// Delete deletes; on macOS the delete key of the keyboard (Backspace) too.
bool isDeleteKey(int key) {
#ifdef Q_OS_MACOS
  return key == Qt::Key_Delete || key == Qt::Key_Backspace;
#else
  return key == Qt::Key_Delete;
#endif
}

// A project row's marks (mitcad#529): as the project indicator shows its
// state, the current project's remote as the remote work knows it, others'
// from their local refs.
RowMarks projectState(const ProjectRow& row, const ProjectIndicator::Remote* remote) {
  int ahead = row.ahead;
  int behind = row.behind;
  QString problem;
  bool conflict = false;
  if (remote != nullptr) {
    ahead = remote->ahead >= 0 ? remote->ahead : ahead;
    behind = remote->behind >= 0 ? remote->behind : behind;
    problem = remote->problem;
    conflict = remote->conflict;
  }
  return projectMarks(row, ahead, behind, problem, conflict);
}

RowMarks designState(const ProjectDesignRow& design) { return designMarks(design); }

// A design row's tooltip: its full name and path first (its name may be
// elided in the row, mitcad#529), its state in full words, then its
// version, changes and edit lock.
QString designTip(const QString& root, const ProjectDesignRow& design) {
  QStringList tip{designName(design.path), QDir::toNativeSeparators(QDir(root).filePath(design.path)),
                  designMarks(design).full};
  if (design.version > 0) {
    const QDateTime when = QDateTime::fromSecsSinceEpoch(design.versionTime).toLocalTime();
    tip << ProjectsPanel::tr("Version %1 (%2), saved %3 by %4")
               .arg(design.version)
               .arg(design.versionId, when.toString(QStringLiteral("yyyy-MM-dd HH:mm")), design.versionAuthor);
  } else {
    tip << ProjectsPanel::tr("No version recorded yet: Save records one");
  }
  if (design.recorded && design.changed) {
    tip << ProjectsPanel::tr("Changed since its latest version");
  }
  if (design.locked) {
    tip << (design.lockMine        ? ProjectsPanel::tr("You are editing it here")
            : design.lockSameOwner ? ProjectsPanel::tr("You are editing it in another window or on another computer")
                                   : ProjectsPanel::tr("%1 is editing it: it opens read-only").arg(design.lockOwner));
  }
  return tip.join(QLatin1Char('\n'));
}

// A context menu whose entries are logged ("Context menu: A | B") for the
// UI tests, which choose entries by their place.
class LoggedMenu {
public:
  explicit LoggedMenu(QWidget* parent) : m_menu(parent) {}
  void add(const QString& icon, const QString& text, std::function<void()> call) {
    QAction* action = m_menu.addAction(icon.isEmpty() ? QIcon() : themeIcon(icon), text);
    QObject::connect(action, &QAction::triggered, &m_menu, std::move(call));
    m_entries << QString(text).remove(QLatin1Char('&'));
  }
  void separator() {
    if (!m_entries.isEmpty()) {
      m_menu.addSeparator();
    }
  }
  void exec(const QPoint& at) {
    if (m_entries.isEmpty()) {
      return;
    }
    qDebug().noquote() << QStringLiteral("Context menu: %1").arg(m_entries.join(QStringLiteral(" | ")));
    execMenu(&m_menu, at);
  }

private:
  QMenu m_menu;
  QStringList m_entries;
};

// The rows' cells (mitcad#529): a name that does not fit is elided in the
// middle, keeping its start and end (a rename's editor still gets the whole
// name); in the Docked panel the rows with their band (PanelRowDelegate).
class ProjectsRowDelegate : public PanelRowDelegate {
public:
  ProjectsRowDelegate(bool bands, QObject* parent) : PanelRowDelegate(parent), m_bands(bands) {}

  // The text a cell shows in `option` (its rectangle, the view's options),
  // as paint() draws it.
  QString shownText(const QStyleOptionViewItem& option, const QModelIndex& index) const {
    QStyleOptionViewItem cell = option;
    initStyleOption(&cell, index);
    return index.column() == kName ? elided(cell) : cell.text;
  }

  void paint(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    if (m_bands) {
      paintPanelRowBand(painter, option, index);
    }
    QStyleOptionViewItem cell = m_bands ? panelRowOption(option) : option;
    initStyleOption(&cell, index);
    if (index.column() == kName) {
      cell.text = elided(cell);
    }
    hideEditedText(cell, index);
    // Status marks (especially the lock) must follow the selected text,
    // rather than keeping a dark outline over the system accent colour.
    if (index.column() == kState && (cell.state & QStyle::State_Selected) && !cell.icon.isNull()) {
      QPixmap mark = cell.icon.pixmap(cell.decorationSize, painter->device()->devicePixelRatioF());
      QPainter tint(&mark);
      tint.setCompositionMode(QPainter::CompositionMode_SourceIn);
      tint.fillRect(mark.rect(), cell.palette.color(QPalette::HighlightedText));
      tint.end();
      cell.icon = QIcon();
      cell.icon.addPixmap(mark, QIcon::Normal);
      cell.icon.addPixmap(mark, QIcon::Selected);
    }
    cell.textElideMode = Qt::ElideMiddle;
    const QWidget* widget = cell.widget;
    const QStyle* style = widget != nullptr ? widget->style() : QApplication::style();
    style->drawControl(QStyle::CE_ItemViewItem, &cell, painter, widget);
  }

  QSize sizeHint(const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    return m_bands ? PanelRowDelegate::sizeHint(option, index) : QStyledItemDelegate::sizeHint(option, index);
  }

private:
  // The cell's text elided in the middle to the room the style gives it
  // (its text rectangle less the margins it draws the text within).
  static QString elided(const QStyleOptionViewItem& cell) {
    const QWidget* widget = cell.widget;
    const QStyle* style = widget != nullptr ? widget->style() : QApplication::style();
    const QRect room = style->subElementRect(QStyle::SE_ItemViewItemText, &cell, widget);
    const int margin = style->pixelMetric(QStyle::PM_FocusFrameHMargin, nullptr, widget) + 1;
    return elideMiddle(cell.text, cell.fontMetrics, room.width() - 2 * margin);
  }

  bool m_bands;
};

} // namespace

// ---------------------------------------------------------------------------
// The tree: keys, double-clicks and drags of rows

class ProjectsTreeWidget : public QTreeWidget {
public:
  explicit ProjectsTreeWidget(ProjectsPanel& panel) : QTreeWidget(&panel), m_panel(panel) {}

  QSize minimumSizeHint() const override { return dockedTreeMinimumSize(this, QTreeWidget::minimumSizeHint()); }

  bool editing() const { return state() == QAbstractItemView::EditingState; }
  bool dragging() const { return m_drag.dragging(); }

  // The text a row's cell shows as it is painted (a name elided in the
  // middle, mitcad#529); "" for a row in a folded folder.
  QString shownText(QTreeWidgetItem* item, int column) const {
    const QModelIndex index = indexFromItem(item, column);
    const QRect rect = visualRect(index);
    if (!rect.isValid()) {
      return {};
    }
    QStyleOptionViewItem option;
    initViewItemOption(&option);
    option.rect = rect;
    if (const auto* delegate = dynamic_cast<const ProjectsRowDelegate*>(itemDelegate())) {
      return delegate->shownText(option, index);
    }
    return item->text(column);
  }

  bool cancelEdit() {
    if (!editing()) {
      return false;
    }
    if (QWidget* editor = indexWidget(currentIndex())) {
      closeEditor(editor, QAbstractItemDelegate::RevertModelCache);
    }
    return true;
  }

  // A row's name cell and back, for the AppKit presentation (mitcad#816).
  QModelIndex indexOf(const QTreeWidgetItem* item) const {
    return item == nullptr ? QModelIndex() : indexFromItem(item, kName);
  }
  QTreeWidgetItem* itemOf(const QModelIndex& index) const {
    return index.isValid() && index.model() == model() ? itemFromIndex(index) : nullptr;
  }

protected:
  // Every end of the name editor (Return, Esc, the focus elsewhere, and
  // another row made current, which takes the name without the delegate's
  // signal), told to the panel once the view is done with the editor; an
  // editor out of sight then is not taken (mitcad#1268).
  void closeEditor(QWidget* editor, QAbstractItemDelegate::EndEditHint hint) override {
    const bool hidden = editor == nullptr || !editor->isVisible() || !isVisible();
    QTreeWidget::closeEditor(editor, hint);
    m_panel.treeEditorClosed(hint != QAbstractItemDelegate::RevertModelCache, hidden);
  }

  bool event(QEvent* event) override {
    // The panel's keys before the window's shortcuts (Return confirms a
    // command there, Delete deletes in a sketch).
    if (event->type() == QEvent::ShortcutOverride && !editing() && currentItem() != nullptr) {
      const auto* key = static_cast<QKeyEvent*>(event);
      const bool plain = (key->modifiers() & ~Qt::KeypadModifier) == Qt::NoModifier;
      if (plain && (key->key() == Qt::Key_Return || key->key() == Qt::Key_Enter || key->key() == Qt::Key_F2 ||
                    isDeleteKey(key->key()))) {
        event->accept();
        return true;
      }
    }
    return QTreeWidget::event(event);
  }

  void keyPressEvent(QKeyEvent* event) override {
    // F2, Delete and Return on the current row (the AppKit outline's too).
    if (m_panel.treeKey(event->key())) {
      return;
    }
    QTreeWidget::keyPressEvent(event);
  }

  void mouseDoubleClickEvent(QMouseEvent* event) override {
    QTreeWidgetItem* item = itemAt(event->position().toPoint());
    if (item != nullptr && event->button() == Qt::LeftButton &&
        m_panel.typeOf(item) == ProjectsPanel::RowType::Design) {
      setCurrentItem(item);
      m_panel.openCurrent();
      return;
    }
    // A Cloud project: its settings (mitcad#152) instead of expanding the
    // row.
    if (item != nullptr && event->button() == Qt::LeftButton && m_panel.settingsOnDoubleClick(item)) {
      return;
    }
    QTreeWidget::mouseDoubleClickEvent(event);
  }

  // A drag of a design or a folder onto a folder, a project or a design
  // (its folder) of the same project moves it there. Done here rather than
  // with the system's drag and drop: nothing leaves the panel.
  void mousePressEvent(QMouseEvent* event) override {
    m_drag.clear();
    if (event->button() == Qt::LeftButton && !editing()) {
      const QPoint pressed = event->position().toPoint();
      QTreeWidgetItem* item = itemAt(pressed);
      const ProjectsPanel::RowType type = m_panel.typeOf(item);
      if (item != nullptr && (type == ProjectsPanel::RowType::Design || type == ProjectsPanel::RowType::Folder)) {
        m_drag.press(indexFromItem(item), pressed);
      }
    }
    QTreeWidget::mousePressEvent(event);
  }

  void mouseMoveEvent(QMouseEvent* event) override {
    if (m_drag.item().isValid() && (event->buttons() & Qt::LeftButton) != 0) {
      const QPoint at = event->position().toPoint();
      const bool wasDragging = m_drag.dragging();
      if (m_drag.move(at, QApplication::startDragDistance())) {
        if (!wasDragging) {
          qInfo().noquote() << QStringLiteral("Projects panel: dragging %1")
                                  .arg(m_panel.keyOf(itemFromIndex(m_drag.item())));
        }
        setOver(itemAt(at));
        return;
      }
    }
    QTreeWidget::mouseMoveEvent(event);
  }

  void mouseReleaseEvent(QMouseEvent* event) override {
    if (m_drag.dragging()) {
      QTreeWidgetItem* dragged = itemFromIndex(m_drag.item());
      QTreeWidgetItem* over = itemAt(event->position().toPoint());
      setOver(nullptr);
      unsetCursor();
      m_drag.clear();
      if (event->button() == Qt::LeftButton) {
        m_panel.dropped(dragged, over);
      }
      m_panel.applyWaitingRows();
      return;
    }
    m_drag.clear();
    unsetCursor();
    QTreeWidget::mouseReleaseEvent(event);
  }

private:
  void setOver(QTreeWidgetItem* item) {
    QTreeWidgetItem* over = itemFromIndex(m_over);
    if (item == over) {
      return;
    }
    if (over != nullptr) {
      for (int column = 0; column < columnCount(); ++column) {
        over->setData(column, Qt::BackgroundRole, QVariant());
      }
    }
    m_over = QPersistentModelIndex();
    QString folder;
    if (item != nullptr && m_panel.dropTarget(itemFromIndex(m_drag.item()), item, &folder)) {
      m_over = indexFromItem(item);
      QColor color = palette().color(QPalette::Highlight);
      color.setAlpha(80);
      for (int column = 0; column < columnCount(); ++column) {
        item->setBackground(column, color);
      }
      setCursor(Qt::DragMoveCursor);
    } else if (m_drag.dragging()) {
      setCursor(Qt::ForbiddenCursor);
    }
  }

  ProjectsPanel& m_panel;
  ProjectRowDrag m_drag;
  QPersistentModelIndex m_over;
};

// ---------------------------------------------------------------------------
// The panel

ProjectsPanel::ProjectsPanel(Host host, QWidget* parent) : QWidget(parent), m_host(std::move(host)) {
  setProperty("nativeCompactPanel", true);
  setObjectName(QStringLiteral("projectsPanel"));
  auto* layout = new QVBoxLayout(this);
  layout->setContentsMargins(0, 0, 0, 0);
  layout->setSpacing(2);
  m_toolbar = new QToolBar(this);
  m_toolbar->setObjectName(QStringLiteral("projectsToolbar"));
  m_toolbar->setIconSize(uiIconSize(16, 16));
  m_toolbar->setToolButtonStyle(Qt::ToolButtonIconOnly);
  layout->addWidget(m_toolbar);
  m_tree = new ProjectsTreeWidget(*this);
  m_tree->setObjectName(QStringLiteral("projectsTree"));
  m_tree->setProperty("nativeSourceList", true);
  layout->addWidget(m_tree, 1);
  m_tree->setColumnCount(2);
  m_tree->setHeaderHidden(true);
  m_tree->header()->setStretchLastSection(false);
  m_tree->header()->setSectionResizeMode(kName, QHeaderView::Stretch);
  m_tree->header()->setSectionResizeMode(kState, QHeaderView::ResizeToContents);
  // The status marks follow a selected row's text natively too (mitcad#1080).
  m_tree->setProperty(NativeSelectedIconTintProperty, QVariantList{kState});
  m_tree->setSelectionMode(QAbstractItemView::SingleSelection);
  m_tree->setEditTriggers(QAbstractItemView::NoEditTriggers);
  m_tree->setContextMenuPolicy(Qt::CustomContextMenu);
  m_tree->setUniformRowHeights(true);
  m_tree->setIconSize(uiIconSize(16, 16));
  // The Browser's indentation per level, so that a narrow panel's nested
  // rows do not drift to the right (mitcad#393).
  panel::setCompactIndentation(m_tree, panel::Indent::Tree);
  m_tree->setExpandsOnDoubleClick(true);
  // Only a click gives the tree the keyboard, so that the view keeps the
  // single-letter shortcuts.
  m_tree->setFocusPolicy(Qt::ClickFocus);
  // The names get the width (mitcad#529): one line per row, a name that
  // does not fit elided in the middle (the delegate), the state as compact
  // marks; no horizontal scrolling, also for deep trees, whose levels
  // indent little (mitcad#393): the panel's width is dragged instead.
  m_tree->setTextElideMode(Qt::ElideMiddle);
  m_tree->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  setMinimumWidth(160);
  if (chromeStyle() == ChromeStyle::Floating) {
    // Inside a card: the card's material shows through.
    m_toolbar->setStyleSheet(QStringLiteral("QToolBar { background: transparent; border: none; }"));
    m_tree->setItemDelegate(new ProjectsRowDelegate(false, m_tree));
    m_tree->setFrameShape(QFrame::NoFrame);
    m_tree->setAttribute(Qt::WA_MacShowFocusRect, false);
    QPalette palette = m_tree->palette();
    for (const QPalette::ColorRole role : {QPalette::Base, QPalette::AlternateBase}) {
      palette.setColor(role, Qt::transparent);
    }
    m_tree->setPalette(palette);
    m_tree->viewport()->setAutoFillBackground(false);
  } else {
    // On the Docked panel's surface, rows with the panels' height and
    // highlight (mitcad#251); before the editor's signal is connected below.
    m_tree->setItemDelegate(new ProjectsRowDelegate(true, m_tree));
    stylePanelTree(m_tree);
  }
  // A drag of the Docked panel's edge rests: its width is remembered.
  m_widthTimer = new QTimer(this);
  m_widthTimer->setSingleShot(true);
  m_widthTimer->setInterval(400);
  connect(m_widthTimer, &QTimer::timeout, this, [this] {
    if (m_container != nullptr) {
      emit dockWidthChosen(m_container->width());
    }
  });

  const auto action = [this](const char* icon, const QString& text, const QString& tip, auto run) {
    QAction* made = m_toolbar->addAction(themeIcon(QString::fromLatin1(icon)), text);
    made->setToolTip(tip);
    connect(made, &QAction::triggered, this, run);
    return made;
  };
  QAction* newProject = action("add", tr("New Project"), tr("New Project: create a Local or Cloud project"), [this] {
    emit commandRequested(QStringLiteral("file.new_project"));
  });
  newProject->setObjectName(QStringLiteral("projectsNewProject"));
  m_newDesign = action("new-design", tr("New Design"),
                       tr("New Design: a design in the selected project folder, named here"), [this] {
    QString root = selectedRoot();
    if (root.isEmpty() && m_host.currentRoot) {
      root = m_host.currentRoot();
    }
    if (!root.isEmpty()) {
      startNewDesign(root, sameRoot(root, selectedRoot()) ? selectedFolder() : QString());
    }
  });
  m_newDesign->setObjectName(QStringLiteral("projectsNewDesign"));
  m_newFolder = action("folder", tr("New Folder"), tr("A new folder in the selected folder, named here"), [this] {
    QString root = selectedRoot();
    if (root.isEmpty() && m_host.currentRoot) {
      root = m_host.currentRoot();
    }
    if (!root.isEmpty()) {
      startNewFolder(root, sameRoot(root, selectedRoot()) ? selectedFolder() : QString());
    }
  });
  m_newFolder->setObjectName(QStringLiteral("projectsNewFolder"));
  // Rename has no button (a crowded toolbar): F2 and the rows' context menu.
  m_delete = action("delete", tr("Delete"),
                    tr("Deletes the selected design or folder as a new version: the history keeps it (Del)"),
                    [this] { deleteCurrent(); });
  m_delete->setObjectName(QStringLiteral("projectsDelete"));
  m_history = action("version-history", tr("Version History"), tr("The selected design's versions"), [this] {
    QTreeWidgetItem* item = m_tree->currentItem();
    if (typeOf(item) == RowType::Design) {
      emit versionHistoryRequested(rootOf(item), pathOf(item));
    }
  });
  m_sync = action("sync", tr("Sync Now"), tr("Takes the project's newer versions and sends yours"), [this] {
    if (!selectedRoot().isEmpty()) {
      emit syncRequested(selectedRoot());
    }
  });
  m_settings = action("settings", tr("Project Settings…"), tr("The selected project's settings"), [this] {
    // The project of the selected row: the project itself, a folder or a
    // design in it (mitcad#153).
    QTreeWidgetItem* item = m_tree->currentItem();
    const QString root = settingsRoot(item);
    qInfo().noquote() << QStringLiteral("Projects panel: settings of %1%2")
                             .arg(root.isEmpty() ? QStringLiteral("no project") : root,
                                  item == nullptr ? QString() : QStringLiteral(" (%1 selected)").arg(keyOf(item)));
    if (!root.isEmpty()) {
      emit projectSettingsRequested(root);
    }
  });
  auto* more = new QMenu(this);
  const auto entry = [this, more](const char* icon, const QString& text, auto run) {
    QAction* made = more->addAction(icon == nullptr ? QIcon() : themeIcon(QString::fromLatin1(icon)), text);
    connect(made, &QAction::triggered, this, run);
    return made;
  };
  entry("new-project", tr("New Project…"), [this] { emit commandRequested(QStringLiteral("file.new_project")); });
  entry("remote-open", tr("Open from Cloud…"), [this] { emit commandRequested(QStringLiteral("file.open_remote")); });
  entry("folder", tr("Add Existing Project Folder…"), [this] { emit addFolderRequested(); });
  more->addSeparator();
  QAction* show = entry(nullptr, tr("Show in Folder"), [this] {
    QTreeWidgetItem* item = m_tree->currentItem();
    if (!rootOf(item).isEmpty()) {
      emit showInFolderRequested(QDir(rootOf(item)).filePath(folderFor(item)));
    }
  });
  QAction* remove = entry(nullptr, tr("Remove from List"), [this] {
    QTreeWidgetItem* item = m_tree->currentItem();
    if (typeOf(item) == RowType::Project) {
      emit removeFromListRequested(rootOf(item));
    }
  });
  // Remove Project... (mitcad#1562): the selected project's folder stops
  // being a project.
  QAction* removeEntry = entry(nullptr, tr("Remove Project…"), [this] {
    QTreeWidgetItem* item = m_tree->currentItem();
    if (typeOf(item) == RowType::Project) {
      emit removeProjectRequested(rootOf(item));
    }
  });
  connect(more, &QMenu::aboutToShow, this, [this, show, remove, removeEntry] {
    QTreeWidgetItem* item = m_tree->currentItem();
    show->setEnabled(!rootOf(item).isEmpty() && project(rootOf(item)) != nullptr &&
                     project(rootOf(item))->kind != ProjectRow::Kind::Missing);
    remove->setEnabled(typeOf(item) == RowType::Project);
    const ProjectRow* row = typeOf(item) == RowType::Project ? project(rootOf(item)) : nullptr;
    removeEntry->setEnabled(row != nullptr && row->kind != ProjectRow::Kind::Missing &&
                            row->kind != ProjectRow::Kind::Unreadable);
  });
  m_more = m_toolbar->addAction(themeIcon(QStringLiteral("new-project")), tr("Projects"));
  m_more->setToolTip(tr("New, open or add a project; Show in Folder, Remove from List, Remove Project"));
  m_more->setMenu(more);
  if (auto* button = qobject_cast<QToolButton*>(m_toolbar->widgetForAction(m_more))) {
    button->setPopupMode(QToolButton::InstantPopup);
  }
  for (QAction* each : m_toolbar->actions()) {
    if (QWidget* button = m_toolbar->widgetForAction(each)) {
      button->setFocusPolicy(Qt::NoFocus);
      button->setAccessibleName(each->text());
    }
  }

  connect(m_tree, &QTreeWidget::currentItemChanged, this, [this](QTreeWidgetItem* item) {
    if (m_building) {
      return;
    }
    updateActions();
    const QString key = keyOf(item);
    if (!key.isEmpty() && key != m_loggedSelection) {
      m_loggedSelection = key;
      qInfo().noquote() << QStringLiteral("Projects panel selected %1").arg(key);
    }
  });
  connect(m_tree, &QTreeWidget::customContextMenuRequested, this, &ProjectsPanel::contextMenu);
  // Qt's subclass handles pointer activation itself. AppKit dispatches
  // the model-view signal instead, using the same project/design actions.
  connect(m_tree, &QTreeView::activated, this, [this] {
    if (m_nativePresented && !settingsOnDoubleClick(m_tree->currentItem())) openCurrent();
  });
  const auto remember = [this](QTreeWidgetItem* item, bool expanded) {
    if (!m_building) {
      m_expanded.insert(keyOf(item), expanded);
      emit contentHeightChanged();
      scheduleLayoutLog();
    }
  };
  connect(m_tree, &QTreeWidget::itemExpanded, this, [remember](QTreeWidgetItem* item) { remember(item, true); });
  connect(m_tree, &QTreeWidget::itemCollapsed, this, [remember](QTreeWidgetItem* item) { remember(item, false); });
  connect(m_tree->verticalScrollBar(), &QScrollBar::valueChanged, this, [this] { scheduleLayoutLog(); });
  // The end of a name typed in place: ProjectsTreeWidget::closeEditor.
  updateActions();
}

ProjectsPanel::~ProjectsPanel() = default;

QSize ProjectsPanel::sizeHint() const { return QSize(kDefaultWidth, QWidget::sizeHint().height()); }

void ProjectsPanel::showEvent(QShowEvent* event) {
  QWidget::showEvent(event);
  refresh();
  scheduleLayoutLog();
}

void ProjectsPanel::hideEvent(QHideEvent* event) {
  QWidget::hideEvent(event);
  // Not when the system hides the window (minimized): it comes back as it
  // was. The AppKit presentation keeps this widget hidden throughout.
  if (event->spontaneous() || m_nativePresented) {
    return;
  }
  if (m_isEditing && !m_nativeEditing) {
    qInfo().noquote() << QStringLiteral("Projects panel: hidden under the name editor: the name is not taken");
    cancelEditing();
  } else if (m_hasPending) {
    m_hasPending = false;
    qInfo().noquote() << QStringLiteral("Projects panel: hidden before the name was asked: nothing to name");
    scheduleNamingEnd();
  }
}

void ProjectsPanel::resizeEvent(QResizeEvent* event) {
  QWidget::resizeEvent(event);
  scheduleLayoutLog();
}

bool ProjectsPanel::eventFilter(QObject* watched, QEvent* event) {
  if (event->type() == QEvent::Move || event->type() == QEvent::Resize || event->type() == QEvent::Show) {
    scheduleLayoutLog();
  }
  // The dock resized while the left button is held: a drag of its edge
  // (the window's own layout resizes it without), remembered once it rests.
  if (event->type() == QEvent::Resize && watched == m_container.data() &&
      qobject_cast<QDockWidget*>(m_container.data()) != nullptr && m_container->isVisible() &&
      (QGuiApplication::mouseButtons() & Qt::LeftButton) != 0) {
    m_widthTimer->start();
  }
  return QWidget::eventFilter(watched, event);
}

void ProjectsPanel::watchContainer(QWidget* container) {
  m_container = container;
  if (container != nullptr) {
    container->installEventFilter(this);
  }
}

// ---------------------------------------------------------------------------
// Reading

void ProjectsPanel::setNativePresented(bool shown) {
  m_nativePresented = shown;
  if (shown) refresh();
}

void ProjectsPanel::refresh() {
  if (!isVisible() && !m_nativePresented) {
    return; // read when shown
  }
  if (m_task != nullptr) {
    m_readAgain = true;
    return;
  }
  startRead();
}

void ProjectsPanel::startRead() {
  const ProjectsSettings settings = ProjectsSettings::load();
  const QStringList known = settings.known;
  const QStringList hidden = settings.hidden;
  const QString folder = projectsDirectory();
  const QString session = m_host.session ? m_host.session() : QString();
  RemoteTask* task = RemoteTask::custom(
      QStringLiteral("project_tree"),
      [known, folder, hidden, session](const SyncControl&) { return readProjects(known, folder, hidden, session); },
      this);
  m_task = task;
  connect(task, &RemoteTask::finished, this, [this, task] { readFinished(task); });
  task->start();
}

void ProjectsPanel::readFinished(RemoteTask* task) {
  if (task != m_task) {
    return;
  }
  m_task = nullptr;
  const QJsonObject answer = task->answer();
  task->deleteLater();
  if (answer.contains(QStringLiteral("error"))) {
    qWarning().noquote() << QStringLiteral("Projects panel: the projects could not be read: %1")
                                .arg(answer.value(QStringLiteral("error"))
                                         .toObject()
                                         .value(QStringLiteral("message"))
                                         .toString());
  }
  const QVector<ProjectRow> rows = projectRows(answer);
  if (rowsHeld()) {
    // Not under the name being typed or the row being dragged.
    m_waitingRows = rows;
    m_rowsWaiting = true;
  } else {
    m_rows = rows;
    rebuild();
  }
  if (m_readAgain) {
    m_readAgain = false;
    startRead();
  }
}

bool ProjectsPanel::rowsHeld() const { return m_isEditing || m_tree->dragging() || m_nativeDragging; }

void ProjectsPanel::applyWaitingRows() {
  if (m_marksWaiting && !m_isEditing) {
    // Marks that changed under the AppKit name editor (mitcad#816); a
    // rebuild below applies them too.
    m_marksWaiting = false;
    if (!m_rowsWaiting || rowsHeld()) {
      applyMarks();
    }
  }
  if (!m_rowsWaiting || rowsHeld()) {
    return;
  }
  m_rowsWaiting = false;
  m_rows = m_waitingRows;
  m_waitingRows.clear();
  rebuild();
}

void ProjectsPanel::rebuild() {
  const QString selected = keyOf(m_tree->currentItem());
  const int scroll = m_tree->verticalScrollBar()->value();
  const QString current = m_host.currentRoot ? m_host.currentRoot() : QString();
  m_building = true;
  m_tree->clear();
  m_messageItem = nullptr; // not there while a name is typed (the rows wait)
  // The projects at the top, nothing above them (mitcad#173), by the name
  // shown.
  QVector<const ProjectRow*> rows;
  for (const ProjectRow& row : std::as_const(m_rows)) {
    rows << &row;
  }
  std::sort(rows.begin(), rows.end(), [](const ProjectRow* a, const ProjectRow* b) {
    const int byName = a->name.compare(b->name, Qt::CaseInsensitive);
    return byName != 0 ? byName < 0 : a->root < b->root;
  });
  for (const ProjectRow* row : std::as_const(rows)) {
    QTreeWidgetItem* projectRow = addProjectRow(*row, projectKeyOf(*row));
    const bool open = m_revealed.contains(row->root) || sameRoot(row->root, current);
    projectRow->setExpanded(m_expanded.value(keyOf(projectRow), open));
  }
  QTreeWidgetItem* select = nullptr;
  if (!m_reveal.isEmpty()) {
    select = projectItem(m_reveal);
    if (select != nullptr) {
      m_reveal.clear();
      for (QTreeWidgetItem* up = select->parent(); up != nullptr; up = up->parent()) {
        up->setExpanded(true);
      }
      select->setExpanded(true);
    }
  }
  if (select == nullptr && !selected.isEmpty()) {
    for (QTreeWidgetItemIterator it(m_tree); *it != nullptr; ++it) {
      if (keyOf(*it) == selected) {
        select = *it;
        break;
      }
    }
  }
  if (select != nullptr) {
    m_tree->setCurrentItem(select);
  }
  m_tree->verticalScrollBar()->setValue(scroll);
  if (select != nullptr && keyOf(select) != selected) {
    m_tree->scrollToItem(select);
  }
  m_building = false;
  if (select != nullptr && keyOf(select) != m_loggedSelection) {
    m_loggedSelection = keyOf(select);
    qInfo().noquote() << QStringLiteral("Projects panel selected %1").arg(m_loggedSelection);
  }
  applyMarks();
  updateActions();
  emit contentHeightChanged();
  scheduleLayoutLog();
  if (m_hasPending) {
    m_hasPending = false;
    if (folderItem(m_pending.root, m_pending.parent) != nullptr) {
      startNew(m_pending.folder, m_pending.root, m_pending.parent);
    } else {
      qInfo().noquote() << QStringLiteral("Projects panel: %1 is not listed: nothing to name")
                               .arg(joinPath(m_pending.root, m_pending.parent));
      scheduleNamingEnd();
    }
  }
}

QString ProjectsPanel::projectKeyOf(const ProjectRow& row) const {
  // The folder's name, which a display name does not change.
  int same = 0;
  for (const ProjectRow& other : m_rows) {
    same += other.folderName == row.folderName ? 1 : 0;
  }
  return same > 1 ? row.root : row.folderName;
}

QTreeWidgetItem* ProjectsPanel::addProjectRow(const ProjectRow& row, const QString& projectKey) {
  // Local and Cloud told apart by the icon and the tooltip (mitcad#173).
  const bool cloud = row.kind == ProjectRow::Kind::Cloud || row.noRemoteChosen;
  auto* top = new QTreeWidgetItem(m_tree);
  top->setText(kName, row.name);
  top->setIcon(kName, themeIcon(cloud ? QStringLiteral("remote-open") : QStringLiteral("home")));
  top->setData(kName, kTypeRole, static_cast<int>(RowType::Project));
  top->setData(kName, kRootRole, row.root);
  top->setData(kName, kPathRole, QString());
  top->setData(kName, kKeyRole, projectKey);
  top->setData(kName, NativeControlIdentifierRole, QStringLiteral("projects.row.") + projectKey);
  top->setFlags(Qt::ItemIsEnabled | Qt::ItemIsSelectable);
  // Its full name first (elided in the row when it does not fit); its state
  // in full words follows it (applyMarks).
  QStringList tip{row.name};
  if (row.kind == ProjectRow::Kind::Cloud) {
    tip << tr("Cloud project: its versions are also in a remote repository");
  } else if (row.noRemoteChosen) {
    tip << tr("Cloud project: no remote chosen");
  } else if (row.kind == ProjectRow::Kind::Local) {
    tip << tr("Local project: its versions are on this computer");
  }
  tip << tr("Folder: %1").arg(QDir::toNativeSeparators(row.root));
  if (!row.remoteUrl.isEmpty()) {
    tip << tr("Remote: %1").arg(row.remoteUrl);
  }
  if (row.kind == ProjectRow::Kind::NoHistory) {
    tip << (row.problem.isEmpty() ? tr("The project keeps no versions")
                                  : tr("The project keeps no versions: it is inside the git repository %1")
                                        .arg(QDir::toNativeSeparators(row.problem)));
  } else if (row.kind == ProjectRow::Kind::Missing) {
    tip << tr("The folder is not there any more: Remove from List takes it out of the list");
  } else if (!row.problem.isEmpty()) {
    tip << row.problem;
  }
  if (row.truncated) {
    tip << tr("Only the first designs and folders are listed");
  }
  top->setData(kName, kTipRole, tip);
  top->setToolTip(kName, tip.join(QLatin1Char('\n')));
  top->setToolTip(kState, tip.join(QLatin1Char('\n')));
  QHash<QString, QTreeWidgetItem*> folders;
  folders.insert(QString(), top);
  std::function<QTreeWidgetItem*(const QString&)> ensure = [&](const QString& path) -> QTreeWidgetItem* {
    if (QTreeWidgetItem* found = folders.value(path)) {
      return found;
    }
    QTreeWidgetItem* parent = ensure(dirOf(path));
    auto* folder = new QTreeWidgetItem(parent);
    folder->setText(kName, lastPart(path));
    folder->setIcon(kName, themeIcon(QStringLiteral("folder")));
    folder->setData(kName, kTypeRole, static_cast<int>(RowType::Folder));
    folder->setData(kName, kRootRole, row.root);
    folder->setData(kName, kPathRole, path);
    folder->setData(kName, kKeyRole, QStringLiteral("%1/%2/").arg(projectKey, path));
    folder->setData(kName, NativeControlIdentifierRole, QStringLiteral("projects.row.%1/%2/").arg(projectKey, path));
    folder->setFlags(Qt::ItemIsEnabled | Qt::ItemIsSelectable | Qt::ItemIsEditable);
    folder->setToolTip(kName, QStringLiteral("%1\n%2").arg(lastPart(path),
                                                           QDir::toNativeSeparators(QDir(row.root).filePath(path))));
    folders.insert(path, folder);
    return folder;
  };
  for (const QString& folder : row.folders) {
    ensure(folder);
  }
  for (const ProjectDesignRow& design : row.designs) {
    auto* item = new QTreeWidgetItem(ensure(dirOf(design.path)));
    item->setText(kName, designName(design.path));
    item->setIcon(kName, themeIcon(QStringLiteral("project")));
    item->setData(kName, kTypeRole, static_cast<int>(RowType::Design));
    item->setData(kName, kRootRole, row.root);
    item->setData(kName, kPathRole, design.path);
    item->setData(kName, kKeyRole, QStringLiteral("%1/%2").arg(projectKey, design.path));
    item->setData(kName, NativeControlIdentifierRole, QStringLiteral("projects.row.%1/%2").arg(projectKey, design.path));
    item->setFlags(Qt::ItemIsEnabled | Qt::ItemIsSelectable | Qt::ItemIsEditable);
    item->setToolTip(kName, designTip(row.root, design));
    item->setToolTip(kState, designTip(row.root, design));
  }
  for (auto it = folders.cbegin(); it != folders.cend(); ++it) {
    if (!it.key().isEmpty()) {
      it.value()->setExpanded(m_expanded.value(keyOf(it.value()), true));
    }
  }
  return top;
}

void ProjectsPanel::updateMarks() {
  if (m_nativeEditing) {
    // Not under the AppKit name editor (mitcad#816): a changed row would
    // replace its native cell, and the editor with it.
    m_marksWaiting = true;
    return;
  }
  if (!m_building) {
    applyMarks();
  }
}

void ProjectsPanel::applyMarks() {
  const QString open = m_host.openDesign ? m_host.openDesign() : QString();
  const QString current = m_host.currentRoot ? m_host.currentRoot() : QString();
  const ProjectIndicator::Remote remote = m_host.currentRemote ? m_host.currentRemote() : ProjectIndicator::Remote();
  for (QTreeWidgetItemIterator it(m_tree); *it != nullptr; ++it) {
    QTreeWidgetItem* item = *it;
    const RowType type = typeOf(item);
    if (type == RowType::Project) {
      if (const ProjectRow* row = project(rootOf(item))) {
        const bool isCurrent = !current.isEmpty() && sameRoot(row->root, current);
        // Compact marks (mitcad#529), the full words in the tooltip after
        // the name.
        const RowMarks marks = projectState(*row, isCurrent ? &remote : nullptr);
        item->setData(kName, kMarksRole, marks.logged());
        item->setText(kState, marks.text);
        item->setIcon(kState, marks.icon.isEmpty() ? QIcon() : themeIcon(marks.icon));
        QStringList tip = item->data(kName, kTipRole).toStringList();
        tip.insert(std::min<qsizetype>(1, tip.size()), marks.full);
        item->setToolTip(kName, tip.join(QLatin1Char('\n')));
        item->setToolTip(kState, tip.join(QLatin1Char('\n')));
        QFont font = item->font(kName);
        font.setBold(isCurrent);
        item->setFont(kName, font);
      }
    } else if (type == RowType::Design) {
      if (const ProjectDesignRow* design = designOf(item)) {
        const RowMarks marks = designState(*design);
        item->setData(kName, kMarksRole, marks.logged());
        item->setText(kState, marks.text);
        item->setIcon(kState, marks.icon.isEmpty() ? QIcon() : themeIcon(marks.icon));
        const bool isOpen = !open.isEmpty() && sameRoot(QDir(rootOf(item)).filePath(design->path), open);
        QFont font = item->font(kName);
        font.setBold(isOpen);
        item->setFont(kName, font);
        item->setFont(kState, font);
      }
    }
  }
  logTree();
  // The names as shown again (a bold name is wider).
  scheduleLayoutLog();
}

void ProjectsPanel::logTree() {
  const QString open = m_host.openDesign ? m_host.openDesign() : QString();
  const QString current = m_host.currentRoot ? m_host.currentRoot() : QString();
  const ProjectIndicator::Remote remote = m_host.currentRemote ? m_host.currentRemote() : ProjectIndicator::Remote();
  QStringList projects;
  QStringList names;
  QHash<QString, QString> lines;
  for (QTreeWidgetItemIterator it(m_tree); *it != nullptr; ++it) {
    QTreeWidgetItem* item = *it;
    if (typeOf(item) != RowType::Project) {
      continue;
    }
    const ProjectRow* row = project(rootOf(item));
    if (row == nullptr) {
      continue;
    }
    const QString key = keyOf(item);
    projects << key;
    names << QStringLiteral("%1 = %2%3").arg(key, item->text(kName),
                                             row->displayName.isEmpty() ? QString() : QStringLiteral(" (set)"));
    const bool isCurrent = !current.isEmpty() && sameRoot(row->root, current);
    QStringList entries;
    for (const QString& folder : row->folders) {
      entries << folder + QLatin1Char('/');
    }
    for (const ProjectDesignRow& design : row->designs) {
      QString entry = QStringLiteral("%1 %2").arg(design.path, designState(design).words);
      if (!open.isEmpty() && sameRoot(QDir(row->root).filePath(design.path), open)) {
        entry += QStringLiteral(", open");
      }
      entries << entry;
    }
    if (entries.isEmpty()) {
      entries << QStringLiteral("no designs");
    }
    lines.insert(key, QStringLiteral("Projects tree: %1 (%2%3): %4")
                          .arg(key, projectState(*row, isCurrent ? &remote : nullptr).words,
                               isCurrent ? QStringLiteral(", current") : QString(),
                               entries.join(QStringLiteral(" | "))));
  }
  QString counted = projects.size() == 1 ? QStringLiteral("Projects panel: 1 project")
                                         : QStringLiteral("Projects panel: %1 projects").arg(projects.size());
  if (!projects.isEmpty()) {
    counted += QStringLiteral(": ") + projects.join(QStringLiteral(" | "));
  }
  if (counted != m_loggedProjects) {
    m_loggedProjects = counted;
    qInfo().noquote() << counted;
  }
  // The rows at the top of the tree (mitcad#173): the projects' keys, in
  // their order; nothing else.
  QStringList topRows;
  for (int i = 0; i < m_tree->topLevelItemCount(); ++i) {
    topRows << keyOf(m_tree->topLevelItem(i));
  }
  const QString top = QStringLiteral("Projects top rows: %1").arg(topRows.join(QStringLiteral(" | ")));
  if (!topRows.isEmpty() && top != m_loggedTopRows) {
    m_loggedTopRows = top;
    qInfo().noquote() << top;
  }
  // The names shown (mitcad#152, #173): `Shop = Shop`, a Cloud project's
  // `gripper = github.com/acme/gripper`, `(set)` for a display name.
  const QString named = QStringLiteral("Projects names: %1").arg(names.join(QStringLiteral(" | ")));
  if (!names.isEmpty() && named != m_loggedNames) {
    m_loggedNames = named;
    qInfo().noquote() << named;
  }
  for (auto it = lines.cbegin(); it != lines.cend(); ++it) {
    if (m_loggedTrees.value(it.key()) != it.value()) {
      m_loggedTrees.insert(it.key(), it.value());
      qInfo().noquote() << it.value();
    }
  }
}

void ProjectsPanel::scheduleLayoutLog() {
  if (m_layoutLogPending) {
    return;
  }
  m_layoutLogPending = true;
  // Once the layout has placed the rows (UI tests click there).
  TestSync::singleShot(50, this, [this] {
    m_layoutLogPending = false;
    logLayout();
  });
}

void ProjectsPanel::logLayout() {
  if (!isVisible()) {
    return;
  }
  QStringList lines;
  // The indentation per level, the same as the Browser's (mitcad#393).
  lines << QStringLiteral("Projects indentation %1").arg(m_tree->indentation());
  const QRect viewport = m_tree->viewport()->rect();
  for (QTreeWidgetItemIterator it(m_tree); *it != nullptr; ++it) {
    const QRect row = m_tree->visualItemRect(*it);
    if (row.isEmpty() || !viewport.intersects(row)) {
      continue; // in a collapsed folder or scrolled away
    }
    const QPoint point = GlassCard::mapToHost(m_tree->viewport(), row.center());
    lines << QStringLiteral("Projects row %1 at %2,%3").arg(keyOf(*it)).arg(point.x()).arg(point.y());
  }
  for (QAction* action : m_toolbar->actions()) {
    QWidget* button = m_toolbar->widgetForAction(action);
    if (button == nullptr || !button->isVisible()) {
      continue;
    }
    const QPoint point = GlassCard::mapToHost(button, button->rect().center());
    // By the command's name, without the ellipsis of one that opens a
    // window ("Project Settings…" is logged "Project Settings"): the UI
    // tests click it by that name whichever way the label is written.
    QString name = action->text();
    if (name.endsWith(QChar(0x2026))) {
      name.chop(1);
    } else if (name.endsWith(QStringLiteral("..."))) {
      name.chop(3);
    }
    lines << QStringLiteral("Projects button %1 at %2,%3").arg(name).arg(point.x()).arg(point.y());
  }
  // The width and where its edge is dragged (mitcad#529): the card's grip,
  // or the window's separator right of the dock.
  lines << QStringLiteral("Projects width %1").arg(m_container != nullptr ? m_container->width() : width());
  if (auto* card = qobject_cast<GlassCard*>(m_container.data())) {
    if (card->isWidthAdjustable()) {
      const QPoint point = GlassCard::mapToHost(card, card->widthGripCenter());
      lines << QStringLiteral("Projects edge at %1,%2").arg(point.x()).arg(point.y());
    }
  } else if (auto* dock = qobject_cast<QDockWidget*>(m_container.data())) {
    auto* window = qobject_cast<QMainWindow*>(dock->parentWidget());
    if (window != nullptr && window->centralWidget() != nullptr && !dock->isFloating()) {
      const QRect docked = dock->geometry();
      const QRect central = window->centralWidget()->geometry();
      const int x = central.left() > docked.right() + 1 ? (docked.right() + 1 + central.left()) / 2 : docked.right() + 1;
      const QPoint point = GlassCard::mapToHost(window, QPoint(x, docked.center().y()));
      lines << QStringLiteral("Projects edge at %1,%2").arg(point.x()).arg(point.y());
    }
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_loggedLayout) {
    m_loggedLayout = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
  // Each row's name as shown, its marks and its tooltip (mitcad#529), when
  // they changed: a name elided in the middle keeps its start and end.
  for (QTreeWidgetItemIterator it(m_tree); *it != nullptr; ++it) {
    QTreeWidgetItem* item = *it;
    if (typeOf(item) == RowType::NewItem || typeOf(item) == RowType::Message) {
      continue;
    }
    const QString shown = m_tree->shownText(item, kName);
    if (shown.isEmpty()) {
      continue; // in a folded folder
    }
    const QString marks = item->data(kName, kMarksRole).toString();
    const QString line = QStringLiteral("Projects text %1: \"%2\" %3; tooltip: %4")
                             .arg(keyOf(item), shown, marks.isEmpty() ? QStringLiteral("none") : marks,
                                  item->toolTip(kName).split(QLatin1Char('\n')).join(QStringLiteral(" / ")));
    if (m_loggedTexts.value(keyOf(item)) != line) {
      m_loggedTexts.insert(keyOf(item), line);
      qInfo().noquote() << line;
    }
  }
}

int ProjectsPanel::contentHeight() const {
  int rows = 0;
  for (QTreeWidgetItemIterator it(m_tree); *it != nullptr; ++it) {
    bool shown = true;
    for (QTreeWidgetItem* up = (*it)->parent(); up != nullptr && shown; up = up->parent()) {
      shown = up->isExpanded();
    }
    rows += shown ? 1 : 0;
  }
  const int rowHeight = std::max(m_tree->sizeHintForRow(0), fontMetrics().height() + 6);
  return m_toolbar->sizeHint().height() + 2 + std::max(rows, 2) * rowHeight + 2 * m_tree->frameWidth() + 4;
}

void ProjectsPanel::updateActions() {
  QTreeWidgetItem* item = m_tree->currentItem();
  const RowType type = typeOf(item);
  const ProjectRow* row = project(rootOf(item));
  const bool healthy = row != nullptr && (row->kind == ProjectRow::Kind::Local || row->kind == ProjectRow::Kind::Cloud);
  const bool anyProject = healthy || (m_host.currentRoot && !m_host.currentRoot().isEmpty());
  m_newDesign->setEnabled(anyProject && (healthy || row == nullptr));
  m_newFolder->setEnabled(anyProject && (healthy || row == nullptr));
  m_delete->setEnabled(healthy && (type == RowType::Design || type == RowType::Folder));
  m_history->setEnabled(healthy && type == RowType::Design);
  m_sync->setEnabled(healthy && row->kind == ProjectRow::Kind::Cloud);
  m_settings->setEnabled(!settingsRoot(item).isEmpty());
}

QString ProjectsPanel::settingsRoot(const QTreeWidgetItem* item) const {
  if (item == nullptr || typeOf(item) == RowType::NewItem || typeOf(item) == RowType::Message) {
    return {};
  }
  const ProjectRow* row = project(rootOf(item));
  const bool healthy = row != nullptr && (row->kind == ProjectRow::Kind::Local || row->kind == ProjectRow::Kind::Cloud);
  return healthy ? row->root : QString();
}

bool ProjectsPanel::settingsOnDoubleClick(QTreeWidgetItem* item) {
  if (typeOf(item) != RowType::Project) {
    return false;
  }
  const QString root = settingsRoot(item);
  const ProjectRow* row = project(root);
  if (row == nullptr || (row->kind != ProjectRow::Kind::Cloud && !row->noRemoteChosen)) {
    return false; // a Local project expands
  }
  m_tree->setCurrentItem(item);
  qInfo().noquote() << QStringLiteral("Projects panel: settings of %1 (double-click on %2)").arg(root, keyOf(item));
  emit projectSettingsRequested(root);
  return true;
}

QString ProjectsPanel::stateText(const QString& root) const {
  const ProjectRow* row = project(root);
  if (row == nullptr) {
    return {};
  }
  const QString current = m_host.currentRoot ? m_host.currentRoot() : QString();
  const bool isCurrent = !current.isEmpty() && sameRoot(row->root, current);
  const ProjectIndicator::Remote remote = m_host.currentRemote ? m_host.currentRemote() : ProjectIndicator::Remote();
  return projectState(*row, isCurrent ? &remote : nullptr).full;
}

// ---------------------------------------------------------------------------
// Rows

ProjectsPanel::RowType ProjectsPanel::typeOf(const QTreeWidgetItem* item) const {
  return item == nullptr ? RowType::NewItem : static_cast<RowType>(item->data(kName, kTypeRole).toInt());
}

QString ProjectsPanel::rootOf(const QTreeWidgetItem* item) const {
  return item == nullptr ? QString() : item->data(kName, kRootRole).toString();
}

QString ProjectsPanel::pathOf(const QTreeWidgetItem* item) const {
  return item == nullptr ? QString() : item->data(kName, kPathRole).toString();
}

QString ProjectsPanel::keyOf(const QTreeWidgetItem* item) const {
  return item == nullptr ? QString() : item->data(kName, kKeyRole).toString();
}

QTreeWidgetItem* ProjectsPanel::projectItem(const QString& root) const {
  for (QTreeWidgetItemIterator it(m_tree); *it != nullptr; ++it) {
    if (typeOf(*it) == RowType::Project && sameRoot(rootOf(*it), root)) {
      return *it;
    }
  }
  return nullptr;
}

QTreeWidgetItem* ProjectsPanel::folderItem(const QString& root, const QString& folder) const {
  if (folder.isEmpty()) {
    return projectItem(root);
  }
  for (QTreeWidgetItemIterator it(m_tree); *it != nullptr; ++it) {
    if (typeOf(*it) == RowType::Folder && sameRoot(rootOf(*it), root) && pathOf(*it) == folder) {
      return *it;
    }
  }
  return nullptr;
}

const ProjectRow* ProjectsPanel::project(const QString& root) const {
  if (root.isEmpty()) {
    return nullptr;
  }
  for (const ProjectRow& row : m_rows) {
    if (sameRoot(row.root, root)) {
      return &row;
    }
  }
  return nullptr;
}

const ProjectDesignRow* ProjectsPanel::designOf(const QTreeWidgetItem* item) const {
  const ProjectRow* row = project(rootOf(item));
  if (row == nullptr || typeOf(item) != RowType::Design) {
    return nullptr;
  }
  const QString path = pathOf(item);
  for (const ProjectDesignRow& design : row->designs) {
    if (design.path == path) {
      return &design;
    }
  }
  return nullptr;
}

QString ProjectsPanel::folderFor(const QTreeWidgetItem* item) const {
  switch (typeOf(item)) {
  case RowType::Folder:
  case RowType::NewItem:
  case RowType::Message: // its path is the folder it is in
    return pathOf(item);
  case RowType::Design:
    return dirOf(pathOf(item));
  case RowType::Project:
    break;
  }
  return {};
}

QString ProjectsPanel::selectedRoot() const { return rootOf(m_tree->currentItem()); }

QString ProjectsPanel::selectedFolder() const { return folderFor(m_tree->currentItem()); }

void ProjectsPanel::reveal(const QString& root) {
  if (root.isEmpty()) {
    return;
  }
  m_revealed.insert(QDir::cleanPath(root));
  if (QTreeWidgetItem* item = projectItem(root)) {
    for (QTreeWidgetItem* up = item; up != nullptr; up = up->parent()) {
      up->setExpanded(true);
    }
    m_tree->setCurrentItem(item);
    m_tree->scrollToItem(item);
  } else {
    m_reveal = QDir::cleanPath(root);
  }
  refresh();
}

// ---------------------------------------------------------------------------
// Names typed in place

void ProjectsPanel::startNewDesign(const QString& root, const QString& folder) { startNew(false, root, folder); }

void ProjectsPanel::startNewFolder(const QString& root, const QString& folder) { startNew(true, root, folder); }

void ProjectsPanel::startNew(bool folder, const QString& root, const QString& parent) {
  if (m_isEditing) {
    cancelEditing();
  }
  m_hasRequest = false;
  const bool reading = m_task != nullptr;
  // Where the name is typed in sight first (mitcad#1268): a panel that is
  // shown now reads its projects (showEvent), and the name waits for them.
  revealForNaming();
  QTreeWidgetItem* host = folderItem(root, parent);
  if (host == nullptr || m_task != nullptr) {
    // Not read yet (a project just made, the panel just shown) or being
    // read: named once it is, from those rows.
    m_pending = {folder, QDir::cleanPath(root), parent};
    m_hasPending = true;
    m_revealed.insert(QDir::cleanPath(root));
    m_reveal = QDir::cleanPath(root);
    if (reading) {
      m_readAgain = true;
    } else if (m_task == nullptr) {
      refresh();
    }
    qInfo().noquote() << QStringLiteral("Projects panel: a new %1 in %2 is named once the projects are read")
                             .arg(folder ? QStringLiteral("folder") : QStringLiteral("design"),
                                  joinPath(QDir::cleanPath(root), parent));
    return;
  }
  for (QTreeWidgetItem* up = host; up != nullptr; up = up->parent()) {
    up->setExpanded(true);
  }
  const QString name = freeName(folder, rootOf(host), parent, host);
  QTreeWidgetItem* item = addNewRow(host, folder, parent, name);
  m_editing = Editing();
  m_editing.kind = RowType::NewItem;
  m_editing.folder = folder;
  m_editing.root = rootOf(host);
  m_editing.parent = parent;
  m_editing.item = item;
  m_isEditing = true;
  ++m_editSerial;
  m_tree->setCurrentItem(item);
  m_tree->scrollToItem(item);
  // The name proposed (mitcad#1268), as the editor shows it.
  const QString line = QStringLiteral("Projects panel: naming a new %1 in %2: %3")
                           .arg(folder ? QStringLiteral("folder") : QStringLiteral("design"), keyOf(host), name);
  openEditor(item, line);
  emit contentHeightChanged();
  scheduleLayoutLog();
}

QString ProjectsPanel::freeName(bool folder, const QString& root, const QString& parent,
                                const QTreeWidgetItem* host) const {
  // Among the folder's rows, and on disk (mitcad#1268): a design made since
  // the rows were read, one another program wrote or one not listed is
  // there all the same, and the core refuses a name that is (as it does on
  // a system that does not tell the case of letters apart, compared here
  // without it).
  QSet<QString> taken;
  for (int i = 0; host != nullptr && i < host->childCount(); ++i) {
    taken.insert(host->child(i)->text(kName).toLower());
  }
  const QDir dir(parent.isEmpty() ? root : QDir(root).filePath(parent));
  const QStringList entries = dir.entryList(QDir::AllEntries | QDir::Hidden | QDir::System | QDir::NoDotAndDotDot);
  for (const QString& entry : entries) {
    taken.insert(entry.toLower());
  }
  const QString base = folder ? tr("Folder") : tr("Design");
  for (int n = 1;; ++n) {
    const QString name = QStringLiteral("%1%2").arg(base).arg(n);
    const QString lower = name.toLower();
    if (!taken.contains(lower) && !taken.contains(lower + kSuffix)) {
      return name;
    }
  }
}

QTreeWidgetItem* ProjectsPanel::addNewRow(QTreeWidgetItem* host, bool folder, const QString& parent,
                                          const QString& name) {
  // A folder before the designs, a design after them.
  int firstDesign = host->childCount();
  for (int i = 0; i < host->childCount(); ++i) {
    if (typeOf(host->child(i)) == RowType::Design) {
      firstDesign = i;
      break;
    }
  }
  auto* item = new QTreeWidgetItem;
  if (folder) {
    host->insertChild(firstDesign, item);
  } else {
    host->addChild(item);
  }
  item->setText(kName, name);
  item->setIcon(kName, themeIcon(folder ? QStringLiteral("folder") : QStringLiteral("project")));
  item->setData(kName, kTypeRole, static_cast<int>(RowType::NewItem));
  item->setData(kName, kRootRole, rootOf(host));
  item->setData(kName, kPathRole, parent);
  item->setData(kName, kKeyRole, QStringLiteral("%1 (new)").arg(keyOf(host)));
  item->setData(kName, NativeControlIdentifierRole, QStringLiteral("projects.row.%1 (new)").arg(keyOf(host)));
  item->setFlags(Qt::ItemIsEnabled | Qt::ItemIsSelectable | Qt::ItemIsEditable);
  return item;
}

void ProjectsPanel::revealForNaming() {
  m_namingShown = true;
  if (m_host.showForNaming) {
    m_host.showForNaming(true);
  }
}

void ProjectsPanel::scheduleNamingEnd() {
  if (!m_namingShown) {
    return;
  }
  // Once the event loop is back: the window's answer to the name (queued
  // before this) may open the editor again (nameRefused).
  QMetaObject::invokeMethod(
      this,
      [this] {
        if (!m_namingShown || m_isEditing || m_hasPending) {
          return;
        }
        m_namingShown = false;
        if (m_host.showForNaming) {
          m_host.showForNaming(false);
        }
      },
      Qt::QueuedConnection);
}

void ProjectsPanel::showRefusal(QTreeWidgetItem* item, const QString& message) {
  clearRefusal();
  QTreeWidgetItem* parent = item != nullptr ? item->parent() : nullptr;
  if (parent == nullptr) {
    return;
  }
  // A row of its own below the name (its icon and text not greyed although
  // it takes no clicks: a click there would end the editor).
  auto* row = new QTreeWidgetItem;
  parent->insertChild(parent->indexOfChild(item) + 1, row);
  const QIcon warning = themeIcon(QStringLiteral("warning"));
  QIcon icon;
  const QPixmap mark = warning.pixmap(m_tree->iconSize(), devicePixelRatioF());
  icon.addPixmap(mark, QIcon::Normal);
  icon.addPixmap(mark, QIcon::Disabled);
  row->setText(kName, message);
  row->setIcon(kName, icon);
  row->setToolTip(kName, message);
  row->setForeground(kName, m_tree->palette().brush(QPalette::Active, QPalette::Text));
  row->setData(kName, kTypeRole, static_cast<int>(RowType::Message));
  row->setData(kName, kRootRole, rootOf(item));
  row->setData(kName, kPathRole, folderFor(item));
  row->setData(kName, kKeyRole, QStringLiteral("%1 (message)").arg(keyOf(item)));
  row->setData(kName, NativeControlIdentifierRole, QStringLiteral("projects.row.%1 (message)").arg(keyOf(item)));
  row->setFlags(Qt::NoItemFlags);
  row->setFirstColumnSpanned(true);
  m_messageItem = row;
  emit contentHeightChanged();
  scheduleLayoutLog();
}

void ProjectsPanel::clearRefusal() {
  if (m_messageItem != nullptr) {
    delete m_messageItem;
    m_messageItem = nullptr;
    emit contentHeightChanged();
  }
}

void ProjectsPanel::openEditor(QTreeWidgetItem* item, const QString& line) {
  // After the menu or the dialog that asked for it has closed and given the
  // focus back, so that the editor keeps it.
  const int serial = m_editSerial;
  TestSync::singleShot(0, this, [this, item, line, serial] {
    if (!m_isEditing || m_editing.item != item || serial != m_editSerial) {
      return;
    }
    m_tree->setCurrentItem(item);
    m_tree->scrollToItem(item);
    if (m_openNativeEditor) {
      // The AppKit presentation's own editor in its row, not the hidden
      // tree's (mitcad#816); its end comes back to finishNativeEditing.
      m_nativeEditing = true;
      qInfo().noquote() << line;
      if (!m_openNativeEditor(m_tree->indexOf(item), item->text(kName)) && m_nativeEditing) {
        m_nativeEditing = false;
        editingClosed(false);
      }
      return;
    }
    m_tree->setFocus(Qt::OtherFocusReason);
    m_tree->editItem(item, kName);
    // The editor has the focus and the name selected, so that typing
    // replaces it (mitcad#1268).
    if (auto* editor = qobject_cast<QLineEdit*>(m_tree->indexWidget(m_tree->indexOf(item)))) {
      editor->setFocus(Qt::OtherFocusReason);
      editor->selectAll();
    }
    qInfo().noquote() << line;
    logEditor(item);
    scheduleLayoutLog();
  });
}

void ProjectsPanel::logEditor(QTreeWidgetItem* item) {
  QWidget* editor = m_tree->indexWidget(m_tree->indexOf(item));
  const auto* field = qobject_cast<QLineEdit*>(editor);
  const bool focused = editor != nullptr && editor->window()->focusWidget() == editor;
  const bool selected = field != nullptr && (field->text().isEmpty() || field->selectedText() == field->text());
  const bool inSight = editor != nullptr && editor->isVisible() && isVisible() &&
                       m_tree->viewport()->rect().intersects(m_tree->visualItemRect(item));
  qInfo().noquote() << QStringLiteral("Projects panel: name editor on %1: \"%2\", %3, %4, %5")
                           .arg(keyOf(item), field != nullptr ? field->text() : item->text(kName),
                                focused ? QStringLiteral("focused") : QStringLiteral("not focused"),
                                selected ? QStringLiteral("all selected") : QStringLiteral("not all selected"),
                                inSight ? QStringLiteral("in sight") : QStringLiteral("not in sight"));
}

void ProjectsPanel::startRename(QTreeWidgetItem* item) {
  const RowType type = typeOf(item);
  if (type != RowType::Design && type != RowType::Folder) {
    return;
  }
  if (m_isEditing) {
    cancelEditing();
  }
  m_hasRequest = false;
  revealForNaming();
  m_editing = Editing();
  m_editing.kind = type;
  m_editing.folder = type == RowType::Folder;
  m_editing.root = rootOf(item);
  m_editing.parent = dirOf(pathOf(item));
  m_editing.from = pathOf(item);
  m_editing.item = item;
  m_isEditing = true;
  ++m_editSerial;
  m_tree->setCurrentItem(item);
  openEditor(item, QStringLiteral("Projects panel: renaming %1").arg(keyOf(item)));
}

bool ProjectsPanel::cancelEditing() {
  if (!m_isEditing) {
    return false;
  }
  // The editor, if it is open already.
  m_tree->cancelEdit();
  editingClosed(false);
  return true;
}

void ProjectsPanel::treeEditorClosed(bool accepted, bool hidden) {
  const int serial = m_editSerial;
  // Once the view is done with the editor (the row of a new name goes).
  QMetaObject::invokeMethod(
      this,
      [this, serial, accepted, hidden] {
        if (serial != m_editSerial || !m_isEditing || m_nativeEditing) {
          return; // ended already, or another editor's
        }
        if (accepted && hidden) {
          // A click elsewhere ended an editor nobody saw: not taken.
          qInfo().noquote()
              << QStringLiteral("Projects panel: the name editor was not in sight: the name is not taken");
          editingClosed(false);
          return;
        }
        editingClosed(accepted);
      },
      Qt::QueuedConnection);
}

void ProjectsPanel::editingClosed(bool accepted) {
  if (!m_isEditing) {
    return;
  }
  if (m_nativeEditing) {
    // Closed from here (Esc in the window, another name started): the
    // AppKit editor goes without a result of its own (mitcad#816).
    m_nativeEditing = false;
    if (m_closeNativeEditor) {
      m_closeNativeEditor();
    }
  }
  clearRefusal();
  const Editing editing = m_editing;
  const QString typed = editing.item != nullptr ? editing.item->text(kName).trimmed() : QString();
  const QString what = namingWhat(editing.kind == RowType::NewItem, editing.folder, editing.from);
  // The naming ends: a new name's row goes (the project's tree read again
  // shows what was made).
  const auto end = [this, &editing] {
    m_isEditing = false;
    m_editing = Editing();
    if (editing.kind == RowType::NewItem && editing.item != nullptr) {
      delete editing.item;
    }
  };
  if (!accepted || editing.item == nullptr) {
    end();
    qInfo().noquote() << QStringLiteral("Projects panel: %1 cancelled").arg(what);
    if (editing.kind != RowType::NewItem && editing.item != nullptr) {
      editing.item->setText(kName, editing.folder ? lastPart(editing.from) : designName(editing.from));
    }
    applyWaitingRows();
    scheduleNamingEnd();
    return;
  }
  QString name = typed;
  if (!editing.folder && name.endsWith(kSuffix, Qt::CaseInsensitive)) {
    name.chop(kSuffix.size());
    name = name.trimmed();
  }
  static const QString forbidden = QStringLiteral("/\\<>:\"|?*");
  bool bad = name.isEmpty() || name.startsWith(QLatin1Char('.')) || name.endsWith(QLatin1Char('.'));
  for (const QChar c : name) {
    bad = bad || forbidden.contains(c) || c.category() == QChar::Other_Control;
  }
  if (bad) {
    qInfo().noquote() << QStringLiteral("Projects panel: %1 refused: the name \"%2\"").arg(what, name);
    const QString message = tr("“%1” cannot be a name: it may not be empty, start or end with a dot, or hold "
                               "/ \\ < > : \" | ? *.")
                                .arg(name);
    emit statusMessage(message, true);
    // The editor stays on the row with what was typed, the refusal below it
    // (mitcad#1268), so that another name can be typed.
    showRefusal(editing.item, message);
    ++m_editSerial;
    openEditor(editing.item,
               QStringLiteral("Projects panel: %1 refused at %2: %3").arg(what, keyOf(editing.item), message));
    return;
  }
  const QString file = editing.folder ? name : name + kSuffix;
  const QString path = joinPath(editing.parent, file);
  end();
  if (editing.kind != RowType::NewItem && path == editing.from) {
    applyWaitingRows();
    qInfo().noquote() << QStringLiteral("Projects panel: %1: the same name").arg(what);
    scheduleNamingEnd();
    return;
  }
  if (editing.kind == RowType::NewItem) {
    qInfo().noquote() << QStringLiteral("Projects panel: %1 %2 in %3").arg(what, path, editing.root);
  } else {
    qInfo().noquote() << QStringLiteral("Projects panel: rename %1 to %2").arg(editing.from, path);
  }
  applyWaitingRows();
  // Answered by the window: a refusal opens the editor again (nameRefused).
  m_request = Request();
  m_request.root = editing.root;
  m_request.path = path;
  m_request.from = editing.kind == RowType::NewItem ? QString() : editing.from;
  m_request.folder = editing.folder;
  m_hasRequest = true;
  if (editing.kind != RowType::NewItem) {
    emit moveRequested(editing.root, editing.from, path, editing.folder);
  } else if (editing.folder) {
    emit newFolderRequested(editing.root, path);
  } else {
    emit newDesignRequested(editing.root, path);
  }
  scheduleNamingEnd();
}

bool ProjectsPanel::nameRefused(const QString& root, const QString& path, const QString& from,
                                const QString& message) {
  if (!m_hasRequest || !sameRoot(m_request.root, root) || m_request.path != path || m_request.from != from) {
    return false;
  }
  const Request request = m_request;
  m_hasRequest = false;
  if (m_isEditing) {
    cancelEditing();
  }
  const QString parent = dirOf(path);
  const QString name = request.folder ? lastPart(path) : designName(path);
  Editing editing;
  editing.folder = request.folder;
  QTreeWidgetItem* item = nullptr;
  if (from.isEmpty()) {
    QTreeWidgetItem* host = folderItem(root, parent);
    if (host == nullptr) {
      return false;
    }
    for (QTreeWidgetItem* up = host; up != nullptr; up = up->parent()) {
      up->setExpanded(true);
    }
    item = addNewRow(host, request.folder, parent, name);
    editing.kind = RowType::NewItem;
    editing.root = rootOf(host);
    editing.parent = parent;
  } else {
    for (QTreeWidgetItemIterator it(m_tree); *it != nullptr && item == nullptr; ++it) {
      const RowType type = typeOf(*it);
      if ((type == RowType::Design || type == RowType::Folder) && sameRoot(rootOf(*it), root) && pathOf(*it) == from) {
        item = *it;
      }
    }
    if (item == nullptr) {
      return false;
    }
    item->setText(kName, name);
    editing.kind = typeOf(item);
    editing.root = rootOf(item);
    editing.parent = dirOf(from);
    editing.from = from;
  }
  editing.item = item;
  m_editing = editing;
  m_isEditing = true;
  ++m_editSerial;
  revealForNaming();
  showRefusal(item, message);
  m_tree->setCurrentItem(item);
  m_tree->scrollToItem(item);
  const QString what = namingWhat(editing.kind == RowType::NewItem, editing.folder, editing.from);
  openEditor(item, QStringLiteral("Projects panel: %1 refused at %2: %3").arg(what, keyOf(item), message));
  emit contentHeightChanged();
  scheduleLayoutLog();
  return true;
}

// ---------------------------------------------------------------------------
// Actions

void ProjectsPanel::openCurrent() {
  QTreeWidgetItem* item = m_tree->currentItem();
  switch (typeOf(item)) {
  case RowType::Design:
    qInfo().noquote() << QStringLiteral("Projects panel: open %1").arg(keyOf(item));
    emit openRequested(QDir::cleanPath(QDir(rootOf(item)).filePath(pathOf(item))));
    break;
  case RowType::Project:
    if (const ProjectRow* row = project(rootOf(item)); row != nullptr && row->kind == ProjectRow::Kind::NoHistory) {
      emit openProjectRequested(row->root);
      break;
    }
    item->setExpanded(!item->isExpanded());
    break;
  case RowType::Folder:
    item->setExpanded(!item->isExpanded());
    break;
  case RowType::NewItem:
  case RowType::Message:
    break;
  }
}

void ProjectsPanel::renameCurrent() {
  QTreeWidgetItem* item = m_tree->currentItem();
  const ProjectRow* row = project(rootOf(item));
  if (row != nullptr && (row->kind == ProjectRow::Kind::Local || row->kind == ProjectRow::Kind::Cloud)) {
    startRename(item);
  }
}

void ProjectsPanel::deleteCurrent() {
  QTreeWidgetItem* item = m_tree->currentItem();
  const RowType type = typeOf(item);
  const ProjectRow* row = project(rootOf(item));
  if (row == nullptr || (row->kind != ProjectRow::Kind::Local && row->kind != ProjectRow::Kind::Cloud) ||
      (type != RowType::Design && type != RowType::Folder)) {
    return;
  }
  qInfo().noquote() << QStringLiteral("Projects panel: delete %1").arg(keyOf(item));
  emit deleteRequested(rootOf(item), pathOf(item), type == RowType::Folder);
}

void ProjectsPanel::contextMenu(const QPoint& at) {
  QTreeWidgetItem* item = m_tree->itemAt(at);
  if (item != nullptr && typeOf(item) == RowType::Message) {
    return; // a refusal's row: nothing to do there, and the editor above it stays
  }
  if (item != nullptr && item != m_tree->currentItem()) {
    m_tree->setCurrentItem(item); // a right click selects the row
  }
  LoggedMenu menu(this);
  const RowType type = typeOf(item);
  const QString root = rootOf(item);
  const ProjectRow* row = project(root);
  const bool healthy = row != nullptr && (row->kind == ProjectRow::Kind::Local || row->kind == ProjectRow::Kind::Cloud);
  const QString folder = folderFor(item);
  if (item != nullptr && type == RowType::Design && healthy) {
    menu.add(QString(), tr("Open"), [this] { openCurrent(); });
    menu.add(QStringLiteral("edit"), tr("Rename"), [this] { renameCurrent(); });
    menu.add(QStringLiteral("delete"), tr("Delete"), [this] { deleteCurrent(); });
    menu.add(QStringLiteral("version-history"), tr("Version History…"),
             [this, root, path = pathOf(item)] { emit versionHistoryRequested(root, path); });
    menu.add(QString(), tr("Show in Folder"),
             [this, root, folder] { emit showInFolderRequested(QDir(root).filePath(folder)); });
    menu.separator();
  }
  if (item != nullptr && type == RowType::Folder && healthy) {
    menu.add(QStringLiteral("new-design"), tr("New Design"), [this, root, folder] { startNewDesign(root, folder); });
    menu.add(QStringLiteral("folder"), tr("New Folder"), [this, root, folder] { startNewFolder(root, folder); });
    menu.add(QStringLiteral("edit"), tr("Rename"), [this] { renameCurrent(); });
    menu.add(QStringLiteral("delete"), tr("Delete"), [this] { deleteCurrent(); });
    menu.add(QString(), tr("Show in Folder"),
             [this, root, folder] { emit showInFolderRequested(QDir(root).filePath(folder)); });
    menu.exec(m_tree->viewport()->mapToGlobal(at));
    return;
  }
  if (item != nullptr && type == RowType::Design && healthy) {
    menu.add(QStringLiteral("new-design"), tr("New Design"), [this, root, folder] { startNewDesign(root, folder); });
    menu.add(QStringLiteral("folder"), tr("New Folder"), [this, root, folder] { startNewFolder(root, folder); });
    menu.exec(m_tree->viewport()->mapToGlobal(at));
    return;
  }
  if (item != nullptr && type == RowType::Project && row != nullptr) {
    if (healthy) {
      menu.add(QStringLiteral("new-design"), tr("New Design"), [this, root] { startNewDesign(root, QString()); });
      menu.add(QStringLiteral("folder"), tr("New Folder"), [this, root] { startNewFolder(root, QString()); });
      menu.add(QString(), tr("Show in Folder"), [this, root] { emit showInFolderRequested(root); });
      menu.add(QStringLiteral("settings"), tr("Project Settings…"), [this, root] { emit projectSettingsRequested(root); });
      if (row->kind == ProjectRow::Kind::Cloud) {
        menu.add(QStringLiteral("sync"), tr("Sync Now"), [this, root] { emit syncRequested(root); });
      }
    } else if (row->kind == ProjectRow::Kind::NoHistory) {
      menu.add(QString(), tr("Open Project"), [this, root] { emit openProjectRequested(root); });
      menu.add(QString(), tr("Show in Folder"), [this, root] { emit showInFolderRequested(root); });
    }
    menu.separator();
    menu.add(QString(), tr("Remove from List"), [this, root] { emit removeFromListRequested(root); });
    // The folder stops being a project (mitcad#1562); not for a folder
    // that is gone or cannot be read.
    if (healthy || row->kind == ProjectRow::Kind::NoHistory) {
      menu.add(QString(), tr("Remove Project…"), [this, root] { emit removeProjectRequested(root); });
    }
  }
  menu.separator();
  menu.add(QStringLiteral("new-project"), tr("New Project…"),
           [this] { emit commandRequested(QStringLiteral("file.new_project")); });
  menu.add(QStringLiteral("remote-open"), tr("Open from Cloud…"),
           [this] { emit commandRequested(QStringLiteral("file.open_remote")); });
  menu.add(QStringLiteral("folder"), tr("Add Existing Project Folder…"), [this] { emit addFolderRequested(); });
  menu.exec(m_tree->viewport()->mapToGlobal(at));
}

bool ProjectsPanel::dropTarget(const QTreeWidgetItem* dragged, const QTreeWidgetItem* over, QString* folder) const {
  if (dragged == nullptr || over == nullptr || dragged == over) {
    return false;
  }
  const RowType type = typeOf(dragged);
  if (type != RowType::Design && type != RowType::Folder) {
    return false;
  }
  const QString root = rootOf(dragged);
  if (root.isEmpty() || !sameRoot(rootOf(over), root)) {
    return false; // not between projects (copying is a later step)
  }
  QString target;
  switch (typeOf(over)) {
  case RowType::Project:
    break;
  case RowType::Folder:
    target = pathOf(over);
    break;
  case RowType::Design:
    target = dirOf(pathOf(over));
    break;
  case RowType::NewItem:
  case RowType::Message:
    return false;
  }
  const QString from = pathOf(dragged);
  if (dirOf(from) == target) {
    return false; // where it is
  }
  if (type == RowType::Folder && (target == from || target.startsWith(from + QLatin1Char('/')))) {
    return false; // into itself
  }
  if (folder != nullptr) {
    *folder = target;
  }
  return true;
}

void ProjectsPanel::dropped(QTreeWidgetItem* dragged, QTreeWidgetItem* over) {
  QString folder;
  if (!dropTarget(dragged, over, &folder)) {
    qInfo().noquote() << QStringLiteral("Projects panel: drop of %1 refused%2")
                             .arg(keyOf(dragged), over != nullptr ? QStringLiteral(" on %1").arg(keyOf(over))
                                                                  : QString());
    if (over != nullptr && dragged != nullptr && !rootOf(over).isEmpty() && !sameRoot(rootOf(over), rootOf(dragged))) {
      emit statusMessage(tr("Designs and folders move only within their project."), false);
    }
    return;
  }
  const QString from = pathOf(dragged);
  const QString to = joinPath(folder, lastPart(from));
  qInfo().noquote() << QStringLiteral("Projects panel: drag %1 to %2").arg(from, to);
  emit moveRequested(rootOf(dragged), from, to, typeOf(dragged) == RowType::Folder);
}

// ---------------------------------------------------------------------------
// The AppKit presentation's keys, name editor and drags (mitcad#816)

bool ProjectsPanel::treeKey(int key) {
  if (m_tree->editing() || m_tree->currentItem() == nullptr) {
    return false;
  }
  if (key == Qt::Key_F2) {
    renameCurrent();
    return true;
  }
  if (isDeleteKey(key)) {
    deleteCurrent();
    return true;
  }
  if (key == Qt::Key_Return || key == Qt::Key_Enter) {
    openCurrent();
    return true;
  }
  return false;
}

void ProjectsPanel::setNativeEditor(std::function<bool(const QModelIndex&, const QString&)> openRow,
                                    std::function<void()> closeRow) {
  m_openNativeEditor = std::move(openRow);
  m_closeNativeEditor = std::move(closeRow);
}

void ProjectsPanel::finishNativeEditing(const QString& text, bool accepted) {
  if (!m_nativeEditing || !m_isEditing) {
    return;
  }
  m_nativeEditing = false;
  // What the tree's delegate does: the typed name into the row, then the
  // editor's end.
  if (accepted && m_editing.item != nullptr) {
    m_editing.item->setText(kName, text);
  }
  editingClosed(accepted);
}

bool ProjectsPanel::canDragRow(const QModelIndex& index) const {
  const RowType type = typeOf(m_tree->itemOf(index));
  return !m_isEditing && (type == RowType::Design || type == RowType::Folder);
}

bool ProjectsPanel::canDropRow(const QModelIndex& dragged, const QModelIndex& over) const {
  return dropTarget(m_tree->itemOf(dragged), m_tree->itemOf(over), nullptr);
}

void ProjectsPanel::dropRow(const QModelIndex& dragged, const QModelIndex& over) {
  if (QTreeWidgetItem* item = m_tree->itemOf(dragged)) {
    dropped(item, m_tree->itemOf(over));
  }
}

void ProjectsPanel::setNativeDragging(const QModelIndex& dragged, bool dragging) {
  if (m_nativeDragging == dragging) {
    return;
  }
  m_nativeDragging = dragging;
  if (dragging) {
    qInfo().noquote() << QStringLiteral("Projects panel: dragging %1").arg(keyOf(m_tree->itemOf(dragged)));
  } else {
    applyWaitingRows();
  }
}

// ---------------------------------------------------------------------------
// A design revealed from outside the panel (mitcad#174)

void ProjectsPanel::revealDesign(const QString& root, const QString& path) {
  if (root.isEmpty() || path.isEmpty()) {
    return;
  }
  m_revealDesignRoot = QDir::cleanPath(root);
  m_revealDesignPath = path;
  // Each rebuild ends with contentHeightChanged: the design is selected
  // once a read lists it.
  connect(this, &ProjectsPanel::contentHeightChanged, this, &ProjectsPanel::selectRevealedDesign,
          Qt::UniqueConnection);
  reveal(root);
  selectRevealedDesign();
}

void ProjectsPanel::selectRevealedDesign() {
  if (m_revealDesignPath.isEmpty() || m_building) {
    return;
  }
  for (QTreeWidgetItemIterator it(m_tree); *it != nullptr; ++it) {
    QTreeWidgetItem* item = *it;
    if (typeOf(item) != RowType::Design || pathOf(item) != m_revealDesignPath ||
        !sameRoot(rootOf(item), m_revealDesignRoot)) {
      continue;
    }
    m_revealDesignRoot.clear();
    m_revealDesignPath.clear();
    disconnect(this, &ProjectsPanel::contentHeightChanged, this, &ProjectsPanel::selectRevealedDesign);
    for (QTreeWidgetItem* up = item->parent(); up != nullptr; up = up->parent()) {
      up->setExpanded(true);
    }
    m_tree->setCurrentItem(item);
    m_tree->scrollToItem(item);
    return;
  }
}

} // namespace mitcad
