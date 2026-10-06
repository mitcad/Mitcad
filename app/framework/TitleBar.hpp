// SPDX-License-Identifier: MIT
#pragma once

#include <QPointer>
#include <QString>
#include <QWidget>

class QAction;
class QEvent;
class QMouseEvent;
class QPaintEvent;
class QTabBar;

namespace mitcad {

class Ribbon;

// The top row of the Floating chrome, in the height of a macOS toolbar: room
// for the window's traffic lights, an Undo and Redo pair, the ribbon's tabs
// (a segmented control), the document's name and state in the middle, the
// command search as a search field and, while sketching, a prominent Finish
// Sketch button. The window's content extends under the native title bar
// (MainWindow sets the flag), so the empty parts of this row move the window.
class TitleBar : public QWidget {
  Q_OBJECT

public:
  // The ribbon's tab bar is placed in the row. The actions are the registry's:
  // `search` opens the command search, `finish` finishes the sketch.
  TitleBar(Ribbon* ribbon, QAction* undo, QAction* redo, QAction* search, QAction* finish,
           QWidget* parent = nullptr);

  // Shows or hides the Finish Sketch button; follows the ribbon's sketch mode.
  void setSketchMode(bool sketching);

  // The height of the row.
  static int rowHeight();

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;
  void showEvent(QShowEvent* event) override;
  void paintEvent(QPaintEvent* event) override;
  void mousePressEvent(QMouseEvent* event) override;
  void mouseDoubleClickEvent(QMouseEvent* event) override;

private:
  // The window's content reaches under the native title bar, which has the
  // traffic lights at the left.
  bool underNativeTitleBar() const;
  void watchWindow();
  void updateInset();
  QString documentTitle() const;

  QWidget* m_inset = nullptr;
  QWidget* m_tabs = nullptr;
  QWidget* m_search = nullptr;
  QWidget* m_finish = nullptr;
  QPointer<QWidget> m_watched;
};

} // namespace mitcad
