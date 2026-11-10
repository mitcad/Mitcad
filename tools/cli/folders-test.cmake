# SPDX-License-Identifier: MIT
# A project's folders and designs (mitcad#144) through mitcad-cli: folders
# and designs made, listed, moved and deleted as versions, in text and
# JSON; every refusal (a target that is there, a path outside the project
# or hidden, a delete that would lose unrecorded work) with exit status 1
# and nothing changed; usage errors with 2. Where git is installed, a Cloud
# project (a bare repository as its remote) whose operations are pushed,
# unless --no-push.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<folder> [-DGIT=<git>] -P folders-test.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "folders-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(author "Mitcad Test <test@example.invalid>")
set(p "${WORK}/project")

function(cli expect)
  execute_process(COMMAND "${CLI}" ${ARGN} RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(expect STREQUAL "ok" AND NOT status EQUAL 0)
    message(FATAL_ERROR "mitcad-cli ${ARGN} failed (${status}):\n${out}${err}")
  elseif(expect STREQUAL "fail" AND NOT status EQUAL 1)
    message(FATAL_ERROR "mitcad-cli ${ARGN} should fail with 1 (${status}):\n${out}${err}")
  elseif(expect STREQUAL "usage" AND NOT status EQUAL 2)
    message(FATAL_ERROR "mitcad-cli ${ARGN} should be a usage error (${status}):\n${out}${err}")
  endif()
  set(out "${out}${err}" PARENT_SCOPE)
endfunction()

function(git dir)
  execute_process(COMMAND "${GIT}" -C "${dir}" ${ARGN} RESULT_VARIABLE status OUTPUT_VARIABLE out
                  ERROR_VARIABLE err OUTPUT_STRIP_TRAILING_WHITESPACE)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "git ${ARGN} failed (${status}):\n${out}${err}")
  endif()
  set(out "${out}" PARENT_SCOPE)
endfunction()

function(expect pattern what)
  if(NOT out MATCHES "${pattern}")
    message(FATAL_ERROR "${what}:\n${out}")
  endif()
endfunction()

function(absent pattern what)
  if(out MATCHES "${pattern}")
    message(FATAL_ERROR "${what}:\n${out}")
  endif()
endfunction()

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")

cli(ok project init "${p}" --author "${author}")

# A folder, a nested one, designs; the placeholder keeps empty folders.
cli(ok project mkdir "${p}" parts --author "${author}" --json)
expect("\"commit\":\"[0-9a-f]+\"" "project mkdir --json")
expect("\"error\":null" "project mkdir --json")
expect("\"path\":\"parts\"" "project mkdir --json")
if(NOT EXISTS "${p}/parts/.gitkeep")
  message(FATAL_ERROR "project mkdir did not write the folder's placeholder")
endif()
cli(ok project mkdir "${p}" parts/sub/deep --author "${author}")
expect("^Made the folder parts/sub/deep\nRecorded version [0-9a-f]+\n$" "project mkdir")
cli(ok project new-design "${p}" parts/a.mitcad --author "${author}" --message "A new bracket")
expect("^Made the design parts/a.mitcad\nRecorded version [0-9a-f]+\n$" "project new-design")
cli(ok info "${p}/parts/a.mitcad")
cli(ok history "${p}/parts/a.mitcad")
expect("A new bracket" "the version's message")

cli(ok project tree "${p}" --json)
expect("\"folders\":\\[\"parts\",\"parts/sub\",\"parts/sub/deep\"\\]" "project tree's folders")
expect("\"kind\":\"local\"" "project tree's kind")
expect("\"path\":\"parts/a.mitcad\",\"recorded\":true,\"version\":{\"author\":{[^}]*},\"date\":\"[^\"]*\",\"id\":\"[0-9a-f]+\",\"number\":1"
       "project tree's design")
absent("gitkeep" "the placeholder listed")
cli(ok project tree "${p}")
expect("^Local project [^\n]*project\n  parts/\n  parts/a.mitcad  v1 [0-9-]+ by Mitcad Test\n  parts/sub/\n  parts/sub/deep/\n$"
       "project tree as text")

# A folder moved: its design follows with its versions.
cli(ok project move "${p}" parts old --author "${author}")
expect("^Moved parts to old\nRecorded version [0-9a-f]+\n$" "project move")
if(EXISTS "${p}/parts" OR NOT EXISTS "${p}/old/a.mitcad" OR NOT EXISTS "${p}/old/sub/deep/.gitkeep")
  message(FATAL_ERROR "project move did not move the folder")
endif()
cli(ok project tree "${p}" --json)
expect("\"folders\":\\[\"old\",\"old/sub\",\"old/sub/deep\"\\]" "the folders after the move")
expect("\"path\":\"old/a.mitcad\",\"recorded\":true,\"version\":{[^}]*}[^}]*\"number\":2" "the moved design's versions")
cli(ok project move "${p}" old/a.mitcad old/b.mitcad --author "${author}" --json)
expect("\"moved\":\\[{\"from\":\"old/a.mitcad\",\"to\":\"old/b.mitcad\"}\\]" "project move --json")

