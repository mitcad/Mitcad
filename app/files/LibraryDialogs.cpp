// SPDX-License-Identifier: MIT
#include "LibraryDialogs.hpp"

#include <algorithm>
#include <utility>

#include <QAction>
#include <QApplication>
#include <QBuffer>
#include <QCheckBox>
#include <QClipboard>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QDir>
#include <QFileDialog>
#include <QFileInfo>
#include <QFormLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QInputDialog>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QMenu>
#include <QMessageBox>
#include <QPixmap>
#include <QPointer>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QRadioButton>
#include <QSignalBlocker>
#include <QSplitter>
#include <QTabWidget>
#include <QTableWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/Json.hpp"
#include "../library/PartPicker.hpp"
#include "../platform/MacDialogs.hpp"
#include "Libraries.hpp"
#include "LibraryAutoUpdate.hpp"
#include "RemoteTask.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad {
namespace {

QString errorOf(const QJsonObject& answer) {
  return answer.value(QStringLiteral("error")).toObject().value(QStringLiteral("message")).toString();
}

QString shortRev(const QString& rev) { return rev.left(7); }

// "v1.0.0 (3f9a2c1)" for a version of library_show's list.
QString versionText(const QJsonObject& version) { return str(version, "text"); }

QString escaped(const QString& text) { return text.toHtmlEscaped(); }

// A URL compared as written, without a trailing slash, separators of
// folders as '/'.
QString normalUrl(const QString& url) {
  QString out = QDir::fromNativeSeparators(url.trimmed());
  while (out.endsWith(QLatin1Char('/'))) {
    out.chop(1);
  }
  return out;
}

QJsonObject command(const char* name, QJsonObject fields = {}) {
  fields.insert(QStringLiteral("cmd"), QLatin1String(name));
  return libraryCommand(fields);
}

// The columns of Libraries...: On, Auto-update (mitcad#576), the source's
// URL, what was fetched from it.
constexpr int kOnColumn = 0;
constexpr int kAutoColumn = 1;
constexpr int kSourceColumn = 2;
constexpr int kLibraryColumn = 3;
constexpr int kVersionsColumn = 4;
constexpr int kLicenceColumn = 5;

QString autoUpdateTip() { return QObject::tr("Fetch this library when MitCad starts"); }

// Fetches the sources one after another (Fetch All in Libraries..., Update in
// the library browser, mitcad#317), then refreshes the dialog, and reports
// the failures in one warning. True when every source was fetched.
bool fetchAndReport(QWidget* parent, const QStringList& urls, const std::function<void()>& refresh) {
  const QList<QJsonObject> answers = fetchLibraries(parent, urls);
  QStringList failures;
  for (int i = 0; i < answers.size(); ++i) {
    const QString error = errorOf(answers[i]);
    if (!error.isEmpty()) {
      failures << QStringLiteral("%1: %2").arg(urls[i], error);
    }
  }
  qInfo().noquote() << QStringLiteral("Library fetch of %1 source(s): %2 failed")
                           .arg(urls.size())
                           .arg(failures.size());
  if (refresh) {
    refresh();
  }
  if (!failures.isEmpty()) {
    sheetWarning(parent, QObject::tr("Fetch Libraries"), failures.join(QStringLiteral("\n\n")));
  }
  return failures.isEmpty();
}

} // namespace

// ---------------------------------------------------------------------------
// LibrariesDialog

LibrariesDialog::LibrariesDialog(QWidget* parent) : QDialog(parent) {
  setWindowTitle(tr("Libraries"));
  resize(820, 420);
  auto* layout = new QVBoxLayout(this);
  auto* intro = new QLabel(tr("Component libraries and community indexes are git repositories. The parts a design "
                              "uses keep the version chosen for them; fetching only brings newer versions to "
                              "choose from. Libraries are fetched when you ask. A source with Auto-update on is "
                              "also fetched in the background when MitCad starts."));
  intro->setWordWrap(true);
  layout->addWidget(intro);
  m_table = new QTableWidget(0, 6);
  m_table->setObjectName(QStringLiteral("librarySources"));
  m_table->setHorizontalHeaderLabels(
      {tr("On"), tr("Auto-update"), tr("Source"), tr("Library"), tr("Versions"), tr("Licence")});
  m_table->horizontalHeaderItem(kAutoColumn)->setToolTip(autoUpdateTip());
  m_table->horizontalHeader()->setSectionResizeMode(kSourceColumn, QHeaderView::Stretch);
  m_table->setSelectionBehavior(QAbstractItemView::SelectRows);
  m_table->setEditTriggers(QAbstractItemView::NoEditTriggers);
  mac::styleListTable(m_table);
  layout->addWidget(m_table, 1);
  auto* buttons = new QHBoxLayout;
  auto* addUrlButton = new QAction(tr("&Add URL…"), this);
  auto* addFolder = new QAction(tr("Add F&older…"), this);
  auto* remove = new QPushButton(tr("&Remove"));
  auto* fetchSelected = new QPushButton(tr("Fetch &Selected"));
  auto* fetchAll = new QPushButton(tr("&Fetch All"));
#ifdef Q_OS_MACOS
  auto* add = new QPushButton(tr("Add Library"));
  add->setObjectName(QStringLiteral("libraryAddSource"));
  auto* addMenu = new QMenu(add);
  addMenu->addAction(addUrlButton);
  addMenu->addAction(addFolder);
  add->setMenu(addMenu);
  mac::styleListButtons(add, remove);
  mac::auxiliaryButton(add);
  auto* sourceButtons = new QHBoxLayout;
  sourceButtons->setSpacing(0);
  sourceButtons->addWidget(add);
  sourceButtons->addWidget(remove);
  buttons->addLayout(sourceButtons);
#else
  auto* addUrlControl = new QPushButton(addUrlButton->text());
  auto* addFolderControl = new QPushButton(addFolder->text());
  for (QPushButton* button : {addUrlControl, addFolderControl, remove}) {
    mac::auxiliaryButton(button);
    buttons->addWidget(button);
  }
  connect(addUrlControl, &QPushButton::clicked, addUrlButton, &QAction::trigger);
  connect(addFolderControl, &QPushButton::clicked, addFolder, &QAction::trigger);
#endif
  for (QPushButton* button : {remove, fetchSelected, fetchAll}) {
    mac::auxiliaryButton(button);
  }
  buttons->addStretch(1);
  buttons->addWidget(fetchSelected);
  buttons->addWidget(fetchAll);
  layout->addLayout(buttons);
  auto* close = new QDialogButtonBox(QDialogButtonBox::Close);
  mac::styleDialogButtons(close);
  layout->addWidget(close);
  connect(addUrlButton, &QAction::triggered, this, [this] {
    bool ok = false;
    const QString url = dialogText(this, tr("Add Library"),
                                              tr("A library's or a community index's git URL (https:// or ssh://):"),
                                              QString(), &ok);
    if (ok && !url.trimmed().isEmpty()) {
      addUrl(url);
    }
  });
  connect(addFolder, &QAction::triggered, this, [this] {
    const QString dir = dialogDirectory(this, tr("Library Folder"));
    if (!dir.isEmpty()) {
      addUrl(QDir::toNativeSeparators(dir));
    }
  });
  connect(remove, &QPushButton::clicked, this, [this] {
    const int row = m_table->currentRow();
    if (row < 0) {
      return;
    }
    LibrarySettings settings = LibrarySettings::load();
    if (row < settings.sources.size()) {
      qInfo().noquote() << QStringLiteral("Library source removed: %1").arg(settings.sources[row].url);
      settings.sources.removeAt(row);
      settings.save();
    }
    reload();
    if (m_table->rowCount() > 0) {
      m_table->setCurrentCell(std::min(row, m_table->rowCount() - 1), kSourceColumn);
    }
  });
  connect(fetchSelected, &QPushButton::clicked, this, [this] { fetch(false); });
  connect(fetchAll, &QPushButton::clicked, this, [this] { fetch(true); });
  connect(close, &QDialogButtonBox::rejected, this, &QDialog::reject);
  connect(m_table, &QTableWidget::itemChanged, this, [this](QTableWidgetItem* item) {
    if (item->column() == kOnColumn) {
      // A source that is off cannot be fetched at start-up: its
      // Auto-update is greyed out (and kept).
      greyAutoUpdate(item->row());
      save();
    } else if (item->column() == kAutoColumn) {
      save();
    }
  });
  // A background fetch that brought something new while the dialog is open.
  connect(&LibraryAutoUpdate::instance(), &LibraryAutoUpdate::updated, this, [this] { reload(); });
  mac::styleDialog(this);
  reload();
}

