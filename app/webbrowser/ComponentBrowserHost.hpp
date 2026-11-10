// SPDX-License-Identifier: MIT
#pragma once

// Insert › Component from Web (mitcad#1579): what the component browser's
// module (mitcad-webbrowser, the only part of MitCad on the Chromium
// Embedded Framework, CEF) and the application give each other. The module
// comes with CEF's runtime in the web browser component, a package of its
// own that MitCad downloads on first use (BrowserComponent.hpp) and loads
// at run time. The module links Qt and CEF and nothing of the application;
// what it needs of MitCad (the import, the bridge's zip reader, MitCad's
// dialogs, menus, window sizes and colors, its folders) it asks of the host
// the application hands it. mitcad itself links nothing of CEF and starts
// without the component.

#include <QColor>
#include <QPoint>
#include <QSize>
#include <QString>

#include "ComponentWebFiles.hpp"

class QAction;
class QDialog;
class QMenu;
class QPalette;
class QWidget;

namespace mitcad {

// The theme's colors the browser shows (Theme.hpp's roles).
struct ComponentBrowserColors {
  QColor window;
  QColor text;
  QColor secondary;
  QColor link;
  QColor warning;
};

class ComponentBrowserHost {
public:
  virtual ~ComponentBrowserHost() = default;
  // Brings a downloaded file into the design (MainWindow::importWebDownload):
  // false with the reason in `error` when it did not.
  virtual bool importDownload(const QString& path, WebFileKind kind, QString* error) = 0;
  // A zip's files as JSON [{"name", "size"}] (the bridge's zip_list); empty
  // with the reason in `error` when it cannot be read.
  virtual QString zipList(const QString& zip, QString* error) = 0;
  // Writes the zip's file `name` to `target` (the bridge's zip_extract).
  virtual bool zipExtract(const QString& zip, const QString& name, const QString& target, QString* error) = 0;
  // MitCad's modal dialog (prepareModal, execDialog), menu (execMenu) and
  // Yes/No question (sheetQuestion: true for Yes).
  virtual int execDialog(QDialog* dialog) = 0;
  virtual QAction* execMenu(QMenu* menu, const QPoint& at) = 0;
  virtual bool ask(QWidget* parent, const QString& title, const QString& text) = 0;
  // fitWindowToContents with a preferred size.
  virtual void fitWindow(QWidget* window, const QSize& preferred) = 0;
  virtual ComponentBrowserColors colors(const QPalette& palette) = 0;
  // The installed component's folder (the module, CEF's runtime, its
  // subprocess helper), and the browsing profile's folder (cookies,
  // sign-ins, cache; MITCAD_WEB_PROFILE_DIR for tests).
  virtual QString componentFolder() = 0;
  virtual QString profileFolder() = 0;
};

// The module's entry points (C linkage, resolved by name): its interface's
// version, which must be kComponentBrowserAbi, and the window, a
// ComponentBrowser that asks `host` (the caller keeps `host` alive while
// the window lives). The interface's version is also the component's
// (component.json's "abi", the update manifest's browser packages): a
// MitCad loads only a component of its own interface. Raise it with every
// change of this file. (CMake reads it from here: keep the line's form.)
constexpr int kComponentBrowserAbi = 2;
using ComponentBrowserAbiFunction = int (*)();
using CreateComponentBrowserFunction = QDialog* (*)(ComponentBrowserHost* host, QWidget* parent);
inline constexpr char kComponentBrowserAbiSymbol[] = "mitcad_component_browser_abi";
inline constexpr char kCreateComponentBrowserSymbol[] = "mitcad_create_component_browser";

} // namespace mitcad
