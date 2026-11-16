# SPDX-License-Identifier: MIT
# The folder source's map detection, scan, thumbnails and fetch, the
# registry, the cache's folders and Use's command: plain Qt Gui.
target_sources(test_app PRIVATE tests/texture_sources.cpp
               materials/FolderTextureSource.cpp materials/FolderTextureSource.hpp
               materials/TextureSource.cpp materials/TextureSource.hpp
               materials/TextureUse.cpp materials/TextureUse.hpp)
