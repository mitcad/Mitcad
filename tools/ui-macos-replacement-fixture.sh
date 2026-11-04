# SPDX-License-Identifier: MIT
# Local-only fixtures for native document replacement (mitcad#131).
# Source from ui-macos-native-test.sh after ui_init; never run on the host.

UI_REPLACEMENT_PID=""

ui_replacement_cleanup() {
  if [ -n "$UI_REPLACEMENT_PID" ]; then
    kill "$UI_REPLACEMENT_PID" 2> /dev/null || true
    wait "$UI_REPLACEMENT_PID" 2> /dev/null || true
    UI_REPLACEMENT_PID=""
  fi
}

ui_replacement_prepare() {
  local cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/../../../.." && pwd)/tools/cli/mitcad-cli}
  if [ ! -x "$cli" ] || ! command -v git > /dev/null; then
    ui_fail 'replacement test requires mitcad-cli and git'
    return 1
  fi
  export MITCAD_TEST_REPLACEMENT=$MITCAD_TEST_WORK/replacement
  export MITCAD_TEST_AUTOSAVE_SECONDS=2
  # Exercise replacement explicitly. The multi-document default keeps the
  # original design open in another window and has no save/lock questions.
  # This is the test's private INI store, not the user's preferences.
  mkdir -p "$MITCAD_SETTINGS_DIR/nocodo.fi" || return 1
  printf '\n[documents]\nopenInCurrentWindow=true\n' >> "$MITCAD_SETTINGS_DIR/nocodo.fi/Mitcad.ini"
  # Remove only this test's fixtures so retries cannot use old barriers.
  rm -rf "$MITCAD_TEST_REPLACEMENT"
  mkdir -p "$MITCAD_TEST_REPLACEMENT" || return 1
  local fixture=$MITCAD_TEST_REPLACEMENT seed=$MITCAD_TEST_REPLACEMENT/seed
  "$cli" project init "$seed" --author 'Ada Tester <ada@example.invalid>' > "$UI_OUT/replacement-setup.log" 2>&1 || return 1
  python3 - "$here/cli/tests/v1_boss.mitcad" "$seed" << 'PY'
import json, os, sys
source, folder = sys.argv[1:]
with open(source) as stream:
    document = json.load(stream)
# A 60 x 40 x 20 extrusion, changed to 27 in the running app.
document['features'] = document['features'][:2]
for name in ('part.mitcad', 'other.mitcad'):
    with open(os.path.join(folder, name), 'w') as stream:
        json.dump(document, stream, indent=2)
        stream.write('\n')
marker_path = os.path.join(folder, '.mitcad', 'project.json')
with open(marker_path) as stream:
    marker = json.load(stream)
marker['edit_locks'] = {'enabled': True, 'idle_minutes': 60, 'poll_seconds': 60}
with open(marker_path, 'w') as stream:
    json.dump(marker, stream, indent=2)
    stream.write('\n')
PY
  [ "$?" = 0 ] || return 1
  git -C "$seed" -c user.name='Ada Tester' -c user.email=ada@example.invalid add -A || return 1
  git -C "$seed" -c user.name='Ada Tester' -c user.email=ada@example.invalid commit -qm 'Add replacement test designs' || return 1
  git init -q --bare "$fixture/remote.git" || return 1
  git -C "$seed" push -q "$fixture/remote.git" HEAD:main || return 1
  git --git-dir="$fixture/remote.git" symbolic-ref HEAD refs/heads/main || return 1
  git clone -q "$fixture/remote.git" "$fixture/ada" || return 1
  git clone -q "$fixture/remote.git" "$fixture/sam" || return 1
  git -C "$fixture/ada" config user.name 'Ada Tester' || return 1
  git -C "$fixture/ada" config user.email ada@example.invalid || return 1
  git -C "$fixture/sam" config user.name 'Sam Other' || return 1
  git -C "$fixture/sam" config user.email sam@example.invalid || return 1
  mkdir -p "$fixture/ada/.mitcad/local" || return 1
  printf '{"send_at_once": false, "check_minutes": 0}\n' > "$fixture/ada/.mitcad/local/sync.json"
  cp "$fixture/ada/part.mitcad" "$fixture/part-before.mitcad" || return 1
  "$cli" export "$fixture/ada/part.mitcad" "$fixture/exchange.step" >> "$UI_OUT/replacement-setup.log" 2>&1 || return 1
}

