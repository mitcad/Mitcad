// SPDX-License-Identifier: MIT
#include "ProfilesDialog.hpp"

#include <algorithm>
#include <exception>
#include <utility>

#include <QAbstractItemView>
#include <QElapsedTimer>
#include <QEvent>
#include <QFont>
#include <QHBoxLayout>
#include <QHash>
#include <QHeaderView>
#include <QIcon>
#include <QJsonValue>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QPoint>
#include <QPushButton>
#include <QRect>
#include <QSet>
#include <QSplitter>
#include <QStringList>
#include <QTreeWidget>
#include <QTreeWidgetItemIterator>
#include <QVBoxLayout>
#include <QtLogging>

#include "../browser/DocumentHost.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Icons.hpp"
#include "../framework/PanelTree.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../platform/MacDialogs.hpp"
#include "../platform/NativeDialogs.hpp"
#include "ProfilePickerRows.hpp"
#include "SettingsForm.hpp"
#include "SlicerImport.hpp"

namespace mitcad {
namespace {

constexpr int kKeyRole = Qt::UserRole; // a row's key; empty on a heading
// A place heading's count of items (a process under two printers once);
// a process row's and a printer heading's printer (mitcad#1216).
constexpr int kCountRole = Qt::UserRole + 1;
constexpr int kPrinterRole = Qt::UserRole + 2;

QString text(const QJsonObject& object, const char* field) {
  return object.value(QLatin1String(field)).toString();
}

bool isPrinter(const QString& kind) { return kind == QStringLiteral("printer"); }

} // namespace

ProfilesDialog::ProfilesDialog(DocumentHost& host, const QString& kind, QWidget* parent)
    : QDialog(parent), m_hosts(host), m_kind(kind),
      m_log(isPrinter(kind) ? QStringLiteral("Printers") : QStringLiteral("Processes")) {
  setWindowTitle(isPrinter(kind) ? tr("Printers") : tr("Processes"));
  setObjectName(isPrinter(kind) ? QStringLiteral("printerManager") : QStringLiteral("processManager"));
  // Its command's icon (mitcad#1096, mitcad#1135).
  setWindowIcon(themeIcon(isPrinter(kind) ? QStringLiteral("print-settings") : QStringLiteral("print-process")));
  resize(960, 620);
  m_hosts.choose();
  auto* layout = new QVBoxLayout(this);
  // Import from Slicer… (mitcad#829) into the place chosen in the box
  // before it, the default as New's (mitcad#1135); its line and Details…
  // before it.
  m_import = new SlicerImportBar(m_hosts.current(), m_log);
  m_import->target = [this] { return defaultPlace(m_hosts.onDesign()); };
  layout->addWidget(m_import);
  m_panel = new ProfilesPanel(m_hosts.current(), kind, m_log);
  layout->addWidget(m_panel, 1);
  // The first item of this kind the import brought, selected where it
  // went.
  connect(m_import, &SlicerImportBar::imported, this, [this](const SlicerImportResult& result) {
    if (!m_hosts.onDesign()) {
      m_changed = true;
    }
    QString id;
    const QJsonArray made =
        result.answer.value(isPrinter(m_kind) ? QStringLiteral("printers") : QStringLiteral("processes")).toArray();
    if (!made.isEmpty()) {
      id = text(made.first().toObject(), "id");
    }
    for (const QJsonValue& value : result.answer.value(QStringLiteral("items")).toArray()) {
      if (id.isEmpty() && text(value.toObject(), "kind") == m_kind) {
        id = text(value.toObject(), "id");
      }
    }
    if (id.isEmpty()) {
      m_panel->refresh();
      return;
    }
    m_panel->showItem(result.into + QLatin1Char('/') + id);
  });
  connect(m_panel, &ProfilesPanel::changed, this, [this] {
    if (!m_hosts.onDesign()) {
      m_changed = true;
    }
  });
  auto* close = new QPushButton(tr("Close"));
  close->setObjectName(QStringLiteral("printManagerClose"));
  m_panel->addButton(close, QStringLiteral("close"));
  mac::styleDialog(this);
  connect(close, &QPushButton::clicked, this, &QDialog::reject);
}

ProfilesDialog::~ProfilesDialog() {
  // The panel and the import before the models they work through.
  delete m_panel;
  delete m_import;
}

void ProfilesDialog::open(const QString& key) {
  if (m_hosts.choose()) {
    m_panel->setHost(m_hosts.current(), false);
    m_import->setHost(m_hosts.current());
  }
  m_import->setPlaces(m_panel->places(), defaultPlace(m_hosts.onDesign()));
  if (m_hosts.onDesign()) {
    m_panel->showItem(key);
    return;
  }
  // Without a design (mitcad#1135): the libraries are read off the UI
  // thread (mitcad#1018), the list fills when they came.
  const int reading = ++m_reads;
  readAhead(this, managerRequests(m_kind, m_hosts.library().projectFolder()),
            [this, reading, key](const QueryAnswers& answers, qint64 readMs) {
              if (reading != m_reads || m_hosts.onDesign()) {
                return;
              }
              m_hosts.library().setAnswers(answers);
              QElapsedTimer timer;
              timer.start();
              m_panel->showItem(key);
              qDebug().noquote() << QStringLiteral("%1 loaded: read %2 ms, list %3 ms")
                                        .arg(m_log)
                                        .arg(readMs)
                                        .arg(timer.elapsed());
            });
}

void ProfilesDialog::refresh() {
  // A design opened or changed meanwhile: on its model now.
  if (m_hosts.choose()) {
    m_panel->setHost(m_hosts.current(), false);
    m_import->setHost(m_hosts.current());
  }
  m_import->setPlaces(m_panel->places(), defaultPlace(m_hosts.onDesign()));
  ++m_reads; // read now: a reading under way shows nothing older
  m_panel->refresh();
}

void ProfilesDialog::hideEvent(QHideEvent* event) {
  QDialog::hideEvent(event);
  qDebug().noquote() << QStringLiteral("%1 closed%2").arg(m_log, m_changed ? QStringLiteral(" after changes")
                                                                          : QString());
  if (m_changed) {
    m_changed = false;
    emit libraryChanged();
  }
}

ProfilesPanel::ProfilesPanel(PrintItemsHost& host, const QString& kind, const QString& logName, QWidget* parent)
    : QWidget(parent), m_host(&host), m_kind(kind), m_log(logName) {
  auto* layout = new QVBoxLayout(this);
  layout->setContentsMargins(0, 0, 0, 0);
  auto* splitter = new QSplitter(Qt::Horizontal);
  splitter->setObjectName(QStringLiteral("printManagerSplit"));

  // The search across the groups, then the list grouped by place
  // (mitcad#1135).
  auto* side = new QWidget;
  auto* sideBox = new QVBoxLayout(side);
  sideBox->setContentsMargins(0, 0, 0, 0);
  m_search = new QLineEdit;
  m_search->setObjectName(QStringLiteral("printManagerSearch"));
  m_search->setPlaceholderText(isPrinter(kind) ? tr("Search printers") : tr("Search processes"));
  m_search->setAccessibleName(tr("Search"));
  m_search->setClearButtonEnabled(true);
  sideBox->addWidget(m_search);
  m_list = new QTreeWidget;
  m_list->setObjectName(QStringLiteral("printProfilesList"));
  m_list->setAccessibleName(isPrinter(kind) ? tr("Printers by place") : tr("Processes by place"));
  m_list->setColumnCount(2);
  m_list->setHeaderHidden(true);
  m_list->setUniformRowHeights(true);
  m_list->setSelectionMode(QAbstractItemView::SingleSelection);
  m_list->header()->setStretchLastSection(false);
  // The mark of the design's one right after the names (mitcad#655).
  m_list->header()->setSectionResizeMode(0, QHeaderView::ResizeToContents);
  m_list->header()->setSectionResizeMode(1, QHeaderView::Stretch);
  styleItemList(m_list);
  m_list->installEventFilter(this);
  mac::styleListTree(m_list);
  // The places' headings keep their fold arrows, in the grouped list's
  // step (mitcad#1214).
  panel::setCompactIndentation(m_list, panel::Indent::GroupedList);
  sideBox->addWidget(m_list, 1);
  splitter->addWidget(side);

  // The editor of the selected one: the header the managers share (its
  // name to edit, Built-in, Copy to), the line that says where it is
  // edited, then the settings.
  m_editor = new QWidget;
  auto* editorBox = new QVBoxLayout(m_editor);
  editorBox->setContentsMargins(8, 0, 0, 0);
  editorBox->addLayout(itemHeader(&m_name, &m_usedState, &m_builtIn, &m_headerDuplicate, &m_copyTo, &m_editing));
  m_name->setObjectName(QStringLiteral("printProfileName"));
  editorBox->addWidget(m_editing);
  m_emptyState = new QLabel(isPrinter(kind) ? tr("No printer selected") : tr("No process selected"));
  m_emptyState->setAlignment(Qt::AlignCenter);
  setRoleStyleSheet(m_emptyState, ColorRole::TextSecondary);
  m_emptyState->hide();
  editorBox->addWidget(m_emptyState, 1);
  m_form = new SettingsForm;
  editorBox->addWidget(m_form, 1);
  splitter->addWidget(m_editor);
  // A narrow list, the editor wide.
  splitter->setStretchFactor(0, 0);
  splitter->setStretchFactor(1, 1);
  splitter->setSizes({260, 700});
  layout->addWidget(splitter, 1);

  m_message = new QLabel;
  m_message->setWordWrap(true);
  layout->addWidget(m_message);
  // New, Duplicate and Delete under the list, as in Materials.
  m_buttons = new QHBoxLayout;
  m_new = new QPushButton(tr("New"));
  m_new->setObjectName(QStringLiteral("printManagerNew"));
  m_new->setToolTip(isPrinter(kind) ? tr("Makes a printer in the place you choose")
                                    : tr("Makes a process in the place you choose"));
  m_duplicate = new QPushButton(tr("Duplicate"));
  m_duplicate->setObjectName(QStringLiteral("printManagerDuplicate"));
  m_delete = new QPushButton(tr("Delete"));
  m_delete->setObjectName(QStringLiteral("printManagerDelete"));
  // The selected one as the design's printer or process (mitcad#655).
  m_use = new QPushButton(tr("Use in this design"));
  m_use->setObjectName(QStringLiteral("printManagerUse"));
  m_buttons->addWidget(m_new);
  m_buttons->addWidget(m_duplicate);
  m_buttons->addWidget(m_delete);
  m_buttons->addStretch(1);
  m_buttons->addWidget(m_use);
  layout->addLayout(m_buttons);
  // Enter in a field saves it: no button takes Enter.
  for (QPushButton* button : {m_headerDuplicate, m_copyTo, m_new, m_duplicate, m_delete, m_use}) {
    mac::auxiliaryButton(button);
  }

  connect(m_search, &QLineEdit::textChanged, this, [this](const QString& wanted) {
    rebuildList();
    int shown = 0;
    for (const LibraryItem& item : std::as_const(m_items)) {
      shown += matchesSearch(item, wanted) && m_places.contains(item.place) ? 1 : 0;
    }
    qDebug().noquote() << QStringLiteral("%1 search: %2, %3 shown").arg(m_log, wanted).arg(shown);
    logRows();
  });
  connect(m_list, &QTreeWidget::currentItemChanged, this, [this](QTreeWidgetItem* row) {
    if (m_building || row == nullptr || row->data(0, kKeyRole).toString().isEmpty()) {
      return;
    }
    m_currentKey = row->data(0, kKeyRole).toString();
    // The printer it was chosen under, where a process is listed twice
    // (mitcad#1216): its row there stays the selected one.
    m_currentPrinter = row->data(0, kPrinterRole).toString();
    const LibraryItem* chosen = current();
    m_loggedValues.clear();
    setMessage(QString());
    showCurrent();
    if (chosen != nullptr) {
      qDebug().noquote() << QStringLiteral("Profile selected: %1 %2 (%3)").arg(chosen->kind, chosen->name, chosen->place);
      if (!m_currentPrinter.isEmpty()) {
        qDebug().noquote() << QStringLiteral("%1 selected under: %2 › %3")
                                  .arg(m_log, placeName(chosen->place), m_currentPrinter);
      }
    }
    scheduleLogPlaces();
  });
  connect(m_name, &QLineEdit::editingFinished, this, [this] {
    const LibraryItem* item = current();
    const QString name = m_name->text().trimmed();
    if (item != nullptr && !item->readOnly && !name.isEmpty() && name != item->name) {
      edit({{QStringLiteral("name"), name}}, QStringLiteral("name = %1").arg(name));
    }
  });
  connect(m_form, &SettingsForm::valueChanged, this, [this](const QString& key, const QJsonValue& value) {
    const PrintKey* printKey = m_keys.find(key);
    const QString shown = value.isNull() ? QStringLiteral("cleared")
                                         : (printKey != nullptr ? printValueText(*printKey, value)
                                                                : value.toVariant().toString());
    edit({{QStringLiteral("values"), QJsonObject{{key, value}}}}, QStringLiteral("%1 = %2").arg(key, shown), key);
  });
  connect(m_form, &SettingsForm::layoutChanged, this, &ProfilesPanel::scheduleLogPlaces);
  connect(m_copyTo, &QPushButton::clicked, this, &ProfilesPanel::copyTo);
  connect(m_new, &QPushButton::clicked, this, &ProfilesPanel::newProfile);
  connect(m_duplicate, &QPushButton::clicked, this, [this] { duplicate(m_duplicate); });
  connect(m_headerDuplicate, &QPushButton::clicked, this, [this] { duplicate(m_headerDuplicate); });
  connect(m_use, &QPushButton::clicked, this, &ProfilesPanel::useInDesign);
  connect(m_delete, &QPushButton::clicked, this, &ProfilesPanel::remove);
}

ProfilesPanel::~ProfilesPanel() = default;

void ProfilesPanel::setHost(PrintItemsHost& host, bool read) {
  m_host = &host;
  m_currentKey.clear();
  m_currentPrinter.clear();
  m_loggedRows.clear();
  m_loggedValues.clear();
  m_loggedEditing.clear();
  m_keys = PrintKeyTable(); // asked of the new model
  if (read) {
    refresh();
  } else {
    m_items.clear();
    m_places.clear();
    rebuildList();
    showCurrent();
  }
}

QStringList ProfilesPanel::places() const { return writablePlaces(hasDesign(), !project().isEmpty()); }

void ProfilesPanel::addButton(QPushButton* button, const QString& name) {
  mac::auxiliaryButton(button);
  m_buttons->addWidget(button);
  m_windowButtons.append({name, button});
}

void ProfilesPanel::showItem(const QString& key) {
  m_currentKey = key;
  m_currentPrinter.clear(); // its first row
  m_loggedValues.clear();
  refresh();
  const LibraryItem* item = current();
  if (key.isEmpty()) {
    // From its own command: the design's printer or process.
    for (const LibraryItem& each : std::as_const(m_items)) {
      if (m_currentKey.isEmpty() && each.chosen) {
        m_currentKey = each.key();
      }
    }
  } else if (item != nullptr && !item->copy.isEmpty() && hasDesign()) {
    // A picker's key of a library item: the design's copy of it, which
    // the design prints with.
    m_currentKey = QStringLiteral("design/") + item->copy;
  }
  rebuildList();
  showCurrent();
  if (const LibraryItem* shown = current()) {
    if (m_list->currentItem() != nullptr) {
      m_list->scrollToItem(m_list->currentItem());
    }
    qDebug().noquote() << QStringLiteral("Profile selected: %1 %2 (%3)").arg(shown->kind, shown->name, shown->place);
  }
  scheduleLogPlaces();
}

const LibraryItem* ProfilesPanel::current() const {
  if (m_currentKey.isEmpty()) {
    return nullptr;
  }
  return findItem(m_items, m_currentKey);
}

void ProfilesPanel::refresh() {
  QString error;
  m_items.clear();
  QStringList order;
  for (const LibraryItem& item : queryPlaceItems(m_host->model(), m_kind, project(), &order, &error)) {
    // Without a design its model's empty design is no place (mitcad#1135).
    if (hasDesign() || item.place != QStringLiteral("design")) {
      m_items.append(item);
    }
  }
  if (order.isEmpty()) {
    order = writablePlaces(true, !project().isEmpty());
    order << QStringLiteral("mitcad");
  }
  if (!hasDesign()) {
    order.removeAll(QStringLiteral("design"));
  }
  m_places = order;
  try {
    m_profiles = m_host->model().queryArray(QStringLiteral("print_profiles"));
    // The design's slicer: whose percentages the fields offer (mitcad#655).
    m_dialect = designDialectOf(m_host->model());
    if (m_keys.isEmpty()) {
      m_keys = PrintKeyTable(m_host->model().queryObject({{QStringLiteral("query"), QStringLiteral("print_keys")}}));
    }
  } catch (const std::exception& e) {
    error = QString::fromUtf8(e.what());
  }
  if (!error.isEmpty()) {
    setMessage(error, true);
  }
  if (current() == nullptr) {
    m_currentKey.clear();
  }
  rebuildList();
  showCurrent();
  logRows();
  scheduleLogPlaces();
  if (!m_pendingFocus.isEmpty() && m_pendingFocus == m_currentKey) {
    m_pendingFocus.clear();
    m_name->setFocus(Qt::OtherFocusReason);
    m_name->selectAll();
    requestNativeFocus(m_name);
  }
}

void ProfilesPanel::rebuildList() {
  m_building = true;
  m_list->clear();
  QTreeWidgetItem* selected = nullptr;
  bool selectedUnderPrinter = false; // the selected key's row under m_currentPrinter
  const QString wanted = m_search->text();
  const auto addRow = [this, &selected, &selectedUnderPrinter](QTreeWidgetItem* under, const LibraryItem& item,
                                                               const QString& printer) {
    auto* row = new QTreeWidgetItem(under, {item.name, QString()});
    row->setData(0, kKeyRole, item.key());
    row->setData(0, kPrinterRole, printer);
    row->setData(0, NativeControlIdentifierRole, QStringLiteral("printManager.item.") + item.key());
    markInDesign(row, usedByDesign(item), tr("Used in this design"));
    // A process under two printers: its row under the printer it was
    // chosen under, else its first (mitcad#1216).
    if (item.key() == m_currentKey && !selectedUnderPrinter) {
      selectedUnderPrinter = !printer.isEmpty() && printer == m_currentPrinter;
      if (selected == nullptr || selectedUnderPrinter) {
        selected = row;
      }
    }
  };
  // Each place a heading with its count, its items by name under it, the
  // design's choice marked (mitcad#1135). Processes under their printers
  // within each place (mitcad#1216): a sub-group per printer, by name,
  // then Any printer; a process for several printers under each.
  const bool byPrinter = !isPrinter(m_kind);
  for (const QString& place : std::as_const(m_places)) {
    int count = 0;
    for (const LibraryItem& item : std::as_const(m_items)) {
      count += item.place == place && matchesSearch(item, wanted) ? 1 : 0;
    }
    auto* heading = new QTreeWidgetItem(m_list, {placeHeading(place, count)});
    heading->setData(0, NativeControlIdentifierRole, QStringLiteral("printManager.group.") + place);
    heading->setFlags(Qt::ItemIsEnabled);
    heading->setFirstColumnSpanned(true);
    heading->setData(1, kKeyRole, place);
    heading->setData(0, kCountRole, count);
    QFont bold = heading->font(0);
    bold.setBold(true);
    heading->setFont(0, bold);
    if (!byPrinter) {
      for (const LibraryItem& item : std::as_const(m_items)) {
        if (item.place == place && matchesSearch(item, wanted)) {
          addRow(heading, item, QString());
        }
      }
      continue;
    }
    // The printers of the place's items shown, each once, as the first
    // names it; an empty one for the items for any printer.
    QStringList printers;
    bool anyPrinter = false;
    for (const LibraryItem& item : std::as_const(m_items)) {
      if (item.place != place || !matchesSearch(item, wanted)) {
        continue;
      }
      const QStringList itemPrinters = processPrinters(item);
      anyPrinter = anyPrinter || itemPrinters.isEmpty();
      for (const QString& printer : itemPrinters) {
        const bool known = std::any_of(printers.cbegin(), printers.cend(),
                                       [&printer](const QString& other) { return samePrinterName(other, printer); });
        if (!known) {
          printers << printer;
        }
      }
    }
    std::sort(printers.begin(), printers.end(), [](const QString& a, const QString& b) {
      return QString::localeAwareCompare(a.toLower(), b.toLower()) < 0;
    });
    if (anyPrinter) {
      printers << QString();
    }
    for (const QString& printer : std::as_const(printers)) {
      const QString title = printer.isEmpty() ? anyPrinterTitle() : printer;
      auto* group = new QTreeWidgetItem(heading, {title});
      group->setFlags(Qt::ItemIsEnabled);
      group->setFirstColumnSpanned(true);
      group->setData(1, kKeyRole, place);
      group->setData(0, kPrinterRole, title);
      group->setData(0, NativeControlIdentifierRole,
                     QStringLiteral("printManager.printer.") + place + QLatin1Char('.') + title);
      group->setForeground(0, themeColor(ColorRole::TextSecondary, m_list->palette()));
      for (const LibraryItem& item : std::as_const(m_items)) {
        if (item.place != place || !matchesSearch(item, wanted)) {
          continue;
        }
        const QStringList itemPrinters = processPrinters(item);
        const bool forIt = printer.isEmpty() ? itemPrinters.isEmpty()
                                             : std::any_of(itemPrinters.cbegin(), itemPrinters.cend(),
                                                           [&printer](const QString& other) {
                                                             return samePrinterName(other, printer);
                                                           });
        if (forIt) {
          addRow(group, item, title);
        }
      }
    }
  }
  // Every group open, the selected item's sub-group too (mitcad#1216).
  m_list->expandAll();
  if (selected != nullptr) {
    m_list->setCurrentItem(selected);
  } else {
    m_list->setCurrentItem(nullptr);
  }
  m_building = false;
  scheduleLogPlaces();
}

void ProfilesPanel::showCurrent() {
  const LibraryItem* item = current();
  m_duplicate->setEnabled(item != nullptr);
  // Only for an item the design does not use (mitcad#655), nor the copy
  // of which it uses (mitcad#1135).
  m_use->setVisible(item != nullptr && hasDesign() && !usedByDesign(*item) && !item->copyUsed);
  m_delete->setEnabled(item != nullptr && !item->readOnly);
  // Nothing selected (Slicer default): a quiet line, never a blank side
  // or an empty disabled form (mitcad#655).
  m_emptyState->setVisible(item == nullptr);
  m_name->setVisible(item != nullptr);
  m_form->setVisible(item != nullptr);
  m_editing->setVisible(item != nullptr);
  if (item == nullptr) {
    m_name->clear();
    m_builtIn->hide();
    m_usedState->hide();
    m_copyTo->hide();
    m_editing->clear();
    // The fields are made for the first item shown (mitcad#1018: not
    // while the window opens with none selected).
    m_form->setValues({});
    return;
  }
  // Its own fields where it lives (mitcad#1135).
  const QJsonObject fields = itemFields(*item, m_profiles, QJsonObject());
  const bool editable = !item->readOnly;
  if (!m_name->hasFocus() || !m_name->isModified()) {
    m_name->setText(item->name);
    m_name->setModified(false);
  }
  m_name->setReadOnly(!editable);
  m_builtIn->setVisible(item->readOnly);
  m_usedState->setVisible(usedByDesign(*item));
  m_copyTo->setVisible(!copyTargets(*item, hasDesign(), !project().isEmpty()).isEmpty());
  // Where it is edited and whom an edit reaches, one line (mitcad#1135).
  const QString line = editingLine(*item);
  m_editing->setText(line);
  if (isVisible() && item->key() + line != m_loggedEditing) {
    m_loggedEditing = item->key() + line;
    qDebug().noquote() << QStringLiteral("%1 editing: %2").arg(m_log, line);
  }
  m_form->setKeys(m_keys, item->kind, m_dialect);
  m_form->setReadOnly(!editable);
  const QJsonObject values = fields.value(QStringLiteral("values")).toObject();
  m_form->setValues(values);
  // The design's profile: the own values bodies, occurrences or components
  // set show beside the fields, "4 on Body1" (mitcad#655).
  QHash<QString, QVector<QPair<QString, QJsonValue>>> ownValues;
  for (const QJsonValue& value : item->row.value(QStringLiteral("overrides")).toArray()) {
    const QJsonObject owner = value.toObject();
    const QJsonObject own = owner.value(QStringLiteral("values")).toObject();
    for (auto it = own.begin(); it != own.end(); ++it) {
      ownValues[it.key()].append({text(owner, "name"), it.value()});
    }
  }
  m_form->setOwnValues(ownValues);
  // Another design changed its library item: asked once, when the design
  // uses it or it is edited (mitcad#655), not on a mere selection.
  if (item->changed && !changeAsked(*item) && usedByDesign(*item)) {
    TestSync::singleShot(0, this, [this] { askChanges(); });
  }
  // What the editor shows, for UI tests.
  const QStringList shown = m_form->rows();
  const QString logged = QStringLiteral("%1 %2 (%3)\n").arg(item->kind, item->name, item->place) +
                         shown.join(QLatin1Char('\n'));
  if (isVisible() && logged != m_loggedValues) {
    m_loggedValues = logged;
    qDebug().noquote() << QStringLiteral("Profile shown: %1 %2 (%3)%4")
                              .arg(item->kind, item->name, item->place,
                                   item->readOnly ? QStringLiteral(", read only") : QString());
    for (const QString& value : shown) {
      qDebug().noquote() << QStringLiteral("Profile %1 value %2").arg(item->name, value);
    }
  }
}

bool ProfilesPanel::canChange() {
  if (m_host->canChangePrintSettings()) {
    return true;
  }
  setMessage(m_host->changeBlockedReason(), true);
  return false;
}

bool ProfilesPanel::run(const QJsonObject& command, QJsonObject* result) {
  if (m_host->runModelCommand(command, result)) {
    setMessage(QString());
    m_host->commandInputsChanged();
    emit changed();
    return true;
  }
  if (!m_host->lastCancelled()) {
    const LibraryItem* item = current();
    setMessage(plainModelError(m_host->lastError(), m_keys, command, item != nullptr ? item->name : QString()),
               true); // plain words (mitcad#655)
  }
  qDebug().noquote() << QStringLiteral("%1: %2 refused: %3").arg(m_log)
                            .arg(command.value(QStringLiteral("cmd")).toString(), m_host->lastError());
  return false;
}

void ProfilesPanel::setMessage(const QString& message, bool error) {
  m_message->setText(message);
  if (error) {
    setErrorStyleSheet(m_message);
  } else {
    m_message->setStyleSheet(QString());
  }
}

void ProfilesPanel::edit(const QJsonObject& change, const QString& what, const QString& field) {
  const LibraryItem* item = current();
  if (item == nullptr || item->readOnly) {
    return;
  }
  const LibraryItem target = *item;
  // Not while the field that changed still handles its signal.
  TestSync::singleShot(0, this, [this, target, change, what, field] {
    if (!canChange()) {
      refresh();
      return;
    }
    // Edited while another design's change waits: asked first
    // (mitcad#655).
    if (target.changed && !changeAsked(target)) {
      run(askChangedElsewhere(this, target, m_log, project()));
    }
    // In its own place (mitcad#1135): a library item by its library, a
    // design's item by its id.
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("edit_print_profile")}};
    nameItem(command, target, project());
    if (!target.library.isEmpty()) {
      command.insert(QStringLiteral("kind"), target.kind); // a library may have a printer and a process of the id
    }
    for (auto it = change.begin(); it != change.end(); ++it) {
      command.insert(it.key(), it.value());
    }
    const bool done = run(command);
    if (done) {
      qDebug().noquote() << QStringLiteral("Profile %1 (%2) %3").arg(target.name, target.place, what);
    }
    refresh();
    if (!done && !field.isEmpty() && !m_host->lastCancelled()) {
      // A refusal of a setting under its field (mitcad#655).
      setMessage(QString());
      m_form->setError(field, m_keys.plainText(m_host->lastError()));
    }
  });
}

