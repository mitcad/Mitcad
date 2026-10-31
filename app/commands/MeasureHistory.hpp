// SPDX-License-Identifier: MIT
#pragma once

#include <vector>

#include <QHash>
#include <QString>
#include <QStringList>

#include "../framework/Selection.hpp"

namespace mitcad {

// Measure's history (mitcad#648, app/COMMANDS.md "Measure's history"): the
// measurements taken while a design is open, newest first, kept across
// closing and opening the Measure panel and not saved with the design.
// Plain data; the main window keeps it and shows it in the panel
// (MeasureHistoryView).
class MeasureHistory {
public:
  struct Row {
    // 1 for the first measurement since the design opened or the history
    // was cleared, counting on.
    int number = 0;
    // The picks' identity: the same picks are one row.
    QString key;
    // How many picks it has (one or two).
    int picks = 0;
    // What was picked: "Face of Body1 ↔ Edge of Body2".
    QString picked;
    // The main values: "Distance 40 mm", "Angle 90°".
    QStringList values;
    // The row's one line in the panel (mitcad#1010): the main value alone
    // ("40 mm") and the picks' kinds ("Face ↔ Face").
    QString main;
    QString kinds;
    // The selection inputs' picks and the choices they were made with
    // (Measure's Selection Filter), to show the measurement again.
    QHash<QString, Selection> inputs;
    QHash<QString, QString> choices;
    // Its geometry is not there any more.
    bool gone = false;
  };

  const std::vector<Row>& rows() const { return m_rows; }
  // The row with this number, or null.
  const Row* row(int number) const;
  // A Measure panel opened: what it measures first never takes the place
  // of an earlier panel's row.
  void beginRun() { m_runRow = 0; }
  // A finished measurement. The row of the same picks takes its values
  // where it is; the newest row, when this run added it and the new picks
  // start with its picks (the first pick, then the second), becomes it;
  // else it goes first with the next number. False when nothing changed.
  bool record(Row row);
  // Whether a row's geometry is gone; false when nothing changed.
  bool setGone(int number, bool gone);
  void clear();
  bool isEmpty() const { return m_rows.empty(); }

  // A row as one line of tab-separated text: its number, what was picked
  // and each value ("1\tFace of Body1 ↔ Face of Body2\tDistance 40 mm").
  static QString tabSeparated(const Row& row);
  // Every row so, newest first, one per line.
  QString tabSeparated() const;

private:
  std::vector<Row> m_rows;
  int m_next = 1;
  int m_runRow = 0; // the number of the row this run added last, 0: none
};

} // namespace mitcad
