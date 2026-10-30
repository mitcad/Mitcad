// SPDX-License-Identifier: MIT
#include "EditOptionsDialog.hpp"

#include <utility>

#include <QDialogButtonBox>
#include <QFormLayout>
#include <QJsonArray>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QShowEvent>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/ParameterField.hpp"
#include "../framework/TestSync.hpp"
#include "../platform/MacDialogs.hpp"
#include "OptionChooser.hpp"

namespace mitcad {

EditOptionsDialog::EditOptionsDialog(JsonRunner query, const QString& component, const QString& name,
                                     const QJsonObject& libraryTable, QWidget* parent)
    : QDialog(parent), m_component(component) {
  setObjectName(QStringLiteral("editOptionsDialog"));
  setWindowTitle(tr("Edit Options - %1").arg(name));
  auto* layout = new QVBoxLayout(this);
  m_intro = new QLabel(tr("The options of %1, shared by all of its occurrences. Choose a value, or give an "
                          "expression of this design's parameters: the component then follows them.")
                           .arg(name));
  m_intro->setWordWrap(true);
  layout->addWidget(m_intro);
  m_chooser = new OptionChooser;
  m_chooser->setLogName(QStringLiteral("Edit Options"));
  // The component's own expressions first; then those bound here.
  m_chooser->setOptionsQuery(componentOptionsQuery(std::move(query), component, [this] {
    return m_read ? QJsonValue(m_bound) : QJsonValue();
  }));
  // The library's rows with their standards and suppliers' articles
  // (mitcad#949): the filters above the choosers.
  QStringList standards;
  for (const QJsonValue& value : libraryTable.value(QStringLiteral("standards")).toArray()) {
    standards << value.toString();
  }
  m_chooser->setFilterRows(libraryTable.value(QStringLiteral("rows")).toArray(), standards);
  layout->addWidget(m_chooser);
  m_supplierParts = new QLabel;
  m_supplierParts->setObjectName(QStringLiteral("editOptionsSupplierParts"));
  m_supplierParts->setWordWrap(true);
  m_supplierParts->setTextInteractionFlags(Qt::TextSelectableByMouse);
  m_supplierParts->hide();
  layout->addWidget(m_supplierParts);
  auto* heading = new QLabel(tr("<b>Expressions</b>"));
  layout->addWidget(heading);
  m_expressionsForm = new QFormLayout;
  layout->addLayout(m_expressionsForm);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  m_ok = buttons->button(QDialogButtonBox::Ok);
  mac::styleDialogButtons(buttons, m_ok, true);
  layout->addWidget(buttons);
  connect(buttons, &QDialogButtonBox::accepted, this, &EditOptionsDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
  mac::styleDialog(this);

  m_has = m_chooser->showOptions() && !m_chooser->answer().options.isEmpty();
  m_read = true;
  for (const PartOption& option : m_chooser->answer().options) {
    if (option.bound) {
      m_bound.insert(option.name, option.parameter);
    }
  }
  if (!m_has) {
    const QString why = m_chooser->answer().ok() ? tr("it has no options") : m_chooser->answer().error;
    m_intro->setText(tr("%1 cannot be configured here: %2.").arg(name, why));
    m_ok->setEnabled(false);
    qInfo().noquote() << QStringLiteral("Edit Options of %1: no options (%2)").arg(name, why);
  }
  fillExpressions();
  showSupplierParts();
  connect(m_chooser, &OptionChooser::optionsChanged, this, &EditOptionsDialog::fillExpressions);
  connect(m_chooser, &OptionChooser::rowChanged, this, &EditOptionsDialog::showSupplierParts);
  m_placesTimer.setSingleShot(true);
  m_placesTimer.setInterval(100);
  connect(&m_placesTimer, &QTimer::timeout, this, [this] { logPlaces(); });
  TestSync::watch(&m_placesTimer);
}

void EditOptionsDialog::fillExpressions() {
  const OptionsAnswer& answer = m_chooser->answer();
  QStringList order;
  for (const PartOption& option : answer.options) {
    order << option.name;
  }
  if (order != m_order) {
    // The old fields go later: one of them may be the sender of this change.
    while (m_expressionsForm->rowCount() > 0) {
      const QFormLayout::TakeRowResult taken = m_expressionsForm->takeRow(0);
      QLayoutItem* const items[] = {taken.labelItem, taken.fieldItem};
      for (QLayoutItem* item : items) {
        if (item == nullptr) {
          continue;
        }
        if (QWidget* widget = item->widget()) {
          widget->hide();
          widget->deleteLater();
        }
        delete item;
      }
    }
    m_expressions.clear();
    m_order = order;
    for (const QString& option : std::as_const(m_order)) {
      auto* edit = new QLineEdit;
      edit->setObjectName(QStringLiteral("optionExpression%1").arg(m_expressions.size()));
      ParameterFieldOptions fieldOptions;
      fieldOptions.anyKind = true;
      fieldOptions.label = tr("Option %1").arg(option);
      ParameterField::attach(edit, fieldOptions);
      connect(edit, &QLineEdit::editingFinished, this, [this, option] { expressionEdited(option); });
      m_expressionsForm->addRow(option + QLatin1Char(':'), edit);
      m_expressions.insert(option, edit);
    }
  }
  // The choosers above may have been made again: the places anew.
  if (isVisible()) {
    m_placesTimer.start();
  }
  for (const PartOption& option : answer.options) {
    QLineEdit* edit = m_expressions.value(option.name);
    if (edit == nullptr || (edit->hasFocus() && edit->isModified())) {
      continue;
    }
    // A value of the source's own formula is not the component's to set.
    const bool sourceDriven = option.driven && !option.bound && !m_bound.contains(option.name);
    edit->setReadOnly(sourceDriven);
    QString text;
    if (m_bound.contains(option.name)) {
      text = m_bound.value(option.name).toString();
    } else if (sourceDriven) {
      text = option.expression;
      edit->setToolTip(tr("Set by the component's own formula"));
    } else {
      text = isConstantExpression(option.expression) ? option.expression
                                                     : constantExpression(option, option.valueText);
    }
    edit->setText(text);
    edit->setModified(false);
  }
}

void EditOptionsDialog::expressionEdited(const QString& option) {
  QLineEdit* edit = m_expressions.value(option);
  const PartOption* shown = m_chooser->answer().option(option);
  if (edit == nullptr || shown == nullptr || edit->isReadOnly()) {
    return;
  }
  const QString text = edit->text().trimmed();
  edit->setModified(false);
  if (text.isEmpty()) {
    fillExpressions();
    return;
  }
  if (isConstantExpression(text)) {
    // A constant: the option is chosen again, no longer bound.
    const bool wasBound = m_bound.contains(option);
    m_bound.remove(option);
    if (wasBound) {
      qInfo().noquote() << QStringLiteral("Edit Options unbound %1").arg(option);
      m_chooser->requery();
    }
    m_chooser->setOption(option, queryValue(*shown, text));
    return;
  }
  if (m_bound.value(option).toString() == text) {
    return;
  }
  m_bound.insert(option, text);
  qInfo().noquote() << QStringLiteral("Edit Options bound %1 = %2").arg(option, text);
  m_chooser->requery();
}

void EditOptionsDialog::showSupplierParts() {
  const QString numbers =
      supplierPartsText(m_chooser->filterRow(m_chooser->row()).value(QStringLiteral("supplier_parts")).toArray());
  m_supplierParts->setText(tr("Supplier part numbers: %1").arg(numbers));
  m_supplierParts->setVisible(!numbers.isEmpty());
  if (!numbers.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Edit Options supplier part numbers: %1").arg(numbers);
  }
}

QJsonObject EditOptionsDialog::parameters() const {
  QJsonObject out;
  for (const PartOption& option : m_chooser->answer().options) {
    if (m_bound.contains(option.name)) {
      out.insert(option.name, m_bound.value(option.name));
      continue;
    }
    if (option.driven) {
      continue; // the source's own formula
    }
    out.insert(option.name, isConstantExpression(option.expression) ? option.expression
                                                                      : constantExpression(option, option.valueText));
  }
  return out;
}

QString EditOptionsDialog::summary() const {
  QStringList parts;
  const QJsonObject expressions = parameters();
  for (const QString& option : m_order) {
    if (expressions.contains(option)) {
      parts << QStringLiteral("%1 = %2").arg(option, expressions.value(option).toString());
    }
  }
  return parts.join(QStringLiteral(", "));
}

void EditOptionsDialog::accept() {
  // A field edited and confirmed with OK at once: taken first.
  for (auto it = m_expressions.cbegin(); it != m_expressions.cend(); ++it) {
    if (it.value()->isModified()) {
      expressionEdited(it.key());
    }
  }
  if (!m_has) {
    return;
  }
  qInfo().noquote() << QStringLiteral("Edit Options accepted: %1").arg(summary());
  QDialog::accept();
}

void EditOptionsDialog::showEvent(QShowEvent* event) {
  QDialog::showEvent(event);
  m_placesTimer.start();
}

void EditOptionsDialog::logPlaces() const {
  if (!isVisible()) {
    return;
  }
  const QWidget* host = parentWidget() != nullptr ? parentWidget()->window() : nullptr;
  for (const QString& option : m_order) {
    const QLineEdit* edit = m_expressions.value(option);
    if (edit == nullptr) {
      continue;
    }
    const QPoint global = edit->mapToGlobal(edit->rect().center());
    const QPoint at = host != nullptr ? host->mapFromGlobal(global) : global;
    qDebug().noquote() << QStringLiteral("Edit Options %1 expression at %2,%3").arg(option).arg(at.x()).arg(at.y());
  }
  if (m_ok != nullptr) {
    const QPoint global = m_ok->mapToGlobal(m_ok->rect().center());
    const QPoint at = host != nullptr ? host->mapFromGlobal(global) : global;
    qDebug().noquote() << QStringLiteral("Edit Options OK at %1,%2").arg(at.x()).arg(at.y());
  }
}

} // namespace mitcad
