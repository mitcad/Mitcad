# SPDX-License-Identifier: MIT
# A sketch's normal and a click's point in turned, nested and oblique
# occurrences (mitcad#958, mitcad#2566): Qt Core and OCCT's math.
target_sources(test_app PRIVATE tests/sketch_normal.cpp commands/SketchNormal.cpp commands/SketchNormal.hpp)
