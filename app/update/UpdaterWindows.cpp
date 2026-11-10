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
// Before that it asks the MCP bridges that run from the installation to
// end and waits a few seconds (mitcad#1454). Whatever from the installation
// still runs it names in a question, End Them and Install or Cancel
// (mitcad#1584), once the installer has passed its check: on End Them and
// Install it asks MitCad (another mitcad.exe from bin) to close as quitting
// it does, one design window at a time, so that MitCad asks about unsaved
// changes; a MitCad still running after that is ended only when the person
// chooses End Anyway. The other programs from bin of this session (the
// bridges, render workers) it ends, and it runs the installer with
// /EndPrograms, which ends the rest the same way with the installer's own
// rights (elevated for all users). Every step is a line of the log. On
// Cancel it runs no installer. An installer that still finds programs from
// the installation running replaces nothing and ends with exit code 3, and
// the log's last line names those programs and asks to close them and try
// again. Then it starts Mitcad again, also when the
// check or the installer failed or the elevation was refused: Mitcad then
// tells so from the log's last line. Win32 only, with the C++ runtime
// linked statically, so that it runs on its own in the temp folder; never
// elevated itself, so Mitcad starts again as the user.

#include <windows.h>

#include <bcrypt.h>
#include <commctrl.h>
#include <shellapi.h>

#include <array>
#include <cstddef>
#include <cwchar>
#include <map>
#include <string>
#include <vector>

#include "RunningPrograms.hpp"
#include "UpdaterChecks.hpp"

#ifdef _MSC_VER
// The common controls 6, which have the task dialog of the question
// (mitcad#1584); the linker writes the dependency into the manifest. It is
// loaded at run time (ask()), so that the updater starts without it too.
#pragma comment(linker, "/manifestdependency:\"type='win32' name='Microsoft.Windows.Common-Controls' "  \
                        "version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' " \
                        "language='*'\"")
#endif

