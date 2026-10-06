// SPDX-License-Identifier: MIT
// Long operations stopped on request (P7e): a modelled thread (OCCT's
// sweeps and boolean cut; seconds in a debug build) asks its source as it
// goes, stops inside when the source says so or another thread cancels,
// and throws Cancelled; the scope restores the source before it, and a
// source that never says yes changes nothing.

#include <atomic>
#include <chrono>
#include <exception>
#include <limits>
#include <thread>

#include "check.hpp"
#include "mitcad/geometry/cancel.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;
using Clock = std::chrono::steady_clock;

// A source that says yes from its n-th ask on; it remembers how often it
// was asked and when it first said yes.
class CountingSource final : public CancelSource {
public:
  explicit CountingSource(long yes_from) : m_yes_from(yes_from) {}

  bool cancel_requested() const noexcept override {
    if (++m_asked < m_yes_from) {
      return false;
    }
    long long none = 0;
    m_first_yes.compare_exchange_strong(none, Clock::now().time_since_epoch().count());
    return true;
  }

  long asked() const { return m_asked.load(); }
  Clock::time_point first_yes() const { return Clock::time_point(Clock::duration(m_first_yes.load())); }

private:
  long m_yes_from;
  mutable std::atomic<long> m_asked{0};
  mutable std::atomic<long long> m_first_yes{0};
};

// M10x1.5 cut into a 10 mm rod `length` long.
ShapePtr rod(double length) { return extrude("F2", {circle(1, 0, 0, 5)}, 0, length); }

ThreadSpec m10() {
  ThreadSpec spec;
  spec.feature = "F3";
  spec.faces = {"F2:side(c1)"};
  spec.pitch = 1.5;
  spec.depth = 0.625 * 0.8660254037844386 * 1.5;
  return spec;
}

double ms(Clock::duration duration) {
  return std::chrono::duration<double, std::milli>(duration).count();
}

void test_scopes() {
  CHECK(!cancel_requested());
  throw_if_cancelled();
  CancelFlag outer;
  CancelFlag inner;
  outer.cancel();
  {
    const CancelScope a(outer);
    CHECK(cancel_requested());
    {
      const CancelScope b(inner);
      CHECK(!cancel_requested());
      inner.cancel();
      CHECK(throws_with([] { throw_if_cancelled(); }, "cancelled"));
    }
    CHECK(cancel_requested());
  }
  CHECK(!cancel_requested());
  // Another thread has no source of its own.
  {
    const CancelScope a(outer);
    bool other = true;
    std::thread([&other] { other = cancel_requested(); }).join();
    CHECK(!other);
  }
  // An operation asked to stop does not start.
  const CancelScope a(outer);
  CHECK(throws_with([] { rod(6); }, "cancelled"));
}

void test_asked_inside() {
  // A source that never says yes is asked inside OCCT's algorithms, and
  // the result is the one made without it.
  const ShapePtr body = rod(6);
  const Clock::time_point start = Clock::now();
  const double plain = volume(*modeled_thread(*body, m10()));
  const Clock::time_point middle = Clock::now();
  CountingSource never(std::numeric_limits<long>::max());
  double asked_volume = 0;
  {
    const CancelScope scope(never);
    asked_volume = volume(*modeled_thread(*body, m10()));
  }
  CHECK(near(asked_volume, plain, 1e-9));
  // Twice per sweep and per cut between the steps; far more inside.
  const long total = never.asked();
  std::printf("cancel: a 6 mm modelled thread asks its source %ld times (%.0f ms; %.0f ms without)\n",
              total, ms(Clock::now() - middle), ms(middle - start));
  CHECK(total > 100);

  // Yes halfway through: it stops there.
  CountingSource halfway(total / 2);
  bool cancelled = false;
  try {
    const CancelScope scope(halfway);
    modeled_thread(*body, m10());
  } catch (const Cancelled&) {
    cancelled = true;
  }
  const Clock::time_point end = Clock::now();
  CHECK(cancelled);
  CHECK(halfway.asked() < total);
  CHECK(ms(end - halfway.first_yes()) < 1000);
}

void test_cancel_from_another_thread() {
  // The 20 mm thread takes seconds in a debug build, still a while in a
  // release build: cancelled 100 ms in, it stops within a second.
  const ShapePtr body = rod(20);
  CancelFlag flag;
  std::atomic<bool> started{false};
  bool cancelled = false;
  std::exception_ptr other;
  std::thread worker([&] {
    try {
      const CancelScope scope(flag);
      started = true;
      modeled_thread(*body, m10());
    } catch (const Cancelled&) {
      cancelled = true;
    } catch (...) {
      other = std::current_exception();
    }
  });
  while (!started) {
    std::this_thread::yield();
  }
  std::this_thread::sleep_for(std::chrono::milliseconds(100));
  const Clock::time_point requested = Clock::now();
  flag.cancel();
  worker.join();
  const double after = ms(Clock::now() - requested);
  std::printf("cancel: the 20 mm modelled thread stopped %.0f ms after the request\n", after);
  CHECK(cancelled);
  CHECK(!other);
  CHECK(after < 1000);
}

} // namespace

void cancel_tests() {
  guarded("test_scopes", test_scopes);
  guarded("test_asked_inside", test_asked_inside);
  guarded("test_cancel_from_another_thread", test_cancel_from_another_thread);
}

} // namespace test
