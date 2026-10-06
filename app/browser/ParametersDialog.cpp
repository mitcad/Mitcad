// SPDX-License-Identifier: MIT
#include "ParametersDialog.hpp"

#include <algorithm>
#include <cmath>
#include <functional>
#include <iterator>
#include <utility>

#include <QComboBox>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QJsonArray>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QTimer>
#include <QTreeWidget>
#include <QTreeWidgetItemIterator>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/Icons.hpp"
#include "../framework/Numbers.hpp"
#include "../framework/Theme.hpp"
#include "DocumentHost.hpp"

namespace mitcad {
namespace {

const QColor kRefusedText(0xb0, 0x00, 0x20);
const QColor kRefusedFill(0xfb, 0xe3, 0xe3);
constexpr int kNameRole = Qt::UserRole;     // the parameter's name (rows of parameters)
constexpr int kKindRole = Qt::UserRole + 1; // user or model

QString refusedKey(const QString& name, int column) {
  return QStringLiteral("%1|%2").arg(name).arg(column);
}

// A bare number gets the parameter's unit ("30" in a
// millimetre parameter is "30 mm"), and a decimal point ("2,5" is
// "2.5 mm"; the model writes other expressions' decimal commas as points).
QString withUnit(const QString& expression, const QString& unit) {
  QString number;
  if (!parsePlainNumber(expression, &number)) {
    return expression;
  }
  return unit.isEmpty() ? number : number + QLatin1Char(' ') + unit;
}

// Add User Parameter: name, unit, expression and comment; stays open with
// the model's reason when the model refuses them.
class AddParameterDialog : public QDialog {
public:
  AddParameterDialog(DocumentHost& host, QWidget* parent) : QDialog(parent), m_host(host) {
    setWindowTitle(tr("Add User Parameter"));
    auto* form = new QFormLayout(this);
    m_name = new QLineEdit;
    m_unit = new QComboBox;
    m_unit->setEditable(true);
    m_unit->addItems({QStringLiteral("mm"), QStringLiteral("cm"), QStringLiteral("m"),
                      QStringLiteral("in"), QStringLiteral("ft"), QStringLiteral("deg"),
                      QStringLiteral("rad"), QString()});
    m_unit->setItemText(m_unit->count() - 1, QString());
    m_unit->setToolTip(tr("Empty for a number without a unit"));
    m_expression = new QLineEdit;
    m_comment = new QLineEdit;
    m_error = new QLabel;
    m_error->setWordWrap(true);
    setErrorStyleSheet(m_error);
    form->addRow(tr("Name"), m_name);
    form->addRow(tr("Unit"), m_unit);
    form->addRow(tr("Expression"), m_expression);
    form->addRow(tr("Comment"), m_comment);
    form->addRow(m_error);
    auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
    form->addRow(buttons);
    connect(buttons, &QDialogButtonBox::accepted, this, &AddParameterDialog::add);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    m_name->setFocus();
  }

  QString unit() const { return m_unit->currentText(); }

private:
  void add() {
    const QString name = m_name->text().trimmed();
    const QString unit = m_unit->currentText().trimmed();
    const QString expression = withUnit(m_expression->text().trimmed(), unit);
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("add_parameter")},
                        {QStringLiteral("name"), name},
                        {QStringLiteral("expression"), expression},
                        {QStringLiteral("unit"), unit},
                        {QStringLiteral("comment"), m_comment->text()}};
    QString problem;
    if (name.isEmpty() || expression.isEmpty()) {
      problem = tr("Give a name and an expression.");
    } else if (m_host.runModelCommand(command)) {
      qDebug().noquote() << QStringLiteral("Added parameter %1 = %2%3")
                                .arg(name, expression,
                                     unit.isEmpty() ? QString() : QStringLiteral(" (%1)").arg(unit));
      accept();
      return;
    } else {
      problem = m_host.lastError();
    }
    m_error->setText(problem);
    qDebug().noquote() << QStringLiteral("Add parameter %1 refused: %2").arg(name, problem);
  }

  DocumentHost& m_host;
  QLineEdit* m_name;
  QComboBox* m_unit;
  QLineEdit* m_expression;
  QLineEdit* m_comment;
  QLabel* m_error;
};

