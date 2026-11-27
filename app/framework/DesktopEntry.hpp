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

// File managers' thumbnails of designs (mitcad#1539) likewise: the
// AppImage's thumbnailer, copied out of it (its folder goes away when
// MitCad closes) to mitcad/bin/ in the data folder, its freedesktop entry
// (thumbnailers/mitcad.thumbnailer) running that copy, and the .mitcad
// MIME type with the import types the system lacks
// (mime/packages/mitcad.xml), after which the MIME database is updated.
// The bundled desktop file's Exec takes the file (%f) and its MimeType
// lists those types, so that file managers open them in MitCad
// (mitcad#2527).

// The bundled desktop file with Exec running the program (the bundled
// Exec's arguments after it), TryExec naming it, X-AppImage-Name (the
// bundled Name) and X-AppImage-Version (version, when given), for AppImage
// managers that list programs by them (mitcad#2413), merged into the
// earlier entry at the same place (empty when there is none), which an
// AppImage manager may have written: the keys and groups the bundled file
// does not define stay (a manager's X-AppImage-* keys, a user's NoDisplay),
// the ones it defines are its, a manager's X-AppImage-Name stays, and when
// the earlier Exec runs the same program, what stands around it there
// stays (env DESKTOPINTEGRATION=1, field codes). Merging the result again
// gives the same text.
QString desktopEntryFor(const QString& bundled, const QString& program, const QString& version,
                        const QString& earlier);

// The bundled thumbnailer entry with TryExec and Exec running the program;
// Exec's arguments (%i %o %s) stay.
QString thumbnailerEntryFor(const QString& bundled, const QString& program);

// The desktop file and the icons of the AppImage at appImage, unpacked at
// appDir, put into dataHome (applications/, icons/hicolor/), and the
// thumbnailer with its entry and the MIME type when the AppImage has them;
// only the files whose content differs are written. The desktop file is
// desktopEntryFor's merge into the one already there, with the AppImage's
// version.
struct DesktopIntegration {
  QStringList written;
  QString error; // empty when everything is in place
};
DesktopIntegration integrateAppImage(const QString& appImage, const QString& appDir, const QString& dataHome,
                                     const QString& version);

// The above when Mitcad runs as an AppImage (APPIMAGE and APPDIR, set by its
// runtime) on Linux, into $XDG_DATA_HOME (~/.local/share), with the
// application's version; logged.
// Elsewhere it does nothing.
void integrateDesktop();

} // namespace mitcad
