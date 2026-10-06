// SPDX-License-Identifier: MIT
#include "mitcad/geometry/cancel.hpp"

#include <Message_ProgressIndicator.hxx>
#include <Message_ProgressScope.hxx>

#include "util.hpp"

namespace mitcad::geometry {
namespace {

// What tells this thread's operations to stop, if anything does.
thread_local const CancelSource* t_source = nullptr;

// A progress indicator that shows nothing: OCCT's algorithms ask its user
// break as they go (from their own worker threads too) and stop when the
// source says so.
class Breaker final : public Message_ProgressIndicator {
public:
  explicit Breaker(const CancelSource& source) : m_source(source) {}

protected:
  bool UserBreak() override { return m_source.cancel_requested(); }
  void Show(const Message_ProgressScope&, const bool) override {}

private:
  const CancelSource& m_source;
};

} // namespace

CancelScope::CancelScope(const CancelSource& source) noexcept : m_previous(t_source) {
  t_source = &source;
}

CancelScope::~CancelScope() { t_source = m_previous; }

bool cancel_requested() noexcept { return t_source != nullptr && t_source->cancel_requested(); }

void throw_if_cancelled() {
  if (cancel_requested()) {
    throw Cancelled();
  }
}

namespace detail {

Interrupt::Interrupt() {
  if (t_source != nullptr) {
    m_indicator = new Breaker(*t_source);
  }
}

Message_ProgressRange Interrupt::range() {
  return m_indicator.IsNull() ? Message_ProgressRange() : m_indicator->Start();
}

} // namespace detail
} // namespace mitcad::geometry
