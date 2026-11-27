// SPDX-License-Identifier: MIT
#include "ParameterReferences.hpp"

#include <QJsonObject>
#include <QJsonValue>
#include <QLatin1String>

namespace mitcad::drawings {
namespace {

const QLatin1String kPrefix("param:");

// The parameter a name in braces refers to: param:Length gives Length;
// empty for another name or an empty one.
QString referenced(const QString& key) {
  const QString trimmed = key.trimmed();
  if (!trimmed.startsWith(kPrefix, Qt::CaseInsensitive)) {
    return QString();
  }
  return trimmed.mid(kPrefix.size()).trimmed();
}

} // namespace

QString parameterReference(const QString& name) {
  return QStringLiteral("{param:%1}").arg(name);
}

QStringList referencedParameters(const QString& text) {
  // As the model reads them: from a brace to the next brace; a closing one
  // ends a name in braces, an opening one starts again from there.
  QStringList out;
  qsizetype from = 0;
  while (true) {
    const qsizetype open = text.indexOf(QLatin1Char('{'), from);
    if (open < 0) {
      break;
    }
    qsizetype end = open + 1;
    while (end < text.size() && text.at(end) != QLatin1Char('{') && text.at(end) != QLatin1Char('}')) {
      ++end;
    }
    if (end >= text.size() || text.at(end) == QLatin1Char('{')) {
      from = open + 1;
      continue;
    }
    const QString name = referenced(text.mid(open + 1, end - open - 1));
    if (name.isEmpty()) {
      from = open + 1;
      continue;
    }
    if (!out.contains(name)) {
      out.append(name);
    }
    from = end + 1;
  }
  return out;
}

QStringList missingParameters(const QString& text, const QJsonArray& parameters) {
  QStringList known;
  for (const QJsonValue& parameter : parameters) {
    known.append(parameter.toObject().value(QStringLiteral("name")).toString());
  }
  QStringList out;
  for (const QString& name : referencedParameters(text)) {
    if (!known.contains(name)) {
      out.append(name);
    }
  }
  return out;
}

QList<ParameterEntry> parameterEntries(const QJsonArray& parameters) {
  QList<ParameterEntry> users;
  QList<ParameterEntry> others;
  for (const QJsonValue& value : parameters) {
    const QJsonObject parameter = value.toObject();
    ParameterEntry entry;
    entry.name = parameter.value(QStringLiteral("name")).toString();
    if (entry.name.isEmpty()) {
      continue;
    }
    entry.value = parameter.value(QStringLiteral("text")).toString();
    // An angle in degrees as the sheet shows it (docs/ui-conventions.md:
    // "deg" only in expressions).
    if (entry.value.endsWith(QLatin1String(" deg"))) {
      entry.value = entry.value.chopped(4) + QStringLiteral("°");
    }
    entry.user = parameter.value(QStringLiteral("kind")).toString() == QLatin1String("user");
    (entry.user ? users : others).append(entry);
  }
  return users + others;
}

} // namespace mitcad::drawings
