// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <memory>
#include <utility>
#include <vector>

#include <QList>
#include <QIcon>
#include <QMargins>
#include <QPoint>
#include <QRect>
#include <QString>
#include <QStringList>
#include <QtGlobal>

#include "framework/NoticeLevel.hpp"

class QWidget;

namespace mitcad {
class MessageLog;
}

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
  // Pin to Toolbar (mitcad#1074, after mitcad#1084): pinned, a button of
  // its own group; pinnable when it is neither pinned nor a button of its
  // group already.
  bool pinnedToToolbar = false;
  bool pinnable = false;
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
  bool pinned = false; // a command the user pinned in this group (mitcad#1084)
  bool operator==(const NativeMacRibbonButton& other) const {
    return action == other.action && width == other.width && iconSize == other.iconSize &&
           menuOnly == other.menuOnly && menu == other.menu && pinned == other.pinned;
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
  // The name's colour as the shared row's (mitcad#1079): "failed" (red),
  // "warning", "dimmed" (grey: suppressed, rolled back, hidden) or none.
  QString tone;
  // A light bulb that follows a Show When condition, or a folder's of such
  // rows: its tooltip in place of Hide / Show; `eyeBound` its fx mark.
  QString eyeTip;
  bool eyeBound = false;
};

struct NativeMacHistoryItem {
  QString id;
  QString title;
  QString detail;
  QString symbol;
  QIcon icon; // Same coloured feature artwork as the shared CAD timeline.
  QString status; // ok, warning, error, suppressed, rolled_back, pending.
  bool noted = false; // Import note: information (mitcad#1040), its badge even for a placeholder.
  bool suppressed = false;
  bool enabled = true;
  bool filtered = false; // Hidden by the shared component timeline filter.
  int bindings = 0;
  // Its timeline group (mitcad#1077) as the shared timeline shows it: the
  // group's name on each of its first to last features the filter shows
  // (empty: none), and whether the group is folded into one cell.
  QString group;
  bool groupCollapsed = false;
};

// An arrow of the orientation cube (GlassButton in the Qt view): a round
// button over the canvas that turns the view to the face beside the one in
// sight or rolls it.
struct NativeMacCubeArrow {
  QString id; // up, down, left, right, cw, ccw (OcctViewer::pressCubeArrow)
  QString toolTip;
  QString symbol; // SF Symbol
  QRect geometry; // Logical viewport coordinates, origin at its top-left corner.
};

// The quick-access group (mitcad#1113, the Qt QuickAccessGroup of
// mitcad#1090): its commands in the menus' order, each button's tooltip
// (the command's name, its key and, while disabled, why), the commands its
// menu offers as checks (File, Edit, View and Tools, their submenus as
// submenus; a leaf's action is the command's id) and the checked ones.
struct NativeMacQuickAccess {
  QStringList shown;
  QStringList tooltips; // one per shown command
  QList<NativeMacMenuItem> menus;
  QStringList chosen;
  bool operator==(const NativeMacQuickAccess& other) const {
    return shown == other.shown && tooltips == other.tooltips && menus == other.menus && chosen == other.chosen;
  }
};

// The navigation bar at the bottom of the view (mitcad#1099, the Qt bar of
// mitcad#1086): its buttons' commands in order (Look At, Fit, the Visual
// Style button, Layout Grid, Snap to Grid). setActions gives their artwork,
// checks, enabled states and tooltips (the command's name and shortcut), a
// click runs the command as its View menu item does. `style` is the Visual
// Style button (its id among `commands`, its name and artwork): a click
// pops up `styles`, the View menu's style commands with the current one
// checked (an empty id: a separator).
struct NativeMacNavigationBar {
  QStringList commands;
  NativeMacAction style;
  QStringList styles;
  bool shown = false;
};

