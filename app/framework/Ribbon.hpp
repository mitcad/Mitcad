// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <utility>
#include <vector>

#include <QHash>
#include <QList>
#include <QPoint>
#include <QPointer>
#include <QRect>
#include <QString>
#include <QStringList>
#include <QToolButton>
#include <QWidget>

#include "UiSizes.hpp"

#include "CommandLayout.hpp"
#include "ToolbarPins.hpp"

class QAction;
class QEvent;
class QHBoxLayout;
class QMenu;
class QMouseEvent;
class QPaintEvent;
class QResizeEvent;
class QShowEvent;
class QStackedWidget;
class QTabBar;

namespace mitcad {

class CommandRegistry;

// The toolbar: workspace tabs (START (mitcad#1258, a page without groups:
// the window shows the Start page), SOLID, BUILD, MAKE (mitcad#1049), and
// SKETCH while sketching; DRAWING alone while a drawing is open, mitcad#1008),
// each with groups (CREATE, MODIFY, ...) of command buttons; the group's
// name opens a menu of all its commands, a submenu's whose first command
// with a button is a split button (mitcad#222), with separators between its
// sections (mitcad#579). Where each command sits is the layout's
// (CommandLayout, mitcad#1951: app/resources/layout/default.json with the
// user's layer), read once; the groups are made again when it changes.
// Each command row of those menus has a ⋮ button (also its right-click and
// the Menu key) with Pin to Toolbar, Change Keyboard Shortcut…, and the
// user's layout's Move to Group…, Also Show in Group… and Remove from This
// Group (mitcad#1951), and Remove from Toolbar for a group's own button
// (also its right-click on the toolbar), Show on Toolbar for one the user
// removed (mitcad#1991; the group caption's right-click lists those too,
// with Move Group Left and Move Group Right); a
// pinned command is a button in its own group, after the group's own
// buttons (mitcad#1001, mitcad#1084; ToolbarPins: the user's preference),
// dragged among the group's pinned buttons to reorder.
// The tabs' menu (mitcad#1975), from a right-click on a tab or the + after
// the last one: the model tabs with ticks (Solid, Surface, Sheet Metal,
// Build, Make and Manage; each shown by default, Start and the modes'
// tabs never hidden; the user's layout layer keeps the hidden ones), then
// Customize Toolbar… (the tab's commands, each with a check box: a button
// on the toolbar or not) and Reset Toolbar (the tab as the default has
// it), which each command row's menu offers too.
//
// Presentation::Mac is the Floating chrome's: the tabs are a segmented control
// that the title bar hosts (tabBar()) on macOS, and on Windows and Linux the
// toolbar panel's first row (setPanel, mitcad#1990: plain names, the
// workspace's tabs alone, a workspace selector at the panel's left); the
// groups are rounded capsules (in the panel: side by side, divided by thin
// lines) and the buttons are painted without bevels. The objects, their
// names and the logged positions are the same in both.
class Ribbon : public QWidget {
  Q_OBJECT

public:
  enum class Presentation { Classic, Mac };

  explicit Ribbon(CommandRegistry& registry, Presentation presentation = Presentation::Classic,
                  QWidget* parent = nullptr);

  // Entries that are not commands (the SELECT group's filters), placed
  // where the layout has the extra's key ("solid.selection"): `menu`
  // entries in the group's menu, after a separator when it has commands,
  // `pinned` ones are buttons.
  void addGroupExtras(const QString& extra, const QList<QAction*>& menu, const QList<QAction*>& pinned = {});
  // Builds the tabs from the layout (commandLayout()) and the registered
  // commands; follows CommandLayoutStore::changed() from then on.
  void build();
  // The groups made again from the layout (a change of the user's layer):
  // the tab shown stays; logged as "Ribbon groups made again from the
  // layout".
  void rebuild();
  // Shows the SKETCH tab and switches to it, or hides it.
  void setSketchMode(bool sketching);
  // While a drawing is open (mitcad#1008): its DRAWING tab alone (CUT
  // SHEET for a cut sheet, mitcad#853), the model's tabs hidden, so that
  // the toolbar is one row; closed, they come back with the tab the
  // drawing was opened from shown. Logs "Ribbon tabs: DRAWING (shown
  // DRAWING)", "Ribbon tabs: START, SOLID, SURFACE, SHEET METAL, BUILD,
  // MAKE_PRINT, MAKE_LASER, MAKE_CNC, MANAGE (shown BUILD)".
  void setDrawingMode(bool open, bool cutSheet = false);
  // The Render workspace (the selector's, mitcad#1990, mitcad#2082): its RENDER tab alone
  // in the bar while on, as a drawing's is; off, Design's tabs come back
  // with the one shown before. Logged as "Ribbon tabs: RENDER (shown
  // RENDER)". Nothing while a drawing's tab is there.
  void setRenderMode(bool on);
  bool renderTabShown() const;
  // The shown page's button of a command (its own, or one that runs it:
  // a pinned command's, a group's extra), or a group's caption by
  // "DRAWING/EXPORT"; null when it is not in sight (another tab, a group
  // folded).
  QWidget* placeOf(const QString& name) const;
  // A command row's menu (Pin to Toolbar, Move to Group…, …) of an open
  // group menu, as the row's ⋮ opens it; false when `menu` is no open
  // group menu or has no row of `id` (UI tests' row-menu step, mitcad#1951).
  bool openRowMenu(QMenu* menu, const QString& id);
  // Logs where the group menus are, for UI tests.
  void logLayout() const;
  // As logLayout, only when the places changed since they were logged
  // (the ribbon or its tab bar shown, moved, resized).
  void logLayoutIfChanged() const;

