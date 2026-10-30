// SPDX-License-Identifier: MIT
#pragma once

#include <QByteArray>
#include <QList>
#include <QString>

namespace mitcad::mcp {

// Requests and responses share the Rust protocol's MAX_FRAME_BYTES limit,
// excluding the newline delimiter. Oversized results become tool errors.
constexpr qsizetype kMaxFrameBytes = 8 * 1024 * 1024;

struct Endpoint {
  QString instance;
  qint64 pid = 0;
  quint16 port = 0;
  QString token;
};

QString discoveryDirectory();
bool prepareDiscoveryDirectory(QString* error);
bool publishEndpoint(const Endpoint& endpoint, QString* error);
void removeEndpoint(const Endpoint& endpoint);
// The current user's private records. A record whose process is no longer
// running (a killed or crashed app, mitcad#437) is removed, not returned;
// a returned record is live only once its token authenticates.
QList<Endpoint> discover(QString* error = nullptr);
// The authentication line; `chat`: the chat panel's marker of its own
// bridge connection (mitcad#947), whose tool calls the panel approves.
QByteArray encodeAuth(const Endpoint& endpoint, const QString& chat = QString());
// The longest chat marker the listener takes.
constexpr qsizetype kMaxChatMarker = 64;
// Whether a process with this id is running, without waiting. True when
// the system cannot tell (no right to query it), so that such a record is
// still probed; false for an id no process can have.
bool processRunning(qint64 pid);

} // namespace mitcad::mcp
