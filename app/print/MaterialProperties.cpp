// SPDX-License-Identifier: MIT
#include "MaterialProperties.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <optional>
#include <utility>

#include <QDialogButtonBox>
#include <QFont>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QJsonValue>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QRect>
#include <QShowEvent>
#include <QTreeWidget>
#include <QTreeWidgetItem>
#include <QTreeWidgetItemIterator>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/AppSettings.hpp"
#include "../framework/Command.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Json.hpp"
#include "../framework/Numbers.hpp"
#include "../framework/PanelTree.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../platform/MacDialogs.hpp"
#include "PrintLibrary.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad {
namespace {

QString text(const QJsonObject& object, const char* field) {
  return object.value(QLatin1String(field)).toString();
}

// The window everything is logged against: the main window, the topmost
// owner of `widget`'s window.
QWidget* mainWindowOf(QWidget* widget) {
  QWidget* top = widget->window();
  while (top->parentWidget() != nullptr) {
    top = top->parentWidget()->window();
  }
  return top;
}

// A class's title for the dataset's list.
QString classTitle(const QString& id) {
  if (id == QStringLiteral("metal")) {
    return QObject::tr("Metal");
  }
  if (id == QStringLiteral("plastic")) {
    return QObject::tr("General Plastic");
  }
  if (id == QStringLiteral("fdm_plastic")) {
    return QObject::tr("FDM Plastic");
  }
  if (id == QStringLiteral("resin")) {
    return QObject::tr("Resin");
  }
  if (id == QStringLiteral("wood")) {
    return QObject::tr("Wood");
  }
  return QObject::tr("Other");
}

} // namespace

MaterialUnit MaterialUnits::unit(const QString& quantity) const {
  return quantities.value(quantity, MaterialUnit{});
}

MaterialUnits MaterialUnits::fromAnswer(const QJsonObject& answer) {
  MaterialUnits units;
  units.system = answer.value(QStringLiteral("units")).toString(units.system);
  units.temperature = answer.value(QStringLiteral("temperature")).toString(units.temperature);
  const QJsonObject quantities = answer.value(QStringLiteral("quantities")).toObject();
  for (auto it = quantities.begin(); it != quantities.end(); ++it) {
    const QJsonObject unit = it.value().toObject();
    MaterialUnit made;
    made.symbol = text(unit, "symbol");
    made.scale = unit.value(QStringLiteral("scale")).toDouble(1.0);
    made.offset = unit.value(QStringLiteral("offset")).toDouble(0.0);
    if (made.scale == 0.0) {
      made.scale = 1.0;
    }
    units.quantities.insert(it.key(), made);
  }
  units.properties = answer.value(QStringLiteral("properties")).toArray();
  units.printing = answer.value(QStringLiteral("printing")).toArray();
  return units;
}

MaterialUnits MaterialUnits::ask(const CommandContext& model) {
  try {
    return fromAnswer(model.queryObject({{QStringLiteral("query"), QStringLiteral("material_units")},
                                         {QStringLiteral("temperature"), preferredTemperatureUnit()}}));
  } catch (const std::exception& e) {
    qWarning().noquote() << "Material units:" << e.what();
    return MaterialUnits{};
  }
}

QString preferredTemperatureUnit() { return GeneralSettings::load().temperatureUnit; }

void applyTemperaturePreference() {
  const QString unit = preferredTemperatureUnit();
  try {
    set_temperature_unit(rustStr(unit.toUtf8()));
  } catch (const std::exception& e) {
    qWarning().noquote() << "Temperature unit:" << e.what();
  }
}

