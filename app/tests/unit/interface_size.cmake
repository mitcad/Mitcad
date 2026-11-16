# SPDX-License-Identifier: MIT
# The kept choice, the factor over the user's own QT_SCALE_FACTOR and
# what a started process inherits: plain Qt Core.
target_sources(test_app PRIVATE tests/interface_size.cpp framework/InterfaceSize.cpp framework/InterfaceSize.hpp)