  // A tab's groups in the application's layout, in order.
  static QStringList groups(const QString& tab);
  // The internal name of the tab shown ("SOLID", "MAKE_CNC"), empty before
  // the tabs are built (mitcad#1049).
  QString currentTab() const;
  // Shows the tab of that internal name ("START", "SOLID"), if it is there
  // (mitcad#1258).
  void showTab(const QString& tab);
  // The same full/collapsed group geometry for other native presentations.
  std::pair<int, int> groupWidths(const QString& tab, const QString& group) const;
  // A group's width with its own buttons and its first k pinned buttons,
  // k = 0 .. its pins (the last one is its full width), for the AppKit
  // ribbon's fit of the same rule (framework/RibbonFit, mitcad#1357);
  // empty for a group without buttons.
  std::vector<int> pinnedGroupWidths(const QString& tab, const QString& group) const;

  // The commands pinned to the toolbar (mitcad#1001): each a button in its
  // own group (the command's tab and group), after the group's own
  // buttons, in the pins' order (mitcad#1084). `tab` is the tab shown; the
  // pin goes to the command's own tab, the one its group is on. Each
  // change is saved (ToolbarPins) and logged: "Pinned to toolbar: Sweep
  // (Solid)", "Unpinned: Sweep (Solid)", "Pinned order SOLID/CREATE: Loft,
  // Sweep", with the group's buttons as "Ribbon group SOLID/CREATE pinned:
  // solid.loft, solid.sweep" ("... pinned: none" without them). movePin's
  // `index` is the place among the group's pinned buttons.
  const ToolbarPins& pins() const { return m_pins; }
  void pin(const QString& tab, const QString& id);
  void unpin(const QString& tab, const QString& id);
  void movePin(const QString& tab, const QString& id, int index);
  // The pins read again from the settings and every group's pinned
  // buttons made again, after the settings changed outside the ribbon
  // (Preferences › Import Settings, mitcad#1889); pinsChanged for each tab.
  void reloadPins();
  // The navigation bar's pins (mitcad#1589): with it set, a command row's
  // menu (and a pinned button's) has Pin to Navigation Bar after Pin to
  // Toolbar, Unpin from Navigation Bar when `pinned` says the command is
  // pinned there; choosing it emits navigationBarPinRequested. Unset (no
  // navigation bar), the menu has no such entry.
  void setNavigationBarPins(std::function<bool(const QString& id)> pinned);
  // The quick-access group's entry (mitcad#1990): with it on, a command
  // row's menu, a pinned button's and a group's own button's (a
  // right-click) end with Add to Quick Access, or Remove from Quick Access
  // while the group has the command (QuickAccessSettings::choose). Off (no
  // Qt quick-access group), no such entry.
  void setQuickAccessEntries(bool on) { m_quickAccessEntries = on; }

  Presentation presentation() const { return m_presentation; }
  // The workspace tabs. In the Mac presentation the bar is not part of the
  // ribbon's layout: whoever hosts it (the title bar) places it.
  QTabBar* tabBar() const { return m_tabs; }
  // The Classic presentation (Docked): a widget at the right end of the
  // tabs' row, after its free stretch, so that the groups below keep the
  // toolbar's width (mitcad#1090): the quick-access group and the command
  // search field (mitcad#2082), the Chat button; `first` puts it right
  // after the stretch, before those added already. Not for the Mac
  // presentation.
  void addTabRowEndWidget(QWidget* widget, bool first = false);

  // The Floating layout's group cards (mitcad#425, floatingTopCards(): Windows
  // and Linux): the ribbon floats over the 3D view, each group a card of its
  // own in the cards' material (CapsuleFrame::setCardMaterial). The ribbon
  // paints nothing else and takes the mouse only on the shown page's cards
  // (a mask), so that a click, drag or wheel between and beside them reaches
  // the view. Mac presentation only; call it after build().
  void setGroupCards(bool cards);
  bool hasGroupCards() const { return m_groupCards; }