QString materialValueText(double value) {
  if (!std::isfinite(value)) {
    return QString();
  }
  if (value == 0.0) {
    return QStringLiteral("0");
  }
  const int magnitude = static_cast<int>(std::floor(std::log10(std::abs(value))));
  const int decimals = std::max(0, 5 - magnitude);
  double rounded = value;
  if (magnitude >= 6) {
    const double step = std::pow(10.0, magnitude - 5);
    rounded = std::round(value / step) * step;
  }
  QString shown = QString::number(rounded, 'f', decimals);
  while (shown.contains(QLatin1Char('.')) && (shown.endsWith(QLatin1Char('0')) || shown.endsWith(QLatin1Char('.')))) {
    shown.chop(1);
  }
  return shown == QStringLiteral("-0") ? QStringLiteral("0") : shown;
}

QString materialReferenceText(const QJsonObject& fields) {
  const QJsonObject reference = fields.value(QStringLiteral("reference")).toObject();
  QStringList parts;
  for (const char* field : {"source", "material", "grade"}) {
    QString part = text(reference, field).trimmed();
    if (!part.isEmpty()) {
      parts << part.replace(QLatin1Char('_'), QLatin1Char(' '));
    }
  }
  return parts.join(QStringLiteral(", "));
}

MaterialPropertiesBox::MaterialPropertiesBox(QWidget* parent) : QWidget(parent) {
  setObjectName(QStringLiteral("materialProperties"));
  auto* box = new QVBoxLayout(this);
  box->setContentsMargins(0, 0, 0, 0);
  m_rows = new QWidget;
  box->addWidget(m_rows);
  // Where the values came from, and that they are typical (mitcad#1213).
  m_reference = new QLabel;
  m_reference->setObjectName(QStringLiteral("materialReference"));
  m_reference->setWordWrap(true);
  m_reference->setTextInteractionFlags(Qt::TextSelectableByMouse);
  box->addWidget(m_reference);
  m_caution = new QLabel(tr("Values are typical: check them against the supplier's data."));
  m_caution->setObjectName(QStringLiteral("materialCaution"));
  m_caution->setWordWrap(true);
  setRoleStyleSheet(m_caution, ColorRole::TextSecondary);
  box->addWidget(m_caution);
  m_error = new QLabel;
  m_error->setWordWrap(true);
  setErrorStyleSheet(m_error);
  m_error->hide();
  box->addWidget(m_error);
  rebuild();
}

void MaterialPropertiesBox::setUnits(const MaterialUnits& units) {
  const bool same = units.system == m_units.system && units.temperature == m_units.temperature &&
                    units.properties == m_units.properties && units.printing == m_units.printing &&
                    !m_fields.isEmpty();
  m_units = units;
  if (!same) {
    rebuild();
  }
  setMaterial(m_fieldsShown, m_editable, m_printing);
}

void MaterialPropertiesBox::rebuild() {
  // The rows again: the groups and their fields, each with its unit.
  delete m_rows->layout();
  for (QWidget* child : m_rows->findChildren<QWidget*>(QString(), Qt::FindDirectChildrenOnly)) {
    delete child;
  }
  m_fields.clear();
  m_groups.clear();
  auto* form = new QFormLayout(m_rows);
  form->setContentsMargins(0, 8, 0, 0);
  const auto heading = [this, &form](const QString& title) {
    auto* label = new QLabel(title);
    QFont bold = label->font();
    bold.setBold(true);
    label->setFont(bold);
    form->addRow(label);
    m_groups.append(std::pair<QLabel*, QVector<int>>(label, QVector<int>()));
  };
  const auto addField = [this, &form](const QJsonObject& entry, bool printing) {
    Field field;
    field.key = text(entry, "key");
    field.name = text(entry, "name");
    field.quantity = printing ? QStringLiteral("temperature") : text(entry, "quantity");
    field.printing = printing;
    auto* row = new QHBoxLayout;
    field.edit = new QLineEdit;
    field.edit->setObjectName(QStringLiteral("materialProperty_") + field.key);
    field.edit->setAccessibleName(field.name);
    // The same widths as Density's and the settings form's fields.
    field.edit->setFixedWidth(150);
    field.edit->setPlaceholderText(tr("Not given"));
    row->addWidget(field.edit);
    field.unit = new QLabel(m_units.unit(field.quantity).symbol);
    field.unit->setFixedWidth(80);
    row->addWidget(field.unit);
    row->addStretch(1);
    field.label = new QLabel(field.name);
    form->addRow(field.label, row);
    const int index = static_cast<int>(m_fields.size());
    connect(field.edit, &QLineEdit::editingFinished, this, [this, index] { finish(index); });
    m_fields.append(field);
    if (!m_groups.isEmpty()) {
      m_groups.last().second.append(index);
    }
  };
  QString group;
  for (const QJsonValue& value : std::as_const(m_units.properties)) {
    const QJsonObject entry = value.toObject();
    if (text(entry, "group") != group) {
      group = text(entry, "group");
      heading(text(entry, "group_title"));
    }
    addField(entry, false);
  }
  if (!m_units.printing.isEmpty()) {
    heading(tr("Printing"));
    for (const QJsonValue& value : std::as_const(m_units.printing)) {
      addField(value.toObject(), true);
    }
  }
}

