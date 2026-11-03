// SPDX-License-Identifier: MIT
// The Bindings tab of the Configurations window (mitcad#862,
// ConfigurationBindingsPage.hpp).
#include "ConfigurationBindingsPage.hpp"

#include <algorithm>
#include <utility>

#include <QAbstractItemView>
#include <QComboBox>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QSignalBlocker>
#include <QTableWidget>
#include <QTimer>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Icons.hpp"
#include "../framework/PlaceWatcher.hpp"
#include "../framework/Theme.hpp"
#include "BindingsCard.hpp"
#include "DocumentHost.hpp"
#include "DocumentSnapshot.hpp"

namespace mitcad {
namespace {

enum Column { kItem, kProperty, kFollows, kNow };
constexpr int kEntryRole = Qt::UserRole; // the entry's index
// A binding with a problem: the warning fill under the text of the same
// appearance (mitcad#1041).
QColor warningFill() { return themeColor(ColorRole::WarningFill); }
QColor warningInk() { return themeColor(ColorRole::Text); }

} // namespace

ConfigurationBindingsPage::ConfigurationBindingsPage(DocumentHost& host, QWidget* parent)
    : ConfigurationPage(parent), m_host(host) {
  setObjectName(QStringLiteral("configurationsBindingsPage"));
  auto* layout = new QVBoxLayout(this);
  auto* top = new QHBoxLayout;
  // The filter by option (mitcad#968: Option, not Driven by).
  auto* filterLabel = new QLabel(tr("Option:"));
  filterLabel->setObjectName(QStringLiteral("configurationsBindingsOptionLabel"));
  top->addWidget(filterLabel);
  m_driven = new QComboBox;
  m_driven->setObjectName(QStringLiteral("configurationsBindingsDrivenBy"));
  m_driven->setToolTip(tr("Only the bindings that follow this option"));
  m_driven->setAccessibleName(tr("Option"));
  qDebug().noquote() << QStringLiteral("Configurations bindings filter label: %1").arg(filterLabel->text());
  m_driven->addItem(tr("All options"), QString());
  top->addWidget(m_driven);
  m_search = new QLineEdit;
  m_search->setObjectName(QStringLiteral("configurationsBindingsSearch"));
  m_search->setPlaceholderText(tr("Find an item"));
  m_search->setClearButtonEnabled(true);
  m_search->setToolTip(tr("Only the bindings of items whose name has this text"));
  top->addWidget(m_search, 1);
  layout->addLayout(top);

  m_table = new QTableWidget;
  m_table->setObjectName(QStringLiteral("configurationsBindings"));
  m_table->setColumnCount(4);
  m_table->setHorizontalHeaderLabels({tr("Item"), tr("Property"), tr("Follows"), tr("Now")});
  m_table->horizontalHeaderItem(kFollows)->setToolTip(tr("What the property follows: an option's values, a condition, "
                                                         "or an expression"));
  m_table->horizontalHeaderItem(kNow)->setToolTip(tr("What it gives with the options' values now"));
  m_table->setSelectionMode(QAbstractItemView::SingleSelection);
  m_table->setSelectionBehavior(QAbstractItemView::SelectRows);
  m_table->setEditTriggers(QAbstractItemView::NoEditTriggers);
  m_table->setAlternatingRowColors(true);
  m_table->verticalHeader()->hide();
  // Follows takes the room the others leave (mitcad#862 review).
  m_table->horizontalHeader()->resizeSection(kItem, 130);
  m_table->horizontalHeader()->resizeSection(kProperty, 120);
  m_table->horizontalHeader()->resizeSection(kNow, 170);
  m_table->horizontalHeader()->setSectionResizeMode(kFollows, QHeaderView::Stretch);
  m_table->horizontalHeader()->setStretchLastSection(false);
  m_table->setWordWrap(false);
  m_table->setTextElideMode(Qt::ElideMiddle);
  layout->addWidget(m_table, 1);

  auto* buttons = new QHBoxLayout;
  m_edit = new QPushButton(tr("Edit…"));
  m_edit->setObjectName(QStringLiteral("configurationsBindingsEdit"));
  m_edit->setToolTip(tr("The Appearance or Materials window for it, or Show When… or Suppress When…"));
  m_select = new QPushButton(tr("Select in Browser"));
  m_select->setObjectName(QStringLiteral("configurationsBindingsSelect"));
  m_remove = new QPushButton(themeIcon(QStringLiteral("delete")), tr("Remove Binding"));
  m_remove->setObjectName(QStringLiteral("configurationsBindingsRemove"));
  m_remove->setToolTip(tr("The property keeps the value it has now and follows nothing"));
  for (QPushButton* button : {m_edit, m_select, m_remove}) {
    button->setAutoDefault(false);
    buttons->addWidget(button);
  }
  buttons->addStretch(1);
  m_status = new QLabel;
  m_status->setObjectName(QStringLiteral("configurationsBindingsStatus"));
  buttons->addWidget(m_status);
  layout->addLayout(buttons);
  m_message = new QLabel;
  m_message->setWordWrap(true);
  setErrorStyleSheet(m_message);
  m_message->hide();
  layout->addWidget(m_message);

  connect(m_driven, &QComboBox::currentIndexChanged, this, [this] {
    if (!m_building) {
      fill();
    }
  });
  connect(m_search, &QLineEdit::textChanged, this, [this] {
    if (!m_building) {
      fill();
    }
  });
  connect(m_table, &QTableWidget::itemSelectionChanged, this, [this] { updateButtons(); });
  connect(m_table, &QTableWidget::cellDoubleClicked, this, [this] { editSelected(); });
  connect(m_edit, &QPushButton::clicked, this, [this] { editSelected(); });
  connect(m_select, &QPushButton::clicked, this, [this] { selectSelected(); });
  connect(m_remove, &QPushButton::clicked, this, [this] {
    // Once the button's own handling is over and no job computes the model.
    QTimer::singleShot(0, this, [this] {
      if (m_host.modelBusy()) {
        m_host.whenIdle(this, [this] { removeSelected(); });
      } else {
        removeSelected();
      }
    });
  });
  new PlaceWatcher(this, [this] { logPlaces(); });
  updateButtons();
}

void ConfigurationBindingsPage::refresh() {
  m_entries = readBindings(m_host.model());
  const BindingLookup lookup = BindingLookup::read(m_host.model());
  m_readable.clear();
  m_usage.clear();
  QStringList used;
  for (const BindingEntry& entry : std::as_const(m_entries)) {
    m_readable.append(readBinding(entry, lookup));
    for (const QString& option : std::as_const(m_readable.last().options)) {
      m_usage[option] += 1;
      if (!used.contains(option)) {
        used << option;
      }
    }
  }
  std::sort(used.begin(), used.end(),
            [](const QString& a, const QString& b) { return a.compare(b, Qt::CaseInsensitive) < 0; });
  // The options the bindings use: the list kept while it is the same.
  QStringList listed;
  for (int i = 1; i < m_driven->count(); ++i) {
    listed << m_driven->itemData(i).toString();
  }
  if (listed != used) {
    m_building = true;
    const QString current = m_driven->currentData().toString();
    while (m_driven->count() > 1) {
      m_driven->removeItem(1);
    }
    for (const QString& option : std::as_const(used)) {
      m_driven->addItem(option, option);
    }
    m_driven->setCurrentIndex(std::max(0, m_driven->findData(current)));
    m_building = false;
  }
  fill();
}

void ConfigurationBindingsPage::showOption(const QString& option) {
  const int index = m_driven->findData(option);
  m_driven->setCurrentIndex(std::max(0, index)); // fills
}

QStringList ConfigurationBindingsPage::usersOf(const QString& parameter) const {
  QStringList users;
  for (int i = 0; i < m_entries.size(); ++i) {
    if (m_readable[i].options.contains(parameter)) {
      users << QStringLiteral("%1 %2").arg(m_entries[i].item, m_entries[i].property());
    }
  }
  return users;
}

void ConfigurationBindingsPage::fill() {
  const int was = selectedIndex();
  const QString selectedKey = was >= 0 ? m_entries[was].key() : QString();
  const QString option = m_driven->currentData().toString();
  const QString search = m_search->text().trimmed();
  m_building = true;
  m_table->setRowCount(0);
  QStringList logged;
  int shown = 0;
  int warnings = 0;
  for (int i = 0; i < m_entries.size(); ++i) {
    const BindingEntry& entry = m_entries[i];
    const ReadableBinding& readable = m_readable[i];
    if ((!option.isEmpty() && !readable.options.contains(option)) ||
        (!search.isEmpty() && !entry.item.contains(search, Qt::CaseInsensitive))) {
      continue;
    }
    if (!readable.warnings.isEmpty()) {
      ++warnings;
    }
    const int row = m_table->rowCount();
    m_table->insertRow(row);
    // The warning is its own text: the row's tooltip and, selected, the
    // status line (mitcad#862 review); Now says what it gives.
    const QString problems = readable.warnings.join(QLatin1Char('\n'));
    const QString texts[] = {entry.item, entry.property(), readable.follows(), readable.now};
    for (int column = kItem; column <= kNow; ++column) {
      auto* item = new QTableWidgetItem(texts[column]);
      item->setData(kEntryRole, i);
      item->setToolTip(problems.isEmpty() ? texts[column] : texts[column] + QStringLiteral("\n⚠ ") + problems);
      if (!problems.isEmpty()) {
        item->setBackground(warningFill());
        // The fill and its ink of one appearance: dark text on the light
        // fill, light text on the dark one.
        item->setForeground(warningInk());
      }
      m_table->setItem(row, column, item);
    }
    // The item's kind as the browser shows it.
    QString icon = QStringLiteral("body");
    if (entry.kind == BindingEntry::Kind::Suppressed) {
      icon = featureIcon(entry.featureType);
    } else if (entry.kind == BindingEntry::Kind::OccurrenceShown) {
      icon = QStringLiteral("component");
    }
    m_table->item(row, kItem)->setIcon(themeIcon(icon));
    if (entry.kind == BindingEntry::Kind::FaceAppearance) {
      m_table->item(row, kProperty)->setToolTip(tr("Face %1 of %2").arg(entry.face, entry.item));
    }
    if (readable.form == ReadableBinding::Form::Expression) {
      m_table->item(row, kFollows)->setToolTip(entry.expression);
    }
    if (!problems.isEmpty()) {
      m_table->item(row, kNow)->setIcon(themeIcon(QStringLiteral("warning")));
    } else if (readable.nowColor.isValid()) {
      m_table->item(row, kNow)->setIcon(QIcon(bindingSwatch(readable.nowColor, 12)));
    }
    if (entry.key() == selectedKey) {
      m_table->selectRow(row);
    }
    ++shown;
    QString line = QStringLiteral("Configurations binding %1 %2: %3; now %4")
                       .arg(entry.item, entry.property(), readable.follows(), readable.now);
    for (const QString& problem : readable.warnings) {
      line += QStringLiteral("; warning %1").arg(problem);
    }
    logged << line;
  }
  // A row selected, so that the buttons work at once (mitcad#862 review).
  if (m_table->selectedItems().isEmpty() && m_table->rowCount() > 0) {
    m_table->selectRow(0);
  }
  m_building = false;
  const int total = static_cast<int>(m_entries.size());
  const QString all = total == 1 ? tr("1 binding") : tr("%1 bindings").arg(total);
  QString status = shown == total ? all : tr("%1 of %2").arg(shown).arg(all);
  if (warnings > 0) {
    status += warnings == 1 ? tr(", 1 warning") : tr(", %1 warnings").arg(warnings);
  }
  m_counts = status;
  logged << QStringLiteral("Configurations bindings status: %1").arg(status);
  const QString text = logged.join(QLatin1Char('\n'));
  if (text != m_logged) {
    m_logged = text;
    for (const QString& line : std::as_const(logged)) {
      qDebug().noquote() << line;
    }
  }
  const QString filter = QStringLiteral("Configurations bindings driven by %1, search '%2': %3")
                             .arg(option.isEmpty() ? QStringLiteral("All options") : option, search, status);
  if (filter != m_loggedFilter) {
    m_loggedFilter = filter;
    qDebug().noquote() << filter;
  }
  updateButtons();
  QTimer::singleShot(0, this, &ConfigurationBindingsPage::logPlaces);
}

int ConfigurationBindingsPage::selectedIndex() const {
  const QList<QTableWidgetItem*> items = m_table->selectedItems();
  if (items.isEmpty()) {
    return -1;
  }
  const int index = items.first()->data(kEntryRole).toInt();
  return index >= 0 && index < m_entries.size() ? index : -1;
}

void ConfigurationBindingsPage::updateButtons() {
  const int index = selectedIndex();
  for (QPushButton* button : {m_edit, m_select, m_remove}) {
    button->setEnabled(index >= 0);
  }
  // The status line: the counts, and the selected row's warning.
  QString status = m_counts;
  QString problem;
  if (index >= 0 && !m_readable[index].warnings.isEmpty()) {
    problem = m_readable[index].warnings.join(QStringLiteral("; "));
    status = QStringLiteral("⚠ %1 — %2").arg(problem, m_counts);
  }
  m_status->setText(m_status->fontMetrics().elidedText(status, Qt::ElideMiddle, 520));
  m_status->setToolTip(problem);
  if (problem != m_loggedProblem) {
    m_loggedProblem = problem;
    if (!problem.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Configurations bindings selected warning: %1").arg(problem);
    }
  }
}

void ConfigurationBindingsPage::editSelected() {
  const int index = selectedIndex();
  if (index < 0 || !m_actions.edit) {
    return;
  }
  const BindingEntry entry = m_entries[index];
  qDebug().noquote() << QStringLiteral("Configurations binding edit: %1 %2").arg(entry.item, entry.property());
  const auto action = m_actions.edit;
  // After the click's own handling: the editor may be modal.
  QTimer::singleShot(0, this, [action, entry] { action(entry); });
}

void ConfigurationBindingsPage::selectSelected() {
  const int index = selectedIndex();
  if (index < 0 || !m_actions.select) {
    return;
  }
  const BindingEntry entry = m_entries[index];
  qDebug().noquote() << QStringLiteral("Configurations binding select: %1 %2").arg(entry.item, entry.property());
  m_actions.select(entry);
}

void ConfigurationBindingsPage::removeSelected() {
  const int index = selectedIndex();
  if (index < 0) {
    return;
  }
  const BindingEntry entry = m_entries[index];
  const QString what = QStringLiteral("%1 %2").arg(entry.item, entry.property());
  QString reason = m_host.readOnlyReason();
  if (reason.isEmpty() && !m_host.canChangeModel()) {
    reason = m_host.changeBlockedReason();
  }
  if (reason.isEmpty()) {
    if (m_host.runModelCommand(entry.removeCommand())) {
      m_message->hide();
      qDebug().noquote() << QStringLiteral("Configurations binding removed: %1").arg(what);
      return;
    }
    if (m_host.lastCancelled()) {
      return;
    }
    reason = m_host.lastError();
  }
  m_message->setText(reason);
  m_message->show();
  qDebug().noquote() << QStringLiteral("Configurations binding %1 refused: %2").arg(what, reason);
}

void ConfigurationBindingsPage::logPlaces() {
  if (!isVisible() || window()->parentWidget() == nullptr) {
    return;
  }
  // In the main window's coordinates, as the window's other places.
  const QPoint origin = window()->parentWidget()->window()->mapToGlobal(QPoint(0, 0));
  QStringList lines;
  const auto logAt = [&lines, &origin](const QString& what, const QWidget* widget, const QPoint& point) {
    const QPoint at = widget->mapToGlobal(point) - origin;
    lines << QStringLiteral("%1 at %2,%3").arg(what).arg(at.x()).arg(at.y());
  };
  const std::pair<const char*, QWidget*> controls[] = {{"driven by", m_driven},
                                                         {"search", m_search},
                                                         {"edit", m_edit},
                                                         {"select", m_select},
                                                         {"remove", m_remove}};
  for (const auto& [name, widget] : controls) {
    logAt(QStringLiteral("Configurations bindings %1").arg(QLatin1String(name)), widget, widget->rect().center());
  }
  const QRect viewport = m_table->viewport()->rect();
  for (int row = 0; row < m_table->rowCount(); ++row) {
    const QTableWidgetItem* item = m_table->item(row, kItem);
    if (item == nullptr) {
      continue;
    }
    const QRect cell = m_table->visualItemRect(item);
    if (!viewport.contains(cell.center())) {
      continue;
    }
    const BindingEntry& entry = m_entries[item->data(kEntryRole).toInt()];
    logAt(QStringLiteral("Configurations binding %1 %2").arg(entry.item, entry.property()), m_table->viewport(),
          cell.center());
  }
  const QString text = lines.join(QLatin1Char('\n'));
  if (text != m_loggedPlaces) {
    m_loggedPlaces = text;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

} // namespace mitcad
