// SPDX-License-Identifier: MIT
#pragma once

// The Chromium Embedded Framework in the web browser component's module
// (mitcad#1579): started once for the process from the component's folder
// (its runtime, resources, locales and subprocess helper) with MitCad's own
// browsing profile, its message loop pumped from Qt's (external message
// pump: CEF's browser thread is Qt's main thread), windowless (off-screen)
// browsers composited in software (no GPU texture; WebGL on the GPU where
// there is one, else in software, mitcad#1861; in software only after the
// GPU path failed in an earlier session, mitcad#1906),
// and shut down as MitCad ends, its browsers closed first.

#include <QString>

#include "include/cef_browser.h"
#include "include/cef_request_context.h"

namespace mitcad {

class CefView;

namespace cef {

// Starts CEF once (later calls answer as the first did); false with the
// reason.
bool startEngine(const QString& componentFolder, const QString& profileFolder, QString* error);
bool engineRunning();
// MitCad's request context: its own cache and cookies in the profile.
CefRefPtr<CefRequestContext> requestContext();
// Asks the loop to run soon: CEF has work (after a browser's creation).
void pumpSoon();

// The views and browsers alive, for the shutdown: a view drops its browser
// before CEF ends, and CEF ends once every browser has closed.
void addView(CefView* view);
void removeView(CefView* view);
void browserCreated(CefRefPtr<CefBrowser> browser);
void browserClosed(CefRefPtr<CefBrowser> browser);

// What Help › Send Feedback's diagnostics carry of the browser (mitcad#1906,
// webbrowser/BrowserDiagnostics.hpp): a message of a page's console (kept
// when it is the page's own error or about WebGL), and a line of MitCad's
// own scripts that is the WebGL probe's or a lost context's (true; false
// for any other, which the view hands on). CEF's log is read every 2 s for
// the GPU process's failures.
void notePageMessage(const QString& pageUrl, bool error, const QString& message, const QString& source, int line);
bool noteScriptMessage(const QString& message);
// The GPU path failed this run: WebGL in software from the next start.
bool gpuFailed();

} // namespace cef
} // namespace mitcad
