// SPDX-License-Identifier: MIT
// mitcad-updater.exe (mitcad#9, Windows): installs an update once Mitcad
// has quit. Mitcad copies it next to the verified installer in a private
// folder of the user's temp folder (the installer replaces bin, where it
// comes from) and starts it as it ends (app/update/UpdateInstaller.hpp):
//
//   mitcad-updater.exe --wait <pid> --installer <file> --sha256 <hex>
//                      --dir <installation> --scope user|all
//                      --restart <mitcad.exe> --log <file>
//
// It waits for that process to end and checks the installer again
// (mitcad#603): it opens the file with read sharing only, hashes it
// through that handle, and runs it only when the SHA-256 is the one of the
// verified manifest, which Mitcad passes with --sha256; the handle stays
// open until the installer has ended, so the file it runs is the file it
// checked. It runs the installer silently into the installation folder
// and its scope (mitcad#118): an installation just for the current user
// with /S /CurrentUser /D=<dir>, as the user, so without a UAC prompt; one
// for all users (Program Files) with /S /AllUsers /D=<dir>, elevated (verb
// runas, UAC asks) unless this process has administrator rights already.
// Then it starts Mitcad again, also when the check or the installer failed
// or the elevation was refused: Mitcad then tells so from the log's last
// line. Win32 only, with the C++ runtime linked statically, so that it runs
// on its own in the temp folder; never elevated itself, so Mitcad starts
// again as the user.

#include <windows.h>

#include <bcrypt.h>
#include <shellapi.h>

#include <array>
#include <cstddef>
#include <cwchar>
#include <string>
#include <vector>

#include "UpdaterChecks.hpp"

namespace {

using Digest = std::array<unsigned char, mitcad::updater::kSha256Size>;

std::wstring g_log;

// Appends a line to the log (UTF-8).
void note(const std::wstring& line) {
  if (g_log.empty()) {
    return;
  }
  const std::wstring text = line + L"\r\n";
  const int size = WideCharToMultiByte(CP_UTF8, 0, text.c_str(), static_cast<int>(text.size()), nullptr, 0,
                                       nullptr, nullptr);
  std::string utf8(static_cast<std::size_t>(size), '\0');
  WideCharToMultiByte(CP_UTF8, 0, text.c_str(), static_cast<int>(text.size()), utf8.data(), size, nullptr,
                      nullptr);
  const HANDLE file = CreateFileW(g_log.c_str(), FILE_APPEND_DATA, FILE_SHARE_READ, nullptr, OPEN_ALWAYS,
                                  FILE_ATTRIBUTE_NORMAL, nullptr);
  if (file != INVALID_HANDLE_VALUE) {
    DWORD written = 0;
    WriteFile(file, utf8.data(), static_cast<DWORD>(utf8.size()), &written, nullptr);
    CloseHandle(file);
  }
}

// SHA-256 with the system's CNG provider (bcrypt.dll).
class Sha256 {
public:
  Sha256() {
    if (BCryptOpenAlgorithmProvider(&m_algorithm, BCRYPT_SHA256_ALGORITHM, nullptr, 0) < 0) {
      m_algorithm = nullptr;
    } else if (BCryptCreateHash(m_algorithm, &m_hash, nullptr, 0, nullptr, 0, 0) < 0) {
      m_hash = nullptr;
    }
  }
  ~Sha256() {
    if (m_hash != nullptr) {
      BCryptDestroyHash(m_hash);
    }
    if (m_algorithm != nullptr) {
      BCryptCloseAlgorithmProvider(m_algorithm, 0);
    }
  }
  Sha256(const Sha256&) = delete;
  Sha256& operator=(const Sha256&) = delete;

