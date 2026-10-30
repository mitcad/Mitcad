// SPDX-License-Identifier: MIT
#pragma once

#include <QString>

class QTcpServer;

namespace mitcad::mcp {

// Makes the server listen on 127.0.0.1 at a port the system chooses, for the
// live MCP endpoint. On Windows the listening socket is WinSock's own, opened
// with SO_EXCLUSIVEADDRUSE before it is bound (mitcad#945), so that no other
// local process can bind the same address and port and take the connections
// meant for MitCad; elsewhere it is QTcpServer::listen. On a failure the
// server does not listen and error (when given) says why.
bool listenOnLoopback(QTcpServer& server, QString* error);

} // namespace mitcad::mcp
