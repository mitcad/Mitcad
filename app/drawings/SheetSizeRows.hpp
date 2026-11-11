// SPDX-License-Identifier: MIT
#pragma once

// A sheet's size in Create Drawing and Sheet… (mitcad#1625): the named
// sizes (A4 … A0, ANSI A … E) and Custom, whose Sheet Width and Sheet
// Height rows show only while it is chosen. A size of its own lies as
// given, so its sheet's Orientation is off while Custom is chosen.

#include <QJsonValue>
#include <QString>

class QComboBox;
class QDoubleSpinBox;
class QFormLayout;

namespace mitcad::drawings {

class SheetSizeRows {
public:
  // The rows added to `form`: Size (named `sizeName`), Sheet Width and
  // Sheet Height (`sizeName` + "Width", "Height").
  SheetSizeRows(QFormLayout* form, const QString& sizeName);
  SheetSizeRows(const SheetSizeRows&) = delete;
  SheetSizeRows& operator=(const SheetSizeRows&) = delete;
  // Off while Custom is chosen.
  void linkOrientation(QComboBox* orientation);
  // A size as sheetSizeText gives it ("A3", "500x300"); one the list has
  // not is added to it.
  void set(const QString& text);
  // The size chosen, as sheetSizeText gives it.
  QString text() const;
  // As the model's `size` takes it.
  QJsonValue value() const;
  bool custom() const;
  QComboBox* combo() const { return m_size; }
  // Shows the width and height rows for Custom only, and the orientation
  // on for named sizes.
  void update();
  // All three rows hidden (a cut sheet in Create Drawing), or shown again
  // as the choice has them.
  void setShown(bool shown);

private:
  QFormLayout* m_form = nullptr;
  QComboBox* m_size = nullptr;
  QDoubleSpinBox* m_width = nullptr;
  QDoubleSpinBox* m_height = nullptr;
  QComboBox* m_orientation = nullptr;
  bool m_shown = true;
};

} // namespace mitcad::drawings
