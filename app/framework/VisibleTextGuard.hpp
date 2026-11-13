// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QRegularExpression>
#include <QString>
#include <QStringList>
#include <QVector>

class QWidget;

namespace mitcad {

// The UI tests' guard against internal identifiers in text a person sees
// (mitcad#1741; docs/ui-conventions.md, "What a text says"). With
// MITCAD_TEST_TEXT_GUARD set (tools/ui-test-lib.sh: the allow-list's path),
// each answer of the sync key (TestSync, after every step of a Linux UI
// test) first reads every text the application's windows show and logs each
// identifier found as
//
//   Identifier in visible text: <widget's object path>: <identifier>: <text>
//
// which fails the test (ui_step, ui_finish). Read: the labels (rich text as
// a person reads it), the buttons, group box titles and tabs, the read-only
// fields and text views, the non-editable combo boxes' current texts, the
// rows of the item views in sight (the Browser, lists, tables) with their
// tooltips, the shown menus' entries, every shown widget's tooltip and each
// window's title, and what a widget paints itself (addSource: the
// timeline's cells). A field a person types in is theirs and not read, nor
// is a file dialog (its files' names are the user's own).
namespace visible_text {

// The internal identifiers in a plain text: topological names (`E{`, `V{`,
// `F<n>:` and side, top, bottom, end or start), region keys (`r{`) and bare
// ids as whole words (`F57`, `F2.b0`'s F2, `O3`, `k12`, `p15`, `c4`). A
// function key's name (F1 to F35 after a modifier, `Ctrl+F6`, in
// parentheses, `(F6)`, or a text that is only keys, `F2`) is no id.
QStringList identifiersIn(const QString& text);

// Rich text as a person reads it: the tags out, the entities decoded. Plain
// text as it is.
QString plainText(const QString& text);

// An entry of the allow-list (tools/ui-visible-text-allow.txt): the
// identifiers `token` matches whole (a regular expression, `p[1-8]`),
// allowed in the texts that match `text`, with its reason. Only for a
// token that is the user's own data (a name typed in a test).
struct AllowEntry {
  QRegularExpression token;
  QRegularExpression text;
  QString reason;
};

// The allow-list's lines: `<token regex> | <text regex> | <reason>`; blank
// lines and lines starting with # left out. A line without three non-empty
// fields, or with an invalid regex, is an error (each in `errors`).
QVector<AllowEntry> parseAllowList(const QString& contents, QStringList* errors);

// Whether `token` found in `text` is allowed by an entry.
bool allowed(const QVector<AllowEntry>& entries, const QString& token, const QString& text);

// A widget's object path for the log: its ancestors' object names (their
// class names where they have none) from the window down.
QString objectPath(const QWidget* widget);

} // namespace visible_text

class VisibleTextGuard {
public:
  // Whether MITCAD_TEST_TEXT_GUARD is set.
  static bool enabled();
  // Reads every visible text now and logs the identifiers found (once per
  // widget, identifier and text); the number found this time.
  static int check();
  // What a widget paints itself, for the guard (the timeline's cells' texts
  // and tooltips); read while the widget is shown. Nothing is kept without
  // the guard.
  static void addSource(QWidget* widget, std::function<QStringList()> texts);
};

} // namespace mitcad
