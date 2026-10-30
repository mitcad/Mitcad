// SPDX-License-Identifier: MIT
#include "ProcessEnd.hpp"

#include <cstdio>
#include <cstdlib>

#include "mitcad/io/mesh.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad {

int endProcess(int code) {
  if (abandoned_imports() > 0 || io::abandoned_faces() > 0) {
    std::fflush(nullptr);
    std::_Exit(code);
  }
  return code;
}

} // namespace mitcad
