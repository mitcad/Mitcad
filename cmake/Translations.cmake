# SPDX-License-Identifier: MIT
# The interface languages (mitcad#1460, mitcad#1284; docs/translations.md).
# English is the source language. Every language of
# app/translations/languages.txt has its Qt Linguist file
# app/translations/mitcad_<code>.ts, which the build prepares
# (tools/translations: a context-free copy of each translated message added)
# and compiles with Qt's lrelease into <build>/translations/mitcad_<code>.qm,
# beside Qt's own qtbase_<code>.qm (Qt's standard dialogs and buttons) where
# Qt has one. A build tree copies them into translations/ beside mitcad, the
# packages carry them (cmake/Packaging.cmake: bin/translations,
# cmake/AppImage.cmake: share/mitcad/translations, a macOS bundle:
# Resources/translations), and app/framework/Language.cpp loads them at
# start-up. A test build adds the pseudo-language mitcad_xx.qm, made from the
# first language's file, for the tests only: it is never installed.
#
# Adding a language is data: its line in languages.txt and its .ts file
# (docs/translations.md). The target update_translations refreshes the .ts
# files from the sources with lupdate (tools/translations-update.cmake).
find_package(Qt6 REQUIRED COMPONENTS LinguistTools)

set(mitcad_ts_dir "${PROJECT_SOURCE_DIR}/app/translations")
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${mitcad_ts_dir}/languages.txt")
file(STRINGS "${mitcad_ts_dir}/languages.txt" mitcad_language_lines)
set(MITCAD_UI_LANGUAGES "")
foreach(line IN LISTS mitcad_language_lines)
  string(REGEX REPLACE "#.*" "" line "${line}")
  string(STRIP "${line}" line)
  if(line STREQUAL "")
    continue()
  endif()
  string(REGEX MATCH "^[^ \t]+" code "${line}")
  if(NOT EXISTS "${mitcad_ts_dir}/mitcad_${code}.ts")
    message(FATAL_ERROR "languages.txt lists ${code}, but app/translations/mitcad_${code}.ts is missing")
  endif()
  list(APPEND MITCAD_UI_LANGUAGES ${code})
endforeach()

set(mitcad_qm_dir "${CMAKE_BINARY_DIR}/translations")
set(mitcad_ts_work "${CMAKE_BINARY_DIR}/translations-work")
file(MAKE_DIRECTORY "${mitcad_qm_dir}" "${mitcad_ts_work}")
set(mitcad_qt_translations "${QT6_INSTALL_PREFIX}/${QT6_INSTALL_TRANSLATIONS}")

# mitcad_compile_catalogue(<ts> <model ts> <code>): <ts> prepared and
# compiled into mitcad_<code>.qm with <model ts>, the model's messages
# (mitcad#1284: model_<code>.ts, the context ModelMessage; empty when the
# language has none), which needs no context-free copy; the path appended to
# mitcad_catalogues.
function(mitcad_compile_catalogue ts model code)
  set(prepared "${mitcad_ts_work}/mitcad_${code}.ts")
  set(qm "${mitcad_qm_dir}/mitcad_${code}.qm")
  add_custom_command(
    OUTPUT "${qm}"
    COMMAND mitcad-translations prepare "${ts}" "${prepared}"
    COMMAND Qt6::lrelease -silent -nounfinished "${prepared}" ${model} -qm "${qm}"
    DEPENDS "${ts}" ${model} mitcad-translations
    COMMENT "Compiling the interface language ${code}"
    VERBATIM)
  set(mitcad_catalogues ${mitcad_catalogues} "${qm}" PARENT_SCOPE)
endfunction()

set(mitcad_catalogues "")
set(mitcad_shipped "")
foreach(code IN LISTS MITCAD_UI_LANGUAGES)
  set(model_ts "")
  if(EXISTS "${mitcad_ts_dir}/model_${code}.ts")
    set(model_ts "${mitcad_ts_dir}/model_${code}.ts")
  endif()
  mitcad_compile_catalogue("${mitcad_ts_dir}/mitcad_${code}.ts" "${model_ts}" ${code})
  list(APPEND mitcad_shipped "${mitcad_qm_dir}/mitcad_${code}.qm")
  # Qt's own catalogue of the language, as Qt ships it (BSD-3-Clause, Qt's
  # translations module), where there is one.
  if(EXISTS "${mitcad_qt_translations}/qtbase_${code}.qm")
    add_custom_command(
      OUTPUT "${mitcad_qm_dir}/qtbase_${code}.qm"
      COMMAND "${CMAKE_COMMAND}" -E copy_if_different "${mitcad_qt_translations}/qtbase_${code}.qm"
              "${mitcad_qm_dir}/qtbase_${code}.qm"
      DEPENDS "${mitcad_qt_translations}/qtbase_${code}.qm"
      VERBATIM)
    list(APPEND mitcad_catalogues "${mitcad_qm_dir}/qtbase_${code}.qm")
    list(APPEND mitcad_shipped "${mitcad_qm_dir}/qtbase_${code}.qm")
  else()
    message(STATUS "Qt has no qtbase_${code}.qm in ${mitcad_qt_translations}: Qt's own texts stay English in ${code}")
  endif()
