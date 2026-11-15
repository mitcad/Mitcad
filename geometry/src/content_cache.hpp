// SPDX-License-Identifier: MIT
#pragma once

// What is computed of a shape again and again, kept by the shape's content
// (mitcad#1801): each dressup works on a copy of its input, so the faces of
// a modelled thread that one fillet after another leaves as they were are
// new shapes every time, with the same geometry; what was measured of them
// or set up to cross them with rays is found again by their bytes.

#include <cstddef>
#include <list>
#include <mutex>
#include <string>
#include <string_view>
#include <unordered_map>
#include <utility>

#include <TopoDS_Shape.hxx>

namespace mitcad::geometry::detail {

// The bytes of a shape as BinTools writes it, with its triangulations: its
// geometry, topology, orientations, locations and tolerances. Two shapes
// with the same bytes give the same answer to any question of their
// geometry; empty where the shape cannot be written (then nothing is kept
// for it).
std::string shape_bytes(const TopoDS_Shape& shape);

// Values kept by the bytes of the shapes they belong to, for every thread,
// the least recently used dropped beyond `entries` values or `bytes` bytes
// of keys. A value is taken out while it is used (`take`), so that a value
// that is not safe to use from two threads at once is never shared, and
// put back after (`keep`).
template <class Value>
class ContentCache {
public:
  ContentCache(std::size_t entries, std::size_t bytes) : max_entries_(entries), max_bytes_(bytes) {}

  // The value kept for `key`, taken out; false where none is.
  bool take(const std::string& key, Value& value) {
    const std::lock_guard<std::mutex> lock(mutex_);
    const auto found = index_.find(std::string_view(key));
    if (found == index_.end()) {
      return false;
    }
    const auto entry = found->second;
    index_.erase(found);
    value = std::move(entry->second);
    bytes_ -= entry->first.size();
    order_.erase(entry);
    return true;
  }

  // A copy of the value kept for `key`; false where none is.
  bool find(const std::string& key, Value& value) {
    const std::lock_guard<std::mutex> lock(mutex_);
    const auto found = index_.find(std::string_view(key));
    if (found == index_.end()) {
      return false;
    }
    order_.splice(order_.begin(), order_, found->second);
    value = found->second->second;
    return true;
  }

  // Keeps `value` for `key` (an empty key, or a key kept already: not).
  void keep(std::string key, Value value) {
    if (key.empty() || key.size() > max_bytes_) {
      return;
    }
    const std::lock_guard<std::mutex> lock(mutex_);
    if (index_.count(std::string_view(key)) != 0) {
      return;
    }
    bytes_ += key.size();
    order_.emplace_front(std::move(key), std::move(value));
    index_.emplace(std::string_view(order_.front().first), order_.begin());
    while (!order_.empty() && (order_.size() > max_entries_ || bytes_ > max_bytes_)) {
      bytes_ -= order_.back().first.size();
      index_.erase(std::string_view(order_.back().first));
      order_.pop_back();
    }
  }

private:
  using Entry = std::pair<std::string, Value>;
  std::mutex mutex_;
  std::list<Entry> order_; // the most recently kept or found first
  // (By views of the keys in `order_`, whose nodes never move.)
  std::unordered_map<std::string_view, typename std::list<Entry>::iterator> index_;
  std::size_t bytes_ = 0;
  const std::size_t max_entries_;
  const std::size_t max_bytes_;
};

} // namespace mitcad::geometry::detail
