// SPDX-License-Identifier: MIT
// The web browser component's subprocess helper (mitcad#1579): CEF's
// renderer, GPU, network and utility processes run this program
// (CefSettings::browser_subprocess_path), never MitCad itself. Plain CEF, no
// Qt. On macOS it is the program of the helper apps ("MitCad Browser
// Helper.app" and its (GPU), (Renderer), (Plugin) and (Alerts) variants),
// which load CEF's framework themselves.
//
// In the renderer it makes the tutorials' bridge (mitcad#2012,
// CefBridgeConfig.hpp): CEF's message router and window.mitcad, only in the
// frames of the tutorial folder a browser was made for.

#include <map>
#include <string>

#include "CefBridgeConfig.hpp"
#include "include/cef_app.h"
#include "include/cef_render_process_handler.h"
#include "include/cef_v8.h"
#if defined(__APPLE__)
#include "include/wrapper/cef_library_loader.h"
#endif

namespace {

class HelperApp : public CefApp, public CefRenderProcessHandler {
public:
  CefRefPtr<CefRenderProcessHandler> GetRenderProcessHandler() override { return this; }

  void OnWebKitInitialized() override {
    m_router = CefMessageRouterRendererSide::Create(mitcad::cef::bridgeRouterConfig());
  }
  void OnBrowserCreated(CefRefPtr<CefBrowser> browser, CefRefPtr<CefDictionaryValue> extra) override {
    if (extra && extra->HasKey(mitcad::cef::kBridgePrefixKey)) {
      m_prefixes[browser->GetIdentifier()] = extra->GetString(mitcad::cef::kBridgePrefixKey).ToString();
    }
  }
  void OnBrowserDestroyed(CefRefPtr<CefBrowser> browser) override { m_prefixes.erase(browser->GetIdentifier()); }

  void OnContextCreated(CefRefPtr<CefBrowser> browser, CefRefPtr<CefFrame> frame,
                        CefRefPtr<CefV8Context> context) override {
    const auto prefix = m_prefixes.find(browser->GetIdentifier());
    const std::string url = frame->GetURL().ToString();
    if (prefix != m_prefixes.end()) {
      // A tutorial's browser, any of its frames (mitcad#2615): no WebRTC,
      // which reaches addresses outside the request filter (STUN and TURN
      // servers), removed before the page's own scripts run.
      CefRefPtr<CefV8Value> global = context->GetGlobal();
      for (const char* name : mitcad::cef::kTutorialRemovedGlobals) {
        if (global->HasValue(name)) {
          global->DeleteValue(name);
        }
      }
    }
    if (!m_router || prefix == m_prefixes.end() || !mitcad::cef::bridgePrefixMatches(url, prefix->second)) {
      return; // no bridge outside the tutorial's own folder
    }
    m_router->OnContextCreated(browser, frame, context);
    CefRefPtr<CefV8Value> result;
    CefRefPtr<CefV8Exception> exception;
    context->Eval(mitcad::cef::kBridgeScript, url, 0, result, exception);
  }
  void OnContextReleased(CefRefPtr<CefBrowser> browser, CefRefPtr<CefFrame> frame,
                         CefRefPtr<CefV8Context> context) override {
    if (m_router) {
      m_router->OnContextReleased(browser, frame, context);
    }
  }
  bool OnProcessMessageReceived(CefRefPtr<CefBrowser> browser, CefRefPtr<CefFrame> frame, CefProcessId source,
                                CefRefPtr<CefProcessMessage> message) override {
    return m_router && m_router->OnProcessMessageReceived(browser, frame, source, message);
  }

private:
  CefRefPtr<CefMessageRouterRendererSide> m_router;
  // The bridge's address prefix of each browser made with one.
  std::map<int, std::string> m_prefixes;
  IMPLEMENT_REFCOUNTING(HelperApp);
};

} // namespace

#if defined(_WIN32)
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>

int APIENTRY wWinMain(HINSTANCE instance, HINSTANCE, LPWSTR, int) {
  CefMainArgs arguments(instance);
  CefRefPtr<HelperApp> app(new HelperApp);
  return CefExecuteProcess(arguments, app, nullptr);
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
  CefRefPtr<HelperApp> app(new HelperApp);
  return CefExecuteProcess(arguments, app, nullptr);
}
#endif
