# SPDX-License-Identifier: MIT
# A plate rounded 15 with a cylinder of radius 15 on one corner's axis
# (GitHub #466, mitcad#2050): shells 2 inside and outside with the
# cylinder's top and the plate's bottom open, and the closed inward
# offset, with OCCT alone (port patch 2050: the foot's intersection all
# round a circle trimmed across its origin), against the volumes from the
# areas; with "probe" a printout of variants for the diagnosis.
target_sources(test_geometry_loft_shell PRIVATE tests/test_shell_boss_on_corner.cpp)
add_test(NAME geometry.shell_boss_on_corner COMMAND test_geometry_loft_shell shell_boss_on_corner)
set_tests_properties(geometry.shell_boss_on_corner PROPERTIES TIMEOUT 300)
