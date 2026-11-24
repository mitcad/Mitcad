// SPDX-License-Identifier: MIT
#pragma once

// Who reads the window through UI Automation (mitcad#2227, Windows only;
// elsewhere these do nothing). Another process that reads the window (a
// screen reader, an assistant's desktop app, a driver's helper) is served
// on the UI thread; such reads froze the window for minutes and nothing
// said who read it. The application's main windows get an accessible
// interface that notes each call coming from another process and logs,
// for a burst of calls of 100 ms or more (at most once a minute per
// process; a burst of a second or more always),
//   Accessibility client: <exe name> (pid N): bulk read, N nodes, T ms
// (N the nodes the window's tree has, T from the burst's first call to the
// event loop's turn after its last), and after a burst of a second or more
// where the window's nodes are ("Accessibility client read: <subtree>: N
// nodes"). The caller comes from CoGetCallerTID (its thread, the thread's
// process), which COM tells only for a caller thread in a single-threaded
// apartment; UI Automation calls from threads of its own in the
// multithreaded apartment, so a UI Automation client is "unknown (pid
// unknown)" (measured 22.11.2026, with the client's thread in either
// apartment). While a burst runs, the hang watchdog's line names the
// client, and for an unknown one the processes that have UI Automation
// loaded, which it then finds (HangWatchdog.hpp).
// MITCAD_LOG_ACCESSIBILITY_CLIENTS=1 logs every burst, and for each unknown
// caller "Accessibility client candidates: UI Automation loaded in <exe>
// (pid N), ..." (the UI test).

#include <QString>

namespace mitcad {

// Installs the accessible interface of the application's top-level windows
// (a QAccessible factory); once, after the QApplication, before the first
// window is shown.
void installAccessibilityClientLog();

// The client of the burst under way, "<exe name> (pid N)", or "" when none.
// Any thread.
QString currentAccessibilityClient();

// The other processes that have UI Automation's client library loaded,
// "<exe> (pid N), ..." or "no other process" ("" off Windows): who can be
// reading the window. It reads every process's module list: off the UI
// thread, and not often.
QString accessibilityClientCandidates();

} // namespace mitcad
