// SPDX-License-Identifier: MIT
#pragma once

// The platform's HEIF decoder (mitcad#1304, view/HeifDecoder.hpp), one per
// platform: ImageIO on macOS (platform/macos/HeifImageIO.mm), WIC on
// Windows (platform/windows/HeifWic.cpp), the distribution's libheif
// loaded at run time elsewhere (platform/unix/HeifUnix.cpp). Each finds its
// decoder when a file is decoded, never before.

#include <QByteArray>

#include "../view/HeifDecoder.hpp"

namespace mitcad {

HeifDecoded platformDecodeHeif(const QByteArray& bytes);

} // namespace mitcad
