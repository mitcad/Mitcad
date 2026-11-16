# SPDX-License-Identifier: MIT
# The query's entries, points on an image and on its plane, the log
# line and the calibration's distance: plain Qt Core.
target_sources(test_app PRIVATE tests/image_references.cpp view/ImageReferences.cpp view/ImageReferences.hpp)
