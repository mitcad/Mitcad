// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <memory>
#include <vector>

#include <QList>
#include <QIcon>
#include <QMargins>
#include <QPoint>
#include <QRect>
#include <QString>
#include <QStringList>
#include <QtGlobal>

class QWidget;

namespace mitcad::mac {

// Presentation snapshots: identifiers belong to the document/controller;
// the native shell never constructs model commands or interprets model IDs.
struct NativeMacAction {
  QString id;
  QString label;
  QString group;
  QString workspace;
  QString symbol;
  QString shortcut; // QKeySequence::PortableText; Ctrl is Command on macOS.
  QStringList shortcuts; // Effective primary and alternate bindings.
  bool enabled = true;
  bool checked = false;
  QString menu; // Optional top-level menu, e.g. File, Edit or View.
  QString tooltip;
  QStringList keywords;
  QIcon icon; // Existing CAD icon if no SF Symbol expresses the command.
  bool iconTemplate = false; // Monochrome alpha mask; AppKit supplies its tint.
  bool toolbarPinned = false; // Same visible command membership as the CAD ribbon.
  int toolbarOrder = -1; // Registry order, independent of menu traversal order.
  QString submenu; // Optional child menu path for a toolbar popup action.
  // A ribbon subgroup's pinned default (mitcad#222): a toolbar button of its
  // group, whose menu lists it under the subgroup's popup entry instead.
  bool toolbarOnly = false;
  bool separator = false;
  // The section of its group's menu (CommandDef::section, mitcad#579; a
  // subgroup's popup entry: its first command's): the group menu has a
  // separator between consecutive entries of different sections.
  QString section;
};

// Both frontends consume the existing ribbon's menu tree and button order.
// These snapshots contain presentation only; actions retain their shared IDs.
struct NativeMacMenuItem {
  QString action;
  QString title;
  QIcon icon;
  bool separator = false;
  QList<NativeMacMenuItem> children;
  bool submenu = false;
  bool operator==(const NativeMacMenuItem& other) const {
    return action == other.action && title == other.title && icon.cacheKey() == other.icon.cacheKey() &&
           separator == other.separator && submenu == other.submenu && children == other.children;
  }
};

struct NativeMacRibbonButton {
  QString action;
  int width = 36;
  int iconSize = 26;
  bool menuOnly = false;
  QList<NativeMacMenuItem> menu;
  bool operator==(const NativeMacRibbonButton& other) const {
    return action == other.action && width == other.width && iconSize == other.iconSize &&
           menuOnly == other.menuOnly && menu == other.menu;
  }
};

struct NativeMacRibbonGroup {
  QString workspace;
  QString name;
  QString title;
  int fullWidth = 0;
  int collapsedWidth = 0;
  QList<NativeMacRibbonButton> buttons;
  QList<NativeMacMenuItem> menu;
  bool operator==(const NativeMacRibbonGroup& other) const {
    return workspace == other.workspace && name == other.name && title == other.title &&
           fullWidth == other.fullWidth && collapsedWidth == other.collapsedWidth &&
           buttons == other.buttons && menu == other.menu;
  }
};

struct NativeMacTreeItem {
  QString id; // Unique presentation key, also used by parentId and callbacks.
  QString parentId;
  QString title;
  QString detail;
  QString symbol;
  QIcon icon; // The browser's shared CAD artwork.
  bool expanded = false;
  bool hasActivation = false;
  bool active = false;
  bool enabled = true;
  bool checked = false;
  QString kind; // body, component, occurrence, feature, folder, ...
  bool editable = false;
  bool canToggleVisibility = false;
  bool visible = true;
  bool hasVisibility = false;
  int bindings = 0;
};

struct NativeMacHistoryItem {
  QString id;
  QString title;
  QString detail;
  QString symbol;
  QIcon icon; // Same coloured feature artwork as the shared CAD timeline.
  QString status; // ok, warning, error, suppressed, rolled_back, pending.
  bool noted = false; // Import note: warning outline even for a placeholder.
  bool suppressed = false;
  bool enabled = true;
  bool filtered = false; // Hidden by the shared component timeline filter.
  int bindings = 0;
};

struct NativeMacWindowCallbacks {
  std::function<void(const QString&)> action;
  std::function<void(const QStringList&)> selection;
  std::function<void(const QString&)> activate;
  std::function<void(const QString&, const QString&)> rename;
  std::function<void(const QString&)> historyActivated;
  std::function<void(const QString&)> treeDelete;
  std::function<void(const QString&)> historyDelete;
  std::function<void(const QString&)> historyRename;
  std::function<bool()> requestClose;
  std::function<void()> closed;
  std::function<void(const QString&, bool)> visibilityChanged;
  std::function<void(int)> historyMarkerChanged;
  // Start only when historyCanChange permits it. Once started, finish must
  // be delivered even while busy; the shared controller waits before
  // merging the drag's model changes into one undo step.
  std::function<bool()> historyCanChange;
  std::function<void()> historyMarkerDragStarted;
  std::function<void(int)> historyMarkerDragged;
  std::function<void(int)> historyMarkerDragFinished;
  std::function<void(const QString&)> treeVisibility;
  std::function<void(const QString&)> componentActivated;
  std::function<void(const QString&, bool)> treeExpanded;
  std::function<void(bool, const QPoint&)> browserIssues;
  std::function<void(const QString&, const QPoint&)> treeContextMenu;
  std::function<void(const QString&, const QRect&, bool)> treeBindings;
  std::function<void(const QStringList&)> historySelection;
  std::function<void(const QString&, int)> historyReordered;
  std::function<void(const QString&, const QPoint&)> historyContextMenu;
  std::function<void(const QString&, const QRect&, bool)> historyBindings;
  std::function<void(const QPoint&)> historyFilterMenu;
  std::function<void()> timelineSearchOpened;
  std::function<void(const QString&)> timelineSearchChanged;
  std::function<void(int)> timelineSearchStepped;
  std::function<void(int)> timelineSearchSelected;
  std::function<void()> timelineSearchClosed;
  std::function<void(const QPoint&)> timelineSearchFilterMenu;
  std::function<void(const QPoint&)> projectMenu;
  // The agent indicator (mitcad#710): a click (Connect AI Assistant or the
  // menu, as the Qt indicator decides) and its menu.
  std::function<void(const QPoint&)> agentClicked;
  std::function<void(const QPoint&)> agentMenu;
  std::function<void(int)> workspaceChanged;
  std::function<void(const QString&)> openFile;
  std::function<bool()> confirm;
  std::function<bool()> cancel;
  // Native menu tracking may consume a mouse release without a Qt popup
  // event. Stop the canvas interaction when AppKit takes or returns input.
  std::function<void()> cancelPointerInteraction;
  // Logical viewport coordinates, with the origin at its top-left corner.
  std::function<void(const QMargins&, const std::vector<QRect>&)> viewportOverlays;
  // A side card's edge released after a drag (mitcad#818): "projects",
  // "browser" or "inspector" and the width it remembers, in points.
  std::function<void(const QString&, int)> cardWidthChosen;
  // A plain key (Qt::Key) with the Projects outline focused, before default
  // buttons and document shortcuts; true when the shared panel took it:
  // Return opens, F2 renames, Delete deletes (mitcad#816).
  std::function<bool(int)> projectsKey;
};

#ifdef Q_OS_MACOS

struct NativeMacWindowState;

// In-process native input acceptance check; called by NativeTestInput.
bool nativeWindowRefreshContract(void* window, QString& error);

// Owns an AppKit window and its controls, but not the supplied Qt viewport.
// The viewport must be a standalone QWidget containing only the CAD canvas
// and CAD-specific overlays. Its public QWindow handle is embedded in a
// foreign NSView; QApplication's existing event loop services both toolkits.
class NativeMacWindow final {
public:
  explicit NativeMacWindow(QWidget* viewport, NativeMacWindowCallbacks callbacks);
  ~NativeMacWindow();
  NativeMacWindow(const NativeMacWindow&) = delete;
  NativeMacWindow& operator=(const NativeMacWindow&) = delete;