void LibrariesDialog::greyAutoUpdate(int row) {
  QTableWidgetItem* on = m_table->item(row, kOnColumn);
  QTableWidgetItem* autoUpdate = m_table->item(row, kAutoColumn);
  if (on == nullptr || autoUpdate == nullptr) {
    return;
  }
  const bool sourceOn = on->checkState() == Qt::Checked;
  const Qt::ItemFlags flags = sourceOn ? (Qt::ItemIsUserCheckable | Qt::ItemIsEnabled | Qt::ItemIsSelectable)
                                       : Qt::ItemFlags(Qt::ItemIsUserCheckable);
  if (autoUpdate->flags() != flags) {
    const QSignalBlocker blocker(m_table); // the flags are no change to save
    autoUpdate->setFlags(flags);
  }
}

void LibrariesDialog::reload() {
  const LibrarySettings settings = LibrarySettings::load();
  const QJsonArray fetched = command("library_list").value(QStringLiteral("libraries")).toArray();
  m_table->blockSignals(true);
  m_table->setRowCount(int(settings.sources.size()));
  for (int row = 0; row < settings.sources.size(); ++row) {
    const LibrarySource& source = settings.sources[row];
    QJsonObject info;
    for (const QJsonValue& value : fetched) {
      if (normalUrl(str(value.toObject(), "url")) == normalUrl(source.url)) {
        info = value.toObject();
      }
    }
    auto* on = new QTableWidgetItem;
    on->setFlags(Qt::ItemIsUserCheckable | Qt::ItemIsEnabled | Qt::ItemIsSelectable);
    on->setCheckState(source.enabled ? Qt::Checked : Qt::Unchecked);
    m_table->setItem(row, kOnColumn, on);
    // Auto-update (mitcad#576): greyed out while the source is off, its
    // state kept.
    auto* autoUpdate = new QTableWidgetItem;
    autoUpdate->setFlags(source.enabled ? (Qt::ItemIsUserCheckable | Qt::ItemIsEnabled | Qt::ItemIsSelectable)
                                        : Qt::ItemFlags(Qt::ItemIsUserCheckable));
    autoUpdate->setCheckState(source.autoUpdate ? Qt::Checked : Qt::Unchecked);
    autoUpdate->setToolTip(autoUpdateTip());
    m_table->setItem(row, kAutoColumn, autoUpdate);
    m_table->setItem(row, kSourceColumn, new QTableWidgetItem(source.url));
    QString what = tr("not fetched");
    QStringList versions;
    if (!info.isEmpty()) {
      what = str(info, "kind") == QLatin1String("index")
                 ? tr("Index: %1 (%2 libraries)").arg(str(info, "name")).arg(info.value(QStringLiteral("entries")).toInt())
                 : QStringLiteral("%1 (%2)").arg(str(info, "name"), str(info, "id"));
      for (const QJsonValue& version : info.value(QStringLiteral("versions")).toArray()) {
        versions << versionText(version.toObject());
      }
    }
    m_table->setItem(row, kLibraryColumn, new QTableWidgetItem(what));
    m_table->setItem(row, kVersionsColumn, new QTableWidgetItem(versions.join(QStringLiteral(", "))));
    m_table->setItem(row, kLicenceColumn, new QTableWidgetItem(info.value(QStringLiteral("license")).toString()));
    qInfo().noquote() << QStringLiteral("Library source %1 (%2): %3; versions %4; auto-update %5")
                             .arg(source.url, source.enabled ? QStringLiteral("on") : QStringLiteral("off"), what,
                                  versions.join(QStringLiteral(", ")),
                                  source.autoUpdate ? QStringLiteral("on") : QStringLiteral("off"));
  }
  m_table->blockSignals(false);
  m_table->resizeColumnToContents(kOnColumn);
  m_table->resizeColumnToContents(kAutoColumn);
  m_table->resizeColumnToContents(kLibraryColumn);
  if (m_table->rowCount() > 0 && m_table->currentRow() < 0) {
    m_table->setCurrentCell(0, kSourceColumn);
  }
}

void LibrariesDialog::addUrl(const QString& url) {
  LibrarySettings settings = LibrarySettings::load();
  if (settings.add(url)) {
    settings.save();
    qInfo().noquote() << QStringLiteral("Library source added: %1").arg(url.trimmed());
  }
  reload();
  m_table->setCurrentCell(m_table->rowCount() - 1, kSourceColumn);
}

void LibrariesDialog::save() {
  LibrarySettings settings = LibrarySettings::load();
  for (int row = 0; row < settings.sources.size() && row < m_table->rowCount(); ++row) {
    settings.sources[row].enabled = m_table->item(row, kOnColumn)->checkState() == Qt::Checked;
    const bool autoUpdate = m_table->item(row, kAutoColumn)->checkState() == Qt::Checked;
    if (autoUpdate != settings.sources[row].autoUpdate) {
      qInfo().noquote() << QStringLiteral("Library source %1: auto-update %2")
                               .arg(settings.sources[row].url,
                                    autoUpdate ? QStringLiteral("on") : QStringLiteral("off"));
    }
    settings.sources[row].autoUpdate = autoUpdate;
  }
  settings.save();
}

void LibrariesDialog::fetch(bool all) {
  save();
  const LibrarySettings settings = LibrarySettings::load();
  QStringList urls;
  if (all) {
    urls = settings.enabledUrls();
  } else if (m_table->currentRow() >= 0 && m_table->currentRow() < settings.sources.size()) {
    urls << settings.sources[m_table->currentRow()].url;
  }
  if (urls.isEmpty()) {
    return;
  }
  fetchAndReport(this, urls, [this] { reload(); });
}

// ---------------------------------------------------------------------------
// LibraryBrowser

