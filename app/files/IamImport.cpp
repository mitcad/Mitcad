// SPDX-License-Identifier: MIT
#include "IamImport.hpp"

#include <initializer_list>

#include <QCoreApplication>
#include <QRegularExpression>
#include <QStringList>

namespace mitcad {
namespace {

const QString kPartsHistory = QStringLiteral("import/iamPartsHistory");

// The file's texts in the context IamImport, where lupdate records them too
// (mitcad#1472).
struct Text {
  Q_DECLARE_TR_FUNCTIONS(IamImport)
};

} // namespace

bool iamPartsHistory(const QSettings& settings) { return settings.value(kPartsHistory, true).toBool(); }

void rememberIamPartsHistory(QSettings& settings, bool history) { settings.setValue(kPartsHistory, history); }

std::optional<IamPartProgress> parseIamPartProgress(const QString& line) {
  static const QRegularExpression pattern(QStringLiteral(
      R"(^import: \[[0-9.]+\] part ([0-9]+) of ([0-9]+): (.+?)(?:: (replayed|bodies) in ([0-9.]+) s)?$)"));
  const QRegularExpressionMatch match = pattern.match(line);
  if (!match.hasMatch()) {
    return std::nullopt;
  }
  IamPartProgress progress;
  progress.part = match.captured(1).toInt();
  progress.parts = match.captured(2).toInt();
  progress.file = match.captured(3);
  progress.outcome = match.captured(4);
  progress.seconds = match.captured(5).toDouble();
  return progress;
}

QString iamPartOutcome(const QJsonObject& part) {
  const QString outcome = part.value(QStringLiteral("outcome")).toString();
  if (outcome == QLatin1String("replayed")) {
    const QJsonObject counts = part.value(QStringLiteral("counts")).toObject();
    const auto count = [&counts](const char* kind) { return counts.value(QLatin1String(kind)).toInt(); };
    // Each count with its kind in a whole translated text (mitcad#1472).
    const QStringList parts = {Text::tr("%1 parametric").arg(count("parametric")),
                               Text::tr("%1 partial").arg(count("partial")),
                               Text::tr("%1 fallback").arg(count("fallback")),
                               Text::tr("%1 skipped").arg(count("skipped"))};
    return Text::tr("replayed, %1 items (%2), %3 verified, %4 parameters")
        .arg(part.value(QStringLiteral("items")).toInt())
        .arg(parts.join(QStringLiteral(", ")))
        .arg(part.value(QStringLiteral("verified")).toInt())
        .arg(part.value(QStringLiteral("parameters")).toInt());
  }
  if (outcome == QLatin1String("bodies")) {
    return Text::tr("stored bodies: %1").arg(part.value(QStringLiteral("reason")).toString());
  }
  return QString();
}

} // namespace mitcad
