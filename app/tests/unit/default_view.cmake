# SPDX-License-Identifier: MIT
# The planes' projected edges apart, View › Isometric exact: OCCT's camera.
target_sources(test_app PRIVATE tests/default_view.cpp view/DefaultView.hpp)
target_link_libraries(test_app PRIVATE TKService)
