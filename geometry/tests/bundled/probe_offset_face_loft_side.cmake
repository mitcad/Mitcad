# SPDX-License-Identifier: MIT
# A probe, not registered as a test (mitcad#2546, GitHub #723): Offset Face
# on the outer wall of a hollow loft, read from a body written by
# mitcad-cli export; the faces, their surfaces and edges, and the offsets,
# printed.
target_sources(test_geometry_loft_shell PRIVATE tests/probe_offset_face_loft_side.cpp)
