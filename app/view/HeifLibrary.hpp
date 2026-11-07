// SPDX-License-Identifier: MIT
// HEIF images through a libheif loaded at run time (mitcad#1304): Linux's
// decoder is the distribution's libheif (libheif.so.1) with its HEVC
// plugin, when the user has installed them. MitCad neither links nor ships
// it (no libheif header at build time either): the few functions it needs
// are resolved by name when the first HEIF file is decoded, and the
// library stays loaded for the next one.
#pragma once

#include <QByteArray>
#include <QString>

#include "view/HeifDecoder.hpp"

namespace mitcad {

// The libheif that the platform's loader takes (dlopen's search: the
// distribution's library folders).
inline const char* const kHeifLibraryName = "libheif.so.1";

// The file's primary image decoded by the libheif at `library` (a name the
// loader searches for, or a path): NoDecoder when there is no such library,
// it lacks a function MitCad needs, or it has no decoder for the file's
// codec (no HEVC plugin); Failed with libheif's reason for a file it
// refuses. libheif applies the file's rotation, mirroring and crop itself.
HeifDecoded decodeHeifWithLibrary(const QString& library, const QByteArray& bytes);

} // namespace mitcad
