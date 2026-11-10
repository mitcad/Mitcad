// SPDX-License-Identifier: MIT
// The web browser component's subprocess helper (mitcad#1579): CEF's
// renderer, GPU, network and utility processes run this program
// (CefSettings::browser_subprocess_path), never MitCad itself. Plain CEF, no
// Qt. On macOS it is the program of the helper apps ("MitCad Browser
// Helper.app" and its (GPU), (Renderer), (Plugin) and (Alerts) variants),
// which load CEF's framework themselves.

#include "include/cef_app.h"
#if defined(__APPLE__)
#include "include/wrapper/cef_library_loader.h"
#endif

#if defined(_WIN32)
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>

int APIENTRY wWinMain(HINSTANCE instance, HINSTANCE, LPWSTR, int) {
  CefMainArgs arguments(instance);
  return CefExecuteProcess(arguments, nullptr, nullptr);
}
#else
int main(int argc, char* argv[]) {
#if defined(__APPLE__)
  CefScopedLibraryLoader library;
  if (!library.LoadInHelper()) {
    return 1;
  }
#endif
  CefMainArgs arguments(argc, argv);
  return CefExecuteProcess(arguments, nullptr, nullptr);
}
#endif
