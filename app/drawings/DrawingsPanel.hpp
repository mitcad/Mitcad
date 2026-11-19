// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QJsonArray>
#include <QPoint>
#include <QString>
#include <QWidget>

class QPushButton;
class QTreeWidget;
class QTreeWidgetItem;

namespace mitcad::drawings {

// The Drawing workspace's Drawings card (mitcad#1990, app/COMMANDS.md
// "The Drawings card"): in place of the model's Browser while a drawing
// takes the view, a list of the design's drawings and cut sheets (the
// `drawings` query's rows, a component's drawing named with its component),
// each drawing with its sheets as rows under it; the open drawing in bold,
// the sheet shown selected. A click on a drawing opens it, on a sheet
// shows that sheet (opening its drawing first); a right-click has the
// drawing's actions of the Browser's row (Open, Rename, Delete, Create
// Drawing…), Rename editing the name in the list. "+ Drawing" under the
// list runs Create Drawing. Logs, in the window's coordinates and when
// they change: "Drawings card Drawing1 at x,y", "Drawings card
// Drawing1/Sheet1 at x,y", "Drawings card add at x,y", and the rows as
// "Drawings card: Drawing1 (open, sheet 2 of 2) | Drawing2".
class DrawingsPanel : public QWidget {
  Q_OBJECT

public:
  struct Hooks {
    // Opens a drawing ("D1") over the view.
    std::function<void(const QString& drawing)> open;
    // Shows a sheet (0-based) of the drawing open.
    std::function<void(int sheet)> showSheet;
    // Create Drawing (the command file.create_drawing's run).
    std::function<void()> create;
    // A drawing's row's right-click menu at a global point (`component`:
    // the drawing's component's id, "C0" for the root's): `rename` starts
    // the edit of its name in the list.
    std::function<void(const QString& drawing, const QString& component, const QString& name, const QPoint& at,
                       const std::function<void()>& rename)>
        menu;
    // A drawing renamed in the list.
    std::function<void(const QString& drawing, const QString& name)> rename;
  };

  explicit DrawingsPanel(Hooks hooks, QWidget* parent = nullptr);

  // The design's drawings (the `drawings` query's rows), the drawing open
  // (empty: none) and its sheet shown.
  void setDrawings(const QJsonArray& drawings, const QString& open, int sheet);
  // The height its rows and its button ask for, for the card.
  int contentHeight() const;
  // Logs the places of the rows in sight and of the button, when they
  // changed.
  void logPlaces();

signals:
  void contentHeightChanged();

private:
  // A row's drawing, and its sheet (-1: the drawing's own row).
  static QString drawingOf(const QTreeWidgetItem* item);
  static int sheetOf(const QTreeWidgetItem* item);
  void chosen(QTreeWidgetItem* item);
  void contextMenu(const QPoint& at);

  Hooks m_hooks;
  QTreeWidget* m_list = nullptr;
  QPushButton* m_add = nullptr;
  QString m_open;
  int m_sheet = 0;
  bool m_building = false;
  QString m_loggedRows;
  QString m_loggedPlaces;
};

} // namespace mitcad::drawings