LibraryBrowser::LibraryBrowser(bool community, QWidget* parent) : QDialog(parent), m_community(community) {
  setWindowTitle(community ? tr("Community Library") : tr("Insert from Library"));
  resize(1180, 680);
  auto* layout = new QVBoxLayout(this);
  // The results are the part picker's grid (mitcad#418): its search runs
  // library_search here (the licence filter beside it).
  m_grid = new PartGrid;
  m_grid->setExternalSearch(true);
  m_grid->view()->setObjectName(QStringLiteral("libraryResults"));
  m_search = m_grid->searchField();
  m_search->setObjectName(QStringLiteral("librarySearch"));
  m_search->setPlaceholderText(tr("Search: name, standard, size, tag (e.g. 4762, washer, M5)"));
  mac::styleSearchField(m_search, m_search->placeholderText());
  m_license = new QComboBox;
  m_license->setObjectName(QStringLiteral("libraryLicense"));
  const QStringList accepted = acceptedLicenses();
  m_license->addItem(tr("Accepted licences"), accepted);
  for (const QString& id : accepted) {
    m_license->addItem(id, QStringList{id});
  }
  m_license->addItem(tr("Any licence"), QVariant());
  m_license->setToolTip(tr("The licences a part may have to be shown"));
  m_unlicensed = new QCheckBox(tr("Show items &without a licence"));
  m_grid->addFilter(m_license);
  m_grid->addFilter(m_unlicensed);
  auto* split = new QSplitter;
  m_grid->setMinimumWidth(560);
  split->addWidget(m_grid);
  auto* right = new QWidget;
  auto* side = new QVBoxLayout(right);
  m_details = new QLabel;
  m_details->setWordWrap(true);
  m_details->setTextFormat(Qt::RichText);
  m_details->setAlignment(Qt::AlignTop | Qt::AlignLeft);
  m_details->setTextInteractionFlags(Qt::TextSelectableByMouse);
  m_details->setMinimumWidth(320);
  side->addWidget(m_details, 1);
  auto* form = new QFormLayout;
  m_version = new QComboBox;
  m_version->setObjectName(QStringLiteral("libraryVersion"));
  m_version->setToolTip(tr("The part keeps this version; it changes only with Library Parts › Update"));
  form->addRow(tr("&Version:"), m_version);
  side->addLayout(form);
  m_size = new SizeChooser;
  side->addWidget(m_size);
  // The row's designation and suppliers' part numbers (mitcad#949).
  m_rowDetails = new QLabel;
  m_rowDetails->setObjectName(QStringLiteral("libraryRowDetails"));
  m_rowDetails->setWordWrap(true);
  m_rowDetails->setTextFormat(Qt::RichText);
  m_rowDetails->setTextInteractionFlags(Qt::TextSelectableByMouse);
  m_rowDetails->hide();
  side->addWidget(m_rowDetails);
  m_link = new QRadioButton(tr("&Linked"));
  m_link->setToolTip(tr("Read only; follows the version chosen for it"));
  m_copy = new QRadioButton(tr("Co&py"));
  m_copy->setToolTip(tr("Editable, with its history"));
  m_link->setChecked(true);
  side->addWidget(m_link);
  side->addWidget(m_copy);
  split->addWidget(right);
  split->setStretchFactor(0, 2);
  split->setStretchFactor(1, 1);
  layout->addWidget(split, 1);
  auto* buttons = new QDialogButtonBox;
  auto* libraries = new QPushButton(tr("Li&braries…"));
  // Fetches every enabled source, as Fetch All in Libraries... (mitcad#317).
  auto* update = new QPushButton(tr("&Update"));
  update->setObjectName(QStringLiteral("libraryUpdate"));
  update->setToolTip(tr("Fetch newer versions of all enabled libraries"));
  m_get = new QPushButton(tr("&Get Library"));
  m_insert = new QPushButton(tr("&Insert"));
  auto* close = new QPushButton(tr("Close"));
  buttons->addButton(libraries, QDialogButtonBox::ResetRole);
  buttons->addButton(update, QDialogButtonBox::ResetRole);
  buttons->addButton(m_get, QDialogButtonBox::ActionRole);
  buttons->addButton(m_insert, QDialogButtonBox::AcceptRole);
  buttons->addButton(close, QDialogButtonBox::RejectRole);
  mac::styleDialogButtons(buttons, m_insert, true);
  layout->addWidget(buttons);
  m_get->setEnabled(false);
  m_insert->setEnabled(false);
  connect(m_grid, &PartGrid::searchChanged, this, &LibraryBrowser::search);
  connect(m_license, &QComboBox::currentIndexChanged, this, &LibraryBrowser::search);
  connect(m_unlicensed, &QCheckBox::toggled, this, &LibraryBrowser::search);
  connect(m_grid, &PartGrid::currentChanged, this, &LibraryBrowser::select);
  connect(m_version, &QComboBox::currentIndexChanged, this, &LibraryBrowser::showVersion);
  connect(m_grid, &PartGrid::activated, this, [this] {
    if (m_insert->isEnabled()) {
      insert();
    } else if (m_get->isEnabled()) {
      getLibrary();
    }
  });
  connect(m_insert, &QPushButton::clicked, this, &LibraryBrowser::insert);
  connect(m_get, &QPushButton::clicked, this, &LibraryBrowser::getLibrary);
  connect(close, &QPushButton::clicked, this, &QDialog::reject);
  connect(libraries, &QPushButton::clicked, this, [this] {
    LibrariesDialog dialog(this);
    prepareModal(&dialog);
    execDialog(&dialog);
    refresh();
  });
  connect(update, &QPushButton::clicked, this, &LibraryBrowser::updateLibraries);
  mac::styleDialog(this);
  connect(m_size, &SizeChooser::rowChanged, this, [this](const QString& row) {
    if (!row.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Library browser size %1").arg(row);
    }
    showRowDetails();
  });
  // The Standard and Supplier filters (mitcad#949): the designation follows
  // the standard; the filters' fields in the tab order.
  connect(m_size, &SizeChooser::filterChanged, this, [this] {
    showRowDetails();
    orderFields();
  });
  // The grid's supplier is the size's to start with.
  connect(m_grid, &PartGrid::supplierChanged, this, [this](const QString& supplier) {
    m_size->setSupplierFilter(supplier);
  });
  // An option chosen may make other fields (mitcad#810): the tab order again.
  connect(m_size, &SizeChooser::optionsChanged, this, &LibraryBrowser::orderFields);
  // A background fetch at start-up that brought something new while the
  // browser is open (mitcad#576): listed, the selection kept.
  connect(&LibraryAutoUpdate::instance(), &LibraryAutoUpdate::updated, this, [this] {
    qInfo().noquote() << QStringLiteral("Library browser: libraries updated in the background");
    refresh();
  });
  // Up and Down in the search field move through the results (the grid's).
  search();
  qInfo().noquote() << QStringLiteral("Part picker opened: %1 parts for %2").arg(m_items.size()).arg(windowTitle());
  m_search->setFocus();
}

void LibraryBrowser::setUsedParts(const QList<UsedPart>& used) { m_grid->setUsedParts(used); }

int LibraryBrowser::currentIndex() const {
  const QString key = m_grid->currentKey();
  return key.isEmpty() ? -1 : static_cast<int>(m_keys.indexOf(key));
}

void LibraryBrowser::search() {
  QJsonObject query{{QStringLiteral("text"), m_search->text()},
                    {QStringLiteral("unlicensed"), m_unlicensed->isChecked()}};
  const QVariant licenses = m_license->currentData();
  if (licenses.isValid()) {
    query.insert(QStringLiteral("licenses"), QJsonArray::fromStringList(licenses.toStringList()));
  }
  // The enabled sources (mitcad#949): a library an index offers that one
  // of them has under another id is left out, logged once.
  const QStringList sources = LibrarySettings::load().enabledUrls();
  query.insert(QStringLiteral("sources"), QJsonArray::fromStringList(sources));
  const QJsonObject answer = command("library_search", query);
  m_items = {};
  const QJsonArray components = answer.value(QStringLiteral("components")).toArray();
  QStringList leftOut;
  const QJsonArray libraries = browserLibraries(answer, sources, m_community, &leftOut);
  logLeftOutLibraries(leftOut);
  const auto addAll = [this](const QJsonArray& list, const char* kind) {
    for (const QJsonValue& value : list) {
      QJsonObject item = value.toObject();
      item.insert(QStringLiteral("item"), QLatin1String(kind));
      m_items.append(item);
    }
  };
  if (m_community) {
    addAll(libraries, "library");
    addAll(components, "component");
  } else {
    addAll(components, "component");
    addAll(libraries, "library");
  }
  // The grid's tiles: the components with their pictures (read as they come
  // into sight), an index's libraries in "Libraries to get".
  QList<PickerPart> parts;
  m_keys.clear();
  for (const QJsonValue& value : m_items) {
    const QJsonObject item = value.toObject();
    PickerPart part;
    if (str(item, "item") == QLatin1String("component")) {
      part = pickerPart(item);
    } else {
      part.library = str(item, "id");
      part.libraryName = str(item, "name");
      part.name = str(item, "name");
      part.url = str(item, "url");
      part.description = str(item, "description");
      part.license = item.value(QStringLiteral("license")).toString();
      part.preview = false;
      part.libraryEntry = true;
    }
    part.key = itemKey(item);
    m_keys << part.key;
    parts.append(part);
  }
  m_grid->setParts(parts);
  qInfo().noquote() << QStringLiteral("Library search '%1': %2 components, %3 libraries, %4 hidden by the licence")
                           .arg(m_search->text())
                           .arg(components.size())
                           .arg(libraries.size())
                           .arg(answer.value(QStringLiteral("hidden")).toInt());
  m_grid->selectFirst(false);
  select();
}

void LibraryBrowser::select() {
  const int row = currentIndex();
  m_version->blockSignals(true);
  m_version->clear();
  m_version->blockSignals(false);
  m_size->setTable({});
  m_rowDetails->hide();
  m_insert->setEnabled(false);
  m_get->setEnabled(false);
  m_shown = {};
  if (row < 0 || row >= m_items.size()) {
    // No enabled source fetched yet (a fresh install): Update fetches them
    // (mitcad#317); Libraries... adds sources.
    if (!anySourceFetched()) {
      m_details->setText(tr("<p>No library is fetched yet. Press <b>Update</b> to fetch the enabled libraries; "
                            "add sources in <b>Libraries…</b>.</p>"));
      qInfo().noquote() << QStringLiteral("Library browser empty: nothing fetched yet, press Update");
      return;
    }
    m_details->setText(tr("<p>Nothing found. Fetch newer versions with <b>Update</b> or add sources in "
                          "<b>Libraries…</b>; items without an accepted licence are hidden unless asked for.</p>"));
    qInfo().noquote() << QStringLiteral("Library browser empty: nothing found");
    return;
  }
  const QJsonObject item = m_items[row].toObject();
  if (str(item, "item") == QLatin1String("library")) {
    QStringList components;
    for (const QJsonValue& c : item.value(QStringLiteral("components")).toArray()) {
      components << escaped(str(c.toObject(), "name"));
    }
    QStringList reviewed;
    for (const QJsonValue& r : item.value(QStringLiteral("reviewed")).toArray()) {
      reviewed << escaped(str(r.toObject(), "label").isEmpty() ? shortRev(str(r.toObject(), "rev"))
                                                                 : str(r.toObject(), "label"));
    }
    const QString license = item.value(QStringLiteral("license")).toString();
    QStringList maintainers;
    for (const QJsonValue& m : item.value(QStringLiteral("maintainers")).toArray()) {
      maintainers << escaped(m.toString());
    }
    m_details->setText(tr("<h3>%1</h3><p>%2</p><p><b>Library</b> in %3<br>%4</p><p><b>Licence:</b> %5 %6</p>"
                          "<p><b>Maintainers:</b> %7</p><p><b>Components:</b> %8</p><p><b>Reviewed versions:</b> %9</p>")
                           .arg(escaped(str(item, "name")), escaped(str(item, "description")),
                                escaped(str(item, "index")), escaped(str(item, "url")),
                                license.isEmpty() ? tr("none") : escaped(license), escaped(licenseNote(license)),
                                maintainers.join(QStringLiteral(", ")), components.join(QStringLiteral(", ")),
                                reviewed.isEmpty() ? tr("none") : reviewed.join(QStringLiteral(", "))));
    m_get->setEnabled(!item.value(QStringLiteral("fetched")).toBool());
    qInfo().noquote() << QStringLiteral("Library browser selected library %1 (%2), licence %3, %4")
                             .arg(str(item, "id"), str(item, "url"), license.isEmpty() ? QStringLiteral("none") : license,
                                  item.value(QStringLiteral("fetched")).toBool() ? QStringLiteral("fetched")
                                                                                   : QStringLiteral("not fetched"));
    return;
  }
  const QJsonObject shown = command("library_show", {{QStringLiteral("url"), str(item, "url")},
                                                     {QStringLiteral("id"), str(item, "library")}});
  m_version->blockSignals(true);
  for (const QJsonValue& value : shown.value(QStringLiteral("versions")).toArray()) {
    const QJsonObject version = value.toObject();
    m_version->addItem(versionText(version), str(version, "rev"));
  }
  m_version->setCurrentIndex(std::max(0, m_version->findData(str(item, "rev"))));
  m_version->blockSignals(false);
  showVersion();
}

