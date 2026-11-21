# SPDX-License-Identifier: MIT
# The wheel ignored during a 3D mouse motion and its quiet time, the cap's
# deflection, and the vendor driver's processes by name (mitcad#1981; plain
# C++ and Qt Core).
target_sources(test_app PRIVATE tests/space_mouse_wheel.cpp view/SpaceMouseNavigation.hpp
               view/SpaceMouseWheelGuard.hpp)
