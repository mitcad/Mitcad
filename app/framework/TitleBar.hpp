// SPDX-License-Identifier: MIT
#pragma once

#include <utility>
#include <vector>

#include <QHash>
#include <QPointer>
#include <QRect>
#include <QString>
#include <QWidget>

class QAction;
class QEvent;
class QHBoxLayout;
class QLabel;
class QMenu;
class QMouseEvent;
class QPaintEvent;
class QTabBar;
class QTimer;
class QToolButton;

namespace mitcad {

class CapsuleFrame;
class QuickAccessGroup;
class Ribbon;

// The top row of the Floating chrome. On macOS (the toolbar row, in the
// height of a macOS toolbar): room for the window's traffic lights, an Undo
// and Redo pair, the ribbon's tabs (a segmented control), the quick-access
// group, the document's name and state in the middle (the native title is
// hidden there), the command search as a search field; the window's content
// extends under the native title bar (MainWindow sets the flag), so the
// empty parts of this row move the window. The Floating row of Windows and
// Linux (setFloatingRow, mitcad#1990) is the window's one top row instead,
// one flat bar: Home, File and Save, Undo and Redo, the document tabs (a +
// for a new design only with RowControls::newDesign), the quick-access
// group's other commands at the free
// stretch's right end, the Boost development button, the indicators,
// Preferences and Help, the version;
// the workspace tabs and the search field are the toolbar panel's
// (Ribbon::setPanel).
class TitleBar : public QWidget {
  Q_OBJECT

public:
  // The ribbon's tab bar is placed in the row. The actions are the registry's:
  // `search` opens the command search.
  TitleBar(Ribbon* ribbon, QAction* undo, QAction* redo, QAction* search, QWidget* parent = nullptr);
  // Lets the hosted document tabs go (hostDocumentTabs) before they would go
  // with the row: their frame takes them back.
  ~TitleBar() override;

  // The project indicator (mitcad#89), before the search field (the
  // Floating row: before Preferences and Help).
  void setIndicator(QWidget* indicator);
  // The agent indicator (mitcad#710), right of the project indicator.
  void setAgentIndicator(QWidget* indicator);
  // The chat panel's button (mitcad#947), right of the agent indicator.
  void setChatButton(QWidget* button);
  // The Boost development button (mitcad#1990, BoostButton): the Floating
  // row's right part, left of the indicators (or Preferences and Help);
  // the part "titleBoost" while it shows.
  void setBoostButton(QWidget* button);
  // The quick-access group (mitcad#1090). In the Floating row it sits at the
  // right end of the free part between the document tabs and the
  // indicators (or Preferences and Help), and folds into its menu when its
  // buttons do not fit there (mitcad#1989, mitcad#1990). In the macOS toolbar row it follows the tabs and folds
  // when the row is too narrow for it and the whole document name.
  void setQuickAccess(QuickAccessGroup* group);
  // The ☰ button of the hidden menu bar (mitcad#1620, MenuBarButton): at the
  // left of the row, before Undo and Redo, in a capsule of its own (the row
  // part "titleMenu"); shown only while setMenuButtonVisible says so.
  void setMenuButton(QWidget* button);
  void setMenuButtonVisible(bool visible);
  // The Floating row (mitcad#1990), where the window's menu bar never shows:
  // one of the menu bar's menus by its keys (MenuBarReveal's opener). File
  // drops from the row's File and Help from its ?, the others from the ☰
  // button beside its menus, each with its first entry active; null (Alt
  // alone) opens the ☰ menu. Logs "Menus by the keys (Alt+E): Edit" (or
  // "(Alt): ☰").
  void openMenuByKeys(QMenu* menu, const QString& how);

  // The height of the row.
  static int rowHeight();
  // The document's name a window title shows: without Qt's "[*]"
  // placeholder of the modified mark and the " - MitCad <version>" after it
  // ("Lid.mitcad[*] - MitCad 0.42.0" gives "Lid.mitcad"; mitcad#1209). Also
  // the title published to assistants (mitcad#1149).
  static QString documentTitleOf(QString windowTitle);

