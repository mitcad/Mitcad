// SPDX-License-Identifier: MIT
#pragma once

// The designs the Start tab lists (mitcad#1258, app/COMMANDS.md "The Start
// tab"): File › Open Recent's list (the setting recentFiles, newest first,
// at most kRecentCount), when each was last opened or saved
// (start/recentOpened), the ones the user pinned with the star
// (start/pinnedDesigns, kept also when they drop out of the recent list),
// and each design's preview: a PNG a save writes from the 3D view, kept in
// the user's local application data by the design's path (designPreviewPath),
// never in the design's file or its folder, so files of every version stay
// as they are. Plain Qt Core; the settings are the application's QSettings.

#include <QDateTime>
#include <QList>
#include <QString>
#include <QStringList>

namespace mitcad {

struct RecentDesign {
  QString path;   // absolute
  QString name;   // "bracket.mitcad"
  QString type;   // "MITCAD", "STEP", "F3D": the suffix in capitals
  QString folder; // the last two folders, "Projects / Robot arm"
  QDateTime opened; // last opened or saved (invalid: not known)
  bool pinned = false;
  bool missing = false; // the file is not there
  QString preview;      // the preview's file, empty when there is none
};

namespace recent {

// File › Open Recent's length.
inline constexpr int kRecentCount = 10;

// The settings keys.
QString recentFilesKey();   // "recentFiles"
QString openedKey();        // "start/recentOpened"
QString pinnedKey();        // "start/pinnedDesigns"
QString showAtLaunchKey();  // "start/showAtLaunch"

// A file opened or saved: first in the list, its time noted.
void noteOpened(const QString& path, const QDateTime& when = QDateTime::currentDateTime());
// The list: the recent files newest first, then the pinned ones no longer
// among them; each with its time, pin, preview and whether it exists.
QList<RecentDesign> load();
// Pins or unpins a design (the star).
void setPinned(const QString& path, bool pinned);
// The pinned designs' paths (absolute), in the order they were pinned: also
// a project's designs pinned on the Start page's project browser
// (mitcad#1301).
QStringList pinnedPaths();
// Takes a design off the list and its pin (a missing file's Remove).
void remove(const QString& path);
// Clear Recent Files: the list and the times; the pins stay.
void clearRecent();

// The designs shown: all or only the pinned ones, whose name or folder
// contains `search` (case-insensitive; empty: all).
QList<RecentDesign> filter(const QList<RecentDesign>& designs, bool pinnedOnly, const QString& search);

// "Today", "Yesterday", "3 days ago", then the date.
QString openedText(const QDateTime& opened, const QDateTime& now = QDateTime::currentDateTime());
// The last two folders of a path's folder, "Projects / Robot arm".
QString folderText(const QString& path);
// The type shown on a design's card: its suffix in capitals.
QString typeText(const QString& path);

// Where a design's preview is kept: "recent/<hash of the absolute
// path>.png" in the folder of the version previews (versionThumbnailPath's:
// MITCAD_THUMBNAIL_DIR for tests, else "thumbnails" in the local
// application data). Empty when the system gives no place.
QString designPreviewPath(const QString& path);
// The preview's longest side, as the version previews'.
inline constexpr int kPreviewSize = 256;

// Whether the Start tab shows when MitCad starts (on by default).
bool showAtLaunch();
void setShowAtLaunch(bool shown);

} // namespace recent
} // namespace mitcad
