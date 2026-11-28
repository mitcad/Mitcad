// SPDX-License-Identifier: MIT
#pragma once

// What a tutorial page may make MitCad do (mitcad#2615, docs/tutorials.md
// "What a page cannot do"): every tutorial page is treated as written by
// someone else, so the bridge's calls are limited in number and size, the
// page's texts are cut and escaped before they reach the log (which goes
// with Help › Send Feedback), and a click MitCad makes in a page (the UI
// tests' clickLink) is honoured only in a test run and only for the one
// link MitCad asked about. Plain Qt Core, compiled into the web browser
// component's module, the application and test_app
// (app/tests/tutorial_page_rules.cpp).
//
// The module and the application share no new interface for this
// (ComponentBrowserHost.hpp stays as it is, so that a component with these
// rules reaches the MitCads already installed): the module asks the
// application to open a web link by setting the tutorial page widget's
// dynamic property kWebLinkProperty, and says which rules it keeps by its
// entry point kTutorialPageGuardsSymbol.

#include <cstdint>
#include <deque>
#include <map>

#include <QString>
#include <QUrl>

namespace mitcad::tutorialrules {

// The tutorial page widget's property the module sets (a QUrl, then cleared)
// when the user clicked a link to the web: the application asks the user
// and opens it in the system's browser.
inline constexpr char kWebLinkProperty[] = "mitcadTutorialWebLink";
// The module's entry point (C linkage) that returns the rules it keeps;
// a module without it, or with fewer, shows no tutorials.
inline constexpr char kTutorialPageGuardsSymbol[] = "mitcad_tutorial_page_guards";
using TutorialPageGuardsFunction = int (*)();
// The rules of mitcad#2615: navigation kept in the tutorial's folder, web
// links in the system's browser, no file, script or permission dialogs, no
// WebGL, a request context of its own, the bridge's limits, scripted clicks
// for tests only.
inline constexpr int kTutorialPageGuards = 1;

// The bridge's limits: calls a second, calls of one kind waiting for their
// answers (a page's Promise.all of two queries is fine; a flood is not), the
// largest request, the longest call name and page text logged.
inline constexpr int kBridgeCallsPerSecond = 20;
inline constexpr int kBridgeCallsWaiting = 4;
inline constexpr int kBridgeRequestBytes = 4 * 1024 * 1024;
inline constexpr int kLoggedCallName = 64;
inline constexpr int kLoggedPageText = 300;

// `text` for one line of the log: at most `maximum` characters (then
// "… (n characters)"), line breaks, tabs and other control characters and
// the characters that turn text around (bidirectional controls) written as
// escapes (\n, \t, \u{202e}), so that a page cannot make a line look like
// MitCad's own.
QString logSafeText(const QString& text, int maximum);

// The bridge's calls of one page counted (BridgeHandler): at most
// kBridgeCallsPerSecond a second, and kBridgeCallsWaiting of each call in
// flight (their answers not yet given). A call refused says why in
// `reason`.
class BridgeLimiter {
public:
  // Whether the call `name` of `bytes` bytes may run at `nowMs` (a
  // monotonic clock); counted when it may.
  bool admit(const QString& name, qint64 bytes, qint64 nowMs, QString* reason);
  // The call's answer was given.
  void finished(const QString& name);

private:
  std::deque<qint64> m_recent;     // the times of the calls admitted in the last second
  std::map<QString, int> m_flying; // the calls in flight by name
};

// Clicks MitCad makes in a page as real input (clickAt) for the UI tests'
// clickLink: only in a test run (MITCAD_TEST_INPUT set).
bool scriptedClicksAllowed();
// A new one-off key for a clickLink (random, 32 hex digits).
QString newClickKey();
// The script that finds the page's first link (or, with `buttons`, link or
// button, mitcad#2610) whose text or address contains `text` and logs
// where it is: "mitcad:linkat:<key>:<x>,<y>,<href or the button's text>",
// or "mitcad:nolink:<text>".
QString linkAtScript(const QString& text, const QString& key, bool buttons = false);
// A "mitcad:linkat:" line of the key asked for: true with the point and
// the address; any other key, or a malformed line, is false.
bool parseLinkAt(const QString& message, const QString& key, int* x, int* y, QString* href);

// An address a tutorial page may send to the system's browser: http or
// https with a host.
bool webLinkAddress(const QUrl& url);

} // namespace mitcad::tutorialrules
