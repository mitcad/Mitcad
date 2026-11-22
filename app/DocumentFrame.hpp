// SPDX-License-Identifier: MIT
#pragma once

#include <QMainWindow>
#include <QPointer>
#include <QString>
#include <QList>

class QStackedWidget;
class QTabBar;
class QVBoxLayout;

namespace mitcad {
class MainWindow;
class WindowManager;

// A presentation container only: each page retains its document, controllers,
// selection and undo history when it moves to another frame.
class DocumentFrame final : public QMainWindow {
  Q_OBJECT
public:
  explicit DocumentFrame(WindowManager& manager);
  ~DocumentFrame() override;
  static void install(WindowManager& manager);
  static DocumentFrame* containing(MainWindow* document);
  void addDocument(MainWindow* document, int index = -1);
  void removeDocument(MainWindow* document);
  void activateDocument(MainWindow* document);
  MainWindow* activeDocument() const;
  QList<MainWindow*> documents() const;
  void moveDocument(MainWindow* document, DocumentFrame* destination = nullptr, int index = -1);
  // Whether a frame holding this many documents shows its document tab bar
  // (mitcad#1172). Only while two or more documents share the frame: with
  // one design the frame shows that design's own menu bar and no tab bar, so
  // the window is laid out exactly as a single document window. Returning
  // true here for one document shows the tabs always.
  static bool showsDocumentTabs(int documentCount);
  // Whether the shown design's top bar title row holds the tab bar
  // (mitcad#1990, Windows and Linux, both layouts since mitcad#2082): then
  // the tabs show for one design too, and the frame's own menu bar and tab
  // row stay hidden (each design keeps its own menu bar, so the window is
  // laid out as a single document window). Elsewhere (macOS) the tabs are
  // the frame's row under its menu bar, as showsDocumentTabs says.
  bool tabsInTitleRow() const;
  // Whether this frame shows its tab bar now: never while its only document
  // has no design open (the last one closed to the Start page, mitcad#2142).
  bool showsTabs() const;
  // `document` closed its last design to the Start page or shows a design
  // again (MainWindow::closeToStartPage, mitcad#2142): its tab and the
  // frame's title follow.
  void documentShownChanged(MainWindow* document);
  // macOS: the system menu bar already shows the frame's menus, so the
  // window's own copy of them is hidden unless Preferences › Display asks
  // for it (off by default). Elsewhere the window's menu bar is the only
  // one and is always shown.
  static bool windowMenuBarShown();
  // Saves the choice and applies it to every open frame; false if unchanged.
  static bool setWindowMenuBarShown(bool shown);
protected:
  bool eventFilter(QObject* watched, QEvent* event) override;
  void closeEvent(QCloseEvent* event) override;
private:
  void updateDocument(MainWindow* document);
  void updateActiveDocument();
  void updateChrome();
  // The tab bar into `host`'s Floating title row, or back into the frame's
  // own row above the designs (null).
  void placeTabs(MainWindow* host);
  void showTabMenu(int index, const QPoint& globalPosition);
  // "Document tab shown at x,y" (the shown design's tab in its window) when
  // it changed.
  void logShownTab();
  // Closes them one after another as the window's close button does (Close
  // All and Close Others too: the last one closed ends MitCad).
  void closeDocuments(const QList<MainWindow*>& documents);
  void continueClosing();
  void scheduleClosing();
  QPointer<WindowManager> m_manager;
  QTabBar* m_tabs;
  QStackedWidget* m_stack;
  QWidget* m_root = nullptr;           // the central widget: the tabs' own row
  QVBoxLayout* m_rootLayout = nullptr; // and the designs under it
  QPointer<MainWindow> m_active;
  QList<QPointer<MainWindow>> m_pendingClose;
  bool m_continuingClose = false;
  bool m_closePosted = false;
  QString m_loggedTab; // logShownTab's last line
  QString m_loggedWidths; // and its last line of the title row's tabs
};
} // namespace mitcad
