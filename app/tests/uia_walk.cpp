// SPDX-License-Identifier: MIT
// A UI Automation client for the Windows UI test of the accessibility tree
// (mitcad#2227, tools/ui-windows-accessible-tree-test.ps1): reads the main
// window of a process as an assistive or automation program does, every
// element of it in one bulk read (FindAllBuildCache over the subtree with
// the common properties cached), and prints
//   UIA walk: <n> elements in <ms> ms
// Exit status 0 when the read succeeded, 1 on a usage error, 2 when the
// window or UI Automation was not there.
//
// Usage: mitcad-uia-walk <pid> [<window handle>] [--sta] [--hold <ms>]
//   the window: its handle in decimal (the test driver passes its own);
//          without it the process's visible top-level window titled
//          "... - MitCad <version>".
//   --sta: the client's thread in a single-threaded apartment (the default
//          is the multithreaded one, which UI Automation recommends); UI
//          Automation still calls the window from threads of its own in
//          the multithreaded apartment, so COM tells the window no caller
//          either way.
//   --hold: stays this long after the read, as a reading program lives on
//          (the window's log looks for it among the running processes).

#include <windows.h>

// COM's declarations first: the build may define WIN32_LEAN_AND_MEAN.
#include <ole2.h>

#include <UIAutomation.h>

#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cwchar>

namespace {

struct Search {
  DWORD pid = 0;
  HWND found = nullptr;
};

// The process's visible top-level window whose title ends in " - MitCad
// <version>" (the main window, as tools/ui-windows-lib.ps1 finds it).
BOOL CALLBACK findWindow(HWND hwnd, LPARAM data) {
  auto* search = reinterpret_cast<Search*>(data);
  DWORD pid = 0;
  GetWindowThreadProcessId(hwnd, &pid);
  if (pid != search->pid || !IsWindowVisible(hwnd) || GetWindow(hwnd, GW_OWNER) != nullptr) {
    return TRUE;
  }
  wchar_t title[512] = {};
  GetWindowTextW(hwnd, title, 511);
  if (std::wcsstr(title, L" - MitCad ") == nullptr) {
    return TRUE;
  }
  search->found = hwnd;
  return FALSE;
}

template <typename T> void release(T*& pointer) {
  if (pointer != nullptr) {
    pointer->Release();
    pointer = nullptr;
  }
}

} // namespace

int main(int argc, char** argv) {
  if (argc < 2) {
    std::fprintf(stderr, "usage: mitcad-uia-walk <pid> [<window handle>] [--sta] [--hold <ms>]\n");
    return 1;
  }
  const DWORD pid = static_cast<DWORD>(std::strtoul(argv[1], nullptr, 10));
  bool sta = false;
  unsigned long hold = 0;
  Search search;
  search.pid = pid;
  for (int i = 2; i < argc; ++i) {
    if (std::strcmp(argv[i], "--sta") == 0) {
      sta = true;
    } else if (std::strcmp(argv[i], "--hold") == 0 && i + 1 < argc) {
      hold = std::strtoul(argv[++i], nullptr, 10);
    } else {
      search.found = reinterpret_cast<HWND>(static_cast<std::uintptr_t>(std::strtoull(argv[i], nullptr, 10)));
    }
  }
  if (search.found == nullptr) {
    EnumWindows(findWindow, reinterpret_cast<LPARAM>(&search));
  }
  if (search.found == nullptr) {
    std::fprintf(stderr, "UIA walk: no MitCad window of process %lu\n", static_cast<unsigned long>(pid));
    return 2;
  }
  if (FAILED(CoInitializeEx(nullptr, sta ? COINIT_APARTMENTTHREADED : COINIT_MULTITHREADED))) {
    std::fprintf(stderr, "UIA walk: COM did not start\n");
    return 2;
  }
  int status = 2;
  IUIAutomation* automation = nullptr;
  IUIAutomationElement* root = nullptr;
  IUIAutomationCacheRequest* cache = nullptr;
  IUIAutomationCondition* condition = nullptr;
  IUIAutomationElementArray* elements = nullptr;
  if (SUCCEEDED(CoCreateInstance(__uuidof(CUIAutomation), nullptr, CLSCTX_INPROC_SERVER, __uuidof(IUIAutomation),
                                 reinterpret_cast<void**>(&automation))) &&
      SUCCEEDED(automation->ElementFromHandle(search.found, &root)) && root != nullptr &&
      SUCCEEDED(automation->CreateCacheRequest(&cache)) && SUCCEEDED(automation->CreateTrueCondition(&condition))) {
    const PROPERTYID properties[] = {UIA_NamePropertyId,         UIA_ControlTypePropertyId,
                                     UIA_AutomationIdPropertyId, UIA_ClassNamePropertyId,
                                     UIA_BoundingRectanglePropertyId, UIA_IsOffscreenPropertyId,
                                     UIA_IsEnabledPropertyId,    UIA_HelpTextPropertyId};
    for (const PROPERTYID property : properties) {
      cache->AddProperty(property);
    }
    cache->put_TreeFilter(condition);
    const auto start = std::chrono::steady_clock::now();
    const HRESULT read = root->FindAllBuildCache(TreeScope_Subtree, condition, cache, &elements);
    const auto ms =
        std::chrono::duration_cast<std::chrono::milliseconds>(std::chrono::steady_clock::now() - start).count();
    int count = 0;
    if (SUCCEEDED(read) && elements != nullptr && SUCCEEDED(elements->get_Length(&count))) {
      std::printf("UIA walk: %d elements in %lld ms\n", count, static_cast<long long>(ms));
      status = 0;
    } else {
      std::fprintf(stderr, "UIA walk: the read failed (0x%08lx)\n", static_cast<unsigned long>(read));
    }
  } else {
    std::fprintf(stderr, "UIA walk: UI Automation or the window's element is not there\n");
  }
  std::fflush(stdout);
  // With UI Automation still loaded: CoUninitialize unloads it.
  Sleep(static_cast<DWORD>(hold));
  release(elements);
  release(condition);
  release(cache);
  release(root);
  release(automation);
  CoUninitialize();
  return status;
}
