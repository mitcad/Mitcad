// SPDX-License-Identifier: MIT
// Ready to Render (mitcad#2013, render/RenderReadyCheck): the findings of
// the Render workspace's check in plain words, the problems first, and
// none for a design that is ready.
#include "UnitTests.hpp"
#include "render/RenderReadyCheck.hpp"

#include <cstdio>

#include <QDir>
#include <QJsonArray>
#include <QJsonObject>
#include <QRegularExpression>
#include <QTemporaryDir>

namespace {

QJsonObject body(bool visible, const QString& appearance) {
  QJsonObject b{{QStringLiteral("uid"), QStringLiteral("F1.b0")}};
  if (!visible) {
    b.insert(QStringLiteral("visible"), false);
  }
  if (!appearance.isEmpty()) {
    b.insert(QStringLiteral("appearance"), appearance);
  }
  return b;
}

} // namespace

int renderReadyCheckTests() {
  int failures = 0;
  const auto check = [&failures](bool ok, const char* message) {
    if (!ok) { std::fprintf(stderr, "render ready check: %s\n", message); ++failures; }
  };
  using mitcad::render::ReadyFinding;
  using mitcad::render::ReadyInput;
  using mitcad::render::readyFindings;
  QTemporaryDir folder;
  check(folder.isValid(), "a temporary folder");

  ReadyInput ready;
  ready.bodies = QJsonArray{body(true, QStringLiteral("paint_red"))};
  ready.settings = QJsonObject{{QStringLiteral("shots"), QJsonArray{QJsonObject{{QStringLiteral("id"), QStringLiteral("shot1")}}}}};
  ready.folder = folder.path();
  ready.allShots = true;
  check(readyFindings(ready).isEmpty(), "a shown body with its appearance, a shot and a folder: ready");
  check(QDir(folder.path()).entryList(QDir::Files | QDir::Hidden).isEmpty(), "the folder's probe file is removed");

  ReadyInput hidden = ready;
  hidden.bodies = QJsonArray{body(true, QStringLiteral("paint_red")), body(false, QString()), body(true, QString())};
  const QList<ReadyFinding> found = readyFindings(hidden);
  check(found.size() == 2, "a hidden body and one in the default look");
  check(!mitcad::render::hasProblem(found), "neither stops a render");
  check(found.value(0).level == ReadyFinding::Level::Warning, "the hidden body first");
  check(found.value(0).text.contains(QStringLiteral("hidden")), "the hidden body said in words");
  check(found.value(1).text.contains(QStringLiteral("default look")), "the default look said in words");

  ReadyInput nothing = ready;
  nothing.bodies = QJsonArray{body(false, QString())};
  nothing.settings = QJsonObject{{QStringLiteral("shots"), QJsonArray()},
                                 {QStringLiteral("environment"),
                                  QJsonObject{{QStringLiteral("preset"), QStringLiteral("image")},
                                              {QStringLiteral("image"), QStringLiteral("missing-sky.hdr")}}}};
  nothing.documentFolder = folder.path();
  nothing.folder = folder.filePath(QStringLiteral("not-there"));
  const QList<ReadyFinding> problems = readyFindings(nothing);
  check(mitcad::render::hasProblem(problems), "nothing shown is a problem");
  check(problems.size() == 5, "nothing shown, the hidden body, the image, no shots, the folder");
  check(problems.value(0).level == ReadyFinding::Level::Problem, "the problems first");
  check(problems.last().level == ReadyFinding::Level::Warning, "the warning last");
  // Plain words: no ids, no file paths, no coordinates.
  const QRegularExpression internal(QStringLiteral("F\\d|shot\\d|not-there|missing-sky|\\d+\\.\\d+"));
  for (const ReadyFinding& finding : problems) {
    check(!internal.match(finding.text).hasMatch(), "a finding without ids, paths or numbers of places");
  }

  ReadyInput single = nothing;
  single.allShots = false;
  check(readyFindings(single).size() == 3, "a single image needs no shots or folder");
  return failures;
}
MITCAD_UNIT_TEST(renderReadyCheckTests);
