# SPDX-License-Identifier: MIT
# A probe, not registered as a test (mitcad#2229): OCCT's own chamfer and
# fillet of the back's bottom edge of the stepped block rounded by 5 on
# eleven edges, whose ends lie on a flat face smaller than the blend's
# section: where OCCT stops or how its result differs from the exact
# shape, for the port patch of the blend's end.
target_sources(test_geometry_chamfer PRIVATE tests/probe_end_face_dressup.cpp)
