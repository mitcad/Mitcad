// SPDX-License-Identifier: MIT
#pragma once

// Preferences › Display › Interface size (mitcad#1495, GitHub
// mitcad/Mitcad#194): the whole UI larger, 100 % (the default), 125, 150,
// 175 or 200 %. Kept in the application's settings
// ("appearance/interfaceSize", the percent) and applied when MitCad starts
// as Qt's scale factor (QT_SCALE_FACTOR, set before the application object
// exists), multiplied with a QT_SCALE_FACTOR the user set: icons, texts,
// panels, the ribbon, the floating cards and the 3D view's overlays scale
// together, and the 3D view renders at the resulting device pixel ratio.
// A change takes effect at the next start.

#include <QByteArray>
#include <QSize>
#include <QString>
#include <QVariant>

class QSettings;

namespace mitcad {

// The choices in percent, smallest first.
inline constexpr int kInterfaceSizes[] = {100, 125, 150, 175, 200};
inline constexpr int kDefaultInterfaceSize = 100;

// A kept value as a choice: one of kInterfaceSizes, else the default.
int interfaceSizeFromValue(const QVariant& value);

// The choice kept in `settings`; without one, the application's
// (QSettings; also before the application object exists, once its names
// are set).
int savedInterfaceSize(const QSettings& settings);
int savedInterfaceSize();

// Saves the choice for the next start; false when it was already so (or is
// not one of kInterfaceSizes).
bool saveInterfaceSize(QSettings& settings, int percent);
bool saveInterfaceSize(int percent);

// The user's own QT_SCALE_FACTOR as a factor: 1 when it is empty or not a
// positive number.
double userScaleFactor(const QByteArray& value);

// The scale factor Qt gets for `percent` over the user's own factor.
double interfaceScaleFactor(int percent, const QByteArray& userValue);

// What applyInterfaceSize set, for the start-up's log line.
struct InterfaceScale {
  int percent = kDefaultInterfaceSize;
  // The user's own QT_SCALE_FACTOR, empty when none was set.
  QByteArray userValue;
  // The factor Qt scales by (1 without any).
  double factor = 1.0;
};

// Before the application object exists: QT_SCALE_FACTOR for the saved
// choice (or `percent`). The user's own value is kept in
// MITCAD_USER_QT_SCALE_FACTOR ("none" when there was none), so that a
// process started from this one (a restart, an installed update)
// multiplies the user's value again, not this one's product; at 100 % the
// user's own value is put back.
InterfaceScale applyInterfaceSize(int percent);
InterfaceScale applyInterfaceSize();

// The size a new design window starts at: 1280 x 800 logical pixels, or the
// primary screen's available area where that is smaller (a larger
// Interface size or the system's scaling leaves fewer logical pixels).
QSize startWindowSize();

// "150% (Qt scale factor 1.5; the user's QT_SCALE_FACTOR none)".
QString interfaceScaleDescription(const InterfaceScale& scale);

} // namespace mitcad
