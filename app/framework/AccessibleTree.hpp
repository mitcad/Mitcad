// SPDX-License-Identifier: MIT
#pragma once

// The accessibility tree as a client reads it (mitcad#2227): a walk from a
// window's QAccessibleInterface through child() down to every node, with
// the properties a screen reader or a UI Automation bulk read asks of each
// (role, name, description, place, the path id Qt's UI Automation bridge
// builds, the index in its parent), and how long it took. Children whose
// state is invisible are skipped with what is below them, as UI
// Automation's navigation skips them. What the platform bridges (UI
// Automation, AT-SPI, NSAccessibility) serve is this tree; a huge one froze
// the window while another process read it.
//
// The TestDriver step `accessible-tree` logs it; nothing else walks it.

#include <QString>
#include <QStringList>
#include <QtGlobal>

class QWidget;

namespace mitcad {

struct AccessibleTreeStats {
  int nodes = 0;          // the nodes reached through child(), the window's own included
  int invisible = 0;      // children skipped because their state says invisible
  int hiddenNodes = 0;    // with walkHidden: the nodes below and with them (walked apart)
  int widest = 0;         // the most children of one node
  QString widestNode;     // that node
  int maxDepth = 0;       // the deepest node's depth (the window's children are at 1)
  bool truncated = false; // stopped at the walk's limit
  qint64 ms = 0;          // the walk's time
  // The big subtrees, outermost first, as "<indent><node>, K children:
  // N nodes": every child of the window, and below them each node with at
  // least `detailShare` of all nodes and at least `detailNodes`, so that
  // the lines show where the nodes are; a hidden one starts "(hidden)".
  QStringList subtrees;
};

// Walks `window`'s tree (nullptr: an empty result); `walkHidden` also
// walks and lists what is below the invisible children, apart from the
// tree's own nodes and time (what showing them would add). A subtree below the
// window's children gets a line of its own with at least `detailShare` of
// all nodes (0..1) and at least `detailNodes` nodes.
AccessibleTreeStats walkAccessibleTree(QWidget* window, bool walkHidden = false, double detailShare = 0.02,
                                       int detailNodes = 20);

} // namespace mitcad