QString ProfilesPanel::askWhere(QPushButton* anchor, const QString& what) {
  return choosePlaceAmong(anchor, QStringLiteral("%1 %2").arg(m_log, what), places(), defaultPlace(hasDesign()));
}

void ProfilesPanel::selectMade(const QString& place, const QString& id) {
  m_currentKey = place + QLatin1Char('/') + id;
  m_pendingFocus = m_currentKey;
  m_loggedValues.clear();
}

void ProfilesPanel::newProfile() {
  if (!canChange()) {
    return;
  }
  // Where it goes (mitcad#1135): in the design it is the design's choice in
  // the same undo step (mitcad#655); in a library a file of its own.
  const QString place = askWhere(m_new, QStringLiteral("new"));
  if (place.isEmpty()) {
    return;
  }
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("create_print_profile")}, {QStringLiteral("kind"), m_kind}};
  if (place == QStringLiteral("design")) {
    command.insert(QStringLiteral("choose"), true);
  } else {
    command.insert(QStringLiteral("library"), place);
    if (!project().isEmpty()) {
      command.insert(QStringLiteral("project"), project());
    }
  }
  QJsonObject result;
  if (run(command, &result)) {
    selectMade(place, text(result, "id"));
    qDebug().noquote() << QStringLiteral("Profile created: %1 %2 in %3").arg(m_kind, text(result, "id"), place);
  }
  refresh();
}

