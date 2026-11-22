// SPDX-License-Identifier: MIT
// Keyboard Shortcuts' key sequences (mitcad#2095): a key that is the start
// of another (E and E, X) is a clash in either order, said in words; a
// three-key sequence saved to the settings and read back (the commas of
// "E, X, C" in an INI value, also written by hand without its quotes), and
// carried through an exported settings file; saved keys that clash this
// way are disabled with the same words in the warning. The Keyboard and
// Mouse Overview's cheat sheet from the user's keys: its title, a table per
// group with its caption, a changed key, a sequence and a default key, and
// the search's rows only when a search is given.
#include "UnitTests.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/SettingsTransfer.hpp"
#include "framework/ShortcutBindings.hpp"
#include "view/ShortcutOverview.hpp"

#include <cstdio>

#include <QCoreApplication>
#include <QDate>
#include <QFile>
#include <QKeySequence>
#include <QSettings>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "shortcut_sequences.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::ShortcutBinding;
using mitcad::ShortcutConflict;

mitcad::CommandDef command(const char* id, const char* name, const QKeySequence& key) {
  mitcad::CommandDef def;
  def.id = QString::fromLatin1(id);
  def.name = QString::fromLatin1(name);
  def.kind = mitcad::CommandDef::Kind::Action;
  def.mode = mitcad::CommandDef::Mode::Model;
  def.shortcut = key;
  def.icon = QStringLiteral("extrude");
  return def;
}

QString fileText(const QString& path) {
  QFile file(path);
  return file.open(QIODevice::ReadOnly) ? QString::fromUtf8(file.readAll()) : QString();
}

bool writeText(const QString& path, const QString& text) {
  QFile file(path);
  return file.open(QIODevice::WriteOnly) && file.write(text.toUtf8()) > 0;
}

// E (Extrude) and E, X (Line) in the 3D view: found whichever comes first,
// in words that say which is the start of which.
void prefixTests() {
  const ShortcutBinding extrude{QStringLiteral("solid.extrude"), QStringLiteral("Extrude"),
                                mitcad::ModelShortcutContext, QKeySequence(Qt::Key_E), QKeySequence(Qt::Key_E), {},
                                false};
  const ShortcutBinding line{QStringLiteral("x.line"), QStringLiteral("Line"), mitcad::ModelShortcutContext,
                             QKeySequence(QStringLiteral("E, X"), QKeySequence::PortableText), QKeySequence(), {},
                             false};
  const QString expected = QStringLiteral("E (Extrude) is the start of E, X (Line) in the 3D view. Change one of them.");
  for (const QList<ShortcutBinding>& order : {QList<ShortcutBinding>{extrude, line}, QList<ShortcutBinding>{line, extrude}}) {
    const QList<ShortcutConflict> conflicts = mitcad::shortcutConflicts(order);
    CHECK(conflicts.size() == 1);
    if (conflicts.isEmpty()) {
      continue;
    }
    CHECK(conflicts.front().prefix());
    CHECK(conflicts.front().description() == expected);
    // The changed key (the sequence) is the one disabled.
    const QSet<QString> disabled = mitcad::conflictingShortcutIds(conflicts);
    CHECK(disabled == QSet<QString>{QStringLiteral("x.line")});
  }
  // The same key is still "used by both".
  const ShortcutBinding other{QStringLiteral("x.other"), QStringLiteral("Other"), mitcad::ModelShortcutContext,
                              QKeySequence(Qt::Key_E), QKeySequence(), {}, false};
  const QList<ShortcutConflict> same = mitcad::shortcutConflicts({extrude, other});
  CHECK(same.size() == 1 && !same.front().prefix());
  CHECK(same.size() == 1 &&
        same.front().description() == QStringLiteral("E is used by both Extrude and Other in the 3D view. Change one of them."));
  // Keys in different contexts never meet; a sequence that only shares a
  // first key with another does not clash.
  ShortcutBinding sketchLine = line;
  sketchLine.contexts = mitcad::SketchShortcutContext;
  CHECK(mitcad::shortcutConflicts({extrude, sketchLine}).isEmpty());
  const ShortcutBinding gl{QStringLiteral("x.gl"), QStringLiteral("GL"), mitcad::ModelShortcutContext,
                           QKeySequence(QStringLiteral("G, L"), QKeySequence::PortableText), QKeySequence(), {}, false};
  const ShortcutBinding gc{QStringLiteral("x.gc"), QStringLiteral("GC"), mitcad::ModelShortcutContext,
                           QKeySequence(QStringLiteral("G, C"), QKeySequence::PortableText), QKeySequence(), {}, false};
  CHECK(mitcad::shortcutConflicts({gl, gc}).isEmpty());
  // A sequence that starts with a reserved key.
  const ShortcutBinding find{QStringLiteral("view.search_timeline"), QStringLiteral("Search Timeline"),
                             mitcad::AllShortcutContexts, QKeySequence(QStringLiteral("Ctrl+F"), QKeySequence::PortableText),
                             QKeySequence(QStringLiteral("Ctrl+F"), QKeySequence::PortableText), {}, true};
  const ShortcutBinding findNext{QStringLiteral("x.find"), QStringLiteral("Find Next"), mitcad::ModelShortcutContext,
                                 QKeySequence(QStringLiteral("Ctrl+F, N"), QKeySequence::PortableText), QKeySequence(),
                                 {}, false};
  const QList<ShortcutConflict> reserved = mitcad::shortcutConflicts({findNext, find});
  CHECK(reserved.size() == 1);
  CHECK(reserved.size() == 1 && reserved.front().description().contains(QStringLiteral(" is reserved for Search Timeline. ")));
  CHECK(reserved.size() == 1 && reserved.front().description().endsWith(QStringLiteral("Choose another key for Find Next.")));
}

