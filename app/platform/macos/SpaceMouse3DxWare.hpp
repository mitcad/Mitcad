// SPDX-License-Identifier: MIT
#pragma once

#include "../SpaceMouseDevice.hpp"

namespace mitcad {

// A client of the framework installed by the user's 3DxWare driver.
class SpaceMouse3DxWareBackend : public SpaceMouseBackend {
public:
  virtual bool isListening() const = 0;
};

std::unique_ptr<SpaceMouse3DxWareBackend> create3DxWareSpaceMouseBackend(QString hidName);

} // namespace mitcad