void MaterialPropertiesBox::setSearch(const QString& search) {
  if (search == m_search) {
    return;
  }
  m_search = search;
  applyVisibility();
}

void MaterialPropertiesBox::applyVisibility() {
  bool anyShown = false;
  for (Field& field : m_fields) {
    const bool shown = (!field.printing || m_printing) &&
                       (m_search.isEmpty() || field.name.contains(m_search, Qt::CaseInsensitive));
    field.edit->setVisible(shown);
    field.unit->setVisible(shown);
    field.label->setVisible(shown);
    anyShown = anyShown || shown;
  }
  for (const auto& [label, indices] : std::as_const(m_groups)) {
    bool any = false;
    for (int index : indices) {
      any = any || m_fields[index].label->isVisibleTo(this);
    }
    label->setVisible(any);
  }
  // Where the values came from and that they are typical: with the values.
  const bool lines = m_search.isEmpty() || anyShown;
  m_reference->setVisible(lines && !m_reference->text().isEmpty());
  m_caution->setVisible(lines);
}

void MaterialPropertiesBox::setMaterial(const QJsonObject& fields, bool editable, bool printing) {
  m_fieldsShown = fields;
  m_editable = editable;
  m_printing = printing;
  const QJsonObject properties = fields.value(QStringLiteral("properties")).toObject();
  const QJsonObject filament = fields.value(QStringLiteral("filament")).toObject();
  for (Field& field : m_fields) {
    const QJsonValue value = field.printing ? filament.value(field.key) : properties.value(field.key);
    field.given = value.isDouble();
    field.shown = field.given ? materialValueText(m_units.unit(field.quantity).shown(value.toDouble())) : QString();
    if (!field.edit->hasFocus() || !field.edit->isModified()) {
      field.edit->setText(field.shown);
      field.edit->setModified(false);
    }
    field.edit->setReadOnly(!editable);
    field.unit->setText(m_units.unit(field.quantity).symbol);
  }
  const QString reference = materialReferenceText(fields);
  m_reference->setText(reference.isEmpty() ? QString() : tr("Source: %1").arg(reference));
  applyVisibility();
  m_error->hide();
}

void MaterialPropertiesBox::setError(const QString& message) {
  m_error->setText(message);
  m_error->setVisible(!message.isEmpty());
}

