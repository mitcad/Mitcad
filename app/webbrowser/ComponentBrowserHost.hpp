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
// without the component. The same module shows the in-app tutorials' pages
// (mitcad#2012, TutorialPage below).

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

// What became of a downloaded file the host was given (interface 4,
// mitcad#2281).
enum class WebImportResult {
  Imported, // in the design (or opened as a design of its own)
  Waiting,  // the host keeps a copy of its own until the open command closes
  Failed,   // not brought in
};

class ComponentBrowserHost {
public:
  virtual ~ComponentBrowserHost() = default;
  // Brings a downloaded file into the design (MainWindow::importWebDownload):
  // Imported; Waiting with the note to show under the page (the file comes
  // in when the open command closes: the host has taken its own copy, the
  // download's file may go); Failed with the reason in `note`. While a file
  // waits, and when it comes in or is dropped later, the host sets the
  // window's note through its invokable `showHostNote(QString note, bool
  // warning)` (ComponentBrowser).
  virtual WebImportResult importDownload(const QString& path, WebFileKind kind, QString* note) = 0;
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

// The tutorial pane's page (mitcad#2012, docs/tutorials.md): a CEF view of
// an installed tutorial package's local pages, docked in the design's window
// (tutorials/MainWindowTutorials.cpp). The bridge `window.mitcad` is given
// to the pages of that package's folder only (the folder given at its
// creation), never to other pages; its calls come to the host, and the
// guide's events go to the page. Made by the module (the object is its
// widget, a child of the parent given, and goes with it).
class TutorialPage {
public:
  virtual ~TutorialPage() = default;
  virtual QWidget* widget() = 0;
  // Shows `page`, a local file (in the package's folder for its bridge).
  virtual void open(const QString& page) = 0;
  // An event of the guide or the tutorial ({"event": ...}) to the page's
  // window.mitcad.on listeners.
  virtual void sendEvent(const QString& json) = 0;
};

class TutorialPageHost {
public:
  virtual ~TutorialPageHost() = default;
  // A call of window.mitcad from a page of the package: {"call", "args"}
  // (docs/tutorials.md "The bridge"); its answer's JSON, or false with the
  // reason. Called from the event loop, never from within CEF.
  virtual bool bridgeCall(const QString& request, QString* answer, QString* error) = 0;
  // As ComponentBrowserHost's.
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
// Interface 4 (mitcad#2281): importDownload's WebImportResult and the
// window's showHostNote.
constexpr int kComponentBrowserAbi = 4;
using ComponentBrowserAbiFunction = int (*)();
using CreateComponentBrowserFunction = QDialog* (*)(ComponentBrowserHost* host, QWidget* parent);
inline constexpr char kComponentBrowserAbiSymbol[] = "mitcad_component_browser_abi";
inline constexpr char kCreateComponentBrowserSymbol[] = "mitcad_create_component_browser";
// The tutorial pane's page (interface 3, mitcad#2012): its bridge for the
// pages in `folder`; null when CEF does not start (`host` lives while the
// page does).
using CreateTutorialPageFunction = TutorialPage* (*)(TutorialPageHost* host, const QString& folder, QWidget* parent);
inline constexpr char kCreateTutorialPageSymbol[] = "mitcad_create_tutorial_page";

} // namespace mitcad