// F2 edits the current cell, as double-click does.
class ParameterTable : public QTreeWidget {
public:
  explicit ParameterTable(std::function<void(QTreeWidgetItem*, int)> edit) : m_edit(std::move(edit)) {}

protected:
  void keyPressEvent(QKeyEvent* event) override {
    if (event->key() == Qt::Key_F2 && currentItem() != nullptr &&
        state() != QAbstractItemView::EditingState) {
      m_edit(currentItem(), currentColumn() >= 0 ? currentColumn() : 2);
      return;
    }
    QTreeWidget::keyPressEvent(event);
  }

private:
  std::function<void(QTreeWidgetItem*, int)> m_edit;
};

} // namespace

ParametersDialog::ParametersDialog(DocumentHost& host, QWidget* parent)
    : QDialog(parent), m_host(host) {
  setWindowTitle(tr("Parameters"));
  setWindowIcon(themeIcon(QStringLiteral("parameters")));
  resize(760, 440);
  auto* layout = new QVBoxLayout(this);

  auto* top = new QHBoxLayout;
  m_add = new QPushButton(themeIcon(QStringLiteral("add")), tr("User Parameter"));
  m_add->setToolTip(tr("Add a user parameter"));
  m_delete = new QPushButton(themeIcon(QStringLiteral("delete")), tr("Delete"));
  m_delete->setToolTip(tr("Delete the selected user parameter (when nothing uses it)"));
  m_filter = new QLineEdit;
  m_filter->setPlaceholderText(tr("Search"));
  m_filter->setClearButtonEnabled(true);
  top->addWidget(m_add);
  top->addWidget(m_delete);
  top->addStretch(1);
  top->addWidget(m_filter);
  layout->addLayout(top);

  m_table = new ParameterTable([this](QTreeWidgetItem* item, int column) { edit(item, column); });
  m_table->setColumnCount(6);
  m_table->setHeaderLabels({tr("Name"), tr("Unit"), tr("Expression"), tr("Value"), tr("Comment"), QStringLiteral("★")});
  m_table->headerItem()->setToolTip(kFavorite, tr("Favorites: listed first"));
  m_table->setEditTriggers(QAbstractItemView::NoEditTriggers);
  m_table->setSelectionMode(QAbstractItemView::SingleSelection);
  m_table->setAlternatingRowColors(true);
  m_table->setUniformRowHeights(true);
  m_table->header()->setStretchLastSection(false);
  m_table->header()->resizeSection(kName, 170);
  m_table->header()->resizeSection(kUnit, 60);
  m_table->header()->resizeSection(kExpression, 190);
  m_table->header()->resizeSection(kValue, 110);
  m_table->header()->setSectionResizeMode(kComment, QHeaderView::Stretch);
  m_table->header()->resizeSection(kFavorite, 32);
  layout->addWidget(m_table, 1);

  m_message = new QLabel;
  m_message->setWordWrap(true);
  setErrorStyleSheet(m_message);
  layout->addWidget(m_message);

  auto* buttons = new QHBoxLayout;
  buttons->addStretch(1);
  m_ok = new QPushButton(tr("OK"));
  m_ok->setAutoDefault(false);
  m_add->setAutoDefault(false);
  m_delete->setAutoDefault(false);
  buttons->addWidget(m_ok);
  layout->addLayout(buttons);

  connect(m_ok, &QPushButton::clicked, this, &QDialog::accept);
  connect(m_add, &QPushButton::clicked, this, &ParametersDialog::addParameter);
  connect(m_delete, &QPushButton::clicked, this, &ParametersDialog::deleteParameter);
  connect(m_filter, &QLineEdit::textChanged, this, &ParametersDialog::applyFilter);
  connect(m_table, &QTreeWidget::itemDoubleClicked, this, &ParametersDialog::edit);
  connect(m_table, &QTreeWidget::itemChanged, this, &ParametersDialog::cellEdited);
  connect(m_table, &QTreeWidget::itemSelectionChanged, this, &ParametersDialog::updateButtons);
  refresh();
}

