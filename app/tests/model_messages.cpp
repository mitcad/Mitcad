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
#include <QRegularExpression>
#include <QString>
#include <QStringList>
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

#ifdef MITCAD_TEST_TRANSLATIONS_DIR
// The model's messages of mitcad#1626 as a person reads them: each was a
// template with an English word put in as a value ("the curve is a %1, not a
// line" with "spline"), or pieces that matched only "%1 is not %2", and is
// now whole messages in core/ (its own tests pin the English). Translated
// through the catalogue `translator` loaded, none may keep an English word.
void wholeMessagesHaveNoEnglishWord(const mitcad::ModelMessageCatalogue& all, const QTranslator& translator) {
  const char* const messages[] = {
      // GeomRef::error: "%1 is not a plane" and the like, the words a value
      // translated in turn.
      "the face is not a plane",
      "the edge is not a plane",
      "the vertex is not an axis",
      "the sketch curve is not a point",
      "the body is not a face",
      "the face is not an edge",
      "the face is no planar face or frame",
      // A missing edge or face, the feature's name the value.
      "an edge no longer exists after Extrude2",
      "an edge does not exist because Extrude3 failed",
      "a face no longer exists after DeleteFace1",
      "a face does not exist because Extrude3 failed",
      "a face does not exist because Extrude3 is suppressed",
      "a face does not exist because Extrude3 is rolled back",
      // The rotation's notes, a count the value.
      "the rotation turned a constraint off the sketch's axes: its line held parallel or perpendicular to the first "
      "line instead",
      "the rotation turned 4 constraints off the sketch's axes: their lines held parallel or perpendicular to the "
      "first line instead",
      "the rotation turned a dimension off the sketch's axes: now an aligned distance",
      "the rotation turned 2 dimensions off the sketch's axes: now an aligned distance",
      "a constraint deactivated: turned off the sketch's axes by a rotation",
      "3 constraints deactivated: turned off the sketch's axes by a rotation",
      // Equal spacing's axis.
      "equal spacing: two items are at the same place along the x axis; give their order (keep the listed order) or "
      "measure another way",
      "equal spacing: two items are at the same place along the y axis; give their order (keep the listed order) or "
      "measure another way",
      "equal spacing: two items are at the same place along the direction; give their order (keep the listed order) "
      "or measure another way",
      // The chain's ends, at one place or a distance apart.
      "the curves are not one connected chain: a curve ends at another curve's end but on a point of its own; a "
      "Coincident constraint joins them",
      "the curves are not one connected chain: a curve ends 0.005 mm from another curve's end but on a point of its "
      "own; a Coincident constraint joins them",
      // A curve's kind.
      "the curve is a point, not a line",
      "the curve is a circle, not a line",
      "the curve is an arc, not a line",
      "the curve is an ellipse, not a line",
      "the curve is an elliptical arc, not a line",
      "the curve is a spline, not a line",
      "the curve is a fitted spline, not a line",
      "the curve is a point, not a circle or an arc",
      "the curve is a line, not a circle or an arc",
      "the curve is an ellipse, not a circle or an arc",
      "the curve is an elliptical arc, not a circle or an arc",
      "the curve is a spline, not a circle or an arc",
      "the curve is a fitted spline, not a circle or an arc",
  };
  // The English words of these messages (the values kept as they are,
  // Extrude2 and 0.005, are no words of these).
  static const QStringList kEnglish{
      QStringLiteral("a"),          QStringLiteral("after"),        QStringLiteral("aligned"),
      QStringLiteral("along"),      QStringLiteral("an"),           QStringLiteral("another"),
      QStringLiteral("arc"),        QStringLiteral("at"),           QStringLiteral("axes"),
      QStringLiteral("axis"),       QStringLiteral("back"),         QStringLiteral("because"),
      QStringLiteral("body"),       QStringLiteral("but"),          QStringLiteral("by"),
      QStringLiteral("chain"),      QStringLiteral("circle"),       QStringLiteral("connected"),
      QStringLiteral("constraint"), QStringLiteral("constraints"),  QStringLiteral("curve"),
      QStringLiteral("curves"),     QStringLiteral("deactivated"),  QStringLiteral("dimension"),
      QStringLiteral("dimensions"), QStringLiteral("direction"),    QStringLiteral("distance"),
      QStringLiteral("does"),       QStringLiteral("edge"),         QStringLiteral("ellipse"),
      QStringLiteral("elliptical"), QStringLiteral("end"),          QStringLiteral("ends"),
      QStringLiteral("equal"),      QStringLiteral("exist"),        QStringLiteral("exists"),
      QStringLiteral("face"),       QStringLiteral("failed"),       QStringLiteral("first"),
      QStringLiteral("fitted"),     QStringLiteral("frame"),        QStringLiteral("from"),
      QStringLiteral("held"),       QStringLiteral("instead"),      QStringLiteral("is"),
      QStringLiteral("items"),      QStringLiteral("its"),          QStringLiteral("line"),
      QStringLiteral("lines"),      QStringLiteral("longer"),       QStringLiteral("no"),
      QStringLiteral("not"),        QStringLiteral("now"),          QStringLiteral("of"),
      QStringLiteral("off"),        QStringLiteral("or"),           QStringLiteral("own"),
      QStringLiteral("parallel"),   QStringLiteral("perpendicular"), QStringLiteral("place"),
      QStringLiteral("planar"),     QStringLiteral("plane"),        QStringLiteral("point"),
      QStringLiteral("rolled"),     QStringLiteral("rotation"),     QStringLiteral("same"),
      QStringLiteral("sketch"),     QStringLiteral("spacing"),      QStringLiteral("spline"),
      QStringLiteral("suppressed"), QStringLiteral("the"),          QStringLiteral("their"),
      QStringLiteral("turned"),     QStringLiteral("two"),          QStringLiteral("vertex"),
  };
  const QRegularExpression notLetters(QStringLiteral("[^\\p{L}]+"));
  for (const char* message : messages) {
    const QString shown = all.translate(QString::fromUtf8(message),
                                        [&translator](const char* s) { return translator.translate("ModelMessage", s); });
    const QStringList words = shown.split(notLetters, Qt::SkipEmptyParts);
    for (const QString& word : words) {
      if (kEnglish.contains(word.toLower())) {
        std::fprintf(stderr, "model_messages.cpp: \"%s\" is shown as \"%s\": the English word \"%s\"\n", message,
                     shown.toUtf8().constData(), word.toUtf8().constData());
        ++failures;
        break;
      }
    }
  }
}
#endif
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
    // A message is shown cleaned of names and ids (mitcad#1327, #1612):
    // the cleaned forms are templates too (mitcad#1635).
    CHECK(seen.contains(QStringLiteral("line c%1 does not exist")));
    CHECK(seen.contains(QStringLiteral("a line does not exist")));
    CHECK(seen.contains(QStringLiteral("a body (from %1) belongs to %2; a feature works on the bodies of its own "
                                       "component (activate that component)")));
    // No second article in a noun phrase that has one (mitcad#1638).
    CHECK(seen.contains(QStringLiteral(
        "the driven dimension (%1) measures nothing, so the dimensions that use it have no value")));
    CHECK(!seen.contains(QStringLiteral(
        "the driven a dimension (%1) measures nothing, so the dimensions that use it have no value")));
    CHECK(seen.contains(QStringLiteral("the mirror line is not a line")));
    CHECK(!seen.contains(QStringLiteral("the mirror a line is not a line")));
    CHECK(!seen.contains(QStringLiteral("a new a body has no participant bodies")));
    // A name where no word of a kind is before it is shown as its kind
    // ("plane F2:top: ...", "plane a face: ..."): that form is a template
    // too (mitcad#1638).
    CHECK(seen.contains(QStringLiteral("plane %1: the face is on several bodies; name the body")));
    CHECK(seen.contains(QStringLiteral("plane a face: the face is on several bodies; name the body")));
    // mitcad#1650: a value named by its kind where it was a positional one
    // ("of the pieces of {region}"), a list of names ("into {region_names}") and
    // a name of any kind ("the projection of {topo}", a form for each kind).
    CHECK(seen.contains(QStringLiteral("a loft section is one region; of the pieces of a region the first is used")));
    CHECK(seen.contains(
        QStringLiteral("a region is split into regions after a change in %1; select the profiles again")));
    for (const char* kind : {"a face", "an edge", "a vertex"}) {
      CHECK(seen.contains(QStringLiteral("the projection of %1 follows its source: nothing to project again")
                              .arg(QLatin1String(kind))));
    }
    // mitcad#1651: the entities a projection does not follow with, after
    // the word of their kind, one sentence for each kind and count; shown
    // as the kinds ("with a curve", "with curves and a point").
    for (const char* entities : {"curve %2", "curves %2", "point %2", "points %2", "curve %2 and point %3",
                                 "curves %2 and point %3", "curve %2 and points %3", "curves %2 and points %3"}) {
      const bool one = std::strcmp(entities, "curve %2") == 0 || std::strcmp(entities, "point %2") == 0;
      const QString tail = one ? QStringLiteral(": nothing like it is in it now, so it keeps its last geometry")
                               : QStringLiteral(": nothing like them is in it now, so they keep their last geometry");
      CHECK(seen.contains(QStringLiteral("the projection of %1 does not follow it with ") +
                          QString::fromLatin1(entities) + tail));
    }
    for (const char* kind : {"a face", "an edge", "a vertex"}) {
      CHECK(seen.contains(QStringLiteral("the projection of %1 does not follow it with a curve: nothing like it is in "
                                         "it now, so it keeps its last geometry")
                              .arg(QLatin1String(kind))));
      CHECK(seen.contains(QStringLiteral("the projection of %1 does not follow it with curves and a point: nothing "
                                         "like them is in it now, so they keep their last geometry")
                              .arg(QLatin1String(kind))));
    }
    // Not the ids as one value, shown with English kind words in it.
    CHECK(!seen.contains(QStringLiteral(
        "the projection of a face does not follow it with %1: nothing like it is in it now, so it keeps its last "
        "geometry")));
  }
  {
    // A cleaned message is matched to its cleaned template, not to a
    // shorter one whose values would take its words.
    const ModelMessageCatalogue all(mitcad::kModelMessageTemplates, mitcad::kModelMessageTemplateCount);
    found = all.match(QStringLiteral("a line does not exist"));
    CHECK(found.found() && std::strcmp(all.source(found.index), "a line does not exist") == 0);
    found = all.match(QStringLiteral("a body (from Extrude1) belongs to Bracket; a feature works on the bodies of its "
                                     "own component (activate that component)"));
    CHECK(found.found() && std::strcmp(all.source(found.index),
                                       "a body (from %1) belongs to %2; a feature works on the bodies of its own "
                                       "component (activate that component)") == 0);
    CHECK(found.values == (QStringList{QStringLiteral("Extrude1"), QStringLiteral("Bracket")}));
    // mitcad#1638: a phrase with its own article, a kind put in for a name.
    found = all.match(
        QStringLiteral("the driven dimension (d3) measures nothing, so the dimensions that use it have no value"));
    CHECK(found.found() &&
          std::strcmp(all.source(found.index),
                      "the driven dimension (%1) measures nothing, so the dimensions that use it have no value") == 0);
    CHECK(found.values == QStringList{QStringLiteral("d3")});
    found = all.match(QStringLiteral("plane a face: the face is on several bodies; name the body"));
    CHECK(found.found() &&
          std::strcmp(all.source(found.index), "plane a face: the face is on several bodies; name the body") == 0);
    CHECK(found.values.isEmpty());
    // mitcad#1650: a kind put in for a value that was positional, matched to
    // its form with the kind, not to the template with English words as
    // its value.
    found = all.match(QStringLiteral("the projection of an edge follows its source: nothing to project again"));
    CHECK(found.found() && std::strcmp(all.source(found.index),
                                       "the projection of an edge follows its source: nothing to project again") == 0);
    CHECK(found.values.isEmpty());
    found = all.match(
        QStringLiteral("a region is split into regions after a change in Sketch1; select the profiles again"));
    CHECK(found.found() &&
          std::strcmp(all.source(found.index),
                      "a region is split into regions after a change in %1; select the profiles again") == 0);
    CHECK(found.values == QStringList{QStringLiteral("Sketch1")});
    // mitcad#1651: the follow warning as shown, matched to its whole form.
    const char* const unfollowed = "the projection of an edge does not follow it with curves and a point: nothing like "
                                   "them is in it now, so they keep their last geometry";
    found = all.match(QString::fromLatin1(unfollowed));
    CHECK(found.found() && std::strcmp(all.source(found.index), unfollowed) == 0);
    CHECK(found.values.isEmpty());
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
    // Messages as they are shown, cleaned of names and ids (mitcad#1635): a
    // line that does not exist, a body of another component. Whole, in the
    // catalogue's language.
    const auto translated = [&](const QString& message) {
      bool fallback = false;
      const QString text = all.translate(
          message, [&translator](const char* s) { return translator.translate("ModelMessage", s); }, &fallback);
      CHECK(!fallback && text != message);
      CHECK(text.startsWith(QLatin1Char('[')) == language.marked);
      return text;
    };
    const QString line = translated(QStringLiteral("a line does not exist"));
    const QString body = translated(QStringLiteral("a body (from Extrude1) belongs to Bracket; a feature works on "
                                                   "the bodies of its own component (activate that component)"));
    CHECK(body.contains(QStringLiteral("Extrude1")) && body.contains(QStringLiteral("Bracket")));
    // Each as its cleaned template's translation, the values put back.
    CHECK(line == translator.translate("ModelMessage", "a line does not exist"));
    CHECK(body == mitcad::fillTemplate(translator.translate("ModelMessage",
                                                            "a body (from %1) belongs to %2; a feature works on the "
                                                            "bodies of its own component (activate that component)"),
                                       {QStringLiteral("Extrude1"), QStringLiteral("Bracket")}));
    // mitcad#1638: a noun phrase with its article ("the driven dimension",
    // not "the driven a dimension") and a kind put in for a name ("plane a
    // face"), each translated whole: in Finnish no English word is left.
    const QString driven = translated(
        QStringLiteral("the driven dimension (d3) measures nothing, so the dimensions that use it have no value"));
    const QString plane = translated(QStringLiteral("plane a face: the face is on several bodies; name the body"));
    CHECK(driven.contains(QStringLiteral("(d3)")));
    if (!language.marked) {
      wholeMessagesHaveNoEnglishWord(all, translator);
      const QStringList englishWords{QStringLiteral("a"),         QStringLiteral("an"),       QStringLiteral("the"),
                                     QStringLiteral("driven"),    QStringLiteral("dimension"), QStringLiteral("face"),
                                     QStringLiteral("plane"),     QStringLiteral("measures"), QStringLiteral("body"),
                                     QStringLiteral("bodies"),    QStringLiteral("several"),  QStringLiteral("name"),
                                     QStringLiteral("nothing"),   QStringLiteral("value")};
      for (const QString& finnish : {driven, plane}) {
        const QStringList words = finnish.toLower().split(QRegularExpression(QStringLiteral("[^\\p{L}]+")),
                                                       Qt::SkipEmptyParts);
        for (const QString& word : words) {
          if (englishWords.contains(word)) {
            std::fprintf(stderr, "model_messages.cpp: an English word \"%s\" left in \"%s\"\n",
                         word.toUtf8().constData(), finnish.toUtf8().constData());
            ++failures;
          }
        }
      }
    }
    // mitcad#1650: messages whose topological name was a positional value,
    // its kind put in ("of the pieces of a region", "the projection of an
    // edge"), each translated whole: in Finnish no English word is left.
    const QString pieces =
        translated(QStringLiteral("a loft section is one region; of the pieces of a region the first is used"));
    const QString projection =
        translated(QStringLiteral("the projection of an edge follows its source: nothing to project again"));
    if (!language.marked) {
      const QStringList englishWords{
          QStringLiteral("a"),      QStringLiteral("an"),      QStringLiteral("the"),     QStringLiteral("of"),
          QStringLiteral("loft"),   QStringLiteral("section"), QStringLiteral("is"),      QStringLiteral("one"),
          QStringLiteral("region"), QStringLiteral("pieces"),  QStringLiteral("first"),   QStringLiteral("used"),
          QStringLiteral("projection"), QStringLiteral("edge"), QStringLiteral("follows"), QStringLiteral("its"),
          QStringLiteral("source"), QStringLiteral("nothing"), QStringLiteral("to"),      QStringLiteral("project"),
          QStringLiteral("again")};
      for (const QString& finnish : {pieces, projection}) {
        const QStringList words = finnish.toLower().split(QRegularExpression(QStringLiteral("[^\\p{L}]+")),
                                                       Qt::SkipEmptyParts);
        for (const QString& word : words) {
          if (englishWords.contains(word)) {
            std::fprintf(stderr, "model_messages.cpp: an English word \"%s\" left in \"%s\"\n",
                         word.toUtf8().constData(), finnish.toUtf8().constData());
            ++failures;
          }
        }
      }
    }
    // mitcad#1651: the warning about the entities a projection does not
    // follow with, for one curve and for several entities of both kinds,
    // and a projection message that names Clean Up Stale Projections
    // (the command's name in the language), each translated whole: in
    // Finnish no English word is left.
    const QString oneCurve = translated(QStringLiteral(
        "the projection of an edge does not follow it with a curve: nothing like it is in it now, so it keeps its "
        "last geometry"));
    const QString several = translated(QStringLiteral(
        "the projection of a face does not follow it with curves and points: nothing like them is in it now, so "
        "they keep their last geometry"));
    const QString stale = translator.translate(
        "ModelMessage",
        "the projection of %1 cannot be projected again: its source is gone (Clean Up Stale Projections unlinks it)");
    CHECK(!stale.isEmpty());
    if (!language.marked) {
      const QStringList englishWords{
          QStringLiteral("a"),         QStringLiteral("an"),       QStringLiteral("the"),      QStringLiteral("of"),
          QStringLiteral("projection"), QStringLiteral("face"),    QStringLiteral("edge"),     QStringLiteral("does"),
          QStringLiteral("not"),       QStringLiteral("follow"),   QStringLiteral("it"),       QStringLiteral("with"),
          QStringLiteral("curve"),     QStringLiteral("curves"),   QStringLiteral("point"),    QStringLiteral("points"),
          QStringLiteral("and"),       QStringLiteral("nothing"),  QStringLiteral("like"),     QStringLiteral("them"),
          QStringLiteral("is"),        QStringLiteral("in"),       QStringLiteral("now"),      QStringLiteral("so"),
          QStringLiteral("they"),      QStringLiteral("keep"),     QStringLiteral("keeps"),    QStringLiteral("its"),
          QStringLiteral("their"),     QStringLiteral("last"),     QStringLiteral("geometry"), QStringLiteral("clean"),
          QStringLiteral("up"),        QStringLiteral("stale"),    QStringLiteral("projections"),
          QStringLiteral("cannot"),    QStringLiteral("be"),       QStringLiteral("projected"), QStringLiteral("again"),
          QStringLiteral("source"),    QStringLiteral("gone"),     QStringLiteral("unlinks")};
      for (const QString& finnish : {oneCurve, several, stale}) {
        const QStringList words = finnish.toLower().split(QRegularExpression(QStringLiteral("[^\\p{L}]+")),
                                                       Qt::SkipEmptyParts);
        for (const QString& word : words) {
          if (englishWords.contains(word)) {
            std::fprintf(stderr, "model_messages.cpp: an English word \"%s\" left in \"%s\"\n",
                         word.toUtf8().constData(), finnish.toUtf8().constData());
            ++failures;
          }
        }
      }
    }
  }
#endif
  return failures;
}