void LibraryBrowser::showVersion() {
  const int row = currentIndex();
  if (row < 0 || row >= m_items.size() || m_version->count() == 0) {
    return;
  }
  const QJsonObject item = m_items[row].toObject();
  const QString rev = m_version->currentData().toString();
  m_shown = command("library_show", {{QStringLiteral("url"), str(item, "url")},
                                     {QStringLiteral("id"), str(item, "library")},
                                     {QStringLiteral("rev"), rev}});
  QJsonObject component;
  for (const QJsonValue& value : m_shown.value(QStringLiteral("components")).toArray()) {
    if (str(value.toObject(), "id") == str(item, "id")) {
      component = value.toObject();
    }
  }
  if (component.isEmpty()) {
    m_details->setText(tr("<p>This version of the library has no %1.</p>").arg(escaped(str(item, "name"))));
    m_size->setTable({});
    m_insert->setEnabled(false);
    return;
  }
  const QJsonObject manifest = m_shown.value(QStringLiteral("manifest")).toObject();
  QStringList authors;
  for (const QJsonValue& a : manifest.value(QStringLiteral("authors")).toArray()) {
    authors << a.toString();
  }
  const QString license = component.value(QStringLiteral("license")).toString();
  QString attribution =
      tr("%1 from %2 by %3, %4")
          .arg(str(component, "name"), str(manifest, "name"),
               authors.isEmpty() ? tr("unknown authors") : authors.join(QStringLiteral(", ")),
               license.isEmpty() ? tr("no licence") : license);
  if (!str(m_shown, "url").isEmpty()) {
    attribution += QStringLiteral(", ") + str(m_shown, "url");
  }
  QStringList details{tr("<h3>%1</h3>").arg(escaped(str(component, "name")))};
  if (!str(component, "description").isEmpty()) {
    details << tr("<p>%1</p>").arg(escaped(str(component, "description")));
  }
  QStringList metadata;
  if (!str(component, "standard").isEmpty()) {
    metadata << tr("<b>Standard:</b> %1").arg(escaped(str(component, "standard")));
  }
  const QString libraryName = str(manifest, "name");
  const QString libraryId = str(manifest, "id");
  if (!libraryName.isEmpty() || !libraryId.isEmpty()) {
    QString library = libraryName;
    if (library.isEmpty()) {
      library = libraryId;
    } else if (!libraryId.isEmpty()) {
      library = tr("%1 (%2)").arg(libraryName, libraryId);
    }
    metadata << tr("<b>Library:</b> %1").arg(escaped(library));
  }
  if (!metadata.isEmpty()) {
    details << QStringLiteral("<p>%1</p>").arg(metadata.join(QStringLiteral("<br>")));
  }
  QString licence = tr("<b>Licence:</b> %1").arg(license.isEmpty() ? tr("none") : escaped(license));
  const QString note = licenseNote(license);
  if (!note.isEmpty()) {
    licence += QStringLiteral("<br>") + escaped(note);
  }
  details << QStringLiteral("<p>%1</p>").arg(licence);
  details << tr("<p><b>Attribution:</b> %1</p>").arg(escaped(attribution));
  m_details->setText(details.join(QString()));
  // An open table's options, filtered by the rules (mitcad#810): asked of
  // the library's component at the version shown.
  m_size->setOptionsQuery(libraryOptionsQuery(libraryCommand, str(item, "url"), str(item, "library"), rev,
                                              str(item, "id"), [this] { return m_size->standard(); }));
  // The Standard and Supplier filters (mitcad#949): the component's
  // standards in its order, the grid's supplier to start with.
  QJsonObject table = component.value(QStringLiteral("configurations")).toObject();
  table.insert(QStringLiteral("standards"), component.value(QStringLiteral("standards")));
  m_size->setSupplierFilter(m_grid->supplierFilter(), false);
  m_size->setTable(table);
  m_insert->setEnabled(true);
  showRowDetails();
  orderFields();
  qInfo().noquote() << QStringLiteral("Library browser selected %1/%2: version %3, size %4, licence %5")
                           .arg(str(item, "library"), str(item, "id"), m_version->currentText(),
                                m_size->row().isEmpty() ? QStringLiteral("-") : m_size->row(),
                                license.isEmpty() ? QStringLiteral("none") : license);
  if (m_size->isOpen()) {
    qInfo().noquote() << QStringLiteral("Library browser options of %1/%2: %3")
                             .arg(str(item, "library"), str(item, "id"), m_size->summary());
  }
}

void LibraryBrowser::showRowDetails() {
  const int index = currentIndex();
  const QString row = m_size->row();
  if (index < 0 || index >= m_items.size() || m_shown.isEmpty() || row.isEmpty()) {
    m_rowDetails->hide();
    return;
  }
  const QJsonObject item = m_items[index].toObject();
  // The designation as the part would get it: the library's own answer,
  // with the standard of the Standard filter.
  QJsonObject asked{{QStringLiteral("url"), str(item, "url")},
                    {QStringLiteral("id"), str(item, "library")},
                    {QStringLiteral("rev"), m_version->currentData().toString()},
                    {QStringLiteral("component"), str(item, "id")},
                    {QStringLiteral("config"), row}};
  if (!m_size->standard().isEmpty()) {
    asked.insert(QStringLiteral("standard"), m_size->standard());
  }
  if (m_size->isOpen()) {
    asked.insert(QStringLiteral("values"), m_size->parameters());
  }
  const QJsonObject configured = command("library_configure", asked);
  const QString designation = str(configured, "designation");
  QJsonArray parts = configured.value(QStringLiteral("supplier_parts")).toArray();
  if (parts.isEmpty()) {
    parts = m_size->filterRow(row).value(QStringLiteral("supplier_parts")).toArray();
  }
  const QString numbers = supplierPartsText(parts);
  QStringList lines;
  if (!designation.isEmpty()) {
    lines << tr("<b>Designation:</b> %1").arg(escaped(designation));
  }
  if (!numbers.isEmpty()) {
    lines << tr("<b>Supplier part numbers:</b> %1").arg(escaped(numbers));
  }
  m_rowDetails->setText(lines.join(QStringLiteral("<br>")));
  m_rowDetails->setVisible(!lines.isEmpty());
  qInfo().noquote() << QStringLiteral("Library browser row %1: designation %2; supplier part numbers %3")
                           .arg(row, designation.isEmpty() ? QStringLiteral("-") : designation,
                                numbers.isEmpty() ? QStringLiteral("none") : numbers);
}

void LibraryBrowser::orderFields() {
  // Tab order: results, version, sizes or options, linked or copy, the
  // buttons.
  QWidget* previous = m_version;
  for (QWidget* field : m_size->fields()) {
    setTabOrder(previous, field);
    previous = field;
  }
  setTabOrder(previous, m_link);
}

