# SPDX-License-Identifier: MIT
# Its rectangle and margins, its colour in the middle and not at the
# corners, the view's gradient around it: plain Qt Gui, painted into
# images.
target_sources(test_app PRIVATE tests/start_backdrop.cpp files/StartBackdrop.cpp files/StartBackdrop.hpp)
