// SPDX-License-Identifier: MIT
// The command search's list (mitcad#1681, framework/CommandSearchRank.hpp):
// outside a sketch the sketch tools that match a search best wait for a
// sketch, so a hint row above them says how to open one and no weaker match
// that can run is selected (Enter would start it); inside a sketch the
// sketch tools can run and the best of them is selected. Matches of the
// same rank keep the commands that can run first, and a command that cannot
// run for another reason (nothing to work on) never brings the hint.
// Among equal matches the commands run last come first (mitcad#1690).
#include "framework/CommandSearchRank.hpp"

#include <cstdio>
#include <vector>

#include <QString>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "command_search.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::search::Candidate;

const QString openSketch = QStringLiteral("Open a sketch first: Create Sketch (Alt+S)");
const QString finishSketch = QStringLiteral("Finish the sketch first: Finish Sketch (Ctrl+Return)");

enum class Where { Model, Sketch };

// Commands as the search sees them, shown in Finnish (`finnish`) or in
// English, outside or inside a sketch: the sketch tools wait for a sketch
// outside one, the model's commands for its end inside one.
std::vector<Candidate> commands(bool finnish, Where where) {
  struct Row {
    const char* english;
    const char* finnish;
    bool sketchTool;
    bool needsBodies; // Center of Mass: nothing to work on in an empty design
  };
  // The Finnish names as the catalogue has them (app/translations), their
  // a and o with umlauts as UTF-8 escapes: the source text stays English.
  const Row rows[] = {
      {"Center Rectangle", "Keskipistesuorakulmio", true, false},
      {"Center Rounded Rectangle", "Py\xc3\xb6ristetty keskipistesuorakulmio", true, false},
      {"Center Diameter Circle", "Keskipiste\xe2\x80\x93halkaisija-ympyr\xc3\xa4", true, false},
      {"Center Point Arc", "Keskipistekaari", true, false},
      {"Midpoint Line", "Keskipisteviiva", true, false},
      {"Center to Center Slot", "Keskipisteiden v\xc3\xa4linen ura", true, false},
      {"2-Point Rectangle", "Kahden pisteen suorakulmio", true, false},
      {"Point at Center of Circle/Sphere/Torus", "Piste ympyr\xc3\xa4n/pallon/toruksen keskipisteess\xc3\xa4", false,
       false},
      {"Center of Mass", "Massakeskipiste", false, true},
      {"Box", "Laatikko", false, false},
      {"Preferences", "Asetukset", false, false},
  };
  std::vector<Candidate> list;
  for (const Row& row : rows) {
    Candidate candidate;
    candidate.english = QString::fromUtf8(row.english);
    candidate.id = candidate.english; // the recent list's key
    candidate.shown = finnish ? QString::fromUtf8(row.finnish) : candidate.english;
    const bool modeAllows = row.sketchTool == (where == Where::Sketch);
    candidate.enabled = modeAllows && !row.needsBodies;
    if (!modeAllows) {
      candidate.modeHint = where == Where::Model ? openSketch : finishSketch;
    }
    list.push_back(candidate);
  }
  return list;
}

QString shownAt(const std::vector<Candidate>& list, const mitcad::search::Listing& listing, int row) {
  if (row < 0 || row >= static_cast<int>(listing.rows.size())) {
    return QString();
  }
  return list[static_cast<size_t>(listing.rows[static_cast<size_t>(row)])].shown;
}

QString selected(const std::vector<Candidate>& list, const mitcad::search::Listing& listing) {
  return listing.current < 0 ? QString() : shownAt(list, listing, listing.current);
}

} // namespace

