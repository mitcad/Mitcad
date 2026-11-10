// SPDX-License-Identifier: MIT
#pragma once

// The Chromium Embedded Framework in the web browser component's module
// (mitcad#1579): started once for the process from the component's folder
// (its runtime, resources, locales and subprocess helper) with MitCad's own
// browsing profile, its message loop pumped from Qt's (external message
// pump: CEF's browser thread is Qt's main thread), windowless (off-screen)
// browsers painted in software (no GPU, which test machines and VMs lack),
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

} // namespace cef
} // namespace mitcad
