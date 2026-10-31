// SPDX-License-Identifier: MIT
#pragma once

#include <vector>

#include <QFrame>
#include <QString>
#include <QStringList>

#include "Command.hpp"

class QLabel;
class QToolButton;
class QVBoxLayout;

namespace mitcad {

class ValueLine;

// An inspection's answer as rows of a name and a value (Inspection::values
// and groups; Measure, mitcad#1010; app/COMMANDS.md "Measure's panel"):
// the pair's values first, the main one in a larger type, then a group for
// each pick under a header that folds it ("Selection 1: cylindrical
// face"). Each row is one line, its name in its group's column and its
// value beside it, never wrapped or cut; where the panel is too narrow for
// both (coordinates far from the origin) the value goes on a line of its
// own under its name. A right click on a row offers Copy, which puts its
// value (number and unit) on the clipboard.
//
// Logged for the UI tests: the rows as they are set ("Panel <command>
// result values: Distance = 40 mm, Angle = 0°", "Panel <command> result
// Selection 1: cylindrical face: Diameter = 10 mm, …"); with the panel's
// places "Panel <command> result <name> at x,y" (a group's rows "Panel
// <command> result <n> <name> at x,y", "… out of sight" while folded; its
// header "Panel <command> result group <n> at x,y"), and whether every row
// fits its line: "Panel <command> result: <n> rows, one line each", or
// "…, under their names: <name>, …" and "…, cut: <name> (<needed> of
// <width>)". A click on a header logs "Panel <command> result group <n>
// folded|unfolded", Copy "Panel <command> result copied <name>: <value>".
class ResultRows : public QFrame {
public:
  explicit ResultRows(const QString& command, QWidget* parent = nullptr);
  // The rows of an answer; none empties it and hides it. A group folded
  // stays folded for the next answers.
  void set(const Inspection& result);
  bool isEmpty() const { return m_rows.empty(); }
  QStringList places() const;
  // The Floating layout's look (a rounded, lightly tinted box) or the
  // Docked one's (a framed panel).
  void setFloatingStyle(bool floating);

private:
  struct Row {
    QString key; // "Distance", or "1 Radius" in the first pick's group
    QString value;
    QLabel* name = nullptr;
    QLabel* text = nullptr;
    ValueLine* line = nullptr;
    int group = -1; // the group it is in, -1: the pair's values
  };

  // The rows of `values` in `column`, their names in one column.
  void addRows(QVBoxLayout* column, const std::vector<Inspection::Value>& values, int group);
  void menu(const QString& key, const QString& value, const QPoint& global);
  void fold(int group, bool folded);

  QString m_command;
  std::vector<Row> m_rows;
  std::vector<QToolButton*> m_headers;
  std::vector<QWidget*> m_bodies;
  std::vector<bool> m_folded; // by group, kept across answers
  QWidget* m_content = nullptr; // made again for each answer
};

} // namespace mitcad
