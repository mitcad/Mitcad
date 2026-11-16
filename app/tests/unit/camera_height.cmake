# SPDX-License-Identifier: MIT
target_sources(test_app PRIVATE tests/camera_height.cpp view/CameraHeight.hpp)
target_link_libraries(test_app PRIVATE TKService)
