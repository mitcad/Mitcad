# SPDX-License-Identifier: MIT
# Remote repositories (P12 remote) through mitcad-cli and the system's git:
# a project connected to an empty bare repository, opened from it into a
# second folder, versions pushed and fetched both ways, a push the remote
# refuses (nothing is forced), syncs with and without a file changed on
# both sides, a URL with credentials refused, and the remote removed.
# Nothing goes over the network.
#
# cmake -DCLI=<mitcad-cli> -DGIT=<git> -DSOURCE=<file.mitcad with d3>
#       -DWORK=<folder> -P remote-test.cmake

foreach(var CLI GIT SOURCE WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "remote-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(author "Mitcad Test <test@example.invalid>")
set(remote "${WORK}/remote.git")
set(a "${WORK}/a")
set(b "${WORK}/b")

function(cli expect)
  execute_process(COMMAND "${CLI}" ${ARGN} RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(expect STREQUAL "ok" AND NOT status EQUAL 0)
    message(FATAL_ERROR "mitcad-cli ${ARGN} failed (${status}):\n${out}${err}")
  elseif(expect STREQUAL "fail" AND NOT status EQUAL 1)
    message(FATAL_ERROR "mitcad-cli ${ARGN} should fail with 1 (${status}):\n${out}${err}")
  elseif(expect STREQUAL "conflict" AND NOT status EQUAL 3)
    message(FATAL_ERROR "mitcad-cli ${ARGN} should stop with 3 (${status}):\n${out}${err}")
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

# A new value of d3 in a project file, recorded as a version.
function(change file d3)
  file(WRITE "${WORK}/d3.json" "[{\"cmd\": \"set_parameter\", \"name\": \"d3\", \"value\": ${d3}}]")
  cli(ok run "${WORK}/d3.json" --open "${file}" --save "${file}")
  cli(ok version save "${file}" -m "d3 = ${d3} mm" --author "${author}")
endfunction()

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
git("${WORK}" init -q --bare remote.git)

# A: a project with a version of the design.
cli(ok project init "${a}" --author "${author}")
cli(ok convert "${SOURCE}" "${a}/part.mitcad" --format v3)
cli(ok version save "${a}/part.mitcad" -m "First version" --author "${author}")

# Connected to the empty remote: its versions are pushed.
cli(ok remote check "${a}" "${remote}")
expect("The repository is empty" "remote check of the empty remote")
cli(ok remote add "${a}" "${remote}" --author "${author}")
expect("^Remote origin set to [^\n]*remote.git; branch main follows origin/main\nPushed to origin/main\nBranch main follows origin/main: up to date\n$"
       "remote add")
cli(ok remote show "${a}")
expect("^git [0-9][^\n]*\nRemote origin: [^\n]*remote.git\nBranch main follows origin/main: up to date\nLast push: [^\n]* \\(ok\\)\n$"
       "remote show")
cli(ok push "${a}")
expect("^Nothing to push: origin/main has every version\n$" "push without new versions")
git("${a}" rev-parse HEAD)
set(first "${out}")
git("${remote}" rev-parse refs/heads/main)
if(NOT out STREQUAL first)
  message(FATAL_ERROR "the remote's main is ${out}, not ${first}")
endif()

# B: the project opened from the remote.
cli(ok clone "${remote}" "${b}")
expect("^Opened [^\n]*remote.git into [^\n]*b: branch main, latest version [0-9a-f]+\nProject files: part.mitcad\n$"
       "clone")
cli(ok info "${a}/part.mitcad")
set(report_a "${out}")
cli(ok info "${b}/part.mitcad")
if(NOT out STREQUAL report_a)
  message(FATAL_ERROR "the opened project differs:\n${out}\nthe original:\n${report_a}")
endif()

# B records a version and pushes it; A fetches it.
change("${b}/part.mitcad" 25)
cli(ok push "${b}")
expect("^Pushed 1 version to origin/main\n$" "push of B's version")
cli(ok fetch "${a}" --json)
expect("\"ahead\":0,\"behind\":1,.*\"error\":null" "fetch as JSON")
cli(ok remote show "${a}")
expect("Branch main follows origin/main: 1 version newer on the remote\nLast fetch: [^\n]* \\(ok\\)\n"
       "remote show after the fetch")

# A records a version too: the remote refuses its push, nothing is forced.
change("${a}/part.mitcad" 30)
git("${remote}" rev-parse refs/heads/main)
set(remote_main "${out}")
cli(fail push "${a}")
expect("mitcad-cli: Pushing to origin/main failed: the remote has versions this project does not have yet"
       "push refused")
cli(fail push "${a}" --json)
expect("\"class\":\"rejected\"" "push refused as JSON")
git("${remote}" rev-parse refs/heads/main)
if(NOT out STREQUAL remote_main)
  message(FATAL_ERROR "the refused push moved the remote's main")
endif()
cli(ok remote show "${a}" --json)
expect("\"ahead\":1,\"behind\":1" "remote show with versions on both sides")

# Sync: A's version and B's changed the same file. Without a choice nothing
# changes; with "copy" A's design is saved next to B's.
git("${a}" rev-parse HEAD)
set(a_head "${out}")
cli(ok sync "${a}" --dry-run)
expect("Sync will replay 1 version of this project after 1 version newer on origin/main\nConflict: part.mitcad: changed on both sides\n  mine:   [0-9a-f]+ [^\n]* Mitcad Test: d3 = 30 mm\n  theirs: [0-9a-f]+ [^\n]* Mitcad Test: d3 = 25 mm\n  copy:   part \\(conflict copy Mitcad Test [0-9-]+ [0-9.]+\\)\\.mitcad\n"
       "sync --dry-run with a file changed on both sides")
cli(conflict sync "${a}")
expect("Conflict: part.mitcad: changed on both sides\n.*Choose for each file: --resolve <path>=mine\\|theirs\\|copy.*\nSync stopped: a file changed both here and on the remote: part.mitcad\\."
       "sync without a choice")
cli(conflict sync "${a}" --json)
expect("\"class\":\"conflict\"" "sync without a choice as JSON")
git("${a}" rev-parse HEAD)
if(NOT out STREQUAL a_head)
  message(FATAL_ERROR "a sync without a choice moved the branch")
endif()
cli(ok sync "${a}" --resolve part.mitcad=copy --author "${author}")
expect("Replayed 1 version after the newer ones on origin/main\nSaved mine as a copy: part \\(conflict copy Mitcad Test [^)]*\\)\\.mitcad \\(theirs: part.mitcad\\)\nVersions before the sync kept in refs/mitcad/sync-backup/[0-9-]+\nFiles updated: [^\n]*part.mitcad\nPushed 1 version to origin/main\nBranch main follows origin/main: up to date\n$"
       "sync with the copy")
file(GLOB copies "${a}/part (conflict copy *).mitcad")
list(LENGTH copies count)
if(NOT count EQUAL 1)
  message(FATAL_ERROR "no copy of A's design: ${copies}")
endif()
list(GET copies 0 copy)
cli(ok history "${copy}")
expect("Versions of part \\(conflict copy [^\n]*\\)\\.mitcad \\(1\\), newest first:\n  v1 [^\n]* d3 = 30 mm\n"
       "the copy's history")
cli(ok info "${a}/part.mitcad")
set(report_a "${out}")
cli(ok info "${b}/part.mitcad")
if(NOT out STREQUAL report_a)
  message(FATAL_ERROR "A's design is not B's after taking theirs:\n${report_a}")
endif()
# B takes it.
cli(ok sync "${b}")
expect("Files updated: part \\(conflict copy [^\n]*\nBranch main follows origin/main: up to date\n$" "sync of B")

# Both change other files: the sync replays A's version without a question.
change("${copy}" 35)
change("${b}/part.mitcad" 40)
cli(ok sync "${b}")
expect("Pushed 1 version to origin/main\n" "sync of B's version")
cli(ok sync "${a}" --json --author "${author}")
foreach(field "\"case\":\"replay\"" "\"conflicts\":\\[\\]" "\"error\":null" "\"pushed\":true")
  expect("${field}" "sync without conflicts")
endforeach()
cli(ok sync "${b}" --no-push)
cli(ok sync "${a}")
expect("Branch main follows origin/main: up to date\n$" "sync when up to date")
git("${a}" rev-parse HEAD)
set(a_head "${out}")
git("${b}" rev-parse HEAD)
if(NOT out STREQUAL a_head)
  message(FATAL_ERROR "A and B differ after the syncs: ${a_head} and ${out}")
endif()
foreach(project "${a}" "${b}")
  git("${project}" status --porcelain)
  if(NOT out STREQUAL "")
    message(FATAL_ERROR "files changed in ${project} after the syncs:\n${out}")
  endif()
  git("${project}" fsck --strict --no-progress --no-dangling)
endforeach()

# A URL with credentials is refused; a project opens only into a new folder.
cli(fail clone "https://user:s3cret@example.invalid/x.git" "${WORK}/c")
expect("holds credentials" "clone with credentials")
if(out MATCHES "s3cret" OR EXISTS "${WORK}/c")
  message(FATAL_ERROR "the credentials were shown or a folder was made:\n${out}")
endif()
cli(fail clone "${remote}" "${b}")
expect("is not an empty folder" "clone into a project")

# The remote removed.
cli(ok remote remove "${a}")
expect("^Remote origin removed\n$" "remote remove")
cli(ok remote show "${a}")
expect("No remote repository" "remote show without a remote")
git("${remote}" fsck --strict --no-progress)
message(STATUS "remote repositories: connected, opened, pushed and fetched both ways, a push refused")
