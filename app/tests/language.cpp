// SPDX-License-Identifier: MIT
// The interface language (mitcad#1460): which language a start takes, and
// the catalogues the build made (cmake/Translations.cmake): Finnish names
// itself and translates the commands, the pseudo-language marks every text,
// and a text is found also where its context at run time is not lupdate's.
#include "framework/Language.hpp"

#include <cstdio>

#include <QDir>
#include <QFileInfo>
#include <QString>
#include <QStringList>
#include <QTranslator>

namespace {
int failures = 0;
void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "language.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}
#define CHECK(expr) check((expr), #expr, __LINE__)
} // namespace

int languageTests() {
  using mitcad::chooseLanguage;
  const QStringList available{QStringLiteral("fi")};
  const QStringList finnishSystem{QStringLiteral("fi-FI"), QStringLiteral("en-US")};
  const QStringList englishSystem{QStringLiteral("en-US")};

  // --language, then MITCAD_LANGUAGE, then the preference, then the system.
  auto choice = chooseLanguage(QStringLiteral("fi"), QStringLiteral("en"), QString(), englishSystem, available);
  CHECK(choice.code == QStringLiteral("fi") && choice.reason == QStringLiteral("--language"));
  choice = chooseLanguage(QStringLiteral("en"), QString(), QStringLiteral("fi"), finnishSystem, available);
  CHECK(choice.code == QStringLiteral("en") && choice.reason == QStringLiteral("--language"));
  choice = chooseLanguage(QString(), QStringLiteral("en"), QStringLiteral("fi"), finnishSystem, available);
  CHECK(choice.code == QStringLiteral("en") && choice.reason == QStringLiteral("MITCAD_LANGUAGE"));
  choice = chooseLanguage(QString(), QString(), QStringLiteral("fi"), englishSystem, available);
  CHECK(choice.code == QStringLiteral("fi") && choice.reason == QStringLiteral("preference"));
  choice = chooseLanguage(QString(), QString(), QStringLiteral("en"), finnishSystem, available);
  CHECK(choice.code == QStringLiteral("en") && choice.reason == QStringLiteral("preference"));
  // Automatic: Finnish only when it is the system's first language.
  choice = chooseLanguage(QString(), QString(), QString(), finnishSystem, available);
  CHECK(choice.code == QStringLiteral("fi") && choice.reason.startsWith(QStringLiteral("automatic: ")));
  choice = chooseLanguage(QString(), QString(), QStringLiteral("auto"), englishSystem, available);
  CHECK(choice.code == QStringLiteral("en") && choice.reason.contains(QStringLiteral("en-US")));
  choice = chooseLanguage(QString(), QString(), QString(),
                          {QStringLiteral("sv-SE"), QStringLiteral("fi-FI")}, available);
  CHECK(choice.code == QStringLiteral("en"));
  choice = chooseLanguage(QString(), QString(), QString(), {}, available);
  CHECK(choice.code == QStringLiteral("en"));
  // A region is matched to its language; a language without a catalogue
  // is English, and says so.
  choice = chooseLanguage(QStringLiteral("fi_FI"), QString(), QString(), englishSystem, available);
  CHECK(choice.code == QStringLiteral("fi"));
  choice = chooseLanguage(QStringLiteral("de"), QString(), QString(), finnishSystem, available);
  CHECK(choice.code == QStringLiteral("en") && choice.reason.contains(QStringLiteral("no catalogue for de")));
  choice = chooseLanguage(QString(), QString(), QStringLiteral("de"), finnishSystem, available);
  CHECK(choice.code == QStringLiteral("fi") && choice.reason.contains(QStringLiteral("no catalogue for the preference de")));

#ifdef MITCAD_TEST_TRANSLATIONS_DIR
  // The catalogues the build made.
  const QDir folder(QStringLiteral(MITCAD_TEST_TRANSLATIONS_DIR));
  const QString finnish = folder.filePath(QStringLiteral("mitcad_fi.qm"));
  CHECK(QFileInfo::exists(finnish));
  CHECK(mitcad::catalogueText(finnish, "LanguageName", "English") == QStringLiteral("Suomi"));
  CHECK(mitcad::catalogueText(finnish, "QObject", "Extrude") == QStringLiteral("Pursotus"));
  // The context-free part: a context lupdate did not see.
  CHECK(mitcad::catalogueText(finnish, "NoSuchContext", "Extrude") == QStringLiteral("Pursotus"));
  CHECK(mitcad::catalogueText(finnish, "QObject", "No such text in MitCad").isEmpty());
  // The files that translate outside a Q_OBJECT class have their texts in
  // the context they look them up in, not only in the context-free part
  // (mitcad#1472): a plain translator finds them.
  {
    QTranslator plain;
    CHECK(plain.load(finnish));
    const char* const own[][2] = {
        {"DrawingTools", "Hole Table"},     {"ProjectDialogs", "Project Folder"},
        {"BindingList", "Appearance"},      {"ProjectSettings", "never"},
        {"UpdateController", "It is a pre-release."},
        {"ProjectBrowser", "Editing"},      {"IamImport", "%1 parametric"},
        {"CloudAddress", "Shared folder"},  {"ProjectPicker", "Local project"},
        {"LiveDialogs", "Live Updates"},    {"ProjectsPanel", "Not recorded"},
        {"ParameterSuggest", "An angle"},
        // The standard views' commands as whole texts (mitcad#1687) and the
        // orientation cube's faces (mitcad#1674).
        {"View", "Top View"},               {"View", "Looks at the design from the top"},
        {"View", "Isometric View"},         {"ViewCube", "TOP"},
        {"ViewCube", "FRONT"}};
    for (const auto& message : own) {
      if (plain.translate(message[0], message[1]).isEmpty()) {
        std::fprintf(stderr, "language.cpp: no translation of '%s' in the context %s\n", message[1], message[0]);
        ++failures;
      }
    }
    // Not a view's name put into "%1 View" (mitcad#1687: a hyphenated
    // compound in Finnish).
    CHECK(plain.translate("mitcad::ViewController", "%1 View").isEmpty());
    CHECK(!plain.translate("View", "Top View").contains(QLatin1Char('-')));
  }
  // English's plural forms (mitcad#1756): a count says "1 degree of
  // freedom" and "17 degrees of freedom", never "degree(s)"; the other
  // texts are not in it (English already).
  {
    const QString english = folder.filePath(QStringLiteral("mitcad_en.qm"));
    CHECK(QFileInfo::exists(english));
    QTranslator plurals;
    CHECK(plurals.load(english));
    CHECK(plurals.translate("mitcad::sketch::SketchOverlay", "%1: %n degree(s) of freedom", nullptr, 1) ==
          QStringLiteral("%1: %n degree of freedom"));
    CHECK(plurals.translate("mitcad::sketch::SketchOverlay", "%1: %n degree(s) of freedom", nullptr, 17) ==
          QStringLiteral("%1: %n degrees of freedom"));
    CHECK(plurals.translate("mitcad::BrowserPanel", "%n annotation(s)", nullptr, 1) ==
          QStringLiteral("%n annotation"));
    CHECK(plurals.translate("mitcad::BrowserPanel", "%n annotation(s)", nullptr, 2) ==
          QStringLiteral("%n annotations"));
    CHECK(plurals.translate("mitcad::AppearanceDialog", "Used by %n body(ies)", nullptr, 3) ==
          QStringLiteral("Used by %n bodies"));
    CHECK(plurals.translate("QObject", "Extrude").isEmpty());
    // Not a language to choose: English is listed once, as itself.
    int englishes = 0;
    for (const mitcad::InterfaceLanguage& language : mitcad::availableLanguages()) {
      englishes += language.code == QStringLiteral("en") ? 1 : 0;
    }
    CHECK(englishes == 1);
  }
  const QString pseudo = folder.filePath(QStringLiteral("mitcad_xx.qm"));
  CHECK(QFileInfo::exists(pseudo));
  const QString marked = mitcad::catalogueText(pseudo, "QObject", "Extrude");
  CHECK(marked.startsWith(QLatin1Char('[')) && marked.endsWith(QLatin1Char(']')) && marked.size() > 9);
  CHECK(mitcad::catalogueText(pseudo, "LanguageName", "English").startsWith(QLatin1Char('[')));
#endif
  return failures;
}
