// SPDX-License-Identifier: MIT
#pragma once

namespace mitcad::geometry::detail {

// Whether the polygonal projection keeps its algorithm's outline flags on
// chords of smooth edges only where the faces come near square to the view
// (drawing.cpp, keep_true_polygonal_outlines; mitcad#2510). On by default;
// geometry.drawing turns it off for a moment to compare the views with the
// lines that algorithm draws by itself.
bool polygonal_true_outlines();
void set_polygonal_true_outlines(bool on);

} // namespace mitcad::geometry::detail