// The settings of the test's own, as the registry reads and writes the
// application's default ones.
struct OwnSettings {
  QTemporaryDir directory;
  QString organization = QCoreApplication::organizationName();
  QString application = QCoreApplication::applicationName();
  QSettings::Format format = QSettings::defaultFormat();
  OwnSettings() {
    QCoreApplication::setOrganizationName(QStringLiteral("MitcadSequenceTest"));
    QCoreApplication::setApplicationName(QStringLiteral("MitcadSequenceTest"));
    QSettings::setDefaultFormat(QSettings::IniFormat);
    QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
    QSettings settings;
    settings.clear();
    settings.sync();
  }
  ~OwnSettings() {
    QCoreApplication::setOrganizationName(organization);
    QCoreApplication::setApplicationName(application);
    QSettings::setDefaultFormat(format);
  }
  OwnSettings(const OwnSettings&) = delete;
  OwnSettings& operator=(const OwnSettings&) = delete;
};

const QKeySequence kExc(QStringLiteral("E, X, C"), QKeySequence::PortableText);
const QKeySequence kGl(QStringLiteral("G, L"), QKeySequence::PortableText);

void settingsTests() {
  OwnSettings own;
  CHECK(own.directory.isValid());
  const QString file = QSettings().fileName();
  {
    mitcad::CommandRegistry registry;
    registry.add(command("x.line", "Line", QKeySequence()));
    registry.setShortcut(QStringLiteral("x.line"), kExc);
    QSettings().sync();
    // Quoted in the INI file: unquoted, its commas would split it.
    CHECK(fileText(file).contains(QStringLiteral("x.line=\"E, X, C\"")));
  }
  {
    mitcad::CommandRegistry registry;
    registry.add(command("x.line", "Line", QKeySequence()));
    CHECK(registry.shortcut(QStringLiteral("x.line")) == kExc);
    CHECK(registry.shortcut(QStringLiteral("x.line")).count() == 3);
  }
  // Written by hand without the quotes: a list of its combinations.
  CHECK(writeText(file, QStringLiteral("[shortcuts]\nx.line=G, L\n")));
  {
    mitcad::CommandRegistry registry;
    registry.add(command("x.line", "Line", QKeySequence()));
    CHECK(registry.shortcut(QStringLiteral("x.line")) == kGl);
  }
  // The comma key written by hand without the quotes: its comma is the
  // key, not a separator (mitcad#2096: "Ctrl+," came back as "Ctrl+, ").
  CHECK(writeText(file, QStringLiteral("[shortcuts]\nx.line=Ctrl+,\n")));
  {
    mitcad::CommandRegistry registry;
    registry.add(command("x.line", "Line", QKeySequence()));
    CHECK(registry.shortcut(QStringLiteral("x.line")) == QKeySequence(Qt::CTRL | Qt::Key_Comma));
  }
  CHECK(mitcad::shortcutTextFromList({QStringLiteral("Ctrl+"), QString()}) == QStringLiteral("Ctrl+,"));
  CHECK(mitcad::shortcutTextFromList({QStringLiteral("Ctrl+"), QString(), QStringLiteral("G")}) ==
        QStringLiteral("Ctrl+,, G"));
  CHECK(mitcad::shortcutTextFromList({QString(), QString()}) == QStringLiteral(","));
  CHECK(mitcad::shortcutTextFromList({QStringLiteral("G"), QString(), QString()}) == QStringLiteral("G, ,"));
  CHECK(mitcad::shortcutTextFromList({QStringLiteral("E"), QStringLiteral("X"), QStringLiteral("C")}) ==
        QStringLiteral("E, X, C"));
  CHECK(QKeySequence(mitcad::shortcutTextFromList({QStringLiteral("Ctrl+"), QString(), QStringLiteral("G")}),
                     QKeySequence::PortableText) == QKeySequence(Qt::CTRL | Qt::Key_Comma, QKeyCombination(Qt::Key_G)));
  // Through an exported settings file and back, unchanged.
  {
    QSettings settings;
    settings.clear();
    settings.setValue(QStringLiteral("shortcuts/x.line"), kExc.toString(QKeySequence::PortableText));
    settings.sync();
  }
  const QString exported = own.directory.filePath(QStringLiteral("keys.mitcad-settings"));
  QString error;
  CHECK(mitcad::writeSettingsFile(QSettings(), {QStringLiteral("shortcuts")}, exported, &error));
  const mitcad::SettingsFileContents contents = mitcad::readSettingsFile(exported);
  CHECK(contents.values.value(QStringLiteral("shortcuts/x.line")).toString() == QStringLiteral("E, X, C"));
  {
    QSettings settings;
    settings.clear();
    const mitcad::SettingsImportPlan plan = mitcad::planSettingsImport(contents, {QStringLiteral("shortcuts")}, settings);
    mitcad::applySettingsImport(settings, plan);
  }
  {
    mitcad::CommandRegistry registry;
    registry.add(command("x.line", "Line", QKeySequence()));
    CHECK(registry.shortcut(QStringLiteral("x.line")) == kExc);
  }
  // A file edited by hand, the sequence without its quotes: imported as the
  // text the window saves.
  QString edited = fileText(exported);
  edited.replace(QStringLiteral("x.line=\"E, X, C\""), QStringLiteral("x.line=G, L"));
  CHECK(edited.contains(QStringLiteral("x.line=G, L")));
  CHECK(writeText(exported, edited));
  const mitcad::SettingsImportPlan plan =
      mitcad::planSettingsImport(mitcad::readSettingsFile(exported), {QStringLiteral("shortcuts")}, QSettings());
  CHECK(plan.write.value(QStringLiteral("shortcuts/x.line")) == QVariant(QStringLiteral("G, L")));
  // Saved keys that clash as a start of another are disabled, with the
  // words of Keyboard Shortcuts in the warning; the default key stays.
  {
    QSettings settings;
    settings.clear();
    settings.setValue(QStringLiteral("shortcuts/x.line"), QStringLiteral("E, X"));
    settings.sync();
  }
  {
    mitcad::CommandRegistry registry;
    registry.add(command("solid.extrude", "Extrude", QKeySequence(Qt::Key_E)));
    registry.add(command("x.line", "Line", QKeySequence()));
    registry.reloadShortcuts();
    CHECK(registry.shortcutWarnings() ==
          QStringList{QStringLiteral("E (Extrude) is the start of E, X (Line) in the 3D view. Change one of them.")});
    CHECK(registry.effectiveShortcuts(QStringLiteral("x.line")).isEmpty());
    CHECK(registry.effectiveShortcuts(QStringLiteral("solid.extrude")) == QList<QKeySequence>{QKeySequence(Qt::Key_E)});
  }
}

