// SPDX-License-Identifier: MIT
#pragma once

#include <QString>
#include <QStringList>

namespace mitcad {

// A Linux desktop shows an application's icon in its dock and task bar from
// the desktop file that matches its window (GNOME 45 and newer use nothing
// else), and an AppImage installs none. Started as one, Mitcad puts its own
// desktop file, Exec pointing to the AppImage, and its icons into the
// user's data folder, and again when the AppImage has moved.

// The bundled desktop file with Exec running the program.
QString desktopEntryFor(const QString& bundled, const QString& program);

// The desktop file and the icons of the AppImage at appImage, unpacked at
// appDir, put into dataHome (applications/, icons/hicolor/); only the files
// whose content differs are written.
struct DesktopIntegration {
  QStringList written;
  QString error; // empty when everything is in place
};
DesktopIntegration integrateAppImage(const QString& appImage, const QString& appDir, const QString& dataHome);

// The above when Mitcad runs as an AppImage (APPIMAGE and APPDIR, set by its
// runtime) on Linux, into $XDG_DATA_HOME (~/.local/share); logged.
// Elsewhere it does nothing.
void integrateDesktop();

} // namespace mitcad