  // The Floating layout's toolbar panel (mitcad#1990, Windows and Linux;
  // instead of setGroupCards): over the 3D view across the window, the
  // workspace selector (a tall block of its own at the left: the
  // workspace's icon, its name and "Workspace" with a chevron) and beside it
  // one card. The card's first row holds the workspace's tabs as plain
  // text, the shown one on a filled accent rectangle, the + of their menu,
  // and at the row's right end `search`, the command search field
  // (TitleBar::takeSearchField); under them the shown tab's groups side by
  // side, divided by thin vertical lines, each with its name and chevron
  // under it (the group's menu). A tab without groups (Start) leaves the
  // card as its tab row, whole. The groups are no cards of their own; the
  // panel takes the mouse on its whole width. They fold from the right as
  // the cards did (fitGroups). Mac presentation only; call it after
  // build().
  void setPanel(QWidget* search);
  bool isPanel() const { return m_panel; }
  // The macOS Docked layout's workspace selector (the Classic
  // presentation, mitcad#2082; Windows and Linux have the toolbar panel,
  // setPanel, in both layouts): the toolbar panel's block (card material, the
  // workspace's icon, its name and "Workspace ˅" under it) at the
  // toolbar's left, as high as the tab row and the groups' row together,
  // 8 px to them, with the same list and choices as the toolbar panel's
  // (setWorkspace, workspaceRequested); the tab row then shows the
  // workspace's tabs, and the Start tab stays first in it (there is no
  // Home button). Logs "Ribbon workspace block: left of the card (...),
  // as high as both rows: yes (...)" and the toolbar's fonts ("Ribbon
  // fonts: ..."). Call it after build().
  void setWorkspaceSelector();
  // Whether the toolbar has the workspace selector (setPanel or
  // setWorkspaceSelector).
  bool hasWorkspaceSelector() const { return m_workspaces; }
  // The macOS Docked toolbar over the Start page (setWorkspaceSelector,
  // mitcad#2082): `on`, the workspace block and the groups' row hidden and
  // the tab row alone the way back (the window has no designs' tabs then);
  // off, both back as they were. The window logs it.
  void setTabRowOnly(bool on);
  bool tabRowOnly() const { return m_tabRowOnly; }
  // setPanel: the panel's parts for the Floating layout (its log, the
  // view's free area), in the ribbon's coordinates: "ribbonWorkspace" (the
  // selector), "ribbonTabs" (the tabs and their +), "ribbonSearch", then
  // the shown page's groups as groupCards() names them
  // ("ribbonGroup_SOLID/CREATE"). Changes are told by groupCardsChanged.
  std::vector<std::pair<QString, QRect>> panelParts() const;
  // setPanel: the selector and the card, the parts the panel's row is
  // shown while one of them is (FloatingLayout::addTopRow): both hidden
  // while the Start tab is shown (the owner, 17.11.2026: the Start page has
  // no workspace), logged as "Ribbon panel hidden (Start)" and "Ribbon
  // panel shown".
  QList<QWidget*> panelRowParts() const;

  // The workspaces of the toolbar panel (setPanel, mitcad#1990): DESIGN
  // (SOLID, SURFACE, SHEET METAL, BUILD and MANAGE), MAKE (MAKE_PRINT,
  // MAKE_LASER and MAKE_CNC: manufacturing, the owner 18.11.2026), RENDER (its
  // RENDER tab, while the workspace is chosen: setRenderMode) and DRAWING
  // (a drawing's or a cut sheet's tab, while one is open). The tab row
  // shows the tabs of the workspace shown; Start is no workspace's (the
  // title row's Home shows it, the panel hidden over it; first in the
  // Docked tab row, setWorkspaceSelector) and SKETCH comes into any while
  // sketching.
  // Without a selector (the macOS Floating title bar) every tab shows, as
  // before.
  static QStringList workspaces();
  // The workspace of a tab ("SOLID": DESIGN); empty for START and SKETCH.
  static QString workspaceOfTab(const QString& tab);
  // A workspace's name, as the selector and its list show it (translated).
  static QString workspaceTitle(const QString& workspace);
  // The workspace shown: DRAWING while a drawing's tab is there, else the
  // model's (DESIGN at first).
  QString workspace() const;
  // The selector's choice (setPanel or setWorkspaceSelector): DESIGN or MAKE shows that
  // workspace's tabs and the one of them shown last (its first at first),
  // logged as "Ribbon workspace: MAKE (tabs MAKE)"; while a drawing is open
  // or Render is shown it asks the window to close the drawing or end the
  // rendered view (workspaceRequested) and the model's tabs come back with
  // the workspace's. DRAWING asks the
  // window to open a drawing, RENDER to turn the rendered view on
  // (workspaceRequested), which shows that workspace's tab. Nothing while
  // sketching (the selector is disabled then).
  void setWorkspace(const QString& workspace);
  // The selector's list, as a click on it opens it: each workspace with its
  // icon, its name ("Current" by the one shown), a line on it and its tabs;
  // one without tabs (all hidden in the tabs' menu) disabled. Logged as
  // "Ribbon workspace menu: Design (current) | Make | Render | Drawing" and
  // each entry's place, "Ribbon workspace entry MAKE at x,y".
  void openWorkspaceMenu();
  // setPanel: the model's workspace's tab shown last (the Start page left
  // by the design's tab), else the first tab there.
  void showWorkspaceTab();
  // The title row's Home (setPanel): where the Start tab is shown from, so
  // logged as the START tab's place ("Ribbon tab START at x,y").
  void setStartButton(QWidget* button);

