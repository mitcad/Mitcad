# SPDX-License-Identifier: MIT
# Refreshes the interface languages' files app/translations/mitcad_<code>.ts
# from the app's sources, and model_<code>.ts from the model's message
# templates (mitcad#1284: app/framework/ModelMessageTemplates.cpp, written
# again from core/ first), with Qt's lupdate (mitcad#1460,
# docs/translations.md): new texts come in as unfinished, texts no longer in
# the sources go (-no-obsolete), no source locations are kept (they would
# change with every edit). Then each language's count of unfinished entries
# is written into app/translations/languages.txt (repo.translations' baseline).
#
#   cmake --build <build> --target update_translations
#   cmake -DLUPDATE=<Qt>/bin/lupdate -DTOOL=<build>/tools/translations/mitcad-translations
#         -DWORK=<folder> [-DADD_LANGUAGE=<code>] -P tools/translations-update.cmake
#
# ADD_LANGUAGE starts a new language: its line in languages.txt and its .ts
# file with every text unfinished. English's line (en, mitcad#1756) keeps
# only the plural messages (lupdate -pluralonly): new ones come in
# unfinished, and their two English forms are written by hand.
cmake_minimum_required(VERSION 3.24)
foreach(variable LUPDATE TOOL WORK)
  if(NOT ${variable})
    message(FATAL_ERROR "${variable} is not set (see the header of this file)")
  endif()
endforeach()
get_filename_component(root "${CMAKE_CURRENT_LIST_DIR}/.." ABSOLUTE)
set(folder "${root}/app/translations")
set(list "${folder}/languages.txt")

# The sources: the app without its tests and the paused AppKit frontend
# (docs/macos-shared-qt.md), whose texts return with it. The Objective-C++
# files of the shared UI (the macOS render client) are read as C++
# (mitcad#1474); the render service's own processes show no texts.
file(GLOB_RECURSE sources "${root}/app/*.cpp" "${root}/app/*.hpp" "${root}/app/*.h" "${root}/app/*.mm")
list(FILTER sources EXCLUDE REGEX "/app/(tests|platform/macos)/")
list(FILTER sources EXCLUDE REGEX "/app/render/Mac(RenderService|FrameXpcProbeService)\\.mm$")
# The model's messages (mitcad#1284) have files of their own, model_<code>.ts
# (below): their templates, written again from core/'s Rust sources first.
set(templates "${root}/app/framework/ModelMessageTemplates.cpp")
list(FILTER sources EXCLUDE REGEX "/app/framework/ModelMessageTemplates\\.cpp$")
execute_process(COMMAND "${TOOL}" messages "${root}" --write RESULT_VARIABLE status)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "the model's message templates were not written (${status})")
endif()
list(SORT sources)
file(MAKE_DIRECTORY "${WORK}")
string(REPLACE ";" "\n" lines "${sources}")
file(WRITE "${WORK}/lupdate-sources.lst" "${lines}\n")

if(ADD_LANGUAGE)
  if(NOT ADD_LANGUAGE MATCHES "^[a-z][a-z][a-z]?(_[A-Z][A-Z])?$")
    message(FATAL_ERROR "ADD_LANGUAGE=${ADD_LANGUAGE}: a language code such as de or pt_BR")
  endif()
  file(STRINGS "${list}" listed REGEX "^${ADD_LANGUAGE}[ \t]")
  if(NOT listed)
    file(APPEND "${list}" "${ADD_LANGUAGE} 0\n")
  endif()
endif()

file(STRINGS "${list}" lines)
foreach(line IN LISTS lines)
  string(REGEX REPLACE "#.*" "" line "${line}")
  string(STRIP "${line}" line)
  if(line STREQUAL "")
    continue()
  endif()
  string(REGEX MATCH "^[^ \t]+" code "${line}")
  message(STATUS "lupdate: mitcad_${code}.ts")
  # English (mitcad#1756): its plural forms only; the model's messages
  # are English already.
  set(plural_only "")
  if(code STREQUAL "en")
    set(plural_only -pluralonly)
  endif()
  execute_process(
    COMMAND "${LUPDATE}" "@${WORK}/lupdate-sources.lst" -I "${root}/app" -no-obsolete -locations none
            ${plural_only} -source-language en -target-language ${code} -silent -ts "${folder}/mitcad_${code}.ts"
    RESULT_VARIABLE status)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "lupdate failed for ${code} (${status})")
  endif()
  if(code STREQUAL "en")
    continue()
  endif()
  message(STATUS "lupdate: model_${code}.ts")
  execute_process(
    COMMAND "${LUPDATE}" "${templates}" -no-obsolete -locations none -source-language en -target-language ${code}
            -silent -ts "${folder}/model_${code}.ts"
    RESULT_VARIABLE status)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "lupdate failed for model_${code} (${status})")
  endif()
endforeach()

execute_process(COMMAND "${TOOL}" baseline "${list}" RESULT_VARIABLE status)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "the catalogues have problems (above); the baselines are not written")
endif()