namespace {

// The installer's exit code when programs from the installation still run
// (mitcad#1454, packaging/windows/NSIS.template.in): it changed no file.
constexpr DWORD kInstallerFilesInUse = 3;

// The answers of the questions about programs still running (mitcad#1584):
// the ids of the task dialog's buttons, which the installer test
// (tools/installer-windows-test.ps1) clicks with TDM_CLICK_BUTTON; Cancel
// is IDCANCEL.
constexpr int kEndThemAndInstall = 101;
constexpr int kEndAnyway = 102;
constexpr int kWait = 103;

// How long MitCad, asked to close, gets before the updater asks again.
constexpr int kCloseSeconds = 30;

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

// A choice of a question besides Cancel: its button's id and label (an &
// before its access key).
struct Choice {
  int id;
  const wchar_t* label;
};

// `label` without the & of its access key.
std::wstring plainLabel(const wchar_t* label) {
  std::wstring text;
  for (const wchar_t* c = label; *c != L'\0'; ++c) {
    if (*c != L'&') {
      text += *c;
    }
  }
  return text;
}

using TaskDialogIndirectFunction = HRESULT(WINAPI*)(const TASKDIALOGCONFIG*, int*, int*, BOOL*);

// Asks the person (mitcad#1584): a task dialog titled "MitCad Update" with
// the choices and Cancel, `preselected` the default button. Without the
// common controls 6 (no task dialog), a message box: OK for one choice, Yes
// and No for two. The id of the choice, IDCANCEL for Cancel. The question
// and the answer are lines of the log; the question's line says which
// window asked ("(task dialog)" or "(message box)"), for the installer test.
int ask(const std::wstring& instruction, const std::wstring& content, const std::vector<Choice>& choices,
        int preselected) {
  std::wstring offered;
  for (const Choice& choice : choices) {
    offered += plainLabel(choice.label) + L", ";
  }
  offered += L"Cancel";
  // The system's comctl32, through the manifest's dependency the common
  // controls 6; never one from the updater's folder.
  const HMODULE controls = LoadLibraryExW(L"comctl32.dll", nullptr, LOAD_LIBRARY_SEARCH_SYSTEM32);
  const auto taskDialog =
      controls == nullptr
          ? nullptr
          : reinterpret_cast<TaskDialogIndirectFunction>(GetProcAddress(controls, "TaskDialogIndirect"));
  int answer = IDCANCEL;
  bool asked = false;
  if (taskDialog != nullptr) {
    std::vector<TASKDIALOG_BUTTON> buttons;
    for (const Choice& choice : choices) {
      buttons.push_back({choice.id, choice.label});
    }
    TASKDIALOGCONFIG config = {};
    config.cbSize = sizeof(config);
    config.dwFlags = TDF_ALLOW_DIALOG_CANCELLATION | TDF_SIZE_TO_CONTENT;
    config.dwCommonButtons = TDCBF_CANCEL_BUTTON;
    config.pszWindowTitle = L"MitCad Update";
    config.pszMainIcon = TD_WARNING_ICON;
    config.pszMainInstruction = instruction.c_str();
    config.pszContent = content.c_str();
    config.cButtons = static_cast<UINT>(buttons.size());
    config.pButtons = buttons.data();
    config.nDefaultButton = preselected;
    note(L"Asking (task dialog): " + instruction + L" [" + offered + L"]");
    int pressed = IDCANCEL;
    const HRESULT result = taskDialog(&config, &pressed, nullptr, nullptr);
    if (SUCCEEDED(result)) {
      answer = pressed;
      asked = true;
    } else {
      note(L"The task dialog could not be shown (error " + std::to_wstring(static_cast<long>(result)) + L")");
    }
  }
  if (!asked) {
    std::wstring text = instruction + L"\n\n" + content + L"\n\n";
    UINT type = MB_ICONWARNING | MB_SETFOREGROUND;
    if (choices.size() == 1) {
      text += L"OK: " + plainLabel(choices[0].label) + L".";
      type |= MB_OKCANCEL | (preselected == IDCANCEL ? MB_DEFBUTTON2 : MB_DEFBUTTON1);
    } else {
      text += L"Yes: " + plainLabel(choices[0].label) + L". No: " + plainLabel(choices[1].label) + L".";
      type |= MB_YESNOCANCEL |
              (preselected == choices[0].id ? MB_DEFBUTTON1 : preselected == choices[1].id ? MB_DEFBUTTON2 : MB_DEFBUTTON3);
    }
    note(L"Asking (message box): " + instruction + L" [" + offered + L"]");
    const int pressed = MessageBoxW(nullptr, text.c_str(), L"MitCad Update", type);
    if (pressed == IDOK || pressed == IDYES) {
      answer = choices[0].id;
    } else if (pressed == IDNO && choices.size() > 1) {
      answer = choices[1].id;
    }
  }
  if (controls != nullptr) {
    FreeLibrary(controls);
  }
  std::wstring chosen = L"Cancel";
  for (const Choice& choice : choices) {
    if (choice.id == answer) {
      chosen = plainLabel(choice.label);
    }
  }
  note(L"Answer: " + chosen);
  return answer;
}

// The session this updater runs in: the programs it may end are its own
// session's (the person's who was asked).
DWORD currentSession() {
  DWORD session = 0;
  if (!ProcessIdToSessionId(GetCurrentProcessId(), &session)) {
    session = 0xffffffff;
  }
  return session;
}

bool inSession(DWORD pid, DWORD session) {
  DWORD its = 0;
  return ProcessIdToSessionId(pid, &its) && its == session;
}

// " (process 1234)".
std::wstring processText(DWORD pid) { return L" (process " + std::to_wstring(pid) + L")"; }

// Ends process `pid` (TerminateProcess) and waits up to five seconds until
// it has; the reason in `failure` when it could not.
bool endProcess(DWORD pid, std::wstring& failure) {
  const HANDLE process = OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, FALSE, pid);
  if (process == nullptr) {
    failure = L"error " + std::to_wstring(GetLastError());
    return false;
  }
  const bool ended = TerminateProcess(process, 1) != FALSE;
  if (ended) {
    WaitForSingleObject(process, 5000);
  } else {
    failure = L"error " + std::to_wstring(GetLastError());
  }
  CloseHandle(process);
  return ended;
}

