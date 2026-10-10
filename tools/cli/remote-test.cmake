# SPDX-License-Identifier: MIT
# Remote repositories (P12 remote) through mitcad-cli and the system's git:
# a project connected to an empty bare repository, opened from it into a
# second folder, versions pushed and fetched both ways, a push the remote
# refuses (nothing is forced), syncs with and without a file changed on
# both sides, edit locks taken, held and released, a URL with credentials
# refused, and the remote removed. Local and Cloud projects (mitcad#89): a
# remote checked without a project, a project made beside a remote's files,
# its settings changed and sent, a repository with files adopted, a Local
# project shared onto a remote's files, and an SSH server's host key
# trusted (with a fake ssh-keyscan).
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

# Edit locks (mitcad#89): A takes the design's lock, B finds it held and
# cannot release it; A releases it, B takes it, --force removes it.
set(other "Other Tester <other@example.invalid>")
cli(ok lock take "${a}/part.mitcad" --author "${author}")
expect("^Edit lock of part.mitcad taken\n$" "lock take")
cli(fail lock take "${b}/part.mitcad" --author "${other}")
expect("^part.mitcad is being edited by Mitcad Test <test@example.invalid> \\(since [^\n]*\\)\n$"
       "lock take of a held lock")
cli(ok lock status "${b}")
expect("^part.mitcad: edited by Mitcad Test <test@example.invalid> since [^\n]*\n  session [0-9a-f-]+, Mitcad [^\n]*\n$"
       "lock status")
cli(ok lock status "${b}/part.mitcad" --json)
expect("\"owner\":{\"email\":\"test@example.invalid\",\"name\":\"Mitcad Test\"}" "lock status as JSON")
cli(fail lock release "${b}/part.mitcad" --author "${other}")
expect("is held by Mitcad Test <test@example.invalid>, not this session: --force removes it"
       "lock release of another's lock")
cli(ok lock release "${a}/part.mitcad" --author "${author}")
expect("^Released the edit lock of part.mitcad\n$" "lock release")
cli(ok lock take "${b}/part.mitcad" --author "${other}")
cli(ok lock release "${a}" --force)
expect("^Released the edit lock of part.mitcad\n$" "lock release --force of a project")
git("${remote}" for-each-ref refs/mitcad/)
if(NOT out STREQUAL "")
  message(FATAL_ERROR "lock refs left on the remote:\n${out}")
endif()

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

# Several remotes, none followed (mitcad#89): remote follow chooses one.
git("${a}" remote add first "${remote}")
git("${a}" remote add second "${remote}")
cli(ok remote show "${a}")
expect("Remotes first \\(.*\\), second \\(.*\\): none is followed" "remote show with several remotes")
cli(fail remote follow "${a}" nowhere)
expect("has no remote 'nowhere'" "remote follow of a remote that is not there")
cli(ok remote follow "${a}" second)
expect("^Branch main follows second/main \\(" "remote follow")
cli(ok remote show "${a}")
expect("Remote second: " "remote show of the remote followed")
cli(ok remote remove "${a}" --name first)
cli(ok remote remove "${a}" --name second)

# Local and Cloud projects (mitcad#89): remotes with a README but no
# project (the same history in three bare repositories).
git("${WORK}" -c init.defaultBranch=main init -q readme-work)
file(WRITE "${WORK}/readme-work/README.md" "# Robot arm\n")
git("${WORK}/readme-work" add README.md)
git("${WORK}/readme-work" -c user.name=Git -c user.email=git@example.invalid commit -q -m "Initial files")
foreach(name readme files share)
  git("${WORK}" init -q --bare ${name}.git)
  git("${WORK}/readme-work" push -q "${WORK}/${name}.git" main:main)
  git("${WORK}/${name}.git" symbolic-ref HEAD refs/heads/main)
endforeach()
# What a remote holds, without a project.
cli(ok remote check "${WORK}/readme.git")
expect("Branch main \\([0-9a-f]+\\): no Mitcad project\n1 version, latest [0-9-]+ by Git\nFiles: README.md\n"
       "remote check without a project")
cli(ok remote check "${remote}" --json)
expect("\"has_project\":true" "remote check of a project as JSON")
# New Project, Cloud: the project beside the remote's files.
cli(ok project inspect "${WORK}/c")
expect("c: does not exist\n" "project inspect of a missing folder")
cli(ok project create "${WORK}/c" --author "${author}" --url "${WORK}/readme.git" --design c.mitcad)
expect("^Created the project [^\n]*c\nThe remote's files were taken in: the project is beside them\nRecorded version [0-9a-f]+\nRemote origin: [^\n]*readme.git\nRecorded: .gitattributes, .gitignore, .mitcad/project.json, c.mitcad\nPushed to the remote\n$"
       "project create beside a README")
