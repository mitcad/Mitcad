// SPDX-License-Identifier: MIT
// Component libraries in the main window (mitcad#64, mitcad#63): Insert
// from Library (the INSERT group, the File menu), and in Tools > Libraries:
// Libraries..., Library Parts... (versions, sizes, updates, the parts
// list), Community Library... and Publish to Library....
#include "../MainWindow.hpp"

#include <QJsonArray>
#include <QMenu>
#include <QtLogging>

#include "../framework/CommandRegistry.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/AppSettings.hpp"
#include "Libraries.hpp"
#include "LibraryDialogs.hpp"

namespace mitcad {

void MainWindow::registerLibraryCommands() {
  configureLibraries();
  const auto def = [](const char* id, const QString& name, const char* icon, const QString& tooltip,
                      std::function<void()> run) {
    CommandDef d;
    d.id = QString::fromLatin1(id);
    d.name = name;
    d.icon = QString::fromLatin1(icon);
    d.tooltip = tooltip;
    d.kind = CommandDef::Kind::Action;
    d.mode = CommandDef::Mode::Any;
    d.tab.clear(); // menus
    d.run = std::move(run);
    return d;
  };
  CommandDef insert = def("insert.library_component", tr("Insert from Library"), "insert-library",
                          tr("Places a part of a component library (a fastener in a size, ...), linked at the "
                             "version you choose or as a copy"),
                          [this] { insertFromLibrary(false); });
  insert.mode = CommandDef::Mode::Model;
  insert.tab = QStringLiteral("SOLID");
  insert.group = QStringLiteral("INSERT");
  insert.keywords = {QStringLiteral("library"), QStringLiteral("fastener"), QStringLiteral("screw"),
                     QStringLiteral("bolt"), QStringLiteral("nut"), QStringLiteral("washer"),
                     QStringLiteral("standard part"), QStringLiteral("iso")};
  m_registry->add(insert);
  CommandDef parts = def("tools.library_parts", tr("Library Parts..."), "parts-list",
                         tr("The design's library parts: their versions and sizes, updates, and the parts list"),
                         [this] { showLibraryParts(); });
  parts.keywords = {QStringLiteral("update library"), QStringLiteral("bill of materials"), QStringLiteral("bom"),
                    QStringLiteral("parts list"), QStringLiteral("licence"), QStringLiteral("attribution")};
  m_registry->add(parts);
  CommandDef libraries = def("tools.libraries", tr("Libraries..."), "library",
                             tr("The component libraries and community indexes to use; fetch them"),
                             [this] { manageLibraries(); });
  libraries.keywords = {QStringLiteral("library"), QStringLiteral("fetch"), QStringLiteral("git"),
                        QStringLiteral("sources")};
  m_registry->add(libraries);
  CommandDef community = def("tools.community_library", tr("Community Library..."), "library",
                             tr("Designs and components others share in git repositories: search, licences, "
                                "insert"),
                             [this] { insertFromLibrary(true); });
  community.keywords = {QStringLiteral("community"), QStringLiteral("share"), QStringLiteral("download"),
                        QStringLiteral("index"), QStringLiteral("library")};
  m_registry->add(community);
  CommandDef publish = def("tools.publish_library", tr("Publish to Library..."), "library",
                           tr("Adds this design to a library of your own and helps to publish it"),
                           [this] { publishToLibrary(); });
  publish.keywords = {QStringLiteral("share"), QStringLiteral("publish"), QStringLiteral("library"),
                      QStringLiteral("community")};
  m_registry->add(publish);
}

void MainWindow::createLibraryMenu(QMenu* tools) {
  QMenu* menu = tools->addMenu(tr("&Libraries"));
  menu->setObjectName(QStringLiteral("librariesMenu"));
  const std::pair<const char*, QString> entries[] = {
      {"insert.library_component", tr("&Insert from Library...")},
      {"tools.library_parts", tr("Library &Parts...")},
      {"tools.community_library", tr("&Community Library...")},
      {"tools.publish_library", tr("P&ublish to Library...")},
      {"tools.libraries", tr("&Libraries...")}};
  for (const auto& [id, text] : entries) {
    if (QAction* action = m_registry->action(QString::fromLatin1(id))) {
      action->setText(text);
      menu->addAction(action);
    }
  }
}

void MainWindow::insertFromLibrary(bool community) {
  if (!canChangeModel()) {
    return;
  }
  LibraryBrowser browser(community, this);
  prepareModal(&browser);
  if (browser.exec() != QDialog::Accepted || !browser.choice()) {
    return;
  }
  const LibraryChoice choice = *browser.choice();
  QJsonObject library{{QStringLiteral("id"), choice.library},
                      {QStringLiteral("url"), choice.url},
                      {QStringLiteral("rev"), choice.rev},
                      {QStringLiteral("component"), choice.component}};
  if (!choice.config.isEmpty()) {
    library.insert(QStringLiteral("config"), choice.config);
  }
  QJsonObject result;
  const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("insert_component")},
                            {QStringLiteral("library"), library},
                            {QStringLiteral("link"), choice.link}};
  if (runModelCommand(command, &result)) {
    qInfo().noquote() << QStringLiteral("Inserted component %1 (%2) from %3 %4, licence %5")
                             .arg(result.value(QStringLiteral("name")).toString(),
                                  choice.link ? QStringLiteral("linked") : QStringLiteral("copy"), choice.library,
                                  choice.version, choice.license.isEmpty() ? QStringLiteral("none") : choice.license);
    showHint(tr("Inserted %1 from %2 %3.")
                 .arg(result.value(QStringLiteral("name")).toString(), choice.library, choice.version));
    m_viewer->fitAll();
  }
}

