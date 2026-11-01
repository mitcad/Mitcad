// SPDX-License-Identifier: MIT
#include "FileCommands.hpp"

#include <QCoreApplication>
#include <QKeySequence>

namespace mitcad {
namespace {

QString text(const char* source) { return QCoreApplication::translate("FileCommands", source); }

} // namespace

std::vector<CommandDef> fileDocumentCommands(const FileCommandRuns& runs) {
  const auto def = [](const char* id, const QString& name, const char* icon, const QString& tooltip,
                      const QKeySequence& shortcut, const std::function<void()>& run) {
    CommandDef d;
    d.id = QString::fromLatin1(id);
    d.name = name;
    d.icon = QString::fromLatin1(icon);
    d.tooltip = tooltip;
    d.shortcut = shortcut;
    d.kind = CommandDef::Kind::Action;
    d.mode = CommandDef::Mode::Any;
    d.tab.clear(); // the File menu
    // As the menu's actions were: also while a command's panel is open
    // (Save then saves the design as the panel left it, or says why not).
    d.duringCommands = true;
    d.run = run;
    return d;
  };
  std::vector<CommandDef> commands;
  CommandDef open = def("file.open", text("Open…"), "open",
                        text("Opens a MitCad design, or a file Import takes as a new design"),
                        QKeySequence(QKeySequence::Open), runs.open);
  open.keywords = {QStringLiteral("load"), QStringLiteral("file"), QStringLiteral("document"),
                   QStringLiteral("design")};
  commands.push_back(open);
  CommandDef close = def("file.close", text("Close"), "close", text("Closes the design and starts an untitled one"),
                         QKeySequence(Qt::CTRL | Qt::Key_W), runs.close);
  close.keywords = {QStringLiteral("file"), QStringLiteral("document"), QStringLiteral("design")};
  commands.push_back(close);
  CommandDef save = def("file.save", text("Save"), "save",
                        text("Writes the design to its file; an untitled one asks for a name"),
                        QKeySequence(QKeySequence::Save), runs.save);
  save.keywords = {QStringLiteral("write"), QStringLiteral("store"), QStringLiteral("file")};
  save.enabled = runs.saveEnabled;
  save.disabledReason = [] { return text("no unsaved changes"); };
  commands.push_back(save);
  // QKeySequence::SaveAs has no key on Windows.
  CommandDef saveAs = def("file.save_as", text("Save As…"), "save-as",
                          text("Writes the design to another file or project"),
                          QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_S), runs.saveAs);
  saveAs.keywords = {QStringLiteral("copy"), QStringLiteral("write"), QStringLiteral("rename"),
                     QStringLiteral("file")};
  commands.push_back(saveAs);
  // macOS names it Quit MitCad in the application menu (QuitRole, set
  // where the File menu is made).
  CommandDef exit = def("file.exit", text("Exit"), "exit", text("Ends MitCad, asking about unsaved changes first"),
                        QKeySequence(QKeySequence::Quit), runs.exit);
  exit.keywords = {QStringLiteral("quit"), QStringLiteral("close"), QStringLiteral("end")};
  commands.push_back(exit);
  return commands;
}

} // namespace mitcad
