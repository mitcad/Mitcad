# SPDX-License-Identifier: MIT
# A feature preview's bodies in placed, linked and hidden components
# (mitcad#266): plain Qt Core.
target_sources(test_app PRIVATE tests/preview_bodies.cpp framework/PreviewBodies.cpp framework/PreviewBodies.hpp)