  // Preferences › Display › Toolbar labels (mitcad#1936, framework/
  // RibbonLabels): each command button with a short label under its icon
  // (its command's CommandDef::shortLabel, else its name; one line, elided
  // past two and a half icons), in a font at Icon size; or the icons only.
  // The rows grow by the labels' height, the groups' widths count the
  // labels, and the groups are fitted again (fitGroups folds them from the
  // right as without labels). Read from the settings when the ribbon is
  // made; a change is logged as "Ribbon labels: labels (rows 62 px high,
  // 24 px for the labels)" or "Ribbon labels: icons (rows 38 px high)", then
  // each group's labels as "Ribbon group SOLID/CREATE labels: Sketch,
  // Extrude, Revolve, Hole".
  void setButtonLabels(bool labels);
  bool buttonLabels() const { return m_labels; }
  // The shown page's group cards in sight: their names
  // ("ribbonGroup_SOLID/SHEET_METAL": spaces as underscores) and rectangles
  // in the ribbon's coordinates, left to right (clipped to the ribbon).
  std::vector<std::pair<QString, QRect>> groupCards() const;

  // The tabs' menu (mitcad#1975) for `tab`, the tab right-clicked or, from
  // the + button, the one shown: the model tabs with ticks, Customize
  // Toolbar… and Reset Toolbar. Logged as "Ribbon tabs menu SOLID: Solid |
  // Surface | ... | - | Customize Toolbar… | Reset Toolbar" and, with the
  // ticks, "Ribbon tabs menu shown: SOLID, SURFACE, ...".
  void showTabsMenu(const QString& tab, const QPoint& global);
  // Customize Toolbar… of a tab: a dialog of every command of its groups
  // (in a submenu as "Pattern › Rectangular Pattern"), each with a check
  // box, ticked when the command has a button on the toolbar (its own in
  // the layout, a submenu's split button, or the user's pin); OK keeps the
  // changes in the user's layer ("Customize Toolbar SOLID: solid.sweep on,
  // solid.hole off"). Reset Toolbar: the tab's changes and pins dropped
  // ("Toolbar reset: SOLID").
  void customizeToolbar(const QString& tab);
  void resetToolbar(const QString& tab);
  // The + after the last tab, which opens the tabs' menu: in the ribbon's
  // tab row (Classic), placed by whoever hosts tabBar() (the title bar) in
  // the Mac presentation.
  QToolButton* tabsButton() const { return m_tabsButton; }
  // The tabs a user may hide, in the toolbar's order.
  static QStringList hideableTabs();
  // Widgets of the window's own on a tab's page (mitcad#2013: the Render
  // workspace's row of the stage's actions on RENDER): `leading` before the
  // tab's groups (alone with setTabRow),
  // `trailing` at the row's right end, after them. They stay the window's,
  // kept when the pages are made again; their places take the mouse as a
  // group does (groupCards: "ribbonContent_<object name>").
  void setTabContent(const QString& tab, QWidget* leading, QWidget* trailing = nullptr);
  // A tab whose tab row is the window's own (mitcad#2013: the Render
  // workspace's stages; the toolbar panel's row, or the Docked tab row with
  // its selector, mitcad#2082): while it is shown, `tabs` stands in the
  // row in place of the workspace's tabs and their +, and `trailing` at the
  // row's right (before the panel's search field; Render and Cancel). Its page holds
  // only its content (setTabContent: a slim row of the stage's actions), not
  // the tab's groups of the layout. Logged as "Ribbon tab RENDER: its own
  // row" in place of the tab's place; the panel's parts name `tabs`
  // "ribbonTabs" and `trailing` "ribbonTabRow_<object name>".
  void setTabRow(const QString& tab, QWidget* tabs, QWidget* trailing = nullptr);
  // The toolbar panel's tabs' look (plain names, the shown one on a filled
  // rectangle of the accent) for a tab row of the window's own (setTabRow).
  // With `steps` the tabs read as a workflow: a dim › between consecutive
  // tabs, neither a tab nor clickable (the Render workspace's stages).
  static QTabBar* createPanelTabBar(QWidget* parent = nullptr, bool steps = false);
  // The parts of a tab's own content (setTabContent) that look as the
  // toolbar's groups do (mitcad#2013: the Render workspace's stage
  // actions). A command button: `action`'s icon at the groups' size, its
  // label under it while the labels are shown (setButtonLabels follows it:
  // `shortLabel`, else the action's text); with `menuArrow` a ˅ beside the
  // icon, the whole button opening the action's menu.
  static QToolButton* createContentButton(QAction* action, const QString& shortLabel = QString(),
                                          bool menuArrow = false);
  // A group named `name` (its object name "ribbonContentGroup_<name>"):
  // `buttons` in a row as high as the groups' rows, `caption` with ˅ under
  // them, which opens `menu` (the group's commands). As high as a group of
  // the layout, so that the panel keeps its height.
  static QWidget* createContentGroup(const QString& name, const QString& caption, const QList<QToolButton*>& buttons,
                                     QMenu* menu);
  // A group's caption changed (a status the group shows as its name, as
  // the live view's): its text and tooltip; `widest` a text whose width it
  // keeps at least, so that the groups after it do not move as it changes.
  static void setContentGroupCaption(QWidget* group, const QString& caption, const QString& tip,
                                     const QString& widest = QString());
  // The thin line between two such groups, as between the layout's groups.
  static QWidget* createContentSeparator();

signals:
  // The SKETCH tab was shown (true) or hidden.
  void sketchModeChanged(bool sketching);
  // The DRAWING (or CUT SHEET) tab was shown (true) or hidden.
  void drawingModeChanged(bool open);
  // logLayout logged the places (they changed).
  void placesLogged();
  // setGroupCards: the shown group cards moved, resized, came or went.
  void groupCardsChanged();
  // Change Keyboard Shortcut… of a command row's menu (mitcad#1001): Tools
  // > Keyboard Shortcuts at the command.
  void shortcutRequested(const QString& id);
  // A tab's pinned commands changed (pinned, unpinned or reordered).
  void pinsChanged(const QString& tab);
  // Pin to Navigation Bar (`pin`) or Unpin from Navigation Bar of a
  // command row's menu (mitcad#1589), once the menus have closed.
  void navigationBarPinRequested(const QString& id, bool pin);
  // Another tab is shown: its internal name ("MAKE_PRINT", mitcad#1049).
  void tabChanged(const QString& tab);
  // The selector asks the window (setWorkspace, mitcad#1990): DRAWING to
  // open a drawing, RENDER to turn the rendered view on (setRenderMode),
  // DESIGN or MAKE to close the drawing open or end the rendered view.
  void workspaceRequested(const QString& workspace);

protected:
  // The SKETCH tab's colour follows the palette.
  void changeEvent(QEvent* event) override;
  void paintEvent(QPaintEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;
  // The group cards' places, for the mask (setGroupCards); a pinned
  // command's button dragged to another place in its group.
  bool eventFilter(QObject* watched, QEvent* event) override;

public:
  // Classic (Docked): a width that always fits the toolbar (it expands to
  // what is there); the height of its content.
  QSize sizeHint() const override;
  QSize minimumSizeHint() const override;

protected:
  void showEvent(QShowEvent* event) override;

private:
  // A group and the widths needed to collapse its complete pinned row.
  struct GroupInfo {
    QWidget* capsule = nullptr;
    QWidget* buttons = nullptr; // the row of pinned commands
    int fullWidth = 0;
    int collapsedWidth = 0;
    bool collapsible = false;   // it has pinned commands to hide
    QString tab;                // "SOLID"
    QString group;              // "CREATE"
    int ownButtons = 0;         // its buttons before the user's pins
    // The user's pinned buttons in the row, left to right, and their
    // widths; the row's width without them, the group's width at least
    // and the room around the row (mitcad#1111: they fold before the
    // group collapses, the last one first).
    QList<QPointer<QToolButton>> pinButtons;
    QList<int> pinWidths;
    int ownRowWidth = 0;
    int minWidth = 0;
    int rowMargins = 0;
    QStringList folded;         // the pinned commands logged as folded
    // setPanel (mitcad#1990): the group collapsed as one button, its first
    // command's icon over its caption; a click opens its menu.
    QPointer<QToolButton> collapsedButton;
    bool overflowed = false;    // setPanel: in the page's » menu
  };
  struct Extras {
    QList<QAction*> menu;
    QList<QAction*> pinned;
  };

  QWidget* createPage(const QString& tab);
  // Every tab's page, in the pages' order (kSolidPage ... kStartPage).
  void createPages();
  // m_layout from the application's layout and this window's commands.
  void resolveLayout();
  // A command's id and home for the pins (its first group in m_layout).
  ToolbarPins::Command pinCommand(const QString& id) const;
  void loadPins();
  // A tab's groups in m_layout.
  QStringList groupsOf(const QString& tab) const;
  // Whether a group has commands or extras to show (mitcad#1049: a group
  // without them is left out).
  bool hasEntries(const QString& tab, const QString& group) const;
  bool hasEntries(const std::vector<LayoutEntry>& entries) const;
  QWidget* createGroup(const QString& tab, const QString& group, GroupInfo* info);
  // The common rest of a group: its caption with the menu, the sizes.
  QWidget* finishGroup(QWidget* widget, QWidget* row, QToolButton* menuButton, QMenu* menu, int pinnedCount,
                       const QString& tab, const QString& group, GroupInfo* info);
  // A group's widths and its row's visibility for its `buttonCount`
  // buttons (its own and the user's pins); collapsedWidth is set.
  void sizeGroup(GroupInfo* info, int buttonCount);
  // The buttons of the commands the user pinned in a group (mitcad#1084),
  // added at the end of its `row`, shown at once with `show`; their count.
  int addPinnedButtons(const QString& tab, const QString& group, QWidget* row, bool show);
  // A group's pinned buttons made again from the pins, its widths fitted
  // and logged ("Ribbon group SOLID/CREATE pinned: solid.sweep").
  void updatePinnedButtons(const QString& tab, const QString& group);
  // Whether a command is a button of its own on the tab (a pinned command
  // of its group, CommandDef::pinned, or a split button's default).
  bool hasOwnButton(const QString& tab, const QString& id) const;
  // A group's menu or submenu, its command rows with their ⋮ menus; `path`
  // is "SOLID/CREATE" or "SOLID/CREATE/Pattern".
  QMenu* createGroupMenu(const QString& tab, const QString& path, QWidget* parent);
  // A command row's menu (the ⋮ button, a right-click, the Menu key), a
  // pinned button's or a group's own button's (a right-click): Pin to
  // Toolbar (Unpin from Toolbar when it is pinned on the tab) and Change
  // Keyboard Shortcut…, the toolbar's and the layout's entries, and last
  // Add to (Remove from) Quick Access (setQuickAccessEntries); logged as "Toolbar menu
  // SOLID/CREATE solid.sweep: Pin to Toolbar | Change Keyboard Shortcut…".
  // From the keyboard its first entry is chosen.
  QMenu* showRowMenu(QWidget* over, const QString& tab, const QString& place, const QString& id,
                     const QPoint& global, bool keyboard);
  // A group caption's right-click menu (mitcad#1991): Move Group Left and
  // Move Group Right (the order of the tab's groups, kept in the user's
  // layer), Show on Toolbar ▸ with the group's buttons the user removed
  // (the way back where a group menu's rows have no row menu: macOS's
  // native menus), then Customize Toolbar… and Reset Toolbar. Logged as
  // "Ribbon caption menu SOLID/MODIFY: Move Group Left | ... " and
  // "Ribbon group SOLID/MODIFY removed buttons: solid.shell" ("none"); a
  // tab's groups in order when its page is made, "Ribbon groups of SOLID:
  // MODIFY, CREATE, ...".
  void showCaptionMenu(const QString& tab, const QString& group, const QPoint& global);
  // Whether the command is a button of its own at the place, as
  // createGroup makes it: a group's command with its button, or a
  // submenu's split button (its first command with one; of a submenu of one
  // command, that command's own).
  bool isOwnButton(const QString& place, const QString& id) const;
  // A dialog of the tab's groups but `exclude` ("SOLID/CREATE"); `chosen`
  // gets the group picked when OK closes it (Move to Group…, Also Show in
  // Group…, mitcad#1951).
  void chooseGroup(const QString& title, const QString& tab, const QString& exclude,
                   std::function<void(const QString& group)> chosen);
  // A pinned command's button pressed, dragged and dropped among its
  // group's pinned buttons.
  bool dragPinned(QToolButton* button, QEvent* event);
  // The tab that shows a page (SOLID 0, SKETCH 1, BUILD 2, DRAWING 3, CUT
  // SHEET 4, MAKE 5, START 6), or -1.
  int tabOfPage(int page) const;
  // START, SOLID, BUILD and MAKE as the first tabs (built, a drawing closed),
  // but those the user hid (mitcad#1975).
  void addModelTabs();
  // The model's tabs addModelTabs adds, in their order (the user's hidden
  // ones left out).
  QStringList modelTabOrder() const;
  // The model tabs made again after the user hid or showed one: the tab
  // shown stays, or the fallback when it was hidden; logged as "Ribbon tabs:
  // START, SOLID, MAKE (shown SOLID)" when they changed. Nothing while a
  // drawing's tab is alone (its close adds them).
  void updateModelTabs();
  // The tab to show when the one wanted is not there: SOLID, else the first
  // tab (START, which is never hidden).
  int fallbackTab() const;
  // The tab at `index` shown: its page, the workspace it is in, and
  // tabChanged (the tab bar's currentChanged).
  void followTabAt(int index);
  // The tab at `index` taken out and the tab of `page` (or the fallback)
  // shown, as one change: the neighbour Qt picks while the tab goes is
  // never shown, since showing a tab can do much (the Start tab covers the
  // view with the Start page, a tab's page rearranges the docks,
  // mitcad#2077).
  void removeTabShowing(int index, int page);
  // Customize Toolbar… and Reset Toolbar of `tab` in a menu (the tabs' menu,
  // a command row's menu).
  void addToolbarEntries(QMenu* menu, const QString& tab);
  // The places logLayout logs: the tabs, the group menus, the split buttons.
  QStringList layoutLines() const;
  // When groups do not fit, collapse pinned rows from the right. Complete
  // captions still open the same menus; Classic can scroll those captions.
  // Before a group collapses, the commands the user pinned in it fold, the
  // last one first (mitcad#1111): they leave its row and stay in its
  // menu, and the group collapses only when its own buttons do not fit
  // either. Logged when a group's folded pins change: "Ribbon pinned
  // folded SOLID/CREATE: solid.sweep" ("... folded SOLID/CREATE: none").
  void fitGroups();
  // The Classic tabs' row (the macOS Docked toolbar, mitcad#2079,
  // docs/ui-conventions.md 13): the room left for the tabs after the
  // workspace block and the row's other widgets, Boost
  // development compact first when the tabs would not all fit, then the
  // tabs folding into their » (PillTabBar::setRoom). scheduleFitTabRow:
  // once, a moment later, when a widget of the row changed.
  void fitTabRow();
  void scheduleFitTabRow();
  // fitGroups for one page's groups: `fixed` is the width of what is not
  // a collapsible group, `available` the room. Returns the width the
  // groups take as they fit; `widths` gets each group's.
  int fitPage(QList<GroupInfo>& infos, int fixed, int available, std::vector<int>* widths = nullptr);
  // setPanel (mitcad#1990): the page's last groups that do not fit even
  // collapsed go into its » menu, the last one first; logged when they
  // change: "Ribbon more SOLID: INSERT, SELECT" ("... SOLID: none").
  void fitOverflow(QWidget* page, QList<GroupInfo>& infos, int total, int available, int spacing,
                   const std::vector<int>& widths);
  // The » button's menu of `page`: its groups' menus that went into it.
  void openOverflowMenu(QWidget* page);
  // A group's width with its first `pins` pinned buttons shown.
  static int groupWidth(const GroupInfo& info, int pins);
  // setGroupCards: the mask of the shown page's group cards; setPanel: the
  // panel's parts told when they moved (groupCardsChanged).
  void updateGroupMask();
  // The command rows' height: a button's, with its label's under it when
  // the labels are shown (setButtonLabels).
  int buttonRowHeight() const;
  // Every group's row at buttonRowHeight and its widths measured again, the
  // groups fitted and the places logged (setButtonLabels).
  void layoutButtonRows();
  // setPanel (mitcad#1990) or setWorkspaceSelector (mitcad#2082): the tabs
  // of the workspace shown in sight, the others (and, in the panel, START)
  // out of it; the selector shows the workspace, enabled but while
  // sketching; "Ribbon workspace: ..." logged when it changed.
  void applyWorkspaceTabs();
  // setPanel: the model tab shown belongs to another workspace (shown by
  // the code, a hidden tab's fallback): that workspace is shown.
  void followCurrentTab();
  // setPanel: the groups' row hidden for a tab without groups (Start), so
  // that the card is its tab row, whole. The tab row and the groups' row
  // keep one height in every workspace (panelPageHeight): Render's slim row
  // of a stage's actions sits on the groups' centre line, so a switch never
  // moves the cards or the view below; logged "Ribbon panel height
  // RENDER: 132" (the panel's height) when it changes.
  void updatePanelPages();
  // The command buttons of the tabs' own content (createContentButton):
  // their labels as setButtonLabels has them, their rows as high as their
  // buttons.
  void applyContentLabels();
  // The groups' row's height in the toolbar panel: the tallest group of any
  // tab's page with the row's bottom margin.
  int panelPageHeight() const;
  // setPanel: every page's groups flat, side by side under the tab row
  // (also those made again, rebuild).
  void stylePanelPages();
  // Whether a drawing's (or a cut sheet's) tab is there.
  bool drawingTabShown() const;
  // A workspace's tabs in the bar, in order (DRAWING: the drawing's tab).
  QStringList tabsOfWorkspace(const QString& workspace) const;
  // setPanel or setWorkspaceSelector: the tab rows of the window's own
  // (setTabRow) put into the panel's tab row (the Classic tab row after the
  // tabs' +, their trailing part after its free stretch), hidden; and shown
  // in place of the tabs and their + while their tab is the one shown.
  void placeTabRows();
  void updateTabRow();
  // Whether a widget of setTabRow is in the tab row already.
  bool tabRowHolds(QWidget* widget) const;
  // The model's workspace of the tab shown, at the selector's start.
  void initWorkspace();

  CommandRegistry& m_registry;
  Presentation m_presentation;
  QTabBar* m_tabs = nullptr;
  bool m_fitTabRowPending = false;
  QHBoxLayout* m_tabRow = nullptr; // Classic: the tabs and addTabRowEndWidget's
  QStackedWidget* m_pages = nullptr;
  QWidget* m_sketchPage = nullptr;
  int m_pageBeforeSketch = 0; // the page Finish Sketch goes back to
  int m_pageBeforeDrawing = 0; // the page Close Drawing goes back to
  mutable QStringList m_loggedLayout; // the places logged last
  QHash<QString, Extras> m_extras;   // by the extra's key
  CommandLayout m_layout;            // resolved against m_registry
  QHash<QString, QWidget*> m_groupButtons; // by "tab/group", for logLayout
  // The subgroups' split buttons (mitcad#222), by "tab/group/subgroup".
  QHash<QString, QToolButton*> m_splitButtons;
  QHash<QWidget*, QList<GroupInfo>> m_groupsOfPage;
  QHash<QWidget*, QWidget*> m_classicPages;
  bool m_groupCards = false; // setGroupCards
  bool m_panel = false;      // setPanel (mitcad#1990)
  bool m_workspaces = false; // a workspace selector (setPanel, setWorkspaceSelector)
  bool m_tabRowOnly = false; // setTabRowOnly: the Docked tab row alone (Start)
  QWidget* m_panelTabRow = nullptr;          // the tabs, their + and the search field
  QWidget* m_panelBody = nullptr;            // the card: the tab row and the groups
  QToolButton* m_workspaceButton = nullptr;  // the selector (mitcad#1990)
  QString m_workspace;                       // DESIGN or MAKE: the model's workspace
  QHash<QString, int> m_workspacePage;       // the page shown last, by workspace
  QString m_loggedWorkspace;                 // as "Ribbon workspace: ..." told it last
  QString m_loggedPanelHeight;               // as "Ribbon panel height ..." told it last
  bool m_applyingWorkspace = false;          // tabs shown or hidden: no following
  QPointer<QWidget> m_startButton;           // the title row's Home
  QPointer<QWidget> m_search;                // the search field in the panel
  std::vector<std::pair<QString, QRect>> m_panelParts; // as last told
  ToolbarPins m_pins;         // mitcad#1001
  std::function<bool(const QString&)> m_navigationBarPinned; // mitcad#1589
  bool m_quickAccessEntries = false;                         // setQuickAccessEntries
  QHash<QString, QWidget*> m_pageOfTab;        // the row of groups, by tab
  QPointer<QToolButton> m_pinDrag;             // the pinned button pressed
  QPoint m_pinDragStart;                       // where (global)
  bool m_pinDragging = false;                  // moved far enough to drag
  bool m_labels = false;                       // setButtonLabels (mitcad#1936)
  QToolButton* m_tabsButton = nullptr;         // the + of the tabs' menu (mitcad#1975)
  QHash<QString, QStringList> m_loggedCollapsed; // by tab: the groups logged as collapsed
  QHash<QWidget*, QToolButton*> m_overflowButtons; // setPanel: the » of each page
  QHash<QString, QStringList> m_loggedOverflow;    // by tab: the groups logged in the » menu
  // setTabContent (mitcad#2013): by tab, its leading and trailing widgets;
  // by page, what holds them there (themselves, or a capsule around them).
  QHash<QString, QList<QPointer<QWidget>>> m_tabContent;
  QHash<QWidget*, QList<QWidget*>> m_contentOfPage;
  // setTabRow (mitcad#2013): by tab, its own tabs and trailing widgets.
  QHash<QString, QList<QPointer<QWidget>>> m_tabRows;
};

// A command button of the Mac presentation (also the title bar's Undo and
// Redo): no bevel, a rounded highlight on hover, a darker one pressed, the
// accent tint when checked. The default action gives icon, tooltip and state.
// With QToolButton::MenuButtonPopup it is a split button (mitcad#222): a
// chevron part at the right opens its menu.
class RibbonButton : public QToolButton {
  Q_OBJECT

public:
  explicit RibbonButton(QAction* action, int iconSize, QWidget* parent = nullptr);