void ProfilesPanel::duplicate(QPushButton* anchor) {
  const LibraryItem* item = current();
  if (item == nullptr || !canChange()) {
    return;
  }
  const LibraryItem source = *item;
  const QString place = askWhere(anchor, QStringLiteral("duplicate"));
  if (place.isEmpty()) {
    return;
  }
  // The user's own copy (mitcad#655), where they chose (mitcad#1135): from
  // the design's item, or from a library's.
  const QJsonValue basedOn = source.library.isEmpty()
                                 ? QJsonValue(source.designId())
                                 : QJsonValue(QJsonObject{{QStringLiteral("library"), source.library},
                                                          {QStringLiteral("id"), source.id}});
  // The model names the copy after the source ("Standard (copy)").
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("create_print_profile")},
                      {QStringLiteral("kind"), source.kind},
                      {QStringLiteral("based_on"), basedOn}};
  if (place == QStringLiteral("design")) {
    command.insert(QStringLiteral("choose"), true);
  } else {
    command.insert(QStringLiteral("library"), place);
  }
  if (!project().isEmpty() && (basedOn.isObject() || place != QStringLiteral("design"))) {
    command.insert(QStringLiteral("project"), project());
  }
  QJsonObject result;
  if (!run(command, &result)) {
    refresh();
    return;
  }
  const QString id = text(result, "id");
  qDebug().noquote() << QStringLiteral("Profile duplicated: %1 as %2 in %3").arg(source.name, id, place);
  selectMade(place, id);
  refresh();
}

