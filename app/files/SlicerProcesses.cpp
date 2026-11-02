// SPDX-License-Identifier: MIT
#include "SlicerProcesses.hpp"

#include <QDir>
#include <QFileInfo>

#if defined(Q_OS_WIN)
#include <windows.h>
#include <tlhelp32.h>

#include <array>
#elif defined(Q_OS_LINUX)
#include <QByteArray>
#include <QFile>
#include <QGuiApplication>
#include <QHash>
#include <QList>
#include <QSet>
#include <QStringList>

#include <xcb/xcb.h>

#include <cstdint>
#include <cstdlib>
#include <cstring>
#elif defined(Q_OS_MACOS)
#include <libproc.h>
#include <sys/proc.h>
#include <sys/proc_info.h>

#include <cstddef>
#include <cstdint>

#include <vector>
#else
#include <signal.h>
#include <sys/types.h>
#endif

namespace mitcad {
namespace {

// Two paths of one program, as the system spells them.
[[maybe_unused]] bool sameProgram(const QString& a, const QString& b) {
  if (a.isEmpty() || b.isEmpty()) {
    return false;
  }
  const QString first = QFileInfo(a).canonicalFilePath();
  const QString second = QFileInfo(b).canonicalFilePath();
#ifdef Q_OS_WIN
  const Qt::CaseSensitivity cs = Qt::CaseInsensitive;
#else
  const Qt::CaseSensitivity cs = Qt::CaseSensitive;
#endif
  const QString cleanA = QDir::cleanPath(QDir::fromNativeSeparators(a));
  const QString cleanB = QDir::cleanPath(QDir::fromNativeSeparators(b));
  return cleanA.compare(cleanB, cs) == 0 || (!first.isEmpty() && first.compare(second, cs) == 0);
}

#if defined(Q_OS_WIN)

// A process opened to be asked about; closed with the object.
class ProcessHandle {
public:
  explicit ProcessHandle(qint64 pid)
      : m_handle(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, FALSE, static_cast<DWORD>(pid))) {}
  ~ProcessHandle() {
    if (m_handle != nullptr) {
      CloseHandle(m_handle);
    }
  }
  ProcessHandle(const ProcessHandle&) = delete;
  ProcessHandle& operator=(const ProcessHandle&) = delete;
  HANDLE get() const { return m_handle; }

private:
  HANDLE m_handle;
};

// The executable of a running process, empty when it cannot be read.
QString imageOf(qint64 pid) {
  const ProcessHandle process(pid);
  if (process.get() == nullptr) {
    return QString();
  }
  std::array<wchar_t, 4096> path{};
  DWORD size = static_cast<DWORD>(path.size());
  if (QueryFullProcessImageNameW(process.get(), 0, path.data(), &size) == 0) {
    return QString();
  }
  return QString::fromWCharArray(path.data(), static_cast<int>(size));
}

// The window looked for by raiseSlicerWindow: a visible top-level window
// of the process that no other window owns.
struct WindowSearch {
  DWORD pid = 0;
  HWND found = nullptr;
};

BOOL CALLBACK findWindow(HWND window, LPARAM data) {
  auto* search = reinterpret_cast<WindowSearch*>(data);
  DWORD owner = 0;
  GetWindowThreadProcessId(window, &owner);
  if (owner == search->pid && IsWindowVisible(window) != 0 && GetWindow(window, GW_OWNER) == nullptr) {
    search->found = window;
    return FALSE;
  }
  return TRUE;
}

#elif defined(Q_OS_LINUX)

// The fields of /proc/<pid>/stat after the command's ")": [0] the state,
// [1] the parent, [19] the start time; empty when there is no such
// process.
QList<QByteArray> statOf(qint64 pid) {
  QFile file(QStringLiteral("/proc/%1/stat").arg(pid));
  if (!file.open(QIODevice::ReadOnly)) {
    return {};
  }
  const QByteArray text = file.readAll();
  const qsizetype close = text.lastIndexOf(')');
  return close < 0 ? QList<QByteArray>() : text.mid(close + 1).simplified().split(' ');
}

// The running processes' ids.
QList<qint64> processIds() {
  QList<qint64> ids;
  for (const QString& name : QDir(QStringLiteral("/proc")).entryList(QDir::Dirs | QDir::NoDotAndDotDot)) {
    bool number = false;
    const qint64 id = name.toLongLong(&number);
    if (number && id > 0) {
      ids.append(id);
    }
  }
  return ids;
}

// Whether `pid` runs `program`: its executable, or the first two words of
// its command line (a script started through its interpreter).
bool runsProgram(qint64 pid, const QString& program) {
  const QString exe = QFileInfo(QStringLiteral("/proc/%1/exe").arg(pid)).symLinkTarget();
  if (sameProgram(exe, program)) {
    return true;
  }
  QFile file(QStringLiteral("/proc/%1/cmdline").arg(pid));
  if (!file.open(QIODevice::ReadOnly)) {
    return false;
  }
  const QList<QByteArray> words = file.readAll().split('\0');
  for (qsizetype i = 0; i < words.size() && i < 2; ++i) {
    const QString word = QString::fromLocal8Bit(words.at(i));
    if (word.startsWith(QLatin1Char('/')) && sameProgram(word, program)) {
      return true;
    }
  }
  return false;
}

// `pid` and the processes it started, theirs too.
QSet<qint64> processTree(qint64 pid) {
  QHash<qint64, QList<qint64>> children;
  for (const qint64 id : processIds()) {
    const QList<QByteArray> stat = statOf(id);
    if (stat.size() > 1) {
      children[stat.at(1).toLongLong()].append(id);
    }
  }
  QSet<qint64> tree{pid};
  QList<qint64> pending{pid};
  while (!pending.isEmpty()) {
    for (const qint64 child : children.value(pending.takeLast())) {
      if (!tree.contains(child)) {
        tree.insert(child);
        pending.append(child);
      }
    }
  }
  return tree;
}

xcb_atom_t atomOf(xcb_connection_t* connection, const char* name) {
  xcb_intern_atom_reply_t* reply = xcb_intern_atom_reply(
      connection, xcb_intern_atom(connection, 1, static_cast<std::uint16_t>(std::strlen(name)), name), nullptr);
  const xcb_atom_t atom = reply != nullptr ? reply->atom : static_cast<xcb_atom_t>(XCB_ATOM_NONE);
  std::free(reply);
  return atom;
}

#endif

} // namespace

