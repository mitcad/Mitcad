// SPDX-License-Identifier: MIT
#pragma once

#include <stdexcept>

namespace mitcad {

// A document access while a job computes the model: the UI thread may not
// use the document then (docs/architecture.md, "Background computation").
// Its own header, without the model's bridge, so that the application's
// input guard (framework/Application.hpp, mitcad#1360) and the unit tests
// can name it.
class ModelBusy : public std::runtime_error {
public:
  ModelBusy() : std::runtime_error("the model is busy computing") {}
};

} // namespace mitcad
