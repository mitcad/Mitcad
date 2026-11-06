// SPDX-License-Identifier: MIT
#include "DrawingPreferences.hpp"

#include <initializer_list>

#include <QVariant>

namespace mitcad::drawings {

namespace {

const char* const kAlways = "drawing/alwaysShowCorners";
const char* const kSize = "drawing/cornerDotSize";

} // namespace

QString cornerDotSizeId(CornerDotSize size) {
  switch (size) {
  case CornerDotSize::Small:
    return QStringLiteral("small");
  case CornerDotSize::Large:
    return QStringLiteral("large");
  case CornerDotSize::Medium:
    break;
  }
  return QStringLiteral("medium");
}

std::optional<CornerDotSize> cornerDotSizeFromId(const QString& id) {
  for (const CornerDotSize size : {CornerDotSize::Small, CornerDotSize::Medium, CornerDotSize::Large}) {
    if (cornerDotSizeId(size) == id) {
      return size;
    }
  }
  return std::nullopt;
}

double cornerDotRadius(CornerDotSize size) {
  switch (size) {
  case CornerDotSize::Small:
    return 2.0;
  case CornerDotSize::Large:
    return 4.5;
  case CornerDotSize::Medium:
    break;
  }
  return 3.0;
}

CornerDots CornerDots::load(const QSettings& settings) {
  CornerDots dots;
  dots.always = settings.value(QLatin1String(kAlways), false).toBool();
  dots.size = cornerDotSizeFromId(settings.value(QLatin1String(kSize)).toString()).value_or(CornerDotSize::Medium);
  return dots;
}

void CornerDots::save(QSettings& settings) const {
  settings.setValue(QLatin1String(kAlways), always);
  settings.setValue(QLatin1String(kSize), cornerDotSizeId(size));
}

CornerDots savedCornerDots() { return CornerDots::load(QSettings()); }

bool saveCornerDots(const CornerDots& dots) {
  if (savedCornerDots() == dots) {
    return false;
  }
  {
    QSettings settings;
    dots.save(settings);
  }
  emit CornerDotsNotifier::instance().changed();
  return true;
}

CornerDotsNotifier& CornerDotsNotifier::instance() {
  static CornerDotsNotifier notifier;
  return notifier;
}

} // namespace mitcad::drawings