void ProfilesPanel::copyTo() {
  const LibraryItem* item = current();
  if (item == nullptr || !canChange()) {
    return;
  }
  const LibraryItem source = *item;
  // The other places (mitcad#1135); a copy keeps a link to its source only
  // as before (copyCommand).
  const QStringList targets = copyTargets(source, hasDesign(), !project().isEmpty());
  const QString place = choosePlaceAmong(m_copyTo, m_log + QStringLiteral(" copy_to"), targets, targets.value(0));
  if (place.isEmpty()) {
    return;
  }
  QJsonObject result;
  if (run(copyCommand(source, place, project()), &result)) {
    qDebug().noquote() << QStringLiteral("Profile copied to %1: %2").arg(place, source.name);
    // The copy selected; a design's item saved into a library stays
    // selected, its copy now.
    if (!text(result, "id").isEmpty()) {
      m_currentKey = place + QLatin1Char('/') + text(result, "id");
    }
    m_loggedValues.clear();
  }
  refresh();
}

void ProfilesPanel::useInDesign() {
  const LibraryItem* item = current();
  if (item == nullptr || item->chosen || !canChange()) {
    return;
  }
  const LibraryItem target = *item;
  // The root's choice, one undo step ("Use Process Standard"); a library's
  // item is copied in with it.
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_property")},
                      {QStringLiteral("owner"), QStringLiteral("C0")},
                      {QStringLiteral("set"), isPrinter(target.kind) ? QStringLiteral("Print Printer")
                                                                     : QStringLiteral("Print Process")},
                      {QStringLiteral("key"), QStringLiteral("profile")}};
  nameItem(command, target, project(), QStringLiteral("value"));
  if (run(command)) {
    qDebug().noquote() << QStringLiteral("Profile used in this design: %1 %2").arg(target.kind, target.name);
  }
  refresh();
}

