// SPDX-License-Identifier: MIT
#include "PickerNewFolder.hpp"

#include <QCoreApplication>
#include <QSet>

namespace mitcad {
namespace {

// The texts in the picker's context, ProjectPicker (mitcad#1472).
struct Text {
  Q_DECLARE_TR_FUNCTIONS(ProjectPicker)
};

} // namespace

QString freeFolderName(const QString& base, const QStringList& taken) {
  QSet<QString> lower;
  for (const QString& name : taken) {
    lower.insert(name.toLower());
  }
  for (int n = 1;; ++n) {
    const QString name = n == 1 ? base : QStringLiteral("%1 %2").arg(base).arg(n);
    if (!lower.contains(name.toLower())) {
      return name;
    }
  }
}

QString folderNameOf(const QString& typed) {
  const QString name = typed.trimmed();
  if (name.isEmpty() || name.startsWith(QLatin1Char('.')) || name.endsWith(QLatin1Char('.'))) {
    return {};
  }
  static const QString forbidden = QStringLiteral("/\\<>:\"|?*");
  for (const QChar c : name) {
    if (forbidden.contains(c) || c.unicode() < 0x20 || c.category() == QChar::Other_Control) {
      return {};
    }
  }
  return name;
}

QString newFolderBlocked(FolderHost host, bool writable) {
  switch (host) {
  case FolderHost::None:
    return Text::tr("Select a project or a folder to hold the new folder.");
  case FolderHost::Reading:
    return Text::tr("Waiting for the projects to be read.");
  case FolderHost::Naming:
    return Text::tr("Type the new folder's name first: Enter makes it, Esc takes it away.");
  case FolderHost::Place:
    break;
  }
  if (!writable) {
    return Text::tr("The folder is read-only: no folder can be made in it.");
  }
  return {};
}

} // namespace mitcad
