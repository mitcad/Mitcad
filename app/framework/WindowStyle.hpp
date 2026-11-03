// SPDX-License-Identifier: MIT
#pragma once

#include <QColor>
#include <QFont>
#include <QGuiApplication>
#include <QList>
#include <QPalette>
#include <QPointer>
#include <QString>
#include <QStyleOptionViewItem>
#include <QStyledItemDelegate>
#include <QWidget>

#include "TreeItemDelegate.hpp"

class QAbstractButton;
class QDockWidget;
class QEnterEvent;
class QEvent;
class QMainWindow;
class QModelIndex;
class QPainter;
class QPaintEvent;
class QToolBar;
class QTreeView;

namespace mitcad {

// Scrollable side trees need one complete row, rather than the scroll
// area's several-row default. Stacked docks must still fit a short screen.
QSize dockedTreeMinimumSize(const QTreeView* tree, const QSize& fallback);

// The look of the Docked chrome's window (mitcad#251), the same on Windows,
// Linux and macOS and in the light and the dark theme: the panels (Projects,
// Browser, Command, Timeline) and the toolbar are raised surfaces, slightly
// lighter than the window, with rounded corners and a gap between them
// instead of separator lines; the panels have small grey upper-case titles
// whose float and close buttons show while the mouse is over them; one
// accent colour, the icons' action blue (actionColor), marks the selection,
// the focus and the timeline's marker; tree rows are about 28 px high, a
// selected one a rounded band. On Windows 11 the title bar takes the menu
// bar's colour, with Mica and rounded corners (platform/WindowsFrame.hpp),
// and the font is Segoe UI Variable. The Floating chrome does not use it.

// The geometry of the panels.
namespace panel {
constexpr int kGap = 6;          // between the panels, and to the window's edges
constexpr int kRadius = 8;       // a panel's corners
constexpr int kTitleHeight = 32; // a panel's title row
constexpr int kStripWidth = 26;  // a panel's title as a strip at its left (the timeline)
constexpr int kRowHeight = 28;   // a tree's rows
constexpr int kRowRadius = 6;    // a selected row's band
constexpr int kRowInset = 2;     // the band's room to the tree's sides
} // namespace panel

// The colours of the window, from the application palette's theme (light or
// dark, isDarkPalette); the platform's palette decides, so they follow the
// theme when it changes while Mitcad runs.
struct WindowColors {
  QColor window;        // behind the panels: the gaps, the menu bar, Windows 11's title bar
  QColor surface;       // the panels and the toolbar
  QColor raised;        // a hovered button on a surface
  QColor text;          // text on the surfaces
  QColor secondaryText; // quieter text: unselected tabs, the panels' buttons
  QColor titleText;     // the panels' and sections' titles
  QColor line;          // hairlines on a surface (the ribbon's groups)
  QColor accent;        // actionColor: the selection, the focus, the timeline's marker
  QColor accentText;    // text on the accent colour
  bool dark = false;
};
WindowColors windowColors(const QPalette& palette = QGuiApplication::palette());

// The font of the panels' and sections' titles: the widget's, smaller,
// semibold and spaced out, preserving the title's spelling.
QFont panelTitleFont(const QFont& font);

// A selected row of a tree in a panel: a rounded band of the accent colour
// behind the whole row (paint it before the row, for the first column), and
// the option to paint the row with, without the style's own selection,
// focus frame and hover.
void paintPanelRowBand(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index);
QStyleOptionViewItem panelRowOption(const QStyleOptionViewItem& option);

// A tree in a panel: no frame, the panel's surface behind its rows (its base
// transparent), no selection marks of the style's beside the rows' bands
// (its Highlight and Accent transparent; no style sheet, so that the rest of
// its palette follows the theme), the bands repainted whole when the
// selection changes. Its delegate paints the bands (PanelRowDelegate, or one
// that calls the functions above).
void stylePanelTree(QTreeView* tree);

// A plain tree's rows in a panel: the band and at least kRowHeight; an
// editor in a row (a rename) with a background of its own.
class PanelRowDelegate : public TreeItemDelegate {
  Q_OBJECT

public:
  using TreeItemDelegate::TreeItemDelegate;

  void paint(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const override;
  QSize sizeHint(const QStyleOptionViewItem& option, const QModelIndex& index) const override;
};

// A section's title in a panel (the Command panel's Autoselect): small grey
// text instead of a group box's frame, preserving its spelling.
class SectionTitle : public QWidget {
  Q_OBJECT

public:
  explicit SectionTitle(const QString& text, QWidget* parent = nullptr);

  QString text() const { return m_text; }
  QSize sizeHint() const override;
  QSize minimumSizeHint() const override;

protected:
  void changeEvent(QEvent* event) override;
  void paintEvent(QPaintEvent* event) override;

private:
  QString m_text;
};

// A panel's title row: the dock's title in small grey text and its float
// and close buttons, which show while the mouse is over the row. A dock
// with a vertical title bar (the timeline) gets a strip at its left with the
// buttons and no text. Clicks on the row go to the dock (QDockWidget moves,
// floats and docks it).
class PanelTitleBar : public QWidget {
  Q_OBJECT

public:
  explicit PanelTitleBar(QDockWidget* dock);

  // A button of the panel's own before the float and close buttons, always
  // shown (the browser's magnifier, mitcad#350); the title bar takes it over.
  void addButton(QAbstractButton* button);

  QSize sizeHint() const override;
  QSize minimumSizeHint() const override;

protected:
  void changeEvent(QEvent* event) override;
  void enterEvent(QEnterEvent* event) override;
  void leaveEvent(QEvent* event) override;
  void paintEvent(QPaintEvent* event) override;

private:
  bool vertical() const;
  void updateButtons();

  QDockWidget* m_dock;
  QAbstractButton* m_float = nullptr;
  QAbstractButton* m_close = nullptr;
  bool m_hovered = false;
  int m_extraWidth = 0; // the buttons added by addButton
};

// Applies the look to a Docked main window and keeps it in step with the
// theme. Call it once, when the window's menus, toolbar and docks exist
// (MainWindow::createPanels); it is the window's child.
class WindowStyle : public QObject {
  Q_OBJECT

public:
  static void install(QMainWindow* window);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  explicit WindowStyle(QMainWindow* window);

  void styleDock(QDockWidget* dock);
  void styleToolBar(QToolBar* bar);
  // Sets the palettes and style sheets of the current theme.
  void apply();
  // The Windows 11 frame (platform/WindowsFrame.hpp), once the window has its
  // native handle.
  void applyFrame();
  // The top-level window that shows the window when it is not one itself
  // (a document frame, mitcad#1172): watched, and in the window's colour.
  void watchHost();
  void styleHost();

  QPointer<QMainWindow> m_window;
  QPointer<QWidget> m_host;
  QPointer<QToolBar> m_toolBar;
  QList<QPointer<QDockWidget>> m_docks;
  QString m_font;          // the family set, or empty
  QString m_applied;       // the colours applied, as logged
  bool m_shown = false;
};

} // namespace mitcad
