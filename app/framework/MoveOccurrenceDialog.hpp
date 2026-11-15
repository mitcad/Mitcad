// SPDX-License-Identifier: MIT
#pragma once

#include <QString>
#include <QStringList>

#include "HybridAssembly.hpp"

class QWidget;

namespace mitcad {

// Move to... on a component occurrence (mitcad#219): asks for the new
// parent among the root and the placed components, each by its full path
// with the reason under it when it cannot be one (mitcad#1847); shows what
// moves with it and the references that follow it; then runs the model's
// `reparent_occurrence` (one undo step; the occurrence stays where it is in
// the design). `occurrence` is its path from the root (`O1/O4`). Returns
// its new path, or an empty string when nothing moved (Cancel). A refused
// move keeps the dialog open with the model's reason.
QString moveOccurrence(QWidget* parent, const HybridModel& model, const QString& occurrence);

// The same for several occurrences of one parent, moved together in one
// step (mitcad#1847: the selected rows); the first one's new path is
// returned. When they are in a rigid group with others, the dialog offers
// to move the whole group.
QString moveOccurrence(QWidget* parent, const HybridModel& model, const QStringList& occurrences);

} // namespace mitcad