void ProfilesPanel::remove() {
  const LibraryItem* item = current();
  if (item == nullptr || item->readOnly || !canChange()) {
    return;
  }
  const LibraryItem target = *item;
  // Always asked, naming what uses it (mitcad#655).
  if (!confirmDelete(this, target, m_log)) {
    return;
  }
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("delete_print_profile")}};
  nameItem(command, target, project());
  if (!target.library.isEmpty()) {
    command.insert(QStringLiteral("kind"), target.kind);
  }
  if (run(command)) {
    qDebug().noquote() << QStringLiteral("Profile deleted: %1 (%2)").arg(target.name, target.place);
  }
  refresh();
}

void ProfilesPanel::askChanges() {
  const LibraryItem* item = current();
  if (item == nullptr || !item->changed || changeAsked(*item) || !isVisible() ||
      !m_host->canChangePrintSettings()) {
    return;
  }
  const LibraryItem target = *item;
  // Use the changes or Keep these, one undo step either way (mitcad#655).
  const QJsonObject command = askChangedElsewhere(this, target, m_log, project());
  if (run(command)) {
    qDebug().noquote() << QStringLiteral("Profile %1: %2").arg(target.name, command.value(QStringLiteral("cmd")).toString());
  }
  m_loggedValues.clear();
  refresh();
}

