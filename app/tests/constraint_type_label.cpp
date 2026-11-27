// SPDX-License-Identifier: MIT
// The Constraint panels' Type list (mitcad#2527): its Automatic row names
// the inferred type in the interface language, as the type's own row below
// it does. The ticket's case: in Finnish, two faces picked whose type is
// Align read the translated pattern around the translated Align, not
// around the English word.
#include "UnitTests.hpp"
#include "commands/ConstraintTypeChoice.hpp"

#include <cstdio>

#include <QCoreApplication>
#include <QDir>
#include <QString>
#include <QTranslator>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line, const QString& detail = QString()) {
  if (!condition) {
    std::fprintf(stderr, "constraint_type_label.cpp:%d: check failed: %s %s\n", line, expression,
                 qPrintable(detail));
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)
#define CHECK_FOR(expr, detail) check((expr), #expr, __LINE__, (detail))

} // namespace

int constraintTypeLabelTests() {
  failures = 0;
  using mitcad::automaticTypeLabel;
  // English.
  CHECK(automaticTypeLabel(QString()) == QStringLiteral("Automatic"));
  CHECK(automaticTypeLabel(QStringLiteral("Align")) == QStringLiteral("Automatic (Align)"));

#ifdef MITCAD_TEST_TRANSLATIONS_DIR
  // Finnish, from the catalogue the build made.
  QTranslator finnish;
  CHECK(finnish.load(QDir(QStringLiteral(MITCAD_TEST_TRANSLATIONS_DIR)).filePath(QStringLiteral("mitcad_fi.qm"))));
  CHECK(QCoreApplication::installTranslator(&finnish));
  // The catalogue's own texts, not literals here (the public tree carries
  // English only): the plain row and the pattern are translated.
  const QString automatic = QCoreApplication::translate("QObject", "Automatic");
  const QString pattern = QCoreApplication::translate("QObject", "Automatic (%1)");
  CHECK(automatic != QStringLiteral("Automatic"));
  CHECK(pattern != QStringLiteral("Automatic (%1)"));
  CHECK(automaticTypeLabel(QString()) == automatic);
  // Every type of the list (kConstraintTypes in commands/JointCommands.cpp):
  // the row reads the label its own row shows (the panel's choiceLabel:
  // the Choice context), which the catalogue translates; never the English
  // label inside the translated pattern.
  const char* const typeLabels[] = {"Align", "Angle", "Center", "Tangent", "Insert", "Insert (Axis)"};
  for (const char* label : typeLabels) {
    const QString shown = QCoreApplication::translate("Choice", label);
    CHECK_FOR(shown != QLatin1String(label), QString::fromLatin1(label));
    const QString row = automaticTypeLabel(QString::fromLatin1(label));
    CHECK_FOR(row == pattern.arg(shown), row);
    CHECK_FOR(row != pattern.arg(QString::fromLatin1(label)), row);
  }
  QCoreApplication::removeTranslator(&finnish);
#endif
  return failures;
}
MITCAD_UNIT_TEST(constraintTypeLabelTests);
