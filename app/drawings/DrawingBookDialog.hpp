// SPDX-License-Identifier: MIT
#pragma once

// File › Drawing Book… (mitcad#2448, app/COMMANDS.md "Drawing book"): the
// drawings of the design and of the designs it links, grouped by design,
// to check into the book and put in its order (Move Up and Move Down move
// a drawing within its design, or a design with its drawings), with the
// book's title and its Cover page (off by default). A chosen drawing that
// is gone or whose design cannot be read is marked with the reason in the
// dialog's message line, and Print… waits until it is unchecked or its
// design is corrected. Save keeps the book with the design (one undo
// step); Print… keeps it and prints it.
//
// Logged for the UI tests: "Drawing Book opened: <design> [<drawing>, …] |
// …" and, whenever the choice changes, "Drawing Book: <design>/<drawing> |
// …; cover on|off; print enabled|disabled[: <problems>]".

#include <QDialog>
#include <QJsonObject>
#include <QList>
#include <QString>
#include <QStringList>

class QCheckBox;
class QLabel;
class QLineEdit;
class QPushButton;
class QTreeWidget;
class QTreeWidgetItem;

namespace mitcad::drawings {

class DrawingBookDialog : public QDialog {
  Q_OBJECT
public:
  // `book`: the model's `drawing_book` answer; `designTitle`: the title an
  // untitled book shows and prints (the design's name).
  DrawingBookDialog(const QJsonObject& book, const QString& designTitle, QWidget* parent = nullptr);

  // The `set_drawing_book` command of what the dialog shows.
  QJsonObject command() const;
  // Whether Print… closed it (else Save).
  bool printAsked() const { return m_printAsked; }
  // The chosen drawings in the book's order, as the log writes them
  // ("a.mitcad/Drawing1", "This design/Assembly").
  QStringList chosenNames() const;
  // Why chosen drawings cannot be printed, in words; empty when the book
  // prints.
  QStringList problems() const;

private:
  void build(const QJsonObject& book);
  QTreeWidgetItem* addGroup(const QString& label, const QString& tooltip, const QString& problem);
  QTreeWidgetItem* addDrawing(QTreeWidgetItem* group, const QJsonObject& drawing, bool own, const QString& source,
                              bool chosen, const QString& problem);
  QList<QTreeWidgetItem*> chosenItems() const;
  void moveCurrent(int step);
  void refresh();

  QLineEdit* m_title = nullptr;
  QTreeWidget* m_tree = nullptr;
  QPushButton* m_up = nullptr;
  QPushButton* m_down = nullptr;
  QCheckBox* m_cover = nullptr;
  QLabel* m_message = nullptr;
  QPushButton* m_print = nullptr;
  QPushButton* m_save = nullptr;
  bool m_printAsked = false;
  bool m_building = false;
  QString m_logged;
};

} // namespace mitcad::drawings