ui_replacement_wait() {
  local path=$1 count=0
  while [ ! -f "$path" ]; do
    if [ -f "$MITCAD_TEST_REPLACEMENT/app-finished" ]; then
      echo "The native app ended before $path"
      return 1
    fi
    count=$((count + 1))
    if [ "$count" -gt 600 ]; then
      echo "Timed out waiting for $path"
      return 1
    fi
    sleep 0.2
  done
}

ui_replacement_retained() {
  local fixture=$MITCAD_TEST_REPLACEMENT
  cmp -s "$fixture/part-before.mitcad" "$fixture/ada/part.mitcad" || return 1
  diff -r "$fixture/recovery-before" "$MITCAD_AUTOSAVE_DIR" || return 1
  local ref
  ref=$(git --git-dir="$fixture/remote.git" for-each-ref --format='%(refname)' refs/mitcad/locks/)
  [ -n "$ref" ] || return 1
  git --git-dir="$fixture/remote.git" show "$ref:lock.json" | python3 -c \
    'import json, sys; lock=json.load(sys.stdin); assert lock["owner"]["email"] == "ada@example.invalid", lock' || return 1
}

ui_replacement_coordinate() {
  local fixture=$MITCAD_TEST_REPLACEMENT folder action
  # The app has acquired the actual remote edit lock and autosaved before
  # divergence is introduced. Startup cannot resolve the fixture instead.
  ui_replacement_wait "$fixture/ready.png" || return 1
  python3 - "$MITCAD_AUTOSAVE_DIR" "$fixture/ada/part.mitcad" << 'PY'
import glob, json, os, sys
folder, path = sys.argv[1:]
metadata = glob.glob(os.path.join(folder, '*.json'))
assert len(metadata) == 1, metadata
with open(metadata[0]) as stream:
    meta = json.load(stream)
assert meta['path'] == path and meta['document'] == 'part.mitcad', meta
assert meta['revision'] > 0, meta
with open(os.path.join(folder, meta['project'])) as stream:
    doc = json.load(stream)
assert next(p for p in doc['parameters'] if p['name'] == 'd3')['value'] == 27, doc['parameters']
assert len(doc['features']) == 2, doc['features']
PY
  [ "$?" = 0 ] || return 1
  cp -R "$MITCAD_AUTOSAVE_DIR" "$fixture/recovery-before" || return 1
  for folder in ada sam; do
    python3 - "$fixture/$folder/other.mitcad" "$folder" << 'PY'
import json, sys
path, owner = sys.argv[1:]
with open(path) as stream:
    doc = json.load(stream)
next(p for p in doc['parameters'] if p['name'] == 'd3')['value'] = 31 if owner == 'ada' else 32
with open(path, 'w') as stream:
    json.dump(doc, stream, indent=2)
    stream.write('\n')
PY
    [ "$?" = 0 ] || return 1
    git -C "$fixture/$folder" add other.mitcad || return 1
    git -C "$fixture/$folder" commit -qm 'Diverge the other design' || return 1
  done
  git -C "$fixture/sam" push -q origin main || return 1
  touch "$fixture/divergence-ready" || return 1
  for action in Open New Close; do
    ui_replacement_wait "$fixture/$action.png" || return 1
    ui_replacement_retained || return 1
    touch "$fixture/$action-verified" || return 1
  done
  ui_replacement_wait "$fixture/imported.png" || return 1
  cmp -s "$fixture/part-before.mitcad" "$fixture/ada/part.mitcad" || return 1
  python3 - "$MITCAD_AUTOSAVE_DIR" << 'PY'
import glob, json, os, sys
folder = sys.argv[1]
metadata = glob.glob(os.path.join(folder, '*.json'))
assert len(metadata) == 1, metadata
with open(metadata[0]) as stream:
    meta = json.load(stream)
assert meta['path'] == '' and meta['document'] == 'exchange', meta
with open(os.path.join(folder, meta['project'])) as stream:
    doc = json.load(stream)
assert [feature['type'] for feature in doc['features']] == ['base'], doc['features']
PY
  [ "$?" = 0 ] || return 1
  touch "$fixture/import-verified"
}
