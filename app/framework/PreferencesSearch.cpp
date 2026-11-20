// SPDX-License-Identifier: MIT
#include "PreferencesSearch.hpp"

#include <algorithm>
#include <map>
#include <utility>

#include <QAbstractButton>
#include <QCheckBox>
#include <QComboBox>
#include <QDebug>
#include <QEvent>
#include <QFormLayout>
#include <QGroupBox>
#include <QHash>
#include <QKeyEvent>
#include <QLabel>
#include <QLayout>
#include <QListWidget>
#include <QRadioButton>
#include <QResizeEvent>
#include <QScrollArea>
#include <QStackedWidget>
#include <QStyle>
#include <QTabBar>
#include <QTabWidget>
#include <QTableWidget>
#include <QTextDocumentFragment>

#include "Theme.hpp"

namespace mitcad {
namespace {

// Whether `widget` is shown on `root` once `root` is: no widget between
// them hidden on purpose. A sub-tab's page counts while another tab is
// shown (the search opens it); another stack's page only while it is the
// stack's current one.
bool shownOn(const QWidget* widget, const QWidget* root) {
  for (const QWidget* at = widget; at != nullptr && at != root; at = at->parentWidget()) {
    if (const auto* stack = qobject_cast<const QStackedWidget*>(at->parentWidget())) {
      if (qobject_cast<const QTabWidget*>(stack->parentWidget()) == nullptr && stack->currentWidget() != at) {
        return false;
      }
      continue;
    }
    // Before a window is first shown, every widget is hidden without
    // having been hidden on purpose.
    if (at->isHidden() && at->testAttribute(Qt::WA_WState_ExplicitShowHide)) {
      return false;
    }
  }
  return true;
}

// The sub-tab `widget` is on, the innermost one below `root`.
void tabOf(QWidget* widget, const QWidget* root, QPointer<QTabWidget>& tabs, int& tab) {
  for (QWidget* at = widget; at != nullptr && at != root; at = at->parentWidget()) {
    auto* stack = qobject_cast<QStackedWidget*>(at->parentWidget());
    auto* owner = stack != nullptr ? qobject_cast<QTabWidget*>(stack->parentWidget()) : nullptr;
    if (owner != nullptr && owner->indexOf(at) >= 0) {
      tabs = owner;
      tab = owner->indexOf(at);
      return;
    }
  }
}

// A setting's own text in logs: its label without the colon.
QString shownText(const QString& plain) {
  QString text = plain;
  while (text.endsWith(QLatin1Char(':'))) {
    text.chop(1);
  }
  return text.trimmed();
}

// What Return focuses for `widget`: itself, or the first of its children
// that takes the focus.
QWidget* focusTarget(QWidget* widget) {
  if (widget == nullptr) {
    return nullptr;
  }
  if ((widget->focusPolicy() & Qt::TabFocus) != 0 && widget->isEnabled()) {
    return widget;
  }
  const QList<QWidget*> children = widget->findChildren<QWidget*>();
  for (QWidget* child : children) {
    if ((child->focusPolicy() & Qt::TabFocus) != 0 && child->isEnabled()) {
      return child;
    }
  }
  return nullptr;
}

// A combo box's items, which are found too ("Dark" finds the theme).
QStringList itemsOf(const QWidget* widget) {
  QStringList items;
  if (const auto* combo = qobject_cast<const QComboBox*>(widget)) {
    for (int i = 0; i < combo->count(); ++i) {
      items << preferencesPlainText(combo->itemText(i));
    }
  }
  return items;
}

// Widgets that are entries of their own: check boxes, radio buttons, group
// boxes and the labels of a buddy.
bool ownEntry(const QWidget* widget) {
  if (qobject_cast<const QCheckBox*>(widget) != nullptr || qobject_cast<const QRadioButton*>(widget) != nullptr ||
      qobject_cast<const QGroupBox*>(widget) != nullptr) {
    return true;
  }
  const auto* label = qobject_cast<const QLabel*>(widget);
  return label != nullptr && label->buddy() != nullptr;
}

// The widgets of a form row's field: the widget, or those of its layout.
void widgetsOf(QLayoutItem* item, std::vector<QWidget*>& widgets) {
  if (item == nullptr) {
    return;
  }
  if (QWidget* widget = item->widget()) {
    widgets.push_back(widget);
  } else if (QLayout* layout = item->layout()) {
    for (int i = 0; i < layout->count(); ++i) {
      widgetsOf(layout->itemAt(i), widgets);
    }
  }
}

QString joined(const QStringList& texts) {
  QStringList kept;
  for (const QString& text : texts) {
    if (!text.trimmed().isEmpty()) {
      kept << text.trimmed();
    }
  }
  return kept.join(QLatin1Char(' '));
}

} // namespace

QStringList preferencesSearchWords(const QString& query) {
  return query.simplified().split(QLatin1Char(' '), Qt::SkipEmptyParts);
}

bool preferencesSearchMatches(const QString& text, const QStringList& words) {
  if (words.isEmpty()) {
    return false;
  }
  return std::all_of(words.begin(), words.end(),
                     [&text](const QString& word) { return text.contains(word, Qt::CaseInsensitive); });
}

QString preferencesSearchQueryFor(const QString& typed, const QString& commandName) {
  QStringList kept;
  for (const QString& word : preferencesSearchWords(typed)) {
    if (!commandName.contains(word, Qt::CaseInsensitive)) {
      kept << word;
    }
  }
  return kept.join(QLatin1Char(' '));
}

QString preferencesPlainText(const QString& text) {
  QString source = text;
  if (Qt::mightBeRichText(source)) {
    source = QTextDocumentFragment::fromHtml(source).toPlainText();
  }
  QString plain;
  plain.reserve(source.size());
  for (qsizetype i = 0; i < source.size(); ++i) {
    if (source.at(i) == QLatin1Char('&')) {
      // "&&" is an ampersand; a lone one marks the mnemonic.
      if (i + 1 < source.size() && source.at(i + 1) == QLatin1Char('&')) {
        plain += QLatin1Char('&');
        ++i;
      }
      continue;
    }
    plain += source.at(i);
  }
  return plain.simplified();
}

void PreferencesSearch::addPage(QWidget* page, const QString& name, const QStringList& keywords) {
  Page added;
  added.widget = page;
  added.name = name;
  QStringList words{name};
  for (QString keyword : keywords) {
    words << keyword.replace(QLatin1Char('_'), QLatin1Char(' '));
  }
  added.words = words.join(QLatin1Char(' '));
  m_pages.push_back(std::move(added));
  m_counts.push_back(0);
}

int PreferencesSearch::total() const {
  int sum = 0;
  for (int count : m_counts) {
    sum += count;
  }
  return sum;
}

std::vector<PreferencesSearch::Entry> PreferencesSearch::index(QWidget* page) {
  std::vector<Entry> entries;
  // The page's widgets in their order, which orders the entries.
  QHash<const QWidget*, int> order;
  const QList<QWidget*> widgets = page->findChildren<QWidget*>();
  for (int i = 0; i < widgets.size(); ++i) {
    order.insert(widgets.at(i), i + 1);
  }
  const auto add = [&](Entry entry, QWidget* anchor) {
    entry.order = order.value(anchor, 0);
    if (entry.tabs.isNull()) {
      tabOf(anchor, page, entry.tabs, entry.tab);
    }
    entries.push_back(std::move(entry));
  };
  QHash<const QLabel*, bool> used;

  // Form rows: the label, with its field's tooltips and items.
  const QList<QFormLayout*> forms = page->findChildren<QFormLayout*>();
  for (QFormLayout* form : forms) {
    for (int row = 0; row < form->rowCount(); ++row) {
      QLayoutItem* labelItem = form->itemAt(row, QFormLayout::LabelRole);
      auto* label = labelItem != nullptr ? qobject_cast<QLabel*>(labelItem->widget()) : nullptr;
      if (label == nullptr) {
        continue;
      }
      const QString text = preferencesPlainText(label->text());
      if (text.isEmpty() || !shownOn(label, page)) {
        continue;
      }
      used.insert(label, true);
      Entry entry;
      entry.paint = label;
      entry.shown = shownText(text);
      QStringList texts{text, label->toolTip()};
      std::vector<QWidget*> fields;
      widgetsOf(form->itemAt(row, QFormLayout::FieldRole), fields);
      // A control with a label of its own in the row ("Theme:" with
      // "Interface size:" beside it) is that label's.
      std::vector<const QWidget*> labelled;
      for (const QWidget* field : fields) {
        if (const auto* inner = qobject_cast<const QLabel*>(field); inner != nullptr && inner->buddy() != nullptr) {
          labelled.push_back(inner->buddy());
        }
      }
      for (QWidget* field : fields) {
        if (ownEntry(field) || std::find(labelled.begin(), labelled.end(), field) != labelled.end()) {
          continue;
        }
        texts << field->toolTip() << itemsOf(field);
        if (const auto* note = qobject_cast<const QLabel*>(field)) {
          texts << preferencesPlainText(note->text());
        }
        if (entry.control.isNull()) {
          entry.control = focusTarget(field);
        }
      }
      if (entry.control.isNull()) {
        entry.control = focusTarget(label->buddy());
      }
      entry.text = joined(texts);
      add(std::move(entry), label);
    }
  }

  // Labels of a buddy outside a form's label column ("Orbit center:").
  const QList<QLabel*> labels = page->findChildren<QLabel*>();
  for (QLabel* label : labels) {
    const QString text = preferencesPlainText(label->text());
    if (used.contains(label) || label->buddy() == nullptr || text.isEmpty() || !shownOn(label, page)) {
      continue;
    }
    Entry entry;
    entry.paint = label;
    entry.control = focusTarget(label->buddy());
    entry.shown = shownText(text);
    entry.text = joined(QStringList{text, label->toolTip(), label->buddy()->toolTip()} + itemsOf(label->buddy()));
    add(std::move(entry), label);
  }

  // Check boxes and radio buttons.
  const QList<QAbstractButton*> buttons = page->findChildren<QAbstractButton*>();
  for (QAbstractButton* button : buttons) {
    if (qobject_cast<QCheckBox*>(button) == nullptr && qobject_cast<QRadioButton*>(button) == nullptr) {
      continue;
    }
    const QString text = preferencesPlainText(button->text());
    if (text.isEmpty() || !shownOn(button, page)) {
      continue;
    }
    Entry entry;
    entry.paint = button;
    entry.control = button;
    entry.shown = text;
    entry.text = joined({text, button->toolTip()});
    add(std::move(entry), button);
  }

  // Group box titles (the dialog's; the Settings window's panes have none).
  const QList<QGroupBox*> boxes = page->findChildren<QGroupBox*>();
  for (QGroupBox* box : boxes) {
    const QString text = preferencesPlainText(box->title());
    if (text.isEmpty() || !shownOn(box, page)) {
      continue;
    }
    Entry entry;
    entry.paint = box;
    entry.control = box->isCheckable() ? box : focusTarget(box);
    entry.shown = text;
    entry.text = joined({text, box->toolTip()});
    add(std::move(entry), box);
  }

  // Sub-tabs by their names.
  const QList<QTabWidget*> tabWidgets = page->findChildren<QTabWidget*>();
  for (QTabWidget* tabs : tabWidgets) {
    if (!shownOn(tabs, page)) {
      continue;
    }
    for (int i = 0; i < tabs->count(); ++i) {
      Entry entry;
      entry.tabs = tabs;
      entry.tab = i;
      entry.control = tabs->tabBar();
      entry.shown = preferencesPlainText(tabs->tabText(i));
      entry.text = joined({entry.shown, tabs->tabToolTip(i)});
      add(std::move(entry), tabs->widget(i) != nullptr ? tabs->widget(i) : tabs);
    }
  }

  // Tables: the header's labels and each row (Navigation's bindings, whose
  // action is the combo box in the row: its choice, not every item).
  const QList<QTableWidget*> tables = page->findChildren<QTableWidget*>();
  for (QTableWidget* table : tables) {
    if (!shownOn(table, page)) {
      continue;
    }
    for (int column = 0; column < table->columnCount(); ++column) {
      const QTableWidgetItem* header = table->horizontalHeaderItem(column);
      if (header == nullptr || header->text().isEmpty()) {
        continue;
      }
      Entry entry;
      entry.table = table;
      entry.control = table;
      entry.shown = preferencesPlainText(header->text());
      entry.text = joined({entry.shown, header->toolTip()});
      add(std::move(entry), table);
    }
    for (int row = 0; row < table->rowCount(); ++row) {
      Entry entry;
      entry.table = table;
      entry.row = row;
      QStringList shown;
      QStringList texts;
      for (int column = 0; column < table->columnCount(); ++column) {
        if (const QTableWidgetItem* item = table->item(row, column)) {
          shown << preferencesPlainText(item->text());
          texts << item->toolTip();
        }
        if (QWidget* cell = table->cellWidget(row, column)) {
          if (const auto* combo = qobject_cast<const QComboBox*>(cell)) {
            shown << preferencesPlainText(combo->currentText());
          }
          texts << cell->toolTip();
          if (entry.control.isNull()) {
            entry.control = focusTarget(cell);
          }
        }
      }
      if (entry.control.isNull()) {
        entry.control = table;
      }
      entry.shown = shown.join(QStringLiteral(" / "));
      entry.text = joined(shown + texts);
      add(std::move(entry), table);
    }
  }

  std::stable_sort(entries.begin(), entries.end(),
                   [](const Entry& a, const Entry& b) { return a.order < b.order; });
  return entries;
}

void PreferencesSearch::mark(QWidget* widget) {
  if (widget == nullptr) {
    return;
  }
  widget->setProperty(kMatchProperty, true);
  m_flagged.emplace_back(widget);
}

void PreferencesSearch::paint(QWidget* widget, const QColor& color) {
  mark(widget);
  // A group box's palette would reach every control in it: its title is
  // counted and marked, not painted.
  if (qobject_cast<QGroupBox*>(widget) != nullptr) {
    return;
  }
  for (const PaintedWidget& done : m_painted) {
    if (done.widget == widget) {
      return;
    }
  }
  m_painted.push_back(PaintedWidget{widget, widget->testAttribute(Qt::WA_SetPalette), widget->palette()});
  QPalette palette = widget->palette();
  for (const QPalette::ColorGroup group : {QPalette::Active, QPalette::Inactive}) {
    palette.setColor(group, QPalette::WindowText, color);
    palette.setColor(group, QPalette::Text, color);
    palette.setColor(group, QPalette::ButtonText, color);
  }
  widget->setPalette(palette);
}

void PreferencesSearch::restore() {
  for (const PaintedWidget& done : m_painted) {
    if (!done.widget.isNull()) {
      // Back to the inherited palette unless the widget had its own.
      done.widget->setPalette(done.ownPalette ? done.palette : QPalette());
    }
  }
  for (const PaintedItem& done : m_paintedItems) {
    // A row rebuilt since (another navigation scheme) has new items.
    if (!done.table.isNull() && done.row < done.table->rowCount() &&
        done.table->item(done.row, done.column) == done.item) {
      done.item->setData(Qt::ForegroundRole, done.foreground);
    }
  }
  for (const MarkedTab& done : m_marked) {
    if (!done.tabs.isNull() && done.index < done.tabs->count()) {
      done.tabs->tabBar()->setTabTextColor(done.index, done.color);
    }
  }
  for (const QPointer<QWidget>& widget : m_flagged) {
    if (!widget.isNull()) {
      widget->setProperty(kMatchProperty, false);
    }
  }
  m_painted.clear();
  m_paintedItems.clear();
  m_marked.clear();
  m_flagged.clear();
}

const std::vector<int>& PreferencesSearch::apply(const QString& query) {
  restore();
  m_query = query;
  const QStringList words = preferencesSearchWords(query);
  m_counts.assign(m_pages.size(), 0);
  const QColor color = themeColor(ColorRole::SearchMatch);
  for (std::size_t p = 0; p < m_pages.size(); ++p) {
    Page& page = m_pages[p];
    page.matched.clear();
    page.entries.clear();
    if (words.isEmpty() || page.widget.isNull()) {
      continue;
    }
    // Made again at each search: the pages change while they are shown
    // (Navigation's bindings follow its scheme).
    page.entries = index(page.widget);
    for (std::size_t i = 0; i < page.entries.size(); ++i) {
      if (preferencesSearchMatches(page.entries[i].text, words)) {
        page.matched.push_back(int(i));
      }
    }
    m_counts[p] = !page.matched.empty() ? int(page.matched.size())
                                        : (preferencesSearchMatches(page.words, words) ? 1 : 0);
    std::map<QTabWidget*, std::vector<int>> hits;
    for (int i : page.matched) {
      const Entry& entry = page.entries[std::size_t(i)];
      if (!entry.paint.isNull()) {
        paint(entry.paint, color);
      }
      if (!entry.control.isNull()) {
        mark(entry.control);
      }
      if (!entry.table.isNull()) {
        mark(entry.table);
        for (int column = 0; entry.row >= 0 && column < entry.table->columnCount(); ++column) {
          if (QTableWidgetItem* item = entry.table->item(entry.row, column)) {
            m_paintedItems.push_back(
                PaintedItem{entry.table, entry.row, column, item, item->data(Qt::ForegroundRole)});
            item->setForeground(color);
          }
        }
      }
      if (!entry.tabs.isNull() && entry.tab >= 0) {
        hits[entry.tabs.data()].push_back(entry.tab);
      }
    }
    // The sub-tabs with a match marked; one of them opened when the tab
    // shown has none.
    for (auto& [tabs, indexes] : hits) {
      std::sort(indexes.begin(), indexes.end());
      indexes.erase(std::unique(indexes.begin(), indexes.end()), indexes.end());
      for (int tab : indexes) {
        m_marked.push_back(MarkedTab{tabs, tab, tabs->tabBar()->tabTextColor(tab)});
        tabs->tabBar()->setTabTextColor(tab, color);
      }
      if (std::find(indexes.begin(), indexes.end(), tabs->currentIndex()) == indexes.end()) {
        tabs->setCurrentIndex(indexes.front());
        qDebug().noquote() << QStringLiteral("Preferences search opens the tab %1 on %2")
                                  .arg(preferencesPlainText(tabs->tabText(indexes.front())), page.name);
      }
    }
    revealFirst(int(p));
  }
  return m_counts;
}

QStringList PreferencesSearch::matchTexts(int page) const {
  QStringList texts;
  if (page < 0 || page >= pageCount()) {
    return texts;
  }
  const Page& shown = m_pages[std::size_t(page)];
  for (int i : shown.matched) {
    texts << shown.entries[std::size_t(i)].shown;
  }
  return texts;
}

int PreferencesSearch::firstShown(const Page& page) const {
  for (int i : page.matched) {
    const Entry& entry = page.entries[std::size_t(i)];
    if (entry.tabs.isNull() || entry.tabs->currentIndex() == entry.tab) {
      return i;
    }
  }
  return page.matched.empty() ? -1 : page.matched.front();
}

QWidget* PreferencesSearch::revealFirst(int page) {
  if (page < 0 || page >= pageCount()) {
    return nullptr;
  }
  const int first = firstShown(m_pages[std::size_t(page)]);
  if (first < 0) {
    return nullptr;
  }
  const Entry& entry = m_pages[std::size_t(page)].entries[std::size_t(first)];
  if (!entry.tabs.isNull() && entry.tab >= 0 && entry.tabs->currentIndex() != entry.tab) {
    entry.tabs->setCurrentIndex(entry.tab);
  }
  if (!entry.table.isNull() && entry.row >= 0) {
    entry.table->scrollTo(entry.table->model()->index(entry.row, 0));
  }
  QWidget* anchor = !entry.paint.isNull()    ? entry.paint.data()
                    : !entry.table.isNull()  ? static_cast<QWidget*>(entry.table.data())
                                             : entry.control.data();
  for (QWidget* at = anchor != nullptr ? anchor->parentWidget() : nullptr; at != nullptr; at = at->parentWidget()) {
    if (auto* scroll = qobject_cast<QScrollArea*>(at)) {
      scroll->ensureWidgetVisible(anchor);
      break;
    }
    if (at == m_pages[std::size_t(page)].widget) {
      break;
    }
  }
  return entry.control.data();
}

PreferencesSearchField::PreferencesSearchField(QListWidget* list, std::function<void(int)> showPage,
                                               QWidget* parent)
    : QLineEdit(parent), m_list(list), m_showPage(std::move(showPage)) {
  setObjectName(QStringLiteral("preferencesSearch"));
  setPlaceholderText(tr("Search settings"));
  // As wide as the page list under it, whatever its text asks for.
  setSizePolicy(QSizePolicy::Ignored, sizePolicy().verticalPolicy());
  m_noMatch = new QLabel(this);
  m_noMatch->setObjectName(QStringLiteral("preferencesSearchNoMatch"));
  m_noMatch->setEnabled(false); // the quiet color of a hint
  m_noMatch->setAttribute(Qt::WA_TransparentForMouseEvents);
  m_noMatch->hide();
  connect(this, &QLineEdit::textChanged, this, [this](const QString& typed) { searchChanged(typed); });
  // A page the user goes to shows its first match.
  connect(m_list, &QListWidget::currentRowChanged, this, [this](int row) {
    if (row >= 0 && !preferencesSearchWords(text()).isEmpty()) {
      m_search.revealFirst(row);
    }
  });
}

QString PreferencesSearchField::noMatchText() { return tr("No setting matches"); }

void PreferencesSearchField::addPage(QWidget* page, const QString& name, const QStringList& keywords) {
  m_search.addPage(page, name, keywords);
  m_names << name;
}

void PreferencesSearchField::searchChanged(const QString& typed) {
  const std::vector<int>& counts = m_search.apply(typed);
  const bool searching = !preferencesSearchWords(typed).isEmpty();
  QStringList found;
  int first = -1;
  for (int row = 0; row < m_list->count() && row < int(counts.size()) && row < m_names.size(); ++row) {
    QListWidgetItem* item = m_list->item(row);
    const int count = counts[std::size_t(row)];
    item->setText(searching && count > 0 ? tr("%1 (%2)").arg(m_names.at(row)).arg(count) : m_names.at(row));
    // Greyed as a disabled entry, still clickable.
    item->setData(Qt::ForegroundRole, searching && count == 0
                                          ? QVariant(m_list->palette().brush(QPalette::Disabled, QPalette::Text))
                                          : QVariant());
    if (count > 0) {
      found << m_names.at(row);
      if (first < 0) {
        first = row;
      }
    }
  }
  m_noMatch->setVisible(searching && first < 0);
  placeNoMatch();
  const int shown = m_list->currentRow();
  if (first >= 0 && (shown < 0 || shown >= int(counts.size()) || counts[std::size_t(shown)] == 0)) {
    m_showPage(first);
    m_search.revealFirst(first);
  }
  if (!searching) {
    return;
  }
  qDebug().noquote() << QStringLiteral("Preferences search: \"%1\": %2 matches on %3")
                            .arg(typed.simplified())
                            .arg(m_search.total())
                            .arg(found.isEmpty() ? QStringLiteral("no page") : found.join(QStringLiteral(", ")));
  for (int row = 0; row < m_search.pageCount() && row < m_names.size(); ++row) {
    const QStringList texts = m_search.matchTexts(row);
    if (!texts.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Preferences search on %1: %2")
                                .arg(m_names.at(row), texts.join(QStringLiteral("; ")));
    }
  }
}

void PreferencesSearchField::focusFirstMatch() {
  const std::vector<int>& counts = m_search.counts();
  int row = m_list->currentRow();
  if (row < 0 || row >= int(counts.size()) || counts[std::size_t(row)] == 0) {
    const auto found = std::find_if(counts.begin(), counts.end(), [](int count) { return count > 0; });
    if (found == counts.end()) {
      return;
    }
    row = int(found - counts.begin());
    m_showPage(row);
  }
  if (QWidget* control = m_search.revealFirst(row)) {
    control->setFocus(Qt::OtherFocusReason);
    qDebug().noquote() << QStringLiteral("Preferences search: Return focuses %1 on %2")
                              .arg(control->objectName().isEmpty()
                                       ? QString::fromLatin1(control->metaObject()->className())
                                       : control->objectName(),
                                   m_names.value(row));
  }
}

bool PreferencesSearchField::event(QEvent* event) {
  // The keys the field takes itself are not shortcuts: Return and Esc
  // while there is text (not the dialog's OK and Cancel), Down.
  if (event->type() == QEvent::ShortcutOverride) {
    const auto* key = static_cast<QKeyEvent*>(event);
    const bool typed = !text().isEmpty();
    if (key->modifiers() == Qt::NoModifier &&
        (((key->key() == Qt::Key_Return || key->key() == Qt::Key_Enter) && !text().trimmed().isEmpty()) ||
         (key->key() == Qt::Key_Escape && typed) || key->key() == Qt::Key_Down)) {
      event->accept();
      return true;
    }
  }
  return QLineEdit::event(event);
}

void PreferencesSearchField::keyPressEvent(QKeyEvent* event) {
  if (event->modifiers() == Qt::NoModifier || event->modifiers() == Qt::KeypadModifier) {
    switch (event->key()) {
    case Qt::Key_Return:
    case Qt::Key_Enter:
      // An empty field leaves Return to the dialog (OK), as before.
      if (!text().trimmed().isEmpty()) {
        focusFirstMatch();
        event->accept();
        return;
      }
      break;
    case Qt::Key_Escape:
      // An empty field leaves Esc to the dialog (Cancel).
      if (!text().isEmpty()) {
        clear();
        event->accept();
        return;
      }
      break;
    case Qt::Key_Down:
      if (m_list != nullptr && m_list->count() > 0) {
        if (m_list->currentRow() < 0) {
          m_list->setCurrentRow(0);
        }
        m_list->setFocus(Qt::TabFocusReason);
        event->accept();
        return;
      }
      break;
    default:
      break;
    }
  }
  QLineEdit::keyPressEvent(event);
}

void PreferencesSearchField::resizeEvent(QResizeEvent* event) {
  QLineEdit::resizeEvent(event);
  placeNoMatch();
}

// "No setting matches" at the field's right, after what is typed, elided
// when the field is narrow.
void PreferencesSearchField::placeNoMatch() {
  if (m_noMatch->isHidden()) {
    setTextMargins(0, 0, 0, 0);
    return;
  }
  const int frame = style()->pixelMetric(QStyle::PM_DefaultFrameWidth, nullptr, this);
  const int gap = fontMetrics().averageCharWidth();
  const int room = std::max(0, width() / 2 - gap);
  const QString note = noMatchText();
  const int wanted = m_noMatch->fontMetrics().horizontalAdvance(note);
  const int shown = std::min(wanted, room);
  m_noMatch->setText(m_noMatch->fontMetrics().elidedText(note, Qt::ElideRight, shown));
  const int noteHeight = m_noMatch->sizeHint().height();
  m_noMatch->setGeometry(width() - frame - gap - shown, (height() - noteHeight) / 2, shown, noteHeight);
  setTextMargins(0, 0, shown + gap, 0);
}

} // namespace mitcad
