# SPDX-License-Identifier: MIT
# Finder's thumbnails of MitCad designs (mitcad#1696): the Quick Look
# thumbnail extension MitCadThumbnails.appex in Contents/PlugIns of the
# application bundle. Its provider (app/platform/macos/
# QuickLookThumbnailProvider.mm) reads a .mitcad file's preview through
# QuickLookThumbnail.cpp and the thumbnailer's C ABI (the static library
# mitcad_thumbnailer_capi, core/thumbnailer-capi), so there is no second
# parser. Included from app/CMakeLists.txt after the bundle
# (app/packaging/macos/Bundle.cmake: MITCAD_BUNDLE_ID, MITCAD_BUNDLE_DIR,
# MITCAD_MACOS_MIN_VERSION), on macOS only. Windows has its own handler
# (app/thumbnails, mitcad#1695), Linux a freedesktop thumbnailer
# (mitcad#1539).

set(MITCAD_QUICKLOOK_EXECUTABLE MitCadThumbnails)
set(mitcad_quicklook_appex_name "${MITCAD_QUICKLOOK_EXECUTABLE}.appex")
set(mitcad_quicklook_entitlements "${CMAKE_CURRENT_SOURCE_DIR}/packaging/macos/QuickLookThumbnails.entitlements")

# The reading code: plain C++ on Core Graphics, linked by the extension and
# its test. Built as extension-safe code (-fapplication-extension).
add_library(mitcad_quicklook_thumbnail STATIC
  platform/macos/QuickLookThumbnail.cpp
  platform/macos/QuickLookThumbnail.hpp)
target_include_directories(mitcad_quicklook_thumbnail PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}/platform/macos")
target_compile_options(mitcad_quicklook_thumbnail PRIVATE -fapplication-extension)
target_link_libraries(mitcad_quicklook_thumbnail
  PUBLIC mitcad_thumbnailer_capi "-framework CoreGraphics" "-framework CoreFoundation"
  PRIVATE mitcad_warnings)

# The extension: an executable whose entry point is NSExtensionMain, in a
# bundle of its own inside the application's (like the renderer's XPC
# service). The build signs it ad hoc with its sandbox entitlement, before
# the application's own signature seals it (app/CMakeLists.txt).
configure_file(packaging/macos/QuickLookThumbnails.plist.in QuickLookThumbnails.plist @ONLY)
set(mitcad_quicklook_appex "$<TARGET_FILE_DIR:mitcad>/../PlugIns/${mitcad_quicklook_appex_name}")
add_executable(mitcad-quicklook-thumbnails platform/macos/QuickLookThumbnailProvider.mm)
target_compile_options(mitcad-quicklook-thumbnails PRIVATE -fobjc-arc -fapplication-extension)
target_link_options(mitcad-quicklook-thumbnails PRIVATE -fapplication-extension LINKER:-e,_NSExtensionMain)
target_link_libraries(mitcad-quicklook-thumbnails PRIVATE mitcad_quicklook_thumbnail mitcad_warnings
  "-framework Foundation" "-framework QuickLookThumbnailing")
set_target_properties(mitcad-quicklook-thumbnails PROPERTIES
  OUTPUT_NAME "${MITCAD_QUICKLOOK_EXECUTABLE}"
  RUNTIME_OUTPUT_DIRECTORY "${mitcad_quicklook_appex}/Contents/MacOS")
add_custom_command(TARGET mitcad-quicklook-thumbnails POST_BUILD
  COMMAND "${CMAKE_COMMAND}" -E copy_if_different "${CMAKE_CURRENT_BINARY_DIR}/QuickLookThumbnails.plist"
          "${mitcad_quicklook_appex}/Contents/Info.plist"
  COMMAND /usr/bin/codesign --force --sign - --entitlements "${mitcad_quicklook_entitlements}"
          "${mitcad_quicklook_appex}"
  VERBATIM)
set_property(TARGET mitcad-quicklook-thumbnails APPEND PROPERTY LINK_DEPENDS
  "${CMAKE_CURRENT_BINARY_DIR}/QuickLookThumbnails.plist" "${mitcad_quicklook_entitlements}")
# Built before the application, whose signature then covers it; a changed
# extension signs the application again.
add_dependencies(mitcad mitcad-quicklook-thumbnails)
set_property(TARGET mitcad APPEND PROPERTY LINK_DEPENDS "$<TARGET_FILE:mitcad-quicklook-thumbnails>")

# Installed into the bundle; tools/package-macos.sh signs it again with its
# entitlements after the application's deep signature, then the
# application around it.
install(TARGETS mitcad-quicklook-thumbnails RUNTIME DESTINATION
  "${MITCAD_BUNDLE_DIR}/Contents/PlugIns/${mitcad_quicklook_appex_name}/Contents/MacOS")
install(FILES "${CMAKE_CURRENT_BINARY_DIR}/QuickLookThumbnails.plist" DESTINATION
  "${MITCAD_BUNDLE_DIR}/Contents/PlugIns/${mitcad_quicklook_appex_name}/Contents" RENAME Info.plist)

# The provider's reading code on designs with and without a preview, and
# the extension in the build's bundle (app/tests/quicklook_thumbnail_macos.mm).
if(BUILD_TESTING)
  add_executable(test_quicklook_thumbnail "${PROJECT_SOURCE_DIR}/app/tests/quicklook_thumbnail_macos.mm")
  target_compile_options(test_quicklook_thumbnail PRIVATE -fobjc-arc)
  target_link_libraries(test_quicklook_thumbnail PRIVATE mitcad_quicklook_thumbnail mitcad_warnings
    "-framework Foundation")
  add_dependencies(test_quicklook_thumbnail mitcad)
  add_test(NAME app.quicklook-thumbnails-macos
    COMMAND test_quicklook_thumbnail --work "${CMAKE_CURRENT_BINARY_DIR}/quicklook-thumbnail-test"
            --appex "$<TARGET_BUNDLE_DIR:mitcad>/Contents/PlugIns/${mitcad_quicklook_appex_name}")
  set_tests_properties(app.quicklook-thumbnails-macos PROPERTIES TIMEOUT 60)
endif()
