# SPDX-License-Identifier: MIT
# Verify the shared Rust/GUI cache path against the platform's actual Qt path.
target_sources(test_app_services PRIVATE tests/library_paths.cpp files/LibraryPaths.cpp)
target_link_libraries(test_app_services PRIVATE mitcad_ffi Qt6::Core)
add_test(NAME app.library_paths COMMAND test_app_services library_paths)
set_tests_properties(app.library_paths PROPERTIES
  ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
