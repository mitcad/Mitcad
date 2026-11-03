// SPDX-License-Identifier: MIT
#include "SlicerImport.hpp"

#include <algorithm>
#include <exception>
#include <utility>

#include <QAction>
#include <QComboBox>
#include <QDialog>
#include <QFileInfo>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonValue>
#include <QLabel>
#include <QMenu>
#include <QPoint>
#include <QPushButton>
#include <QSignalBlocker>
#include <QStringList>
#include <QVector>
#include <QtLogging>

#include "../framework/Command.hpp"
#include "../framework/DialogFolders.hpp"
#include "../framework/Json.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../platform/MacDialogs.hpp"
#include "ImportDetailsDialog.hpp"
#include "PrintItemsHost.hpp"
#include "PrintKeys.hpp"
#include "PrintLibrary.hpp"
#include "SlicerPresetsDialog.hpp"

namespace mitcad {

QString chooseSlicerFile(QWidget* parent) {
  const QString path = dialogOpenFileName(parent, QObject::tr("Import from Slicer"),
                                          dialogStartFolder(DialogGroup::Import, QString()), slicerFileFilter());
  if (!path.isEmpty()) {
    rememberDialogFolder(DialogGroup::Import, path);
  }
  return path;
}

bool runSlicerImport(PrintItemsHost& host, const QString& path, const QString& into, const QString& log,
                     SlicerImportResult* result, QString* error) {
  // Into the design its root chooses the printer and process read, in the
  // import's one undo step; into a library only its files change.
  const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("import_print_profile")},
                            {QStringLiteral("path"), path}};
  return runPrintImport(host, command, QFileInfo(path).fileName(), into, log, result, error);
}

namespace {

// The `slicer_presets` query for an import into `into`; false when the
// test's folders are not valid.
bool slicersQuery(const PrintItemsHost& host, const QString& into, QJsonObject* query) {
  *query = QJsonObject{{QStringLiteral("query"), QStringLiteral("slicer_presets")}, {QStringLiteral("into"), into}};
  // A UI regression can name synthetic preset folders through the same
  // query argument as the API tests, without reading the user's presets.
  const QByteArray testFolders = qgetenv("MITCAD_TEST_SLICER_FOLDERS");
  if (!qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT") && !testFolders.isEmpty()) {
    const QJsonDocument folders = QJsonDocument::fromJson(testFolders);
    if (!folders.isArray()) {
      qWarning() << "Invalid MITCAD_TEST_SLICER_FOLDERS: expected a JSON array";
      return false;
    }
    query->insert(QStringLiteral("folders"), folders.array());
  }
  const QString project = host.projectFolder();
  if (!project.isEmpty() && into != QStringLiteral("user")) {
    query->insert(QStringLiteral("project"), project);
  }
  return true;
}

} // namespace

QJsonArray installedSlicers(PrintItemsHost& host, const QString& into, QJsonArray* merge) {
  QJsonObject query;
  if (!slicersQuery(host, into, &query)) {
    return {};
  }
  try {
    const QJsonObject answer = host.model().queryObject(query);
    if (merge != nullptr) {
      *merge = answer.value(QStringLiteral("merge")).toArray();
    }
    return answer.value(QStringLiteral("slicers")).toArray();
  } catch (const std::exception& e) {
    qDebug().noquote() << QStringLiteral("Installed slicers not read: %1").arg(QString::fromUtf8(e.what()));
    return {};
  }
}

