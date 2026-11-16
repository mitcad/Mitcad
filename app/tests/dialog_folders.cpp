// SPDX-License-Identifier: MIT
// The file dialogs' folders per group (mitcad#498): a folder accepted in a
// group is where that group's dialogs start and no other group's, across a
// new settings object (a restart); a file accepted gives its folder, an
// application bundle (a folder) the folder it is in; a folder that is gone falls back to the caller's; Save
// Copy As neither reads nor writes one.
#include "UnitTests.hpp"
#include "framework/DialogFolders.hpp"

#include <cstdio>

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QSettings>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "dialog_folders.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

} // namespace

int dialogFolderTests() {
  using mitcad::DialogGroup;
  using mitcad::dialogStartFolder;
  using mitcad::rememberDialogFolder;
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QString root = QFileInfo(directory.path()).absoluteFilePath();
  const QString design = root + QStringLiteral("/design");
  const QString imports = root + QStringLiteral("/imports");
  const QString exports = root + QStringLiteral("/exports");
  const QString opened = root + QStringLiteral("/opened");
  for (const QString& folder : {design, imports, exports, opened}) {
    CHECK(QDir().mkpath(folder));
  }
  const QString ini = root + QStringLiteral("/settings.ini");

  {
    QSettings settings(ini, QSettings::IniFormat);
    // Nothing remembered: the caller's folder.
    for (const DialogGroup group : {DialogGroup::Open, DialogGroup::Export, DialogGroup::Import,
                                    DialogGroup::Texture, DialogGroup::Program, DialogGroup::SaveCopy}) {
      CHECK(dialogStartFolder(settings, group, design) == design);
    }
    // A file accepted: its folder, for its group only.
    rememberDialogFolder(settings, DialogGroup::Import, imports + QStringLiteral("/part.stl"));
    CHECK(dialogStartFolder(settings, DialogGroup::Import, design) == imports);
    CHECK(dialogStartFolder(settings, DialogGroup::Export, design) == design);
    CHECK(dialogStartFolder(settings, DialogGroup::Open, design) == design);
    // A file to be written (not there yet), and an application bundle: the
    // folder it is in, not the bundle.
    rememberDialogFolder(settings, DialogGroup::Export, exports + QStringLiteral("/new.step"));
    CHECK(QDir().mkpath(opened + QStringLiteral("/Slicer.app/Contents")));
    rememberDialogFolder(settings, DialogGroup::Open, opened + QStringLiteral("/Slicer.app"));
    CHECK(dialogStartFolder(settings, DialogGroup::Export, design) == exports);
    CHECK(dialogStartFolder(settings, DialogGroup::Open, design) == opened);
    CHECK(dialogStartFolder(settings, DialogGroup::Import, design) == imports);
    CHECK(settings.value(QStringLiteral("fileDialogs/export")).toString() == exports);
    // Nothing for an empty path (a cancelled dialog).
    rememberDialogFolder(settings, DialogGroup::Import, QString());
    CHECK(dialogStartFolder(settings, DialogGroup::Import, design) == imports);
    // Save Copy As: the design's folder, whatever the others remember, and
    // nothing remembered for it.
    rememberDialogFolder(settings, DialogGroup::SaveCopy, exports + QStringLiteral("/copy.mitcad"));
    CHECK(dialogStartFolder(settings, DialogGroup::SaveCopy, design) == design);
    const QStringList keys = settings.allKeys();
    CHECK(keys.size() == 3);
    for (const QString& key : keys) {
      CHECK(key.startsWith(QStringLiteral("fileDialogs/")));
    }
    settings.sync();
  }

  {
    // A restart: read again from the file.
    QSettings settings(ini, QSettings::IniFormat);
    CHECK(dialogStartFolder(settings, DialogGroup::Import, design) == imports);
    CHECK(dialogStartFolder(settings, DialogGroup::Export, design) == exports);
    CHECK(dialogStartFolder(settings, DialogGroup::Open, design) == opened);
    CHECK(dialogStartFolder(settings, DialogGroup::Texture, design) == design);
    // A folder that is gone: the caller's again, the others unchanged.
    CHECK(QDir(imports).removeRecursively());
    CHECK(dialogStartFolder(settings, DialogGroup::Import, design) == design);
    CHECK(dialogStartFolder(settings, DialogGroup::Export, design) == exports);
    // A file where the folder was is no folder either.
    QFile file(exports);
    CHECK(QDir(exports).removeRecursively());
    CHECK(file.open(QIODevice::WriteOnly));
    file.close();
    CHECK(dialogStartFolder(settings, DialogGroup::Export, design) == design);
    // The fallback as given, even empty.
    CHECK(dialogStartFolder(settings, DialogGroup::Program, QString()).isEmpty());
  }
  return failures;
}
MITCAD_UNIT_TEST(dialogFolderTests);
