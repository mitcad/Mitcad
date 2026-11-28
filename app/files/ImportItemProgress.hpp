// SPDX-License-Identifier: MIT
#pragma once

#include <QString>

namespace mitcad {

// The import worker's timeline progress as the progress dialog shows it
// (mitcad#2331), plain Qt Core (app.unit): read from the worker's lines
// ("import-items <n>", "import: [<s>] item <position>: <name>",
// "import: [<s>] item component: <name>", "import: [<s>] item document:
// <name>", "import: [<s>] <item>: <outcome> in <s> s").
//
// The number shown is the items done so far plus one, against the whole
// count the import replays: the design's items and those of each document
// it refers to that it imports, once each (mitcad#2635). It never goes
// back, as the bar does not. (The worker's own item position is the
// position inside a component's timeline, and the components are replayed
// in dependency order, so it starts again for each component; each
// document's own timeline starts again too.)
class ImportItemProgress {
public:
  enum class Line { Other, Items, Start, Component, Document, Done };

  // Reads one line of the worker; what it was.
  Line read(const QString& line);
  // An .iam assembly's next part file: its own timeline follows.
  void startPart();

  int items() const { return m_items; }
  int done() const { return m_done; }
  // The worker's position of the current item in its own timeline (from 1),
  // 0 before the first.
  int position() const { return m_position; }
  const QString& current() const { return m_current; }
  // The current item's component, when the design has several and the item
  // is not in its root component; empty otherwise.
  const QString& component() const { return m_component; }
  // The design the current item is in, when it is a document the design
  // opened refers to (mitcad#2635); empty for the design's own items.
  const QString& design() const { return m_design; }
  // The item done last with its outcome: "Extrude2 parametric".
  const QString& lastDone() const { return m_lastDone; }
  // The number shown: done + 1, at most the count, at least 1.
  int shown() const;
  // "Timeline item 37 of 120: Fillet3, component Lid", with ", design
  // Gear" for an item of a document the design refers to; empty without a
  // count.
  QString text() const;
  // An .iam part's item: "Component 2 of 5: Base plate.ipt, item 4 of its
  // timeline: Extrusion1", or without the item before its timeline starts.
  QString partText(int part, int parts, const QString& file) const;

private:
  int m_items = 0;
  int m_done = 0;
  int m_position = 0;
  QString m_current;
  QString m_component;
  QString m_design;
  QString m_lastDone;
};

} // namespace mitcad
