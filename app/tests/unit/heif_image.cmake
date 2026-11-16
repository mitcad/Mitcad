# SPDX-License-Identifier: MIT
# The formats told apart, HEIF images decoded upright through a decoder
# of the test's own and through the platform's, embedded as PNG, the
# message without a decoder, and the run-time libheif with a library
# that is not there (on every platform): tests/heif_image.cpp.
target_sources(test_app PRIVATE tests/heif_image.cpp ${mitcad_heif_sources})
if(WIN32 OR APPLE)
  target_sources(test_app PRIVATE view/HeifLibrary.cpp view/HeifLibrary.hpp)
endif()
if(mitcad_heif_platform_libraries)
  target_link_libraries(test_app PRIVATE ${mitcad_heif_platform_libraries})
endif()
