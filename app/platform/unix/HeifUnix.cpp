// SPDX-License-Identifier: MIT
// HEIF images on Linux and the BSDs (mitcad#1304, platform/PlatformHeif.hpp):
// the distribution's libheif, loaded when the first HEIF file is decoded
// (view/HeifLibrary.hpp). MITCAD_TEST_HEIF_LIBRARY names another library
// or path for the tests (one that is not there: no decoder).
#include "../PlatformHeif.hpp"

#include <QString>
#include <QtGlobal>

#include "../../view/HeifLibrary.hpp"

namespace mitcad {

HeifDecoded platformDecodeHeif(const QByteArray& bytes) {
  const QString library = qEnvironmentVariable("MITCAD_TEST_HEIF_LIBRARY", QString::fromLatin1(kHeifLibraryName));
  return decodeHeifWithLibrary(library, bytes);
}

} // namespace mitcad
