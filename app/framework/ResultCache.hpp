// SPDX-License-Identifier: MIT
#pragma once

// The caches of computed results (P7d, docs/architecture.md): the result
// store on disk, which opening a design takes the costly features' results
// from, and its settings (Preferences, Cache).

#include <QString>
#include <QtGlobal>

namespace mitcad {

// Kept in the user's settings (cache/disk, cache/diskMegabytes,
// cache/memoryMegabytes). Sizes, like every unit the application chooses,
// are metric.
struct CacheSettings {
  static constexpr int kMinDiskGigabytes = 1;
  static constexpr int kMaxDiskGigabytes = 10000;
  static constexpr int kMinMemoryMegabytes = 256;

  // Results of costly evaluations are kept on disk, at most this much.
  bool disk = true;
  qint64 diskMegabytes = 5 * 1024;
  // The model's memory cache of computed results, at most this much
  // (estimated): a quarter of the machine's memory unless set.
  qint64 memoryMegabytes = defaultMemoryMegabytes();

  static qint64 defaultMemoryMegabytes();
  // The machine's memory, the most the memory cache can be given.
  static qint64 physicalMegabytes();
  static CacheSettings load();
  void save() const;
};

// The result store's folder: MITCAD_RESULT_STORE when set (tests; "off"
// for none), else "results" in the user's cache folder
// (~/.cache/Mitcad/Mitcad/results on Linux,
// %LOCALAPPDATA%\Mitcad\Mitcad\cache\results on Windows). Empty when the
// store is off (Preferences) or the system gives no place.
QString resultStoreDirectory();
// Where the store would be with it on, for Preferences.
QString resultStoreLocation();
// What results in the store must have been stored by: this build of the
// program (geometry::kernel_build_id()).
QString resultStoreBuildId();
// Evaluations quicker than this are not stored: 100 ms, or
// MITCAD_RESULT_STORE_MIN_MS (tests).
double resultStoreMinMs();

// Removes the store's files used longest ago when it holds more than its
// size, on a thread of the pool (at start-up and after results were
// stored); the store's documents may go on using it meanwhile.
void collectResultStoreGarbage();

} // namespace mitcad
