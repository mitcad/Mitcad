// SPDX-License-Identifier: MIT
#pragma once

#include "../SpaceMouseDevice.hpp"

namespace mitcad {

std::unique_ptr<SpaceMouseBackend> createWindows3DxWareBackend();

} // namespace mitcad