if(NOT EXISTS "${WORK}/c/README.md")
  message(FATAL_ERROR "the remote's README is not in the project")
endif()
cli(ok project inspect "${WORK}/c")
expect(": a Cloud project\nRemote origin: [^\n]*readme.git \\(branch main follows origin/main\\)\nDesigns: c.mitcad\nEdit locks: on \\(idle time 10 min, poll 10 s\\)\nLive updates: none\n"
       "project inspect of a Cloud project")
cli(fail project create "${WORK}/d" --author "${author}" --url "${remote}")
expect("holds a Mitcad project" "project create on a remote with a project")
# A shared setting is recorded as a version and sent with sync.
cli(ok project settings "${WORK}/c" --set "{\"shared\": {\"edit_locks\": {\"enabled\": false}}}"
    --author "${author}")
expect("^Edit locks: off\n.*Recorded version [0-9a-f]+ \\(send it with mitcad-cli sync\\)\n$" "project settings --set")
cli(ok sync "${WORK}/c")
expect("Pushed 1 version to origin/main\n" "sync of the settings")
cli(ok project settings "${WORK}/c" --json)
expect("\"kind\":\"cloud\"" "project settings as JSON")
expect("\"enabled\":false" "project settings as JSON")
# Open from Cloud: a repository with files made a project.
cli(fail clone "${WORK}/files.git" "${WORK}/d")
expect("holds no Mitcad project" "clone of a repository without a project")
cli(ok clone "${WORK}/files.git" "${WORK}/d" --adopt --author "${author}")
expect("^Opened [^\n]*d\nThe remote's files were taken in: the project is beside them\nRecorded version [0-9a-f]+\nPushed to the remote\n$"
       "clone --adopt")
git("${WORK}/d" log -1 --format=%s)
if(NOT out STREQUAL "Make this repository a Mitcad project")
  message(FATAL_ERROR "the adopted repository's version: ${out}")
endif()
# Project Settings, Local to Cloud: shared onto the remote's files.
cli(ok project create "${WORK}/e" --author "${author}" --design e.mitcad --json)
expect("\"remote\":null" "project create of a Local project")
expect("\"error\":null" "project create of a Local project")
cli(ok remote share "${WORK}/e" "${WORK}/share.git" --author "${author}")
expect("Replayed 1 version after the newer ones on origin/main\n.*Pushed 1 version to origin/main\nBranch main follows origin/main: up to date\n"
       "remote share onto a README")
if(NOT EXISTS "${WORK}/e/README.md")
  message(FATAL_ERROR "the shared project lacks the remote's README")
endif()
foreach(project "${WORK}/c" "${WORK}/d" "${WORK}/e")
  git("${project}" status --porcelain)
  if(NOT out STREQUAL "")
    message(FATAL_ERROR "files changed in ${project}:\n${out}")
  endif()
  git("${project}" fsck --strict --no-progress --no-dangling)
endforeach()
foreach(name readme files share)
  git("${WORK}/${name}.git" fsck --strict --no-progress)
endforeach()
# SSH host keys, with a fake ssh-keyscan (a shell script) and a home of
# the test's own.
if(NOT WIN32)
  file(WRITE "${WORK}/keyscan.txt"
       "github.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl\n")
  file(WRITE "${WORK}/ssh-keyscan" "#!/bin/sh\ncat '${WORK}/keyscan.txt'\n")
  file(CHMOD "${WORK}/ssh-keyscan" PERMISSIONS OWNER_READ OWNER_WRITE OWNER_EXECUTE)
  execute_process(COMMAND "${CMAKE_COMMAND}" -E env "HOME=${WORK}/home" "MITCAD_SSH_KEYSCAN=${WORK}/ssh-keyscan"
                          "${CLI}" host-keys github.com --trust
                  RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "mitcad-cli host-keys failed (${status}):\n${out}${err}")
  endif()
  expect("^SSH host keys of github.com \\(port 22\\):\n  ssh-ed25519 SHA256:\\+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU\nVerified: the keys GitHub publishes\nTrusted: the key was added to [^\n]*known_hosts\n$"
         "host-keys --trust")
  file(READ "${WORK}/home/.ssh/known_hosts" known)
  if(NOT known MATCHES "^github.com ssh-ed25519 AAAAC3")
    message(FATAL_ERROR "known_hosts: ${known}")
  endif()
endif()
message(STATUS "remote repositories: connected, opened, pushed and fetched both ways, a push refused; "
               "projects made beside a remote's files, adopted and shared")