// A design window of MitCad process `pid`: a window without an owner
// titled "<design> - MitCad <version>" (MainWindow::updateTitle), a
// visible one first; nullptr when it has none. Its hidden helper windows
// (the 3D view's offscreen surfaces, titled "MitCad") are never closed.
HWND designWindow(DWORD pid, std::wstring& title) {
  struct Search {
    DWORD pid;
    HWND visible;
    HWND hidden;
  } search = {pid, nullptr, nullptr};
  EnumWindows(
      [](HWND window, LPARAM data) -> BOOL {
        Search& found = *reinterpret_cast<Search*>(data);
        DWORD owner = 0;
        GetWindowThreadProcessId(window, &owner);
        if (owner != found.pid || GetWindow(window, GW_OWNER) != nullptr) {
          return TRUE;
        }
        std::array<wchar_t, 512> text = {};
        GetWindowTextW(window, text.data(), static_cast<int>(text.size()));
        if (std::wcsstr(text.data(), L" - MitCad ") == nullptr) {
          return TRUE;
        }
        if (IsWindowVisible(window)) {
          found.visible = window;
          return FALSE;
        }
        if (found.hidden == nullptr) {
          found.hidden = window;
        }
        return TRUE;
      },
      reinterpret_cast<LPARAM>(&search));
  const HWND window = search.visible != nullptr ? search.visible : search.hidden;
  if (window != nullptr) {
    std::array<wchar_t, 512> text = {};
    GetWindowTextW(window, text.data(), static_cast<int>(text.size()));
    title = text.data();
  }
  return window;
}

// The MitCad processes (mitcad.exe) from `bin` of `session`.
std::vector<mitcad::installation::RunningProgram> mitcadProcesses(const std::wstring& dir, const std::wstring& bin,
                                                                   DWORD session) {
  std::vector<mitcad::installation::RunningProgram> found;
  for (const mitcad::installation::RunningProgram& program : mitcad::installation::runningPrograms(dir)) {
    if (mitcad::installation::isBelow(program.path, bin) && mitcad::installation::runs(program, L"mitcad.exe") &&
        inSession(program.pid, session)) {
      found.push_back(program);
    }
  }
  return found;
}

// Asks the MitCad processes from `bin` of `session` to close as quitting
// them does (mitcad#1584): WM_CLOSE to one design window at a time, so
// that MitCad asks about the design's unsaved changes; the next window once
// that one is gone. A MitCad still running after kCloseSeconds is named
// in a question: End Anyway ends it (its unsaved changes are lost), Wait
// gives it as long again, Cancel gives up. True when no MitCad of the
// session runs any more; false with the reason in `outcome` on Cancel.
bool closeMitcad(const std::wstring& dir, const std::wstring& bin, DWORD session, std::wstring& outcome) {
  std::map<DWORD, HWND> asked;
  for (;;) {
    std::vector<mitcad::installation::RunningProgram> apps;
    for (int tick = 0; tick < 4 * kCloseSeconds; ++tick) {
      apps = mitcadProcesses(dir, bin, session);
      if (apps.empty()) {
        return true;
      }
      for (const mitcad::installation::RunningProgram& app : apps) {
        HWND& window = asked[app.pid];
        if (window != nullptr && IsWindow(window)) {
          continue;
        }
        std::wstring title;
        window = designWindow(app.pid, title);
        if (window != nullptr) {
          PostMessageW(window, WM_CLOSE, 0, 0);
          note(L"Asked MitCad" + processText(app.pid) + L" to close its window \"" + title + L"\"");
        }
      }
      Sleep(250);
    }
    const std::wstring running = mitcad::installation::describePrograms(apps);
    note(L"MitCad still runs: " + running);
    const int answer =
        ask(L"MitCad is still running",
            L"MitCad was asked to close, as when you quit it, but it is still running. It may be asking whether to "
            L"save the changes to a design: answer it there, then choose Wait.\n\nEnd Anyway ends MitCad without "
            L"saving: changes it has not saved are lost.",
            {{kEndAnyway, L"End &Anyway"}, {kWait, L"&Wait"}}, kWait);
    if (answer == kWait) {
      continue;
    }
    if (answer != kEndAnyway) {
      outcome = L"programs from the installation are still running (" + running +
                L") and ending them was cancelled, so no file was replaced. Close them, then choose Try Again";
      return false;
    }
    for (const mitcad::installation::RunningProgram& app : mitcadProcesses(dir, bin, session)) {
      std::wstring failure;
      if (endProcess(app.pid, failure)) {
        note(L"Ended MitCad" + processText(app.pid) + L"; its unsaved changes are lost");
      } else {
        note(L"Could not end MitCad" + processText(app.pid) + L" (" + failure + L")");
      }
    }
    return true;
  }
}