  bool add(const unsigned char* data, std::size_t size) {
    // BCryptHashData does not write to its input; its parameter is not
    // const only by its declaration.
    return m_hash != nullptr && BCryptHashData(m_hash, const_cast<PUCHAR>(data), static_cast<ULONG>(size), 0) >= 0;
  }
  bool finish(Digest& digest) {
    return m_hash != nullptr && BCryptFinishHash(m_hash, digest.data(), static_cast<ULONG>(digest.size()), 0) >= 0;
  }

private:
  BCRYPT_ALG_HANDLE m_algorithm = nullptr;
  BCRYPT_HASH_HANDLE m_hash = nullptr;
};

// The path of an open file with every link and junction resolved
// (GetFinalPathNameByHandleW), in the usual form when it fits MAX_PATH
// (the shell takes C:\... and \\server\share\... more readily than
// \\?\C:\...); empty when it cannot be read.
std::wstring finalPath(HANDLE file) {
  std::vector<wchar_t> buffer(static_cast<std::size_t>(MAX_PATH) + 1);
  for (int attempt = 0; attempt < 2; ++attempt) {
    const DWORD size = GetFinalPathNameByHandleW(file, buffer.data(), static_cast<DWORD>(buffer.size()),
                                                 FILE_NAME_NORMALIZED | VOLUME_NAME_DOS);
    if (size == 0) {
      return std::wstring();
    }
    if (size < buffer.size()) {
      const std::wstring path(buffer.data(), size);
      const std::wstring unc = L"\\\\?\\UNC\\";
      const std::wstring local = L"\\\\?\\";
      std::wstring plain = path;
      if (path.compare(0, unc.size(), unc) == 0) {
        plain = L"\\\\" + path.substr(unc.size());
      } else if (path.compare(0, local.size(), local) == 0) {
        plain = path.substr(local.size());
      }
      return plain.size() < static_cast<std::size_t>(MAX_PATH) ? plain : path;
    }
    // Too small: `size` is the length it needs, with the terminator.
    buffer.resize(static_cast<std::size_t>(size) + 1);
  }
  return std::wstring();
}

// The second check (mitcad#603): opens the installer for reading with read
// sharing only and hashes it through that handle. While the handle is open
// nobody can write to the file, rename it or delete it (neither write nor
// delete sharing), and NTFS does not rename a folder with an open file
// below it; `path` is the file's final path, with links and junctions
// resolved, so it names exactly the file that was hashed. The handle, to
// keep open until the installer has ended, or INVALID_HANDLE_VALUE with
// the reason in `failure` when the file cannot be read or is not the
// verified download.
HANDLE openChecked(const std::wstring& installer, const std::wstring& sha256, std::wstring& path,
                   std::wstring& failure) {
  const HANDLE file = CreateFileW(installer.c_str(), GENERIC_READ, FILE_SHARE_READ, nullptr, OPEN_EXISTING,
                                  FILE_ATTRIBUTE_NORMAL | FILE_FLAG_SEQUENTIAL_SCAN, nullptr);
  if (file == INVALID_HANDLE_VALUE) {
    failure = L"The installer could not be opened for its check (error " + std::to_wstring(GetLastError()) +
              L"), so it was not run";
    return INVALID_HANDLE_VALUE;
  }
  BY_HANDLE_FILE_INFORMATION information = {};
  if (GetFileType(file) != FILE_TYPE_DISK || !GetFileInformationByHandle(file, &information) ||
      (information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) != 0) {
    CloseHandle(file);
    failure = L"The installer is not a regular file, so it was not run";
    return INVALID_HANDLE_VALUE;
  }
  Sha256 hash;
  Digest digest = {};
  DWORD readError = 0;
  const bool read = mitcad::updater::hashAll(
      [file, &readError](unsigned char* buffer, std::size_t capacity) -> long long {
        DWORD got = 0;
        if (!ReadFile(file, buffer, static_cast<DWORD>(capacity), &got, nullptr)) {
          readError = GetLastError();
          return -1;
        }
        return static_cast<long long>(got);
      },
      [&hash](const unsigned char* data, std::size_t size) { return hash.add(data, size); });
  if (!read || !hash.finish(digest)) {
    CloseHandle(file);
    failure = L"The installer could not be read for its check (error " + std::to_wstring(readError) +
              L"), so it was not run";
    return INVALID_HANDLE_VALUE;
  }
  if (!mitcad::updater::digestMatches(digest.data(), digest.size(), sha256)) {
    CloseHandle(file);
    failure = L"The installer is not the verified download: its SHA-256 " +
              mitcad::updater::toHex(digest.data(), digest.size()) + L" differs from the release's " + sha256 +
              L", so it was not run";
    return INVALID_HANDLE_VALUE;
  }
  path = finalPath(file);
  if (path.empty()) {
    const DWORD error = GetLastError();
    CloseHandle(file);
    failure = L"The checked installer's path could not be read (error " + std::to_wstring(error) +
              L"), so it was not run";
    return INVALID_HANDLE_VALUE;
  }
  return file;
}

// Whether this process has administrator rights: the Administrators group
// is enabled in its token (an elevated one, or no UAC), as the installer's
// own check (UserInfo::GetAccountType) sees it.
bool isAdministrator() {
  SID_IDENTIFIER_AUTHORITY authority = SECURITY_NT_AUTHORITY;
  PSID administrators = nullptr;
  if (!AllocateAndInitializeSid(&authority, 2, SECURITY_BUILTIN_DOMAIN_RID, DOMAIN_ALIAS_RID_ADMINS, 0, 0, 0, 0, 0,
                                0, &administrators)) {
    return false;
  }
  BOOL member = FALSE;
  if (!CheckTokenMembership(nullptr, administrators, &member)) {
    member = FALSE;
  }
  FreeSid(administrators);
  return member != FALSE;
}

// Checks the installer again and runs it, and waits for it. True when it
// ran and ended with exit code 0; otherwise `outcome` says why not (each
// step is also in the log). The scope: "user", "all" or empty (the
// installer decides, as for an updater of an earlier release).
bool runInstaller(const mitcad::updater::Arguments& arguments, std::wstring& outcome) {
  std::wstring installer;
  const HANDLE checked = openChecked(arguments.installer, arguments.sha256, installer, outcome);
  if (checked == INVALID_HANDLE_VALUE) {
    note(outcome);
    return false;
  }
  note(L"Checked " + installer + L": its SHA-256 is the release's (" + arguments.sha256 +
       L"); it stays open, without write or delete sharing, until the installer has ended");

  const bool allUsers = arguments.scope == L"all";
  const bool elevate = allUsers && !isAdministrator();
  // NSIS: /D must come last and without quotes, also with spaces.
  std::wstring parameters = L"/S ";
  if (allUsers) {
    parameters += L"/AllUsers ";
  } else if (arguments.scope == L"user") {
    parameters += L"/CurrentUser ";
  }
  parameters += L"/D=" + arguments.dir;
  note(L"Running " + installer + L" " + parameters +
       (elevate ? std::wstring(L" (for all users, asking for administrator rights)")
                : allUsers ? std::wstring(L" (for all users, with administrator rights)")
                           : std::wstring(L" (as the user)")));
  // The checked path, so that the file the shell opens is the one held
  // open; with SEE_MASK_NOASYNC the call returns once the process exists.
  SHELLEXECUTEINFOW info = {};
  info.cbSize = sizeof(info);
  info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
  info.lpVerb = elevate ? L"runas" : L"open";
  info.lpFile = installer.c_str();
  info.lpParameters = parameters.c_str();
  info.nShow = SW_SHOWNORMAL;
  if (!ShellExecuteExW(&info) || info.hProcess == nullptr) {
    const DWORD error = GetLastError();
    CloseHandle(checked);
    outcome = error == ERROR_CANCELLED ? std::wstring(L"The installer was not allowed to run (elevation refused)")
                                       : L"The installer could not be started (error " + std::to_wstring(error) + L")";
    note(outcome);
    return false;
  }
  WaitForSingleObject(info.hProcess, INFINITE);
  DWORD exitCode = 1;
  GetExitCodeProcess(info.hProcess, &exitCode);
  CloseHandle(info.hProcess);
  CloseHandle(checked);
  outcome = exitCode == 0 ? std::wstring(L"The installer finished")
                          : L"The installer failed (exit code " + std::to_wstring(exitCode) + L")";
  note(outcome);
  return exitCode == 0;
}

} // namespace

