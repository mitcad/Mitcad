# SPDX-License-Identifier: MIT
# The start-up's Graphics environment: line (mitcad#2031): the graphics cards
# under a folder laid out as /sys/class/drm and the line's words; plain Qt
# Core.
target_sources(test_app PRIVATE tests/graphics_environment.cpp framework/GraphicsEnvironment.cpp
               framework/GraphicsEnvironment.hpp)
