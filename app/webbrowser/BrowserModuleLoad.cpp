// SPDX-License-Identifier: MIT
// Loading the web browser component's module (mitcad#1579): see
// BrowserModuleLoad.hpp.
#include "BrowserModuleLoad.hpp"

#include <string>

#include <QDir>
#include <QFileInfo>
#include <QLibrary>

#ifdef Q_OS_WIN
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#endif

namespace mitcad {

QLibrary* loadBrowserModuleFile(const QString& path, QString* error) {
  auto* library = new QLibrary(path);
#ifdef Q_OS_WIN
  const std::wstring folder = QDir::toNativeSeparators(QFileInfo(path).absolutePath()).toStdWString();
  SetDllDirectoryW(folder.c_str());
#endif
  const bool loaded = library->load();
#ifdef Q_OS_WIN
  SetDllDirectoryW(nullptr);
#endif
  if (!loaded) {
    *error = library->errorString();
    delete library;
    return nullptr;
  }
  return library;
}

} // namespace mitcad
