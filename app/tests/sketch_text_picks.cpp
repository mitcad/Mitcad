// SPDX-License-Identifier: MIT
// The text a pick inside a letter stands for (mitcad#1369): a region a
// text's contours bound (a letter's stroke or its counter, the hole of an
// 'o') names its text, as its outline does; a region of no text, of
// another sketch or of a text that is gone names none. The item Edit Text's
// input keeps for such a region is the text's curve pick, and an input made
// as Edit Text's takes the region selected before it.
#include "sketch/SketchModel.hpp"

#include <cstdio>
#include <initializer_list>

#include <QString>

#include "framework/Command.hpp"
#include "framework/Selection.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "sketch_text_picks.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;
using namespace mitcad::sketch;

// Sketch F1 with the text t2 ("Io") and the QR code t5; the regions as the
// `sketch` query names them: the I, the o's stroke and its counter, a code
// module, a rectangle round them all (no text).
SketchModel textSketch() {
  SketchModel model;
  model.uid = QStringLiteral("F1");
  TextData text;
  text.id = QStringLiteral("t2");
  text.text = QStringLiteral("Io");
  model.texts.push_back(text);
  TextData code;
  code.id = QStringLiteral("t5");
  code.text = QStringLiteral("https://mitcad.org");
  code.code = true;
  model.texts.push_back(code);
  model.regionTexts.insert(QStringLiteral("r{t2.g0.c0}"), QStringLiteral("t2"));
  model.regionTexts.insert(QStringLiteral("r{t2.g1.c0}"), QStringLiteral("t2"));
  model.regionTexts.insert(QStringLiteral("r{t2.g1.c1}"), QStringLiteral("t2"));
  model.regionTexts.insert(QStringLiteral("r{t5.g0.c0}"), QStringLiteral("t5"));
  // A region of a text deleted since (an answer from before).
  model.regionTexts.insert(QStringLiteral("r{t9.g0.c0}"), QStringLiteral("t9"));
  return model;
}

SelectionItem profile(const QString& sketch, const QString& key) {
  return {SelectKind::Profile, sketch, key, QStringLiteral("plane")};
}

// A sketch curve of F1 (a text's outline is picked by the text's id).
SelectionItem curvePick(const QString& id) {
  return {SelectKind::SketchCurve, QStringLiteral("F1"), id, QString()};
}

void pickTests() {
  const SketchModel model = textSketch();
  // The outline and the regions of the letters and of the counter.
  const TextData* outline = model.textOfPick(curvePick(QStringLiteral("t2")));
  CHECK(outline != nullptr && outline->id == QStringLiteral("t2"));
  for (const char* key : {"r{t2.g0.c0}", "r{t2.g1.c0}", "r{t2.g1.c1}"}) {
    const TextData* text = model.textOfPick(profile(QStringLiteral("F1"), QString::fromLatin1(key)));
    CHECK(text != nullptr && text->id == QStringLiteral("t2") && !text->code);
  }
  const TextData* code = model.textOfPick(profile(QStringLiteral("F1"), QStringLiteral("r{t5.g0.c0}")));
  CHECK(code != nullptr && code->id == QStringLiteral("t5") && code->code);
  // No text: the rectangle's region, another sketch's region with the same
  // key, a curve that is no text, a text gone since.
  const QString rect = QStringLiteral("r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}");
  CHECK(model.textOfPick(profile(QStringLiteral("F1"), rect)) == nullptr);
  CHECK(model.textOfPick(profile(QStringLiteral("F3"), QStringLiteral("r{t2.g0.c0}"))) == nullptr);
  CHECK(model.textOfPick(curvePick(QStringLiteral("c3"))) == nullptr);
  CHECK(model.textOfPick(profile(QStringLiteral("F1"), QStringLiteral("r{t9.g0.c0}"))) == nullptr);
}

void itemTests() {
  const SketchModel model = textSketch();
  // A letter's region becomes the text's curve pick, its occurrence kept.
  SelectionItem counter = profile(QStringLiteral("F1"), QStringLiteral("r{t2.g1.c1}"));
  counter.occurrence = QStringLiteral("O1");
  const SelectionItem text = textSelectionItem(model, counter);
  CHECK(text.kind == SelectKind::SketchCurve && text.owner == QStringLiteral("F1") &&
        text.name == QStringLiteral("t2") && text.occurrence == QStringLiteral("O1"));
  // Anything else stays as it is.
  const SelectionItem rect = profile(QStringLiteral("F1"), QStringLiteral("r{c1[c2]}"));
  CHECK(textSelectionItem(model, rect) == rect);
  const SelectionItem other = profile(QStringLiteral("F3"), QStringLiteral("r{t2.g0.c0}"));
  CHECK(textSelectionItem(model, other) == other);
  const SelectionItem curve = curvePick(QStringLiteral("c3"));
  CHECK(textSelectionItem(model, curve) == curve);
}

void inputTests() {
  // An input as Edit Text's: sketch curves of a text that is no code, and
  // the regions selected before it in their text's form.
  const SketchModel model = textSketch();
  const InputDef input =
      selectionInput(QStringLiteral("text"), QStringLiteral("Text"), SelectKind::SketchCurve, 1, 1)
          .withAccepts([&model](const SelectionItem& item) {
            const TextData* t = model.textOfPick(item);
            return item.kind == SelectKind::SketchCurve && t != nullptr && !t->code;
          })
          .withAlsoTakes(SelectKind::Profile)
          .withConvert([&model](const SelectionItem& item) { return textSelectionItem(model, item); });
  CHECK(input.takes(profile(QStringLiteral("F1"), QStringLiteral("r{t2.g1.c1}"))));
  CHECK(input.takes(profile(QStringLiteral("F1"), QStringLiteral("r{t2.g0.c0}"))));
  CHECK(input.takes(curvePick(QStringLiteral("t2"))));
  // Not a code's region (Edit QR Code's), a region of no text or another
  // sketch's; the view still picks sketch curves only.
  CHECK(!input.takes(profile(QStringLiteral("F1"), QStringLiteral("r{t5.g0.c0}"))));
  CHECK(!input.takes(profile(QStringLiteral("F1"), QStringLiteral("r{c1[c2]}"))));
  CHECK(!input.takes(profile(QStringLiteral("F3"), QStringLiteral("r{t2.g0.c0}"))));
  CHECK(!input.accepts(profile(QStringLiteral("F1"), QStringLiteral("r{t2.g1.c1}"))));
}

} // namespace

int sketchTextPickTests() {
  failures = 0;
  pickTests();
  itemTests();
  inputTests();
  return failures;
}
