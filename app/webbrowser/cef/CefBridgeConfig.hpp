// SPDX-License-Identifier: MIT
#pragma once

// The tutorials' bridge window.mitcad (mitcad#2012, docs/tutorials.md "The
// bridge"): what the browser side (CefView, in the module) and the renderer
// side (the subprocess helper, CefHelper.cpp) share. Plain CEF, no Qt: the
// helper links nothing else.
//
// CEF's message router carries the calls: its JavaScript function
// (mitcadQuery) and window.mitcad over it are made only in the frames of a
// tutorial's own folder, whose file address starts with the prefix the
// browser was made with (its extra information, kBridgePrefixKey); the
// browser side checks each call's frame again before it answers. Other pages
// (the web, other files) get neither.

#include <cstddef>
#include <string>

#include "include/wrapper/cef_message_router.h"

namespace mitcad::cef {

inline CefMessageRouterConfig bridgeRouterConfig() {
  CefMessageRouterConfig config;
  config.js_query_function = "mitcadQuery";
  config.js_cancel_function = "mitcadQueryCancel";
  return config;
}

// The browser's extra information: the file address prefix of the pages
// that get the bridge ("file:///.../tutorials/<id>/").
inline constexpr char kBridgePrefixKey[] = "mitcad_bridge_prefix";

// The strict policy of every tutorial's pages, whatever their source (a
// git source's or a course from MitCad Cloud, mitcad#2401; the service
// serves a course's pages with the same, its folder's address for 'self',
// docs/tutorials.md "The strict policy"): the files of the tutorial's own
// folder are given by the browser side (CefView.cpp) with this header;
// scripts, styles, pictures, media and fonts only from there or inline,
// never from an address and never by eval (no 'unsafe-eval'); no frames,
// objects, workers, forms or <base>. A file page's 'self' is every local
// file, so the browser side also refuses a tutorial page's requests for
// anything outside its folder.
inline constexpr char kTutorialPolicy[] =
    "default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; "
    "img-src 'self' data: blob:; media-src 'self' data: blob:; font-src 'self' data:; connect-src 'self'; "
    "object-src 'none'; frame-src 'none'; worker-src 'none'; manifest-src 'none'; base-uri 'none'; "
    "form-action 'none'";

// What a tutorial's frames do not get (mitcad#2615, CefHelper.cpp): WebRTC,
// whose connections to STUN and TURN servers pass by the browser side's
// request filter.
inline constexpr const char* kTutorialRemovedGlobals[] = {
    "RTCPeerConnection", "webkitRTCPeerConnection", "RTCDataChannel", "RTCSessionDescription",
    "RTCIceCandidate"};

// Whether a frame's address is under the prefix: on Windows without regard
// to case (a drive letter, a folder name written otherwise).
inline bool bridgePrefixMatches(const std::string& url, const std::string& prefix) {
  if (prefix.empty() || url.size() < prefix.size()) {
    return false;
  }
  for (std::size_t i = 0; i < prefix.size(); ++i) {
    char a = url[i];
    char b = prefix[i];
#if defined(_WIN32)
    if (a >= 'A' && a <= 'Z') {
      a = static_cast<char>(a - 'A' + 'a');
    }
    if (b >= 'A' && b <= 'Z') {
      b = static_cast<char>(b - 'A' + 'a');
    }
#endif
    if (a != b) {
      return false;
    }
  }
  return true;
}

// window.mitcad, run in a tutorial frame as its context is made (before the
// page's own scripts). Each call is one query whose answer resolves its
// promise; the events come through one persistent query.
inline constexpr char kBridgeScript[] = R"JS((function () {
  if (window.mitcad || typeof window.mitcadQuery !== 'function') { return; }
  var query = window.mitcadQuery;
  var listeners = {};
  function call(name, args) {
    return new Promise(function (resolve, reject) {
      query({
        request: JSON.stringify({call: name, args: args || {}}),
        persistent: false,
        onSuccess: function (answer) { resolve(answer ? JSON.parse(answer) : null); },
        onFailure: function (code, message) { reject(new Error(message)); }
      });
    });
  }
  function deliver(event) {
    [event.event, '*'].forEach(function (name) {
      (listeners[name] || []).slice().forEach(function (listener) {
        try { listener(event); } catch (error) { console.error(error); }
      });
    });
  }
  query({
    request: JSON.stringify({call: 'events', args: {}}),
    persistent: true,
    onSuccess: function (text) { try { deliver(JSON.parse(text)); } catch (error) { console.error(error); } },
    onFailure: function () {}
  });
  var api = {
    version: 1,
    call: call,
    startStep: function (step) { return call('start_step', {step: step}); },
    endStep: function () { return call('end_step'); },
    releaseHold: function () { return call('release_hold'); },
    state: function () { return call('state'); },
    query: function (request) { return call('query', request); },
    openDesign: function (path) { return call('open_design', {path: path}); },
    setProgress: function (progress) { return call('set_progress', progress); },
    place: function (side) { return call('place', {side: side}); },
    demonstrate: function (demonstration) { return call('demonstrate', demonstration); },
    stopDemonstration: function () { return call('stop_demonstration'); },
    shortcut: function (command) { return call('shortcut', {command: command}); },
    navigation: function () { return call('navigation'); },
    leave: function () { return call('leave'); },
    on: function (name, listener) { (listeners[name] = listeners[name] || []).push(listener); },
    off: function (name, listener) {
      listeners[name] = (listeners[name] || []).filter(function (l) { return l !== listener; });
    }
  };
  Object.defineProperty(window, 'mitcad', {value: Object.freeze(api), writable: false, configurable: false});
})();)JS";

} // namespace mitcad::cef