SlicerProcess slicerProcess(qint64 pid) {
  SlicerProcess process;
  process.pid = pid;
  if (pid <= 0) {
    return process;
  }
#if defined(Q_OS_WIN)
  const ProcessHandle handle(pid);
  FILETIME created{};
  FILETIME exited{};
  FILETIME kernel{};
  FILETIME user{};
  if (handle.get() != nullptr && WaitForSingleObject(handle.get(), 0) == WAIT_TIMEOUT &&
      GetProcessTimes(handle.get(), &created, &exited, &kernel, &user) != 0) {
    process.started = (static_cast<qint64>(created.dwHighDateTime) << 32) | static_cast<qint64>(created.dwLowDateTime);
  }
#elif defined(Q_OS_LINUX)
  const QList<QByteArray> stat = statOf(pid);
  // A zombie or a dead process does not run.
  if (stat.size() > 19 && stat.at(0) != "Z" && stat.at(0) != "X") {
    process.started = stat.at(19).toLongLong();
  }
#elif defined(Q_OS_MACOS)
  struct proc_bsdinfo info {};
  if (proc_pidinfo(static_cast<int>(pid), PROC_PIDTBSDINFO, 0, &info, PROC_PIDTBSDINFO_SIZE) == PROC_PIDTBSDINFO_SIZE &&
      info.pbi_status != SZOMB) {
    process.started = static_cast<qint64>(info.pbi_start_tvsec) * 1000000 + static_cast<qint64>(info.pbi_start_tvusec);
  }
#else
  // Without the start time: the id alone.
  if (::kill(static_cast<pid_t>(pid), 0) == 0) {
    process.started = 1;
  }
#endif
  return process;
}

bool slicerProcessRunning(const SlicerProcess& process) {
  return process.isValid() && slicerProcess(process.pid).started == process.started;
}

int slicerProcessesOf(const QString& program, qint64 except) {
  if (program.isEmpty()) {
    return -1;
  }
#if defined(Q_OS_WIN)
  HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
  if (snapshot == INVALID_HANDLE_VALUE) {
    return -1;
  }
  const QString fileName = QFileInfo(program).fileName();
  int count = 0;
  PROCESSENTRY32W entry{};
  entry.dwSize = sizeof(entry);
  for (BOOL more = Process32FirstW(snapshot, &entry); more != 0; more = Process32NextW(snapshot, &entry)) {
    const qint64 pid = static_cast<qint64>(entry.th32ProcessID);
    if (pid == except || QString::fromWCharArray(entry.szExeFile).compare(fileName, Qt::CaseInsensitive) != 0) {
      continue;
    }
    // By the name alone when the path cannot be read: maybe another window.
    const QString image = imageOf(pid);
    if (image.isEmpty() || sameProgram(image, program)) {
      ++count;
    }
  }
  CloseHandle(snapshot);
  return count;
#elif defined(Q_OS_LINUX)
  int count = 0;
  for (const qint64 pid : processIds()) {
    if (pid != except && runsProgram(pid, program)) {
      const QList<QByteArray> stat = statOf(pid);
      if (!stat.isEmpty() && stat.at(0) != "Z" && stat.at(0) != "X") {
        ++count;
      }
    }
  }
  return count;
#elif defined(Q_OS_MACOS)
  const int listed = proc_listallpids(nullptr, 0);
  if (listed <= 0) {
    return -1;
  }
  std::vector<pid_t> pids(static_cast<std::size_t>(listed) + 32);
  const int found = proc_listallpids(pids.data(), static_cast<int>(pids.size() * sizeof(pid_t)));
  if (found <= 0) {
    return -1;
  }
  int count = 0;
  std::vector<char> path(PROC_PIDPATHINFO_MAXSIZE);
  for (int i = 0; i < found && i < static_cast<int>(pids.size()); ++i) {
    const pid_t pid = pids[static_cast<std::size_t>(i)];
    if (pid <= 0 || static_cast<qint64>(pid) == except) {
      continue;
    }
    const int length = proc_pidpath(pid, path.data(), static_cast<std::uint32_t>(path.size()));
    if (length > 0 && sameProgram(QString::fromUtf8(path.data(), length), program)) {
      ++count;
    }
  }
  return count;
#else
  Q_UNUSED(except);
  return -1;
#endif
}

