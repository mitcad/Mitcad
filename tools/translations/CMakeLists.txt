# SPDX-License-Identifier: MIT
# The interface languages' catalogues (mitcad#1460, docs/translations.md):
# the build prepares each app/translations/mitcad_<code>.ts for lrelease and
# makes the tests' pseudo-language with this tool (cmake/Translations.cmake);
# the ctest repo.translations checks the .ts files against their baselines.
add_executable(mitcad-translations mitcad-translations.cpp)
mitcad_utf8_code_page(mitcad-translations)
target_link_libraries(mitcad-translations PRIVATE mitcad_warnings)
# Plain C++: nothing of Qt's to generate.
set_target_properties(mitcad-translations PROPERTIES AUTOMOC OFF AUTOUIC OFF AUTORCC OFF)

if(BUILD_TESTING)
  add_test(NAME repo.translations
           COMMAND mitcad-translations check "${PROJECT_SOURCE_DIR}/app/translations/languages.txt")
  # The scan and the pseudo-language on planted catalogues.
  add_test(NAME repo.translations-self-test COMMAND mitcad-translations --self-test)
  # Every message template of core/'s Rust sources is in the generated list
  # app/framework/ModelMessageTemplates.cpp, and so in the model_<code>.ts
  # files, and none that is gone (mitcad#1284).
  add_test(NAME repo.model-messages COMMAND mitcad-translations messages "${PROJECT_SOURCE_DIR}")
endif()
