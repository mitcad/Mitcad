# SPDX-License-Identifier: MIT
# A probe, not registered as a test (mitcad#2352, GitHub #620): roundings
# wider than the face beside their edge whose chains end at sharp corners
# (a ridge's and a spike's foot 1 in from a block's sides, a pocket's rim
# beside a thin sloped wall), with and without the corners rounded first:
# MitCad's fillet, the narrow-face check, the faces beyond grown and
# OCCT's own fillet, printed.
target_sources(test_geometry_fillet_o_s PRIVATE tests/probe_fillet_rim_narrow_corner.cpp)
