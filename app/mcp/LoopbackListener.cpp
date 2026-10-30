// SPDX-License-Identifier: MIT
#include "LoopbackListener.hpp"

#include <QHostAddress>
#include <QTcpServer>

#ifdef Q_OS_WIN
// NOMINMAX and WIN32_LEAN_AND_MEAN come from the top-level CMakeLists.txt,
// so windows.h brings no old winsock.h in beside this one.
#include <winsock2.h>
// Older SDK headers lack it (as Qt's own socket engine allows for).
#ifndef WSA_FLAG_NO_HANDLE_INHERIT
#define WSA_FLAG_NO_HANDLE_INHERIT 0x80
#endif
#endif

namespace mitcad::mcp {

#ifdef Q_OS_WIN
namespace {

// WinSock counts its users: this reference holds while the socket is made,
// and Qt's socket engine keeps its own once it has taken the socket.
class WinsockReference {
public:
  WinsockReference() {
    WSADATA data;
    m_result = WSAStartup(MAKEWORD(2, 2), &data);
  }
  ~WinsockReference() {
    if (m_result == 0) {
      WSACleanup();
    }
  }
  WinsockReference(const WinsockReference&) = delete;
  WinsockReference& operator=(const WinsockReference&) = delete;

  int result() const { return m_result; }

private:
  int m_result = 0;
};

QString winsockFailure(const char* step, int code) {
  return QStringLiteral("%1 failed (WinSock error %2)").arg(QLatin1String(step)).arg(code);
}

} // namespace

bool listenOnLoopback(QTcpServer& server, QString* error) {
  const auto fail = [error](const QString& message) {
    if (error) {
      *error = message;
    }
    return false;
  };
  const WinsockReference winsock;
  if (winsock.result() != 0) {
    return fail(winsockFailure("WSAStartup", winsock.result()));
  }
  // Not inherited by the processes MitCad starts, as Qt's own sockets.
  const SOCKET listener =
      WSASocketW(AF_INET, SOCK_STREAM, IPPROTO_TCP, nullptr, 0, WSA_FLAG_OVERLAPPED | WSA_FLAG_NO_HANDLE_INHERIT);
  if (listener == INVALID_SOCKET) {
    return fail(winsockFailure("socket", WSAGetLastError()));
  }
  const auto abandon = [&](const char* step) {
    const QString message = winsockFailure(step, WSAGetLastError());
    closesocket(listener);
    return fail(message);
  };
  // Before bind: no other socket can then bind this address and port, not
  // even one with SO_REUSEADDR.
  const BOOL exclusive = TRUE;
  if (setsockopt(listener, SOL_SOCKET, SO_EXCLUSIVEADDRUSE, reinterpret_cast<const char*>(&exclusive),
                 static_cast<int>(sizeof(exclusive))) == SOCKET_ERROR) {
    return abandon("setsockopt(SO_EXCLUSIVEADDRUSE)");
  }
  sockaddr_in address{};
  address.sin_family = AF_INET;
  address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
  address.sin_port = 0;
  if (::bind(listener, reinterpret_cast<const sockaddr*>(&address), static_cast<int>(sizeof(address))) ==
      SOCKET_ERROR) {
    return abandon("bind");
  }
  if (::listen(listener, SOMAXCONN) == SOCKET_ERROR) {
    return abandon("listen");
  }
  // Qt makes the socket non-blocking and owns it from here; close() and the
  // server's destructor close it.
  if (!server.setSocketDescriptor(static_cast<qintptr>(listener))) {
    const QString message = server.errorString();
    closesocket(listener);
    return fail(message);
  }
  return true;
}

#else

bool listenOnLoopback(QTcpServer& server, QString* error) {
  if (server.listen(QHostAddress::LocalHost, 0)) {
    return true;
  }
  if (error) {
    *error = server.errorString();
  }
  return false;
}

#endif

} // namespace mitcad::mcp
