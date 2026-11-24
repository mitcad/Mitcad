// SPDX-License-Identifier: MIT
#include "AccessibilityClients.hpp"

#include <atomic>
#include <mutex>

#include <QtGlobal>

#ifdef Q_OS_WIN
#include <algorithm>
#include <cwchar>
#include <vector>

#include <QAccessible>
#include <QAccessibleWidget>
#include <QDebug>
#include <QElapsedTimer>
#include <QFileInfo>
#include <QHash>
#include <QMainWindow>
#include <QPointer>
#include <QStringList>
#include <QThreadPool>
#include <QTimer>

#include <windows.h>

#include <objbase.h>
#include <psapi.h>

#include "AccessibleTree.hpp"
#endif

namespace mitcad {
namespace {

std::mutex g_currentMutex;
QString g_current;

#ifdef Q_OS_WIN

void setCurrent(const QString& client) {
  const std::lock_guard<std::mutex> lock(g_currentMutex);
  g_current = client;
}

// A burst ends when no call came for this long.
constexpr int kQuietMs = 250;
// A burst this long gets a line, a process's next one after kRepeatMs; a
// burst of kSlowMs always gets one.
constexpr qint64 kReadMs = 100;
constexpr qint64 kRepeatMs = 60000;
constexpr qint64 kSlowMs = 1000;
// The subtree lines of a slow burst.
constexpr int kSlowLines = 30;

struct Caller {
  DWORD pid = 0;
  QString exe;
};

QString exeOf(DWORD pid) {
  QString exe = QStringLiteral("unknown");
  HANDLE process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
  if (process != nullptr) {
    constexpr DWORD kPathSize = MAX_PATH * 2;
    wchar_t path[kPathSize] = {};
    DWORD size = kPathSize;
    if (QueryFullProcessImageNameW(process, 0, path, &size)) {
      exe = QFileInfo(QString::fromWCharArray(path, static_cast<int>(size))).fileName();
    }
    CloseHandle(process);
  }
  return exe;
}

// The process of the COM call this thread is serving; an empty exe when
// the call is from this process or no call is under way. COM tells the
// calling thread only when it is in a single-threaded apartment; a caller
// in the multithreaded apartment (most UI Automation clients) is "unknown"
// (pid 0), and no documented call tells its process.
Caller callingProcess() {
  Caller caller;
  DWORD thread = 0;
  // S_OK: the caller is in another process; S_FALSE: in this one.
  if (CoGetCallerTID(&thread) != S_OK) {
    return caller;
  }
  if (thread != 0) {
    if (HANDLE threadHandle = OpenThread(THREAD_QUERY_LIMITED_INFORMATION, FALSE, thread)) {
      caller.pid = GetProcessIdOfThread(threadHandle);
      CloseHandle(threadHandle);
    }
  }
  caller.exe = caller.pid != 0 ? exeOf(caller.pid) : QStringLiteral("unknown");
  return caller;
}

std::atomic<bool> g_listing{false};

// The other processes that have UI Automation's client library loaded, as
// "<exe> (pid N)": the ones that can be reading the window.
QStringList uiAutomationProcesses() {
  QStringList found;
  std::vector<DWORD> pids(4096);
  DWORD bytes = 0;
  if (!EnumProcesses(pids.data(), static_cast<DWORD>(pids.size() * sizeof(DWORD)), &bytes)) {
    return found;
  }
  pids.resize(bytes / sizeof(DWORD));
  const DWORD self = GetCurrentProcessId();
  // A system library is mapped at the same address in every 64-bit process
  // of a boot: the module lists are compared by this process's copy, which
  // Qt's UI Automation bridge has loaded, and by name only where that
  // cannot tell (a 32-bit process, or no copy here): reading every
  // module's name of every process took seconds.
  const HMODULE ours = GetModuleHandleW(L"UIAutomationCore.dll");
  std::vector<HMODULE> modules(2048);
  for (const DWORD pid : pids) {
    if (pid == 0 || pid == self) {
      continue;
    }
    HANDLE process = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, FALSE, pid);
    if (process == nullptr) {
      continue;
    }
    DWORD needed = 0;
    bool loaded = false;
    if (EnumProcessModulesEx(process, modules.data(), static_cast<DWORD>(modules.size() * sizeof(HMODULE)), &needed,
                             LIST_MODULES_ALL)) {
      const std::size_t count = std::min<std::size_t>(needed / sizeof(HMODULE), modules.size());
      BOOL wow64 = FALSE;
      const bool byName = ours == nullptr || (IsWow64Process(process, &wow64) && wow64);
      for (std::size_t i = 0; i < count && !loaded; ++i) {
        if (!byName) {
          loaded = modules[i] == ours;
          continue;
        }
        wchar_t name[64] = {};
        loaded = GetModuleBaseNameW(process, modules[i], name, 63) > 0 && _wcsicmp(name, L"UIAutomationCore.dll") == 0;
      }
    }
    CloseHandle(process);
    if (loaded) {
      found << QStringLiteral("%1 (pid %2)").arg(exeOf(pid)).arg(pid);
    }
  }
  return found;
}

class Bursts : public QObject {
public:
  Bursts() {
    m_quiet.setSingleShot(true);
    m_quiet.setInterval(kQuietMs);
    connect(&m_quiet, &QTimer::timeout, this, [this] { finish(); });
    m_clock.start();
    // Every burst's line, and the processes with UI Automation loaded for
    // each unknown caller (the UI test of the lines).
    m_everyBurst = qEnvironmentVariableIntValue("MITCAD_LOG_ACCESSIBILITY_CLIENTS") == 1;
  }

