// SPDX-License-Identifier: MIT
#include "framework/InterfaceSize.hpp"

#include <QGuiApplication>
#include <QScreen>
#include <QSettings>
#include <QtGlobal>

namespace mitcad {
namespace {

constexpr const char* kKey = "appearance/interfaceSize";
// The user's own QT_SCALE_FACTOR, for the processes this one starts.
constexpr const char* kUserScaleVariable = "MITCAD_USER_QT_SCALE_FACTOR";
constexpr const char* kQtScaleVariable = "QT_SCALE_FACTOR";
// What kUserScaleVariable holds when the user set none: an empty variable
// cannot be kept on Windows, where setting one removes it.
constexpr const char* kNone = "none";

bool isChoice(int percent) {
  for (const int size : kInterfaceSizes) {
    if (size == percent) {
      return true;
    }
  }
  return false;
}

} // namespace

int interfaceSizeFromValue(const QVariant& value) {
  bool ok = false;
  const int percent = value.toInt(&ok);
  return ok && isChoice(percent) ? percent : kDefaultInterfaceSize;
}

int savedInterfaceSize(const QSettings& settings) {
  return interfaceSizeFromValue(settings.value(QLatin1String(kKey)));
}

int savedInterfaceSize() { return savedInterfaceSize(QSettings()); }

bool saveInterfaceSize(QSettings& settings, int percent) {
  if (!isChoice(percent) || savedInterfaceSize(settings) == percent) {
    return false;
  }
  settings.setValue(QLatin1String(kKey), percent);
  return true;
}

bool saveInterfaceSize(int percent) {
  QSettings settings;
  return saveInterfaceSize(settings, percent);
}

double userScaleFactor(const QByteArray& value) {
  bool ok = false;
  const double factor = value.trimmed().toDouble(&ok);
  return ok && factor > 0.0 ? factor : 1.0;
}

double interfaceScaleFactor(int percent, const QByteArray& userValue) {
  const int chosen = isChoice(percent) ? percent : kDefaultInterfaceSize;
  return userScaleFactor(userValue) * chosen / 100.0;
}

InterfaceScale applyInterfaceSize(int percent) {
  InterfaceScale scale;
  // The user's value: kept by the process that started this one, else
  // QT_SCALE_FACTOR as the user's environment has it.
  if (qEnvironmentVariableIsSet(kUserScaleVariable)) {
    const QByteArray kept = qgetenv(kUserScaleVariable);
    scale.userValue = kept == kNone ? QByteArray() : kept;
  } else {
    scale.userValue = qgetenv(kQtScaleVariable);
    qputenv(kUserScaleVariable, scale.userValue.isEmpty() ? QByteArray(kNone) : scale.userValue);
  }
  scale.percent = isChoice(percent) ? percent : kDefaultInterfaceSize;
  if (scale.percent == kDefaultInterfaceSize) {
    // The user's own value as it was (a process started from one with
    // another size inherited that one's product).
    if (scale.userValue.isEmpty()) {
      qunsetenv(kQtScaleVariable);
    } else {
      qputenv(kQtScaleVariable, scale.userValue);
    }
    scale.factor = userScaleFactor(scale.userValue);
    return scale;
  }
  scale.factor = interfaceScaleFactor(scale.percent, scale.userValue);
  qputenv(kQtScaleVariable, QByteArray::number(scale.factor, 'g', 6));
  return scale;
}

InterfaceScale applyInterfaceSize() { return applyInterfaceSize(savedInterfaceSize()); }

QSize startWindowSize() {
  QSize size(1280, 800);
  if (const QScreen* screen = QGuiApplication::primaryScreen()) {
    size = size.boundedTo(screen->availableGeometry().size());
  }
  return size;
}

QString interfaceScaleDescription(const InterfaceScale& scale) {
  return QStringLiteral("%1% (Qt scale factor %2; the user's QT_SCALE_FACTOR %3)")
      .arg(scale.percent)
      .arg(scale.factor)
      .arg(scale.userValue.isEmpty() ? QStringLiteral("none") : QString::fromLocal8Bit(scale.userValue));
}

} // namespace mitcad
