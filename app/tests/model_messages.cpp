// SPDX-License-Identifier: MIT
// The model's messages in the interface language (mitcad#1284): a message
// matched to its template and its values, the values put into the
// translated template, a value that is a message translated in turn, the
// English fallback; and the catalogues the build made: every template of
// core/ (ModelMessageTemplates.cpp) translated in Finnish and marked in the
// pseudo-language, so that no model message is left English.
#include "framework/ModelMessages.hpp"

#include <cstdio>
#include <cstring>

#include <QDir>
#include <QFileInfo>
#include <QHash>
#include <QString>
#include <QTranslator>

namespace {
int failures = 0;
void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "model_messages.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}
#define CHECK(expr) check((expr), #expr, __LINE__)

const char* const kTemplates[] = {
    "feature %1 does not exist",
    "%1: parameter %2 does not exist",
    "%1: %2",
    "the geometry kernel does not support %1",
    "patterns of faces",
    "the radius must be positive, got %1",
    "%1 is used by %2; delete them too, or change them first",
    "no translation for this one",
};

// A catalogue of the test's own, a language in capitals: every template
// but the last translated.
QString planted(const char* source) {
  static const QHash<QString, QString> capitals{
      {QStringLiteral("feature %1 does not exist"), QStringLiteral("NO FEATURE %1")},
      {QStringLiteral("%1: parameter %2 does not exist"), QStringLiteral("%1: NO PARAMETER %2")},
      {QStringLiteral("%1: %2"), QStringLiteral("%1: %2")},
      {QStringLiteral("the geometry kernel does not support %1"), QStringLiteral("THE KERNEL CANNOT DO %1")},
      {QStringLiteral("patterns of faces"), QStringLiteral("FACE PATTERNS")},
      {QStringLiteral("the radius must be positive, got %1"), QStringLiteral("RADIUS NOT POSITIVE: %1")},
      {QStringLiteral("%1 is used by %2; delete them too, or change them first"), QStringLiteral("%2 USES %1")},
  };
  const QString english = QString::fromUtf8(source);
  return capitals.value(english, english);
}
} // namespace

