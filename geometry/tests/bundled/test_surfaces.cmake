# SPDX-License-Identifier: MIT
# Patch, Stitch and Thicken: surface bodies filled, sewn and made solid (mitcad#456).
target_sources(test_geometry_surfaces PRIVATE tests/test_surfaces.cpp)
add_test(NAME geometry.surfaces COMMAND test_geometry_surfaces surfaces)