bool runPrintImport(PrintItemsHost& host, QJsonObject command, const QString& file, const QString& into,
                    const QString& log, SlicerImportResult* result, QString* error) {
  command.insert(QStringLiteral("into"), into);
  const QString project = host.projectFolder();
  if (!project.isEmpty() && into != QStringLiteral("user")) {
    command.insert(QStringLiteral("project"), project);
  }
  // The problems by the plain names of their keys (mitcad#655).
  PrintKeyTable keys;
  try {
    keys = PrintKeyTable(host.model().queryObject({{QStringLiteral("query"), QStringLiteral("print_keys")}}));
  } catch (const std::exception&) {
    // Without the names: as reported.
  }
  QJsonObject answer;
  if (!host.runModelCommand(command, &answer)) {
    if (error != nullptr) {
      *error = host.lastCancelled() ? QString() : plainModelError(host.lastError(), keys, command);
    }
    qDebug().noquote() << QStringLiteral("%1 import refused: %2").arg(log, host.lastError());
    return false;
  }
  SlicerImportResult done;
  done.file = file;
  done.into = into;
  done.answer = answer;
  done.summary = printImportSummary(done.file, into, answer, &keys);
  const PrintImportSummary& summary = done.summary;
  // Each filament's chain and colour (mitcad#1053): `Slicer import: <preset>
  // inherits <chain>, colour <hex|none>`.
  const QJsonArray filamentLog =
      answer.value(QStringLiteral("report")).toObject().value(QStringLiteral("log")).toArray();
  for (const QJsonValue& line : filamentLog) {
    qInfo().noquote() << line.toString();
  }
  if (into == QStringLiteral("design")) {
    const auto names = [&answer](const char* field) {
      QStringList list;
      for (const QJsonValue& value : answer.value(QLatin1String(field)).toArray()) {
        list << value.toObject().value(QStringLiteral("name")).toString();
      }
      return list.join(QStringLiteral(", "));
    };
    const QJsonObject report = answer.value(QStringLiteral("report")).toObject();
    qInfo().noquote() << QStringLiteral("Imported print settings from %1: printers: %2; processes: %3; materials: %4; "
                                        "mapped %5, kept %6, left out: %7%8")
                             .arg(done.file, names("printers"), names("processes"), names("materials"))
                             .arg(report.value(QStringLiteral("mapped")).toInt())
                             .arg(report.value(QStringLiteral("kept")).toInt())
                             .arg(summary.leftOut.isEmpty() ? QStringLiteral("nothing")
                                                            : summary.leftOut.join(QStringLiteral("; ")),
                                  summary.problems.isEmpty()
                                      ? QString()
                                      : QStringLiteral("; not read: %1").arg(summary.problems.join(QStringLiteral("; "))));
    qInfo().noquote() << QStringLiteral("%1 import: %2").arg(log, summary.line);
  } else {
    QStringList items;
    for (const QJsonValue& value : answer.value(QStringLiteral("items")).toArray()) {
      const QJsonObject item = value.toObject();
      items << QStringLiteral("%1 %2").arg(item.value(QStringLiteral("kind")).toString(),
                                           item.value(QStringLiteral("name")).toString());
    }
    qDebug().noquote() << QStringLiteral("%1 imported into %2: %3")
                              .arg(log, into, items.isEmpty() ? QStringLiteral("nothing") : items.join(QStringLiteral(", ")));
    qDebug().noquote() << QStringLiteral("%1 import: %2").arg(log, summary.line);
  }
  // One filament's separate materials merged first (mitcad#1054).
  for (const QJsonValue& value : answer.value(QStringLiteral("merged")).toArray()) {
    const QJsonObject merged = value.toObject();
    QStringList from;
    for (const QJsonValue& id : merged.value(QStringLiteral("from")).toArray()) {
      from << id.toString();
    }
    qDebug().noquote() << QStringLiteral("%1 merged: %2 (%3)")
                              .arg(log, merged.value(QStringLiteral("name")).toString(), from.join(QStringLiteral(", ")));
  }
  if (result != nullptr) {
    *result = std::move(done);
  }
  return true;
}

