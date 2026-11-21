// SPDX-License-Identifier: MIT
#pragma once

namespace mitcad::timeline {

// Shared geometry for the Qt and AppKit feature strips, in logical points.
inline constexpr int padding = 6;
inline constexpr int cell = 28;
// The icon in its cell, 2 points around it (mitcad#2092: 24, was 22).
inline constexpr int icon = 24;
inline constexpr int gap = 3;
inline constexpr int marker = 14;
inline constexpr int top = 5;
inline constexpr int height = top + cell + 1;
inline constexpr int cardHeight = height + 8;

} // namespace mitcad::timeline
