# SPDX-License-Identifier: MIT
# Qt's platform plugin on Linux (mitcad#2031): Wayland or X11, Qt's EGL
# integration on X11, the start-up line: plain Qt Core.
target_sources(test_app PRIVATE tests/display_platform.cpp framework/DisplayPlatform.cpp framework/DisplayPlatform.hpp)
