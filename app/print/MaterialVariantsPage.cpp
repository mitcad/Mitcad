// SPDX-License-Identifier: MIT
#include "MaterialVariantsPage.hpp"

#include <cmath>
#include <utility>

#include <QAbstractItemView>
#include <QAction>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QJsonValue>
#include <QLabel>
#include <QMenu>
#include <QPushButton>
#include <QRect>
#include <QTreeWidget>
#include <QTreeWidgetItem>
#include <QVBoxLayout>
#include <QVector>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../platform/MacDialogs.hpp"
#include "PrintKeys.hpp"
#include "SettingsForm.hpp"

namespace mitcad {
namespace {

constexpr int kLabelRole = Qt::UserRole; // a row's variant label

QString text(const QJsonObject& object, const char* field) {
  return object.value(QLatin1String(field)).toString();
}

// Whether two nozzles (null: any) are the same.
bool sameNozzle(const QJsonValue& a, const QJsonValue& b) {
  if (a.isNull() || a.isUndefined() || b.isNull() || b.isUndefined()) {
    return (a.isNull() || a.isUndefined()) && (b.isNull() || b.isUndefined());
  }
  return std::abs(a.toDouble() - b.toDouble()) < 1e-6;
}

} // namespace

MaterialVariantsPage::MaterialVariantsPage(const QString& logName, QWidget* parent)
    : QWidget(parent), m_log(logName) {
  setObjectName(QStringLiteral("materialVariants"));
  auto* layout = new QVBoxLayout(this);
  layout->setContentsMargins(4, 8, 4, 4);
  // The variants: printer and nozzle, then what differs.
  m_list = new QTreeWidget;
  m_list->setObjectName(QStringLiteral("materialVariantsList"));
  m_list->setColumnCount(2);
  m_list->setHeaderLabels({tr("Printer and nozzle"), tr("Differs in")});
  m_list->setRootIsDecorated(false);
  m_list->setSelectionMode(QAbstractItemView::SingleSelection);
  m_list->setAccessibleName(tr("Variants"));
  m_list->header()->setSectionResizeMode(0, QHeaderView::ResizeToContents);
  m_list->header()->setStretchLastSection(true);
  m_list->setMaximumHeight(160);
  mac::styleListTree(m_list);
  layout->addWidget(m_list);
  // No variant yet: what they are, quietly.
  m_empty = new QLabel(
      tr("No variants. A variant keeps this material's values for one printer and nozzle; Add Variant makes one "
         "from a printer profile."));
  m_empty->setWordWrap(true);
  m_empty->setStyleSheet(QStringLiteral("color: gray;"));
  layout->addWidget(m_empty);
  auto* buttons = new QHBoxLayout;
  m_add = new QPushButton(tr("Add Variant ▾"));
  m_add->setObjectName(QStringLiteral("materialVariantAdd"));
  m_add->setToolTip(tr("Adds this material's values for a printer and its nozzle"));
  m_remove = new QPushButton(tr("Remove Variant"));
  m_remove->setObjectName(QStringLiteral("materialVariantRemove"));
  buttons->addWidget(m_add);
  buttons->addWidget(m_remove);
  buttons->addStretch(1);
  for (QPushButton* button : {m_add, m_remove}) {
    mac::auxiliaryButton(button);
  }
  layout->addLayout(buttons);
  // The selected variant's values: the filament's keys, its own set,
  // the material's as placeholders.
  m_form = new SettingsForm;
  m_form->setNamePrefix(QStringLiteral("variant"));
  layout->addWidget(m_form, 1);

  connect(m_list, &QTreeWidget::currentItemChanged, this, [this](QTreeWidgetItem* row) {
    if (m_building || row == nullptr) {
      return;
    }
    m_current = row->data(0, kLabelRole).toString();
    qDebug().noquote() << QStringLiteral("%1 variant selected: %2").arg(m_log, m_current);
    showSelected();
    emit layoutChanged();
  });
  connect(m_add, &QPushButton::clicked, this, &MaterialVariantsPage::addVariant);
  connect(m_remove, &QPushButton::clicked, this, &MaterialVariantsPage::removeVariant);
  connect(m_form, &SettingsForm::valueChanged, this, [this](const QString& key, const QJsonValue& value) {
    const QJsonObject variant = selected();
    if (variant.isEmpty() || m_keys == nullptr) {
      return;
    }
    const PrintKey* printKey = m_keys->find(key);
    const QString shown = value.isNull() ? QStringLiteral("cleared")
                                         : (printKey != nullptr ? printValueText(*printKey, value)
                                                                : value.toVariant().toString());
    QJsonObject change{{QStringLiteral("printer"), variant.value(QStringLiteral("printer"))},
                       {QStringLiteral("filament"), QJsonObject{{key, value}}}};
    if (!variant.value(QStringLiteral("nozzle")).isNull() && !variant.value(QStringLiteral("nozzle")).isUndefined()) {
      change.insert(QStringLiteral("nozzle"), variant.value(QStringLiteral("nozzle")));
    }
    emit changeRequested(change, QStringLiteral("variant %1 %2 = %3").arg(m_current, key, shown));
  });
  connect(m_form, &SettingsForm::layoutChanged, this, &MaterialVariantsPage::layoutChanged);
}

MaterialVariantsPage::~MaterialVariantsPage() = default;

QString MaterialVariantsPage::variantLabel(const QString& printer, const QJsonValue& nozzle) {
  if (nozzle.isNull() || nozzle.isUndefined()) {
    return tr("%1, any nozzle").arg(printer.trimmed());
  }
  return tr("%1, %2 mm nozzle").arg(printer.trimmed(), printNumberText(nozzle.toDouble()));
}

int MaterialVariantsPage::count() const { return static_cast<int>(m_variants.size()); }

void MaterialVariantsPage::setMaterial(const QJsonObject& fields, const PrintKeyTable& keys, const QString& dialect,
                                       bool readOnly) {
  m_keys = &keys;
  m_dialect = dialect;
  m_readOnly = readOnly;
  m_base = fields.value(QStringLiteral("filament")).toObject();
  m_variants = fields.value(QStringLiteral("variants")).toArray();
  // The fields are made once, for the first material with print settings.
  m_form->setKeys(keys, QStringLiteral("filament"), dialect);
  // The density is the material's; a colour is never a variant's.
  m_form->setHiddenKeys({QStringLiteral("filament_density"), QStringLiteral("filament_colour")});
  rebuild();
}

QString MaterialVariantsPage::differences(const QJsonObject& variant) const {
  const QJsonObject values = variant.value(QStringLiteral("filament")).toObject();
  QStringList shown;
  for (auto it = values.begin(); it != values.end(); ++it) {
    const PrintKey* key = m_keys != nullptr ? m_keys->find(it.key()) : nullptr;
    if (key == nullptr) {
      shown << it.key();
    } else if (!key->perDialect) {
      // A slicer's preset name is the slicers' own, not shown (as in the
      // settings form).
      shown << QStringLiteral("%1 %2").arg(key->name, printValueText(*key, it.value()));
    }
  }
  return shown.isEmpty() ? tr("Same as the material") : shown.join(QStringLiteral(", "));
}

void MaterialVariantsPage::rebuild() {
  m_building = true;
  m_list->clear();
  QTreeWidgetItem* current = nullptr;
  QStringList rows;
  for (const QJsonValue& value : std::as_const(m_variants)) {
    const QJsonObject variant = value.toObject();
    const QString label = variantLabel(text(variant, "printer"), variant.value(QStringLiteral("nozzle")));
    const QString differs = differences(variant);
    auto* row = new QTreeWidgetItem(m_list, {label, differs});
    row->setData(0, kLabelRole, label);
    row->setToolTip(1, differs);
    if (label == m_current) {
      current = row;
    }
    rows << QStringLiteral("%1: %2").arg(label, differs);
  }
  if (current == nullptr && m_list->topLevelItemCount() > 0) {
    current = m_list->topLevelItem(0);
  }
  m_current = current != nullptr ? current->data(0, kLabelRole).toString() : QString();
  m_list->setCurrentItem(current);
  m_building = false;
  const bool any = !m_variants.isEmpty();
  m_list->setVisible(any);
  m_empty->setVisible(!any);
  m_add->setEnabled(!m_readOnly);
  m_remove->setEnabled(!m_readOnly && current != nullptr);
  const QString logged = rows.join(QLatin1Char('\n'));
  if (logged != m_loggedRows) {
    m_loggedRows = logged;
    qDebug().noquote() << QStringLiteral("%1 variants: %2").arg(m_log).arg(rows.size());
    for (const QString& row : std::as_const(rows)) {
      qDebug().noquote() << QStringLiteral("%1 variant row: %2").arg(m_log, row);
    }
  }
  showSelected();
}

QJsonObject MaterialVariantsPage::selected() const {
  for (const QJsonValue& value : m_variants) {
    const QJsonObject variant = value.toObject();
    if (variantLabel(text(variant, "printer"), variant.value(QStringLiteral("nozzle"))) == m_current) {
      return variant;
    }
  }
  return {};
}

void MaterialVariantsPage::showSelected() {
  const QJsonObject variant = selected();
  m_form->setVisible(!variant.isEmpty());
  m_remove->setEnabled(!m_readOnly && !variant.isEmpty());
  m_form->setReadOnly(m_readOnly);
  m_form->setValues(variant.value(QStringLiteral("filament")).toObject());
  // What the variant leaves unset is the material's own value.
  m_form->setPlaceholderValues(m_base);
}

void MaterialVariantsPage::addVariant() {
  if (m_readOnly) {
    return;
  }
  const QJsonArray rows = printers ? printers() : QJsonArray();
  QMenu menu(this);
  QStringList entries;
  for (const QJsonValue& value : rows) {
    const QJsonObject row = value.toObject();
    const QString printer = text(row, "variant_printer");
    if (printer.isEmpty()) {
      continue;
    }
    const QJsonValue nozzle = row.value(QStringLiteral("nozzle"));
    const QString label = variantLabel(printer, nozzle);
    bool has = false;
    for (const QJsonValue& own : std::as_const(m_variants)) {
      const QJsonObject variant = own.toObject();
      has = has || (text(variant, "printer").compare(printer, Qt::CaseInsensitive) == 0 &&
                    sameNozzle(variant.value(QStringLiteral("nozzle")), nozzle));
    }
    // The profile's name, and the variant it makes when that says more.
    const QString name = text(row, "name");
    const QString entry = has ? tr("%1 (has a variant)").arg(name) : name;
    QAction* action = menu.addAction(entry);
    action->setEnabled(!has);
    action->setToolTip(label);
    action->setData(QJsonObject{{QStringLiteral("printer"), printer}, {QStringLiteral("nozzle"), nozzle}});
    entries << entry;
  }
  if (entries.isEmpty()) {
    QAction* none = menu.addAction(tr("No printer profiles"));
    none->setEnabled(false);
    entries << none->text();
  }
  qDebug().noquote() << QStringLiteral("%1 variant add menu: %2").arg(m_log, entries.join(QStringLiteral(" | ")));
  QAction* chosen = execMenu(&menu, m_add->mapToGlobal(QPoint(0, m_add->height())));
  if (chosen == nullptr || !chosen->data().isValid()) {
    return;
  }
  const QJsonObject made = chosen->data().toJsonObject();
  QJsonObject change{{QStringLiteral("printer"), made.value(QStringLiteral("printer"))},
                     {QStringLiteral("filament"), QJsonObject()}};
  if (!made.value(QStringLiteral("nozzle")).isNull()) {
    change.insert(QStringLiteral("nozzle"), made.value(QStringLiteral("nozzle")));
  }
  // The new one selected once the material is shown again.
  m_current = variantLabel(text(made, "printer"), made.value(QStringLiteral("nozzle")));
  emit changeRequested(change, QStringLiteral("variant %1 added").arg(m_current));
}

void MaterialVariantsPage::removeVariant() {
  const QJsonObject variant = selected();
  if (m_readOnly || variant.isEmpty()) {
    return;
  }
  QJsonObject change{{QStringLiteral("printer"), variant.value(QStringLiteral("printer"))},
                     {QStringLiteral("remove"), true}};
  if (!variant.value(QStringLiteral("nozzle")).isNull() && !variant.value(QStringLiteral("nozzle")).isUndefined()) {
    change.insert(QStringLiteral("nozzle"), variant.value(QStringLiteral("nozzle")));
  }
  const QString label = m_current;
  m_current.clear();
  emit changeRequested(change, QStringLiteral("variant %1 removed").arg(label));
}

QStringList MaterialVariantsPage::places(const std::function<QString(QWidget*, const QPoint&)>& at) const {
  QStringList lines;
  if (!isVisible()) {
    return lines;
  }
  for (int i = 0; i < m_list->topLevelItemCount(); ++i) {
    QTreeWidgetItem* row = m_list->topLevelItem(i);
    const QRect rect = m_list->visualItemRect(row);
    if (m_list->isVisible() && rect.isValid() && m_list->viewport()->rect().contains(rect.center())) {
      lines << QStringLiteral("%1 variant item %2 at %3")
                   .arg(m_log, row->data(0, kLabelRole).toString(),
                        at(m_list->viewport(), QPoint(rect.left() + 40, rect.center().y())));
    }
  }
  const std::pair<const char*, QPushButton*> buttons[] = {{"add", m_add}, {"remove", m_remove}};
  for (const auto& [name, button] : buttons) {
    if (button->isVisible()) {
      lines << QStringLiteral("%1 variant %2 at %3").arg(m_log, QLatin1String(name), at(button, button->rect().center()));
    }
  }
  if (m_form->isVisible()) {
    lines << m_form->places(m_log + QStringLiteral(" variant settings"), at);
  }
  return lines;
}

} // namespace mitcad