void LibraryBrowser::getLibrary() {
  const int row = currentIndex();
  if (row < 0 || row >= m_items.size()) {
    return;
  }
  const QJsonObject item = m_items[row].toObject();
  const QString url = str(item, "url");
  const QString license = item.value(QStringLiteral("license")).toString();
  // The address comes from an index: shown before anything is fetched.
  if (sheetQuestion(this, tr("Get Library"),
                    tr("Fetch the library %1 from\n%2\n\nLicence: %3\n\nIts designs are data from that repository; "
                       "nothing in it is run.")
                        .arg(str(item, "name"), url, license.isEmpty() ? tr("none") : license)) != QMessageBox::Yes) {
    return;
  }
  LibrarySettings settings = LibrarySettings::load();
  if (settings.add(url)) {
    settings.save();
  }
  const QList<QJsonObject> answers = fetchLibraries(this, {url});
  if (!answers.isEmpty() && !errorOf(answers.first()).isEmpty()) {
    sheetWarning(this, tr("Get Library"), errorOf(answers.first()));
  }
  refresh();
}

void LibraryBrowser::updateLibraries() {
  // Only when asked (the button): fetching brings newer versions to choose
  // from; parts already in a design keep theirs.
  const QStringList urls = LibrarySettings::load().enabledUrls();
  qInfo().noquote() << QStringLiteral("Library browser update: %1 enabled source(s)").arg(urls.size());
  if (urls.isEmpty()) {
    sheetInformation(this, tr("Update"), tr("No library source is enabled. Add or enable sources in Libraries…"));
    return;
  }
  fetchAndReport(this, urls, [this] { refresh(); });
}

void LibraryBrowser::refresh() {
  // The search again, keeping the selected item when it is still listed.
  QString kept;
  const int row = currentIndex();
  if (row >= 0 && row < m_items.size()) {
    kept = itemKey(m_items[row].toObject());
  }
  search();
  if (kept.isEmpty()) {
    return;
  }
  // Kept when the grid still shows it (its group and filters).
  if (m_grid->shownKeys().contains(kept)) {
    if (kept != m_grid->currentKey()) {
      m_grid->setCurrent(kept, true);
    }
    qInfo().noquote() << QStringLiteral("Library browser kept the selection %1").arg(kept);
  }
}

QString LibraryBrowser::itemKey(const QJsonObject& item) {
  return QStringList{str(item, "item"), normalUrl(str(item, "url")), str(item, "library"), str(item, "id")}.join(
      QLatin1Char('|'));
}

bool LibraryBrowser::anySourceFetched() {
  QStringList fetched;
  for (const QJsonValue& value : command("library_list").value(QStringLiteral("libraries")).toArray()) {
    fetched << normalUrl(str(value.toObject(), "url"));
  }
  const QStringList urls = LibrarySettings::load().enabledUrls();
  return std::any_of(urls.cbegin(), urls.cend(),
                     [&fetched](const QString& url) { return fetched.contains(normalUrl(url)); });
}

void LibraryBrowser::insert() {
  const int row = currentIndex();
  if (row < 0 || row >= m_items.size() || !m_insert->isEnabled()) {
    return;
  }
  const QJsonObject item = m_items[row].toObject();
  LibraryChoice choice;
  choice.library = str(item, "library");
  choice.url = str(item, "url");
  choice.rev = m_version->currentData().toString();
  choice.version = m_version->currentText();
  choice.component = str(item, "id");
  choice.name = str(item, "name");
  choice.config = m_size->row();
  // An open table: the options chosen (mitcad#810), the row their
  // combination matches as `config`, else none.
  choice.parameters = m_size->parameters();
  choice.license = item.value(QStringLiteral("license")).toString();
  // The designation follows the Standard filter (mitcad#949).
  choice.standard = m_size->standard();
  choice.link = m_link->isChecked();
  // Its mount frames at the version shown (mitcad#525).
  for (const QJsonValue& value : m_shown.value(QStringLiteral("components")).toArray()) {
    if (value.toObject().value(QStringLiteral("id")).toString() == choice.component) {
      choice.mounts = value.toObject().value(QStringLiteral("mounts")).toArray();
    }
  }
  m_choice = choice;
  qInfo().noquote() << QStringLiteral("Part picker chose %1/%2").arg(choice.library, choice.component);
  accept();
}

// ---------------------------------------------------------------------------
// LibraryPartsDialog

LibraryPartsDialog::LibraryPartsDialog(const QJsonArray& parts, const QJsonArray& list, QWidget* parent)
    : QDialog(parent) {
  setWindowTitle(tr("Library Parts"));
  resize(900, 560);
  auto* layout = new QVBoxLayout(this);
  auto* tabs = new QTabWidget;
  layout->addWidget(tabs, 1);

  auto* partsPage = new QWidget;
  auto* page = new QVBoxLayout(partsPage);
  auto* intro = new QLabel(tr("Each part keeps the library version chosen for it, also when the library has moved on. "
                              "Choose another version or size and Update; nothing changes on its own."));
  intro->setWordWrap(true);
  page->addWidget(intro);
  m_table = new QTableWidget(0, 6);
  m_table->setObjectName(QStringLiteral("libraryParts"));
  m_table->setHorizontalHeaderLabels({tr("Part"), tr("Library"), tr("Version"), tr("Size"), tr("Licence"), tr("Status")});
  m_table->horizontalHeader()->setSectionResizeMode(5, QHeaderView::Stretch);
  m_table->setSelectionBehavior(QAbstractItemView::SelectRows);
  m_table->setSelectionMode(QAbstractItemView::SingleSelection);
  m_table->setEditTriggers(QAbstractItemView::NoEditTriggers);
  mac::styleListTable(m_table);
  page->addWidget(m_table, 1);
  auto* editor = new QFormLayout;
  m_version = new QComboBox;
  m_version->setObjectName(QStringLiteral("libraryPartVersion"));
  editor->addRow(tr("&Version:"), m_version);
  page->addLayout(editor);
  m_size = new SizeChooser;
  page->addWidget(m_size);
  // The chosen part's suppliers' part numbers (mitcad#949).
  m_supplierParts = new QLabel;
  m_supplierParts->setObjectName(QStringLiteral("libraryPartSupplierParts"));
  m_supplierParts->setWordWrap(true);
  m_supplierParts->setTextInteractionFlags(Qt::TextSelectableByMouse);
  m_supplierParts->hide();
  page->addWidget(m_supplierParts);
  m_changes = new QPlainTextEdit;
  m_changes->setReadOnly(true);
  m_changes->setPlaceholderText(tr("Show Changes lists what an update changes."));
  m_changes->setMaximumHeight(140);
  page->addWidget(m_changes);
  auto* row = new QDialogButtonBox;
  auto* check = new QPushButton(tr("Check for &Newer Versions"));
  check->setToolTip(tr("Fetches the libraries these parts come from; nothing changes until you update"));
  m_get = new QPushButton(tr("&Get Missing Libraries"));
  auto* show = new QPushButton(tr("&Show Changes"));
  m_update = new QPushButton(tr("&Update"));
  row->addButton(check, QDialogButtonBox::ResetRole);
  row->addButton(m_get, QDialogButtonBox::ActionRole);
  row->addButton(show, QDialogButtonBox::ActionRole);
  row->addButton(m_update, QDialogButtonBox::AcceptRole);
  mac::styleDialogButtons(row, m_update);
  page->addWidget(row);
  tabs->addTab(partsPage, tr("Library Parts"));

  auto* listPage = new QWidget;
  auto* listLayout = new QVBoxLayout(listPage);
  auto* bom = new QTableWidget(0, 6);
  bom->setObjectName(QStringLiteral("partsList"));
  bom->setHorizontalHeaderLabels(
      {tr("Qty"), tr("Part"), tr("Library"), tr("Version"), tr("Licence"), tr("Supplier part numbers")});
  bom->horizontalHeader()->setSectionResizeMode(1, QHeaderView::Stretch);
  bom->setEditTriggers(QAbstractItemView::NoEditTriggers);
  mac::styleListTable(bom);
  bom->setRowCount(int(list.size()));
  QStringList csv{QStringLiteral("Quantity,Part,Library,Version,Licence,Supplier part numbers")};
  for (int i = 0; i < list.size(); ++i) {
    const QJsonObject r = list[i].toObject();
    // The suppliers' articles of the part's row (mitcad#949).
    const QString numbers = str(r, "supplier_parts_text").isEmpty()
                                ? supplierPartsText(r.value(QStringLiteral("supplier_parts")).toArray())
                                : str(r, "supplier_parts_text");
    const QStringList cells{QString::number(r.value(QStringLiteral("quantity")).toInt()),
                            str(r, "designation"),
                            str(r, "library"),
                            str(r, "version"),
                            str(r, "license"),
                            numbers};
    for (int c = 0; c < cells.size(); ++c) {
      bom->setItem(i, c, new QTableWidgetItem(cells[c]));
    }
    QStringList quoted;
    for (const QString& cell : cells) {
      quoted << QStringLiteral("\"%1\"").arg(QString(cell).replace(QLatin1Char('"'), QStringLiteral("\"\"")));
    }
    csv << quoted.join(QLatin1Char(','));
    qInfo().noquote() << QStringLiteral("Parts list row: %1 x %2%3%4")
                             .arg(cells[0], cells[1],
                                  cells[2].isEmpty() ? QString()
                                                     : QStringLiteral(" (%1 %2, %3)").arg(cells[2], cells[3], cells[4]),
                                  numbers.isEmpty() ? QString() : QStringLiteral("; %1").arg(numbers));
  }
  bom->resizeColumnsToContents();
  listLayout->addWidget(bom, 1);
  auto* copy = new QPushButton(tr("&Copy as CSV"));
  mac::auxiliaryButton(copy);
  listLayout->addWidget(copy, 0, Qt::AlignRight);
  connect(copy, &QPushButton::clicked, this, [csv] {
    QApplication::clipboard()->setText(csv.join(QLatin1Char('\n')) + QLatin1Char('\n'));
    qInfo().noquote() << QStringLiteral("Parts list copied as CSV: %1 rows").arg(csv.size() - 1);
  });
  tabs->addTab(listPage, tr("Parts List"));

  auto* close = new QDialogButtonBox(QDialogButtonBox::Close);
  mac::styleDialogButtons(close);
  layout->addWidget(close);
  const auto defaultForTab = [tabs, close, this] {
    const bool parts = tabs->currentIndex() == 0;
    QPushButton* closeButton = close->button(QDialogButtonBox::Close);
    closeButton->setAutoDefault(!parts);
    closeButton->setDefault(!parts);
    m_update->setAutoDefault(parts);
    m_update->setDefault(parts);
  };
  connect(tabs, &QTabWidget::currentChanged, this, defaultForTab);
  defaultForTab();
  mac::styleDialog(this);
  connect(close, &QDialogButtonBox::rejected, this, &QDialog::reject);
  connect(m_table, &QTableWidget::currentCellChanged, this, &LibraryPartsDialog::selectPart);
  connect(m_version, &QComboBox::currentIndexChanged, this, &LibraryPartsDialog::versionChosen);
  connect(m_size, &SizeChooser::rowChanged, this, [this](const QString& size) {
    const int index = m_table->currentRow();
    if (m_loading || index < 0 || index >= m_parts.size() || size.isEmpty()) {
      return;
    }
    m_parts[index].config = size;
    load(index);
  });
  connect(check, &QPushButton::clicked, this, &LibraryPartsDialog::checkNewer);
  connect(m_get, &QPushButton::clicked, this, &LibraryPartsDialog::getMissing);
  connect(show, &QPushButton::clicked, this, &LibraryPartsDialog::showChanges);
  connect(m_update, &QPushButton::clicked, this, [this] {
    if (changes().isEmpty()) {
      return;
    }
    showChanges();
    accept();
  });

  for (const QJsonValue& value : parts) {
    Part part;
    part.part = value.toObject();
    const QJsonObject library = part.part.value(QStringLiteral("library")).toObject();
    part.rev = str(library, "rev");
    part.config = str(library, "config");
    m_parts.append(part);
  }
  m_table->setRowCount(int(m_parts.size()));
  for (int i = 0; i < m_parts.size(); ++i) {
    load(i);
  }
  qInfo().noquote() << QStringLiteral("Library parts: %1; parts list: %2 rows").arg(m_parts.size()).arg(list.size());
  if (!m_parts.isEmpty()) {
    m_table->setCurrentCell(0, 0);
    selectPart();
  }
}

