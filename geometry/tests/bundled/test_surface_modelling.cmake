# SPDX-License-Identifier: MIT
# Surface modelling: surface bodies and their features (mitcad#455).
target_sources(test_geometry_surfaces PRIVATE tests/test_surface_modelling.cpp)
add_test(NAME geometry.surface_modelling COMMAND test_geometry_surfaces surface_modelling)
