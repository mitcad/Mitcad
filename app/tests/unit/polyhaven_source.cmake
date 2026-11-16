# SPDX-License-Identifier: MIT
target_sources(test_app PRIVATE tests/polyhaven_source.cpp
               materials/PolyHavenTextureSource.cpp materials/PolyHavenTextureSource.hpp
               materials/TextureFetcher.cpp materials/TextureFetcher.hpp)
target_link_libraries(test_app PRIVATE Qt6::Network)
