# SPDX-License-Identifier: MIT
# Isoparametric curves (mitcad#2522): a cylinder's u curve a line along its
# axis and its v curve a circle of its radius, also through a picked point;
# a B-spline face's curves on the face within 1e-6 mm; a hole cutting a
# curve in two; the refusals.
target_sources(test_geometry_surfaces PRIVATE tests/test_iso_curve.cpp)
add_test(NAME geometry.iso_curve COMMAND test_geometry_surfaces iso_curve)
