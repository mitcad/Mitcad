# SPDX-License-Identifier: MIT
# Repeated work taken out of OCCT with the same results (mitcad#1823): a
# blend's line read by index in constant time keeps its points' order and
# addresses (port patch 1690), and a curve/surface intersection with the
# surface's kept polyhedra gives the intersections made without them
# (patch 1691).
target_sources(test_geometry_base PRIVATE tests/test_kernel_repeated_work.cpp)
add_test(NAME geometry.kernel_repeated_work COMMAND test_geometry_base kernel_repeated_work)
