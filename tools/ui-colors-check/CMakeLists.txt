# SPDX-License-Identifier: MIT
# Keeps the colors the user sees in the role table (docs/ui-conventions.md,
# "Colors"; mitcad#1041): no color literal in app/ outside
# app/framework/Theme.cpp and the recorded exceptions.
add_executable(mitcad-check-ui-colors check-ui-colors.cpp)
mitcad_utf8_code_page(mitcad-check-ui-colors)
target_link_libraries(mitcad-check-ui-colors PRIVATE mitcad_warnings)

add_test(NAME repo.ui-colors COMMAND mitcad-check-ui-colors "${PROJECT_SOURCE_DIR}/app")
# The scan finds the literals planted in it, and only those.
add_test(NAME repo.ui-colors-self-test COMMAND mitcad-check-ui-colors --self-test)
