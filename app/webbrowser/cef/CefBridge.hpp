// SPDX-License-Identifier: MIT
#pragma once

// The tutorials' bridge on the browser side (mitcad#2012, CefBridgeConfig.hpp):
// the message router's handler of a tutorial page's CefView. A query is
// answered only for a frame whose page is a local file in the tutorial's
// folder (checked again here, whatever the renderer did); its call goes to
// the view's owner from the event loop (never from within CEF's callback,
// which must not run Qt's dialogs or end the view), and the guide's events
// go to the page's persistent "events" query, one per frame. A page's calls
// are limited (mitcad#2615, TutorialPageRules.hpp): at most 4 MB each, 20 a
// second, 4 of each call waiting for their answers.

#include <cstdint>
#include <functional>
#include <map>
#include <string>
#include <utility>

#include <QString>

#include "include/wrapper/cef_message_router.h"
#include "webbrowser/TutorialPageRules.hpp"

namespace mitcad::cef {

// The file address prefix of a folder's pages, as the renderer compares it
// (kBridgePrefixKey).
std::string bridgePrefix(const QString& folder);
// Whether `url` is a local file inside `folder` (both resolved as they are
// on the disk).
bool bridgeAllows(const QString& folder, const QString& url);

class BridgeHandler : public CefMessageRouterBrowserSide::Handler {
public:
  // `post` runs a call from the event loop: the request's JSON and the
  // answer to give (true with the answer's JSON, false with the reason).
  using Reply = std::function<void(bool ok, const QString& text)>;
  using Post = std::function<void(const QString& request, Reply reply)>;
  BridgeHandler(QString folder, Post post) : m_folder(std::move(folder)), m_post(std::move(post)) {}

  bool OnQuery(CefRefPtr<CefBrowser> browser, CefRefPtr<CefFrame> frame, int64_t queryId, const CefString& request,
               bool persistent, CefRefPtr<Callback> callback) override;
  void OnQueryCanceled(CefRefPtr<CefBrowser> browser, CefRefPtr<CefFrame> frame, int64_t queryId) override;

  // An event to every page listening.
  void send(const QString& json);
  // The view goes: no more events.
  void clear() {
    m_listeners.clear();
    m_eventFrames.clear();
  }

private:
  using BridgeLimiter = tutorialrules::BridgeLimiter;
  QString m_folder;
  Post m_post;
  std::map<int64_t, CefRefPtr<Callback>> m_listeners;
  // The events listener of each frame (its identifier): one each.
  std::map<std::string, int64_t> m_eventFrames;
  // The calls' limits (mitcad#2615, TutorialPageRules.hpp); a run of
  // refusals is logged once.
  BridgeLimiter m_limiter;
  bool m_limitLogged = false;
};

} // namespace mitcad::cef
