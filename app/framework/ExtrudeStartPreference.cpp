// SPDX-License-Identifier: MIT
#include "ExtrudeStartPreference.hpp"

#include <QLatin1String>
#include <QSettings>

namespace mitcad {
namespace {

constexpr const char* kKey = "extrude/startOperation";

} // namespace

QString newFeatureOperationId(NewFeatureOperation choice) {
  return choice == NewFeatureOperation::NewBody ? QStringLiteral("new_body") : QStringLiteral("automatic");
}

std::optional<NewFeatureOperation> newFeatureOperationFromId(const QString& id) {
  if (id == QLatin1String("automatic")) {
    return NewFeatureOperation::Automatic;
  }
  if (id == QLatin1String("new_body")) {
    return NewFeatureOperation::NewBody;
  }
  return std::nullopt;
}

NewFeatureOperation savedExtrudeStart() {
  return newFeatureOperationFromId(QSettings().value(QLatin1String(kKey)).toString())
      .value_or(NewFeatureOperation::Automatic);
}

bool saveExtrudeStart(NewFeatureOperation choice) {
  if (savedExtrudeStart() == choice) {
    return false;
  }
  QSettings().setValue(QLatin1String(kKey), newFeatureOperationId(choice));
  return true;
}

} // namespace mitcad
