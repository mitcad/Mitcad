# SPDX-License-Identifier: MIT
# Decals (mitcad#2244): the query's entries as the view draws them, the
# decal under a point and the log line: plain Qt Core.
target_sources(test_app PRIVATE tests/decals.cpp view/Decals.cpp view/Decals.hpp)
