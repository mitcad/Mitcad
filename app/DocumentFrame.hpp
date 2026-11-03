// SPDX-License-Identifier: MIT
#pragma once

#include <QMainWindow>
#include <QPointer>
#include <QList>

class QStackedWidget;
class QTabBar;

namespace mitcad {
class MainWindow;
class WindowManager;

// A presentation container only: each page retains its document, controllers,
// selection and undo history when it moves to another frame.
class DocumentFrame final : public QMainWindow {
  Q_OBJECT
public:
  explicit DocumentFrame(WindowManager& manager);
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
protected:
  bool eventFilter(QObject* watched, QEvent* event) override;
  void closeEvent(QCloseEvent* event) override;
private:
  void updateDocument(MainWindow* document);
  void updateActiveDocument();
  void updateChrome();
  void showTabMenu(int index, const QPoint& globalPosition);
  void closeDocuments(const QList<MainWindow*>& documents);
  void continueClosing();
  void scheduleClosing();
  QPointer<WindowManager> m_manager;
  QTabBar* m_tabs;
  QStackedWidget* m_stack;
  QPointer<MainWindow> m_active;
  QList<QPointer<MainWindow>> m_pendingClose;
  bool m_continuingClose = false;
  bool m_closePosted = false;
};
} // namespace mitcad
