// SPDX-License-Identifier: MIT
#include "CommandSearchRank.hpp"

#include <algorithm>
#include <climits>
#include <initializer_list>
#include <tuple>

#include <QChar>
#include <QSettings>

#include "CommandLayout.hpp"

namespace mitcad::search {
namespace {

const char* kRecentCommandsKey = "commands/recent";

// A caption without its mnemonic's "&" ("&Tools" is Tools; "&&" is "&").
QString withoutMnemonic(const QString& text) {
  QString plain;
  for (qsizetype i = 0; i < text.size(); ++i) {
    if (text.at(i) == QLatin1Char('&') && i + 1 < text.size()) {
      ++i;
    }
    plain.append(text.at(i));
  }
  return plain;
}

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

const std::vector<std::pair<QString, QStringList>>& otherNames() {
  // Grouped as the toolbar is; plain function names, never a program's name.
  static const std::vector<std::pair<QString, QStringList>> names = [] {
    const auto row = [](const char* id, std::initializer_list<const char*> words) {
      QStringList list;
      for (const char* word : words) {
        list.append(QString::fromLatin1(word));
      }
      return std::pair<QString, QStringList>(QString::fromLatin1(id), list);
    };
    return std::vector<std::pair<QString, QStringList>>{
        // Solid
        row("solid.parameters", {"change parameters", "parameter table", "variables"}),
        row("solid.configurations", {"design table", "family table", "variants"}),
        row("solid.materials", {"manage materials", "material browser", "physical material"}),
        row("solid.appearances", {"manage appearances", "appearance browser"}),
        row("solid.material", {"physical material", "assign material"}),
        row("solid.appearance", {"assign appearance", "colour", "color"}),
        row("solid.press_pull", {"push pull"}),
        row("solid.move", {"move/copy", "transform"}),
        row("solid.combine", {"join", "intersect", "cut body"}),
        row("solid.hole", {"drill", "counterbore", "countersink"}),
        row("solid.thread", {"cosmetic thread", "tap"}),
        row("solid.emboss", {"deboss", "engrave"}),
        row("solid.form", {"create form", "sculpt"}),
        row("surface.stitch", {"knit", "sew"}),
        row("sheet.flat_pattern", {"flat blank", "unfold"}),
        // Assemble
        row("assemble.joint", {"mate", "assembly constraint"}),
        row("assemble.as_built_joint", {"joint in place"}),
        row("assemble.derived_component", {"derive", "insert derive", "derived part", "derived body"}),
        row("assemble.drive_joint", {"drive joints", "joint drive"}),
        row("assemble.contact_set", {"contact sets", "enable contact", "enable contact sets"}),
        row("insert.component", {"insert part", "place component"}),
        // Construct, inspect, insert
        row("construct.offset_plane", {"work plane", "datum plane", "reference plane"}),
        row("construct.coordinate_system", {"ucs", "datum coordinate system"}),
        row("inspect.section", {"section analysis", "section view"}),
        row("inspect.interference", {"interference check", "clash detection"}),
        row("inspect.properties", {"mass properties", "physical properties"}),
        row("insert.image_reference", {"canvas", "attached canvas", "underlay", "background image"}),
        row("insert.mesh", {"stl", "obj"}),
        row("file.render_image", {"render", "rendering", "ray trace"}),
        // Sketch
        row("sketch.project", {"project geometry", "include", "use edges", "convert entities"}),
        // Drawings
        row("drawing.parts_list", {"bill of materials", "bom"}),
        row("drawing.balloon", {"item number"}),
        // Manage (mitcad#1982)
        row("manage.recompute_all", {"compute all", "rebuild all", "regenerate all"}),
        // Views and tools
        row("view.fit", {"zoom extents"}),
        row("view.named_view", {"saved view"}),
        row("tools.preferences", {"options", "settings"}),
        // Decals (mitcad#2244)
        row("insert.decal", {"sticker", "label", "logo", "image on face", "texture on face"}),
    };
  }();
  return names;
}

QStringList withOtherNames(const QString& id, const QStringList& keywords) {
  QStringList all = keywords;
  for (const auto& [command, words] : otherNames()) {
    if (command != id) {
      continue;
    }
    for (const QString& word : words) {
      if (!all.contains(word, Qt::CaseInsensitive)) {
        all.append(word);
      }
    }
  }
  return all;
}

QStringList placeTexts(const CommandLayout& layout, const QString& id, const PlaceCaptions& captions) {
  const auto ribbon = [&captions](const QString& key) { return captions.ribbon ? captions.ribbon(key) : key; };
  const auto menu = [&captions](const QString& key) {
    return withoutMnemonic(captions.menu ? captions.menu(key) : key);
  };
  const QString arrow = QStringLiteral(" › ");
  QStringList groups;
  QStringList menus;
  // The home first, whatever order the user's layer gave the others.
  const LayoutPlace home = layout.home(id);
  if (!home.isEmpty()) {
    groups.append(ribbon(home.tab) + arrow + ribbon(home.group));
  }
  for (const QString& place : layout.placesOf(id)) {
    const QStringList keys = place.split(QLatin1Char('/'));
    if (keys.size() < 2) {
      continue;
    }
    if (keys.front() == QLatin1String("menu")) {
      const QString text = captions.menuBar + arrow + menu(keys.at(1));
      if (!menus.contains(text)) {
        menus.append(text);
      }
      continue;
    }
    const QString text = ribbon(keys.at(0)) + arrow + ribbon(keys.at(1));
    if (!groups.contains(text)) {
      groups.append(text);
    }
  }
  return groups + menus;
}

QString placeLine(const CommandLayout& layout, const QString& id, const PlaceCaptions& captions) {
  const QStringList texts = placeTexts(layout, id, captions);
  return texts.isEmpty() ? captions.nowhere : texts.join(QStringLiteral(", "));
}

} // namespace mitcad::search
