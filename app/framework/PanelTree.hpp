// SPDX-License-Identifier: MIT
#pragma once

#include <QTreeView>

namespace mitcad {
namespace panel {

// The indentation per level of the side panels' trees (the Browser and the
// Projects panel), which is also the width the style centres a branch's
// disclosure arrow in. The styles' own (20 to 30 pixels: Qt's cross-platform
// style, Windows 11) waste a narrow panel's width and leave the arrows far
// from the names. One definition for both trees, so that their rows keep the
// same offset per level (mitcad#393).
constexpr int kTreeIndentation = 16;

// The step of a grouped list's items in from their group's heading: one
// small step, so that the names are not cut in a narrow list (the Materials
// window, mitcad#1083). A list whose headings fold draws their arrows in it:
// Qt's styles draw the branch arrow in a 9 px decoration centred in the
// indentation, so 12 leaves it whole with a margin on each side.
constexpr int kGroupedListIndentation = 12;

// What a list or tree shows, which decides its indentation per level
// (mitcad#1214, docs/ui-conventions.md "Lists and trees").
enum class Indent {
  Tree,        // nested levels that fold (components, folders, groups of groups)
  GroupedList, // one level of items under their groups' headings
  Flat,        // rows without children: no space in front of them
};

constexpr int indentationOf(Indent kind) {
  return kind == Indent::Tree ? kTreeIndentation : kind == Indent::GroupedList ? kGroupedListIndentation : 0;
}

// Every QTreeView and QTreeWidget of the app takes its indentation from
// here, where it is made, and never keeps the style's default or a number
// of its own: the ctest repo.tree-indentation (tools/tree-indentation-check)
// fails on a view made without it. It changes the indentation only: the
// view's root decoration, folding, item flags and headings stay as the view
// sets them. The indentation is also the gap between a fold arrow and the
// item's icon or text (the arrow is centred in the step, the item begins at
// its end): the Projects panel's tidy gap is its 16 px step and nothing
// else. The macOS frontend's outline views
// follow the same values (their indentationPerLevel).
inline void setCompactIndentation(QTreeView* view, Indent kind) {
  if (view != nullptr) {
    view->setIndentation(indentationOf(kind));
  }
}

} // namespace panel
} // namespace mitcad