int WINAPI wWinMain(HINSTANCE, HINSTANCE, PWSTR, int) {
  int count = 0;
  LPWSTR* argv = CommandLineToArgvW(GetCommandLineW(), &count);
  std::vector<std::wstring> line;
  for (int i = 0; argv != nullptr && i < count; ++i) {
    line.emplace_back(argv[i]);
  }
  LocalFree(argv);
  mitcad::updater::Arguments arguments;
  std::wstring error;
  const bool usable = mitcad::updater::parseArguments(line, arguments, error);
  g_log = arguments.log;
  if (!usable) {
    note(error);
    return 2;
  }

  if (!arguments.wait.empty()) {
    note(L"Waiting for MitCad (process " + arguments.wait + L") to end");
    const HANDLE process =
        OpenProcess(SYNCHRONIZE, FALSE, static_cast<DWORD>(std::wcstoul(arguments.wait.c_str(), nullptr, 10)));
    if (process != nullptr) {
      const DWORD waited = WaitForSingleObject(process, 120000);
      CloseHandle(process);
      if (waited != WAIT_OBJECT_0) {
        note(L"MitCad did not end within two minutes, so the update was not installed");
        return 1;
      }
    }
  }

  std::wstring outcome;
  const bool installed = runInstaller(arguments, outcome);

  // The new start reads the log's last line when the update did not
  // install: the reason goes after the line of the start, before the
  // process exists.
  note(L"Starting " + arguments.restart);
  if (!installed) {
    note(outcome);
  }
  STARTUPINFOW startup = {};
  startup.cb = sizeof(startup);
  PROCESS_INFORMATION started = {};
  std::wstring command = L"\"" + arguments.restart + L"\"";
  if (CreateProcessW(arguments.restart.c_str(), command.data(), nullptr, nullptr, FALSE, 0, nullptr,
                     arguments.dir.c_str(), &startup, &started)) {
    CloseHandle(started.hThread);
    CloseHandle(started.hProcess);
  } else {
    note(L"Could not start " + arguments.restart + L" (error " + std::to_wstring(GetLastError()) + L")");
  }
  return installed ? 0 : 1;
}