bool raiseSlicerWindow(qint64 pid) {
  if (pid <= 0) {
    return false;
  }
#if defined(Q_OS_WIN)
  WindowSearch search;
  search.pid = static_cast<DWORD>(pid);
  EnumWindows(findWindow, reinterpret_cast<LPARAM>(&search));
  if (search.found == nullptr) {
    return false;
  }
  if (IsIconic(search.found) != 0) {
    ShowWindow(search.found, SW_RESTORE);
  }
  // MitCad is the foreground application (Send was just pressed), so it
  // may give the foreground away.
  return SetForegroundWindow(search.found) != 0;
#elif defined(Q_OS_LINUX)
  auto* x11 = qGuiApp != nullptr ? qGuiApp->nativeInterface<QNativeInterface::QX11Application>() : nullptr;
  xcb_connection_t* connection = x11 != nullptr ? x11->connection() : nullptr;
  if (connection == nullptr) {
    return false; // Wayland: no window of another application can be raised
  }
  const xcb_screen_t* screen = xcb_setup_roots_iterator(xcb_get_setup(connection)).data;
  const xcb_atom_t clientList = atomOf(connection, "_NET_CLIENT_LIST");
  const xcb_atom_t windowPid = atomOf(connection, "_NET_WM_PID");
  const xcb_atom_t activeWindow = atomOf(connection, "_NET_ACTIVE_WINDOW");
  if (screen == nullptr || clientList == XCB_ATOM_NONE || windowPid == XCB_ATOM_NONE ||
      activeWindow == XCB_ATOM_NONE) {
    return false;
  }
  // The window manager's list of the top-level windows (none without one).
  xcb_get_property_reply_t* list = xcb_get_property_reply(
      connection, xcb_get_property(connection, 0, screen->root, clientList, XCB_ATOM_WINDOW, 0, 4096), nullptr);
  if (list == nullptr) {
    return false;
  }
  const QSet<qint64> tree = processTree(pid);
  xcb_window_t found = XCB_WINDOW_NONE;
  if (list->format == 32) {
    const auto* windows = static_cast<const xcb_window_t*>(xcb_get_property_value(list));
    const int count = xcb_get_property_value_length(list) / static_cast<int>(sizeof(xcb_window_t));
    for (int i = 0; i < count && found == XCB_WINDOW_NONE; ++i) {
      xcb_get_property_reply_t* owner = xcb_get_property_reply(
          connection, xcb_get_property(connection, 0, windows[i], windowPid, XCB_ATOM_CARDINAL, 0, 1), nullptr);
      if (owner != nullptr && owner->format == 32 && xcb_get_property_value_length(owner) >= 4) {
        const std::uint32_t id = *static_cast<const std::uint32_t*>(xcb_get_property_value(owner));
        if (tree.contains(static_cast<qint64>(id))) {
          found = windows[i];
        }
      }
      std::free(owner);
    }
  }
  std::free(list);
  if (found == XCB_WINDOW_NONE) {
    return false;
  }
  // The request a pager makes (source 2), which window managers honour
  // whatever window has the focus.
  xcb_client_message_event_t event;
  std::memset(&event, 0, sizeof(event));
  event.response_type = XCB_CLIENT_MESSAGE;
  event.format = 32;
  event.window = found;
  event.type = activeWindow;
  event.data.data32[0] = 2;
  event.data.data32[1] = XCB_CURRENT_TIME;
  xcb_send_event(connection, 0, screen->root,
                 XCB_EVENT_MASK_SUBSTRUCTURE_REDIRECT | XCB_EVENT_MASK_SUBSTRUCTURE_NOTIFY,
                 reinterpret_cast<const char*>(&event));
  xcb_flush(connection);
  return true;
#else
  // macOS: an application bundle is started with `open -a`, which brings
  // the running application forward itself.
  return false;
#endif
}

} // namespace mitcad
