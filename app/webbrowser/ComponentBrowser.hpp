// SPDX-License-Identifier: MIT
#pragma once

// Insert › Component from Web (mitcad#1579, app/COMMANDS.md "Component from
// Web"): a browser window (Qt WebEngine) with the user's own sites, whose
// downloads of the formats MitCad imports, and zips of them, come into the
// open design as components. The browser is the Chromium Embedded Framework
// (CefView). Built as the module mitcad-webbrowser of the web browser
// component where the CEF distribution is (MITCAD_CEF_ROOT,
// app/CMakeLists.txt) and loaded by the application at run time; it asks the
// application for the rest through ComponentBrowserHost.

#include <QDialog>
#include <QString>
#include <QUrl>

#include "ComponentBrowserHost.hpp"
#include "ComponentWebFiles.hpp"

class QLabel;
class QLineEdit;
class QProgressBar;
class QToolButton;

namespace mitcad {

class CefView;

class ComponentBrowser : public QDialog {
  Q_OBJECT

public:
  // `host` brings a downloaded file into the design: a Component into the
  // open design, then Move, or a Document as a design of its own, as Import
  // opens it (true while that import still reads the file).
  ComponentBrowser(ComponentBrowserHost* host, QWidget* parent);
  ~ComponentBrowser() override;

  // For the tests (TestDriver's invoke-widget): follows the first link of
  // the page whose text or address contains `text`, as a click would.
  Q_INVOKABLE void clickLink(const QString& text);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  void showStartPage();
  void openAddress(const QString& typed);
  void showSitesMenu();
  void editSites(bool addThisPage);
  void clearBrowsingData();
  // A download about to start: its path in a new temporary folder, or
  // empty when it is refused (the format filter by its name).
  QString downloadStarting(const QString& suggestedName, const QUrl& url);
  // Its progress, or its end (CefView::downloadUpdated).
  void downloadUpdated(quint32 id, qint64 received, qint64 total, int state, const QString& path,
                       const QString& reason);
  // A finished download (or a file of a zip): checked by its content, then
  // imported. False when it was refused or its import failed.
  bool importFile(const QString& path, bool* keep);
  void importZip(const QString& path, const QString& folder, bool* keep);
  void showNote(const QString& text, bool warning);
  void updateNavigation();
  // What the address field shows of a page's address (empty for the start
  // page and about:blank).
  static QString shownAddress(const QUrl& url);

  ComponentBrowserHost* m_host = nullptr;
  WebDownloadFolders m_folders;
  CefView* m_view = nullptr;
  QLineEdit* m_address = nullptr;
  // The page's address as the field shows it; the user's edit since the last
  // navigation (kept over the page's address changes); the field follows the
  // page although it has the focus (after the user's own navigation).
  QString m_pageAddress;
  bool m_addressEdited = false;
  bool m_followPage = false;
  QToolButton* m_back = nullptr;
  QToolButton* m_forward = nullptr;
  QToolButton* m_reload = nullptr;
  QToolButton* m_sites = nullptr;
  QLabel* m_note = nullptr;
  QProgressBar* m_progress = nullptr;
  // Downloads under way; a new one waits for none.
  int m_downloading = 0;
};

} // namespace mitcad
