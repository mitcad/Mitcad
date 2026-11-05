// SPDX-License-Identifier: MIT
#include "mcp/UiCommandList.hpp"

#include <algorithm>
#include <tuple>
#include <vector>

#include <QJsonArray>
#include <QRegularExpression>

#include "framework/CommandRegistry.hpp"

namespace mitcad::mcp {

namespace {

// The words of an id or a name, lower case: `file.new_design` has "file",
// "new" and "design", "New Design" "new" and "design".
QStringList wordsOf(const QString& text) {
  static const QRegularExpression separators(QStringLiteral("[._\\-\\s]+"));
  return text.toLower().split(separators, Qt::SkipEmptyParts);
}

// The fewest one-character edits that make `a` into `b` (Levenshtein).
int edits(const QString& a, const QString& b) {
  std::vector<int> row(static_cast<size_t>(b.size()) + 1);
  for (size_t j = 0; j < row.size(); ++j) {
    row[j] = static_cast<int>(j);
  }
  for (qsizetype i = 1; i <= a.size(); ++i) {
    int diagonal = row[0];
    row[0] = static_cast<int>(i);
    for (qsizetype j = 1; j <= b.size(); ++j) {
      const auto at = static_cast<size_t>(j);
      const int above = row[at];
      const int cost = a.at(i - 1) == b.at(j - 1) ? 0 : 1;
      row[at] = std::min({above + 1, row[at - 1] + 1, diagonal + cost});
      diagonal = above;
    }
  }
  return row.back();
}

QString kindName(const CommandDef& def) {
  switch (def.kind) {
  case CommandDef::Kind::Feature:
    return QStringLiteral("feature");
  case CommandDef::Kind::Action:
    return QStringLiteral("action");
  case CommandDef::Kind::Tool:
    return QStringLiteral("tool");
  }
  return QStringLiteral("action");
}

} // namespace

bool opensDialog(const QString& id) {
  // This creates an empty document owner without a file, modal question or
  // external application. All existing dialog/file refusals still apply.
  if (id == QStringLiteral("window.new")) return false;
  for (const char* prefix : {"file.", "help.", "tools.", "insert."}) {
    if (id.startsWith(QLatin1String(prefix))) {
      return true;
    }
  }
  for (const char* known : {"sketch.insert_dxf", "sketch.import_dxf", "sketch.insert_svg", "sketch.import_svg",
                            "sheet.export_dxf", "make.print3d",
                            "view.render_environment", "view.grid_settings", "solid.parameters",
                            "sketch.parameters", "solid.configurations"}) {
    if (id == QLatin1String(known)) {
      return true;
    }
  }
  return false;
}

QStringList nearestCommandIds(const CommandRegistry& registry, const QString& wanted, int count) {
  const QString text = wanted.trimmed().toLower();
  const QStringList words = wordsOf(text);
  // (fewer shared words first negated, edits, id) per command.
  std::vector<std::tuple<int, int, QString>> ranked;
  for (const auto& def : registry.commands()) {
    QStringList own = wordsOf(def->id);
    own += wordsOf(def->name);
    int shared = 0;
    for (const QString& word : words) {
      if (own.contains(word)) {
        ++shared;
      }
    }
    const int distance = std::min(edits(text, def->id.toLower()), edits(text, def->name.toLower()));
    ranked.emplace_back(-shared, distance, def->id);
  }
  std::sort(ranked.begin(), ranked.end());
  QStringList nearest;
  for (const auto& entry : ranked) {
    if (nearest.size() >= count) {
      break;
    }
    nearest.append(std::get<2>(entry));
  }
  return nearest;
}

QString nearestCommandsText(const CommandRegistry& registry, const QString& wanted) {
  QStringList named;
  for (const QString& id : nearestCommandIds(registry, wanted)) {
    const CommandDef* def = registry.find(id);
    named.append(def != nullptr && def->name != id ? QStringLiteral("%1 (%2)").arg(id, def->name) : id);
  }
  const QString listing = QStringLiteral(
      "ui_command with search lists the commands and whether an agent may run each (\"\" lists all, words such as "
      "\"fillet\" find them).");
  if (named.isEmpty()) {
    return listing;
  }
  return QStringLiteral("Nearest ids: %1. %2").arg(named.join(QStringLiteral(", ")), listing);
}

QJsonObject commandListing(const CommandRegistry& registry, const QString& search,
                           const std::function<bool(const CommandDef&)>& availableNow) {
  const QStringList words = search.toLower().split(QRegularExpression(QStringLiteral("\\s+")), Qt::SkipEmptyParts);
  QJsonArray commands;
  for (const auto& def : registry.commands()) {
    const QString found =
        QStringList{def->id, def->name, def->keywords.join(QLatin1Char(' '))}.join(QLatin1Char(' ')).toLower();
    const bool matches =
        std::all_of(words.cbegin(), words.cend(), [&found](const QString& word) { return found.contains(word); });
    if (!matches) {
      continue;
    }
    const bool agents = !opensDialog(def->id);
    QJsonObject entry{{QStringLiteral("id"), def->id},
                      {QStringLiteral("name"), def->name},
                      {QStringLiteral("kind"), kindName(*def)},
                      {QStringLiteral("agents"), agents},
                      {QStringLiteral("available"), availableNow ? availableNow(*def) : true}};
    if (!agents) {
      entry.insert(QStringLiteral("why"),
                   QStringLiteral("it opens a file dialog or another window, which an agent cannot use"));
    }
    commands.append(entry);
  }
  return QJsonObject{{QStringLiteral("search"), search},
                     {QStringLiteral("commands"), commands},
                     {QStringLiteral("count"), static_cast<qint64>(commands.size())},
                     {QStringLiteral("total"), static_cast<qint64>(registry.commands().size())}};
}

} // namespace mitcad::mcp
