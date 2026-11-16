// SPDX-License-Identifier: MIT
// An .iam assembly's import in the application (mitcad#507): Parts with
// their design history on by default and remembered across a new settings
// object (a restart); the worker's part progress lines read, other lines
// not; each part's outcome as the Import Report shows it.
#include "UnitTests.hpp"
#include "files/IamImport.hpp"

#include <cstdio>
#include <optional>

#include <QJsonObject>
#include <QSettings>
#include <QString>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "iam_import.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

} // namespace

int iamImportTests() {
  using mitcad::iamPartOutcome;
  using mitcad::iamPartsHistory;
  using mitcad::IamPartProgress;
  using mitcad::parseIamPartProgress;
  using mitcad::rememberIamPartsHistory;

  // The choice: on until turned off, kept by the settings.
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QString ini = directory.filePath(QStringLiteral("settings.ini"));
  {
    QSettings settings(ini, QSettings::IniFormat);
    CHECK(iamPartsHistory(settings));
    rememberIamPartsHistory(settings, false);
    CHECK(!iamPartsHistory(settings));
  }
  {
    QSettings settings(ini, QSettings::IniFormat);
    CHECK(!iamPartsHistory(settings));
    CHECK(settings.value(QStringLiteral("import/iamPartsHistory")).toBool() == false);
    rememberIamPartsHistory(settings, true);
  }
  {
    QSettings settings(ini, QSettings::IniFormat);
    CHECK(iamPartsHistory(settings));
  }

  // The worker's progress lines: a part started, a part done.
  const std::optional<IamPartProgress> started =
      parseIamPartProgress(QStringLiteral("import: [1.25] part 2 of 7: Base plate.ipt"));
  CHECK(started.has_value());
  if (started) {
    CHECK(started->part == 2);
    CHECK(started->parts == 7);
    CHECK(started->file == QStringLiteral("Base plate.ipt"));
    CHECK(started->outcome.isEmpty());
  }
  const std::optional<IamPartProgress> done =
      parseIamPartProgress(QStringLiteral("import: [9.50] part 2 of 7: Base plate.ipt: replayed in 8.25 s"));
  CHECK(done.has_value());
  if (done) {
    CHECK(done->part == 2);
    CHECK(done->parts == 7);
    CHECK(done->file == QStringLiteral("Base plate.ipt"));
    CHECK(done->outcome == QStringLiteral("replayed"));
    CHECK(done->seconds > 8.2 && done->seconds < 8.3);
  }
  const std::optional<IamPartProgress> bodies =
      parseIamPartProgress(QStringLiteral("import: [12.00] part 3 of 7: pin.ipt: bodies in 0.10 s"));
  CHECK(bodies.has_value() && bodies->outcome == QStringLiteral("bodies") && bodies->file == QStringLiteral("pin.ipt"));
  // A timeline item's lines and other output are no part's.
  CHECK(!parseIamPartProgress(QStringLiteral("import: [1.00] item 3: Extrusion1")).has_value());
  CHECK(!parseIamPartProgress(QStringLiteral("import: [1.00] Extrusion1: parametric in 0.20 s")).has_value());
  CHECK(!parseIamPartProgress(QStringLiteral("import-items 12")).has_value());

  // A part's outcome: replayed with its counts, or its stored bodies and why.
  const QJsonObject replayed{{QStringLiteral("outcome"), QStringLiteral("replayed")},
                             {QStringLiteral("items"), 12},
                             {QStringLiteral("verified"), 9},
                             {QStringLiteral("parameters"), 3},
                             {QStringLiteral("counts"), QJsonObject{{QStringLiteral("parametric"), 10},
                                                                    {QStringLiteral("partial"), 1},
                                                                    {QStringLiteral("fallback"), 1}}}};
  CHECK(iamPartOutcome(replayed) ==
        QStringLiteral("replayed, 12 items (10 parametric, 1 partial, 1 fallback, 0 skipped), 9 verified, 3 parameters"));
  const QJsonObject failed{{QStringLiteral("outcome"), QStringLiteral("bodies")},
                           {QStringLiteral("reason"), QStringLiteral("the replay failed: no profile")}};
  CHECK(iamPartOutcome(failed) == QStringLiteral("stored bodies: the replay failed: no profile"));
  // An older report, or a file that was not imported: nothing to say.
  CHECK(iamPartOutcome(QJsonObject{{QStringLiteral("bodies"), 2}}).isEmpty());
  CHECK(iamPartOutcome(QJsonObject()).isEmpty());

  return failures;
}
MITCAD_UNIT_TEST(iamImportTests);
