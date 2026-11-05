// SPDX-License-Identifier: MIT
#pragma once

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
// name opens a menu of all its commands, a subgroup's (CommandDef::subgroup)
// as a submenu whose pinned command is a split button (mitcad#222), with
// separators between its sections (CommandDef::section, mitcad#579).
// Each command row of those menus has a ⋮ button (also its right-click and
// the Menu key) with Pin to Toolbar and Change Keyboard Shortcut…; a
// pinned command is a button in its own group, after the group's own
// buttons (mitcad#1001, mitcad#1084; ToolbarPins: the user's preference),
// dragged among the group's pinned buttons to reorder.
//
// Presentation::Mac is the Floating chrome's: the tabs are a segmented control
// that the title bar hosts (tabBar()), the groups are rounded capsules and the
// buttons are painted without bevels. The objects, their names and the logged
// positions are the same in both.
class Ribbon : public QWidget {
  Q_OBJECT

public:
  enum class Presentation { Classic, Mac };

  explicit Ribbon(CommandRegistry& registry, Presentation presentation = Presentation::Classic,
                  QWidget* parent = nullptr);

  // Entries that are not commands (the SELECT group's filters): `menu`
  // entries go after the group's commands, `pinned` ones are buttons.
  void addGroupExtras(const QString& tab, const QString& group, const QList<QAction*>& menu,
                      const QList<QAction*>& pinned = {});
  // Builds the tabs from the registered commands.
  void build();
  // Shows the SKETCH tab and switches to it, or hides it.
  void setSketchMode(bool sketching);
  // While a drawing is open (mitcad#1008): its DRAWING tab alone (CUT
  // SHEET for a cut sheet, mitcad#853), SOLID, BUILD and MAKE hidden, so
  // that the toolbar is one row; closed, they come back with the tab the
  // drawing was opened from shown. Logs "Ribbon tabs: DRAWING (shown
  // DRAWING)", "Ribbon tabs: START, SOLID, BUILD, MAKE (shown BUILD)".
  void setDrawingMode(bool open, bool cutSheet = false);
  // The shown page's button of a command (its own, or one that runs it:
  // a pinned command's, a group's extra), or a group's caption by
  // "DRAWING/EXPORT"; null when it is not in sight (another tab, a group
  // folded).
  QWidget* placeOf(const QString& name) const;
  // Logs where the group menus are, for UI tests.
  void logLayout() const;
  // As logLayout, only when the places changed since they were logged
  // (the ribbon or its tab bar shown, moved, resized).
  void logLayoutIfChanged() const;

  static const QStringList& groups(const QString& tab);
  // The internal name of the tab shown ("SOLID", "MAKE"), empty before
  // the tabs are built (mitcad#1049).
  QString currentTab() const;
  // Shows the tab of that internal name ("START", "SOLID"), if it is there
  // (mitcad#1258).
  void showTab(const QString& tab);
  // The same full/collapsed group geometry for other native presentations.
  std::pair<int, int> groupWidths(const QString& tab, const QString& group) const;

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

  Presentation presentation() const { return m_presentation; }
  // The workspace tabs. In the Mac presentation the bar is not part of the
  // ribbon's layout: whoever hosts it (the title bar) places it.
  QTabBar* tabBar() const { return m_tabs; }
  // The Classic presentation (Docked): a widget in the tabs' row, right after
  // the tabs (the quick-access group, mitcad#1090), so that the groups below
  // keep the toolbar's width. Not for the Mac presentation.
  void addTabRowWidget(QWidget* widget);

