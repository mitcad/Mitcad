# SPDX-License-Identifier: MIT
# Sketch curves picked in the Browser (mitcad#1978): the rows read from the
# sketch answers (plain Qt Core) and their rows in a BrowserPanel (whose
# sources browser_roles.cmake gives test_app).
target_sources(test_app PRIVATE tests/browser_sketch_curves.cpp browser/SketchCurveRows.cpp
               browser/SketchCurveRows.hpp)
