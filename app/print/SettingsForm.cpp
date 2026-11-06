// SPDX-License-Identifier: MIT
#include "SettingsForm.hpp"

#include <utility>

#include <QCheckBox>
#include <QEvent>
#include <QFont>
#include <QHBoxLayout>
#include <QHash>
#include <QJsonValue>
#include <QLabel>
#include <QLineEdit>
#include <QRect>
#include <QScrollArea>
#include <QScrollBar>
#include <QSettings>
#include <QSignalBlocker>
#include <QTabBar>
#include <QToolButton>
#include <QVBoxLayout>

#include "SettingField.hpp"

namespace mitcad {
namespace {

const char* kAdvancedSetting = "print/advancedSettings";
const char* kGeneralTab = "general"; // the tab of a window's own fields
const char* kEndTab = "variants";    // the tab of a window's page after the print tabs

} // namespace

SettingsForm::SettingsForm(QWidget* parent) : QWidget(parent) {
  auto* layout = new QVBoxLayout(this);
  layout->setContentsMargins(0, 0, 0, 0);
  layout->setSpacing(4);
  auto* top = new QHBoxLayout;
  m_search = new QLineEdit;
  m_search->setPlaceholderText(tr("Search settings"));
  m_search->setClearButtonEnabled(true);
  m_search->setObjectName(QStringLiteral("settingsSearch"));
  m_advanced = new QCheckBox(tr("Advanced"));
  m_advanced->setLayoutDirection(Qt::RightToLeft);
  m_advanced->setChecked(QSettings().value(QLatin1String(kAdvancedSetting), false).toBool());
  top->addWidget(m_search, 1);
  top->addStretch(0);
  top->addWidget(m_advanced);
  layout->addLayout(top);
  m_tabs = new QTabBar;
  m_tabs->setObjectName(QStringLiteral("settingsTabs"));
  m_tabs->setAccessibleName(tr("Print settings category"));
  m_tabs->setExpanding(false);
  m_tabs->setDrawBase(true);
  // The selected tab clearly marked (mitcad#655): bold, underlined.
  m_tabs->setStyleSheet(QStringLiteral("QTabBar::tab { padding: 4px 12px; border: none; margin-right: 2px; }"
                                      "QTabBar::tab:selected { font-weight: bold;"
                                      " border-bottom: 2px solid palette(highlight); }"));
  layout->addWidget(m_tabs);
  m_scroll = new QScrollArea;
  m_scroll->setWidgetResizable(true);
  m_scroll->setFrameShape(QFrame::NoFrame);
  layout->addWidget(m_scroll, 1);

  connect(m_search, &QLineEdit::textChanged, this, [this] {
    m_forced.clear();
    applyFilter();
  });
  connect(m_advanced, &QCheckBox::toggled, this, [this](bool on) {
    QSettings().setValue(QLatin1String(kAdvancedSetting), on);
    applyFilter();
  });
  connect(m_tabs, &QTabBar::currentChanged, this, [this](int index) {
    m_currentTab = index >= 0 ? m_tabs->tabData(index).toString() : QString();
    m_forced.clear();
    applyFilter();
    m_scroll->verticalScrollBar()->setValue(0);
  });
  connect(m_scroll->verticalScrollBar(), &QScrollBar::valueChanged, this, &SettingsForm::layoutChanged);
  // The rows' places settle only after the layouts have run, which can be
  // after the filter's layoutChanged was answered: a search that hides the
  // window's own fields above the list moves the list up a moment later.
  // Its move and its page's new layout say so again, so the places logged
  // last are where the rows are (a click at a place logged too early hit
  // the gap between rows).
  m_scroll->installEventFilter(this);
}

SettingsForm::~SettingsForm() = default;

bool SettingsForm::eventFilter(QObject* watched, QEvent* event) {
  const QEvent::Type type = event->type();
  if ((watched == m_scroll && (type == QEvent::Move || type == QEvent::Resize)) ||
      (watched == m_page && (type == QEvent::Resize || type == QEvent::LayoutRequest))) {
    emit layoutChanged();
  }
  return QWidget::eventFilter(watched, event);
}

void SettingsForm::clearRows() {
  m_rows.clear();
  m_tabTitles.clear();
  m_groups.clear();
  m_categories.clear();
  // A new set starts on its first tab (General when the window has one).
  m_currentTab.clear();
  m_page = new QWidget;
  m_rowsLayout = new QVBoxLayout(m_page);
  m_rowsLayout->setContentsMargins(4, 4, 4, 4);
  m_rowsLayout->setSpacing(2);
  m_page->installEventFilter(this); // its rows' places settled (eventFilter)
  m_scroll->setWidget(m_page);      // the page before is deleted
}

void SettingsForm::setKeys(const PrintKeyTable& table, const QString& set, const QString& dialect) {
  if (set == m_set && dialect == m_dialect && !m_rows.empty()) {
    return;
  }
  m_set = set;
  m_dialect = dialect;
  clearRows();
  const QVector<const PrintKey*> keys = table.keys(set);
  for (const PrintCategory& category : table.layout(set)) {
    bool any = false;
    for (const PrintGroup& group : category.groups) {
      Group shown;
      shown.category = category.id;
      shown.categoryTitle = category.title;
      shown.id = group.id;
      shown.title = group.title;
      for (const PrintKey* key : keys) {
        if (table.categoryOf(*key) != category.id || table.groupOf(*key) != group.id) {
          continue;
        }
        // A key kept per slicer that the form edits: a row per slicer
        // (mitcad#1062); any other key one row.
        QStringList slicers{QString()};
        if (key->perDialect && m_slicerKeys.contains(key->key) && !key->dialects.isEmpty()) {
          slicers = key->dialects;
        }
        for (const QString& slicer : std::as_const(slicers)) {
          Row row;
          row.key = *key;
          row.slicer = slicer;
          row.title = slicer.isEmpty() ? key->name : slicerTitle(slicer);
          row.category = category.id;
          row.group = group.id;
          row.line = new QWidget;
          auto* box = new QHBoxLayout(row.line);
          box->setContentsMargins(12, 0, 0, 0);
          auto* label = new QLabel(row.title);
          label->setMinimumWidth(200);
          label->setMaximumWidth(260);
          // [unit | %] only where the design's slicer takes a percentage
          // (mitcad#655).
          row.field = new SettingField(
              *key, nullptr, key->kind != PrintKey::Kind::NumberOrPercent || key->takesPercent(dialect), slicer);
          if (QWidget* editor = row.field->placeWidget()) {
            // printSetting_<key>, or the variants form's own (mitcad#1054).
            editor->setObjectName(m_fieldPrefix + key->key +
                                  (slicer.isEmpty() ? QString() : QStringLiteral(".") + slicer));
          }
          if (!slicer.isEmpty()) {
            row.field->setChoicesSource([this, name = key->key, slicer] {
              return slicerChoices ? slicerChoices(name, slicer) : QStringList();
            });
          }
          row.field->setDialect(dialect); // its range (mitcad#746)
          row.field->setReadOnly(m_readOnly);
          connect(row.field, &SettingField::committed, this, &SettingsForm::valueChanged);
          connect(row.field, &SettingField::refused, this, &SettingsForm::refused);
          box->addWidget(label);
          box->addWidget(row.field, 1);
          if (shown.rows.empty()) {
            shown.heading = new QLabel;
            QFont bold = shown.heading->font();
            bold.setBold(true);
            shown.heading->setFont(bold);
            shown.heading->setContentsMargins(0, 6, 0, 2);
            m_rowsLayout->addWidget(shown.heading);
          }
          m_rowsLayout->addWidget(row.line);
          shown.rows.push_back(static_cast<int>(m_rows.size()));
          m_rows.push_back(std::move(row));
        }
      }
      if (!shown.rows.empty()) {
        m_groups.push_back(std::move(shown));
        any = true;
      }
    }
    if (any) {
      m_tabTitles.insert(category.id, category.title);
      m_categories << category.id;
    }
  }
  m_rowsLayout->addStretch(1);
  applyFilter();
}

QString SettingsForm::tabTitle(const QString& id) const {
  const QString title = id == QLatin1String(kGeneralTab) ? m_generalTitle
                        : id == QLatin1String(kEndTab)   ? m_endTitle
                                                         : m_tabTitles.value(id);
  // A dot, not a count that reads as one of filaments (mitcad#655).
  return m_hintCounts.value(id) > 0 ? QStringLiteral("%1 •").arg(title) : title;
}

void SettingsForm::syncTabs(const QStringList& wanted) {
  // The bar holds the tabs with something to show only, made again when
  // they change (mitcad#655): a tab hidden and shown again could leave
  // the bar laid out without height.
  QStringList present;
  for (int i = 0; i < m_tabs->count(); ++i) {
    present << m_tabs->tabData(i).toString();
  }
  const QSignalBlocker block(m_tabs);
  if (present != wanted) {
    while (m_tabs->count() > 0) {
      m_tabs->removeTab(0);
    }
    for (const QString& id : wanted) {
      const int index = m_tabs->addTab(tabTitle(id));
      m_tabs->setTabData(index, id);
    }
  }
  if (!wanted.contains(m_currentTab)) {
    m_currentTab = wanted.value(0);
  }
  const int current = static_cast<int>(wanted.indexOf(m_currentTab));
  if (current >= 0 && m_tabs->currentIndex() != current) {
    m_tabs->setCurrentIndex(current);
  }
}

void SettingsForm::setSlicerKeys(const QStringList& keys) {
  if (keys == m_slicerKeys) {
    return;
  }
  m_slicerKeys = keys;
  m_set.clear(); // the next setKeys builds the rows again
}

void SettingsForm::setHiddenKeys(const QStringList& keys) {
  m_hidden = QSet<QString>(keys.begin(), keys.end());
  applyFilter();
}

void SettingsForm::setValues(const QJsonObject& values) {
  for (Row& row : m_rows) {
    row.field->setValue(values.value(row.key.key));
  }
  applyFilter();
}

void SettingsForm::setError(const QString& key, const QString& message) {
  for (Row& row : m_rows) {
    if (row.key.key == key) {
      row.field->setError(message);
    }
  }
}

void SettingsForm::setReadOnly(bool readOnly) {
  m_readOnly = readOnly;
  for (Row& row : m_rows) {
    row.field->setReadOnly(readOnly);
  }
}

bool SettingsForm::rowShown(const Row& row, bool searching, const QString& search, const QString& category) const {
  // A value per slicer (a printer preset's name) is the slicers' own, never
  // edited: not shown (mitcad#655), unless the form edits it a slicer's row
  // each (mitcad#1062).
  if (m_hidden.contains(row.key.key) || (row.key.perDialect && row.slicer.isEmpty())) {
    return false;
  }
  if (row.key.key == m_forced) {
    return row.category == category || searching;
  }
  if (searching) {
    // By the plain names only (mitcad#655): a slicer's row by the key's
    // name or the slicer's.
    return row.key.name.contains(search, Qt::CaseInsensitive) ||
           (!row.slicer.isEmpty() && row.title.contains(search, Qt::CaseInsensitive));
  }
  // Nothing that changes the print is hidden: an advanced key the item
  // sets is shown. The slicers' presets a material builds on are always
  // offered (mitcad#1062: 3D Print points at them).
  return row.category == category &&
         (m_advanced->isChecked() || !row.key.advanced || !row.slicer.isEmpty() || row.field->isSet());
}

void SettingsForm::applyFilter() {
  if (m_page == nullptr) {
    return;
  }
  const QString search = m_search->text().trimmed();
  const bool searching = !search.isEmpty();
  // The search finds the window's own fields too (mitcad#655).
  bool generalHit = false;
  for (const QString& name : std::as_const(m_generalNames)) {
    generalHit = generalHit || (searching && name.contains(search, Qt::CaseInsensitive));
  }
  // The tabs with something to show; the window's own page always.
  QSet<QString> shownCategories;
  if (m_general != nullptr) {
    shownCategories.insert(QString::fromLatin1(kGeneralTab));
  }
  for (const Row& row : m_rows) {
    if (m_printShown && rowShown(row, false, QString(), row.category)) {
      shownCategories.insert(row.category);
    }
  }
  QStringList wanted;
  if (m_general != nullptr) {
    wanted << QString::fromLatin1(kGeneralTab);
  }
  for (const QString& id : std::as_const(m_categories)) {
    if (shownCategories.contains(id)) {
      wanted << id;
    }
  }
  // The window's page after the print tabs (mitcad#1054): with them.
  if (m_end != nullptr && m_printShown) {
    wanted << QString::fromLatin1(kEndTab);
  }
  syncTabs(wanted);
  // One tab is no choice (mitcad#655: a printer's one category, a metal's
  // General alone): no bar, nor while searching. Search stays on every
  // tab (it finds the own fields too), Advanced wherever there are print
  // settings.
  m_tabs->setVisible(!searching && m_tabs->count() > 1);
  m_tabs->updateGeometry();
  const QString category = m_currentTab;
  const bool general =
      m_general != nullptr && (searching ? generalHit : (!m_printShown || category == QLatin1String(kGeneralTab)));
  const bool end = m_end != nullptr && m_printShown && !searching && category == QLatin1String(kEndTab);
  m_search->setVisible(m_printShown || m_general != nullptr);
  m_advanced->setVisible(m_printShown && !end);
  if (m_general != nullptr) {
    // While searching, only the general fields found and no more room
    // than they take: the settings found below keep the rest (mitcad#1228).
    if (filterGeneral) {
      filterGeneral(searching && general ? search : QString());
    }
    static_cast<QVBoxLayout*>(layout())->setStretchFactor(m_general, searching ? 0 : 1);
    m_general->setVisible(general);
  }
  if (m_end != nullptr) {
    m_end->setVisible(end);
  }
  m_scroll->setVisible(searching ? m_printShown : (!general && !end));
  for (Group& group : m_groups) {
    bool any = false;
    for (int index : group.rows) {
      Row& row = m_rows[static_cast<std::size_t>(index)];
      const bool shown = m_printShown && rowShown(row, searching, search, category);
      row.line->setVisible(shown);
      any = any || shown;
    }
    group.heading->setText(searching ? QStringLiteral("%1 › %2").arg(group.categoryTitle, group.title) : group.title);
    group.heading->setVisible(any);
  }
  emit layoutChanged();
}

void SettingsForm::setGeneralPage(QWidget* page, const QString& title) {
  m_general = page;
  m_generalTitle = title;
  // Between the tabs and the settings, shown for its tab.
  auto* box = static_cast<QVBoxLayout*>(layout());
  box->insertWidget(box->indexOf(m_scroll), page, 1);
  m_currentTab = QString::fromLatin1(kGeneralTab);
  applyFilter();
}

void SettingsForm::setEndPage(QWidget* page, const QString& title) {
  m_end = page;
  m_endTitle = title;
  // Between the tabs and the settings, shown for its tab.
  auto* box = static_cast<QVBoxLayout*>(layout());
  box->insertWidget(box->indexOf(m_scroll), page, 1);
  page->hide();
  applyFilter();
}

void SettingsForm::setNamePrefix(const QString& prefix) {
  m_fieldPrefix = prefix.isEmpty() ? QStringLiteral("printSetting_") : prefix + QStringLiteral("Setting_");
  m_search->setObjectName(prefix.isEmpty() ? QStringLiteral("settingsSearch") : prefix + QStringLiteral("SettingsSearch"));
  m_tabs->setObjectName(prefix.isEmpty() ? QStringLiteral("settingsTabs") : prefix + QStringLiteral("SettingsTabs"));
}

void SettingsForm::setGeneralNames(const QStringList& names) {
  m_generalNames = names;
  applyFilter();
}

void SettingsForm::showGeneral() {
  if (m_general == nullptr) {
    return;
  }
  {
    const QSignalBlocker block(m_search);
    m_search->clear();
  }
  m_currentTab = QString::fromLatin1(kGeneralTab);
  applyFilter();
}

void SettingsForm::setPrintShown(bool shown) {
  if (shown == m_printShown) {
    return;
  }
  m_printShown = shown;
  applyFilter();
}

void SettingsForm::setPlaceholderValues(const QJsonObject& values, const QHash<QString, QString>& words) {
  for (Row& row : m_rows) {
    row.field->setPlaceholderValue(values.value(row.key.key), words.value(row.key.key));
  }
}

void SettingsForm::setOwnValues(const QHash<QString, QVector<QPair<QString, QJsonValue>>>& owners) {
  for (Row& row : m_rows) {
    QStringList entries;
    for (const auto& [name, value] : owners.value(row.key.key)) {
      // The one wording (mitcad#655): "4 on Body1".
      entries << tr("%1 on %2").arg(printValueText(row.key, value), name);
    }
    row.field->setOwnValues(entries);
  }
}

int SettingsForm::setHints(const QJsonObject& values, const QString& from, int general) {
  // Each tab with differences is marked (mitcad#655).
  QHash<QString, int>& counts = m_hintCounts;
  counts.clear();
  if (general > 0) {
    counts.insert(QString::fromLatin1(kGeneralTab), general);
  }
  int shownCount = 0;
  for (Row& row : m_rows) {
    if (!values.contains(row.key.key) || m_hidden.contains(row.key.key) || row.key.perDialect) {
      row.field->setHint(QString());
      continue;
    }
    const QJsonValue value = values.value(row.key.key);
    const QString shown =
        value.isNull() || value.isUndefined() ? tr("Slicer default") : printValueText(row.key, value);
    row.field->setHint(tr("Newer: %1").arg(shown), tr("%1 has %2").arg(from, shown));
    ++counts[row.category];
    ++shownCount;
  }
  const QSignalBlocker block(m_tabs);
  for (int i = 0; i < m_tabs->count(); ++i) {
    m_tabs->setTabText(i, tabTitle(m_tabs->tabData(i).toString()));
  }
  return shownCount;
}

void SettingsForm::focusKey(const QString& key) {
  for (Row& row : m_rows) {
    if (row.key.key != key) {
      continue;
    }
    m_forced = key;
    {
      const QSignalBlocker block(m_search);
      m_search->clear();
    }
    m_currentTab = row.category;
    applyFilter();
    m_scroll->ensureWidgetVisible(row.line);
    row.field->startEditing();
    return;
  }
}

QStringList SettingsForm::places(const QString& prefix,
                                 const std::function<QString(QWidget*, const QPoint&)>& at) const {
  QStringList lines;
  if (m_search->isVisibleTo(this)) {
    lines << QStringLiteral("%1 search at %2").arg(prefix, at(m_search, m_search->rect().center()));
  }
  if (m_advanced->isVisibleTo(this)) {
    lines << QStringLiteral("%1 advanced at %2").arg(prefix, at(m_advanced, m_advanced->rect().center()));
  }
  if (m_tabs->isVisible()) {
    for (int i = 0; i < m_tabs->count(); ++i) {
      lines << QStringLiteral("%1 tab %2 at %3")
                   .arg(prefix, m_tabs->tabData(i).toString(), at(m_tabs, m_tabs->tabRect(i).center()));
    }
  }
  if (!m_scroll->isVisibleTo(this)) {
    return lines; // the window's own page
  }
  const QRect view = m_scroll->viewport()->rect();
  const auto inSight = [this, &view](QWidget* widget) {
    return widget != nullptr && widget->isVisibleTo(m_scroll->viewport()) &&
           view.contains(widget->mapTo(m_scroll->viewport(), widget->rect().center()));
  };
  for (const Row& row : m_rows) {
    if (!row.line->isVisibleTo(m_page)) {
      continue;
    }
    const QString name = row.slicer.isEmpty() ? row.key.key : QStringLiteral("%1.%2").arg(row.key.key, row.slicer);
    QWidget* field = row.field->placeWidget();
    if (inSight(field)) {
      lines << QStringLiteral("%1 field %2 at %3").arg(prefix, name, at(field, field->rect().center()));
    }
    QWidget* clear = row.field->clearWidget();
    if (inSight(clear)) {
      lines << QStringLiteral("%1 clear %2 at %3").arg(prefix, name, at(clear, clear->rect().center()));
    }
  }
  return lines;
}

QStringList SettingsForm::rows() const {
  QStringList lines;
  for (const Row& row : m_rows) {
    if (!row.field->isSet()) {
      continue;
    }
    if (row.slicer.isEmpty()) {
      lines << QStringLiteral("%1 = %2").arg(row.key.key, printValueText(row.key, row.field->value()));
    } else {
      lines << QStringLiteral("%1.%2 = %3")
                   .arg(row.key.key, row.slicer, slicerPresetOf(row.field->value(), row.slicer));
    }
  }
  return lines;
}

bool SettingsForm::keyShown(const QString& key) const {
  for (const Row& row : m_rows) {
    if (row.key.key == key) {
      return row.line->isVisibleTo(m_page);
    }
  }
  return false;
}

QStringList SettingsForm::shownTabs() const {
  QStringList tabs;
  for (int i = 0; i < m_tabs->count(); ++i) {
    tabs << m_tabs->tabData(i).toString();
  }
  return tabs;
}

} // namespace mitcad