  // The Floating layout's group cards (mitcad#425, floatingTopCards(): Windows
  // and Linux): the ribbon floats over the 3D view, each group a card of its
  // own in the cards' material (CapsuleFrame::setCardMaterial). The ribbon
  // paints nothing else and takes the mouse only on the shown page's cards
  // (a mask), so that a click, drag or wheel between and beside them reaches
  // the view. Mac presentation only; call it after build().
  void setGroupCards(bool cards);
  bool hasGroupCards() const { return m_groupCards; }
  // The shown page's group cards in sight: their names
  // ("ribbonGroup_SOLID/SHEET_METAL": spaces as underscores) and rectangles
  // in the ribbon's coordinates, left to right (clipped to the ribbon).
  std::vector<std::pair<QString, QRect>> groupCards() const;

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
  // Another tab is shown: its internal name ("MAKE", mitcad#1049).
  void tabChanged(const QString& tab);

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
  };
  struct Extras {
    QList<QAction*> menu;
    QList<QAction*> pinned;
  };

  QWidget* createPage(const QString& tab);
  // Whether a group has commands or extras to show (mitcad#1049: a group
  // without them is left out).
  bool hasEntries(const QString& tab, const QString& group) const;
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
  // A command row's menu (the ⋮ button, a right-click, the Menu key) or a
  // pinned button's: Pin to Toolbar (Unpin from Toolbar when it is pinned on
  // the tab) and Change Keyboard Shortcut…; logged as "Toolbar menu
  // SOLID/CREATE solid.sweep: Pin to Toolbar | Change Keyboard Shortcut…".
  // From the keyboard its first entry is chosen.
  QMenu* showRowMenu(QWidget* over, const QString& tab, const QString& place, const QString& id,
                     const QPoint& global, bool keyboard);
  // A pinned command's button pressed, dragged and dropped among its
  // group's pinned buttons.
  bool dragPinned(QToolButton* button, QEvent* event);
  // The tab that shows a page (SOLID 0, SKETCH 1, BUILD 2, DRAWING 3, CUT
  // SHEET 4, MAKE 5, START 6), or -1.
  int tabOfPage(int page) const;
  // START, SOLID, BUILD and MAKE as the first tabs (built, a drawing closed).
  void addModelTabs();
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
  // fitGroups for one page's groups: `fixed` is the width of what is not
  // a collapsible group, `available` the room.
  void fitPage(QList<GroupInfo>& infos, int fixed, int available);
  // A group's width with its first `pins` pinned buttons shown.
  static int groupWidth(const GroupInfo& info, int pins);
  // setGroupCards: the mask of the shown page's group cards.
  void updateGroupMask();

  CommandRegistry& m_registry;
  Presentation m_presentation;
  QTabBar* m_tabs = nullptr;
  QHBoxLayout* m_tabRow = nullptr; // Classic: the tabs and addTabRowWidget's
  QStackedWidget* m_pages = nullptr;
  QWidget* m_sketchPage = nullptr;
  int m_pageBeforeSketch = 0; // the page Finish Sketch goes back to
  int m_pageBeforeDrawing = 0; // the page Close Drawing goes back to
  mutable QStringList m_loggedLayout; // the places logged last
  QHash<QString, Extras> m_extras;   // by "tab/group"
  QHash<QString, QWidget*> m_groupButtons; // by "tab/group", for logLayout
  // The subgroups' split buttons (mitcad#222), by "tab/group/subgroup".
  QHash<QString, QToolButton*> m_splitButtons;
  QHash<QWidget*, QList<GroupInfo>> m_groupsOfPage;
  QHash<QWidget*, QWidget*> m_classicPages;
  bool m_groupCards = false; // setGroupCards
  ToolbarPins m_pins;         // mitcad#1001
  QHash<QString, QWidget*> m_pageOfTab;        // the row of groups, by tab
  QPointer<QToolButton> m_pinDrag;             // the pinned button pressed
  QPoint m_pinDragStart;                       // where (global)
  bool m_pinDragging = false;                  // moved far enough to drag
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

protected:
  void paintEvent(QPaintEvent* event) override;
  void changeEvent(QEvent* event) override;
  void mousePressEvent(QMouseEvent* event) override;

private:
  int m_iconSize;
};

// A rounded panel in the palette's material (the group capsules, the Undo and
// Redo pair): fill, hairline border and a faint shadow below.
class CapsuleFrame : public QWidget {
  Q_OBJECT

public:
  // A radius below zero is half the height.
  explicit CapsuleFrame(int radius = 12, QWidget* parent = nullptr);

  // The Floating chrome's card material (cardFillColor, a hairline of
  // cardBorderColor), for a group that is a card of its own over the 3D view
  // (Ribbon::setGroupCards, mitcad#425).
  void setCardMaterial(bool card);

protected:
  void paintEvent(QPaintEvent* event) override;

private:
  int m_radius;
  bool m_cardMaterial = false;
};

// "CREATE" -> "Create", "FINISH SKETCH" -> "Finish Sketch": the names every
// presentation shows. The internal names stay in capitals.
QString displayName(const QString& name);

} // namespace mitcad
