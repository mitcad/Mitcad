// SPDX-License-Identifier: MIT
// Parameters in a drawing's texts (mitcad#2483): the reference Insert
// Parameter writes, the parameters a text refers to (read as the model
// reads them: other braces are the text's own), those the design lacks,
// and the parameters as Insert Parameter lists them.
#include "UnitTests.hpp"
#include "drawings/ParameterReferences.hpp"

#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "parameter_references.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

QJsonObject parameter(const QString& name, const QString& text, const QString& kind) {
  return QJsonObject{{QStringLiteral("name"), name},
                     {QStringLiteral("text"), text},
                     {QStringLiteral("kind"), kind}};
}

} // namespace

int parameterReferencesTests() {
  using namespace mitcad::drawings;
  CHECK(parameterReference(QStringLiteral("Length")) == QStringLiteral("{param:Length}"));

  // References, each once, in order; the prefix in any case, spaces left out.
  CHECK(referencedParameters(QStringLiteral("L {param:Length} BY { PARAM: Maker } {param:Length}")) ==
        QStringList({QStringLiteral("Length"), QStringLiteral("Maker")}));
  // Other braces are the text's own.
  CHECK(referencedParameters(QStringLiteral("{title} {Set/Key} {param:} open { brace")).isEmpty());
  CHECK(referencedParameters(QStringLiteral("a {b {param:Maker}}")) == QStringList({QStringLiteral("Maker")}));
  CHECK(referencedParameters(QString()).isEmpty());

  QJsonArray parameters;
  parameters.append(parameter(QStringLiteral("d1"), QStringLiteral("20 mm"), QStringLiteral("model")));
  parameters.append(parameter(QStringLiteral("Length"), QStringLiteral("120 mm"), QStringLiteral("user")));
  parameters.append(parameter(QStringLiteral("Tilt"), QStringLiteral("15 deg"), QStringLiteral("user")));
  parameters.append(parameter(QStringLiteral("Maker"), QStringLiteral("ACME"), QStringLiteral("user")));

  CHECK(missingParameters(QStringLiteral("{param:Length} {param:Width} {param:Finish} {param:Width}"), parameters) ==
        QStringList({QStringLiteral("Width"), QStringLiteral("Finish")}));
  CHECK(missingParameters(QStringLiteral("{param:Maker} {title}"), parameters).isEmpty());

  // The user parameters first, then the model's; an angle in °.
  const QList<ParameterEntry> entries = parameterEntries(parameters);
  CHECK(entries.size() == 4);
  if (entries.size() == 4) {
    CHECK(entries[0].name == QStringLiteral("Length") && entries[0].value == QStringLiteral("120 mm") &&
          entries[0].user);
    CHECK(entries[1].name == QStringLiteral("Tilt") && entries[1].value == QStringLiteral("15°"));
    CHECK(entries[2].name == QStringLiteral("Maker") && entries[2].value == QStringLiteral("ACME"));
    CHECK(entries[3].name == QStringLiteral("d1") && !entries[3].user);
  }
  return failures;
}
MITCAD_UNIT_TEST(parameterReferencesTests);
