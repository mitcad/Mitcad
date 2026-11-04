// SPDX-License-Identifier: MIT
#include "PickerComboBox.hpp"

#include <algorithm>

#include <QIcon>
#include <QImage>
#include <QObject>
#include <QPixmap>
#include <QPoint>
#include <QRect>
#include <QSize>
#include <QStringList>
#include <QStyle>
#include <QStyleOptionComboBox>
#include <QStylePainter>
#include <QtLogging>

namespace mitcad {
namespace {

// The size of an option's swatch in a choice's picker, in pixels
// (mitcad#1141; the Material tool's picker's too).
constexpr int kPickerSwatch = 32;

} // namespace

MiddleElideComboBox::MiddleElideComboBox(QWidget* parent) : QComboBox(parent) {
  setSizeAdjustPolicy(QComboBox::AdjustToMinimumContentsLengthWithIcon);
  setMinimumContentsLength(8);
}

void MiddleElideComboBox::paintEvent(QPaintEvent*) {
  QStylePainter painter(this);
  QStyleOptionComboBox option;
  initStyleOption(&option);
  painter.drawComplexControl(QStyle::CC_ComboBox, option);
  const QRect field = style()->subControlRect(QStyle::CC_ComboBox, &option, QStyle::SC_ComboBoxEditField, this);
  const int icon = option.currentIcon.isNull() ? 0 : option.iconSize.width() + 4;
  option.currentText = fontMetrics().elidedText(shownText(), Qt::ElideMiddle, std::max(0, field.width() - icon));
  painter.drawControl(QStyle::CE_ComboBoxLabel, option);
}

PickerComboBox::PickerComboBox(const QString& label, QWidget* parent) : MiddleElideComboBox(parent), m_label(label) {}

void PickerComboBox::setRows(const QVector<ChoicePickerRow>& rows) {
  m_rows.clear();
  for (const ChoicePickerRow& row : rows) {
    m_rows.insert(row.value, row);
  }
  update();
}

QString PickerComboBox::shownText() const {
  const QString shortLabel = m_rows.value(currentData().toString()).shortLabel;
  return shortLabel.isEmpty() ? currentText() : shortLabel;
}

void PickerComboBox::logShown(const QString& command, const QString& key) {
  const QString line = QStringLiteral("Panel %1 input %2 shows: %3").arg(command, key, shownText());
  if (line != m_logged) {
    m_logged = line;
    qDebug().noquote() << line;
  }
}

void PickerComboBox::showPopup() {
  if (m_picker != nullptr && m_picker->isVisible()) {
    return; // open already
  }
  const QString chosenValue = currentData().toString();
  QVector<PickerEntry> entries;
  bool lineAbove = false;
  for (int i = 0; i < count(); ++i) {
    const QString value = itemData(i).toString();
    if (isChoiceSeparator(value)) {
      lineAbove = true;
      continue;
    }
    if (value.startsWith(choiceHeadingPrefix())) {
      continue; // the rows carry the headings
    }
    const auto found = m_rows.constFind(value);
    const bool known = found != m_rows.constEnd();
    PickerEntry entry;
    entry.value = value;
    entry.name = known && !found->shortLabel.isEmpty() ? found->shortLabel : itemText(i);
    entry.section = known ? found->heading : QString();
    // The option's picture as its swatch (InputDef::choiceImage,
    // mitcad#1141: Send to Slicer's materials).
    const QIcon icon = itemIcon(i);
    if (!icon.isNull()) {
      entry.swatch = icon.pixmap(QSize(kPickerSwatch, kPickerSwatch)).toImage();
    }
    entry.tooltip = itemText(i);
    entry.tag = known ? found->tag : QString();
    entry.search = known ? QStringList{itemText(i), found->heading, found->search}.join(QLatin1Char(' ')) : itemText(i);
    entry.checked = value == chosenValue;
    entry.lineBefore = lineAbove;
    lineAbove = false;
    entries.append(entry);
  }
  QPointer<PickerComboBox> self(this);
  m_picker = LibraryPicker::open(this, QObject::tr("%1 Picker").arg(m_label), QStringLiteral("%1 picker").arg(m_label),
                                 entries, chosenValue, mapToGlobal(QPoint(0, height())),
                                 [self](const QString& value) {
                                   if (self != nullptr) {
                                     self->choose(value);
                                   }
                                 },
                                 objectName(), width());
}

void PickerComboBox::choose(const QString& value) {
  const int at = findData(value);
  if (at < 0) {
    return;
  }
  setCurrentIndex(at);
  emit activated(at);
}

} // namespace mitcad
