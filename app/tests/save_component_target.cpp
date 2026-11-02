// SPDX-License-Identifier: MIT
// Save Component as Design's guard (mitcad#1148): the open design's own
// file is refused however its path is spelled (a `..` path, a path through
// a symbolic link, a name that differs only in case where the disk ignores
// case), and so is a file a component of the design is linked from;
// another file, and a name that differs in case where the disk tells case
// apart, may be written.
#include "files/SaveComponentTarget.hpp"

#include <cstdio>

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "save_component_target.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool writeFile(const QString& path, const QByteArray& bytes) {
  QFile file(path);
  return file.open(QIODevice::WriteOnly) && file.write(bytes) == bytes.size();
}

} // namespace

int saveComponentTargetTests() {
  using mitcad::SaveComponentClash;
  using mitcad::saveComponentClash;
  using mitcad::sameFilePath;
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QDir root(QFileInfo(directory.path()).absoluteFilePath());
  CHECK(root.mkpath(QStringLiteral("parts")));
  const QString design = root.filePath(QStringLiteral("Lid.mitcad"));
  const QString part = root.filePath(QStringLiteral("parts/Box.mitcad"));
  CHECK(writeFile(design, "{}"));
  CHECK(writeFile(part, "{}"));
  const QStringList linked{part};

  // The design's file itself, and by a `..` path.
  CHECK(saveComponentClash(design, design, linked) == SaveComponentClash::Design);
  const QString dotted = root.filePath(QStringLiteral("parts/../Lid.mitcad"));
  CHECK(sameFilePath(dotted, design));
  CHECK(saveComponentClash(dotted, design, linked) == SaveComponentClash::Design);

  // A name that differs only in case: the same file where the disk ignores
  // case (it exists then), another file where the disk tells case apart.
  const QString lower = root.filePath(QStringLiteral("lid.mitcad"));
  const bool ignoresCase = QFileInfo::exists(lower);
  CHECK(saveComponentClash(lower, design, linked) ==
        (ignoresCase ? SaveComponentClash::Design : SaveComponentClash::None));
#ifdef _WIN32
  CHECK(ignoresCase);
#endif

#ifndef _WIN32
  // Through a symbolic link to the design's folder.
  const QString through = root.filePath(QStringLiteral("through"));
  CHECK(QFile::link(root.path(), through));
  CHECK(saveComponentClash(QDir(through).filePath(QStringLiteral("Lid.mitcad")), design, linked) ==
        SaveComponentClash::Design);
  CHECK(saveComponentClash(QDir(through).filePath(QStringLiteral("parts/Box.mitcad")), design, linked) ==
        SaveComponentClash::Linked);
#endif

  // A file a component is linked from, also by a `..` path.
  CHECK(saveComponentClash(part, design, linked) == SaveComponentClash::Linked);
  CHECK(saveComponentClash(root.filePath(QStringLiteral("parts/../parts/Box.mitcad")), design, linked) ==
        SaveComponentClash::Linked);

  // Other files: a new one, an existing one, and any file for a design
  // without a file of its own.
  const QString other = root.filePath(QStringLiteral("Box.mitcad"));
  CHECK(saveComponentClash(other, design, linked) == SaveComponentClash::None);
  const QString existing = root.filePath(QStringLiteral("Other.mitcad"));
  CHECK(writeFile(existing, "{}"));
  CHECK(saveComponentClash(existing, design, linked) == SaveComponentClash::None);
  CHECK(saveComponentClash(design, QString(), QStringList()) == SaveComponentClash::None);
  CHECK(!sameFilePath(QString(), design));

  // Neither exists: the same cleaned path.
  CHECK(sameFilePath(root.filePath(QStringLiteral("gone/../New.mitcad")), root.filePath(QStringLiteral("New.mitcad"))));
  CHECK(!sameFilePath(root.filePath(QStringLiteral("New.mitcad")), root.filePath(QStringLiteral("Newer.mitcad"))));
  return failures;
}
