// SPDX-License-Identifier: MIT
#include "SaveComponentTarget.hpp"

#include <filesystem>
#include <system_error>

#include <QDir>
#include <QFileInfo>

namespace mitcad {
namespace {

// The path with links and ".." resolved where it exists, else cleaned.
QString normalPath(const QString& path) {
  const QFileInfo info(path);
  const QString canonical = info.canonicalFilePath();
  return canonical.isEmpty() ? QDir::cleanPath(info.absoluteFilePath()) : canonical;
}

} // namespace

bool sameFilePath(const QString& a, const QString& b) {
  if (a.isEmpty() || b.isEmpty()) {
    return false;
  }
  const QFileInfo first(a);
  const QFileInfo second(b);
  const bool firstExists = first.exists();
  const bool secondExists = second.exists();
  if (firstExists != secondExists) {
    // On a disk that ignores case a name differing only in case exists
    // too: one missing is another file.
    return false;
  }
  if (firstExists) {
    // The same file on disk (its volume and number), whatever the names.
    std::error_code error;
    const bool same =
        std::filesystem::equivalent(first.filesystemAbsoluteFilePath(), second.filesystemAbsoluteFilePath(), error);
    if (!error) {
      return same;
    }
  }
#if defined(_WIN32) || defined(__APPLE__)
  return normalPath(a).compare(normalPath(b), Qt::CaseInsensitive) == 0;
#else
  return normalPath(a) == normalPath(b);
#endif
}

SaveComponentClash saveComponentClash(const QString& target, const QString& design, const QStringList& linked) {
  if (!design.isEmpty() && sameFilePath(target, design)) {
    return SaveComponentClash::Design;
  }
  for (const QString& file : linked) {
    if (sameFilePath(target, file)) {
      return SaveComponentClash::Linked;
    }
  }
  return SaveComponentClash::None;
}

} // namespace mitcad
