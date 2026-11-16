// SPDX-License-Identifier: MIT
// A command's former id (mitcad#503: File > Project > Move to a Project
// became File > Save to Project, `file.move_to_project` an alias of
// `file.save_to_project`): the registry finds the command by it, and a
// shortcut saved under it moves to the new id, unless the new id has one.
#include "UnitTests.hpp"
#include "framework/CommandRegistry.hpp"

#include <cstdio>

#include <QCoreApplication>
#include <QDir>
#include <QKeySequence>
#include <QSettings>
#include <QString>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "command_alias.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

mitcad::CommandDef action(const char* id) {
  mitcad::CommandDef def;
  def.id = QString::fromLatin1(id);
  def.name = def.id;
  def.kind = mitcad::CommandDef::Kind::Action;
  def.mode = mitcad::CommandDef::Mode::Any;
  return def;
}

} // namespace

int commandAliasTests() {
  // The settings of the test's own: the registry reads and writes the
  // application's default ones.
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  const QSettings::Format format = QSettings::defaultFormat();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadAliasTest"));
  QCoreApplication::setApplicationName(QStringLiteral("MitcadAliasTest"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
  {
    QSettings settings;
    settings.clear();
    settings.setValue(QStringLiteral("shortcuts/file.move_to_project"), QStringLiteral("F12"));
    settings.sync();
  }
  {
    mitcad::CommandRegistry registry;
    registry.add(action("file.save_to_project"));
    CHECK(registry.find(QStringLiteral("file.move_to_project")) == nullptr);
    registry.alias(QStringLiteral("file.move_to_project"), QStringLiteral("file.save_to_project"));
    const mitcad::CommandDef* found = registry.find(QStringLiteral("file.move_to_project"));
    CHECK(found != nullptr && found->id == QStringLiteral("file.save_to_project"));
    CHECK(registry.find(QStringLiteral("file.save_to_project")) == found);
    CHECK(registry.find(QStringLiteral("file.unknown")) == nullptr);
    // The shortcut saved under the old id is the new command's.
    CHECK(registry.shortcut(QStringLiteral("file.save_to_project")) == QKeySequence(Qt::Key_F12));
    QSettings settings;
    CHECK(!settings.contains(QStringLiteral("shortcuts/file.move_to_project")));
  }
  {
    // A shortcut the new id has already stays; the old one is dropped.
    QSettings settings;
    settings.setValue(QStringLiteral("shortcuts/file.move_to_project"), QStringLiteral("F11"));
    settings.setValue(QStringLiteral("shortcuts/file.save_to_project"), QStringLiteral("F10"));
    settings.sync();
    mitcad::CommandRegistry registry;
    registry.add(action("file.save_to_project"));
    registry.alias(QStringLiteral("file.move_to_project"), QStringLiteral("file.save_to_project"));
    CHECK(registry.shortcut(QStringLiteral("file.save_to_project")) == QKeySequence(Qt::Key_F10));
    CHECK(!settings.contains(QStringLiteral("shortcuts/file.move_to_project")));
  }
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  QSettings::setDefaultFormat(format);
  return failures;
}
MITCAD_UNIT_TEST(commandAliasTests);
