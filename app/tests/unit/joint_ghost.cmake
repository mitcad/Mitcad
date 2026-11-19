# SPDX-License-Identifier: MIT
# The ghost of a joint's side A before B is picked, and which bodies a
# ghost entry covers (mitcad#2027): plain Qt Core, header only.
target_sources(test_app PRIVATE tests/joint_ghost.cpp framework/ViewGhosts.hpp)
