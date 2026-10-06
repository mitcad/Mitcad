# SPDX-License-Identifier: MIT
"""A local HTTPS server for the update tests (mitcad#9).

Serves a folder on 127.0.0.1 with a certificate the test made, on a free
port, which it writes to a file once it listens. Every request goes to a
log, one line each: the method, the path and the headers that could tell
anything about the user (User-Agent, Accept-Language, Cookie), so that a
test can check what Mitcad sends, and that it sends nothing at all when
checks are off. Besides the files:

  /redirect/<path>       302 to https://127.0.0.1:<port>/<path>
  /redirect-http/<path>  302 to http://127.0.0.1:<port>/<path>

Usage: update-test-server.py <folder> <cert.pem> <key.pem> <log> <port file>
"""

import functools
import http.server
import os
import ssl
import sys


class Handler(http.server.SimpleHTTPRequestHandler):
    log_path = None

    def record(self):
        with open(self.log_path, "a", encoding="utf-8") as log:
            log.write(
                "%s %s ua=%s lang=%s cookie=%s\n"
                % (
                    self.command,
                    self.path,
                    self.headers.get("User-Agent", "-"),
                    self.headers.get("Accept-Language", "-"),
                    self.headers.get("Cookie", "-"),
                )
            )

    def redirect(self, scheme, rest):
        port = self.server.server_address[1]
        self.send_response(302)
        self.send_header("Location", "%s://127.0.0.1:%d/%s" % (scheme, port, rest))
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_GET(self):
        self.record()
        for prefix, scheme in (("/redirect-http/", "http"), ("/redirect/", "https")):
            if self.path.startswith(prefix):
                self.redirect(scheme, self.path[len(prefix):])
                return
        super().do_GET()

    def log_message(self, format, *args):
        pass


def main():
    folder, cert, key, log, port_file = sys.argv[1:6]
    Handler.log_path = log
    open(log, "a", encoding="utf-8").close()
    handler = functools.partial(Handler, directory=folder)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(cert, key)
    server.socket = context.wrap_socket(server.socket, server_side=True)
    with open(port_file + ".tmp", "w", encoding="ascii") as out:
        out.write("%d\n" % server.server_address[1])
    os.replace(port_file + ".tmp", port_file)
    server.serve_forever()


if __name__ == "__main__":
    main()
