// SPDX-License-Identifier: MIT
#pragma once

// Loading the web browser component's module (mitcad#1579) from the
// component's folder, with the libraries it links (CEF's) found beside it:
// on Windows the folder is the DLL directory while it loads (SetDllDirectory;
// a folder's own DLLs are otherwise not searched for a library's imports),
// elsewhere the module's own run path ($ORIGIN, @loader_path) finds them.

#include <QString>

#include "ComponentBrowserHost.hpp"

class QLibrary;

namespace mitcad {

struct BrowserComponentInfo;

// The loaded module, never unloaded; null with the reason in `error`.
QLibrary* loadBrowserModuleFile(const QString& path, QString* error);

// The browsing profile's folder: cookies, sign-ins, cache
// (MITCAD_WEB_PROFILE_DIR for tests; MainWindowComponentWeb.cpp).
QString browserProfileFolder();
// The tutorial pane's entry of the component's module (mitcad#2012), loaded
// once as Component from Web loads it, and the folder of the component it
// came from; null with the reason in `error`.
CreateTutorialPageFunction loadTutorialPageModule(const BrowserComponentInfo& component, QString* folder,
                                                  QString* error);

} // namespace mitcad
