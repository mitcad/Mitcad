// SPDX-License-Identifier: MIT
#pragma once

#include <QColor>
#include <QIcon>
#include <QRectF>
#include <QString>

class QPainter;

namespace mitcad {

// How a feature's build state is shown (mitcad#961): its timeline cell's
// fill, rings and status badge, and the selection card's line. A failed
// feature must be told from the others in every state of its cell, by shape
// as well as by colour.
enum class FeatureState { Ok, Warning, Failed };

// The state from the model's status text: `error` failed, `warning`
// succeeded with warnings, anything else ok.
FeatureState featureState(const QString& status);

namespace timeline {

// What a timeline cell shows. A folded group's cell has the worst state of
// its features.
struct CellState {
  FeatureState state = FeatureState::Ok;
  bool noted = false;      // an import placeholder (mitcad#170): a yellow outline, no badge
  bool selected = false;
  bool found = false;      // a match of the search (mitcad#179)
  bool hovered = false;
  bool rolledBack = false; // after the marker or suppressed: the icon dimmed, the badge not
};

// The strip's colours for the cells (TimelineWidget's stripColors).
struct CellColors {
  QColor errorFill, errorBorder, warningFill, warningBorder;
  QColor selectedFill, selectedBorder, hoverFill, found;
  QColor hoverRing; // outside the status ring of a failed or warned cell the pointer is on
  QColor halo;      // around the status badge: the strip's background
};

// What a cell is painted with: its fill, the status ring (the red or
// yellow border, or an import note's), the outer ring of the selection or
// a search match (or, faint, the pointer's on a failed or warned cell)
// drawn outside the status ring, and the status badge.
// An invalid colour: none.
struct CellLook {
  QColor fill;
  QColor statusRing;
  QColor outerRing;
  bool badge = false;
};

CellLook cellLook(const CellState& state, const CellColors& colors);

// The cell's fill and rings, under its icon. `bounds` is the cell's whole
// rectangle: with an outer ring, that ring is on its edge and the status
// ring just inside it; else the status ring is on the edge.
void paintCellFrame(QPainter& painter, const QRectF& bounds, const CellLook& look);

// Where the status badge is: over the top right corner of the cell (the
// fx badge of mitcad#809 is at the bottom right).
QRectF statusBadgeRect(const QRectF& bounds);

// The status badge over the icon, at full opacity: a red circle with a
// white "!" for a failed feature, an amber triangle with a dark "!" for a
// warning, each in a ring of `halo` that keeps it apart from the cell.
// Nothing for an ok one.
void paintStatusBadge(QPainter& painter, const QRectF& bounds, FeatureState state, const QColor& halo);

// For the log and the tests: "failed, selected, status ring, outer ring,
// badge" (the state, then what was painted).
QString describeCell(const CellState& state, const CellLook& look);

} // namespace timeline

// The failures' and warnings' buttons (mitcad#961) are badges of a colour of
// their own on every theme and background: red with white text, amber with
// dark text (at least 4.5:1), as capsules. Their fill, text colour and
// style sheet, for a failed or a warned state.
QColor issueBadgeColor(FeatureState state);
QColor issueBadgeTextColor(FeatureState state);
QString issueBadgeStyle(FeatureState state);

// The badge as an icon on those buttons: the red circle or the amber
// triangle in a halo of the button's text colour; empty for an ok feature.
QIcon featureStateIcon(FeatureState state);

// The selection card's line of a failed feature, `Failed: <error>` in the
// error colour, or of one with a warning, `Warning: <message>` in the
// warning colour (rich text); empty for any other.
QString featureStatusLine(const QString& status, const QString& message, const QColor& errorColor,
                          const QColor& warningColor);

} // namespace mitcad