SlicerImportBar::SlicerImportBar(PrintItemsHost& host, const QString& logName, QWidget* parent)
    : QWidget(parent), m_host(&host), m_log(logName) {
  m_row = new QHBoxLayout(this);
  m_row->setContentsMargins(0, 0, 0, 0);
  m_line = new QLabel;
  setRoleStyleSheet(m_line, ColorRole::TextSecondary);
  m_line->hide();
  m_details = new QPushButton(tr("Details…"));
  m_details->setObjectName(QStringLiteral("slicerImportDetails"));
  m_details->hide();
  m_import = new QPushButton(tr("Import from Slicer…"));
  m_import->setObjectName(QStringLiteral("slicerImportButton"));
  // Where the import goes, chosen before it (the managers, mitcad#1135):
  // shown only when there is more than one place.
  m_into = new QComboBox;
  m_into->setObjectName(QStringLiteral("slicerImportInto"));
  m_into->setAccessibleName(tr("Import into"));
  m_into->setToolTip(tr("Where Import from Slicer puts what it reads"));
  m_into->hide();
  m_row->addStretch(1);
  m_row->addWidget(m_line);
  m_row->addWidget(m_details);
  m_row->addWidget(m_into);
  m_row->addWidget(m_import);
  for (QPushButton* button : {m_details, m_import}) {
    mac::auxiliaryButton(button);
  }
  connect(m_import, &QPushButton::clicked, this, &SlicerImportBar::importClicked);
  connect(m_into, &QComboBox::currentIndexChanged, this, [this] {
    qDebug().noquote() << QStringLiteral("%1 import into: %2").arg(m_log, m_into->currentText());
    // The slicer's presets are marked against the place chosen.
    findSlicersAhead();
  });
  connect(m_details, &QPushButton::clicked, this, [this] { ImportDetailsDialog::showReport(m_summary, window()); });
}

SlicerImportBar::~SlicerImportBar() = default;

QString SlicerImportBar::into() const {
  if (m_into->count() > 0) {
    return m_into->currentData().toString();
  }
  return target ? target() : QStringLiteral("design");
}

void SlicerImportBar::setHost(PrintItemsHost& host) {
  m_host = &host;
  clear();
}

void SlicerImportBar::setPlaces(const QStringList& places, const QString& current) {
  QStringList shown;
  for (int i = 0; i < m_into->count(); ++i) {
    shown << m_into->itemData(i).toString();
  }
  if (shown == places) {
    return; // the user's choice stays
  }
  {
    const QSignalBlocker block(m_into);
    m_into->clear();
    for (const QString& place : places) {
      m_into->addItem(placeName(place), place);
    }
    m_into->setCurrentIndex(std::max(0, static_cast<int>(places.indexOf(current))));
  }
  m_into->setVisible(places.size() > 1);
  scheduleLogPlaces();
}

void SlicerImportBar::clear() {
  m_line->hide();
  m_details->hide();
  m_summary = PrintImportSummary();
  scheduleLogPlaces();
}

void SlicerImportBar::findSlicers() {
  // An installed Bambu-family slicer's presets (mitcad#830): the button
  // opens a menu of a file or a slicer's presets. A look ahead under way
  // is older.
  ++m_finds;
  showSlicers(installedSlicers(*m_host, into(), &m_merge));
}

void SlicerImportBar::findSlicersAhead() {
  // Off the UI thread (mitcad#1018): an installed slicer's presets are
  // many files. For the button's text only; a click looks again.
  QJsonObject query;
  if (!slicersQuery(*m_host, into(), &query)) {
    return;
  }
  const int finding = ++m_finds;
  const QByteArray key = compactJson(query);
  QVector<QJsonObject> requests;
  requests.append(query);
  readAhead(this, requests, [this, finding, key](const QueryAnswers& answers, qint64 readMs) {
    if (finding != m_finds) {
      return;
    }
    const QJsonArray slicers = answers.value(key).toObject().value(QStringLiteral("slicers")).toArray();
    m_merge = answers.value(key).toObject().value(QStringLiteral("merge")).toArray();
    qDebug().noquote() << QStringLiteral("%1 slicers: read %2 ms, %3 found").arg(m_log).arg(readMs).arg(slicers.size());
    showSlicers(slicers);
    scheduleLogPlaces();
  });
}

void SlicerImportBar::showSlicers(const QJsonArray& slicers) {
  m_slicers = slicers;
  m_import->setText(m_slicers.isEmpty() ? tr("Import from Slicer…") : tr("Import from Slicer ▾"));
}

