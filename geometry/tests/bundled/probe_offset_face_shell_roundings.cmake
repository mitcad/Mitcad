# SPDX-License-Identifier: MIT
# A probe, not registered as a test (mitcad#2457, GitHub #678): Offset
# Face on the inside of a shelled block with rounded edges, read from a
# body written by mitcad-cli export; the faces, their neighbours and the
# offsets, printed.
target_sources(test_geometry_loft_shell PRIVATE tests/probe_offset_face_shell_roundings.cpp)