  // A call of a client into `window`'s interface; on the UI thread.
  void note(QWidget* window) {
    if (!m_active) {
      const Caller caller = callingProcess();
      if (caller.exe.isEmpty()) {
        return;
      }
      m_active = true;
      m_caller = caller;
      m_window = window->window();
      m_start = m_clock.elapsed();
      m_lastTurn = m_start;
      setCurrent(describe(caller));
      if (caller.pid == 0 && m_everyBurst) {
        listCandidates();
      }
    } else if (m_caller.pid == 0) {
      // A later call of the burst may come from a caller COM can tell.
      const Caller caller = callingProcess();
      if (caller.pid != 0) {
        m_caller = caller;
        setCurrent(describe(caller));
      }
    }
    if (!m_turnPending) {
      // The event loop's next turn: after the call that is being served.
      m_turnPending = true;
      QTimer::singleShot(0, this, [this] {
        m_turnPending = false;
        m_lastTurn = m_clock.elapsed();
        m_quiet.start();
      });
    }
  }

private:
  static QString describe(const Caller& caller) {
    return caller.pid != 0 ? QStringLiteral("%1 (pid %2)").arg(caller.exe).arg(caller.pid)
                           : QStringLiteral("%1 (pid unknown)").arg(caller.exe);
  }

  bool wouldLog(DWORD pid, qint64 now) const {
    const auto last = m_logged.constFind(pid);
    return last == m_logged.constEnd() || now - *last >= kRepeatMs;
  }

  // An unknown caller: the processes that can be reading, found off the
  // UI thread while the caller still waits for its answer, in a line of
  // their own; one search at a time.
  void listCandidates() {
    if (g_listing.exchange(true)) {
      return;
    }
    QThreadPool::globalInstance()->start([] {
      qInfo().noquote() << QStringLiteral("Accessibility client candidates: UI Automation loaded in %1")
                               .arg(accessibilityClientCandidates());
      g_listing = false;
    });
  }

  void finish() {
    if (!m_active) {
      return;
    }
    m_active = false;
    setCurrent(QString());
    const qint64 ms = m_lastTurn - m_start;
    const qint64 now = m_clock.elapsed();
    // A short poll (some system processes read the window every quarter of
    // a second) is no read worth a line.
    const bool read = ms >= kReadMs && wouldLog(m_caller.pid, now);
    if (!m_everyBurst && !read && ms < kSlowMs) {
      return;
    }
    m_logged.insert(m_caller.pid, now);
    const AccessibleTreeStats tree =
        m_window.isNull() ? AccessibleTreeStats{} : walkAccessibleTree(m_window.data(), false, 0.05);
    qInfo().noquote() << QStringLiteral("Accessibility client: %1: bulk read, %2 nodes, %3 ms")
                             .arg(describe(m_caller))
                             .arg(tree.nodes)
                             .arg(ms);
    // A slow read: where the window's nodes are, for the next look.
    if (ms >= kSlowMs) {
      for (int i = 0; i < std::min<int>(kSlowLines, static_cast<int>(tree.subtrees.size())); ++i) {
        qInfo().noquote() << QStringLiteral("Accessibility client read: %1").arg(tree.subtrees.at(i));
      }
    }
  }

  QElapsedTimer m_clock;
  QTimer m_quiet;
  bool m_everyBurst = false;
  bool m_active = false;
  bool m_turnPending = false;
  Caller m_caller;
  QPointer<QWidget> m_window;
  qint64 m_start = 0;
  qint64 m_lastTurn = 0;
  QHash<DWORD, qint64> m_logged; // pid (0: unknown) -> when its last line was written
};

Bursts& bursts() {
  static Bursts* instance = new Bursts;
  return *instance;
}

// The interface of the application's main windows (the top-level frame and
// the design window in it), as Qt's own for QMainWindow (a Window of the
// widget's children), noting the calls that come from other processes. A
// client's read of the window starts at the top-level one.
class WatchedWindow : public QAccessibleWidget {
public:
  explicit WatchedWindow(QWidget* window) : QAccessibleWidget(window, QAccessible::Window) {}

  int childCount() const override {
    note();
    return QAccessibleWidget::childCount();
  }
  QAccessibleInterface* child(int index) const override {
    note();
    return QAccessibleWidget::child(index);
  }
  QString text(QAccessible::Text t) const override {
    note();
    return QAccessibleWidget::text(t);
  }
  QAccessible::State state() const override {
    note();
    return QAccessibleWidget::state();
  }

private:
  void note() const { bursts().note(widget()); }
};

QAccessibleInterface* watchedWindowFactory(const QString& className, QObject* object) {
  if (object == nullptr || !object->isWidgetType() || className != QLatin1String(object->metaObject()->className()) ||
      !className.startsWith(QLatin1String("mitcad::"))) {
    return nullptr;
  }
  auto* window = qobject_cast<QMainWindow*>(object);
  return window != nullptr ? new WatchedWindow(window) : nullptr;
}

#endif

} // namespace

void installAccessibilityClientLog() {
#ifdef Q_OS_WIN
  QAccessible::installFactory(&watchedWindowFactory);
#endif
}

QString currentAccessibilityClient() {
  const std::lock_guard<std::mutex> lock(g_currentMutex);
  return g_current;
}

QString accessibilityClientCandidates() {
#ifdef Q_OS_WIN
  const QStringList processes = uiAutomationProcesses();
  return processes.isEmpty() ? QStringLiteral("no other process") : processes.join(QStringLiteral(", "));
#else
  return QString();
#endif
}

} // namespace mitcad
