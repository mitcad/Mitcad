// SPDX-License-Identifier: MIT
#include "SheetSizeRows.hpp"

#include <QComboBox>
#include <QCoreApplication>
#include <QDoubleSpinBox>
#include <QFormLayout>

#include "DrawingTemplates.hpp"

namespace mitcad::drawings {
namespace {

const QString kCustom = QStringLiteral("custom");

// Shows the width and height rows for Custom only, and the orientation
// on for named sizes.
void showCustom(QFormLayout* form, QComboBox* size, QDoubleSpinBox* width, QDoubleSpinBox* height,
                QComboBox* orientation) {
  const bool own = size->currentData().toString() == kCustom;
  form->setRowVisible(width, own);
  form->setRowVisible(height, own);
  if (orientation != nullptr) {
    orientation->setEnabled(!own);
  }
}

} // namespace

SheetSizeRows::SheetSizeRows(QFormLayout* form, const QString& sizeName) : m_form(form) {
  m_size = new QComboBox;
  m_size->setObjectName(sizeName);
  for (const char* name : {"A4", "A3", "A2", "A1", "A0", "ANSI_A", "ANSI_B", "ANSI_C", "ANSI_D", "ANSI_E"}) {
    const QString value = QString::fromLatin1(name);
    m_size->addItem(QString(value).replace(QLatin1Char('_'), QLatin1Char(' ')), value);
  }
  m_size->addItem(QCoreApplication::translate("SheetSizeRows", "Custom"), kCustom);
  m_size->setToolTip(QCoreApplication::translate("SheetSizeRows", "A standard sheet, or Custom for a width and height of your own"));
  const auto length = [](const QString& name, double value) {
    auto* box = new QDoubleSpinBox;
    box->setObjectName(name);
    // The model's range (core/model/src/drawing/mod.rs, CUSTOM_SHEET_MIN
    // and CUSTOM_SHEET_MAX).
    box->setRange(10.0, 10000.0);
    box->setDecimals(1);
    box->setSuffix(QStringLiteral(" mm"));
    box->setValue(value);
    return box;
  };
  m_width = length(sizeName + QStringLiteral("Width"), 500.0);
  m_height = length(sizeName + QStringLiteral("Height"), 300.0);
  form->addRow(QCoreApplication::translate("SheetSizeRows", "Size"), m_size);
  form->addRow(QCoreApplication::translate("SheetSizeRows", "Sheet Width"), m_width);
  form->addRow(QCoreApplication::translate("SheetSizeRows", "Sheet Height"), m_height);
  // The rows follow the choice; the connections hold the widgets, not this
  // (the dialog's widgets outlive it).
  QObject::connect(m_size, &QComboBox::currentIndexChanged, m_size,
                   [form, size = m_size, width = m_width, height = m_height] {
                     showCustom(form, size, width, height, nullptr);
                   });
  update();
}

void SheetSizeRows::linkOrientation(QComboBox* orientation) {
  m_orientation = orientation;
  QObject::connect(m_size, &QComboBox::currentIndexChanged, orientation, [size = m_size, orientation] {
    orientation->setEnabled(size->currentData().toString() != kCustom);
  });
  update();
}

bool SheetSizeRows::custom() const { return m_size->currentData().toString() == kCustom; }

void SheetSizeRows::set(const QString& text) {
  double width = 0.0;
  double height = 0.0;
  if (customSheetSize(text, &width, &height)) {
    m_width->setValue(width);
    m_height->setValue(height);
    m_size->setCurrentIndex(m_size->findData(kCustom));
    return;
  }
  if (text.isEmpty()) {
    return;
  }
  int at = m_size->findData(text);
  if (at < 0) {
    m_size->insertItem(m_size->count() - 1, text, text);
    at = m_size->findData(text);
  }
  m_size->setCurrentIndex(at);
}

QString SheetSizeRows::text() const {
  if (custom()) {
    return QStringLiteral("%1x%2").arg(QString::number(m_width->value(), 'g', 10),
                                       QString::number(m_height->value(), 'g', 10));
  }
  return m_size->currentData().toString();
}

QJsonValue SheetSizeRows::value() const { return sheetSizeValue(text()); }

void SheetSizeRows::update() {
  if (m_shown) {
    showCustom(m_form, m_size, m_width, m_height, m_orientation);
  }
}

void SheetSizeRows::setShown(bool shown) {
  m_shown = shown;
  m_form->setRowVisible(m_size, shown);
  if (shown) {
    update();
  } else {
    m_form->setRowVisible(m_width, false);
    m_form->setRowVisible(m_height, false);
  }
}

} // namespace mitcad::drawings