  // The Floating layout's title row on Windows and Linux (mitcad#425,
  // mitcad#1990; FloatingLayout::addTopRow, flush with the view's edges):
  // one flat bar of the window's surface with a hairline at its bottom, its
  // controls flat on it, 8 px in from its ends, a thin line after the File
  // group and after Undo and Redo. The row shows no document name
  // (mitcad#1989): the document tabs do. It takes the mouse on its whole
  // width (no mask). Named "titleRow" then. The ribbon's tabs and their +
  // leave the row for the toolbar panel (Ribbon::setPanel; they do not come
  // back), as the search field does (takeSearchField). Call it before the
  // row is shown.
  void setFloatingRow(bool floating);
  bool isFloatingRow() const { return m_floatingRow; }
  // The Floating row's own controls (mitcad#1990; after setFloatingRow):
  // Home (the Start tab), File (the menu bar's File menu) and Save in a
  // capsule ("titleFile") before Undo and Redo; after them the document
  // tabs (hostDocumentTabs) and, with `newDesign`, a + that runs it, in a
  // capsule ("titleTabs"; the window passes none: New Design is in File,
  // Ctrl+N and the Start page); at the right end Preferences and Help (the menu bar's
  // Help menu) in a capsule ("titleSettings") and the version
  // ("titleVersion").
  struct RowControls {
    QAction* home = nullptr;
    QMenu* file = nullptr;
    QAction* save = nullptr;
    QAction* newDesign = nullptr;
    QAction* preferences = nullptr;
    QMenu* help = nullptr;
  };
  void setRowControls(const RowControls& controls);
  // setRowControls' Home button (the Start tab's place, Ribbon::
  // setStartButton); null without one.
  QWidget* homeButton() const { return m_home; }
  // The document frame's tab bar (mitcad#1990) in the "titleTabs" capsule,
  // before its +. It stays the frame's: the frame moves it to the row of the
  // design shown, and this row gives it up when it goes first.
  void hostDocumentTabs(QTabBar* tabs);
  // The command search field out of the row, for the toolbar panel
  // (Ribbon::setPanel, mitcad#1990); null when it was taken already.
  QWidget* takeSearchField();
  // setFloatingRow: the controls shown, named ("titleMenu", "titleFile",
  // "titleHistory", "titleTabs", "titleQuickAccess", "titleBoost",
  // "titleIndicator", "titleAgent", "titleChat", "titleSettings",
  // "titleVersion"), in the
  // row's coordinates, left to right.
  std::vector<std::pair<QString, QRect>> rowParts() const;

signals:
  // setFloatingRow: the parts moved, resized, came or went.
  void rowPartsChanged();

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;
  void showEvent(QShowEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;
  void paintEvent(QPaintEvent* event) override;
  void mousePressEvent(QMouseEvent* event) override;
  void mouseDoubleClickEvent(QMouseEvent* event) override;

private:
  // The macOS toolbar row: where the document's name and its state go,
  // between the tabs (or the group after them) and the indicator (or the
  // search field), in the middle of the row when they fit there; `valid`
  // is false when there is no room, and always in the Floating row.
  struct TitleText {
    bool valid = false;
    QString title;    // elided
    QString subtitle; // "Edited" or empty
    QRect block;      // both lines
  };
  TitleText titleText() const;
  // The window's content reaches under the native title bar, which has the
  // traffic lights at the left.
  bool underNativeTitleBar() const;
  void watchWindow();
  void updateInset();
  // setFloatingRow: the mask of the parts (rowParts).
  void updateRowMask();
  // The quick-access group folded while it does not fit at its place
  // (updateQuickFold), unfolded again once it does (after the layout).
  void scheduleQuickFold();
  void updateQuickFold();
  // The quick-access group's place in the row's layout: after the tabs in
  // the macOS toolbar row, after the stretch in the Floating row.
  void placeQuickAccess();
  // The document's name and state at full length.
  int titleWidth() const;
  // The leftmost of the indicators and the search field (the Floating row:
  // Preferences and Help); null when there is none.
  const QWidget* rightPart() const;
  // Where the indicators go in the row's layout: before the search field
  // (the Floating row: before Preferences and Help), else at its end.
  int rightIndex() const;
  // The tabs' right end: the + after them (mitcad#1975), else the tabs. In
  // the Floating row the document tabs' capsule, else the last control at
  // the left.
  const QWidget* tabsEnd() const;
  // setFloatingRow: the ribbon's tabs and their + out of the row.
  void releaseRibbonTabs();
  // A row control that is a capsule of its own (setFloatingRow).
  void addPart(QWidget* part);
  QString documentTitle() const;

  QWidget* m_inset = nullptr;
  CapsuleFrame* m_menu = nullptr; // the ☰ button's capsule (setMenuButton)
  QPointer<QWidget> m_menuButton;  // the ☰ button itself
  QPointer<QToolButton> m_fileButton; // setRowControls' File and ?
  QPointer<QToolButton> m_helpButton;
  CapsuleFrame* m_history = nullptr;
  QWidget* m_tabs = nullptr;
  QWidget* m_tabsButton = nullptr; // the + of the tabs' menu (mitcad#1975)
  QWidget* m_search = nullptr;
  QPointer<QWidget> m_indicator;
  QPointer<QWidget> m_agentIndicator;
  QPointer<QWidget> m_chatButton;
  QPointer<QWidget> m_boost; // setBoostButton
  QPointer<QuickAccessGroup> m_quickAccess;
  // The Floating row's own controls (setRowControls, mitcad#1990).
  CapsuleFrame* m_file = nullptr;      // Home, File, Save
  QWidget* m_home = nullptr;
  CapsuleFrame* m_documents = nullptr; // the document tabs and +
  QHBoxLayout* m_documentsRow = nullptr;
  QPointer<QTabBar> m_documentTabs;    // hostDocumentTabs
  CapsuleFrame* m_settings = nullptr;  // Preferences, Help
  QLabel* m_version = nullptr;
  // The bar's own buttons of commands the quick-access group may hold
  // (file.save, file.new_design, tools.preferences): not shown twice.
  QHash<QString, QWidget*> m_rowCommands;
  std::vector<std::pair<QString, QRect>> m_toldParts; // rowParts as told
  QTimer* m_foldTimer = nullptr;
  QPointer<QWidget> m_watched;
  bool m_floatingRow = false;
};

} // namespace mitcad