int modelMessageTests() {
  using mitcad::fillTemplate;
  using mitcad::ModelMessageCatalogue;
  const ModelMessageCatalogue catalogue(kTemplates, static_cast<int>(sizeof kTemplates / sizeof kTemplates[0]));
  CHECK(catalogue.size() == 8);

  // The id and the values.
  auto found = catalogue.match(QStringLiteral("feature F12 does not exist"));
  CHECK(found.found() && std::strcmp(catalogue.source(found.index), "feature %1 does not exist") == 0);
  CHECK(found.values == QStringList{QStringLiteral("F12")});
  // The template with the most fixed text wins over "%1: %2".
  found = catalogue.match(QStringLiteral("Fillet1.radius: parameter r2 does not exist"));
  CHECK(found.found() && std::strcmp(catalogue.source(found.index), "%1: parameter %2 does not exist") == 0);
  CHECK(found.values == (QStringList{QStringLiteral("Fillet1.radius"), QStringLiteral("r2")}));
  CHECK(!catalogue.match(QStringLiteral("something the model never says")).found());
  CHECK(!catalogue.match(QString()).found());
  // The whole message, not a part of it.
  CHECK(!catalogue.match(QStringLiteral("the feature F1 does not exist yet")).found());

  // Translated, the values as they are (and in their own order).
  bool untranslated = false;
  CHECK(catalogue.translate(QStringLiteral("feature F12 does not exist"), planted, &untranslated) ==
        QStringLiteral("NO FEATURE F12"));
  CHECK(!untranslated);
  CHECK(catalogue.translate(QStringLiteral("the radius must be positive, got -2.5 mm"), planted) ==
        QStringLiteral("RADIUS NOT POSITIVE: -2.5 mm"));
  // A value that is a message of its own is translated too.
  CHECK(catalogue.translate(QStringLiteral("the geometry kernel does not support patterns of faces"), planted) ==
        QStringLiteral("THE KERNEL CANNOT DO FACE PATTERNS"));
  CHECK(catalogue.translate(QStringLiteral("Extrude2: the radius must be positive, got 0"), planted) ==
        QStringLiteral("Extrude2: RADIUS NOT POSITIVE: 0"));
  // Values with spaces that are no message stay.
  CHECK(catalogue.translate(
            QStringLiteral("Top Plate is used by Hole 1, Hole 2; delete them too, or change them first"), planted) ==
        QStringLiteral("Hole 1, Hole 2 USES Top Plate"));

  // The English fallback: a template without its translation keeps the
  // whole message English, and a text that is no template stays as it is.
  untranslated = false;
  CHECK(catalogue.translate(QStringLiteral("no translation for this one"), planted, &untranslated) ==
        QStringLiteral("no translation for this one"));
  CHECK(untranslated);
  untranslated = false;
  CHECK(catalogue.translate(QStringLiteral("a text of no template"), planted, &untranslated) ==
        QStringLiteral("a text of no template"));
  CHECK(untranslated);
  // Lines, and a lead-in of the application's before ": ".
  CHECK(catalogue.translate(QStringLiteral("feature F1 does not exist\nfeature F2 does not exist"), planted) ==
        QStringLiteral("NO FEATURE F1\nNO FEATURE F2"));
  CHECK(catalogue.translate(QStringLiteral("Export failed: feature F3 does not exist"), planted) ==
        QStringLiteral("Export failed: NO FEATURE F3"));

  // The placeholders.
  CHECK(fillTemplate(QStringLiteral("%2 and %1"), {QStringLiteral("a"), QStringLiteral("b")}) ==
        QStringLiteral("b and a"));
  CHECK(fillTemplate(QStringLiteral("%10 %1"), {QStringLiteral("x")}) == QStringLiteral("x0 x"));
  CHECK(fillTemplate(QStringLiteral("100% %3"), {QStringLiteral("x")}) == QStringLiteral("100% %3"));

  // English shows the model's text as it is.
  CHECK(mitcad::modelMessage(QStringLiteral("feature F12 does not exist")) ==
        QStringLiteral("feature F12 does not exist"));

  // The generated list: templates, each once.
  CHECK(mitcad::kModelMessageTemplateCount > 1000);
  {
    QHash<QString, int> seen;
    for (int i = 0; i < mitcad::kModelMessageTemplateCount; ++i) {
      ++seen[QString::fromUtf8(mitcad::kModelMessageTemplates[i])];
    }
    CHECK(seen.size() == mitcad::kModelMessageTemplateCount);
    CHECK(seen.contains(QStringLiteral("feature %1 does not exist")));
  }

#ifdef MITCAD_TEST_TRANSLATIONS_DIR
  // Every template of core/ has its Finnish translation and its
  // pseudo-language text in the catalogues the build made: no model
  // message is left English.
  const QDir folder(QStringLiteral(MITCAD_TEST_TRANSLATIONS_DIR));
  const ModelMessageCatalogue all(mitcad::kModelMessageTemplates, mitcad::kModelMessageTemplateCount);
  const struct {
    const char* file;
    bool marked;
  } languages[] = {{"mitcad_fi.qm", false}, {"mitcad_xx.qm", true}};
  for (const auto& language : languages) {
    QTranslator translator;
    const QString file = folder.filePath(QString::fromLatin1(language.file));
    CHECK(QFileInfo::exists(file) && translator.load(file));
    int missing = 0;
    for (int i = 0; i < mitcad::kModelMessageTemplateCount; ++i) {
      const char* source = mitcad::kModelMessageTemplates[i];
      const QString text = translator.translate("ModelMessage", source);
      const bool ok = !text.isEmpty() && (!language.marked || text.trimmed().startsWith(QLatin1Char('[')));
      if (!ok && ++missing <= 10) {
        std::fprintf(stderr, "model_messages.cpp: %s has no translation of \"%s\"\n", language.file, source);
      }
    }
    if (missing > 0) {
      std::fprintf(stderr, "model_messages.cpp: %s: %d of %d model message templates untranslated\n", language.file,
                   missing, mitcad::kModelMessageTemplateCount);
      ++failures;
    }
    // A model message in the catalogue's language, through the matching:
    // translated, its value kept.
    const QString english = QStringLiteral("feature F12 does not exist");
    const QString shown =
        all.translate(english, [&translator](const char* s) { return translator.translate("ModelMessage", s); });
    CHECK(shown != english && shown.contains(QStringLiteral("F12")));
    CHECK(shown.startsWith(QLatin1Char('[')) == language.marked);
  }
#endif
  return failures;
}
