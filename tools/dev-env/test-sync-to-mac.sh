#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Local SSH stand-in: changed old sources get fresh times, unchanged sources
# keep theirs, deletions propagate, and the guest's build/preset survive.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT HUP INT TERM
mkdir -p "$fixture/source/tools/dev-env" "$fixture/guest/build" "$fixture/bin"
cp "$here/sync-to-mac.sh" "$fixture/source/tools/dev-env/"
git -C "$fixture/source" init -q
printf 'unchanged\n' > "$fixture/source/stable.cpp"
printf 'before\n' > "$fixture/source/changed.cpp"
printf 'remove me\n' > "$fixture/source/deleted.cpp"
printf 'keep build\n' > "$fixture/guest/build/object.o"
printf 'keep preset\n' > "$fixture/guest/CMakeUserPresets.json"
cat > "$fixture/bin/ssh" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
for argument in "$@"; do command_text=$argument; done
exec /bin/bash -c "$command_text"
MOCK
chmod +x "$fixture/bin/ssh"
export PATH="$fixture/bin:$PATH"
bash "$fixture/source/tools/dev-env/sync-to-mac.sh" fixture "$fixture/guest"
touch -t 200001010000 "$fixture/guest/stable.cpp"
touch -r "$fixture/guest/stable.cpp" "$fixture/stable-time"
touch -t 200101010000 "$fixture/guest/build/object.o"
printf 'after\n' > "$fixture/source/changed.cpp"
touch -t 199901010000 "$fixture/source/changed.cpp"
rm "$fixture/source/deleted.cpp"
bash "$fixture/source/tools/dev-env/sync-to-mac.sh" fixture "$fixture/guest"
cmp "$fixture/source/changed.cpp" "$fixture/guest/changed.cpp"
test "$fixture/guest/changed.cpp" -nt "$fixture/guest/build/object.o"
test ! "$fixture/guest/stable.cpp" -nt "$fixture/stable-time"
test ! "$fixture/stable-time" -nt "$fixture/guest/stable.cpp"
test ! -e "$fixture/guest/deleted.cpp"
test "$(cat "$fixture/guest/build/object.o")" = 'keep build'
test "$(cat "$fixture/guest/CMakeUserPresets.json")" = 'keep preset'
echo 'macOS sync content and timestamp regression passed'