int commandSearchTests() {
  using mitcad::search::list;
  using mitcad::search::normalizedText;
  {
    // Outside a sketch, "keskipiste": the sketch tools match best (the start
    // of their names) but wait for a sketch; Point at Center (a word of its
    // name) can run but matches worse. The hint is shown above the sketch
    // tools and nothing is selected.
    const std::vector<Candidate> fi = commands(true, Where::Model);
    const auto listing = list(fi, normalizedText(QStringLiteral("keskipiste")));
    CHECK(listing.hint == openSketch);
    CHECK(listing.current == -1);
    CHECK(!fi[static_cast<size_t>(listing.rows.front())].enabled);
    CHECK(!fi[static_cast<size_t>(listing.rows.front())].modeHint.isEmpty());
    // The sketch tools first, then Point at Center, the commands that cannot
    // run for another reason last.
    int point = -1;
    for (int row = 0; row < static_cast<int>(listing.rows.size()); ++row) {
      if (shownAt(fi, listing, row).startsWith(QStringLiteral("Piste "))) {
        point = row;
      }
    }
    CHECK(point > 0);
    for (int row = 0; row < point; ++row) {
      CHECK(!fi[static_cast<size_t>(listing.rows[static_cast<size_t>(row)])].enabled);
    }
    CHECK(shownAt(fi, listing, static_cast<int>(listing.rows.size()) - 1) == QStringLiteral("Massakeskipiste"));
  }
  {
    // The same in English with "center".
    const std::vector<Candidate> en = commands(false, Where::Model);
    const auto listing = list(en, normalizedText(QStringLiteral("Center")));
    CHECK(listing.hint == openSketch);
    CHECK(listing.current == -1);
    CHECK(shownAt(en, listing, 0) == QStringLiteral("Center Diameter Circle"));
  }
  {
    // Inside a sketch the sketch tools can run: no hint, the best selected.
    const std::vector<Candidate> fi = commands(true, Where::Sketch);
    const auto listing = list(fi, normalizedText(QStringLiteral("keskipiste")));
    CHECK(listing.hint.isEmpty());
    CHECK(listing.current == 0);
    CHECK(fi[static_cast<size_t>(listing.rows.front())].enabled);
    // A few more letters single Center Rectangle out.
    const auto more = list(fi, normalizedText(QStringLiteral("keskipistes")));
    CHECK(more.hint.isEmpty());
    CHECK(selected(fi, more) == QStringLiteral("Keskipistesuorakulmio"));
    const std::vector<Candidate> en = commands(false, Where::Sketch);
    CHECK(selected(en, list(en, normalizedText(QStringLiteral("center r")))) == QStringLiteral("Center Rectangle"));
    CHECK(list(en, normalizedText(QStringLiteral("center"))).hint.isEmpty());
  }
  {
    // Inside a sketch, Box waits for the sketch's end: its hint, as it
    // matches better than any sketch tool.
    const std::vector<Candidate> en = commands(false, Where::Sketch);
    const auto listing = list(en, normalizedText(QStringLiteral("box")));
    CHECK(listing.hint == finishSketch);
    CHECK(listing.current == -1);
    CHECK(shownAt(en, listing, 0) == QStringLiteral("Box"));
  }
  {
    // A match of the same rank that can run keeps it first and selected:
    // "rectangle" outside a sketch has none, so the hint; Preferences finds
    // itself with no hint.
    const std::vector<Candidate> en = commands(false, Where::Model);
    const auto prefs = list(en, normalizedText(QStringLiteral("preferences")));
    CHECK(prefs.hint.isEmpty());
    CHECK(selected(en, prefs) == QStringLiteral("Preferences"));
    std::vector<Candidate> tie = en;
    Candidate other;
    other.shown = QStringLiteral("Center Mark");
    other.english = other.shown;
    tie.push_back(other); // can run, the start of its name as the sketch tools
    const auto listing = list(tie, normalizedText(QStringLiteral("center")));
    CHECK(listing.hint.isEmpty());
    CHECK(selected(tie, listing) == QStringLiteral("Center Mark"));
  }
  {
    // Disabled for another reason (Center of Mass in an empty design): no
    // hint, nothing selected, as before.
    const std::vector<Candidate> en = commands(false, Where::Model);
    const auto listing = list(en, normalizedText(QStringLiteral("center of mass")));
    CHECK(listing.hint.isEmpty());
    CHECK(listing.current == -1);
    CHECK(shownAt(en, listing, 0) == QStringLiteral("Center of Mass"));
  }
  {
    // The English name in Finnish, "..." and an ellipsis at the end.
    const std::vector<Candidate> fi = commands(true, Where::Model);
    CHECK(selected(fi, list(fi, normalizedText(QStringLiteral("preferences")))) == QStringLiteral("Asetukset"));
    CHECK(normalizedText(QStringLiteral("  Library Sources... ")) == QStringLiteral("library sources"));
    CHECK(normalizedText(QStringLiteral("Export") + QChar(0x2026)) == QStringLiteral("export"));
    CHECK(list(fi, normalizedText(QStringLiteral("zzzz"))).rows.empty());
  }
  {
    // Each word on its own, in any order (mitcad#1682): a compound typed
    // with a space finds the Finnish name, the English words in either
    // order the English one, ranked after whole-text matches.
    const std::vector<Candidate> fi = commands(true, Where::Sketch);
    const auto both = list(fi, normalizedText(QStringLiteral("keskipiste suorakulmio")));
    CHECK(selected(fi, both) == QStringLiteral("Keskipistesuorakulmio"));
    CHECK(both.hint.isEmpty());
    const auto reversed = list(fi, normalizedText(QStringLiteral("  suorakulmio   keskipiste ")));
    CHECK(selected(fi, reversed) == QStringLiteral("Keskipistesuorakulmio"));
    // Only the rectangles from the center have both words.
    CHECK(both.rows.size() == 2);
    CHECK(mitcad::search::score(fi.front(), QStringLiteral("keskipiste suorakulmio")) == 6);
    CHECK(mitcad::search::score(fi.front(), QStringLiteral("keskipiste")) == 1);
    // A word that is in no name finds nothing.
    CHECK(list(fi, normalizedText(QStringLiteral("keskipiste kolmio"))).rows.empty());
    // The English name in Finnish too, and a word of each.
    CHECK(selected(fi, list(fi, normalizedText(QStringLiteral("rectangle center")))) ==
          QStringLiteral("Keskipistesuorakulmio"));
    const std::vector<Candidate> en = commands(false, Where::Sketch);
    CHECK(selected(en, list(en, normalizedText(QStringLiteral("center rect")))) == QStringLiteral("Center Rectangle"));
    CHECK(selected(en, list(en, normalizedText(QStringLiteral("rectangle center")))) ==
          QStringLiteral("Center Rectangle"));
    CHECK(mitcad::search::score(en.front(), QStringLiteral("center rect")) == 1);
    CHECK(mitcad::search::score(en.front(), QStringLiteral("rectangle center")) == 6);
    // A keyword's word counts.
    Candidate keyworded;
    keyworded.shown = QStringLiteral("Bill of Materials");
    keyworded.english = keyworded.shown;
    keyworded.keywords = {QStringLiteral("parts list")};
    CHECK(mitcad::search::score(keyworded, QStringLiteral("list bill")) == 6);
    // Single words keep their order: the rectangle's rank and the order of
    // the sketch tools that start with "center" are as before.
    const auto single = list(en, normalizedText(QStringLiteral("center")));
    CHECK(shownAt(en, single, 0) == QStringLiteral("Center Diameter Circle"));
    CHECK(shownAt(en, single, 1) == QStringLiteral("Center Point Arc"));
    CHECK(shownAt(en, single, 2) == QStringLiteral("Center Rectangle"));
    // Outside a sketch the words find the sketch tool and its hint.
    const std::vector<Candidate> model = commands(true, Where::Model);
    const auto waiting = list(model, normalizedText(QStringLiteral("keskipiste suorakulmio")));
    CHECK(waiting.hint == openSketch);
    CHECK(waiting.current == -1);
    CHECK(shownAt(model, waiting, 0) == QStringLiteral("Keskipistesuorakulmio"));
  }
  {
    // The commands run last come first among equal matches (mitcad#1690).
    using mitcad::search::withRecentCommand;
    const QString rectangle = QStringLiteral("Center Rectangle");
    const QString arc = QStringLiteral("Center Point Arc");
    // With no history the name decides, as before: "keskipiste" in a
    // sketch selects Center to Center Slot (its Finnish name comes first),
    // "center" Center Diameter Circle.
    const std::vector<Candidate> fi = commands(true, Where::Sketch);
    const std::vector<Candidate> en = commands(false, Where::Sketch);
    CHECK(selected(fi, list(fi, normalizedText(QStringLiteral("keskipiste")))).startsWith(
        QStringLiteral("Keskipisteiden ")));
    CHECK(selected(en, list(en, normalizedText(QStringLiteral("center")))) == QStringLiteral("Center Diameter Circle"));
    CHECK(list(en, normalizedText(QStringLiteral("center"))).rows ==
          list(en, normalizedText(QStringLiteral("center")), QStringList()).rows);
    // After Center Rectangle was run, both select it.
    const QStringList ran = withRecentCommand(QStringList(), rectangle);
    CHECK(ran == QStringList{rectangle});
    CHECK(selected(fi, list(fi, normalizedText(QStringLiteral("keskipiste")), ran)) ==
          QStringLiteral("Keskipistesuorakulmio"));
    CHECK(selected(en, list(en, normalizedText(QStringLiteral("center")), ran)) == rectangle);
    // Only among equal matches: a better match stays ahead ("center p"
    // finds Center Point Arc although the rectangle ran last).
    CHECK(selected(en, list(en, normalizedText(QStringLiteral("center p")), ran)) == arc);
    // The one run last first, then the one before it, then the others by
    // name.
    const QStringList both = withRecentCommand(ran, arc);
    CHECK(both == (QStringList{arc, rectangle}));
    const auto listing = list(en, normalizedText(QStringLiteral("center")), both);
    CHECK(shownAt(en, listing, 0) == arc);
    CHECK(shownAt(en, listing, 1) == rectangle);
    CHECK(shownAt(en, listing, 2) == QStringLiteral("Center Diameter Circle"));
    // The tiers stay: outside a sketch a recent sketch tool still waits for
    // one, behind its hint, and a command that can run stays ahead of the
    // ones that cannot.
    const std::vector<Candidate> model = commands(false, Where::Model);
    const auto waiting = list(model, normalizedText(QStringLiteral("center")), ran);
    CHECK(waiting.hint == openSketch);
    CHECK(waiting.current == -1);
    CHECK(shownAt(model, waiting, 0) == rectangle);
    std::vector<Candidate> tie = model;
    Candidate other;
    other.id = QStringLiteral("Center Mark");
    other.shown = other.id;
    other.english = other.id;
    tie.push_back(other);
    CHECK(selected(tie, list(tie, normalizedText(QStringLiteral("center")), ran)) == QStringLiteral("Center Mark"));
    // The list: run again moves to the front, no duplicates, at most the
    // limit, an empty id changes nothing.
    CHECK(withRecentCommand(both, rectangle) == (QStringList{rectangle, arc}));
    CHECK(withRecentCommand(both, QString()) == both);
    QStringList many;
    for (int i = 0; i < mitcad::search::kRecentCommands + 5; ++i) {
      many = withRecentCommand(many, QStringLiteral("command.%1").arg(i));
    }
    CHECK(many.size() == mitcad::search::kRecentCommands);
    CHECK(many.front() == QStringLiteral("command.%1").arg(mitcad::search::kRecentCommands + 4));
    CHECK(withRecentCommand(both, QStringLiteral("c"), 2) == (QStringList{QStringLiteral("c"), arc}));
  }
  return failures;
}
