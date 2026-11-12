// SPDX-License-Identifier: MIT
// Windows Explorer's thumbnails of MitCad designs (mitcad#1695): the class
// of the thumbnail handler mitcad-thumbnails.dll (ThumbnailHandler.cpp), an
// in-process COM server with IInitializeWithStream and IThumbnailProvider.
// The installer registers it for .mitcad (packaging/windows/filetype.nsh,
// which repeats this CLSID); the test app/tests/thumbnail_handler_windows.cpp
// checks that both agree.
#pragma once

#include <windows.h>

namespace mitcad::thumbnails {

// {3A94966B-4167-4E3A-81FA-E3F4B2233496}
inline constexpr CLSID kHandlerClsid = {
    0x3a94966b, 0x4167, 0x4e3a, {0x81, 0xfa, 0xe3, 0xf4, 0xb2, 0x23, 0x34, 0x96}};
inline constexpr const wchar_t* kHandlerClsidText = L"{3A94966B-4167-4E3A-81FA-E3F4B2233496}";

// The shell's key under a file type's ShellEx for its thumbnail handler
// (IThumbnailProvider's interface id).
inline constexpr const wchar_t* kThumbnailProviderKey = L"{e357fccd-a995-4576-b01f-234630154e96}";

// The largest design file the handler reads, in bytes: a larger one gets no
// thumbnail (Explorer shows the file type's icon).
inline constexpr unsigned long long kMaxFileBytes = 512ull * 1024 * 1024;

}  // namespace mitcad::thumbnails