void MaterialPropertiesBox::finish(int index) {
  if (index < 0 || index >= m_fields.size() || !m_editable) {
    return;
  }
  Field& field = m_fields[index];
  const QString typed = field.edit->text().trimmed();
  if (typed == field.shown) {
    return;
  }
  const QString setName = field.printing ? QStringLiteral("filament") : QStringLiteral("properties");
  if (typed.isEmpty()) {
    // An emptied field: the value is not given (null takes it away).
    if (field.given) {
      emit changeRequested({{setName, QJsonObject{{field.key, QJsonValue()}}}},
                           QStringLiteral("%1 = not given").arg(field.key));
    }
    return;
  }
  const std::optional<double> number = parsePlainNumber(typed);
  if (!number) {
    setError(tr("%1: use a number, or leave it empty when it is not given.").arg(field.name));
    field.edit->setText(field.shown);
    return;
  }
  m_error->hide();
  const double model = m_units.unit(field.quantity).model(*number);
  emit changeRequested({{setName, QJsonObject{{field.key, model}}}},
                       QStringLiteral("%1 = %2 %3").arg(field.key, materialValueText(*number),
                                                        m_units.unit(field.quantity).symbol)
                           .trimmed());
}

QString MaterialPropertiesBox::shownText() const {
  QStringList parts;
  for (const Field& field : m_fields) {
    if (field.printing && !m_printing) {
      continue;
    }
    const QString symbol = m_units.unit(field.quantity).symbol;
    parts << (field.given ? QStringLiteral("%1 %2 %3").arg(field.name, field.shown, symbol).trimmed()
                          : QStringLiteral("%1 not given").arg(field.name));
  }
  return parts.join(QStringLiteral(" | "));
}

QStringList MaterialPropertiesBox::places(const QString& prefix,
                                          const std::function<QString(QWidget*, const QPoint&)>& at) const {
  QStringList lines;
  for (const Field& field : m_fields) {
    if (field.edit->isVisibleTo(window())) {
      lines << QStringLiteral("%1 property %2 at %3").arg(prefix, field.key, at(field.edit, field.edit->rect().center()));
    }
  }
  return lines;
}

MaterialDatasetDialog::MaterialDatasetDialog(const CommandContext& model, const MaterialUnits& units, QWidget* parent)
    : QDialog(parent), m_model(model), m_units(units) {
  setWindowTitle(tr("Material Dataset"));
  setObjectName(QStringLiteral("materialDataset"));
  resize(720, 520);
  auto* layout = new QVBoxLayout(this);
  m_search = new QLineEdit;
  m_search->setObjectName(QStringLiteral("materialDatasetSearch"));
  m_search->setPlaceholderText(tr("Search materials and grades"));
  m_search->setAccessibleName(tr("Search"));
  m_search->setClearButtonEnabled(true);
  layout->addWidget(m_search);
  auto* middle = new QHBoxLayout;
  m_list = new QTreeWidget;
  m_list->setObjectName(QStringLiteral("materialDatasetList"));
  m_list->setAccessibleName(tr("Materials of the dataset"));
  m_list->setColumnCount(2);
  m_list->setHeaderLabels({tr("Material"), tr("Class")});
  m_list->setRootIsDecorated(false);
  panel::setCompactIndentation(m_list, panel::Indent::Flat);
  m_list->setSelectionMode(QAbstractItemView::SingleSelection);
  m_list->header()->setSectionResizeMode(0, QHeaderView::Stretch);
  m_list->header()->setSectionResizeMode(1, QHeaderView::ResizeToContents);
  m_list->header()->setStretchLastSection(false);
  styleItemList(m_list);
  middle->addWidget(m_list, 3);
  m_details = new QLabel;
  m_details->setObjectName(QStringLiteral("materialDatasetDetails"));
  m_details->setWordWrap(true);
  m_details->setAlignment(Qt::AlignTop | Qt::AlignLeft);
  m_details->setTextInteractionFlags(Qt::TextSelectableByMouse);
  m_details->setMinimumWidth(240);
  middle->addWidget(m_details, 2);
  layout->addLayout(middle, 1);
  // Where the values come from, and that they are typical (mitcad#1213):
  // the dataset's sources and their licences as the query gives them (the
  // third-party dataset and MitCad's woods and panels, mitcad#1552).
  auto* source = new QLabel;
  source->setObjectName(QStringLiteral("materialDatasetSource"));
  source->setWordWrap(true);
  setRoleStyleSheet(source, ColorRole::TextSecondary);
  layout->addWidget(source);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Cancel);
  m_create = buttons->addButton(tr("Create"), QDialogButtonBox::AcceptRole);
  m_create->setObjectName(QStringLiteral("materialDatasetCreate"));
  m_create->setToolTip(tr("Make a material of your own with these values"));
  m_create->setEnabled(false);
  layout->addWidget(buttons);
  // Made the default once the box is in the dialog: before it, the dialog
  // does not record it and makes Cancel, the first button, its default on
  // show, so Enter cancelled.
  m_create->setDefault(true);
  mac::styleDialog(this);
  connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
  connect(m_search, &QLineEdit::textChanged, this, &MaterialDatasetDialog::search);
  connect(m_list, &QTreeWidget::currentItemChanged, this, [this] { showSelected(); });
  connect(m_list, &QTreeWidget::itemDoubleClicked, this, [this] {
    if (!chosenId().isEmpty()) {
      accept();
    }
  });
  search(QString());
  QStringList sources;
  QStringList logged;
  const QJsonArray listed = m_about.value(QStringLiteral("sources")).toArray();
  for (const QJsonValue& value : listed) {
    const QJsonObject named = value.toObject();
    sources << tr("the %1 (%2)").arg(text(named, "name"), text(named, "license"));
    logged << QStringLiteral("%1 (%2)").arg(text(named, "name"), text(named, "license"));
  }
  if (sources.isEmpty()) {
    sources << tr("the %1 (%2)").arg(m_about.value(QStringLiteral("name")).toString(QStringLiteral("material dataset")),
                                     m_about.value(QStringLiteral("license")).toString(QStringLiteral("Apache-2.0")));
  }
  QString from = sources.takeLast();
  if (!sources.isEmpty()) {
    from = tr("%1 and %2").arg(sources.join(QStringLiteral(", ")), from);
  }
  source->setText(tr("Values from %1, converted into the design's units. Values are typical: "
                     "check them against the supplier's data.")
                      .arg(from));
  qDebug().noquote() << QStringLiteral("Material dataset opened: %1 materials").arg(m_entries.size());
  qDebug().noquote() << QStringLiteral("Material dataset sources: %1").arg(logged.join(QStringLiteral(" | ")));
}

