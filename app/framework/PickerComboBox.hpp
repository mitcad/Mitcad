// SPDX-License-Identifier: MIT
#pragma once

// The drop-downs of a command panel's choices that elide in the middle and
// open a searchable picker (InputDef::choicePicker, mitcad#1108), shared
// with the Make tab's body card (mitcad#1239: its Printer and Process).

#include <QColor>
#include <QComboBox>
#include <QHash>
#include <QImage>
#include <QPointer>
#include <QString>
#include <QVector>

#include "../browser/LibraryPicker.hpp"
#include "Command.hpp"

class QPaintEvent;
class QWidget;

namespace mitcad {

// A material's swatch in a material picker and its field (mitcad#1141:
// Send to Slicer's material rows; mitcad#1411: the Make card's Material):
// a rounded square of its colour (none: the default look's grey, a default
// appearance's base colour), as the Material tool's picker.
QImage materialSwatch(const QColor& color);

// A drop-down whose chosen text is cut in the middle, and only when it
// must, so that a long name keeps its start and its end (mitcad#655: 3D
// Print's printer and process); it may be narrower than its longest
// option.
class MiddleElideComboBox : public QComboBox {
public:
  explicit MiddleElideComboBox(QWidget* parent = nullptr);

  // What the closed field shows of the chosen option.
  virtual QString shownText() const { return currentText(); }

protected:
  void paintEvent(QPaintEvent* event) override;
};

// A drop-down that opens a searchable picker instead of its list
// (InputDef::choicePicker, mitcad#1108: Send to Slicer's Printer and
// Process): the options in their order under the rows' headings, by their
// short labels with their tags, the chosen one checked, a line where the
// options have a separator, the options' pictures as their swatches
// (mitcad#1141); a window titled "<label> Picker" (LibraryPicker,
// logged "<label> picker …") under the field, as wide as its names need.
// Its choice is the drop-down's, as if chosen in its list. The closed
// field shows the chosen row's short label.
class PickerComboBox : public MiddleElideComboBox {
public:
  explicit PickerComboBox(const QString& label, QWidget* parent = nullptr);

  void setRows(const QVector<ChoicePickerRow>& rows);

  QString shownText() const override;

  // "Panel <command> input <key> shows: <short label>" when it changes, for
  // UI tests.
  void logShown(const QString& command, const QString& key);

  void showPopup() override;

private:
  void choose(const QString& value);

  QString m_label;
  QHash<QString, ChoicePickerRow> m_rows;
  QPointer<LibraryPicker> m_picker;
  QString m_logged;
};

} // namespace mitcad
