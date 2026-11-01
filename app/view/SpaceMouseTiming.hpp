// SPDX-License-Identifier: MIT
#pragma once

// A 3D mouse motion's time (mitcad#1023): the step each frame moves the
// camera by (SpaceMouseFramePacer) and the timing its log line gives
// (SpaceMouseTiming), both pure so that the unit tests check them from
// given frame times. OcctViewer measures the time between the motion's
// frames as each one begins (view/SpaceMouseView.cpp).
//
// Why the step is paced: the motion moves the camera by the time since its
// last frame, so that it keeps its speed at any frame rate. The time a
// frame begins is not the time it is shown, though: frames that the
// windowing system takes in bursts (two at once, then a wait of two
// refreshes) begin at uneven times while the screen still shows one per
// refresh, and a step of each frame's own interval then moves the view by
// uneven amounts on an even display (GitHub mitcad/Mitcad#68, the view
// moving jerkily under a steady push). The mean of the last frames'
// intervals moves the view by even amounts, and over a steady frame rate
// it is that rate's interval, so the speed still follows real time.

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>

#include <QString>

namespace mitcad {

// The longest time one frame counts for: a stall beyond it (a recompute, a
// dialog) is not made up afterwards, and frame rates down to 10 per second
// keep the cap's speed.
constexpr double kSpaceMouseLongestStep = 0.1;
// A motion's first frame, before any interval is known: one of 60 Hz.
constexpr double kSpaceMouseFirstStep = 1.0 / 60.0;

class SpaceMouseFramePacer {
public:
  // The intervals averaged: enough to even out frames taken a few at a
  // time, few enough to follow a changed frame rate within a fraction of a
  // second.
  static constexpr std::size_t kWindow = 8;

  void reset() {
    m_count = 0;
    m_next = 0;
  }
  // The seconds this frame moves the camera by, from the seconds since the
  // motion's previous frame (negative for its first frame).
  double step(double interval) {
    if (!(interval >= 0.0)) {
      return kSpaceMouseFirstStep;
    }
    m_intervals[m_next] = std::min(interval, kSpaceMouseLongestStep);
    m_next = (m_next + 1) % kWindow;
    m_count = std::min(m_count + 1, kWindow);
    double sum = 0.0;
    for (std::size_t index = 0; index < m_count; ++index) {
      sum += m_intervals[index];
    }
    return sum / static_cast<double>(m_count);
  }

private:
  std::array<double, kWindow> m_intervals{};
  std::size_t m_count = 0;
  std::size_t m_next = 0;
};

// A motion's frames as its log line gives them (app/COMMANDS.md, "Logs
// that UI tests read"): how many, the time from the first to the last,
// their rate, the longest interval between two of them (and whether a grid
// rebuild fell in it), and how many times the layout grid was rebuilt.
struct SpaceMouseTiming {
  int frames = 0;
  double seconds = 0.0; // from the first frame to the last
  double longest = 0.0; // seconds
  bool longestRebuiltGrid = false;
  int gridRebuilds = 0;

  // A frame began `interval` seconds after the previous one (negative for
  // the motion's first); `gridRebuilt`: the previous frame rebuilt the grid.
  void frame(double interval, bool gridRebuilt) {
    ++frames;
    if (!(interval >= 0.0) || !std::isfinite(interval)) {
      return;
    }
    seconds += interval;
    if (interval > longest) {
      longest = interval;
      longestRebuiltGrid = gridRebuilt;
    }
  }
  // Frames per second over the motion (0 for a single frame).
  double framesPerSecond() const { return frames > 1 && seconds > 0.0 ? (frames - 1) / seconds : 0.0; }
  // "0.70 s, 58.6 fps, longest frame 33.4 ms, 1 grid rebuild(s)", the
  // longest frame followed by " with a grid rebuild" when one fell in it.
  QString text() const {
    return QStringLiteral("%1 s, %2 fps, longest frame %3 ms%4, %5 grid rebuild(s)")
        .arg(seconds, 0, 'f', 2)
        .arg(framesPerSecond(), 0, 'f', 1)
        .arg(longest * 1000.0, 0, 'f', 1)
        .arg(longestRebuiltGrid ? QStringLiteral(" with a grid rebuild") : QString())
        .arg(gridRebuilds);
  }
};

} // namespace mitcad
