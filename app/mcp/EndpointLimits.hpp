// SPDX-License-Identifier: MIT
#pragma once

#include <vector>

#include <QString>
#include <QtGlobal>

namespace mitcad::mcp {

// The live MCP endpoint's limits against a hostile local client
// (mitcad#935): how much of a client's text a refusal repeats, the slots of
// authenticated and waiting connections, and the token's comparison.

// The most characters of a client's own text that a refusal repeats.
constexpr qsizetype kEchoChars = 200;
// The most characters of a refusal that its log line keeps.
constexpr qsizetype kRefusalLogChars = 2000;
// Connections that authenticated (agents and the bridge's probes).
constexpr int kMaxClients = 8;
// Connections that have not authenticated yet: they hold no client slot.
constexpr int kMaxPending = 4;

// `text` as a message repeats it: whole when it has at most `most`
// characters, else its first `most` with "…" and how many were left out.
QString echoed(const QString& text, qsizetype most = kEchoChars);

// For a new connection: the index in `pendingSince` (the order in which
// the connections still to authenticate came, any order of the list) of
// the oldest one, to be closed when `most` already wait; -1 when there is
// room.
int pendingToClose(const std::vector<quint64>& pendingSince, int most = kMaxPending);

// Whether a connection that has just authenticated may stay with
// `authenticated` others in.
bool clientSlotFree(int authenticated, int most = kMaxClients);

// Whether `given` is the endpoint's token, in a time that does not depend
// on where they differ: fixed-length digests of both compared byte by
// byte without an early exit. An empty `expected` matches nothing.
bool sameToken(const QString& given, const QString& expected);

} // namespace mitcad::mcp