struct NativeMacWindowCallbacks {
  std::function<void(const QString&)> action;
  std::function<void(const QStringList&)> selection;
  std::function<void(const QString&)> activate;
  std::function<void(const QString&, const QString&)> rename;
  std::function<void(const QString&)> historyActivated;
  std::function<void(const QStringList&)> treeDelete;
  std::function<void(const QStringList&)> historyDelete;
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
  // An orientation cube arrow clicked: its NativeMacCubeArrow::id.
  std::function<void(const QString&)> cubeArrow;
  // The timeline's hover (mitcad#1076): the feature of the cell under the
  // pointer, a folded group's first feature for the group; empty when it
  // left, at a press or a drag, and at a rebuild. A cell's tooltip text,
  // asked for when AppKit shows it.
  std::function<void(const QString&)> historyHovered;
  std::function<QString(const QString&)> historyToolTip;
  // Timeline groups (mitcad#1077): a click on an open group's band folds
  // it, a double-click on its folded cell opens it (the group's name and
  // whether to fold); a right click on either opens the group menu.
  std::function<void(const QString&, bool)> historyGroupCollapsed;
  std::function<void(const QString&, const QPoint&)> historyGroupMenu;
  // A group menu's Pin to Toolbar submenu or a pinned button's menu
  // (mitcad#1001, mitcad#1074): pin (true) or unpin the command (its id),
  // the shared pins; the tab is the menu's ("SOLID"), the pin goes to the
  // command's own group (mitcad#1084).
  std::function<void(const QString&, const QString&, bool)> pinCommand;
  // Their Change Keyboard Shortcut…: Keyboard Shortcuts at the command.
  std::function<void(const QString&)> commandShortcut;
  // The Browser row under the pointer (mitcad#1072): its key, or empty when
  // the pointer left the outline or the card; called when it changes.
  std::function<void(const QString&)> treeHovered;
  // The left column (mitcad#1020): a boundary between two left cards
  // released after a drag, with the open cards' heights by name
  // ("projects", "configuration", "browser"), in points; empty after a
  // double-click returned the column to its automatic split.
  std::function<void(const std::vector<std::pair<QString, int>>&)> columnSplitChosen;
  // The Configuration card's disclosure (mitcad#1020): collapsed or not.
  std::function<void(bool)> configurationCollapsed;
  // The quick-access group's menu (mitcad#1113): a command checked or
  // unchecked, and Reset to Default; after the menu has closed.
  std::function<void(const QString&, bool)> quickAccessChosen;
  std::function<void()> quickAccessReset;
  // The message band (mitcad#1098): its × closed the message shown in full;
  // a history row about a feature was clicked (the feature's uid).
  std::function<void()> messageClosed;
  std::function<void(const QString&)> messageFeatureChosen;
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
  // The message band above the timeline (mitcad#1098, NativeMessageBand.hpp):
  // what the window's shared MessageLog shows, called again whenever it
  // changes; its times are the log's (MessageCenter). The window keeps the
  // log for the band's history; nullptr clears the band.
  void setMessages(const MessageLog* log);
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
  // The orientation cube's arrows over the canvas, under the cards; none
  // hides them. Logs "Native cube arrows" when they change.
  void setCubeArrows(const QList<NativeMacCubeArrow>& arrows);
  // The Chat card (mitcad#1075), as the Floating chatCard: on the right
  // edge from below the toolbar's rows to above the timeline, 340 points
  // wide in a wide window; the inspector and the cube's arrows move left of
  // it. Its content is the shared panel's native view (the bridge's); the
  // shell retains it while installed, nullptr clears it. Set again after
  // the view's content changed: the conversation keeps its end in sight.
  // showChat shows or hides the card, gives the message field the keyboard
  // when shown, sets the toolbar's Chat button beside the agent indicator
  // and logs "Native chat card shown: <width> x <height>" or "hidden".
  void setChatView(void* view);
  void showChat(bool shown);
  bool chatShown() const;
  // While a drawing is open (mitcad#1008, mitcad#1073): of the cards over
  // the canvas only the Browser, as the Qt layout's showOnlyCards; closed,
  // the others are shown or hidden as they were asked to be meanwhile.
  // Logs "Native cards: Browser only (a drawing is open)" and "Native
  // cards: all (the drawing closed)".
  void setDrawingOpen(bool open);
  // The left column's remembered split (columnSplitChosen); empty: the
  // automatic one. The Configuration card's remembered disclosure.
  void setColumnSplit(const std::vector<std::pair<QString, int>>& split);
  void setConfigurationCollapsed(bool collapsed);
  // The quick-access group after the workspace tabs (mitcad#1113), in the
  // same toolbar item, so that a saved toolbar shows it too: a button per
  // shown command (its artwork and state from setActions, its tooltip from
  // here) and a ⋮ that opens the group's menu. It folds into the ⋮ before
  // the document's name would be cut, as the Qt title row's. Logs "Native
  // quick access: <ids>", each button's state with its tooltip ("Native
  // quick access file.save disabled: Save (⌘S): no unsaved changes") and
  // "Native quick access folded" or "unfolded".
  void setQuickAccess(const NativeMacQuickAccess& group);
  // The navigation bar (mitcad#1099): a capsule card at the bottom centre
  // of the view, above the timeline, the status pill above it; hidden while
  // a drawing is open. Logs, when they change, "Native navigation bar shown
  // at x,y WxH" (canvas coordinates) or "hidden", "Native navigation bar
  // tooltips: <tip> | …", "Native navigation bar states: Layout Grid on
  // (View menu on), … , Visual Style <style>" (each button's check and its
  // View menu item's) and, when the Visual Style pop-up opens, "Native
  // navigation bar styles: Shaded | …".
  void setNavigationBar(const NativeMacNavigationBar& bar);

private:
  std::unique_ptr<NativeMacWindowState> m_state;
};

#endif

} // namespace mitcad::mac