  QSize sizeHint() const override;
  // A short label under the icon (Ribbon::setButtonLabels, mitcad#1936).
  void setLabelShown(bool shown);
  // A ˅ beside the icon for a button whose click opens its menu
  // (QToolButton::InstantPopup; Ribbon::createContentButton).
  void setMenuArrow(bool arrow);

protected:
  void paintEvent(QPaintEvent* event) override;
  void changeEvent(QEvent* event) override;
  void mousePressEvent(QMouseEvent* event) override;

private:
  // The arrow part's width: a split button's or a menu button's.
  int arrowWidth() const;

  int m_iconSize;
  bool m_labelShown = false;
  bool m_menuArrow = false;
};

// A rounded panel in the palette's material (the group capsules, the Undo and
// Redo pair): fill, hairline border and a faint shadow below.
class CapsuleFrame : public QWidget {
  Q_OBJECT

public:
  // A radius below zero is half the height; any radius at most half of it.
  // Every card's and bar's by default (kCardRadius, mitcad#1990).
  explicit CapsuleFrame(int radius = kCardRadius, QWidget* parent = nullptr);

  // The Floating chrome's card material (cardFillColor, a hairline of
  // cardBorderColor), for a group that is a card of its own over the 3D view
  // (Ribbon::setGroupCards, mitcad#425).
  void setCardMaterial(bool card);
  // Nothing painted: a group of the toolbar panel, which is the card
  // (Ribbon::setPanel, mitcad#1990), or the quick-access group on the Docked
  // tab row (mitcad#2082).
  void setFlat(bool flat);
  bool isFlat() const { return m_flat; }

protected:
  void paintEvent(QPaintEvent* event) override;

private:
  int m_radius;
  bool m_cardMaterial = false;
  bool m_flat = false;
};

// "CREATE" -> "Create", "FINISH SKETCH" -> "Finish Sketch": the names every
// presentation shows; a tab keyed apart from its caption by its own
// ("MAKE_PRINT" -> "3D Print"). The internal names stay in capitals.
QString displayName(const QString& key);
// Whether a tab's, group's or submenu's key in the layout ("SHEET METAL",
// "Machine Elements") has a caption to translate (mitcad#1951: the layout
// file holds keys only).
bool hasRibbonCaption(const QString& key);

} // namespace mitcad
