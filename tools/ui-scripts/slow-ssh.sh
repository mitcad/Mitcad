# SPDX-License-Identifier: MIT
# A slow server for the UI tests of mitcad#1895 (tools/ui-slow-remote-test.sh
# and its Windows and macOS wrappers): git's ssh, GIT_SSH_COMMAND='sh
# "<this file>"' with GIT_SSH_VARIANT=ssh, that waits SLOW_SSH_SECONDS before
# each connection and then runs the command git gives it ("git-upload-pack
# '/shop.git'") in the folder SLOW_SSH_ROOT, as the server would. The
# repositories are local and synthetic: nothing goes to a network.
for last; do :; done
sleep "${SLOW_SSH_SECONDS:-0}"
cd "$SLOW_SSH_ROOT" || exit 1
command=$(printf '%s' "$last" | sed "s#'/#'#")
eval "git ${command#git-}"