QTreeWidgetItem* ParametersDialog::addRow(QTreeWidgetItem* parent, const QJsonObject& parameter) {
  const QString name = parameter.value(QStringLiteral("name")).toString();
  const bool user = parameter.value(QStringLiteral("kind")).toString() == QStringLiteral("user");
  auto* row = new QTreeWidgetItem(parent);
  row->setText(kName, name);
  row->setText(kUnit, parameter.value(QStringLiteral("unit")).toString());
  row->setText(kExpression, parameter.value(QStringLiteral("expression")).toString());
  row->setText(kValue, parameter.value(QStringLiteral("text")).toString());
  row->setText(kComment, parameter.value(QStringLiteral("comment")).toString());
  row->setData(kName, kNameRole, name);
  row->setData(kName, kKindRole, user ? QStringLiteral("user") : QStringLiteral("model"));
  row->setFlags(row->flags() | Qt::ItemIsEditable | Qt::ItemIsUserCheckable);
  row->setCheckState(kFavorite, parameter.value(QStringLiteral("favorite")).toBool() ? Qt::Checked : Qt::Unchecked);
  row->setToolTip(kFavorite, tr("Favorite"));
  const QStringList uses = parameter.value(QStringLiteral("dependencies")).toVariant().toStringList();
  if (!uses.isEmpty()) {
    row->setToolTip(kExpression, tr("Uses %1").arg(uses.join(QStringLiteral(", "))));
  }
  for (const int column : {kName, kUnit, kExpression, kComment}) {
    const auto refused = m_refused.constFind(refusedKey(name, column));
    if (refused != m_refused.cend()) {
      row->setText(column, refused.value().first);
      row->setForeground(column, kRefusedText);
      row->setBackground(column, kRefusedFill);
      row->setToolTip(column, refused.value().second);
    }
  }
  return row;
}

void ParametersDialog::refresh() {
  QString current;
  if (QTreeWidgetItem* item = m_table->currentItem()) {
    current = item->data(kName, kNameRole).toString();
  }
  m_building = true;
  m_table->clear();
  QJsonArray parameters;
  QHash<QString, QString> featureNames;
  try {
    parameters = m_host.model().queryArray(QStringLiteral("parameters"));
    for (const QJsonValue& value : m_host.model()
                                       .queryObject({{QStringLiteral("query"), QStringLiteral("timeline")}})
                                       .value(QStringLiteral("features"))
                                       .toArray()) {
      const QJsonObject feature = value.toObject();
      featureNames.insert(feature.value(QStringLiteral("uid")).toString(),
                          feature.value(QStringLiteral("name")).toString());
    }
  } catch (const std::exception& e) {
    m_message->setText(QString::fromUtf8(e.what()));
  }
  // Favourites first (P9); they stay in their own
  // groups too.
  auto* favorites = new QTreeWidgetItem(m_table, {tr("Favorites")});
  auto* users = new QTreeWidgetItem(m_table, {tr("User Parameters")});
  auto* models = new QTreeWidgetItem(m_table, {tr("Model Parameters")});
  QHash<QString, QTreeWidgetItem*> owners;
  QStringList names;
  QStringList rows; // for the log
  QStringList favored;
  for (const QJsonValue& value : parameters) {
    const QJsonObject parameter = value.toObject();
    if (parameter.value(QStringLiteral("favorite")).toBool()) {
      favored << parameter.value(QStringLiteral("name")).toString();
      addRow(favorites, parameter);
    }
  }
  if (favored.isEmpty()) {
    delete favorites;
    favorites = nullptr;
  }
  for (const QJsonValue& value : parameters) {
    const QJsonObject parameter = value.toObject();
    names << parameter.value(QStringLiteral("name")).toString();
    rows << QStringLiteral("%1 = %2 (%3)")
                .arg(names.last(), parameter.value(QStringLiteral("expression")).toString(),
                     parameter.value(QStringLiteral("text")).toString());
    QTreeWidgetItem* parent = users;
    if (parameter.value(QStringLiteral("kind")).toString() != QStringLiteral("user")) {
      const QString owner = parameter.value(QStringLiteral("owner")).toString();
      parent = owners.value(owner);
      if (parent == nullptr) {
        parent = new QTreeWidgetItem(models, {featureNames.value(owner, owner)});
        parent->setExpanded(true);
        owners.insert(owner, parent);
      }
    }
    QTreeWidgetItem* row = addRow(parent, parameter);
    if (row->data(kName, kNameRole).toString() == current) {
      m_table->setCurrentItem(row);
    }
  }
  // Refusals of parameters that are gone are forgotten.
  for (auto it = m_refused.begin(); it != m_refused.end();) {
    it = names.contains(it.key().section(QLatin1Char('|'), 0, 0)) ? std::next(it) : m_refused.erase(it);
  }
  users->setExpanded(true);
  models->setExpanded(true);
  for (QTreeWidgetItem* group : {favorites, users, models}) {
    if (group == nullptr) {
      continue;
    }
    group->setExpanded(true);
    QFont font = group->font(kName);
    font.setBold(true);
    group->setFont(kName, font);
    group->setFirstColumnSpanned(true);
  }
  m_building = false;
  applyFilter();
  updateButtons();
  const QString logged = rows.join(QStringLiteral("; "));
  if (logged != m_loggedRows && isVisible()) {
    m_loggedRows = logged;
    qDebug().noquote() << QStringLiteral("Parameters: %1").arg(logged);
  }
  const QString favoriteNames = favored.join(QStringLiteral(", "));
  if (favoriteNames != m_loggedFavorites && isVisible()) {
    m_loggedFavorites = favoriteNames;
    qDebug().noquote() << QStringLiteral("Parameters favorites: %1")
                              .arg(favoriteNames.isEmpty() ? QStringLiteral("none") : favoriteNames);
  }
  QTimer::singleShot(0, this, &ParametersDialog::logLayout);
}