QString MaterialDatasetDialog::chosenId() const {
  const QTreeWidgetItem* row = m_list->currentItem();
  return row != nullptr ? row->data(0, Qt::UserRole).toString() : QString();
}

QString MaterialDatasetDialog::chosenName() const {
  const QTreeWidgetItem* row = m_list->currentItem();
  return row != nullptr ? row->text(0) : QString();
}

void MaterialDatasetDialog::search(const QString& wanted) {
  try {
    m_about = m_model.queryObject(
        {{QStringLiteral("query"), QStringLiteral("material_dataset")}, {QStringLiteral("search"), wanted}});
    m_entries = m_about.value(QStringLiteral("materials")).toArray();
  } catch (const std::exception& e) {
    m_entries = QJsonArray();
    m_details->setText(QString::fromUtf8(e.what()));
  }
  const QString selected = chosenId();
  m_list->clear();
  QTreeWidgetItem* keep = nullptr;
  for (const QJsonValue& value : std::as_const(m_entries)) {
    const QJsonObject entry = value.toObject();
    auto* row = new QTreeWidgetItem(m_list, {text(entry, "name"), classTitle(text(entry, "class"))});
    row->setData(0, Qt::UserRole, text(entry, "id"));
    row->setToolTip(0, text(entry, "name"));
    if (text(entry, "id") == selected) {
      keep = row;
    }
  }
  m_list->setCurrentItem(keep);
  showSelected();
  if (!wanted.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Material dataset search: %1, %2 shown").arg(wanted).arg(m_entries.size());
    // Each row found with its class as the list shows it (mitcad#1552).
    for (int i = 0; i < m_list->topLevelItemCount(); ++i) {
      const QTreeWidgetItem* row = m_list->topLevelItem(i);
      qDebug().noquote() << QStringLiteral("Material dataset row: %1 | %2").arg(row->text(0), row->text(1));
    }
  }
  TestSync::singleShot(0, this, [this] { logPlaces(); });
}

