// SPDX-License-Identifier: MIT
#include "view/SmallCirclesPreference.hpp"

#include <QSettings>

namespace mitcad {
namespace {

constexpr const char* kKey = "view/smallCircles";

} // namespace

QString smallCirclesId(SmallCircles choice) {
  return choice == SmallCircles::FullDetail ? QStringLiteral("full") : QStringLiteral("adaptive");
}

std::optional<SmallCircles> smallCirclesFromId(const QString& id) {
  if (id == QLatin1String("adaptive")) {
    return SmallCircles::Adaptive;
  }
  if (id == QLatin1String("full")) {
    return SmallCircles::FullDetail;
  }
  return std::nullopt;
}

SmallCircles savedSmallCircles() {
  return smallCirclesFromId(QSettings().value(QLatin1String(kKey)).toString()).value_or(SmallCircles::Adaptive);
}

void applySavedSmallCircles() { setViewSmallCircles(savedSmallCircles()); }

bool saveSmallCircles(SmallCircles choice) {
  if (savedSmallCircles() == choice && viewSmallCircles() == choice) {
    return false;
  }
  {
    QSettings settings;
    settings.setValue(QLatin1String(kKey), smallCirclesId(choice));
  }
  setViewSmallCircles(choice);
  emit SmallCirclesNotifier::instance().changed();
  return true;
}

SmallCirclesNotifier& SmallCirclesNotifier::instance() {
  static SmallCirclesNotifier notifier;
  return notifier;
}

} // namespace mitcad