void ParametersDialog::edit(QTreeWidgetItem* item, int column) {
  if (item == nullptr || item->data(kName, kNameRole).toString().isEmpty() || column == kValue ||
      column == kFavorite) {
    return;
  }
  // A model parameter's unit is its feature's.
  if (column == kUnit && item->data(kName, kKindRole).toString() != QStringLiteral("user")) {
    return;
  }
  m_table->editItem(item, column);
  qDebug().noquote() << QStringLiteral("Editing parameter %1 %2")
                            .arg(item->data(kName, kNameRole).toString(),
                                 m_table->headerItem()->text(column).toLower());
}

void ParametersDialog::cellEdited(QTreeWidgetItem* item, int column) {
  const QString name = item->data(kName, kNameRole).toString();
  if (m_building || name.isEmpty() || column == kValue) {
    return;
  }
  QString text = item->text(column).trimmed();
  QJsonObject command{{QStringLiteral("name"), name}};
  switch (column) {
  case kFavorite: {
    // The star (P9); the model makes no undo step when nothing changes.
    const bool on = item->checkState(kFavorite) == Qt::Checked;
    command.insert(QStringLiteral("cmd"), QStringLiteral("set_parameter"));
    command.insert(QStringLiteral("favorite"), on);
    text = on ? QStringLiteral("on") : QStringLiteral("off");
    break;
  }
  case kName:
    if (text == name) {
      return;
    }
    command.insert(QStringLiteral("cmd"), QStringLiteral("rename_parameter"));
    command.insert(QStringLiteral("new_name"), text);
    break;
  case kUnit:
    command.insert(QStringLiteral("cmd"), QStringLiteral("set_parameter"));
    command.insert(QStringLiteral("unit"), text);
    break;
  case kExpression:
    command.insert(QStringLiteral("cmd"), QStringLiteral("set_parameter"));
    command.insert(QStringLiteral("expression"), withUnit(text, item->text(kUnit)));
    break;
  case kComment:
    command.insert(QStringLiteral("cmd"), QStringLiteral("set_parameter"));
    command.insert(QStringLiteral("comment"), item->text(column));
    break;
  default:
    return;
  }
  const QString what =
      column == kFavorite ? QStringLiteral("favorite") : m_table->headerItem()->text(column).toLower();
  // The model command refreshes the window, this table too: not while the
  // table is still handling its edit.
  QTimer::singleShot(0, this, [this, name, column, text, what, command] {
    const CommandContext& model = m_host.model();
    // The window logs what the change did to the bodies.
    if (m_host.runModelCommand(command)) {
      m_refused.remove(refusedKey(name, column));
      m_message->clear();
      QString value;
      const QString shown = column == kName ? text : name;
      for (const QJsonValue& parameter : model.queryArray(QStringLiteral("parameters"))) {
        if (parameter.toObject().value(QStringLiteral("name")).toString() == shown) {
          value = parameter.toObject().value(QStringLiteral("text")).toString();
        }
      }
      qDebug().noquote() << QStringLiteral("Parameter %1 %2: %3 = %4").arg(name, what, text, value);
    } else if (m_host.lastCancelled()) {
      // Cancelled while it computed (P7): no refusal, and the cell shows
      // the parameter as it still is.
      m_refused.remove(refusedKey(name, column));
      m_message->setText(tr("%1: the change was cancelled.").arg(name));
      qDebug().noquote() << QStringLiteral("Parameter %1 %2: %3 cancelled").arg(name, what, text);
    } else {
      // The model's reason stays with the cell; the document is unchanged.
      const QString problem = m_host.lastError();
      m_refused.insert(refusedKey(name, column), {text, problem});
      m_message->setText(QStringLiteral("%1: %2").arg(name, problem));
      qDebug().noquote() << QStringLiteral("Parameter %1 %2: %3 refused: %4").arg(name, what, text, problem);
    }
    refresh();
  });
}

