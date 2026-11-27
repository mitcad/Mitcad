# SPDX-License-Identifier: MIT
target_sources(test_app PRIVATE tests/face_projection.cpp view/FaceProjection.hpp)
target_link_libraries(test_app PRIVATE TKService)
