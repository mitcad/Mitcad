// SPDX-License-Identifier: MIT
#pragma once

namespace mitcad {

// The OpenSSL configuration of the Linux AppImage's TLS (mitcad#1208). The
// AppImage's own OpenSSL reads the system's openssl.cnf (its OPENSSLDIR is
// the system's, packaging/linux/make-appimage.sh), so the distribution's
// crypto policy applies. A configuration that this OpenSSL build cannot
// load (an option only the distribution's OpenSSL knows, as openSUSE's
// rh-allow-sha1-signatures) would leave Qt without TLS. This checks, before
// anything uses TLS, whether the system's configuration loads in the
// bundled libcrypto: when it does, nothing changes and OpenSSL loads it as
// before; when it does not, the AppImage's own minimal configuration
// (usr/share/mitcad/openssl.cnf) is loaded in its place and the log says so
// once ("TLS: the system's OpenSSL configuration failed to load (<first
// error>); using the AppImage's own"). Nothing happens where Mitcad has no
// OpenSSL of its own next to it (usr/lib/libcrypto.so.3): the system's
// OpenSSL reads the system's configuration itself. Elsewhere than on Linux
// it does nothing.
void checkOpenSslConfiguration();

} // namespace mitcad
