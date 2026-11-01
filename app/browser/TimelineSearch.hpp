// SPDX-License-Identifier: MIT
#pragma once

#include <QString>
#include <QStringList>

namespace mitcad {

// What a timeline search looks at of a feature (mitcad#179).
struct TimelineSearchItem {
  QString name;
  QString type;      // sketch, fillet, construction_plane, ...
  QString component; // the component's name
  bool failed = false;
  bool warned = false; // built with warnings
  bool noted = false;  // an import note (mitcad#170): information (mitcad#1040)
  bool suppressed = false;
  bool rolledBack = false; // after the history marker
};

// The text of the timeline's search field, read (mitcad#179): words that
// a feature's name or type must contain (all of them, ignoring case; a
// "quoted phrase" is one word), and filters. `in:<component>` (or
// `component:`) keeps the features of the components whose names contain
// it, `in:"Two words"` with quotes; `is:<status>` (or `status:`) those of a
// status: failed (error), warning, info (note: the import's notes,
// information since mitcad#1040), suppressed, rolled-back (rolledback). A
// status may be abbreviated (is:f, is:sup). Filters of one kind are
// alternatives, of different kinds all apply. A filter without a value is
// ignored while it is typed; a status it does not know matches nothing.
struct TimelineQuery {
  enum Status { Failed = 1, Warning = 2, Suppressed = 4, RolledBack = 8, Info = 16 };

  QStringList words;      // lower case
  QStringList components; // lower case
  int statuses = 0;       // Status flags
  QStringList unknown;    // filters not understood, as typed

  static TimelineQuery parse(const QString& text);
  bool isEmpty() const { return words.isEmpty() && components.isEmpty() && statuses == 0 && unknown.isEmpty(); }
  bool matches(const TimelineSearchItem& item) const;
};

// The filters the field's menu adds: `is:failed`, `in:Part`, `in:"Two
// words"`.
QString timelineStatusToken(TimelineQuery::Status status);
QString timelineComponentToken(const QString& component);
// The text with the filter added, or taken out when it has it (in any
// case, quoted or not); the other words and filters stay in their order.
QString toggleTimelineToken(const QString& text, const QString& token);
// Whether the text has the filter.
bool hasTimelineToken(const QString& text, const QString& token);

} // namespace mitcad