void ParametersDialog::addParameter() {
  AddParameterDialog dialog(m_host, this);
  qDebug().noquote() << QStringLiteral("Add User Parameter dialog opened: unit %1").arg(dialog.unit());
  prepareModal(&dialog);
  dialog.exec();
  refresh();
}

void ParametersDialog::deleteParameter() {
  QTreeWidgetItem* item = m_table->currentItem();
  if (item == nullptr || item->data(kName, kKindRole).toString() != QStringLiteral("user")) {
    return;
  }
  const QString name = item->data(kName, kNameRole).toString();
  if (m_host.runModelCommand({{QStringLiteral("cmd"), QStringLiteral("delete_parameter")},
                              {QStringLiteral("name"), name}})) {
    m_message->clear();
    qDebug().noquote() << QStringLiteral("Deleted parameter %1").arg(name);
  } else {
    m_message->setText(QStringLiteral("%1: %2").arg(name, m_host.lastError()));
    qDebug().noquote() << QStringLiteral("Delete parameter %1 refused: %2").arg(name, m_host.lastError());
  }
  refresh();
}

void ParametersDialog::updateButtons() {
  QTreeWidgetItem* item = m_table->currentItem();
  m_delete->setEnabled(item != nullptr &&
                       item->data(kName, kKindRole).toString() == QStringLiteral("user"));
}

void ParametersDialog::applyFilter() {
  const QString filter = m_filter->text().trimmed();
  for (QTreeWidgetItemIterator it(m_table); *it != nullptr; ++it) {
    QTreeWidgetItem* item = *it;
    if (item->data(kName, kNameRole).toString().isEmpty()) {
      continue; // a group
    }
    const bool shown = filter.isEmpty() || item->text(kName).contains(filter, Qt::CaseInsensitive) ||
                       item->text(kComment).contains(filter, Qt::CaseInsensitive);
    item->setHidden(!shown);
  }
}

void ParametersDialog::logLayout() {
  if (!isVisible() || parentWidget() == nullptr) {
    return;
  }
  // In the main window's coordinates, as the other logged places.
  const QPoint origin = parentWidget()->window()->mapToGlobal(QPoint(0, 0));
  const auto at = [&origin](QWidget* widget, const QPoint& point) {
    const QPoint global = widget->mapToGlobal(point) - origin;
    return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
  };
  QStringList lines;
  for (QTreeWidgetItemIterator it(m_table); *it != nullptr; ++it) {
    QTreeWidgetItem* item = *it;
    const QString name = item->data(kName, kNameRole).toString();
    if (name.isEmpty() || item->isHidden()) {
      continue;
    }
    for (const int column : {kName, kUnit, kExpression, kComment, kFavorite}) {
      const QRect rect = m_table->visualItemRect(item);
      const QRect cell(m_table->header()->sectionViewportPosition(column), rect.top(),
                       m_table->header()->sectionSize(column), rect.height());
      // The favourite's check box sits at the cell's left.
      const QPoint point = column == kFavorite ? QPoint(cell.left() + 10, cell.center().y()) : cell.center();
      lines << QStringLiteral("Parameters %1 %2 at %3")
                   .arg(name,
                        column == kFavorite ? QStringLiteral("favorite")
                                            : m_table->headerItem()->text(column).toLower(),
                        at(m_table->viewport(), point));
    }
  }
  lines << QStringLiteral("Parameters add at %1").arg(at(m_add, m_add->rect().center()));
  lines << QStringLiteral("Parameters delete at %1").arg(at(m_delete, m_delete->rect().center()));
  lines << QStringLiteral("Parameters OK at %1").arg(at(m_ok, m_ok->rect().center()));
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_logged) {
    m_logged = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

} // namespace mitcad
