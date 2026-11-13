// SPDX-License-Identifier: MIT
#include "AnnotationsWindow.hpp"

#include <algorithm>
#include <set>
#include <utility>

#include <QColor>
#include <QComboBox>
#include <QCoreApplication>
#include <QEvent>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QItemSelection>
#include <QItemSelectionModel>
#include <QJsonValue>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QScrollBar>
#include <QSignalBlocker>
#include <QTableWidget>
#include <QTableWidgetItem>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Icons.hpp"
#include "../framework/NoticeLevel.hpp"
#include "../framework/UserText.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../platform/MacDialogs.hpp"
#include "AnnotationDialogs.hpp"
#include "MeasurementDrawing.hpp"

namespace mitcad {
namespace {

enum Column { kName, kKind, kValue, kTolerance, kText, kColour, kGroup, kShown, kColumns };

constexpr int kNameRole = Qt::UserRole + 1;

const std::pair<const char*, const char*> kKinds[] = {
    {"distance", QT_TRANSLATE_NOOP("Annotations", "Distance")},
    {"angle", QT_TRANSLATE_NOOP("Annotations", "Angle")},
    {"offset", QT_TRANSLATE_NOOP("Annotations", "Offset")},
    {"axis_distance", QT_TRANSLATE_NOOP("Annotations", "Axis distance")},
    {"center_distance", QT_TRANSLATE_NOOP("Annotations", "Center distance")},
    {"radius", QT_TRANSLATE_NOOP("Annotations", "Radius")},
    {"diameter", QT_TRANSLATE_NOOP("Annotations", "Diameter")},
    {"note", QT_TRANSLATE_NOOP("Annotations", "Note")},
    {"datum", QT_TRANSLATE_NOOP("Annotations", "Datum label")},
    {"frame", QT_TRANSLATE_NOOP("Annotations", "Feature control frame")},
    {"hole_callout", QT_TRANSLATE_NOOP("Annotations", "Hole callout")},
    {"material", QT_TRANSLATE_NOOP("Annotations", "Material")},
    {"process_note", QT_TRANSLATE_NOOP("Annotations", "Process note")}};

// A note, a datum label, a feature control frame, a hole callout, a
// material or process note (mitcad#1049): on a leader, without a value of
// its own.
bool isLeader(const QString& kind) {
  return kind == QLatin1String("note") || kind == QLatin1String("datum") || kind == QLatin1String("frame") ||
         kind == QLatin1String("hole_callout") || kind == QLatin1String("material") ||
         kind == QLatin1String("process_note");
}

// What a feature control frame can stand under: a dimension or a hole
// callout (mitcad#902).
bool takesFrame(const QString& kind) { return !isLeader(kind) || kind == QLatin1String("hole_callout"); }

QString kindName(const QString& key) {
  for (const auto& [kind, name] : kKinds) {
    if (key == QLatin1String(kind)) {
      return QCoreApplication::translate("Annotations", name);
    }
  }
  return key;
}

// An annotation's own text on one line: a note's, where a frame stands
// (under a dimension), or a dimension's text before and after its value,
// or the text that replaces it. None of a datum label.
QString ownText(const QJsonObject& a) {
  const QString kind = a.value(QStringLiteral("kind")).toString();
  if (kind == QLatin1String("note")) {
    return a.value(QStringLiteral("text")).toString().replace(QLatin1Char('\n'), QStringLiteral(" / "));
  }
  if (kind == QLatin1String("frame")) {
    const QString on = a.value(QStringLiteral("on")).toString();
    return on.isEmpty() ? QString() : QCoreApplication::translate("Annotations", "Under %1").arg(on);
  }
  if (kind == QLatin1String("datum")) {
    return QString();
  }
  const QJsonObject format = a.value(QStringLiteral("format")).toObject();
  QStringList parts;
  const QString before = format.value(QStringLiteral("before")).toString();
  const QString replace = format.value(QStringLiteral("replace")).toString();
  const QString after = format.value(QStringLiteral("after")).toString();
  if (!before.isEmpty()) {
    parts << before + QStringLiteral(" …");
  }
  if (!replace.isEmpty()) {
    parts << replace;
  }
  if (!after.isEmpty()) {
    parts << QStringLiteral("… ") + after;
  }
  return parts.join(QStringLiteral(", "));
}

// A row's item, not editable.
QTableWidgetItem* cell(const QString& text) {
  auto* item = new QTableWidgetItem(text);
  item->setFlags(item->flags() & ~Qt::ItemIsEditable);
  return item;
}

} // namespace

AnnotationsWindow::AnnotationsWindow(QWidget* parent) : QDialog(parent) {
  setWindowTitle(tr("Annotations"));
  setWindowIcon(themeIcon(QStringLiteral("measure")));
  setObjectName(QStringLiteral("annotationsWindow"));
  resize(760, 420);
  auto* layout = new QVBoxLayout(this);

  // The search and the filters.
  auto* filters = new QHBoxLayout;
  m_search = new QLineEdit(this);
  m_search->setPlaceholderText(tr("Search"));
  m_search->setClearButtonEnabled(true);
  mac::styleSearchField(m_search);
  filters->addWidget(m_search, 1);
  m_kind = new QComboBox(this);
  auto* kindLabel = new QLabel(tr("&Kind:"), this);
  kindLabel->setBuddy(m_kind);
  filters->addWidget(kindLabel);
  filters->addWidget(m_kind);
  m_group = new QComboBox(this);
  auto* groupLabel = new QLabel(tr("&Group:"), this);
  groupLabel->setBuddy(m_group);
  filters->addWidget(groupLabel);
  filters->addWidget(m_group);
  m_showGroup = new QPushButton(themeIcon(QStringLiteral("eye")), tr("Sho&w Group"), this);
  m_hideGroup = new QPushButton(themeIcon(QStringLiteral("eye-off")), tr("Hide Gro&up"), this);
  for (QPushButton* button : {m_showGroup, m_hideGroup}) {
    button->setAutoDefault(false);
    filters->addWidget(button);
  }
  layout->addLayout(filters);

  m_table = new QTableWidget(0, kColumns, this);
  m_table->setObjectName(QStringLiteral("annotationsTable"));
  m_table->setHorizontalHeaderLabels(
      {tr("Name"), tr("Kind"), tr("Value"), tr("Tolerance"), tr("Text"), tr("Color"), tr("Group"), tr("Shown")});
  m_table->setSelectionBehavior(QAbstractItemView::SelectRows);
  m_table->setSelectionMode(QAbstractItemView::ExtendedSelection);
  m_table->setEditTriggers(QAbstractItemView::NoEditTriggers);
  m_table->verticalHeader()->hide();
  m_table->horizontalHeader()->setStretchLastSection(false);
  m_table->horizontalHeader()->setSectionResizeMode(QHeaderView::ResizeToContents);
  m_table->horizontalHeader()->setSectionResizeMode(kText, QHeaderView::Stretch);
  m_table->setSortingEnabled(true);
  m_table->sortByColumn(-1, Qt::AscendingOrder); // the order they were kept until a header is clicked
  layout->addWidget(m_table, 1);

  auto* buttons = new QHBoxLayout;
  m_edit = new QPushButton(tr("&Edit…"), this);
  m_colour = new QPushButton(tr("&Color…"), this);
  m_groupButton = new QPushButton(tr("G&roup…"), this);
  m_decimals = new QPushButton(tr("&Decimals…"), this);
  m_tolerance = new QPushButton(tr("&Tolerance…"), this);
  // A feature control frame under the dimension selected (phase 2).
  m_addFrame = new QPushButton(tr("Add &Frame…"), this);
  m_show = new QPushButton(tr("S&how"), this);
  m_hide = new QPushButton(tr("H&ide"), this);
  m_delete = new QPushButton(themeIcon(QStringLiteral("delete")), tr("De&lete"), this);
  for (QPushButton* button : {m_edit, m_colour, m_groupButton, m_decimals, m_tolerance, m_addFrame, m_show, m_hide, m_delete}) {
    button->setAutoDefault(false);
    buttons->addWidget(button);
  }
  buttons->addStretch(1);
  // The design's texts, not the selected rows' (mitcad#894).
  m_textHeight = new QPushButton(tr("Te&xt Height…"), this);
  m_textHeight->setAutoDefault(false);
  m_textHeight->setToolTip(tr("The height of all the annotations' texts in the design"));
  buttons->addWidget(m_textHeight);
  auto* close = new QPushButton(tr("Close"), this);
  close->setAutoDefault(false);
  buttons->addWidget(close);
  layout->addLayout(buttons);

  connect(m_search, &QLineEdit::textChanged, this, [this] { fill(); });
  connect(m_kind, &QComboBox::currentIndexChanged, this, [this] { fill(); });
  connect(m_group, &QComboBox::currentIndexChanged, this, [this] {
    qDebug().noquote() << QStringLiteral("Annotations group filter: %1").arg(m_group->currentText());
    fill();
  });
  connect(m_table, &QTableWidget::itemSelectionChanged, this, [this] {
    if (!m_filling) {
      followSelection();
    }
  });
  connect(m_table, &QTableWidget::cellDoubleClicked, this, [this](int row, int) {
    const QTableWidgetItem* item = m_table->item(row, kName);
    if (item != nullptr && m_actions.edit) {
      m_actions.edit(item->data(kNameRole).toString());
    }
  });
  // The light bulb column: a click shows or hides the row's annotation.
  connect(m_table, &QTableWidget::itemChanged, this, [this](QTableWidgetItem* item) {
    if (m_filling || item->column() != kShown || !m_actions.change) {
      return;
    }
    const QTableWidgetItem* nameItem = m_table->item(item->row(), kName);
    if (nameItem == nullptr) {
      return;
    }
    m_actions.change({nameItem->data(kNameRole).toString()},
                     {{QStringLiteral("visible"), item->checkState() == Qt::Checked}});
  });
  connect(m_edit, &QPushButton::clicked, this, [this] {
    const QStringList names = selectedNames();
    if (names.size() == 1 && m_actions.edit) {
      m_actions.edit(names.front());
    }
  });
  connect(m_colour, &QPushButton::clicked, this, [this] { chooseColour(); });
  connect(m_groupButton, &QPushButton::clicked, this, [this] { chooseGroup(); });
  connect(m_decimals, &QPushButton::clicked, this, [this] { chooseDecimals(); });
  connect(m_tolerance, &QPushButton::clicked, this, [this] { chooseTolerance(); });
  connect(m_addFrame, &QPushButton::clicked, this, [this] {
    const QStringList names = selectedNames();
    QStringList under;
    for (const QString& name : names) {
      if (takesFrame(annotation(name).value(QStringLiteral("kind")).toString())) {
        under << name;
      }
    }
    qDebug().noquote() << QStringLiteral("Annotations Add Frame clicked: %1").arg(under.join(QStringLiteral(", ")));
    if (under.size() == 1 && m_actions.frame) {
      m_actions.frame(under.front());
    }
  });
  connect(m_show, &QPushButton::clicked, this, [this] { changeSelected({{QStringLiteral("visible"), true}}); });
  connect(m_hide, &QPushButton::clicked, this, [this] { changeSelected({{QStringLiteral("visible"), false}}); });
  connect(m_delete, &QPushButton::clicked, this, [this] {
    const QStringList names = selectedNames();
    qDebug().noquote() << QStringLiteral("Annotations Delete clicked: %1").arg(names.join(QStringLiteral(", ")));
    if (!names.isEmpty() && m_actions.remove) {
      m_actions.remove(names);
    }
  });
  connect(m_textHeight, &QPushButton::clicked, this, [this] {
    qDebug().noquote() << QStringLiteral("Annotations Text Height clicked");
    if (m_actions.textHeight) {
      m_actions.textHeight();
    }
  });
  connect(m_showGroup, &QPushButton::clicked, this, [this] { showGroup(true); });
  connect(m_hideGroup, &QPushButton::clicked, this, [this] { showGroup(false); });
  connect(close, &QPushButton::clicked, this, &QDialog::close);
  followSelection();
}

void AnnotationsWindow::setAnnotations(const QJsonArray& annotations) {
  m_annotations = annotations;
  // The filters' choices: the kinds and the groups there are, the chosen
  // one kept.
  const QString kind = m_kind->currentData().toString();
  const QString group = m_group->currentData().toString();
  std::set<QString> kinds;
  for (const QJsonValue& value : m_annotations) {
    kinds.insert(value.toObject().value(QStringLiteral("kind")).toString());
  }
  {
    const QSignalBlocker blockKind(m_kind);
    const QSignalBlocker blockGroup(m_group);
    m_kind->clear();
    m_kind->addItem(tr("All kinds"), QString());
    for (const auto& [key, name] : kKinds) {
      if (kinds.count(QLatin1String(key)) > 0) {
        m_kind->addItem(QCoreApplication::translate("Annotations", name), QString::fromLatin1(key));
      }
    }
    m_kind->setCurrentIndex(std::max(0, m_kind->findData(kind)));
    m_group->clear();
    m_group->addItem(tr("All groups"), QString());
    m_group->addItem(tr("No group"), QStringLiteral("\n")); // (no group's name has a line break)
    for (const QString& name : groups()) {
      m_group->addItem(name, name);
    }
    m_group->setCurrentIndex(std::max(0, m_group->findData(group)));
  }
  fill();
}

QStringList AnnotationsWindow::groups() const {
  std::set<QString> names;
  for (const QJsonValue& value : m_annotations) {
    const QString group = value.toObject().value(QStringLiteral("group")).toString();
    if (!group.isEmpty()) {
      names.insert(group);
    }
  }
  return QStringList(names.begin(), names.end());
}

QJsonObject AnnotationsWindow::annotation(const QString& name) const {
  for (const QJsonValue& value : m_annotations) {
    if (value.toObject().value(QStringLiteral("name")).toString() == name) {
      return value.toObject();
    }
  }
  return {};
}

void AnnotationsWindow::fill() {
  const QStringList selected = selectedNames();
  const QString search = m_search->text().trimmed();
  const QString kind = m_kind->currentData().toString();
  const QString group = m_group->currentData().toString();
  // Where the list is scrolled, kept through the rebuild (mitcad#759: a
  // light bulb clicked far down did not jump the list to its top).
  const int scrolledDown = m_table->verticalScrollBar()->value();
  const int scrolledRight = m_table->horizontalScrollBar()->value();
  m_filling = true;
  const int sortColumn = m_table->horizontalHeader()->sortIndicatorSection();
  const Qt::SortOrder sortOrder = m_table->horizontalHeader()->sortIndicatorOrder();
  m_table->setSortingEnabled(false);
  m_table->clearContents();
  m_table->setRowCount(0);
  QStringList logged;
  for (const QJsonValue& value : m_annotations) {
    const QJsonObject a = value.toObject();
    const QString name = a.value(QStringLiteral("name")).toString();
    const QString own = a.value(QStringLiteral("kind")).toString();
    const QString ownGroup = a.value(QStringLiteral("group")).toString();
    const QJsonObject label = a.value(QStringLiteral("label")).toObject();
    const QString error = a.value(QStringLiteral("error")).toString();
    const bool shown = a.value(QStringLiteral("visible")).toBool(true);
    const QString valueText = error.isEmpty() ? label.value(QStringLiteral("text")).toString() : tr("Broken");
    const QString tolerance = label.value(QStringLiteral("tolerance")).toString();
    const QString text = ownText(a);
    const QString state = !error.isEmpty() ? QStringLiteral("broken: ") + error
                          : shown          ? QStringLiteral("shown")
                                           : QStringLiteral("hidden");
    logged << QStringLiteral("Annotations row %1: %2, %3, %4, %5, %6, %7")
                  .arg(name, own, valueText, tolerance.isEmpty() ? QStringLiteral("-") : tolerance,
                       ownGroup.isEmpty() ? QStringLiteral("-") : ownGroup,
                       a.value(QStringLiteral("color")).toString(), state);
    if (!error.isEmpty()) {
      // Its level as every surface logs it (mitcad#1051).
      logged << QStringLiteral("Annotations level %1: %2").arg(name, noticeLevelName(NoticeLevel::Warning));
    }
    if (!kind.isEmpty() && own != kind) {
      continue;
    }
    if (group == QLatin1String("\n") ? !ownGroup.isEmpty() : (!group.isEmpty() && ownGroup != group)) {
      continue;
    }
    if (!search.isEmpty()) {
      const QString haystack = QStringList{name, kindName(own), valueText, tolerance, text, ownGroup}.join(QLatin1Char('\n'));
      if (!haystack.contains(search, Qt::CaseInsensitive)) {
        continue;
      }
    }
    const int row = m_table->rowCount();
    m_table->insertRow(row);
    QTableWidgetItem* nameItem = cell(name);
    nameItem->setData(kNameRole, name);
    const QColor colour(a.value(QStringLiteral("color")).toString());
    if (!error.isEmpty()) {
      // Broken: a warning (mitcad#1040), with its icon and the reason.
      nameItem->setIcon(noticeIcon(NoticeLevel::Warning));
      // In words (mitcad#1741): the reason's names and ids are the log's.
      nameItem->setToolTip(noticeLevelText(NoticeLevel::Warning, tr("%1 is broken: %2").arg(name, shownError(a))));
    } else {
      nameItem->setIcon(measurementSwatch(colour));
    }
    m_table->setItem(row, kName, nameItem);
    m_table->setItem(row, kKind, cell(kindName(own)));
    QTableWidgetItem* valueItem = cell(valueText);
    if (!error.isEmpty()) {
      valueItem->setToolTip(shownError(a));
      // A warning's colour, as its name's badge (mitcad#1051).
      valueItem->setForeground(noticeColor(NoticeLevel::Warning, m_table->palette()));
    } else if (label.contains(QStringLiteral("warning"))) {
      valueItem->setToolTip(label.value(QStringLiteral("warning")).toString());
    }
    m_table->setItem(row, kValue, valueItem);
    m_table->setItem(row, kTolerance, cell(tolerance));
    m_table->setItem(row, kText, cell(text));
    // Auto (mitcad#894): no colour of its own.
    QTableWidgetItem* colourItem = cell(colour.isValid() ? colour.name() : tr("Auto"));
    colourItem->setIcon(measurementSwatch(colour));
    m_table->setItem(row, kColour, colourItem);
    m_table->setItem(row, kGroup, cell(ownGroup));
    auto* shownItem = new QTableWidgetItem;
    shownItem->setFlags(Qt::ItemIsEnabled | Qt::ItemIsSelectable | Qt::ItemIsUserCheckable);
    shownItem->setCheckState(shown ? Qt::Checked : Qt::Unchecked);
    m_table->setItem(row, kShown, shownItem);
    if (!shown) {
      for (int column = 0; column < kShown; ++column) {
        if (QTableWidgetItem* item = m_table->item(row, column); item != nullptr && error.isEmpty()) {
          item->setForeground(palette().color(QPalette::Disabled, QPalette::Text));
        }
      }
    }
  }
  m_table->setSortingEnabled(true);
  if (sortColumn >= 0 && sortColumn < kColumns) {
    m_table->sortByColumn(sortColumn, sortOrder);
  }
  // The selection kept, whole (one row selected after another would
  // keep only the last).
  selectRows(selected, false);
  // The scroll bars' ranges for the new rows first, then where they were.
  m_table->doItemsLayout();
  m_table->verticalScrollBar()->setValue(scrolledDown);
  m_table->horizontalScrollBar()->setValue(scrolledRight);
  m_filling = false;
  const QString rows = logged.isEmpty() ? QStringLiteral("Annotations rows: none") : logged.join(QLatin1Char('\n'));
  if (rows != m_loggedRows) {
    m_loggedRows = rows;
    for (const QString& line : logged.isEmpty() ? QStringList{rows} : logged) {
      qDebug().noquote() << line;
    }
  }
  // The rows the search and the filters leave, in the table's order.
  QStringList left;
  for (int row = 0; row < m_table->rowCount(); ++row) {
    left << m_table->item(row, kName)->data(kNameRole).toString();
  }
  const QString shownRows =
      QStringLiteral("Annotations shown rows: %1").arg(left.isEmpty() ? QStringLiteral("none") : left.join(QStringLiteral(", ")));
  if (shownRows != m_loggedShown) {
    m_loggedShown = shownRows;
    qDebug().noquote() << shownRows;
  }
  followSelection();
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

QStringList AnnotationsWindow::selectedNames() const {
  QStringList names;
  for (const QModelIndex& index : m_table->selectionModel()->selectedRows(kName)) {
    names << index.data(kNameRole).toString();
  }
  // In the table's order.
  std::sort(names.begin(), names.end(), [this](const QString& a, const QString& b) {
    const auto rowOf = [this](const QString& name) {
      for (int row = 0; row < m_table->rowCount(); ++row) {
        if (m_table->item(row, kName)->data(kNameRole).toString() == name) {
          return row;
        }
      }
      return -1;
    };
    return rowOf(a) < rowOf(b);
  });
  return names;
}

QStringList AnnotationsWindow::selectedDimensions() const {
  QStringList names;
  for (const QString& name : selectedNames()) {
    if (!isLeader(annotation(name).value(QStringLiteral("kind")).toString())) {
      names << name;
    }
  }
  return names;
}

void AnnotationsWindow::selectNames(const QStringList& names) {
  m_filling = true;
  selectRows(names, true);
  m_filling = false;
  followSelection();
}

void AnnotationsWindow::selectRows(const QStringList& names, bool scroll) {
  QItemSelection selection;
  QModelIndex first;
  QAbstractItemModel* model = m_table->model();
  for (int row = 0; row < m_table->rowCount(); ++row) {
    if (names.contains(m_table->item(row, kName)->data(kNameRole).toString())) {
      selection.select(model->index(row, 0), model->index(row, kColumns - 1));
      if (!first.isValid()) {
        first = model->index(row, 0);
      }
    }
  }
  // All at once; the current row moved without touching the selection.
  QItemSelectionModel* selected = m_table->selectionModel();
  selected->select(selection, QItemSelectionModel::ClearAndSelect | QItemSelectionModel::Rows);
  if (first.isValid()) {
    selected->setCurrentIndex(first, QItemSelectionModel::NoUpdate);
    if (scroll) {
      m_table->scrollTo(first);
    }
  }
}

void AnnotationsWindow::followSelection() {
  const QStringList names = selectedNames();
  const bool any = !names.isEmpty();
  m_edit->setEnabled(names.size() == 1);
  for (QPushButton* button : {m_colour, m_groupButton, m_show, m_hide, m_delete}) {
    button->setEnabled(any);
  }
  const QStringList dimensions = selectedDimensions();
  m_decimals->setEnabled(!dimensions.isEmpty());
  // A tolerance is an angle's (degrees) or a length's (millimetres, a
  // fit): not both at once.
  int angles = 0;
  for (const QString& name : dimensions) {
    angles += annotation(name).value(QStringLiteral("kind")).toString() == QLatin1String("angle") ? 1 : 0;
  }
  m_tolerance->setEnabled(!dimensions.isEmpty() && (angles == 0 || angles == dimensions.size()));
  // A frame under one dimension or hole callout that is not broken.
  m_addFrame->setEnabled(names.size() == 1 &&
                         takesFrame(annotation(names.front()).value(QStringLiteral("kind")).toString()) &&
                         !annotation(names.front()).contains(QStringLiteral("error")));
  const QString group = m_group->currentData().toString();
  const bool oneGroup = !group.isEmpty() && group != QLatin1String("\n");
  m_showGroup->setEnabled(oneGroup);
  m_hideGroup->setEnabled(oneGroup);
  const QString logged =
      QStringLiteral("Annotations selected: %1").arg(any ? names.join(QStringLiteral(", ")) : QStringLiteral("none"));
  if (logged != m_loggedSelection) {
    m_loggedSelection = logged;
    qDebug().noquote() << logged;
    if (m_actions.selected) {
      m_actions.selected(names);
    }
    TestSync::singleShot(0, this, [this] { logLayout(); });
  }
}

void AnnotationsWindow::changeSelected(const QJsonObject& change) {
  const QStringList names = selectedNames();
  if (!names.isEmpty() && m_actions.change) {
    m_actions.change(names, change);
  }
}

void AnnotationsWindow::chooseColour() {
  const QStringList names = selectedNames();
  if (names.isEmpty()) {
    return;
  }
  const QString current = annotation(names.front()).value(QStringLiteral("color")).toString();
  const QString what = names.size() == 1 ? names.front() : tr("%n annotation(s)", nullptr, static_cast<int>(names.size()));
  if (const std::optional<QString> colour = askAnnotationColour(this, current, what)) {
    changeSelected({{QStringLiteral("color"), *colour}});
  }
}

void AnnotationsWindow::chooseGroup() {
  const QStringList names = selectedNames();
  if (names.isEmpty()) {
    return;
  }
  const QString current = annotation(names.front()).value(QStringLiteral("group")).toString();
  const QString what = names.size() == 1 ? names.front() : tr("%n annotation(s)", nullptr, static_cast<int>(names.size()));
  if (const std::optional<QString> group = askAnnotationGroup(this, groups(), current, what)) {
    changeSelected({{QStringLiteral("group"), group->isEmpty() ? QJsonValue(QJsonValue::Null) : QJsonValue(*group)}});
  }
}

void AnnotationsWindow::chooseDecimals() {
  const QStringList names = selectedDimensions();
  if (names.isEmpty() || !m_actions.change) {
    return;
  }
  const QJsonValue current =
      annotation(names.front()).value(QStringLiteral("format")).toObject().value(QStringLiteral("decimals"));
  const QString what = names.size() == 1 ? names.front() : tr("%n dimension(s)", nullptr, static_cast<int>(names.size()));
  if (const std::optional<QJsonValue> decimals = askAnnotationDecimals(this, current, what)) {
    m_actions.change(names, {{QStringLiteral("format"), QJsonObject{{QStringLiteral("decimals"), *decimals}}}});
  }
}

void AnnotationsWindow::chooseTolerance() {
  const QStringList names = selectedDimensions();
  if (names.isEmpty() || !m_actions.change) {
    return;
  }
  bool angles = true;
  for (const QString& name : names) {
    angles = angles && annotation(name).value(QStringLiteral("kind")).toString() == QLatin1String("angle");
  }
  const QJsonValue current = annotation(names.front()).value(QStringLiteral("tolerance"));
  const QString what = names.size() == 1 ? names.front() : tr("%n dimension(s)", nullptr, static_cast<int>(names.size()));
  if (const std::optional<QJsonValue> tolerance = askAnnotationTolerance(this, current, angles, what)) {
    m_actions.change(names, {{QStringLiteral("tolerance"), *tolerance}});
  }
}

void AnnotationsWindow::showGroup(bool shown) {
  const QString group = m_group->currentData().toString();
  if (group.isEmpty() || group == QLatin1String("\n") || !m_actions.change) {
    return;
  }
  QStringList names;
  for (const QJsonValue& value : m_annotations) {
    const QJsonObject a = value.toObject();
    if (a.value(QStringLiteral("group")).toString() == group) {
      names << a.value(QStringLiteral("name")).toString();
    }
  }
  qDebug().noquote() << QStringLiteral("Annotations %1 group %2: %3")
                            .arg(shown ? QStringLiteral("show") : QStringLiteral("hide"), group,
                                 names.join(QStringLiteral(", ")));
  if (!names.isEmpty()) {
    m_actions.change(names, {{QStringLiteral("visible"), shown}});
  }
}

void AnnotationsWindow::showEvent(QShowEvent* event) {
  QDialog::showEvent(event);
  // The keyboard on the table: arrows choose rows, Ctrl+A all of them.
  m_table->setFocus();
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void AnnotationsWindow::resizeEvent(QResizeEvent* event) {
  QDialog::resizeEvent(event);
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void AnnotationsWindow::moveEvent(QMoveEvent* event) {
  QDialog::moveEvent(event);
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void AnnotationsWindow::logLayout() {
  if (!isVisible() || parentWidget() == nullptr) {
    return;
  }
  QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
  // In the main window's coordinates, as the other logged places.
  const QPoint origin = parentWidget()->window()->mapToGlobal(QPoint(0, 0));
  const auto at = [&origin](const QWidget* widget, const QPoint& point) {
    const QPoint global = widget->mapToGlobal(point) - origin;
    return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
  };
  QStringList lines;
  const std::pair<const char*, const QWidget*> places[] = {
      {"search", m_search},       {"kind filter", m_kind},  {"group filter", m_group},
      {"Show Group", m_showGroup}, {"Hide Group", m_hideGroup}, {"Edit", m_edit},
      {"Colour", m_colour},       {"Group", m_groupButton}, {"Decimals", m_decimals},
      {"Tolerance", m_tolerance}, {"Add Frame", m_addFrame}, {"Show", m_show},
      {"Hide", m_hide},           {"Delete", m_delete},     {"Text Height", m_textHeight}};
  for (const auto& [name, widget] : places) {
    lines << QStringLiteral("Annotations %1 at %2%3")
                 .arg(QLatin1String(name), at(widget, widget->rect().center()),
                      widget->isEnabled() ? QString() : QStringLiteral(" (disabled)"));
  }
  // The rows in sight: their names', and their light bulbs'.
  for (int row = 0; row < m_table->rowCount(); ++row) {
    const QTableWidgetItem* item = m_table->item(row, kName);
    const QRect rect = m_table->visualItemRect(item);
    if (!rect.isValid() || !m_table->viewport()->rect().contains(rect.center())) {
      continue;
    }
    const QString name = item->data(kNameRole).toString();
    lines << QStringLiteral("Annotations row %1 at %2").arg(name, at(m_table->viewport(), rect.center()));
    const QRect bulb = m_table->visualItemRect(m_table->item(row, kShown));
    if (bulb.isValid()) {
      // The check box is at the cell's left.
      const QPoint box(bulb.left() + 10, bulb.center().y());
      lines << QStringLiteral("Annotations shown %1 at %2").arg(name, at(m_table->viewport(), box));
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
