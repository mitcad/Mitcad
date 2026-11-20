// SPDX-License-Identifier: MIT
#pragma once

// A selection input's picks listed by name under its chip on demand
// (mitcad#2064, GitHub mitcad/Mitcad#473), apart from CommandPanel.cpp so
// that the unit tests link them alone:
//   - one open or closed state for every list of picks of every panel,
//     remembered in the settings (`commandPanel/picksShown`, closed by
//     default): opening the list in one panel opens it in every panel;
//   - the picks in words, each pick's owner asked once (pickWords);
//   - the chip's tooltip that lists them (inputPicksTooltip);
//   - the row of a list under the pointer (PickHoverWatch), whose pick the
//     view highlights meanwhile.

#include <functional>

#include <QObject>
#include <QString>
#include <QStringList>

#include "Selection.hpp"

class QEvent;
class QListWidget;

namespace mitcad {

// Whether the lists of picks are open (closed by default).
bool inputPicksShown();
// Opens or closes them, remembers it and tells every panel
// (InputPicksState::shownChanged); nothing when it is so already.
void setInputPicksShown(bool shown);

// Tells the panels that the lists were opened or closed.
class InputPicksState : public QObject {
  Q_OBJECT

public:
  static InputPicksState& instance();

signals:
  void shownChanged(bool shown);
};

// The picks in words, one for each pick in order, through `text`
// (CommandContext::itemText): picks of one kind on one owner (a Fillet's
// edges of one body) have the same words, so `text` is asked once for
// them; construction planes, axes and points are asked one by one (a
// mount frame's words come from its name).
QStringList pickWords(const Selection& items, const std::function<QString(const SelectionItem&)>& text);

// How many picks the chip's tooltip names before "and N more".
constexpr int kInputPicksTipItems = 10;

// The chip's tooltip: the picks in words, one a line (the first
// kInputPicksTipItems, then "and N more"), above `hint`; `hint` alone
// without words.
QString inputPicksTooltip(const QStringList& words, const QString& hint);

// The row of a list of picks under the pointer, watched on the list's
// viewport: each move over another row calls `hovered` with its index,
// leaving the list (or its rows) with -1. Read from the moves themselves,
// as RowHoverWatch does, so that posted moves (the UI tests' driver, a
// session without a desktop) count as the pointer's do.
class PickHoverWatch : public QObject {
public:
  PickHoverWatch(QListWidget* list, std::function<void(int)> hovered);
  // The row hovered now; -1: none.
  int row() const { return m_row; }
  // No row hovered any more (the list hidden or filled again): calls
  // `hovered` with -1 when one was.
  void reset() { update(-1); }

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  void update(int row);

  QListWidget* m_list;
  std::function<void(int)> m_hovered;
  int m_row = -1;
};

} // namespace mitcad
