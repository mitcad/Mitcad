// SPDX-License-Identifier: MIT
#include "FindDialog.hpp"

#include <algorithm>

#include <utility>

#include <QAbstractTableModel>
#include <QCheckBox>
#include <QComboBox>
#include <QCoreApplication>
#include <QGuiApplication>
#include <QDialogButtonBox>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QItemSelection>
#include <QItemSelectionModel>
#include <QLabel>
#include <QLineEdit>
#include <QPalette>
#include <QPushButton>
#include <QSet>
#include <QStringList>
#include <QTableView>
#include <QTimer>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Icons.hpp"
#include "../framework/TestSync.hpp"
#include "../platform/MacDialogs.hpp"

namespace mitcad {

// The results as a table: name, kind and the place in the assembly. A model
// rather than item widgets, so that thousands of rows stay cheap.
class FindModel : public QAbstractTableModel {
  Q_DECLARE_TR_FUNCTIONS(mitcad::FindModel)

public:
  explicit FindModel(QObject* parent) : QAbstractTableModel(parent) {}

  void set(QVector<FindResult> results) {
    beginResetModel();
    m_results = std::move(results);
    endResetModel();
  }
  const QVector<FindResult>& results() const { return m_results; }

  int rowCount(const QModelIndex& parent = QModelIndex()) const override {
    return parent.isValid() ? 0 : static_cast<int>(m_results.size());
  }
  int columnCount(const QModelIndex& parent = QModelIndex()) const override { return parent.isValid() ? 0 : 3; }
  QVariant headerData(int section, Qt::Orientation orientation, int role) const override {
    if (orientation != Qt::Horizontal || role != Qt::DisplayRole) {
      return {};
    }
    switch (section) {
    case 0: return tr("Name");
    case 1: return tr("Kind");
    default: return tr("In");
    }
  }
  QVariant data(const QModelIndex& index, int role) const override {
    if (!index.isValid() || index.row() < 0 || index.row() >= m_results.size()) {
      return {};
    }
    const FindResult& result = m_results[index.row()];
    switch (role) {
    case Qt::DisplayRole:
      switch (index.column()) {
      case 0: return result.name;
      case 1: return result.kind == FindResult::Kind::Body ? tr("Body") : tr("Component");
      default: return result.path;
      }
    case Qt::DecorationRole:
      if (index.column() == 0) {
        return themeIcon(result.kind == FindResult::Kind::Body ? QStringLiteral("body") : QStringLiteral("component"));
      }
      return {};
    case Qt::ForegroundRole:
      if (!result.visible) {
        return QGuiApplication::palette().color(QPalette::Disabled, QPalette::Text);
      }
      return {};
    case Qt::ToolTipRole:
      return QStringLiteral("%1 (%2)%3")
          .arg(result.name, result.uid, result.visible ? QString() : tr(", hidden"));
    default: return {};
    }
  }

private:
  QVector<FindResult> m_results;
};

FindDialog::FindDialog(QWidget* parent) : QDialog(parent) {
  setWindowTitle(tr("Find in Design"));
  setWindowIcon(themeIcon(QStringLiteral("search")));
  resize(560, 480);
  auto* layout = new QVBoxLayout(this);

  m_text = new QLineEdit;
  m_text->setPlaceholderText(tr("Name (* and ? are wildcards)"));
  m_text->setClearButtonEnabled(true);
  mac::styleSearchField(m_text);
  layout->addWidget(m_text);

  auto* filters = new QHBoxLayout;
  m_components = new QCheckBox(tr("Components"));
  m_components->setChecked(true);
  m_bodies = new QCheckBox(tr("Bodies"));
  m_bodies->setChecked(true);
  m_visibility = new QComboBox;
  m_visibility->addItem(tr("Visible and hidden"), static_cast<int>(FindOptions::Visibility::Any));
  m_visibility->addItem(tr("Visible only"), static_cast<int>(FindOptions::Visibility::Visible));
  m_visibility->addItem(tr("Hidden only"), static_cast<int>(FindOptions::Visibility::Hidden));
  filters->addWidget(m_components);
  filters->addWidget(m_bodies);
  filters->addStretch(1);
  filters->addWidget(m_visibility);
  layout->addLayout(filters);

  m_model = new FindModel(this);
  m_table = new QTableView;
  m_table->setModel(m_model);
  m_table->setSelectionBehavior(QAbstractItemView::SelectRows);
  m_table->setSelectionMode(QAbstractItemView::ExtendedSelection);
  m_table->setEditTriggers(QAbstractItemView::NoEditTriggers);
  m_table->setShowGrid(false);
  m_table->setWordWrap(false);
  m_table->setAlternatingRowColors(true);
  m_table->verticalHeader()->hide();
  m_table->verticalHeader()->setDefaultSectionSize(22);
  m_table->horizontalHeader()->setStretchLastSection(true);
  m_table->horizontalHeader()->setSectionResizeMode(0, QHeaderView::Interactive);
  m_table->setColumnWidth(0, 200);
  m_table->setColumnWidth(1, 90);
  mac::styleListTable(m_table);
  layout->addWidget(m_table, 1);

  m_count = new QLabel;
  layout->addWidget(m_count);

  auto* buttons = new QDialogButtonBox;
  m_show = new QPushButton(tr("Show"));
  m_show->setToolTip(tr("Shows the selected results, or all when none is selected, in one undo step"));
  m_hide = new QPushButton(tr("Hide"));
  m_hide->setToolTip(tr("Hides the selected results, or all when none is selected, in one undo step"));
  m_appearance = new QPushButton(tr("Appearance…"));
  m_appearance->setToolTip(tr("Edit Appearances with the bodies of the results as the targets"));
  m_select = new QPushButton(tr("Select"));
  m_select->setToolTip(tr("Makes the results the selection, for the commands that take one"));
  for (QPushButton* button : {m_show, m_hide, m_appearance, m_select}) {
    buttons->addButton(button, QDialogButtonBox::ActionRole);
  }
  m_all = new QPushButton(tr("Select All"));
  m_none = new QPushButton(tr("Select None"));
  m_close = new QPushButton(tr("Close"));
  buttons->addButton(m_all, QDialogButtonBox::ResetRole);
  buttons->addButton(m_none, QDialogButtonBox::ResetRole);
  buttons->addButton(m_close, QDialogButtonBox::RejectRole);
  layout->addWidget(buttons);
  for (QPushButton* button : {m_show, m_hide, m_appearance, m_select, m_all, m_none, m_close}) {
    button->setAutoDefault(false);
  }

  mac::styleDialogButtons(buttons, m_close);
  mac::styleDialog(this);
  m_timer = new QTimer(this);
  m_timer->setSingleShot(true);
  m_timer->setInterval(150);
  TestSync::watch(m_timer);
  connect(m_timer, &QTimer::timeout, this, [this] { search(); });
  connect(m_text, &QLineEdit::textChanged, m_timer, qOverload<>(&QTimer::start));
  connect(m_components, &QCheckBox::toggled, m_timer, qOverload<>(&QTimer::start));
  connect(m_bodies, &QCheckBox::toggled, m_timer, qOverload<>(&QTimer::start));
  connect(m_visibility, qOverload<int>(&QComboBox::currentIndexChanged), m_timer, qOverload<>(&QTimer::start));

  connect(m_table->selectionModel(), &QItemSelectionModel::selectionChanged, this, [this] {
    updateButtons();
    if (!m_updating) {
      emit selectionPicked();
    }
  });
  connect(m_table, &QTableView::doubleClicked, this, [this] { emit revealRequested(); });
  connect(m_show, &QPushButton::clicked, this, &FindDialog::showRequested);
  connect(m_hide, &QPushButton::clicked, this, &FindDialog::hideRequested);
  connect(m_appearance, &QPushButton::clicked, this, &FindDialog::appearanceRequested);
  connect(m_select, &QPushButton::clicked, this, &FindDialog::selectRequested);
  connect(m_all, &QPushButton::clicked, m_table, &QTableView::selectAll);
  connect(m_none, &QPushButton::clicked, m_table, &QTableView::clearSelection);
  connect(m_close, &QPushButton::clicked, this, &QDialog::close);
  updateButtons();
}

FindDialog::~FindDialog() = default;

void FindDialog::setSnapshot(const DocumentSnapshot& snapshot) {
  if (m_haveSnapshot && snapshot.key == m_snapshot.key) {
    return;
  }
  m_snapshot = snapshot;
  m_haveSnapshot = true;
  if (isVisible()) {
    search();
  }
}

void FindDialog::setText(const QString& text) {
  m_text->setText(text);
  m_timer->stop();
  search();
}

void FindDialog::setPropertySearch(PropertySearch search) { m_propertySearch = std::move(search); }

void FindDialog::focusSearch() {
  m_text->setFocus();
  m_text->selectAll();
}

void FindDialog::search() {
  m_timer->stop();
  if (!m_haveSnapshot) {
    return;
  }
  FindOptions options;
  options.text = m_text->text();
  options.components = m_components->isChecked();
  options.bodies = m_bodies->isChecked();
  options.visibility = static_cast<FindOptions::Visibility>(m_visibility->currentData().toInt());
  // Property values (mitcad#496): what the model finds; nothing when it
  // cannot read the search.
  if (m_propertySearch && isPropertySearch(options.text)) {
    QString error;
    std::optional<QSet<QString>> owners = m_propertySearch(options.text.trimmed(), &error);
    options.propertyOwners = owners ? *owners : QSet<QString>();
    if (!owners) {
      qDebug().noquote() << QStringLiteral("Find in Design property search: %1").arg(error);
    }
  }
  QVector<FindResult> found = findInDesign(m_snapshot, options);

  // The selection stays on the results that are still there.
  QSet<QString> keep;
  for (const FindResult& result : selected()) {
    keep.insert(result.key());
  }
  m_updating = true;
  m_model->set(found);
  if (!keep.isEmpty()) {
    QItemSelection again;
    for (int row = 0; row < found.size(); ++row) {
      if (keep.contains(found[row].key())) {
        again.select(m_model->index(row, 0), m_model->index(row, m_model->columnCount() - 1));
      }
    }
    m_table->selectionModel()->select(again, QItemSelectionModel::ClearAndSelect);
  }
  m_updating = false;
  m_count->setText(countText());
  updateButtons();
  qDebug().noquote() << QStringLiteral("Find in Design found: %1").arg(countText());
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

QString FindDialog::countText() const {
  int bodies = 0;
  int components = 0;
  for (const FindResult& result : m_model->results()) {
    (result.kind == FindResult::Kind::Body ? bodies : components)++;
  }
  return QStringLiteral("%1, %2")
      .arg(bodies == 1 ? QStringLiteral("1 body") : QStringLiteral("%1 bodies").arg(bodies),
           components == 1 ? QStringLiteral("1 component") : QStringLiteral("%1 components").arg(components));
}

QVector<FindResult> FindDialog::selected() const {
  QVector<FindResult> chosen;
  const QModelIndexList rows = m_table->selectionModel()->selectedRows();
  QVector<int> numbers;
  numbers.reserve(rows.size());
  for (const QModelIndex& index : rows) {
    numbers.append(index.row());
  }
  std::sort(numbers.begin(), numbers.end());
  for (const int row : std::as_const(numbers)) {
    chosen.append(m_model->results()[row]);
  }
  return chosen;
}

QVector<FindResult> FindDialog::targets() const {
  const QVector<FindResult> chosen = selected();
  return chosen.isEmpty() ? m_model->results() : chosen;
}

bool FindDialog::current(FindResult* result) const {
  const QModelIndex index = m_table->currentIndex();
  if (!index.isValid() || index.row() >= m_model->results().size()) {
    return false;
  }
  *result = m_model->results()[index.row()];
  return true;
}

void FindDialog::updateButtons() {
  const bool any = !m_model->results().isEmpty();
  for (QPushButton* button : {m_show, m_hide, m_appearance, m_select, m_all}) {
    button->setEnabled(any);
  }
  const int count = static_cast<int>(selected().size());
  m_none->setEnabled(count > 0);
  m_count->setText(count > 0 ? tr("%1 (%2 selected)").arg(countText()).arg(count) : countText());
}

void FindDialog::showEvent(QShowEvent* event) {
  QDialog::showEvent(event);
  // The design may have changed while the window was closed.
  search();
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void FindDialog::resizeEvent(QResizeEvent* event) {
  QDialog::resizeEvent(event);
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void FindDialog::moveEvent(QMoveEvent* event) {
  QDialog::moveEvent(event);
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void FindDialog::logLayout() {
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
  lines << QStringLiteral("Find in Design field at %1").arg(at(m_text, m_text->rect().center()));
  for (const auto& [name, widget] : {std::pair<const char*, QWidget*>{"components", m_components},
                                     {"bodies", m_bodies},
                                     {"visibility", m_visibility},
                                     {"show", m_show},
                                     {"hide", m_hide},
                                     {"appearance", m_appearance},
                                     {"select", m_select},
                                     {"all", m_all},
                                     {"none", m_none},
                                     {"close", m_close}}) {
    lines << QStringLiteral("Find in Design %1 at %2").arg(QString::fromLatin1(name), at(widget, widget->rect().center()));
  }
  // The rows in sight, by number and by name.
  const int rows = static_cast<int>(m_model->results().size());
  for (int row = 0; row < rows && row < 60; ++row) {
    const QRect rect = m_table->visualRect(m_model->index(row, 0));
    if (!rect.isValid() || !m_table->viewport()->rect().contains(rect.center())) {
      continue;
    }
    const QPoint center = rect.center();
    lines << QStringLiteral("Find in Design row %1 at %2").arg(row + 1).arg(at(m_table->viewport(), center));
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_logged) {
    m_logged = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

} // namespace mitcad
