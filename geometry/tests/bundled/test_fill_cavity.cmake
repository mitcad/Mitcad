# SPDX-License-Identifier: MIT
# Fill Cavity (mitcad#1818): a block with a blind V slit beside a taller
# wall, its empty spaces below the cap: the slit closed by the cap with
# its volume and its walls' names, the space round the block open; a box
# the body does not reach whole; the body unchanged.
target_sources(test_geometry_surfaces PRIVATE tests/test_fill_cavity.cpp)
add_test(NAME geometry.fill_cavity COMMAND test_geometry_surfaces fill_cavity)