// What still runs from the installation once the MCP bridges were asked to
// end (askProgramsToEnd), named in a question (mitcad#1584): End Them and
// Install or Cancel. On End Them and Install, MitCad is asked to close
// (closeMitcad) and the other programs from bin of this session are ended;
// true, and the installer gets /EndPrograms for what this updater could not
// end. False with the reason in `outcome` on Cancel. `ending` is true when
// anything ran.
bool offerToEndPrograms(const std::wstring& dir, bool& ending, std::wstring& outcome) {
  using mitcad::installation::RunningProgram;
  ending = false;
  const std::vector<RunningProgram> running = mitcad::installation::runningPrograms(dir);
  if (running.empty()) {
    return true;
  }
  ending = true;
  const std::wstring bin = mitcad::installation::folderPrefix(mitcad::installation::folderPrefix(dir) + L"bin");
  const DWORD session = currentSession();
  std::vector<std::wstring> names;
  bool appRuns = false;
  for (const RunningProgram& program : running) {
    names.push_back(mitcad::installation::programName(program.path));
    appRuns = appRuns || mitcad::installation::runs(program, L"mitcad.exe");
  }
  std::wstring list;
  for (const std::wstring& line : mitcad::updater::describeForPerson(names)) {
    list += L"\x2022 " + line + L"\n";
  }
  std::wstring content = L"These programs from the installation in " + dir + L" are still running:\n\n" + list +
                         L"\nThe update replaces no file while they run. End Them and Install ends them and "
                         L"installs the update.";
  if (appRuns) {
    content += L" MitCad is asked to close first, as when you quit it, so that you can save your changes.";
  }
  if (ask(L"Programs from MitCad are still running", content, {{kEndThemAndInstall, L"&End Them and Install"}},
          kEndThemAndInstall) != kEndThemAndInstall) {
    outcome = L"programs from the installation are still running (" +
              mitcad::installation::describePrograms(running) +
              L") and ending them was cancelled, so no file was replaced. Close them, then choose Try Again";
    return false;
  }
  if (!closeMitcad(dir, bin, session, outcome)) {
    return false;
  }
  for (const RunningProgram& program : mitcad::installation::runningPrograms(dir)) {
    const std::wstring name = mitcad::installation::programName(program.path) + processText(program.pid);
    if (!mitcad::installation::isBelow(program.path, bin)) {
      note(L"Not ended (not in bin): " + program.path + processText(program.pid));
    } else if (mitcad::installation::runs(program, L"mitcad.exe")) {
      // Another session's, or one End Anyway could not end: never here.
      note(L"Not ended (MitCad): " + name);
    } else if (!inSession(program.pid, session)) {
      note(L"Not ended (another session): " + name);
    } else {
      std::wstring failure;
      if (endProcess(program.pid, failure)) {
        note(L"Ended " + name);
      } else {
        note(L"Could not end " + name + L" (" + failure + L")");
      }
    }
  }
  const std::vector<RunningProgram> remaining = mitcad::installation::runningPrograms(dir);
  if (!remaining.empty()) {
    note(L"Still running from the installation: " + mitcad::installation::describePrograms(remaining) +
         L"; the installer ends what it can of them (/EndPrograms)");
  }
  return true;
}

