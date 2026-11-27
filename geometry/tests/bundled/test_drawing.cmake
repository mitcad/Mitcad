# SPDX-License-Identifier: MIT
# Drawings: views with hidden lines removed and round holes (mitcad#471).
target_sources(test_geometry_surfaces PRIVATE tests/test_drawing.cpp)
add_test(NAME geometry.drawing COMMAND test_geometry_surfaces drawing)
