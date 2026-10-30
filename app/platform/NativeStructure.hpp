// SPDX-License-Identifier: MIT
#pragma once

#include <QList>

class QWidget;

namespace mitcad {

// The order in which the native frontend compares a presenter's widget tree
// before rebuilding its native controls (mitcad#986). `widgets` are the
// presented descendants of `root` as findChildren lists them (each one's
// parent is `root` or another of them); the result holds the same widgets.
// Children keep their order, since a container without a layout presents
// them in that order, except a scroll area's own children (an item view's
// header, viewport and scroll bar containers): Qt raises and lowers those
// as it lays them out, and no native control presents their order, so they
// are compared as a set. Each one's descendants still follow it in order.
QList<QWidget*> nativeStructureOrder(const QWidget* root, const QList<QWidget*>& widgets);

} // namespace mitcad
