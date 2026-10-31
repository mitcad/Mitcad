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
class QMenu;
class QMouseEvent;
class QPaintEvent;
class QResizeEvent;
class QShowEvent;
class QStackedWidget;
class QTabBar;

namespace mitcad {

class CommandRegistry;

// The toolbar: workspace tabs (SOLID, BUILD, and SKETCH while sketching;
// DRAWING alone while a drawing is open, mitcad#1008),
// each with groups (CREATE, MODIFY, ...) of command buttons; the group's
// name opens a menu of all its commands, a subgroup's (CommandDef::subgroup)
// as a submenu whose pinned command is a split button (mitcad#222), with
// separators between its sections (CommandDef::section, mitcad#579).
// Each command row of those menus has a ⋮ button (also its right-click and
// the Menu key) with Pin to Toolbar and Change Keyboard Shortcut…; the
// commands pinned on a tab are the PINNED group at its end (mitcad#1001,
// ToolbarPins: the user's preference), dragged there to reorder.
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
  // SHEET for a cut sheet, mitcad#853), SOLID and BUILD hidden, so that the
  // toolbar is one row; closed, they come back with the tab the drawing
  // was opened from shown. Logs "Ribbon tabs: DRAWING (shown DRAWING)",
  // "Ribbon tabs: SOLID, BUILD (shown BUILD)".
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
  // The same full/collapsed group geometry for other native presentations.
  std::pair<int, int> groupWidths(const QString& tab, const QString& group) const;

  // The commands pinned to the toolbar (mitcad#1001), per tab: the PINNED
  // group at the end of the tab, in the pins' order. Each change is saved
  // (ToolbarPins) and logged: "Pinned to toolbar: Sweep (Solid)",
  // "Unpinned: Sweep (Solid)", "Pinned order (Solid): Loft, Sweep".
  const ToolbarPins& pins() const { return m_pins; }
  void pin(const QString& tab, const QString& id);
  void unpin(const QString& tab, const QString& id);
  void movePin(const QString& tab, const QString& id, int index);

  Presentation presentation() const { return m_presentation; }
  // The workspace tabs. In the Mac presentation the bar is not part of the
  // ribbon's layout: whoever hosts it (the title bar) places it.
  QTabBar* tabBar() const { return m_tabs; }

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
    bool pinnedGroup = false;   // the PINNED group (mitcad#1001)
  };
  struct Extras {
    QList<QAction*> menu;
    QList<QAction*> pinned;
  };

  QWidget* createPage(const QString& tab);
  QWidget* createGroup(const QString& tab, const QString& group, GroupInfo* info);
  // The PINNED group of a tab's pinned commands (mitcad#1001).
  QWidget* createPinnedGroup(const QString& tab, GroupInfo* info);
  // The common rest of a group: its caption with the menu, the sizes.
  QWidget* finishGroup(QWidget* widget, QWidget* row, QToolButton* menuButton, QMenu* menu, int pinnedCount,
                       const QString& tab, const QString& group, GroupInfo* info);
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
  // The tab's PINNED group made again from the pins (gone without pins);
  // `changed`: after a change, so the groups are fitted and logged again
  // (not while the ribbon is built).
  void updatePinnedGroup(const QString& tab, bool changed);
  // A pinned command's button pressed, dragged and dropped in its group.
  bool dragPinned(QToolButton* button, QEvent* event);
  // The tab that shows a page (SOLID 0, SKETCH 1, BUILD 2, DRAWING 3, CUT
  // SHEET 4), or -1.
  int tabOfPage(int page) const;
  // SOLID and BUILD as the first tabs (built, a drawing closed).
  void addModelTabs();
  // The places logLayout logs: the tabs, the group menus, the split buttons.
  QStringList layoutLines() const;
  // When groups do not fit, collapse pinned rows from the right. Complete
  // captions still open the same menus; Classic can scroll those captions.
  void fitGroups();
  // setGroupCards: the mask of the shown page's group cards.
  void updateGroupMask();

  CommandRegistry& m_registry;
  Presentation m_presentation;
  QTabBar* m_tabs = nullptr;
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
  QHash<QString, QWidget*> m_pinnedGroups;     // the PINNED group, by tab
  QHash<QString, QWidget*> m_pinnedSeparators; // Classic: the line before it
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