void ProfilesPanel::logRows() {
  if (!isVisible()) {
    return;
  }
  QStringList rows;
  for (const LibraryItem& item : std::as_const(m_items)) {
    rows << QStringLiteral("%1/%2 %3 (%4)%5")
                .arg(item.kind, item.id, item.name, item.place, item.chosen ? QStringLiteral(", the design's") : QString());
  }
  // The groups as the list shows them (mitcad#1135), "This design 1 | ...".
  QStringList groups;
  // A process manager's sub-groups by printer (mitcad#1216): "Processes
  // printers: My library: Bench X 3 | Any printer 1", then each one's
  // items, "Processes printer group: My library › Bench X: Fine, Shared".
  QStringList printerLines;
  QStringList printerGroups;
  for (int i = 0; i < m_list->topLevelItemCount(); ++i) {
    QTreeWidgetItem* heading = m_list->topLevelItem(i);
    const QString place = placeName(heading->data(1, kKeyRole).toString());
    groups << QStringLiteral("%1 %2").arg(place).arg(heading->data(0, kCountRole).toInt());
    QStringList counts;
    for (int j = 0; j < heading->childCount(); ++j) {
      QTreeWidgetItem* group = heading->child(j);
      const QString printer = group->data(0, kPrinterRole).toString();
      if (!group->data(0, kKeyRole).toString().isEmpty() || printer.isEmpty()) {
        continue; // an item: no sub-groups
      }
      counts << QStringLiteral("%1 %2").arg(printer).arg(group->childCount());
      QStringList names;
      for (int k = 0; k < group->childCount(); ++k) {
        names << group->child(k)->text(0);
      }
      printerGroups << QStringLiteral("%1 › %2: %3").arg(place, printer, names.join(QStringLiteral(", ")));
    }
    if (!counts.isEmpty()) {
      printerLines << QStringLiteral("%1: %2").arg(place, counts.join(QStringLiteral(" | ")));
    }
  }
  const QString logged = rows.join(QLatin1Char('\n')) + QLatin1Char('|') + groups.join(QLatin1Char('|')) +
                         QLatin1Char('|') + printerGroups.join(QLatin1Char('\n'));
  if (logged == m_loggedRows) {
    return;
  }
  m_loggedRows = logged;
  qDebug().noquote() << QStringLiteral("%1: %2 items").arg(m_log).arg(rows.size());
  qDebug().noquote() << QStringLiteral("%1 groups: %2").arg(m_log, groups.join(QStringLiteral(" | ")));
  for (const QString& row : std::as_const(rows)) {
    qDebug().noquote() << QStringLiteral("%1 row: %2").arg(m_log).arg(row);
  }
  for (const QString& line : std::as_const(printerLines)) {
    qDebug().noquote() << QStringLiteral("%1 printers: %2").arg(m_log, line);
  }
  for (const QString& line : std::as_const(printerGroups)) {
    qDebug().noquote() << QStringLiteral("%1 printer group: %2").arg(m_log, line);
  }
}

