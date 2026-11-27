# SPDX-License-Identifier: MIT
# A block's two long top edges rounded where an oval pocket beside each
# lies closer to it than the rounding is wide (GitHub #635, mitcad#2365):
# pockets mirrored about the middle plane and turned half a turn, at 2.5
# and 4 mm; each edge alone, both in one set and in two. Each edge rounds
# the way the other does (the material, point by point, each other's
# image) or both fail in words; one valid closed solid each. `--probe`
# prints the faces and the kernel's fillet alone. Up to seven minutes on an
# idle machine (the 4 mm cases most of it).
target_sources(test_geometry_fillet_o_s PRIVATE tests/test_fillet_pocket_rims.cpp)
add_test(NAME geometry.fillet_pocket_rims COMMAND test_geometry_fillet_o_s fillet_pocket_rims)
set_tests_properties(geometry.fillet_pocket_rims PROPERTIES TIMEOUT 900)
