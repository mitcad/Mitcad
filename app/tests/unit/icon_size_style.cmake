# SPDX-License-Identifier: MIT
# The metrics it scales (once, not again through the base style's own
# queries), the style keys through it and the application's style it
# sets (also used by WindowStyle and MacToolButtonStyle in test_app); its
# pop-up rows' band is WindowStyle's (browser_roles.cmake, mitcad#2084).
target_sources(test_app PRIVATE tests/icon_size_style.cpp framework/IconSizeStyle.cpp
               framework/IconSizeStyle.hpp)
