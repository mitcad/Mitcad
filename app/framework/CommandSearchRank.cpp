// SPDX-License-Identifier: MIT
#include "CommandSearchRank.hpp"

#include <algorithm>
#include <climits>
#include <tuple>

#include <QChar>
#include <QSettings>

namespace mitcad::search {
namespace {

const char* kRecentCommandsKey = "commands/recent";

QString withoutEllipsis(QString text) {
  if (text.endsWith(QStringLiteral("..."))) {
    text.chop(3);
  } else if (text.endsWith(QChar(0x2026))) {
    text.chop(1);
  }
  return text;
}

// The score of one name of the command (shown or English).
int nameScore(const QString& shown, const QStringList& keywords, const QString& text) {
  if (text.isEmpty()) {
    return 6;
  }
  const QString name = shown.toLower();
  // The whole name first, so "export" finds Export... before Export Drawing...
  if (withoutEllipsis(name) == text) {
    return 0;
  }
  if (name.startsWith(text)) {
    return 1;
  }
  const QStringList words = name.split(QLatin1Char(' '), Qt::SkipEmptyParts);
  if (std::any_of(words.begin(), words.end(), [&](const QString& w) { return w.startsWith(text); })) {
    return 2;
  }
  if (name.contains(text)) {
    return 3;
  }
  for (const QString& keyword : keywords) {
    if (keyword.toLower().startsWith(text)) {
      return 4;
    }
  }
  // The letters in order, as in "fll" for Fillet.
  int at = 0;
  for (const QChar c : text) {
    at = static_cast<int>(name.indexOf(c, at));
    if (at < 0) {
      return -1;
    }
    ++at;
  }
  return 5;
}

} // namespace

QString normalizedText(const QString& typed) {
  return withoutEllipsis(typed.trimmed().toLower()).trimmed();
}

// The name the interface shows, and in another language its English name
// as well (mitcad#1460, mitcad#1284): "extrude" finds Extrude whatever the
// language.
int score(const Candidate& candidate, const QString& text) {
  const int shown = nameScore(candidate.shown, candidate.keywords, text);
  const bool translated = !candidate.english.isEmpty() && candidate.english != candidate.shown;
  const int other = translated ? nameScore(candidate.english, candidate.keywords, text) : -1;
  if (shown >= 0 || other >= 0) {
    return shown < 0 ? other : (other < 0 ? shown : std::min(shown, other));
  }
  // Each word of the text on its own, in any order (mitcad#1682: Finnish
  // writes a compound as one word and users type it with a space,
  // "keskipiste suorakulmio" for Keskipistesuorakulmio, Center Rectangle;
  // "rectangle center" finds it in English): every word somewhere in the
  // shown or the English name, or in a keyword. After whole-text matches.
  const QStringList words = text.split(QLatin1Char(' '), Qt::SkipEmptyParts);
  if (words.size() < 2) {
    return -1;
  }
  const QString shownName = candidate.shown.toLower();
  const QString englishName = candidate.english.toLower();
  for (const QString& word : words) {
    const bool inKeyword = std::any_of(candidate.keywords.begin(), candidate.keywords.end(),
                                       [&word](const QString& keyword) { return keyword.toLower().contains(word); });
    if (!shownName.contains(word) && !englishName.contains(word) && !inKeyword) {
      return -1;
    }
  }
  return 6;
}

Listing list(const std::vector<Candidate>& candidates, const QString& text, const QStringList& recent) {
  struct Match {
    int index;
    int rank;
    int recency; // its place in the recent list, INT_MAX when not in it
  };
  std::vector<Match> matches;
  int bestEnabled = INT_MAX;
  for (int i = 0; i < static_cast<int>(candidates.size()); ++i) {
    const Candidate& candidate = candidates[static_cast<size_t>(i)];
    const int rank = score(candidate, text);
    if (rank < 0) {
      continue;
    }
    const int place = candidate.id.isEmpty() ? -1 : static_cast<int>(recent.indexOf(candidate.id));
    matches.push_back({i, rank, place < 0 ? INT_MAX : place});
    if (candidates[static_cast<size_t>(i)].enabled) {
      bestEnabled = std::min(bestEnabled, rank);
    }
  }
  // The tier of a match: 0 waits for the mode and matches better than any
  // command that can run (mitcad#1681), 1 can run now, 2 cannot. Matches of
  // the same rank keep the commands that can run first; among them the
  // commands run last come first (mitcad#1690: "keskipiste" in a sketch
  // selects Center Rectangle after it was used), then the name.
  const auto tier = [&](const Match& match) {
    const Candidate& candidate = candidates[static_cast<size_t>(match.index)];
    if (candidate.enabled) {
      return 1;
    }
    return !candidate.modeHint.isEmpty() && match.rank < bestEnabled ? 0 : 2;
  };
  std::sort(matches.begin(), matches.end(), [&](const Match& a, const Match& b) {
    const int tierA = tier(a);
    const int tierB = tier(b);
    const QString& nameA = candidates[static_cast<size_t>(a.index)].shown;
    const QString& nameB = candidates[static_cast<size_t>(b.index)].shown;
    return std::tie(tierA, a.rank, a.recency, nameA, a.index) < std::tie(tierB, b.rank, b.recency, nameB, b.index);
  });
  Listing listing;
  for (const Match& match : matches) {
    listing.rows.push_back(match.index);
  }
  if (!matches.empty()) {
    const Match& first = matches.front();
    const Candidate& best = candidates[static_cast<size_t>(first.index)];
    if (tier(first) == 0) {
      listing.hint = best.modeHint;
    } else if (best.enabled) {
      listing.current = 0;
    }
  }
  return listing;
}

QStringList withRecentCommand(const QStringList& recent, const QString& id, int limit) {
  if (id.isEmpty()) {
    return recent;
  }
  QStringList updated = recent;
  updated.removeAll(id);
  updated.prepend(id);
  while (updated.size() > limit) {
    updated.removeLast();
  }
  return updated;
}

QStringList recentCommands() {
  return QSettings().value(QLatin1String(kRecentCommandsKey)).toStringList();
}

void rememberRecentCommand(const QString& id) {
  const QStringList before = recentCommands();
  const QStringList after = withRecentCommand(before, id);
  // Not written again for the command already first (Undo pressed again).
  if (after != before) {
    QSettings().setValue(QLatin1String(kRecentCommandsKey), after);
  }
}

} // namespace mitcad::search