// Reads a part's library (its versions) and shows its row.
void LibraryPartsDialog::load(int index) {
  Part& part = m_parts[index];
  const QJsonObject library = part.part.value(QStringLiteral("library")).toObject();
  part.shown = command("library_show", {{QStringLiteral("url"), str(library, "url")},
                                        {QStringLiteral("id"), str(library, "library")}});
  const bool missing = !errorOf(part.shown).isEmpty();
  QString version = shortRev(part.rev);
  for (const QJsonValue& value : part.shown.value(QStringLiteral("versions")).toArray()) {
    if (str(value.toObject(), "rev") == part.rev) {
      version = versionText(value.toObject());
    }
  }
  const bool changed = part.rev != str(library, "rev") || part.config != str(library, "config");
  const bool linked = part.part.value(QStringLiteral("linked")).toBool();
  QString status;
  if (!linked) {
    status = tr("a copy: does not follow the library");
  } else if (missing) {
    status = tr("library not on this computer (the part keeps its saved bodies)");
  } else if (changed) {
    status = tr("to update");
  } else {
    const QJsonArray versions = part.shown.value(QStringLiteral("versions")).toArray();
    const bool newest = !versions.isEmpty() && str(versions.first().toObject(), "rev") == part.rev;
    status = newest ? tr("newest version") : tr("a newer version is fetched");
  }
  const QStringList cells{str(part.part, "name"), str(library, "library"), version, part.config,
                          str(library, "license"), status};
  for (int c = 0; c < cells.size(); ++c) {
    auto* item = m_table->item(index, c);
    if (item == nullptr) {
      m_table->setItem(index, c, new QTableWidgetItem(cells[c]));
    } else {
      item->setText(cells[c]);
    }
  }
  qInfo().noquote() << QStringLiteral("Library part %1: %2 %3, size %4, %5")
                           .arg(cells[0], cells[1], version, part.config.isEmpty() ? QStringLiteral("-") : part.config,
                                status);
  bool anyMissing = false;
  for (const Part& p : m_parts) {
    anyMissing = anyMissing || !errorOf(p.shown).isEmpty();
  }
  m_get->setEnabled(anyMissing);
  m_update->setEnabled(!changes().isEmpty());
}

void LibraryPartsDialog::selectPart() {
  const int index = m_table->currentRow();
  if (index < 0 || index >= m_parts.size()) {
    return;
  }
  m_loading = true;
  const Part& part = m_parts[index];
  const bool linked = part.part.value(QStringLiteral("linked")).toBool();
  m_version->clear();
  bool found = false;
  for (const QJsonValue& value : part.shown.value(QStringLiteral("versions")).toArray()) {
    const QJsonObject version = value.toObject();
    m_version->addItem(versionText(version), str(version, "rev"));
    found = found || str(version, "rev") == part.rev;
  }
  if (!found) {
    m_version->addItem(tr("%1 (recorded)").arg(shortRev(part.rev)), part.rev);
  }
  m_version->setCurrentIndex(m_version->findData(part.rev));
  m_version->setEnabled(linked && errorOf(part.shown).isEmpty());
  m_loading = false;
  versionChosen();
}

void LibraryPartsDialog::versionChosen() {
  const int index = m_table->currentRow();
  if (m_loading || index < 0 || index >= m_parts.size() || m_version->count() == 0) {
    return;
  }
  Part& part = m_parts[index];
  const QString rev = m_version->currentData().toString();
  const QJsonObject library = part.part.value(QStringLiteral("library")).toObject();
  const QJsonObject shown = command("library_show", {{QStringLiteral("url"), str(library, "url")},
                                                     {QStringLiteral("id"), str(library, "library")},
                                                     {QStringLiteral("rev"), rev}});
  QJsonObject component;
  for (const QJsonValue& value : shown.value(QStringLiteral("components")).toArray()) {
    if (str(value.toObject(), "id") == str(library, "component")) {
      component = value.toObject();
    }
  }
  m_loading = true;
  // The Standard and Supplier filters in the component's order (mitcad#949).
  QJsonObject table = component.value(QStringLiteral("configurations")).toObject();
  table.insert(QStringLiteral("standards"), component.value(QStringLiteral("standards")));
  m_size->setTable(table, part.config);
  m_size->setEnabled(part.part.value(QStringLiteral("linked")).toBool() && !component.isEmpty());
  // The suppliers' part numbers of the row chosen: the library's at this
  // version, else those recorded with the part.
  QJsonArray articles = m_size->filterRow(m_size->row().isEmpty() ? part.config : m_size->row())
                            .value(QStringLiteral("supplier_parts"))
                            .toArray();
  if (articles.isEmpty()) {
    articles = library.value(QStringLiteral("supplier_parts")).toArray();
  }
  const QString numbers = supplierPartsText(articles);
  m_supplierParts->setText(tr("Supplier part numbers: %1").arg(numbers));
  m_supplierParts->setVisible(!numbers.isEmpty());
  if (!numbers.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Library part %1 supplier part numbers: %2")
                             .arg(str(part.part, "name"), numbers);
  }
  // Tab order: the version, then the sizes.
  QWidget* previous = m_version;
  for (QComboBox* combo : m_size->combos()) {
    setTabOrder(previous, combo);
    previous = combo;
  }
  setTabOrder(previous, m_changes);
  m_loading = false;
  if (part.rev != rev || (!m_size->row().isEmpty() && m_size->row() != part.config)) {
    part.rev = rev;
    if (!m_size->row().isEmpty()) {
      part.config = m_size->row();
    }
    load(index);
  }
}