# Refusals: nothing changes, exit status 1.
cli(fail project mkdir "${p}" old --author "${author}")
expect("old is there already" "project mkdir of a folder that is there")
cli(fail project mkdir "${p}" old --author "${author}" --json)
expect("\"class\":\"exists\"" "a refusal as JSON")
expect("\"commit\":null" "a refusal as JSON")
cli(fail project new-design "${p}" ../outside.mitcad --author "${author}")
expect("not a path inside the project" "a design outside the project")
if(EXISTS "${WORK}/outside.mitcad")
  message(FATAL_ERROR "a design was made outside the project")
endif()
cli(fail project move "${p}" old/b.mitcad .hidden/b.mitcad --author "${author}")
expect("hidden" "a move into a hidden folder")
cli(fail project move "${p}" old old/sub/old --author "${author}")
expect("into itself" "a folder moved into itself")
cli(fail project delete "${p}" missing.mitcad --author "${author}")
expect("is neither in the project's folder nor in its latest version" "a delete of nothing")
file(APPEND "${p}/old/b.mitcad" "\n")
cli(fail project delete "${p}" old/b.mitcad --author "${author}")
expect("save it as a version first, or delete with force" "a delete of unrecorded changes")
if(NOT EXISTS "${p}/old/b.mitcad")
  message(FATAL_ERROR "a refused delete deleted the design")
endif()

# Deleted with --force; the folder stays, kept by its placeholder.
cli(ok project delete "${p}" old/b.mitcad --force --author "${author}")
expect("^Deleted old/b.mitcad\nRecorded version [0-9a-f]+\n$" "project delete --force")
cli(ok project tree "${p}" --json)
expect("\"designs\":\\[\\]" "no designs after the delete")
expect("\"folders\":\\[\"old\"," "the folder stays after its design is deleted")
cli(ok project delete "${p}" old --author "${author}")
expect("^Deleted old\n" "project delete of a folder")

# Remove Project (mitcad#1562): the plan without --yes, nothing changed;
# then the project's files go and the designs stay as they were.
set(r "${WORK}/removed")
cli(ok project create "${r}" --author "${author}" --design a.mitcad)
file(READ "${r}/a.mitcad" design_text HEX)
file(READ "${r}/a.mitcad" design_copy)
file(WRITE "${r}/sub/b.mitcad" "${design_copy}")
cli(ok project remove "${r}")
expect("^Would remove the project [^\n]*removed \\(nothing changed: add --yes\\)\n2 designs stay in the folder\nVersion history: deleted with .git \\(1 version\\)\nRemoved: .git, .mitcad, .gitattributes, .gitignore\n$"
       "project remove without --yes")
if(NOT EXISTS "${r}/.mitcad/project.json" OR NOT EXISTS "${r}/.git")
  message(FATAL_ERROR "project remove without --yes changed the folder")
endif()
cli(ok project remove "${r}" --yes --json)
expect("\"repository\":\"removed\"" "project remove --yes --json")
expect("\"error\":null" "project remove --yes --json")
foreach(gone .git .mitcad .gitattributes .gitignore)
  if(EXISTS "${r}/${gone}")
    message(FATAL_ERROR "project remove left ${gone}")
  endif()
endforeach()
file(READ "${r}/a.mitcad" after HEX)
if(NOT after STREQUAL design_text OR NOT EXISTS "${r}/sub/b.mitcad")
  message(FATAL_ERROR "project remove changed the designs")
endif()
cli(fail project remove "${r}")
expect("is not a MitCad project" "project remove of a folder that is no project")
cli(usage project create "${r}" --yes)

# Usage errors.
cli(usage project move "${p}" old)
cli(usage project tree "${p}" --author "${author}")
cli(usage project mkdir "${p}" new --force)

if(NOT GIT)
  message(STATUS "folders-test: git is not installed, the Cloud project is skipped")
  return()
endif()

# A Cloud project: each operation is pushed, unless --no-push.
git("${WORK}" init -q --bare remote.git)
cli(ok remote add "${p}" "${WORK}/remote.git" --author "${author}")
cli(ok project mkdir "${p}" shared --author "${author}")
expect("Recorded version [0-9a-f]+\nSent to origin\n" "project mkdir in a Cloud project")
git("${p}" rev-parse HEAD)
set(head "${out}")
git("${WORK}/remote.git" rev-parse refs/heads/main)
if(NOT out STREQUAL head)
  message(FATAL_ERROR "the remote's main is ${out}, not ${head}")
endif()
cli(ok project new-design "${p}" shared/c.mitcad --author "${author}" --no-push --json)
expect("\"push\":null" "project new-design --no-push")
cli(ok project tree "${p}" --json)
expect("\"ahead\":1,\"behind\":0," "the version waiting to be sent")
expect("\"kind\":\"cloud\"" "a Cloud project's kind")
git("${p}" status --porcelain)
if(NOT out STREQUAL "")
  message(FATAL_ERROR "git status is not clean:\n${out}")
endif()