endforeach()

# The pseudo-language of the tests (docs/translations.md): every text of the
# first language's file, accented, longer and in brackets.
if(BUILD_TESTING AND MITCAD_UI_LANGUAGES)
  list(GET MITCAD_UI_LANGUAGES 0 mitcad_first_language)
  set(mitcad_pseudo_ts "${mitcad_ts_work}/mitcad_xx.source.ts")
  add_custom_command(
    OUTPUT "${mitcad_pseudo_ts}"
    COMMAND mitcad-translations pseudo "${mitcad_ts_dir}/mitcad_${mitcad_first_language}.ts" "${mitcad_pseudo_ts}"
    DEPENDS "${mitcad_ts_dir}/mitcad_${mitcad_first_language}.ts" mitcad-translations
    COMMENT "Making the tests' pseudo-language"
    VERBATIM)
  # The model's messages too (mitcad#1284), so that a test sees any left
  # English.
  set(mitcad_pseudo_model_ts "")
  if(EXISTS "${mitcad_ts_dir}/model_${mitcad_first_language}.ts")
    set(mitcad_pseudo_model_ts "${mitcad_ts_work}/model_xx.source.ts")
    add_custom_command(
      OUTPUT "${mitcad_pseudo_model_ts}"
      COMMAND mitcad-translations pseudo "${mitcad_ts_dir}/model_${mitcad_first_language}.ts"
              "${mitcad_pseudo_model_ts}"
      DEPENDS "${mitcad_ts_dir}/model_${mitcad_first_language}.ts" mitcad-translations
      COMMENT "Making the tests' pseudo-language of the model's messages"
      VERBATIM)
  endif()
  mitcad_compile_catalogue("${mitcad_pseudo_ts}" "${mitcad_pseudo_model_ts}" xx)
endif()

add_custom_target(mitcad_translations ALL DEPENDS ${mitcad_catalogues})
add_dependencies(mitcad mitcad_translations)
# The packages install these (never the pseudo-language).
set_property(GLOBAL PROPERTY MITCAD_TRANSLATION_FILES "${mitcad_shipped}")

if(APPLE)
  # In the bundle's Resources/translations (macOS looks there:
  # Language.cpp's translationsFolder). Not the pseudo-language: the bundle
  # is what installs.
  foreach(qm IN LISTS mitcad_shipped)
    set_source_files_properties("${qm}" PROPERTIES GENERATED TRUE MACOSX_PACKAGE_LOCATION Resources/translations)
  endforeach()
  target_sources(mitcad PRIVATE ${mitcad_shipped})
elseif(mitcad_catalogues)
  # Beside mitcad, at every build (a custom target is always out of date,
  # so a catalogue changed without mitcad being linked again is copied too).
  add_custom_command(TARGET mitcad_translations POST_BUILD
    COMMAND "${CMAKE_COMMAND}" -E make_directory "$<TARGET_FILE_DIR:mitcad>/translations"
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different ${mitcad_catalogues} "$<TARGET_FILE_DIR:mitcad>/translations"
    VERBATIM)
endif()
if(BUILD_TESTING AND TARGET test_app)
  add_dependencies(test_app mitcad_translations)
  target_compile_definitions(test_app PRIVATE MITCAD_TEST_TRANSLATIONS_DIR="${mitcad_qm_dir}")
endif()

# Refreshes the .ts files from the sources (lupdate) and writes the counts of
# unfinished entries into languages.txt: cmake --build <build> --target
# update_translations.
# First the model's message templates are written again from core/
# (app/framework/ModelMessageTemplates.cpp, mitcad#1284), whose texts
# lupdate takes into the model_<code>.ts files.
add_custom_target(update_translations
  COMMAND "${CMAKE_COMMAND}" -DLUPDATE=$<TARGET_FILE:Qt6::lupdate> -DTOOL=$<TARGET_FILE:mitcad-translations>
          -DWORK=${mitcad_ts_work} -P "${PROJECT_SOURCE_DIR}/tools/translations-update.cmake"
  DEPENDS mitcad-translations
  COMMENT "Refreshing app/translations from the sources"
  VERBATIM)
