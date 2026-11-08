// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <QList>
#include <QObject>
#include <QPointer>
#include <QString>

namespace mitcad {
class MainWindow;

// One process, one DocumentSession per MainWindow. Frontends may present
// the windows separately or embed them in document frames (mitcad#1172).
class WindowManager final : public QObject {
  Q_OBJECT
public:
  explicit WindowManager(bool nativeUi = false, QObject* parent = nullptr);
  ~WindowManager() override;
  static WindowManager* instance();
  QList<MainWindow*> windows() const;
  MainWindow* activeWindow() const;
  void setActiveWindow(MainWindow* window);
  MainWindow* createWindow(bool demo = false, MainWindow* source = nullptr, bool separate = false);
  MainWindow* newWindow(MainWindow* source = nullptr);
  MainWindow* newDocument(MainWindow* source);
  // replaceAsked: the source asked its save question already, to replace
  // its design (Open in the current window); its own file is then read
  // again instead of only being brought forward.
  MainWindow* openFile(MainWindow* source, const QString& path, QString& error, bool replaceAsked = false);
  MainWindow* findByPath(const QString& path) const;
  // Opens `path` in a window of its own beside `source` (a new tab), or
  // brings forward the window that has it open already; never replaces
  // `source`'s design (Open Linked Design, mitcad#1416).
  MainWindow* openFileBeside(MainWindow* source, const QString& path, QString& error);
  // `source` saved its design to `path` (mitcad#1416): the other windows
  // whose designs link that file offer to take it.
  void designSaved(MainWindow* source, const QString& path);
  MainWindow* findByHandle(const QString& handle) const;
  MainWindow* windowForDocument(const QString& id) const;
  QString handle(MainWindow* window) const;
  void present(MainWindow* window);
  void activateWindow(MainWindow* window) { present(window); }
  bool closeWindow(MainWindow* window);
  void quit();
  // A deferred close reached a save/lock question that was cancelled.
  void cancelQuit(MainWindow* window = nullptr) {
    m_quitting = false;
    if (window != nullptr) emit closeCancelled(window);
  }
  // Called by MainWindow only after its normal save/lock close guards.
  void windowClosed(MainWindow* window);
  void setPresenter(std::function<void(MainWindow*)> presenter);
  // An untitled design nothing is made in yet, not modified and not busy:
  // New reuses its window, and new default units restart it (mitcad#1452).
  bool reusable(MainWindow* window) const;

signals:
  void windowAdded(MainWindow* window, MainWindow* source, bool separate);
  void windowRemoved(MainWindow* window);
  void closeCancelled(MainWindow* window);
  void windowsChanged();
  void activeWindowChanged(MainWindow* window);

protected:
  bool eventFilter(QObject* object, QEvent* event) override;

private:
  void installWindowMenu(MainWindow* window);
  QList<QPointer<MainWindow>> m_windows;
  QPointer<MainWindow> m_active;
  std::function<void(MainWindow*)> m_presenter;
  bool m_nativeUi = false;
  bool m_quitting = false;
  bool m_quitRunning = false;
  quint64 m_nextHandle = 1;
};
} // namespace mitcad
