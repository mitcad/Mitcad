// SPDX-License-Identifier: MIT
#include "bridge/cancel.hpp"

#include <utility>

#include "mitcad/geometry/cancel.hpp"
#include "mitcad_bridge/kernel/cancel.h"

namespace mitcad::bridge {

// The check as the geometry's source, and the scope that sets it.
class CancelScope::State final : public geometry::CancelSource {
public:
  explicit State(rust::Box<CancelCheck> check) : m_check(std::move(check)), m_scope(*this) {}

  bool cancel_requested() const noexcept override { return m_check->cancel_requested(); }

private:
  rust::Box<CancelCheck> m_check;
  geometry::CancelScope m_scope;
};

CancelScope::CancelScope(rust::Box<CancelCheck> check) : m_state(std::make_unique<State>(std::move(check))) {}

CancelScope::~CancelScope() = default;

std::unique_ptr<CancelScope> enter_cancel_scope(rust::Box<CancelCheck> check) {
  return std::make_unique<CancelScope>(std::move(check));
}

} // namespace mitcad::bridge
