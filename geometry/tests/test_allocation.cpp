// SPDX-License-Identifier: MIT
// Allocations that fail inside OCCT (mitcad#132): with the port's patch
// OCCT's allocator throws Standard_OutOfMemory instead of returning null
// (the collections wrote to the null block: the checker crashed in
// CSLib_Class2d under a memory limit) and counts the failure, per process
// and per thread (with the jobs of its thread pool), so an operation
// reports one that OCCT caught inside as out of memory.

#include <cstddef>
#include <cstdlib>
#include <string>
#include <thread>

#include <NCollection_Array1.hxx>
#include <NCollection_LocalArray.hxx>
#include <OSD_Parallel.hxx>
#include <Standard_Failure.hxx>
#include <Standard_OutOfMemory.hxx>

// OSD_Parallel.hxx brings in windows.h, whose empty near and far macros
// would break check.hpp's near().
#undef near
#undef far

#include "check.hpp"
#include "mitcad/geometry/guard.hpp"
#include "mitcad/geometry/primitive.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

void set_env(const char* name, const char* value) {
#ifdef _WIN32
  _putenv_s(name, value);
#else
  setenv(name, value, 1);
#endif
}

// A megabyte: 2^30 of them are more than any address space.
struct Megabyte {
  double values[1 << 17];
};

void test_collection_allocation_throws() {
  const std::size_t before = failed_allocations();
  bool thrown = false;
  try {
    NCollection_Array1<Megabyte> huge;
    huge.Resize(1, 1 << 30, false);
    // Not reached: without the patch the block was null here.
    huge.ChangeFirst().values[0] = 1.0;
  } catch (const Standard_OutOfMemory&) {
    thrown = true;
  }
  CHECK(thrown);
  CHECK(failed_allocations() == before + 1);
  // OCCT's other failures still carry their messages.
  const Standard_Failure failure("still there");
  CHECK(std::string(failure.what()) == "still there");
}

// A collection whose resize fails is left valid, and its destructor frees
// nothing twice (the port's patch 0032; before it the process ended here
// with "double free").
void test_collections_intact_after_failure() {
  bool thrown = false;
  {
    NCollection_Array1<Megabyte> array(1, 2);
    array.ChangeFirst().values[0] = 1.0;
    try {
      array.Resize(1, 1 << 30, false);
    } catch (const Standard_OutOfMemory&) {
      thrown = true;
    }
    CHECK(array.IsEmpty());
  }
  CHECK(thrown);
  thrown = false;
  {
    NCollection_LocalArray<double, 4> local(1000);
    local[999] = 1.0;
    try {
      local.Allocate(std::size_t(1) << 60);
    } catch (const Standard_OutOfMemory&) {
      thrown = true;
    }
    CHECK(local.Size() == 0);
  }
  CHECK(thrown);
}

// Fails one allocation in OCCT's collections and catches the failure, as
// the checker catches its own.
void fail_one_allocation() {
  try {
    NCollection_Array1<Megabyte> huge;
    huge.Resize(1, 1 << 30, false);
    huge.ChangeFirst().values[0] = 1.0;
  } catch (const Standard_OutOfMemory&) {
  }
}

// Failures in the jobs of OCCT's thread pool count for the thread that ran
// the loop; those of another thread do not, but count for the process.
void test_failures_are_counted_per_thread() {
  constexpr int kJobs = 64;
  const std::size_t process = failed_allocations();
  const std::size_t here = failed_allocations_in_thread();
  OSD_Parallel::For(0, kJobs, [](int) { fail_one_allocation(); }, false);
  CHECK(failed_allocations_in_thread() == here + kJobs);
  CHECK(failed_allocations() == process + kJobs);
  std::thread other([] { fail_one_allocation(); });
  other.join();
  CHECK(failed_allocations_in_thread() == here + kJobs);
  CHECK(failed_allocations() == process + kJobs + 1);
}

// One OCCT caught inside the operation, which only the count tells.
void test_caught_failure_is_out_of_memory() {
  PrimitiveSpec box;
  box.feature = "F1";
  box.a = 10;
  box.b = 10;
  box.c = 10;
  set_env("MITCAD_TEST_OCCT_ALLOCATION_FAILS", "primitive");
  std::string error;
  try {
    primitive(box);
  } catch (const std::exception& e) {
    error = e.what();
  }
  set_env("MITCAD_TEST_OCCT_ALLOCATION_FAILS", "");
  CHECK(error == "primitive: out of memory");
  CHECK(primitive(box) != nullptr);
}

} // namespace

void allocation_tests() {
  guarded("collection allocation throws", test_collection_allocation_throws);
  guarded("collections intact after a failure", test_collections_intact_after_failure);
  guarded("failures are counted per thread", test_failures_are_counted_per_thread);
  guarded("caught failure is out of memory", test_caught_failure_is_out_of_memory);
}

} // namespace test