void ProfilesPanel::showEvent(QShowEvent* event) {
  QWidget::showEvent(event);
  m_loggedRows.clear();
  m_loggedPlaces.clear();
  m_loggedValues.clear();
  m_loggedEditing.clear();
  logRows();
  showCurrent();
  scheduleLogPlaces();
}

void ProfilesPanel::resizeEvent(QResizeEvent* event) {
  QWidget::resizeEvent(event);
  scheduleLogPlaces();
}

bool ProfilesPanel::eventFilter(QObject* watched, QEvent* event) {
  // F2 on a row: its name.
  if (watched == m_list && event->type() == QEvent::KeyPress &&
      static_cast<QKeyEvent*>(event)->key() == Qt::Key_F2 && current() != nullptr && !current()->readOnly) {
    m_name->setFocus(Qt::OtherFocusReason);
    m_name->selectAll();
    requestNativeFocus(m_name);
    return true;
  }
  return QWidget::eventFilter(watched, event);
}

void ProfilesPanel::scheduleLogPlaces() {
  TestSync::singleShot(0, this, [this] { logPlaces(); });
}

void ProfilesPanel::logPlaces() {
  // In the main window's coordinates (the parent of the panel's window), as
  // the other logged places.
  if (!isVisible() || window()->parentWidget() == nullptr) {
    return;
  }
  const QPoint origin = window()->parentWidget()->window()->mapToGlobal(QPoint(0, 0));
  const auto at = [&origin](QWidget* widget, const QPoint& point) {
    const QPoint global = widget->mapToGlobal(point) - origin;
    return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
  };
  QStringList lines;
  // Each row by its place and name; by its name alone the first of that
  // name (This design's before a library's).
  QSet<QString> named;
  for (QTreeWidgetItemIterator it(m_list); *it != nullptr; ++it) {
    QTreeWidgetItem* row = *it;
    const QString key = row->data(0, kKeyRole).toString();
    const QRect rect = m_list->visualItemRect(row);
    if (key.isEmpty() || row->isHidden() || !rect.isValid() || !m_list->viewport()->rect().contains(rect.center())) {
      continue;
    }
    const QString place = at(m_list->viewport(), QPoint(60, rect.center().y()));
    if (!named.contains(row->text(0))) {
      named.insert(row->text(0));
      lines << QStringLiteral("%1 item %2 at %3").arg(m_log).arg(row->text(0), place);
    }
    lines << QStringLiteral("%1 %2 item %3 at %4").arg(m_log, key.section(QLatin1Char('/'), 0, 0), row->text(0), place);
    // Under a printer (mitcad#1216): "Processes design › Bench Y item
    // Shared at x,y", one line for each printer it is under.
    const QString printer = row->data(0, kPrinterRole).toString();
    if (!printer.isEmpty()) {
      lines << QStringLiteral("%1 %2 › %3 item %4 at %5")
                   .arg(m_log, key.section(QLatin1Char('/'), 0, 0), printer, row->text(0), place);
    }
  }
  QVector<std::pair<QString, QWidget*>> widgets{{QStringLiteral("search"), m_search},
                                               {QStringLiteral("name"), m_name},
                                               {QStringLiteral("copy_to"), m_copyTo},
                                               {QStringLiteral("header_duplicate"), m_headerDuplicate},
                                               {QStringLiteral("new"), m_new},
                                               {QStringLiteral("duplicate"), m_duplicate},
                                               {QStringLiteral("delete"), m_delete},
                                               {QStringLiteral("use"), m_use}};
  // The window's buttons (Close).
  for (const auto& [name, button] : std::as_const(m_windowButtons)) {
    widgets.append(std::pair<QString, QWidget*>(name, button));
  }
  for (const auto& [name, widget] : std::as_const(widgets)) {
    if (widget->isVisibleTo(this)) {
      lines << QStringLiteral("%1 %2 at %3").arg(m_log).arg(name, at(widget, widget->rect().center()));
    }
  }
  lines << m_form->places(m_log + QStringLiteral(" settings"), at);
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_loggedPlaces) {
    m_loggedPlaces = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

} // namespace mitcad
