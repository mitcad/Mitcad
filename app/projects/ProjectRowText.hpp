// SPDX-License-Identifier: MIT
#pragma once

// The Projects panel's rows in words and marks (mitcad#529): a row's name
// gets the width, and its state shrinks to compact marks, a short text and
// at most one icon, with the full text in the tooltip:
//
//   design   "v2"   version 2          "v2*"  changed since version 2
//            "new"  no version yet     icon   lock (Editing elsewhere,
//                                             Locked: Alex), edit (Editing)
//   file     "new"  no version yet     "*"    changed since the latest
//                                             version (mitcad#2204)
//   project  icon   cloud-synced (Cloud, synced), cloud (Cloud, with "↑2 ↓1"
//                   to send and newer), cloud-offline (Offline, "↑2"
//                   waiting), warning (conflict, sign-in needed, missing,
//                   cannot be read); a Local project none (its row's icon
//                   tells), one without versions "–"
//
// A name that still does not fit is elided in the middle (elideMiddle),
// keeping its start and end. Plain Qt Core and Gui, for app.unit.

#include <QString>

class QFontMetrics;

namespace mitcad {

struct ProjectDesignRow;
struct ProjectFileRow;
struct ProjectRow;

// A row's state column.
struct RowMarks {
  QString text;  // the compact text shown ("v2*", "↑2 ↓1"; "" none)
  QString icon;  // a theme icon's name ("lock", "cloud-synced"; "" none)
  QString full;  // the state in full words ("v2 · Changed · Editing elsewhere"), for the tooltip
  QString words; // the state as the log's "Projects tree:" lines have it ("v2, changed, editing elsewhere")
  // The marks as the log's "Projects text" lines show them: the text and
  // the icon in brackets ("v2* (lock)", "(cloud-synced)"); "none".
  QString logged() const;
};

// A design row's marks: its version, Changed and its edit lock.
RowMarks designMarks(const ProjectDesignRow& design);
// Another file's marks (mitcad#2204): "new" when no version holds it, "*"
// when it changed since the latest version, none when it is recorded as it
// is.
RowMarks fileMarks(const ProjectFileRow& file);

// A project row's marks: Local or Cloud and its sync state as the project
// indicator shows it. `ahead`, `behind` (-1 not known), `problem` (the
// remote's last failure class, "" none) and `conflict`: the current
// project's remote as the remote work knows it, else the row's own.
RowMarks projectMarks(const ProjectRow& row, int ahead, int behind, const QString& problem, bool conflict);

// `text` elided in the middle to `width` pixels of `metrics` (its start and
// end kept, "mounting-br…ly-rev-b"); whole when it fits.
QString elideMiddle(const QString& text, const QFontMetrics& metrics, int width);

// A design's name as the panel shows it: its file's name without .mitcad.
QString designDisplayName(const QString& path);

} // namespace mitcad
