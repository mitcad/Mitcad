// SPDX-License-Identifier: MIT
#pragma once

// Loading the web browser component's module (mitcad#1579) from the
// component's folder, with the libraries it links (CEF's) found beside it:
// on Windows the folder is the DLL directory while it loads (SetDllDirectory;
// a folder's own DLLs are otherwise not searched for a library's imports),
// elsewhere the module's own run path ($ORIGIN, @loader_path) finds them.

#include <QString>

class QLibrary;

namespace mitcad {

// The loaded module, never unloaded; null with the reason in `error`.
QLibrary* loadBrowserModuleFile(const QString& path, QString* error);

} // namespace mitcad
