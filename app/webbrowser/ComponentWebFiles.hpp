// SPDX-License-Identifier: MIT
#pragma once

// Insert › Component from Web (mitcad#1579, app/COMMANDS.md "Component from
// Web"): what the component browser decides about its downloads without the
// browser itself, in plain Qt Core, so that the unit tests
// (app/tests/component_web_files.cpp) use it without Qt WebEngine: the
// format filter, the component's name and part number, the user's sites
// and the downloads' temporary folders.

#include <memory>

#include <QByteArray>
#include <QList>
#include <QString>

class QLockFile;

namespace mitcad {

// What a download is to the component browser.
enum class WebFileKind {
  Refused,   // not a file MitCad imports: refused with its note
  Component, // STEP, IGES, BRep, STL, OBJ or a design: a component of the open design
  Document,  // a design of another format (.f3d, .FCStd, .ipt, ...): a design of its own, as Import opens it
  Zip,       // a zip: its supported files listed to pick from
};

struct WebFileCheck {
  WebFileKind kind = WebFileKind::Refused;
  // Why a file is refused, one sentence that names its type ("MitCad cannot
  // import PDF documents (.pdf)."); empty otherwise.
  QString note;
};

// The format filter by the file's name: its extension.
WebFileCheck checkWebFileName(const QString& fileName);
// The same, then by the file's first bytes where that is cheap: a STEP file
// starts with its ISO-10303-21 header, a zip with its signature, and a web
// page (a site's sign-in or error page sent under the file's name) or a PDF
// is no model whatever its name says.
WebFileCheck checkWebFileContent(const QString& fileName, const QByteArray& head);
// How many bytes checkWebFileContent looks at.
constexpr qsizetype kWebFileHeadSize = 4096;

// The new component's name: the file's name without its extension
// ("6494K13_NO THREADS_Bearing.step" → "6494K13_NO THREADS_Bearing").
QString webComponentName(const QString& fileName);
// Its part number: the name's first part, up to the first underscore or
// space, when that looks like a part number (letters and digits, at least
// one digit: "6494K13"); empty otherwise.
QString webPartNumber(const QString& componentName);

// Why a download stopped, in words for "The download of <file> failed:
// <why>" (mitcad#1716): from the browser's interrupt code (Chromium's
// download interrupt reasons, as CEF's cef_download_interrupt_reason_t
// numbers them), never the code itself, which goes only to the log.
QString webDownloadFailure(int interruptReason);

// The user's sites (bookmarks): a name and a base URL, kept in the settings
// (componentBrowser/sites); none are bundled.
struct WebSite {
  QString name;
  QString url;
  bool operator==(const WebSite& other) const { return name == other.name && url == other.url; }
};
QList<WebSite> loadWebSites();
void saveWebSites(const QList<WebSite>& sites);
// A typed address as a URL text: "example.com/parts" → "https://example.com/parts";
// one with a scheme stays as it is. Empty for an empty text.
QString normalWebAddress(const QString& typed);

// The downloads' temporary folders: under MitCad's own folder in the
// system's temporary folder (MITCAD_WEB_DOWNLOADS_DIR for tests), one per
// application run, held by a lock file while that run lives.
class WebDownloadFolders {
public:
  WebDownloadFolders();
  ~WebDownloadFolders();
  WebDownloadFolders(const WebDownloadFolders&) = delete;
  WebDownloadFolders& operator=(const WebDownloadFolders&) = delete;

  // MitCad's folder of downloads.
  static QString root();
  // Removes the folders of earlier runs that no running MitCad holds (left
  // by a crash or a run that was killed); returns how many it removed.
  static int removeLeftovers();

  // This run's folder, made and locked on first use; empty when it cannot
  // be made.
  QString runFolder();
  // A new, empty folder for one download in this run's folder.
  QString newDownloadFolder();
  // Removes a download's folder (it must be inside this run's folder).
  void remove(const QString& folder);
  // Removes this run's folder with everything in it.
  void removeAll();

private:
  QString m_folder;
  std::unique_ptr<QLockFile> m_lock;
  int m_next = 0;
};

} // namespace mitcad