QList<LibraryPartChange> LibraryPartsDialog::changes() const {
  QList<LibraryPartChange> out;
  for (const Part& part : m_parts) {
    const QJsonObject library = part.part.value(QStringLiteral("library")).toObject();
    const bool rev = part.rev != str(library, "rev");
    const bool config = part.config != str(library, "config");
    if (rev || config) {
      out.append({str(part.part, "component"), rev ? part.rev : QString(), config ? part.config : QString()});
    }
  }
  return out;
}

void LibraryPartsDialog::showChanges() {
  QStringList lines;
  for (const Part& part : m_parts) {
    const QJsonObject library = part.part.value(QStringLiteral("library")).toObject();
    const QString from = str(library, "rev");
    if (part.rev == from && part.config == str(library, "config")) {
      continue;
    }
    const QString name = str(part.part, "name");
    if (part.config != str(library, "config")) {
      lines << tr("%1: size %2 -> %3").arg(name, str(library, "config"), part.config);
    }
    if (part.rev != from) {
      const QJsonObject diff =
          command("library_diff", {{QStringLiteral("id"), str(library, "library")},
                                   {QStringLiteral("url"), str(library, "url")},
                                   {QStringLiteral("from"), from},
                                   {QStringLiteral("to"), part.rev},
                                   {QStringLiteral("parts"),
                                    QJsonArray{QJsonObject{{QStringLiteral("component"), str(library, "component")},
                                                           {QStringLiteral("config"), part.config}}}}});
      if (!errorOf(diff).isEmpty()) {
        lines << tr("%1: %2").arg(name, errorOf(diff));
        continue;
      }
      const QString to = diff.value(QStringLiteral("to_labels")).toArray().isEmpty()
                             ? shortRev(part.rev)
                             : diff.value(QStringLiteral("to_labels")).toArray().first().toString();
      const QString was = diff.value(QStringLiteral("from_labels")).toArray().isEmpty()
                              ? shortRev(from)
                              : diff.value(QStringLiteral("from_labels")).toArray().first().toString();
      lines << tr("%1: version %2 -> %3").arg(name, was, to);
      for (const QJsonValue& line : diff.value(QStringLiteral("library")).toArray()) {
        lines << QStringLiteral("  ") + line.toString();
      }
      for (const QJsonValue& value : diff.value(QStringLiteral("parts")).toArray()) {
        lines << QStringLiteral("  ") + str(value.toObject(), "text");
      }
    }
  }
  if (lines.isEmpty()) {
    lines << tr("No changes chosen.");
  }
  m_changes->setPlainText(lines.join(QLatin1Char('\n')));
  for (const QString& line : lines) {
    qInfo().noquote() << QStringLiteral("Library changes: %1").arg(line.trimmed());
  }
}

void LibraryPartsDialog::checkNewer() {
  QStringList urls;
  for (const Part& part : m_parts) {
    const QString url = str(part.part.value(QStringLiteral("library")).toObject(), "url");
    if (errorOf(part.shown).isEmpty() && !urls.contains(url)) {
      urls << url;
    }
  }
  if (urls.isEmpty()) {
    return;
  }
  fetchLibraries(this, urls);
  const int current = m_table->currentRow();
  for (int i = 0; i < m_parts.size(); ++i) {
    load(i);
  }
  m_table->setCurrentCell(std::max(0, current), 0);
  selectPart();
}

void LibraryPartsDialog::getMissing() {
  QStringList urls;
  QStringList shown;
  for (const Part& part : m_parts) {
    const QJsonObject library = part.part.value(QStringLiteral("library")).toObject();
    if (!errorOf(part.shown).isEmpty() && !urls.contains(str(library, "url"))) {
      urls << str(library, "url");
      shown << QStringLiteral("%1 (%2, licence %3)").arg(str(library, "library"), str(library, "url"),
                                                          str(library, "license"));
    }
  }
  if (urls.isEmpty()) {
    return;
  }
  // The addresses come from the design file: shown before anything is fetched.
  if (sheetQuestion(this, tr("Get Missing Libraries"),
                    tr("The design uses libraries that are not on this computer:\n%1\n\nFetch them?")
                        .arg(shown.join(QLatin1Char('\n')))) != QMessageBox::Yes) {
    return;
  }
  LibrarySettings settings = LibrarySettings::load();
  for (const QString& url : urls) {
    settings.add(url);
  }
  settings.save();
  fetchLibraries(this, urls);
  for (int i = 0; i < m_parts.size(); ++i) {
    load(i);
  }
  selectPart();
}

// ---------------------------------------------------------------------------
// PublishLibraryDialog

PublishLibraryDialog::PublishLibraryDialog(std::function<QString()> design, std::function<QImage()> preview,
                                           QString author, QWidget* parent)
    : QDialog(parent), m_design(std::move(design)), m_preview(std::move(preview)), m_author(std::move(author)) {
  setWindowTitle(tr("Publish to Library"));
  resize(640, 640);
  auto* layout = new QVBoxLayout(this);
  auto* intro = new QLabel(tr("Adds this design to a library of your own: a folder that is a MitCad project with "
                              "version history. Push it to a git host, then others add its URL in Libraries; "
                              "a community index lists it when its maintainers take the entry."));
  intro->setWordWrap(true);
  layout->addWidget(intro);
  auto* where = new QFormLayout;
  m_folder = new QLineEdit;
  m_folder->setObjectName(QStringLiteral("publishFolder"));
  auto* browse = new QPushButton(tr("Bro&wse…"));
  mac::auxiliaryButton(browse);
  auto* folderRow = new QHBoxLayout;
  folderRow->addWidget(m_folder, 1);
  folderRow->addWidget(browse);
  auto* folderLabel = new QLabel(tr("Library &folder:"));
  folderLabel->setBuddy(m_folder);
  where->addRow(folderLabel, folderRow);
  m_status = new QLabel;
  where->addRow(QString(), m_status);
  layout->addLayout(where);

  auto* library = new QGroupBox(tr("New library"));
  mac::styleSection(library);
  auto* libraryForm = new QFormLayout(library);
  m_libraryId = new QLineEdit;
  m_libraryId->setPlaceholderText(tr("my-parts (a-z, 0-9, -)"));
  m_libraryName = new QLineEdit;
  m_libraryLicense = new QComboBox;
  m_libraryLicense->addItems(acceptedLicenses());
  m_libraryAuthor = new QLineEdit;
  libraryForm->addRow(tr("Library &id:"), m_libraryId);
  libraryForm->addRow(tr("Library &name:"), m_libraryName);
  libraryForm->addRow(tr("Licen&ce:"), m_libraryLicense);
  libraryForm->addRow(tr("&Author:"), m_libraryAuthor);
  layout->addWidget(library);

  auto* item = new QGroupBox(tr("This design as a component"));
  mac::styleSection(item);
  auto* itemForm = new QFormLayout(item);
  m_id = new QLineEdit;
  m_id->setPlaceholderText(tr("bracket-20 (a-z, 0-9, -)"));
  m_name = new QLineEdit;
  m_category = new QLineEdit;
  m_category->setPlaceholderText(tr("brackets"));
  m_standard = new QLineEdit;
  m_keywords = new QLineEdit;
  m_keywords->setPlaceholderText(tr("tags, separated by commas"));
  m_withPreview = new QCheckBox(tr("With an image of the &view as its preview"));
  m_withPreview->setChecked(true);
  itemForm->addRow(tr("Component i&d:"), m_id);
  itemForm->addRow(tr("Na&me:"), m_name);
  itemForm->addRow(tr("Cate&gory:"), m_category);
  itemForm->addRow(tr("&Standard:"), m_standard);
  itemForm->addRow(tr("&Tags:"), m_keywords);
  itemForm->addRow(QString(), m_withPreview);
  layout->addWidget(item);

  auto* remoteForm = new QFormLayout;
  m_remote = new QLineEdit;
  m_remote->setPlaceholderText(tr("https://host/you/my-parts.git (optional)"));
  remoteForm->addRow(tr("&Remote:"), m_remote);
  layout->addLayout(remoteForm);
  m_log = new QPlainTextEdit;
  m_log->setReadOnly(true);
  layout->addWidget(m_log, 1);
  auto* buttons = new QDialogButtonBox;
  auto* add = new QPushButton(tr("Add to &Library"));
  auto* push = new QPushButton(tr("&Push"));
  auto* entry = new QPushButton(tr("Inde&x Entry"));
  auto* close = new QPushButton(tr("Close"));
  buttons->addButton(add, QDialogButtonBox::AcceptRole);
  buttons->addButton(push, QDialogButtonBox::ActionRole);
  buttons->addButton(entry, QDialogButtonBox::ActionRole);
  buttons->addButton(close, QDialogButtonBox::RejectRole);
  mac::styleDialogButtons(buttons, add);
  layout->addWidget(buttons);
  connect(browse, &QPushButton::clicked, this, [this] {
    const QString dir = dialogDirectory(this, tr("Library Folder"), m_folder->text());
    if (!dir.isEmpty()) {
      m_folder->setText(QDir::toNativeSeparators(dir));
    }
  });
  connect(m_folder, &QLineEdit::textChanged, this, &PublishLibraryDialog::folderChanged);
  connect(add, &QPushButton::clicked, this, &PublishLibraryDialog::add);
  connect(push, &QPushButton::clicked, this, &PublishLibraryDialog::push);
  connect(entry, &QPushButton::clicked, this, &PublishLibraryDialog::indexEntry);
  connect(close, &QPushButton::clicked, this, &QDialog::accept);
  mac::styleDialog(this);
  folderChanged();
  m_folder->setFocus();
}

