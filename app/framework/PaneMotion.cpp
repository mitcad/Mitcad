// SPDX-License-Identifier: MIT
// A pane's moves between its places (mitcad#2367): see PaneMotion.hpp.
#include "PaneMotion.hpp"

#include <algorithm>

#include <QtGlobal>

#if defined(Q_OS_WIN)
#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#ifdef _MSC_VER
#pragma comment(lib, "user32")
#endif
#elif defined(Q_OS_MACOS)
#include <CoreFoundation/CoreFoundation.h>
#else
#include <QDir>
#include <QProcess>
#include <QSettings>
#include <QStandardPaths>
#include <QString>
#endif

namespace mitcad {
namespace {

#if !defined(Q_OS_WIN) && !defined(Q_OS_MACOS)
// KDE's animation speed: AnimationDurationFactor in kdeglobals' [KDE]
// group, 0 at the slider's Instant end. -1 when it is not set.
double kdeAnimationFactor() {
  const QString folder = QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation);
  const QString path = QDir(folder).filePath(QStringLiteral("kdeglobals"));
  if (folder.isEmpty() || !QDir(folder).exists(QStringLiteral("kdeglobals"))) {
    return -1.0;
  }
  QSettings globals(path, QSettings::IniFormat);
  bool ok = false;
  const double factor = globals.value(QStringLiteral("KDE/AnimationDurationFactor")).toDouble(&ok);
  return ok ? factor : -1.0;
}

// GNOME's (and the desktops on its settings'): enable-animations false.
bool gnomeAnimationsOff() {
  const QString gsettings = QStandardPaths::findExecutable(QStringLiteral("gsettings"));
  if (gsettings.isEmpty()) {
    return false;
  }
  QProcess process;
  process.setProcessChannelMode(QProcess::SeparateChannels);
  process.start(gsettings, {QStringLiteral("get"), QStringLiteral("org.gnome.desktop.interface"),
                            QStringLiteral("enable-animations")});
  if (!process.waitForFinished(1000)) {
    process.kill();
    process.waitForFinished(1000);
    return false;
  }
  return process.exitCode() == 0 &&
         QString::fromUtf8(process.readAllStandardOutput()).trimmed() == QLatin1String("false");
}
#endif

} // namespace

bool reducedMotion() {
  const QByteArray given = qgetenv("MITCAD_REDUCED_MOTION");
  if (!given.isEmpty()) {
    return given != "0";
  }
#if defined(Q_OS_WIN)
  BOOL animations = TRUE;
  if (SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, &animations, 0) != FALSE) {
    return animations == FALSE;
  }
  return false;
#elif defined(Q_OS_MACOS)
  Boolean valid = false;
  const Boolean reduce =
      CFPreferencesGetAppBooleanValue(CFSTR("reduceMotion"), CFSTR("com.apple.universalaccess"), &valid);
  return valid && reduce;
#else
  if (qEnvironmentVariable("XDG_CURRENT_DESKTOP").contains(QLatin1String("KDE"), Qt::CaseInsensitive)) {
    const double factor = kdeAnimationFactor();
    if (factor >= 0.0) {
      return factor <= 0.0;
    }
  }
  return gnomeAnimationsOff();
#endif
}

int paneMoveMs() { return reducedMotion() ? 0 : kPaneMoveMs; }

QRect centredPaneRect(const QRect& area) {
  if (area.width() <= 0 || area.height() <= 0) {
    return QRect(area.topLeft(), QSize(0, 0));
  }
  const int width = std::min(area.width(), std::max(std::min(area.width(), 480), area.width() * 4 / 5));
  const int height = std::min(area.height(), std::max(std::min(area.height(), 320), area.height() * 9 / 10));
  return QRect(area.left() + (area.width() - width) / 2, area.top() + (area.height() - height) / 2, width, height);
}

} // namespace mitcad
