// SPDX-License-Identifier: MIT
#pragma once

// The Browser's row painting for another tree (mitcad#2427: the timeline's
// History list): the same delegate the Browser's rows have in the layout
// the window has (Floating: the sidebar's rounded selection band;
// Docked: the panels' row band), reading the same item data roles. Made in
// BrowserPanel.cpp, beside the delegates, so that there is one painting of
// a row.

#include <QColor>
#include <QIcon>
#include <QModelIndex>
#include <QRect>
#include <QStyleOptionViewItem>
#include <QtGlobal>

#include "../framework/NoticeLevel.hpp"

class QAbstractItemView;
class QPainter;
class QStyledItemDelegate;

namespace mitcad::browserRows {

// The item data roles of the name column (BrowserPanel.cpp's own, checked
// equal there): a row's light bulb (bool: shown), its fx badge (the count
// of its properties that follow expressions), its name's colour role
// (int: a ColorRole), the owner's mark (bool, mitcad#1704) and its
// component's colour bar (QColor, mitcad#1876).
constexpr int kEyeRole = Qt::UserRole + 1;
constexpr int kBindingsRole = Qt::UserRole + 4;
constexpr int kEyeTipRole = Qt::UserRole + 8;
constexpr int kTextRole = Qt::UserRole + 12;
constexpr int kOwnerMarkRole = Qt::UserRole + 15;
constexpr int kComponentColorRole = Qt::UserRole + 16;

// The Browser's delegate for the window's layout, owned by `view`, set as
// its item delegate.
QStyledItemDelegate* installDelegate(QAbstractItemView* view);
// The light bulb before the icon (Classic) or in the column at the row's
// right edge (Right), as Preferences › Display › Browser Show eye puts the
// Browser's (mitcad#2223).
void setEyeRight(QStyledItemDelegate* delegate, bool right);

// Where the delegate draws a row's parts, in the view's viewport: its
// light bulb and its fx badge (empty without one). `option` is the view's
// item option with the row's rect.
struct RowParts {
  QRect eye;
  QRect fx;
  QRect name;
};
RowParts rowParts(const QStyledItemDelegate* delegate, const QStyleOptionViewItem& option,
                  const QModelIndex& index);

// A row's icon with a level's badge in its corner, as the Browser marks a
// failed or warned feature's row.
QIcon levelIcon(const QIcon& icon, NoticeLevel level);

// The component colour bar at a row's left (mitcad#1876), as the Browser
// paints it: in the fold arrow's place left of the row's `cell` (its
// visual rect in the viewport), `indentation` wide.
void paintComponentBar(QPainter* painter, const QRect& cell, int indentation, const QColor& color);

// The active component's header tint (mitcad#2618, Preferences > Display >
// Highlight the active component in the Browser): the component's colour
// (its bar's) at one intensity, translucent over the row's surface, so that
// it follows the theme: a pale tint on a light surface, a deep one on a dark
// surface. 25%: the most that keeps the rows' name colours (Text and the
// hidden rows' TextHidden) at least 4.5:1 on it in either theme with every
// component colour, also under the Rows background's hover tint (the unit
// test browser_active_highlight checks it).
inline constexpr double kActiveComponentTintAlpha = 0.25;
inline QColor activeComponentTint(QColor component) {
  if (component.isValid()) {
    component.setAlphaF(static_cast<float>(kActiveComponentTintAlpha));
  }
  return component;
}
// The tint as it shows over an opaque `surface`: what the row's text is
// read on.
inline QColor activeComponentTintOn(const QColor& component, const QColor& surface) {
  const double a = kActiveComponentTintAlpha;
  const auto mix = [a](int top, int under) { return qRound(a * top + (1.0 - a) * under); };
  return QColor(mix(component.red(), surface.red()), mix(component.green(), surface.green()),
                mix(component.blue(), surface.blue()));
}

} // namespace mitcad::browserRows
