// SPDX-License-Identifier: MIT
// Component libraries in the main window (mitcad#64, mitcad#63): Insert
// from Library (the Assemble group), and Tools > Libraries (mitcad#1250),
// grouped: Browse Components... (mitcad#1283: Installed, Community and
// Favorites in one window; Insert from Library and Community Library stay
// as commands that open it on Installed and on Community); Components in
// This Design... (versions, sizes, updates, the parts list) and Publish to
// Library...; Material, Printer and Process Library... (the print managers
// on My library, also without a design); Library Sources... (which
// component libraries to use).
#include "../MainWindow.hpp"

#include <QJsonArray>
#include <QMenu>
#include <QStringList>
#include <QtLogging>

#include "../browser/BrowserController.hpp"
#include "../commands/MountCommands.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/AppSettings.hpp"
#include "../library/MountFrames.hpp"
#include "../library/UsedParts.hpp"
#include "Libraries.hpp"
#include "LibraryAutoUpdate.hpp"
#include "LibraryDialogs.hpp"

namespace mitcad {

void MainWindow::registerLibraryCommands() {
  configureLibraries();
  // Place Part (mitcad#525): a part with mount frames placed by one.
  m_mountPart = std::make_shared<MountPart>();
  registerMountCommands(*m_registry, *this, m_mountPart);
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
  // Browse Components on Installed (mitcad#1283): the Assemble group's
  // button, its shortcut and scripts keep this id.
  CommandDef insert = def("insert.library_component", tr("Insert from Library"), "insert-library",
                          tr("Places a library component, linked or as a copy"),
                          [this] { browseComponents(false); });
  insert.mode = CommandDef::Mode::Model;
  insert.tab = QStringLiteral("SOLID");
  insert.group = QStringLiteral("ASSEMBLE");
  insert.section = QStringLiteral("insert"); // after Insert Component (mitcad#1778)
  insert.keywords = {QStringLiteral("library"), QStringLiteral("fastener"), QStringLiteral("screw"),
                     QStringLiteral("bolt"), QStringLiteral("nut"), QStringLiteral("washer"),
                     QStringLiteral("standard part"), QStringLiteral("iso")};
  m_registry->add(insert);
  // One window to find, get and insert library components (mitcad#1283,
  // GitHub #136): its views Installed, Community and Favorites, the menu's
  // first entry in place of Insert from Library… and Community Library….
  CommandDef browse = def("tools.browse_components", tr("Browse Components…"), "insert-library",
                          tr("Finds, gets and inserts library components: installed, community and favorites"),
                          [this] { browseComponents(false); });
  browse.mode = CommandDef::Mode::Model;
  browse.keywords = {QStringLiteral("library"),       QStringLiteral("insert from library"),
                     QStringLiteral("community"),     QStringLiteral("community library"),
                     QStringLiteral("favorites"),     QStringLiteral("get library"),
                     QStringLiteral("download"),      QStringLiteral("fastener"),
                     QStringLiteral("standard part"), QStringLiteral("components")};
  m_registry->add(browse);
  // Named for what it shows, this design's library components (mitcad#1250:
  // once Library Components…, the id and the old name as a keyword kept).
  CommandDef parts = def("tools.library_parts", tr("Components in This Design…"), "parts-list",
                         tr("The design's library components: versions, sizes, updates, bill of materials"),
                         [this] { showLibraryParts(); });
  parts.keywords = {QStringLiteral("library components"), QStringLiteral("update library"),
                    QStringLiteral("bill of materials"), QStringLiteral("bom"), QStringLiteral("parts"),
                    QStringLiteral("license"), QStringLiteral("licence"), QStringLiteral("attribution")};
  m_registry->add(parts);
  // Which component libraries and community indexes to use (mitcad#1250:
  // once Libraries…, the id and the old name as a keyword kept).
  CommandDef libraries = def("tools.libraries", tr("Library Sources…"), "library",
                             tr("The component libraries and community indexes to use; fetch them"),
                             [this] { manageLibraries(); });
  libraries.keywords = {QStringLiteral("libraries"), QStringLiteral("library"), QStringLiteral("fetch"),
                        QStringLiteral("git"), QStringLiteral("sources")};
  m_registry->add(libraries);
  // Browse Components on Community (mitcad#1283): no longer in the menu,
  // the id kept for shortcuts, scripts and the command search.
  CommandDef community = def("tools.community_library", tr("Community Library…"), "library",
                             tr("Searches and inserts designs others share in git repositories"),
                             [this] { browseComponents(true); });
  community.keywords = {QStringLiteral("community"), QStringLiteral("share"), QStringLiteral("download"),
                        QStringLiteral("index"), QStringLiteral("library")};
  m_registry->add(community);
  CommandDef publish = def("tools.publish_library", tr("Publish to Library…"), "library",
                           tr("Adds this design to a library of your own and helps to publish it"),
                           [this] { publishToLibrary(); });
  publish.keywords = {QStringLiteral("share"), QStringLiteral("publish"), QStringLiteral("library"),
                      QStringLiteral("community")};
  m_registry->add(publish);
  // The print managers on My library (mitcad#1250, GitHub #132): also with
  // no design open, where they work on the libraries alone (mitcad#1135),
  // and in a sketch; with a design as the Make tab's buttons, the group of
  // My library in sight and the place New and Import offer first.
  const auto library = [this, &def](const char* id, const QString& name, const char* icon, const QString& tooltip,
                                    const QString& kind, const QStringList& keywords) {
    CommandDef entry = def(id, name, icon, tooltip, [this, kind] {
      if (m_controller != nullptr) {
        m_controller->openLibrary(kind);
      }
    });
    entry.keywords = keywords;
    entry.keywords << QStringLiteral("my library") << QStringLiteral("library");
    entry.enabled = [this] { return m_controller != nullptr; };
    m_registry->add(entry);
  };
  library("tools.material_library", tr("Material Library…"), "material",
          tr("Your own materials: create, edit, import and delete them"), QStringLiteral("material"),
          {QStringLiteral("material"), QStringLiteral("filament"), QStringLiteral("density"),
           QStringLiteral("physical material")});
  library("tools.printer_library", tr("Printer Library…"), "print-settings",
          tr("Your own printers: create, edit, import and delete them"), QStringLiteral("printer"),
          {QStringLiteral("printer"), QStringLiteral("print"), QStringLiteral("slicer"), QStringLiteral("profile"),
           QStringLiteral("nozzle")});
  library("tools.process_library", tr("Process Library…"), "print-process",
          tr("Your own print processes: create, edit, import and delete them"), QStringLiteral("process"),
          {QStringLiteral("process"), QStringLiteral("print"), QStringLiteral("slicer"), QStringLiteral("profile"),
           QStringLiteral("layer height"), QStringLiteral("infill")});
}

void MainWindow::updateLibrariesAtStart() {
  LibraryAutoUpdate& update = LibraryAutoUpdate::instance();
  // Only a short note in the status bar: a failure was logged by the fetch,
  // and none of this opens a dialog.
  connect(&update, &LibraryAutoUpdate::finished, this, [this](const QString& message) {
    if (!message.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Library auto-update: %1").arg(message);
      showHint(message);
    }
  });
  update.start();
}

void MainWindow::createLibraryMenu(QMenu* tools) {
  QMenu* menu = tools->addMenu(tr("&Libraries"));
  menu->setObjectName(QStringLiteral("librariesMenu"));
  // Grouped (mitcad#1250): what to place, this design's components and
  // publishing, the print libraries, then where the libraries come from.
  // An empty id is a separator. The native macOS menu reads this one
  // (MainWindow::refreshNativeActions), separators included. Browse
  // Components… (mitcad#1283) is the one entry to place: Insert from
  // Library and Community Library open its views from the ribbon, the
  // shortcuts and the command search.
  const std::pair<const char*, QString> entries[] = {
      {"tools.browse_components", tr("&Browse Components…")},
      {"", QString()},
      {"tools.library_parts", tr("Components in This &Design…")},
      {"tools.publish_library", tr("P&ublish to Library…")},
      {"", QString()},
      {"tools.material_library", tr("&Material Library…")},
      {"tools.printer_library", tr("P&rinter Library…")},
      {"tools.process_library", tr("Pr&ocess Library…")},
      {"", QString()},
      {"tools.libraries", tr("Library &Sources…")}};
  for (const auto& [id, text] : entries) {
    if (*id == '\0') {
      menu->addSeparator();
    } else if (QAction* action = m_registry->action(QString::fromLatin1(id))) {
      action->setText(text);
      menu->addAction(action);
    }
  }
  // For the UI tests: the entries as the menu opens, `-` a separator.
  connect(menu, &QMenu::aboutToShow, this, [menu] {
    QStringList shown;
    for (const QAction* entry : menu->actions()) {
      shown << (entry->isSeparator() ? QStringLiteral("-") : entry->text().remove(QLatin1Char('&')));
    }
    qDebug().noquote() << QStringLiteral("Libraries menu: %1").arg(shown.join(QStringLiteral(" | ")));
  });
}

void MainWindow::browseComponents(bool community) {
  if (!canChangeModel()) {
    return;
  }
  // What was selected before the command (mitcad#1197): Place Component's
  // Place On where it takes it (a hole's rim or round face, an axis, a
  // flat face or a plane).
  const Selection before = m_selection;
  LibraryBrowser browser(community ? LibraryBrowser::View::Community : LibraryBrowser::View::Installed, this);
  // The picker's In this design (mitcad#418).
  browser.setUsedParts(usedLibraryParts(model()));
  prepareModal(&browser);
  if (execDialog(&browser) != QDialog::Accepted || !browser.choice()) {
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
  // The Standard filter's standard names the part (mitcad#949).
  if (!choice.standard.isEmpty()) {
    library.insert(QStringLiteral("standard"), choice.standard);
  }
  // A part with mount frames (mitcad#525): Place Part, where a click snaps a
  // frame to a face, a hole or a grid and Enter drops it in as below. The
  // selection made before is its Place On when it takes it, the part shown
  // there at once (mitcad#1197).
  const QVector<MountFrameInfo> frames = mountFramesOf(choice.mounts);
  if (!frames.isEmpty()) {
    *m_mountPart =
        MountPart{library, choice.name, choice.version, choice.license, choice.link, choice.mounts, choice.parameters};
    QStringList names;
    for (const MountFrameInfo& frame : frames) {
      names << mountFrameLabel(frame);
    }
    qInfo().noquote() << QStringLiteral("Insert from Library: %1/%2 has mount frames %3: Place Part")
                             .arg(choice.library, choice.component, names.join(QStringLiteral(", ")));
    startCommand(QString::fromLatin1(kMountPartCommand), before, nullptr);
    return;
  }
  QJsonObject result;
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("insert_component")},
                      {QStringLiteral("library"), library},
                      {QStringLiteral("link"), choice.link}};
  // An open table's options as the component's own (mitcad#810).
  if (!choice.parameters.isEmpty()) {
    command.insert(QStringLiteral("parameters"), choice.parameters);
  }
  if (runModelCommand(command, &result)) {
    qInfo().noquote() << QStringLiteral("Inserted component %1 (%2) from %3 %4, licence %5")
                             .arg(result.value(QStringLiteral("name")).toString(),
                                  choice.link ? QStringLiteral("linked") : QStringLiteral("copy"), choice.library,
                                  choice.version, choice.license.isEmpty() ? QStringLiteral("none") : choice.license);
    if (!choice.parameters.isEmpty()) {
      QStringList options;
      for (auto it = choice.parameters.begin(); it != choice.parameters.end(); ++it) {
        options << QStringLiteral("%1 = %2").arg(it.key(), it.value().toString());
      }
      qInfo().noquote() << QStringLiteral("Inserted component %1 with options %2 (%3)")
                               .arg(result.value(QStringLiteral("name")).toString(),
                                    options.join(QStringLiteral(", ")),
                                    choice.config.isEmpty() ? QStringLiteral("no row")
                                                            : QStringLiteral("row %1").arg(choice.config));
    }
    // At the origin, never silently (mitcad#1197): the message band says
    // so and how to place it.
    qInfo().noquote() << QStringLiteral("Insert from Library: %1/%2 has no mount frame: dropped in at the origin")
                             .arg(choice.library, choice.component);
    showStatus(tr("Inserted %1 at the origin: no mount frame; join it with Joint.")
                   .arg(result.value(QStringLiteral("name")).toString()),
               NoticeLevel::Information);
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
  if (execDialog(&dialog) != QDialog::Accepted || !canChangeModel()) {
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
    showHint(tr("Updated %n library component(s).", nullptr, int(changes.size())));
  }
}

void MainWindow::manageLibraries() {
  LibrariesDialog dialog(this);
  prepareModal(&dialog);
  execDialog(&dialog);
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
  execDialog(&dialog);
}

} // namespace mitcad