  bool isEmbedded() const;
  bool modalBlocked() const;
  QString embeddingError() const;
  void show();
  void close();
  void focusViewport();

  void setActions(const QList<NativeMacAction>& actions);
  void setRibbon(const QList<NativeMacRibbonGroup>& groups, const QStringList& workspaces, int selected);
  void setModelItems(const QList<NativeMacTreeItem>& items);
  void setSelection(const QStringList& keys);
  void setHistory(const QList<NativeMacHistoryItem>& items, int marker = -1);
  void setHistorySelection(const QStringList& keys);
  void setBrowserIssues(const QString& failures, const QString& failureDetails,
                        const QString& warnings, const QString& warningDetails);
  void setHistoryFilter(const QString& label, bool active);
  void setProjectStatus(const QString& text, const QString& tooltip);
  void setAgentStatus(const QString& text, const QString& tooltip, bool connected, bool available);
  void setDocument(const QString& title, const QString& path, bool edited);
  void setSketchMode(bool sketching);
  void setProjectsView(void* view, bool shown);
  void setConfigurationView(void* view);
  bool projectsShown() const;
  // Names a Projects row in place (mitcad#816): the shown outline's row
  // whose cell has `identifier` edits `text` in its own text field, its
  // content disclosed first; `finished` gets the name and whether it was
  // taken. False when the row is not shown. closeProjectsEditor ends an
  // open editor without calling `finished`.
  bool editProjectsRow(const QString& identifier, const QString& text,
                       std::function<void(const QString&, bool)> finished);
  void closeProjectsEditor();
  // The shared status pill supplies its lifetime; a new message restarts it.
  void setStatus(const QString& message, bool error, int milliseconds);
  void setInspectorText(const QString& title, const QString& text);
  // The shell retains this NSView while installed. nullptr clears it. A
  // footer (a command form's OK and Cancel, mitcad#815) stays under the
  // view, which scrolls above it.
  void setInspectorView(void* view, void* footer = nullptr);
  void showInspector(bool shown = true);
  void showSidebar(bool shown = true);
  void showHistory(bool shown = true);
  void showCommandSearch();
  void showTimelineSearch();
  void setTimelineSearchResults(const QString& text, const QString& count,
                                const QList<NativeMacHistoryItem>& results, int selected);
  void saveTimelineSearchScroll();
  void restoreTimelineSearchScroll();

  void* nativeWindow() const; // NSWindow*, borrowed.
  void* nativeInspectorHost() const; // NSView*, borrowed.
  // A side card's remembered width (mitcad#818), named as cardWidthChosen
  // names it; placed within the canvas as a dragged width is.
  void setCardWidth(const QString& card, int width);

private:
  std::unique_ptr<NativeMacWindowState> m_state;
};

#endif

} // namespace mitcad::mac
