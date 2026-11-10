// SPDX-License-Identifier: MIT
#pragma once

#include <utility>
#include <vector>

#include <QPointer>
#include <QRect>
#include <QString>
#include <QWidget>

class QAction;
class QEvent;
class QMouseEvent;
class QPaintEvent;
class QTabBar;
class QTimer;

namespace mitcad {

class CapsuleFrame;
class QuickAccessGroup;
class Ribbon;

// The top row of the Floating chrome, in the height of a macOS toolbar: room
// for the window's traffic lights, an Undo and Redo pair, the ribbon's tabs
// (a segmented control), the document's name and state in the middle, the
// command search as a search field. The window's content extends under the
// native title bar (MainWindow sets the flag), so the empty parts of this
// row move the window.
class TitleBar : public QWidget {
  Q_OBJECT

public:
  // The ribbon's tab bar is placed in the row. The actions are the registry's:
  // `search` opens the command search.
  TitleBar(Ribbon* ribbon, QAction* undo, QAction* redo, QAction* search, QWidget* parent = nullptr);

  // The project indicator (mitcad#89), beside the document's name: right of
  // it, before the search field.
  void setIndicator(QWidget* indicator);
  // The agent indicator (mitcad#710), right of the project indicator.
  void setAgentIndicator(QWidget* indicator);
  // The chat panel's button (mitcad#947), right of the agent indicator.
  void setChatButton(QWidget* button);
  // The quick-access group (mitcad#1090), right of the tabs. It folds into
  // its menu when the row is too narrow for it and the whole document name.
  void setQuickAccess(QuickAccessGroup* group);
  // The ☰ button of the hidden menu bar (mitcad#1620, MenuBarButton): at the
  // left of the row, before Undo and Redo, in a capsule of its own (the row
  // part "titleMenu"); shown only while setMenuButtonVisible says so.
  void setMenuButton(QWidget* button);
  void setMenuButtonVisible(bool visible);

  // The height of the row.
  static int rowHeight();
  // The document's name a window title shows: without Qt's "[*]"
  // placeholder of the modified mark and the " - MitCad <version>" after it
  // ("Lid.mitcad[*] - MitCad 0.42.0" gives "Lid.mitcad"; mitcad#1209). Also
  // the title published to assistants (mitcad#1149).
  static QString documentTitleOf(QString windowTitle);

  // The Floating layout's title row on Windows and Linux (mitcad#425,
  // mitcad#429; FloatingLayout::addTopRow): no card behind the row and no
  // margins at its ends. Its controls (the Undo and Redo pair, the tabs, the
  // project and agent indicators, the search field) are capsules of
  // their own over the 3D view, on the cards' material (legible over the
  // view in both themes); the document's name is on a small box of the
  // cards' material, as wide as the text (titleBox). The row takes the
  // mouse only on them (a mask): a click, drag or wheel elsewhere reaches
  // the view. Named "titleRow" then. Call it before the row is shown.
  void setFloatingRow(bool floating);
  bool isFloatingRow() const { return m_floatingRow; }
  // setFloatingRow: the controls shown and the title's box, named
  // ("titleMenu", "titleHistory", "titleTabs", "titleQuickAccess", "titleBox",
  // "titleIndicator", "titleAgent", "titleChat", "titleSearch"), in the
  // row's coordinates, left to right.
  std::vector<std::pair<QString, QRect>> rowParts() const;
  // The box behind the document's name in the row's coordinates (empty
  // while there is no room for the name).
  QRect titleBox() const;

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
  // Where the document's name and its state go: between the tabs and the
  // indicator (or the search field), in the middle of the row when they fit
  // there; `valid` is false when there is no room.
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
  // The quick-access group folded while the document's whole name would
  // not fit beside it, unfolded again once it does (after the layout).
  void scheduleQuickFold();
  void updateQuickFold();
  // The document's name and state at full length, with the box's padding.
  int titleWidth() const;
  // The leftmost of the indicators and the search field.
  const QWidget* rightPart() const;
  QString documentTitle() const;

  QWidget* m_inset = nullptr;
  CapsuleFrame* m_menu = nullptr; // the ☰ button's capsule (setMenuButton)
  CapsuleFrame* m_history = nullptr;
  QWidget* m_tabs = nullptr;
  QWidget* m_search = nullptr;
  QPointer<QWidget> m_indicator;
  QPointer<QWidget> m_agentIndicator;
  QPointer<QWidget> m_chatButton;
  QPointer<QuickAccessGroup> m_quickAccess;
  QTimer* m_foldTimer = nullptr;
  QPointer<QWidget> m_watched;
  bool m_floatingRow = false;
};

} // namespace mitcad