void MaterialDatasetDialog::showSelected() {
  const QString id = chosenId();
  m_create->setEnabled(!id.isEmpty());
  QJsonObject entry;
  for (const QJsonValue& value : std::as_const(m_entries)) {
    if (text(value.toObject(), "id") == id) {
      entry = value.toObject();
    }
  }
  if (entry.isEmpty()) {
    m_details->setText(tr("Pick a material and grade to see its values."));
    return;
  }
  // Its values in the design's units; one not given says so.
  QStringList lines;
  const auto line = [&lines](const QString& name, const QJsonValue& value, const MaterialUnit& unit) {
    lines << (value.isDouble() ? QStringLiteral("%1: %2 %3").arg(name, materialValueText(unit.shown(value.toDouble())),
                                                                  unit.symbol)
                                     .trimmed()
                               : tr("%1: not given").arg(name));
  };
  line(tr("Density"), entry.value(QStringLiteral("density")), m_units.unit(QStringLiteral("density")));
  const QJsonObject properties = entry.value(QStringLiteral("properties")).toObject();
  for (const QJsonValue& value : std::as_const(m_units.properties)) {
    const QJsonObject property = value.toObject();
    line(text(property, "name"), properties.value(text(property, "key")), m_units.unit(text(property, "quantity")));
  }
  const QJsonObject filament = entry.value(QStringLiteral("filament")).toObject();
  if (!filament.isEmpty()) {
    for (const QJsonValue& value : std::as_const(m_units.printing)) {
      const QJsonObject setting = value.toObject();
      line(text(setting, "name"), filament.value(text(setting, "key")), m_units.unit(QStringLiteral("temperature")));
    }
  }
  const QString reference = materialReferenceText(entry);
  if (!reference.isEmpty()) {
    lines << tr("Source: %1").arg(reference);
  }
  m_details->setText(lines.join(QLatin1Char('\n')));
  const QString logged =
      QStringLiteral("Material dataset selected: %1: %2").arg(text(entry, "name"), lines.join(QStringLiteral(" | ")));
  if (logged != m_loggedSelected) {
    m_loggedSelected = logged;
    qDebug().noquote() << logged;
  }
}

void MaterialDatasetDialog::showEvent(QShowEvent* event) {
  QDialog::showEvent(event);
  m_search->setFocus(Qt::OtherFocusReason);
  TestSync::singleShot(0, this, [this] { logPlaces(); });
}

void MaterialDatasetDialog::logPlaces() {
  if (!isVisible()) {
    return;
  }
  // In the main window's coordinates, as the other logged places.
  const QPoint origin = mainWindowOf(this)->mapToGlobal(QPoint(0, 0));
  const auto at = [&origin](QWidget* widget, const QPoint& point) {
    const QPoint global = widget->mapToGlobal(point) - origin;
    return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
  };
  QStringList lines;
  lines << QStringLiteral("Material dataset search at %1").arg(at(m_search, m_search->rect().center()));
  lines << QStringLiteral("Material dataset create at %1").arg(at(m_create, m_create->rect().center()));
  for (QTreeWidgetItemIterator it(m_list); *it != nullptr; ++it) {
    const QRect rect = m_list->visualItemRect(*it);
    if (rect.isValid() && m_list->viewport()->rect().contains(rect.center())) {
      lines << QStringLiteral("Material dataset item %1 at %2")
                   .arg((*it)->text(0), at(m_list->viewport(), QPoint(40, rect.center().y())));
    }
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_loggedPlaces) {
    m_loggedPlaces = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

} // namespace mitcad