// The cheat sheet of the overview from the user's keys: Extrude's default
// E, Fillet's F changed to Shift+F, Line's G, L.
void cheatSheetTests() {
  OwnSettings own;
  {
    QSettings settings;
    settings.setValue(QStringLiteral("shortcuts/solid.fillet"), QStringLiteral("Shift+F"));
    settings.setValue(QStringLiteral("shortcuts/x.line"), QStringLiteral("G, L"));
    settings.sync();
  }
  mitcad::CommandRegistry registry;
  registry.add(command("solid.extrude", "Extrude", QKeySequence(Qt::Key_E)));
  registry.add(command("solid.fillet", "Fillet", QKeySequence(Qt::Key_F)));
  registry.add(command("x.line", "Line", QKeySequence()));
  registry.add(command("x.none", "No Key", QKeySequence()));
  registry.reloadShortcuts();
  // The keys in the native text: "Shift+F" on Windows and Linux, the Shift
  // symbol and F on macOS.
  const QString shiftF = QKeySequence(Qt::SHIFT | Qt::Key_F).toString(QKeySequence::NativeText);
  const auto groupOf = [](const QString& id) {
    return id.startsWith(QStringLiteral("solid.")) ? QStringLiteral("Solid › Create")
                                                    : QStringLiteral("Sketch › Create");
  };
  QVector<mitcad::OverviewGroup> groups{{QStringLiteral("Mouse (MitCad)"),
                                         {{QStringLiteral("Middle drag"), QStringLiteral("Pan"), QStringLiteral("mouse.0"),
                                           QString()}}}};
  const QVector<mitcad::OverviewGroup> commands = mitcad::commandShortcutGroups(registry, groupOf);
  CHECK(commands.size() == 2);
  groups += commands;
  const mitcad::CheatSheet all =
      mitcad::shortcutCheatSheet(groups, QString(), QStringLiteral("MitCad"), QDate(2026, 11, 19));
  CHECK(all.title == QStringLiteral("Keyboard and Mouse Overview"));
  // The date set apart from the scheme's name, whose own commas would run
  // into it (mitcad#2096).
  CHECK(all.subtitle == QStringLiteral("Mouse: MitCad · 2026-11-19"));
  const mitcad::CheatSheet commas = mitcad::shortcutCheatSheet(groups, QString(),
                                                               QStringLiteral("Middle pans, Shift + middle orbits"),
                                                               QDate(2026, 11, 19));
  CHECK(commas.subtitle == QStringLiteral("Mouse: Middle pans, Shift + middle orbits · 2026-11-19"));
  CHECK(all.tables.size() == 3);
  if (all.tables.size() == 3) {
    CHECK(all.tables[0].heading == QStringLiteral("Mouse (MitCad)"));
    CHECK(all.tables[1].heading == QStringLiteral("Solid › Create"));
    CHECK(all.tables[2].heading == QStringLiteral("Sketch › Create"));
    for (const auto& table : all.tables) {
      CHECK(table.columns == (QStringList{QStringLiteral("Keys or mouse"), QStringLiteral("Does")}));
    }
    CHECK(all.tables[1].rows ==
          (QVector<QStringList>{{QStringLiteral("E"), QStringLiteral("Extrude")},
                                {shiftF, QStringLiteral("Fillet")}}));
    // The sequence in the native text; a command with no key is not listed.
    CHECK(all.tables[2].rows == (QVector<QStringList>{{QStringLiteral("G, L"), QStringLiteral("Line")}}));
  }
  // The search's rows only, by keys or by what they do.
  const mitcad::CheatSheet found =
      mitcad::shortcutCheatSheet(groups, QStringLiteral("line"), QStringLiteral("MitCad"), QDate(2026, 11, 19));
  CHECK(found.tables.size() == 1);
  // A sheet of a search says which search.
  CHECK(found.subtitle == QStringLiteral("Mouse: MitCad · Search: line · 2026-11-19"));
  CHECK(found.tables.size() == 1 && found.tables[0].heading == QStringLiteral("Sketch › Create") &&
        found.tables[0].rows.size() == 1);
  // By the key as the sheet shows it (on macOS its Shift is the symbol, so
  // the word "shift" finds nothing there).
  const mitcad::CheatSheet byKey =
      mitcad::shortcutCheatSheet(groups, shiftF.toLower(), QStringLiteral("MitCad"), QDate(2026, 11, 19));
  CHECK(byKey.tables.size() == 1 &&
        byKey.tables[0].rows == (QVector<QStringList>{{shiftF, QStringLiteral("Fillet")}}));
  // Several keys of one command: a sequence among them is set apart by ";".
  CHECK(mitcad::overviewKeysText({QKeySequence(Qt::Key_F2), QKeySequence(Qt::Key_F3)}) == QStringLiteral("F2, F3"));
  CHECK(mitcad::overviewKeysText({kGl, QKeySequence(Qt::Key_F3)}) == QStringLiteral("G, L; F3"));
  CHECK(mitcad::cheatSheetFileName() == QStringLiteral("MitCad keyboard shortcuts.pdf"));
}

} // namespace

int shortcutSequenceTests() {
  failures = 0;
  prefixTests();
  settingsTests();
  cheatSheetTests();
  return failures;
}
MITCAD_UNIT_TEST(shortcutSequenceTests);
