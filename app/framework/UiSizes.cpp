// SPDX-License-Identifier: MIT
#include "framework/UiSizes.hpp"

#include <QApplication>
#include <QFont>
#include <QFontDatabase>
#include <QFontInfo>
#include <QSettings>

namespace mitcad {
namespace {

constexpr const char* kTextKey = "appearance/textSize";
constexpr const char* kIconKey = "appearance/iconSize";
constexpr const char* kFontKey = "appearance/fontFamily";

bool isUiSize(int percent) {
  return percent >= kUiSizeMinimum && percent <= kUiSizeMaximum && percent % kUiSizeStep == 0;
}

bool saveSize(QSettings& settings, const char* key, int percent) {
  if (!isUiSize(percent) || uiSizeFromValue(settings.value(QLatin1String(key))) == percent) {
    return false;
  }
  settings.setValue(QLatin1String(key), percent);
  return true;
}

// A point size as the log shows it: 9, 7.2, 10.35.
QString points(double value) { return QString::number(value, 'g', 4); }

} // namespace

int uiSizeFromValue(const QVariant& value) {
  bool ok = false;
  const int percent = value.toInt(&ok);
  return ok && isUiSize(percent) ? percent : kDefaultUiSize;
}

int savedTextSize(const QSettings& settings) { return uiSizeFromValue(settings.value(QLatin1String(kTextKey))); }
int savedTextSize() { return savedTextSize(QSettings()); }
int savedIconSize(const QSettings& settings) { return uiSizeFromValue(settings.value(QLatin1String(kIconKey))); }
int savedIconSize() { return savedIconSize(QSettings()); }

QString savedFontFamily(const QSettings& settings) {
  return settings.value(QLatin1String(kFontKey)).toString().trimmed();
}
QString savedFontFamily() { return savedFontFamily(QSettings()); }

bool saveTextSize(QSettings& settings, int percent) { return saveSize(settings, kTextKey, percent); }
bool saveTextSize(int percent) {
  QSettings settings;
  return saveTextSize(settings, percent);
}
bool saveIconSize(QSettings& settings, int percent) { return saveSize(settings, kIconKey, percent); }
bool saveIconSize(int percent) {
  QSettings settings;
  return saveIconSize(settings, percent);
}

bool saveFontFamily(QSettings& settings, const QString& family) {
  const QString chosen = family.trimmed();
  if (savedFontFamily(settings) == chosen) {
    return false;
  }
  if (chosen.isEmpty()) {
    settings.remove(QLatin1String(kFontKey));
  } else {
    settings.setValue(QLatin1String(kFontKey), chosen);
  }
  return true;
}
bool saveFontFamily(const QString& family) {
  QSettings settings;
  return saveFontFamily(settings, family);
}

TextAndIconSize applyTextAndIconSize(int textPercent, int iconPercent, const QString& family) {
  TextAndIconSize sizes;
  sizes.textPercent = isUiSize(textPercent) ? textPercent : kDefaultUiSize;
  sizes.iconPercent = isUiSize(iconPercent) ? iconPercent : kDefaultUiSize;
  sizes.family = family.trimmed();
  ui_sizes_detail::textPercent = sizes.textPercent;
  ui_sizes_detail::iconPercent = sizes.iconPercent;

  QFont font = QApplication::font();
  ui_sizes_detail::platformPointSize = font.pointSizeF() > 0.0 ? font.pointSizeF() : 0.0;
  bool changed = false;
  if (!sizes.family.isEmpty()) {
    sizes.familyInstalled = QFontDatabase::hasFamily(sizes.family);
    if (sizes.familyInstalled) {
      font.setFamilies({sizes.family});
      ui_sizes_detail::fontChosen = true;
      changed = true;
    }
  }
  if (sizes.textPercent != kDefaultUiSize) {
    if (font.pointSizeF() > 0.0) {
      font.setPointSizeF(font.pointSizeF() * sizes.textPercent / 100.0);
    } else if (font.pixelSize() > 0) {
      font.setPixelSize(std::max(1, (font.pixelSize() * sizes.textPercent + 50) / 100));
    }
    changed = true;
  }
  if (changed) {
    QApplication::setFont(font);
  }
  return sizes;
}

TextAndIconSize applyTextAndIconSize() {
  const QSettings settings;
  return applyTextAndIconSize(savedTextSize(settings), savedIconSize(settings), savedFontFamily(settings));
}

QString textAndIconSizeDescription(const TextAndIconSize& sizes) {
  QString font = QStringLiteral("the system's");
  if (!sizes.family.isEmpty()) {
    font = sizes.familyInstalled ? sizes.family : QStringLiteral("%1 (not installed; the system's)").arg(sizes.family);
  }
  return QStringLiteral("Text size %1%, Icon size %2%, Font %3")
      .arg(sizes.textPercent)
      .arg(sizes.iconPercent)
      .arg(font);
}

QString applicationFontDescription() {
  const QFont font = QApplication::font();
  const QString family = QFontInfo(font).family();
  const QString size = font.pointSizeF() > 0.0 ? QStringLiteral("%1 pt").arg(points(font.pointSizeF()))
                                               : QStringLiteral("%1 px").arg(font.pixelSize());
  const QString platform = ui_sizes_detail::platformPointSize > 0.0
                               ? QStringLiteral("%1 pt").arg(points(ui_sizes_detail::platformPointSize))
                               : QStringLiteral("the platform's size");
  return QStringLiteral("%1, %2 (Text size %3% of %4)")
      .arg(family, size)
      .arg(ui_sizes_detail::textPercent)
      .arg(platform);
}

} // namespace mitcad