void SlicerImportBar::importClicked() {
  if (!m_host->canChangePrintSettings()) {
    showRefusal(m_host->changeBlockedReason());
    return;
  }
  // Looked for again, in the files: a slicer may have come or gone since.
  m_host->readAgain();
  findSlicers();
  if (m_slicers.isEmpty()) {
    importFile();
    return;
  }
  QMenu menu(this);
  QAction* file = menu.addAction(tr("From a file…"));
  QStringList entries{file->text()};
  QVector<QAction*> presets;
  for (const QJsonValue& slicer : std::as_const(m_slicers)) {
    presets.append(menu.addAction(tr("Presets of %1…").arg(SlicerPresetsDialog::slicerTitle(slicer.toObject()))));
    entries << presets.last()->text();
  }
  qDebug().noquote() << QStringLiteral("%1 import menu: %2").arg(m_log, entries.join(QStringLiteral(" | ")));
  QAction* chosen = execMenu(&menu, m_import->mapToGlobal(QPoint(0, m_import->height())));
  if (chosen == file) {
    importFile();
  } else if (chosen != nullptr) {
    importPresets(static_cast<int>(presets.indexOf(chosen)));
  }
}

void SlicerImportBar::importFile() {
  const QString path = chooseSlicerFile(window());
  if (path.isEmpty()) {
    return;
  }
  SlicerImportResult result;
  QString error;
  if (!runSlicerImport(*m_host, path, into(), m_log, &result, &error)) {
    if (!error.isEmpty()) {
      showRefusal(error);
    }
    return;
  }
  showImported(result);
}

void SlicerImportBar::importPresets(int index) {
  if (index < 0 || index >= m_slicers.size()) {
    return;
  }
  SlicerPresetsDialog dialog(m_slicers, index, into(), window(), m_merge);
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted || (dialog.tickedCount() == 0 && !dialog.mergeChosen())) {
    qDebug().noquote() << QStringLiteral("Slicer presets cancelled");
    return;
  }
  // "3 presets" in the line and the report: "3 presets imported into My
  // library", "Imported 3 presets (Bambu Studio) into this design."; one
  // preset by its name: "Fine imported into My library"; a merge alone
  // (mitcad#1054): "Separate materials".
  const QStringList names = dialog.tickedNames();
  const QString what = names.isEmpty()       ? tr("Separate materials")
                       : names.size() == 1 ? names.first()
                                           : tr("%1 presets").arg(names.size());
  SlicerImportResult result;
  QString error;
  if (!runPrintImport(*m_host, dialog.command(), what, into(), m_log, &result, &error)) {
    if (!error.isEmpty()) {
      showRefusal(error);
    }
    return;
  }
  showImported(result);
}

void SlicerImportBar::showImported(const SlicerImportResult& result) {
  m_summary = result.summary;
  m_line->setText(m_summary.line);
  setRoleStyleSheet(m_line, ColorRole::TextSecondary);
  m_line->show();
  m_details->setVisible(!m_summary.details.isEmpty());
  // A running command's panel (3D Print's) follows the design.
  m_host->commandInputsChanged();
  emit imported(result);
  scheduleLogPlaces();
}

void SlicerImportBar::showRefusal(const QString& error) {
  m_line->setText(error);
  setErrorStyleSheet(m_line);
  m_line->show();
  m_details->hide();
  scheduleLogPlaces();
}

void SlicerImportBar::showEvent(QShowEvent* event) {
  QWidget::showEvent(event);
  findSlicersAhead();
  m_loggedPlaces.clear();
  scheduleLogPlaces();
}

void SlicerImportBar::resizeEvent(QResizeEvent* event) {
  QWidget::resizeEvent(event);
  scheduleLogPlaces();
}

void SlicerImportBar::scheduleLogPlaces() {
  TestSync::singleShot(0, this, [this] { logPlaces(); });
}

void SlicerImportBar::logPlaces() {
  // In the main window's coordinates (the parent of the bar's window), as
  // the other logged places.
  if (!isVisible() || window()->parentWidget() == nullptr) {
    return;
  }
  const QPoint origin = window()->parentWidget()->window()->mapToGlobal(QPoint(0, 0));
  QStringList lines;
  const std::pair<const char*, QWidget*> widgets[] = {
      {"import", m_import}, {"details", m_details}, {"import_into", m_into}};
  for (const auto& [name, widget] : widgets) {
    if (widget->isVisibleTo(this)) {
      const QPoint global = widget->mapToGlobal(widget->rect().center()) - origin;
      lines << QStringLiteral("%1 %2 at %3,%4").arg(m_log, QLatin1String(name)).arg(global.x()).arg(global.y());
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