void PublishLibraryDialog::say(const QString& text) {
  m_log->appendPlainText(text);
  qInfo().noquote() << QStringLiteral("Publish: %1").arg(text);
}

void PublishLibraryDialog::folderChanged() {
  const QString dir = m_folder->text().trimmed();
  const QFileInfo manifest(QDir(dir).filePath(QStringLiteral("mitcad-library.json")));
  const bool exists = !dir.isEmpty() && manifest.isFile();
  m_libraryId->parentWidget()->setEnabled(!exists);
  if (dir.isEmpty()) {
    m_status->setText(tr("A folder for a new library, or one of yours."));
  } else if (exists) {
    const QJsonObject checked = command("library_check", {{QStringLiteral("dir"), dir}});
    m_status->setText(tr("Library %1 with %2 components").arg(str(checked, "id")).arg(checked.value(QStringLiteral("components")).toInt()));
  } else {
    m_status->setText(tr("A new library is made there."));
  }
}

void PublishLibraryDialog::add() {
  const QString dir = m_folder->text().trimmed();
  if (dir.isEmpty() || m_id->text().trimmed().isEmpty()) {
    say(tr("Give the library folder and the component's id."));
    return;
  }
  QStringList paths;
  if (!QFileInfo(QDir(dir).filePath(QStringLiteral("mitcad-library.json"))).isFile()) {
    const QJsonObject made = command(
        "library_init", {{QStringLiteral("dir"), dir},
                         {QStringLiteral("id"), m_libraryId->text().trimmed()},
                         {QStringLiteral("name"), m_libraryName->text().trimmed()},
                         {QStringLiteral("license"), m_libraryLicense->currentText()},
                         {QStringLiteral("authors"), m_libraryAuthor->text().trimmed().isEmpty()
                                                         ? QJsonArray()
                                                         : QJsonArray{m_libraryAuthor->text().trimmed()}}});
    if (!errorOf(made).isEmpty()) {
      say(tr("The library could not be made: %1").arg(errorOf(made)));
      return;
    }
    for (const QJsonValue& path : made.value(QStringLiteral("written")).toArray()) {
      paths << path.toString();
    }
    say(tr("Made the library %1 in %2").arg(str(made, "id"), dir));
  }
  QJsonObject fields{{QStringLiteral("dir"), dir},
                     {QStringLiteral("text"), m_design()},
                     {QStringLiteral("id"), m_id->text().trimmed()},
                     {QStringLiteral("name"), m_name->text().trimmed().isEmpty() ? m_id->text().trimmed()
                                                                                  : m_name->text().trimmed()},
                     {QStringLiteral("category"), m_category->text().trimmed()},
                     {QStringLiteral("standard"), m_standard->text().trimmed()}};
  QJsonArray keywords;
  for (const QString& keyword : m_keywords->text().split(QLatin1Char(','), Qt::SkipEmptyParts)) {
    keywords.append(keyword.trimmed());
  }
  fields.insert(QStringLiteral("keywords"), keywords);
  if (m_withPreview->isChecked()) {
    QImage image = m_preview();
    if (!image.isNull()) {
      image = image.scaled(256, 256, Qt::KeepAspectRatio, Qt::SmoothTransformation);
      QByteArray png;
      QBuffer buffer(&png);
      buffer.open(QIODevice::WriteOnly);
      image.save(&buffer, "PNG");
      fields.insert(QStringLiteral("preview"), QString::fromLatin1(png.toBase64()));
    }
  }
  const QJsonObject added = command("library_add", fields);
  if (!errorOf(added).isEmpty()) {
    say(tr("The design could not be added: %1").arg(errorOf(added)));
    return;
  }
  for (const QJsonValue& path : added.value(QStringLiteral("written")).toArray()) {
    if (!paths.contains(path.toString())) {
      paths << path.toString();
    }
  }
  for (const QJsonValue& error : added.value(QStringLiteral("errors")).toArray()) {
    say(tr("Error: %1").arg(error.toString()));
  }
  for (const QJsonValue& warning : added.value(QStringLiteral("warnings")).toArray()) {
    say(tr("Warning: %1").arg(warning.toString()));
  }
  // Recorded as a version of the library's project.
  QJsonArray files;
  for (const QString& path : paths) {
    files.append(QDir(dir).filePath(path));
  }
  try {
    const rust::Box<Project> project = open_project(rustStr(dir.toUtf8()));
    QJsonObject commit{{QStringLiteral("cmd"), QStringLiteral("commit")},
                       {QStringLiteral("paths"), files},
                       {QStringLiteral("message"), tr("Add %1").arg(m_id->text().trimmed())}};
    if (!m_author.isEmpty()) {
      commit.insert(QStringLiteral("fallback_author"), m_author);
    }
    const QJsonObject answer = parseObject(project->command(rustStr(compactJson(commit))));
    say(tr("Added %1 to the library, recorded as version %2")
            .arg(m_id->text().trimmed(), shortRev(answer.value(QStringLiteral("commit")).toString())));
  } catch (const std::exception& e) {
    say(tr("Added %1 to the library; not recorded as a version: %2").arg(m_id->text().trimmed(), errorText(e)));
  }
  folderChanged();
}

void PublishLibraryDialog::push() {
  const QString dir = m_folder->text().trimmed();
  if (dir.isEmpty()) {
    return;
  }
  const QString url = m_remote->text().trimmed();
  QJsonObject remoteCommand{{QStringLiteral("cmd"), QStringLiteral("push")}};
  if (!url.isEmpty()) {
    remoteCommand = {{QStringLiteral("cmd"), QStringLiteral("connect")}, {QStringLiteral("url"), url}};
    if (!m_author.isEmpty()) {
      remoteCommand.insert(QStringLiteral("fallback_author"), m_author);
    }
  }
  RemoteTask* task = RemoteTask::command(dir, remoteCommand, nullptr);
  QObject waiting;
  bool finished = false;
  const QPointer<RemoteTask> pending = task;
  const QPointer<PublishLibraryDialog> presenter = this;
  connect(task, &RemoteTask::finished, &waiting, [&finished] { finished = true; });
  connect(this, &QObject::destroyed, task, &RemoteTask::cancel);
  say(tr("Pushing %1…").arg(dir));
  task->start();
  waitDialogUntil([&] { return finished || pending == nullptr; });
  if (pending == nullptr) { return; }
  const QJsonObject answer = task->answer();
  delete task;
  if (presenter == nullptr) { return; }
  const QString error = errorOf(answer);
  say(error.isEmpty() ? tr("Pushed the library.") : tr("The push failed: %1").arg(error));
}

void PublishLibraryDialog::indexEntry() {
  const QString dir = m_folder->text().trimmed();
  const QString url = m_remote->text().trimmed();
  if (dir.isEmpty() || url.isEmpty()) {
    say(tr("Give the folder and the remote URL where others fetch the library."));
    return;
  }
  const QJsonObject entry = command("index_entry", {{QStringLiteral("dir"), dir}, {QStringLiteral("url"), url}});
  if (!errorOf(entry).isEmpty()) {
    say(tr("No index entry: %1").arg(errorOf(entry)));
    return;
  }
  QApplication::clipboard()->setText(str(entry, "text"));
  say(tr("The index entry %1 (copied; propose it to the index's maintainers):\n%2")
          .arg(str(entry, "path"), str(entry, "text")));
}

} // namespace mitcad