// Checks the installer again, offers to end what still runs from the
// installation (offerToEndPrograms), runs the installer and waits for it.
// True when it ran and ended with exit code 0; otherwise `outcome` says why
// not (each step is also in the log). The scope: "user", "all" or empty
// (the installer decides, as for an updater of an earlier release).
bool runInstaller(const mitcad::updater::Arguments& arguments, std::wstring& outcome) {
  std::wstring installer;
  const HANDLE checked = openChecked(arguments.installer, arguments.sha256, installer, outcome);
  if (checked == INVALID_HANDLE_VALUE) {
    note(outcome);
    return false;
  }
  note(L"Checked " + installer + L": its SHA-256 is the release's (" + arguments.sha256 +
       L"); it stays open, without write or delete sharing, until the installer has ended");

  // Only an installer that will run is worth ending programs for.
  bool ending = false;
  if (!offerToEndPrograms(arguments.dir, ending, outcome)) {
    CloseHandle(checked);
    note(outcome);
    return false;
  }

  const bool allUsers = arguments.scope == L"all";
  const bool elevate = allUsers && !isAdministrator();
  // NSIS: /D must come last and without quotes, also with spaces.
  std::wstring parameters = L"/S ";
  if (allUsers) {
    parameters += L"/AllUsers ";
  } else if (arguments.scope == L"user") {
    parameters += L"/CurrentUser ";
  }
  if (ending) {
    // The person chose End Them and Install (mitcad#1584): the installer
    // ends what still runs from bin, all but MitCad, with its own rights.
    parameters += L"/EndPrograms ";
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
  if (exitCode == kInstallerFilesInUse) {
    // The installer replaced nothing (mitcad#1454): what still runs, by
    // name, as the next start reports it after "The update to MitCad X.Y.Z
    // was not installed: ".
    const std::wstring running =
        mitcad::installation::describePrograms(mitcad::installation::runningPrograms(arguments.dir));
    outcome = L"programs from the installation are still running" +
              (running.empty() ? std::wstring() : L" (" + running + L")") +
              L", so no file was replaced. Close them (AI assistant sessions connected to MitCad run "
              L"mitcad-cli.exe; Task Manager can end them), then choose Try Again";
    note(outcome);
    return false;
  }
  outcome = exitCode == 0 ? std::wstring(L"The installer finished")
                          : L"The installer failed (exit code " + std::to_wstring(exitCode) + L")";
  note(outcome);
  return exitCode == 0;
}

// Asks the MCP bridges that run from the installation to end, and gives
// them and whatever else runs from it a few seconds (mitcad#1454): the
// installer replaces no file while a program from the installation runs.
void askProgramsToEnd(const std::wstring& dir) {
  std::vector<mitcad::installation::RunningProgram> running = mitcad::installation::runningPrograms(dir);
  if (running.empty()) {
    return;
  }
  const int asked = mitcad::installation::askBridgesToEnd(running);
  note(L"Running from the installation: " + mitcad::installation::describePrograms(running) + L"; asked " +
       std::to_wstring(asked) + L" MCP bridge(s) to end");
  for (int waited = 0; waited < 20 && !running.empty(); ++waited) {
    Sleep(250);
    running = mitcad::installation::runningPrograms(dir);
  }
  if (!running.empty()) {
    note(L"Still running from the installation: " + mitcad::installation::describePrograms(running));
  }
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

  // MitCad has quit for the update: the bridges it served end too
  // (mitcad#1454), so that the installer can replace their files.
  askProgramsToEnd(arguments.dir);
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