void MainWindow::showLibraryParts() {
  const QJsonArray parts =
      queryObject({{QStringLiteral("query"), QStringLiteral("library_parts")}}).value(QStringLiteral("parts")).toArray();
  const QJsonArray list =
      queryObject({{QStringLiteral("query"), QStringLiteral("parts_list")}}).value(QStringLiteral("rows")).toArray();
  LibraryPartsDialog dialog(parts, list, this);
  prepareModal(&dialog);
  if (dialog.exec() != QDialog::Accepted || !canChangeModel()) {
    return;
  }
  QJsonArray changes;
  QStringList described;
  for (const LibraryPartChange& change : dialog.changes()) {
    QJsonObject entry{{QStringLiteral("component"), change.component}};
    if (!change.rev.isEmpty()) {
      entry.insert(QStringLiteral("rev"), change.rev);
    }
    if (!change.config.isEmpty()) {
      entry.insert(QStringLiteral("config"), change.config);
    }
    changes.append(entry);
  }
  if (changes.isEmpty()) {
    return;
  }
  QJsonObject result;
  if (runModelCommand({{QStringLiteral("cmd"), QStringLiteral("update_library_parts")},
                       {QStringLiteral("changes"), changes}},
                      &result)) {
    for (const QJsonValue& line : result.value(QStringLiteral("changes")).toArray()) {
      described << line.toString();
    }
    qInfo().noquote() << QStringLiteral("Updated library parts: %1").arg(described.join(QStringLiteral("; ")));
    showHint(tr("Updated %n library part(s).", nullptr, int(changes.size())));
  }
}

void MainWindow::manageLibraries() {
  LibrariesDialog dialog(this);
  prepareModal(&dialog);
  dialog.exec();
}

void MainWindow::publishToLibrary() {
  const VersionSettings author = VersionSettings::load();
  PublishLibraryDialog dialog(
      [this] {
        const rust::String json = idleDocument().to_json();
        return QString::fromUtf8(json.data(), static_cast<qsizetype>(json.size()));
      },
      [this] { return m_viewer->grabFramebuffer(); }, author.complete() ? author.author() : QString(), this);
  prepareModal(&dialog);
  dialog.exec();
}

} // namespace mitcad
