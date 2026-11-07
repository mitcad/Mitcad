// SPDX-License-Identifier: MIT
#include "LinkedFiles.hpp"

#include <QDir>
#include <QFileInfo>
#include <QRegularExpression>

namespace mitcad {

std::optional<LinkedRefusal> linkedRefusal(const QString& message) {
  // core/model/src/document/components.rs, DocState::editable. The name
  // starts the message or follows a command's own words ("…: ").
  static const QRegularExpression refusal(
      QStringLiteral(R"((?:^|:\s)([^:\n]+?) is linked from (.+?); it changes only with that file)"));
  const QRegularExpressionMatch match = refusal.match(message);
  if (!match.hasMatch()) {
    return std::nullopt;
  }
  return LinkedRefusal{match.captured(1).trimmed(), match.captured(2).trimmed()};
}

QString linkedFilePath(const QString& designFile, const QString& path) {
  if (path.isEmpty()) {
    return QString();
  }
  if (QDir::isAbsolutePath(path)) {
    return QDir::cleanPath(path);
  }
  if (designFile.isEmpty()) {
    return QString();
  }
  const QDir folder = QFileInfo(designFile).absoluteDir();
  return QDir::cleanPath(folder.absoluteFilePath(path));
}

bool sameFile(const QString& a, const QString& b) {
  if (a.isEmpty() || b.isEmpty()) {
    return false;
  }
  const QString canonicalA = QFileInfo(a).canonicalFilePath();
  const QString canonicalB = QFileInfo(b).canonicalFilePath();
  if (!canonicalA.isEmpty() && !canonicalB.isEmpty()) {
    return canonicalA == canonicalB;
  }
  return QDir::cleanPath(QFileInfo(a).absoluteFilePath()) == QDir::cleanPath(QFileInfo(b).absoluteFilePath());
}

} // namespace mitcad
