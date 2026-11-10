// SPDX-License-Identifier: MIT
#pragma once

// The programs that run from an installation folder (mitcad#1454): a
// program from bin keeps its executable and every DLL it loaded locked, so
// that an update cannot replace them. An MCP bridge (`mitcad-cli mcp
// --connect`, started by an AI assistant) loads all of OCCT's DLLs and
// once kept them through five updates, which left the new mitcad.exe with
// the old OCCT. The updater (UpdaterWindows.cpp) asks the bridges to end
// and names what still runs when the installer refuses to replace files in
// use; the start-up check (InstallationCheck.cpp) names them when it finds
// such a mixed installation; the bridge (tools/cli/mcp-connect.cpp) waits
// for the request. The installer (packaging/windows/NSIS.template.in)
// does the same with the same event names. What still runs after that the
// updater offers to end (mitcad#1584): the helpers directly, MitCad itself
// by asking its windows to close.
//
// Windows only, plain Win32 and C++17, so that mitcad-updater.exe (no Qt,
// its own C++ runtime) uses it too.

#ifdef _WIN32

#include <windows.h>

#include <tlhelp32.h>

#include <cstddef>
#include <map>
#include <string>
#include <utility>
#include <vector>

namespace mitcad::installation {

// The event a bridge waits on (a manual-reset event in the session's Local
// namespace, made by the bridge, mitcad#1454): whoever sets it asks bridge
// `pid` to end. The installer opens it by the same name.
inline std::wstring bridgeQuitEventName(DWORD pid) { return L"Local\\MitcadBridgeQuit-" + std::to_wstring(pid); }

struct RunningProgram {
  DWORD pid = 0;
  std::wstring path; // the program's full path
};

// `folder` with backslashes and one at its end.
inline std::wstring folderPrefix(std::wstring folder) {
  for (wchar_t& c : folder) {
    if (c == L'/') {
      c = L'\\';
    }
  }
  if (folder.empty() || folder.back() != L'\\') {
    folder += L'\\';
  }
  return folder;
}

// Whether `path` is in `prefix` (folderPrefix) or below it, ignoring case
// as the file system does.
inline bool isBelow(const std::wstring& path, const std::wstring& prefix) {
  if (path.size() <= prefix.size()) {
    return false;
  }
  return CompareStringOrdinal(path.c_str(), static_cast<int>(prefix.size()), prefix.c_str(),
                              static_cast<int>(prefix.size()), TRUE) == CSTR_EQUAL;
}

// The file name of a program's path: "mitcad-cli.exe".
inline std::wstring programName(const std::wstring& path) {
  const std::size_t slash = path.find_last_of(L"\\/");
  return slash == std::wstring::npos ? path : path.substr(slash + 1);
}

// Whether `program` runs the file `name` ("mitcad.exe"), ignoring case.
inline bool runs(const RunningProgram& program, const wchar_t* name) {
  const std::wstring file = programName(program.path);
  return CompareStringOrdinal(file.c_str(), static_cast<int>(file.size()), name, -1, TRUE) == CSTR_EQUAL;
}

// Every process whose program is in `folder` or below it, except process
// `except` (this one by default). A process this user may not query (a
// protected one, another user's without administrator rights) is not seen,
// nor one that has ended while a handle keeps it known (mitcad#1584: an
// ended program has let its files go).
inline std::vector<RunningProgram> runningPrograms(const std::wstring& folder, DWORD except = GetCurrentProcessId()) {
  std::vector<RunningProgram> programs;
  const std::wstring prefix = folderPrefix(folder);
  const HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
  if (snapshot == INVALID_HANDLE_VALUE) {
    return programs;
  }
  PROCESSENTRY32W entry = {};
  entry.dwSize = sizeof(entry);
  for (BOOL more = Process32FirstW(snapshot, &entry); more; more = Process32NextW(snapshot, &entry)) {
    if (entry.th32ProcessID == 0 || entry.th32ProcessID == except) {
      continue;
    }
    const HANDLE process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, entry.th32ProcessID);
    if (process == nullptr) {
      continue;
    }
    DWORD exitCode = 0;
    if (GetExitCodeProcess(process, &exitCode) && exitCode != STILL_ACTIVE) {
      CloseHandle(process);
      continue;
    }
    std::vector<wchar_t> buffer(32768);
    DWORD size = static_cast<DWORD>(buffer.size());
    if (QueryFullProcessImageNameW(process, 0, buffer.data(), &size)) {
      std::wstring path(buffer.data(), size);
      if (isBelow(path, prefix)) {
        programs.push_back({entry.th32ProcessID, std::move(path)});
      }
    }
    CloseHandle(process);
  }
  CloseHandle(snapshot);
  return programs;
}

// Asks every bridge among `programs` to end (bridgeQuitEventName); other
// programs have no such event and are left alone. The number asked.
inline int askBridgesToEnd(const std::vector<RunningProgram>& programs) {
  int asked = 0;
  for (const RunningProgram& program : programs) {
    const HANDLE event = OpenEventW(EVENT_MODIFY_STATE, FALSE, bridgeQuitEventName(program.pid).c_str());
    if (event != nullptr) {
      if (SetEvent(event)) {
        ++asked;
      }
      CloseHandle(event);
    }
  }
  return asked;
}

// The programs by file name, "mitcad-cli.exe (4 processes), mitcad.exe".
inline std::wstring describePrograms(const std::vector<RunningProgram>& programs) {
  std::map<std::wstring, int> counts;
  for (const RunningProgram& program : programs) {
    ++counts[programName(program.path)];
  }
  std::wstring text;
  for (const auto& [name, count] : counts) {
    if (!text.empty()) {
      text += L", ";
    }
    text += name;
    if (count > 1) {
      text += L" (" + std::to_wstring(count) + L" processes)";
    }
  }
  return text;
}

} // namespace mitcad::installation

#endif // _WIN32
