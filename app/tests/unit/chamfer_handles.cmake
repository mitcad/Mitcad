# SPDX-License-Identifier: MIT
# Convex, concave, obtuse and placed edges, the measured face: plain OCCT
# math; an L bracket's edges read from its solid (mitcad#571).
target_sources(test_app PRIVATE tests/chamfer_handles.cpp commands/ChamferHandles.cpp
               commands/ChamferHandles.hpp)
