// SPDX-License-Identifier: MIT
#pragma once

#include <vector>

#include <QString>
#include <QStringList>

namespace mitcad::search {

// A command as the command search (S) weighs it against the text typed.
struct Candidate {
  QString id;           // its command id, as the recent list keeps it
  QString shown;        // its name as the interface shows it
  QString english;      // its English name; the same as `shown` in English
  QStringList keywords; // other words it is found by
  bool enabled = true;  // it can run now
  // When it cannot run only because of the mode it waits for (a sketch
  // tool outside a sketch, mitcad#1681), the row that says how to get
  // there: "Open a sketch first: Create Sketch (Alt+S)". Empty when it can
  // run, or when something else keeps it from running.
  QString modeHint;
};

// The text typed as the search compares it: trimmed, in lower case, without
// a trailing "..." or ellipsis ("Library Sources..." finds Library
// Sources…).
QString normalizedText(const QString& typed);

// How well `text` (normalizedText) matches the command: lower is better,
// -1 is no match. 0 the whole name, 1 the start of the name, 2 the start of
// a word of it, 3 inside it, 4 the start of a keyword, 5 its letters in
// order ("fll" for Fillet); the better of the shown and the English name.
// When none of these, 6 when each word of a text of several words is in
// the shown or the English name or a keyword, in any order (mitcad#1682:
// "keskipiste suorakulmio" finds Keskipistesuorakulmio, "rectangle center"
// Center Rectangle). An empty text is 6 for every command.
int score(const Candidate& candidate, const QString& text);

// The search's list.
struct Listing {
  // When the best matches cannot run until the mode changes (a sketch tool
  // outside a sketch): their mode hint, shown as a row above them; empty
  // for none.
  QString hint;
  // Indexes into the candidates, in the list's order: with a hint, the
  // matches that wait for the mode and match better than any command that
  // can run, then those that can run, then the other matches that cannot;
  // without one, those that can run, then the others. Within each, the
  // better match first; among equal matches the commands run last first
  // (the recent list, mitcad#1690), then by the shown name.
  std::vector<int> rows;
  // The position in `rows` of the row selected at first (Enter runs it):
  // the first if it can run and no hint is shown; -1 for none, so that
  // Enter does not start a weaker match (mitcad#1681).
  int current = -1;
};

// `recent`: the ids of the commands run last, the newest first
// (recentCommands()); empty for no history, which keeps the name order.
Listing list(const std::vector<Candidate>& candidates, const QString& text, const QStringList& recent = {});

// The commands run last (mitcad#1690), which come first among equal
// matches: kept in the local settings ("commands/recent", the commands'
// ids, the newest first), per user, as the Appearance picker keeps its
// Recent (mitcad#1685).
constexpr int kRecentCommands = 10;
QStringList recentCommands();
// Puts a command just run first in the settings' list.
void rememberRecentCommand(const QString& id);
// The recent list with `id` first (moved there when it was in it), at most
// `limit` long; an empty id changes nothing.
QStringList withRecentCommand(const QStringList& recent, const QString& id, int limit = kRecentCommands);

} // namespace mitcad::search
