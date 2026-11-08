# SPDX-License-Identifier: MIT
# Keeps the user-facing texts to the UI conventions (docs/ui-conventions.md,
# "The check"): the ellipsis, MitCad, menu paths with "›", no retired terms
# and no British spelling, Title Case's short words, "×" and "°", AppKit
# texts through tr(), in the app's texts, the user guide and the README; and
# the quoting of values in the core's Rust messages. The commands'
# developer document (app/COMMANDS.md) is read for the British spellings
# only (mitcad#1478).
add_executable(mitcad-check-ui-strings check-ui-strings.cpp)
mitcad_utf8_code_page(mitcad-check-ui-strings)
target_link_libraries(mitcad-check-ui-strings PRIVATE mitcad_warnings)

add_test(NAME repo.ui-strings
         COMMAND mitcad-check-ui-strings "${PROJECT_SOURCE_DIR}/app" "${PROJECT_SOURCE_DIR}/core"
                 "${PROJECT_SOURCE_DIR}/docs/user-guide.md" "${PROJECT_SOURCE_DIR}/README.md"
                 --spelling "${PROJECT_SOURCE_DIR}/app/COMMANDS.md")
# The scan finds the texts planted in it, and only those.
add_test(NAME repo.ui-strings-self-test COMMAND mitcad-check-ui-strings --self-test)
