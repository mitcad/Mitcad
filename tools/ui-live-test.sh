#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files
# check-all sources: core/vcs core/ffi/src/vcs.rs core/ffi/src/remote.rs core/ffi/src/diff.rs core/ffi/src/live.rs tools/cli
# Live updates through an MQTT broker (mitcad#89) through the real UI, with
# a bare repository in a folder standing in for the cloud and brokers the
# test starts (mosquitto on free ports of this computer; skipped without
# mosquitto):
#   1. The trust question before the first connection: Not Now is kept,
#      nothing connects; a changed address (Project Settings) asks again;
#      Connect: the indicator says Live, the remote's timer stops; Test.
#   2. The broker stops: Live offline (polling); it comes back: Live, and
#      the remote is checked for what was missed.
#   3. Two instances: a second Mitcad (its own settings, the project cloned)
#      sees who has the design open; a version the first one saves arrives
#      at once (the indicator's newer version and the notice, long before
#      the remote's 10-minute timer); a clean quit is seen as a leave.
#   4. Preferences > Cloud: Allow live updates off and on; Forget a broker
#      (asked again at the next start).
#   5. A broker that wants a password (TLS with a test certificate
#      authority, MITCAD_TEST_LIVE_CA): the sign-in notice, Sign In; without
#      a keychain the credentials last the session only. A password over
#      plain text (mqtt://) is refused with the core's reason.
#   6. With a Secret Service (GNOME's keyring in a D-Bus session of the
#      test's own, when installed): the credentials kept in the keychain,
#      read at the next start; Preferences' Sign Out removes them.
#
# git's configuration and HOME are the test's own. Runs headless on Xvfb
# (see ui-test-lib.sh); Qt's own file dialogs (--no-native-dialogs).
# Usage: tools/ui-live-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

command -v git > /dev/null || { echo "git is required (apt-get install git)" >&2; exit 2; }
MOSQUITTO=${MITCAD_MOSQUITTO:-$(command -v mosquitto || ls /usr/sbin/mosquitto /usr/local/sbin/mosquitto 2> /dev/null | head -1)}
if [ -z "$MOSQUITTO" ] || [ ! -x "$MOSQUITTO" ]; then
  echo "SKIP: mosquitto is not installed (tools/dev-env/install-packages.sh)"
  exit 0
fi
MOSQUITTO_PASSWD=$(dirname "$MOSQUITTO")/mosquitto_passwd
[ -x "$MOSQUITTO_PASSWD" ] || MOSQUITTO_PASSWD=$(command -v mosquitto_passwd || true)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}

WORK=$(mktemp -d /tmp/mitcad-ui-live.XXXXXX)
BROKER_PIDS=()
B_PID=""
KEYRING_PIDS=()
cleanup() {
  ui_cleanup
  [ -n "$B_PID" ] && kill "$B_PID" 2> /dev/null
  for pid in "${BROKER_PIDS[@]}" "${KEYRING_PIDS[@]}"; do
    kill "$pid" 2> /dev/null
  done
  rm -rf "$WORK"
}
trap cleanup EXIT
export HOME=$WORK/home
mkdir -p "$HOME"
DOCUMENTS=$HOME/Documents/Mitcad
export GIT_CONFIG_GLOBAL=$WORK/gitconfig
export GIT_CONFIG_NOSYSTEM=1
unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_DIR GIT_WORK_TREE MITCAD_GIT
unset MITCAD_TEST_LIVE_CA
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
# A lost broker is tried again after 0.2 s (then 0.4, ...), not a second.
export MITCAD_TEST_LIVE_RETRY_MS=200
AUTHOR="Ada Tester <ada@example.invalid>"

TEAM=$WORK/team.git
A=$DOCUMENTS/team
FILE_A=$A/block.mitcad
B=$WORK/b/team
FILE_B=$B/block.mitcad
FILE_ID=$(printf %s block.mitcad | sha256sum | cut -c1-8)

# --- Helpers

free_port() {
  python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])'
}

listening() { (exec 3<> "/dev/tcp/127.0.0.1/$1") 2> /dev/null; }

# start_broker name: mosquitto with $WORK/<name>.conf (its ports in
# <name>.ports); the pid in BROKER_<name>.
start_broker() {
  local name=$1 port
  "$MOSQUITTO" -c "$WORK/$name.conf" > "$WORK/$name.out" 2>&1 &
  eval "BROKER_$name=$!"
  BROKER_PIDS+=($!)
  for port in $(cat "$WORK/$name.ports"); do
    for _ in $(seq 1 100); do
      listening "$port" && break
      sleep 0.05
    done
    listening "$port" || { cat "$WORK/$name.out" "$WORK/$name.log" 2> /dev/null; ui_fail "mosquitto $name does not listen on $port"; }
  done
}

stop_broker() {
  local pid
  eval "pid=\$BROKER_$1"
  kill "$pid" 2> /dev/null
  wait "$pid" 2> /dev/null
}

# expect_re "extended regex" description [seconds]: as ui_expect_new.
expect_re() {
  for _ in $(seq 1 $((${3:-6} * 5))); do
    tail -n +$((${UI_MARK:-0} + 1)) "$UI_LOG" | grep -qE -- "$1" && { echo "ok   $2"; return; }
    sleep 0.2
  done
  ui_fail "$2: /$1/ not in the log since the mark"
}

# expect_none "text" description: not logged since the mark (after a moment).
expect_none() {
  sleep 1
  ui_sync
  if tail -n +$((${UI_MARK:-0} + 1)) "$UI_LOG" | grep -qF -- "$1"; then
    ui_fail "$2: '$1' was logged"
  fi
  echo "ok   $2"
}

type_text() { ui_type_text "$1"; }

expect_exit() {
  for _ in $(seq 1 50); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "$1: Mitcad did not exit"
  UI_RUNNER=""
  echo "ok   $1"
}

quit() {
  ui_focus_main
  ui_key ctrl+q
  expect_exit "${1:-exited}"
}

# start_a arguments...: Mitcad (A) started; its log is new, all of it counts.
start_a() {
  ui_start_app "$@"
  UI_MARK=0
}

# set_broker address: the project's broker for everyone, a version sent.
set_broker() {
  "$CLI" project settings "$A" --set "{\"shared\": {\"live_updates\": {\"broker\": \"$1\"}}}" --author "$AUTHOR" \
    > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli project settings"; }
  git -C "$A" push -q origin main 2> "$WORK/push.log" || { cat "$WORK/push.log"; ui_fail "git push"; }
  echo "ok   the project's broker is $1"
}

answer_trust() {
  ui_expect_new "Live trust question: team through $1 [Connect, Not Now]" "the trust question for $1" 20
  ui_focus_dialog '^Live Updates$'
  if [ "$2" = connect ]; then
    ui_step "connect (Enter)"            ui_key Return
  else
    ui_step "not now (Esc)"              ui_key Escape
  fi
  ui_expect_new "Live trust: $1 ${2/_/ }" "answered: $2"
}

# The second Mitcad (B): settings, data and log of its own; no gdb.
B_LOG=$WORK/b.log
start_b() {
  XDG_CONFIG_HOME=$WORK/b-config XDG_DATA_HOME=$WORK/b-data XDG_CACHE_HOME=$WORK/b-cache \
    "$UI_APP" --no-recovery --open "$1" > "$B_LOG" 2>&1 &
  B_PID=$!
  B_WINDOW=""
  for _ in $(seq 1 100); do
    # --all: B's process and the name (without it either would do).
    B_WINDOW=$(xdotool search --all --pid "$B_PID" --onlyvisible --name ' - Mitcad$' 2> /dev/null | head -1)
    [ -n "$B_WINDOW" ] && break
    sleep 0.2
  done
  [ -n "$B_WINDOW" ] || { cat "$B_LOG"; ui_fail "the second Mitcad's window did not appear"; }
  echo "ok   the second Mitcad started"
}

b_expect() {
  for _ in $(seq 1 $((${3:-10} * 5))); do
    grep -qF -- "$1" "$B_LOG" && { echo "ok   B: $2"; return; }
    sleep 0.2
  done
  echo "--- the second Mitcad's log:"
  tail -40 "$B_LOG"
  ui_fail "B: $2: '$1' not in its log"
}

# b_key window-name-regex keys...: keys to a window of B's.
b_key() {
  local name=$1 window=""
  shift
  for _ in $(seq 1 50); do
    window=$(xdotool search --all --pid "$B_PID" --onlyvisible --name "$name" 2> /dev/null | head -1)
    [ -n "$window" ] && break
    sleep 0.2
  done
  [ -n "$window" ] || ui_fail "B has no window '$name'"
  xdotool windowfocus --sync "$window" 2> /dev/null
  sleep 0.3
  xdotool key --delay 80 "$@"
  sleep 0.3
}

# --- Brokers

# Anonymous, plain text, two addresses (listeners) of one broker.
P1=$(free_port)
P2=$(free_port)
cat > "$WORK/open.conf" << EOF
per_listener_settings false
allow_anonymous true
persistence false
log_dest file $WORK/open.log
listener $P1 127.0.0.1
listener $P2 127.0.0.1
EOF
echo "$P1 $P2" > "$WORK/open.ports"
start_broker open

ui_start_display

echo "--- A Cloud project with a broker; the trust question: Not Now"
git init -q --bare "$TEAM"
"$CLI" project create "$A" --author "$AUTHOR" --url "$TEAM" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli project create"; }
# Without edit locks: both instances edit the design and see the newer
# version's notice (tools/ui-locks-test.sh has the locks, with a broker
# too); who has the design open is told all the same.
"$CLI" project settings "$A" --set '{"shared": {"edit_locks": {"enabled": false}}}' --author "$AUTHOR" \
  > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli project settings"; }
set_broker "mqtt://127.0.0.1:$P1"
start_a --demo --no-native-dialogs
ui_step "save as (Ctrl+Shift+S)"         ui_key ctrl+shift+s
ui_step "into the project, Enter"        ui_type_path "Save As" "$FILE_A"
ui_expect_new "Version recorded: block.mitcad " "the design is a version of the Cloud project" 20
answer_trust "mqtt://127.0.0.1:$P1" not_now
ui_focus_main
expect_none "Live: team subscribes" "nothing connects"
expect_re "Project indicator: team, cloud, [a-z0-9 ,]*synced$" "the indicator without live updates" 20
quit

echo "--- Not Now kept; a changed address asks again; Connect: Live; Test"
start_a --open "$FILE_A" --no-native-dialogs
ui_expect_new "Live: team: mqtt://127.0.0.1:$P1 not trusted (Not Now): not connected" "not asked again" 20
expect_none "Live trust question" "no question"
ui_mark
ui_step "project settings (search)"      ui_command "Project Settings"
ui_expect_new "live updates mqtt://127.0.0.1:$P1 mitcad" "Project Settings shows the broker"
ui_focus_dialog '^Project Settings - team$'
ui_step "the broker (Alt+R, Tab)"        ui_key alt+r Tab
ui_step "another address"                type_text "mqtt://127.0.0.1:$P2"
ui_step "done (Tab)"                     ui_key Tab
ui_expect_new "Project Settings: shared settings recorded: " "a version of its own"
answer_trust "mqtt://127.0.0.1:$P2" connect
ui_expect_new "Live: session " "this run's session"
ui_expect_new "Live: team subscribes through mqtt://127.0.0.1:$P2, prefix mitcad, as anonymous" "subscribed"
expect_re "Live: connection c[0-9]+ \(mqtt://127.0.0.1:$P2\) connected" "connected" 10
ui_expect_new "Live: team subscribed" "the subscription acknowledged" 10
ui_expect_new "Remote check: no timer while live updates are connected" "no timer while live"
ui_focus_dialog '^Project Settings - team$'
ui_step "test (Alt+S)"                   ui_key alt+s
ui_expect_new "Live test dialog: mqtt://127.0.0.1:$P2, prefix mitcad" "Test"
ui_expect_new "Live test: mqtt://127.0.0.1:$P2: ok; steps connect ok, sign_in ok, subscribe ok, publish ok, receive ok" \
  "the test's steps" 20
ui_focus_dialog '^Test Live Updates$'
ui_step "close the test (Esc)"           ui_key Escape
ui_focus_dialog '^Project Settings - team$'
ui_step "close (Esc)"                    ui_key Escape
ui_focus_main
expect_re "Project indicator: team, cloud, synced, live$" "the indicator: Live" 20

echo "--- The broker stops: Live offline (polling); back: Live"
ui_mark
stop_broker open
expect_re "Live: connection c[0-9]+ \(mqtt://127.0.0.1:$P2\) offline" "the connection lost" 10
expect_re "Project indicator: team, cloud, synced, live offline \(polling\)$" "the indicator: Live offline" 10
ui_expect_new "Remote check: the timer again (every 10 min)" "the remote's timer again"
ui_mark
start_broker open
expect_re "Live: connection c[0-9]+ \(mqtt://127.0.0.1:$P2\) connected" "connected again" 20
ui_expect_new "Live: team: the remote is checked (connected again)" "what was missed is checked" 10
expect_re "Project indicator: team, cloud, synced, live$" "Live again" 10
quit

echo "--- Two instances: who has the design open, a version at once"
mkdir -p "$(dirname "$B")"
git clone -q "$TEAM" "$B" 2> /dev/null || ui_fail "git clone"
git -C "$B" config user.name "Bea Other"
git -C "$B" config user.email bea@example.invalid
start_a --open "$FILE_A" --set d3=35 --no-native-dialogs
ui_expect_new "Live: team subscribed" "A is live" 20
A_SESSION=$(grep -o 'Live: session [0-9a-f-]*' "$UI_LOG" | tail -1 | cut -d' ' -f3 | cut -c1-8)
start_b "$FILE_B"
b_expect "Live trust question: team through mqtt://127.0.0.1:$P2 [Connect, Not Now]" "its own trust question" 20
b_key '^Live Updates$' Return
b_expect "Live trust: mqtt://127.0.0.1:$P2 connect" "Connect"
b_expect "Live: team subscribed" "B is live" 10
b_expect "Project indicator: team, cloud, synced, live" "B's indicator: Live"
b_expect "Live event: team: open $FILE_ID by Ada Tester (editing) in session $A_SESSION" "B sees A has the design open"
B_SESSION=$(grep -o 'Live: session [0-9a-f-]*' "$B_LOG" | tail -1 | cut -d' ' -f3 | cut -c1-8)
xdotool windowfocus --sync "$UI_WINDOW" 2> /dev/null
ui_expect_new "Live event: team: open $FILE_ID by Bea Other (editing) in session $B_SESSION" \
  "A sees B has it open" 10
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Version recorded: block.mitcad " "A saves a version"
ui_expect_new "Remote push: sent 1 version(s) to origin/main" "sent" 20
SENT=$(date +%s)
COMMIT=$(git -C "$A" rev-parse --short=7 HEAD)
ui_expect_new "Live: team published version $COMMIT" "announced"
b_expect "Live event: team: version $COMMIT on main by Ada Tester" "the version announced to B" 10
b_expect "Remote check: announced by live updates" "B checks the remote at once"
b_expect "Project indicator: team, cloud, behind 1, live" "B's indicator: a newer version" 10
b_expect "Remote notice: A newer version of block.mitcad is on the remote, saved by Ada Tester at " \
  "B's notice of the newer version"
[ $(($(date +%s) - SENT)) -lt 30 ] || ui_fail "B learnt of the version after $(($(date +%s) - SENT)) s"
echo "ok   B learnt of it in $(($(date +%s) - SENT)) s (its remote check timer: 10 min)"
b_key ' - Mitcad$' ctrl+q
for _ in $(seq 1 50); do
  kill -0 "$B_PID" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$B_PID" 2> /dev/null && ui_fail "B did not exit"
B_PID=""
b_expect "Live: closed, every connection left cleanly" "B left cleanly"
xdotool windowfocus --sync "$UI_WINDOW" 2> /dev/null
ui_expect_new "Live event: team: open $FILE_ID closed by session $B_SESSION" "A sees B's window closed" 10
ui_expect_new "Live event: team: session $B_SESSION left" "A sees B leave" 10

echo "--- Preferences: live updates off and on, Forget"
ui_mark
ui_step "preferences, cloud (search)"    ui_command "Preferences: Cloud"
ui_expect_new "Preferences: known brokers: mqtt://127.0.0.1:$P1 (not now), mqtt://127.0.0.1:$P2 (trusted)" \
  "the known brokers"
ui_focus_dialog '^Preferences$'
ui_step "allow live updates off (Alt+L)" ui_key alt+l
ui_step "OK (Enter)"                     ui_key Return
ui_focus_main
ui_expect_new "live updates off, default broker none" "saved"
ui_expect_new "Live: team: off (not allowed in Preferences)" "off"
ui_expect_new "Live: team unsubscribed from c" "unsubscribed"
expect_re "Project indicator: team, cloud, [a-z0-9 ,]*synced$" "the indicator without Live"
ui_mark
ui_step "preferences, cloud (search)"    ui_command "Preferences: Cloud"
ui_focus_dialog '^Preferences$'
ui_step "allow live updates on (Alt+L)"  ui_key alt+l
ui_step "OK (Enter)"                     ui_key Return
ui_focus_main
ui_expect_new "Live: team subscribes through mqtt://127.0.0.1:$P2" "on again" 10
expect_re "Project indicator: team, cloud, [a-z0-9 ,]*synced, live$" "Live" 10
ui_mark
ui_step "preferences, cloud (search)"    ui_command "Preferences: Cloud"
ui_focus_dialog '^Preferences$'
ui_step "the known brokers (Alt+K)"      ui_key alt+k
ui_step "the second one (Down)"          ui_key Down
ui_step "forget (Alt+F)"                 ui_key alt+f
ui_expect_new "Preferences: forget mqtt://127.0.0.1:$P2" "Forget"
ui_expect_new "Live: mqtt://127.0.0.1:$P2 forgotten: team disconnected" "disconnected at once"
ui_expect_new "Preferences: known brokers: mqtt://127.0.0.1:$P1 (not now)" "the list without it"
ui_step "OK (Enter)"                     ui_key Return
ui_focus_main
expect_none "Live trust question" "not asked while the project stays open"
quit
start_a --open "$FILE_A" --no-native-dialogs
answer_trust "mqtt://127.0.0.1:$P2" not_now
quit

echo "--- A broker that wants a password: the sign-in notice, Sign In"
if ! command -v openssl > /dev/null || [ ! -x "$MOSQUITTO_PASSWD" ]; then
  echo "note: no openssl or mosquitto_passwd: the brokers with passwords are not tested"
  ui_finish "UI live updates test"
  exit 0
fi
CERTS=$WORK/certs
mkdir -p "$CERTS"
cat > "$CERTS/openssl.cnf" << 'EOF'
[req]
distinguished_name = dn
prompt = no
[dn]
CN = Mitcad test
[ca]
basicConstraints = critical,CA:TRUE
keyUsage = critical,keyCertSign,cRLSign
subjectKeyIdentifier = hash
[server]
basicConstraints = critical,CA:FALSE
keyUsage = critical,digitalSignature
extendedKeyUsage = serverAuth
subjectAltName = DNS:localhost
EOF
EC=(-newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes)
(
  cd "$CERTS" &&
    openssl req -x509 -new "${EC[@]}" -keyout ca.key -out ca.pem -days 2 -config openssl.cnf -extensions ca \
      -subj "/CN=Mitcad test CA" &&
    openssl req -new "${EC[@]}" -keyout server.key -out server.csr -config openssl.cnf -subj "/CN=localhost" &&
    openssl x509 -req -in server.csr -CA ca.pem -CAkey ca.key -set_serial 2 -out server.pem -days 2 \
      -extfile openssl.cnf -extensions server
) > "$WORK/openssl.log" 2>&1 || { cat "$WORK/openssl.log"; ui_fail "openssl made no test certificates"; }
"$MOSQUITTO_PASSWD" -b -c "$WORK/passwd" ada "s3cret word" > /dev/null 2>&1 || ui_fail "mosquitto_passwd"
PT=$(free_port)
P3=$(free_port)
cat > "$WORK/closed.conf" << EOF
per_listener_settings false
allow_anonymous false
password_file $WORK/passwd
persistence false
log_dest file $WORK/closed.log
listener $PT 127.0.0.1
cafile $CERTS/ca.pem
certfile $CERTS/server.pem
keyfile $CERTS/server.key
listener $P3 127.0.0.1
EOF
echo "$PT $P3" > "$WORK/closed.ports"
start_broker closed
export MITCAD_TEST_LIVE_CA=$CERTS/ca.pem
TLS="mqtts://localhost:$PT"
set_broker "$TLS"

# sign_in user password: the notice's Sign In, the dialog filled in.
sign_in() {
  ui_expect_new "Live notice: Live updates need you to sign in to localhost [Sign In, Not Now]" "the sign-in notice" 20
  ui_expect_new "Live notice Sign In at " "its buttons placed"
  ui_step "sign in (the notice)"         ui_click_logged "Live notice Sign In"
  ui_expect_new "Live sign-in dialog: $TLS, user none, keychain $3" "the sign-in dialog (keychain $3)"
  ui_focus_dialog '^Sign In to localhost$'
  ui_step "the user"                     type_text "$1"
  ui_step "the password (Tab)"           ui_key Tab
  xdotool type --delay 20 "$2"
  ui_step "sign in (Enter)"              ui_key Return
}

start_a --open "$FILE_A" --no-native-dialogs
answer_trust "$TLS" connect
ui_expect_new "Keychain: read localhost:$PT: unavailable" "no keychain on this display (no D-Bus)" 10
ui_expect_new "Live: team subscribes through $TLS, prefix mitcad, as anonymous" "anonymous first"
expect_re "Live: connection c[0-9]+ \($TLS\) failed \(auth_failed: " "refused" 10
expect_re "Project indicator: team, cloud, [a-z0-9 ,]*live offline \(polling\)$" "offline: polling" 10
sign_in ada "s3cret word" off
ui_expect_new "Live sign-in: $TLS as ada" "signed in"
ui_focus_main
ui_expect_new "Live: team subscribes through $TLS, prefix mitcad, as ada" "with the credentials"
ui_expect_new "Live: team subscribed" "subscribed" 10
expect_re "Project indicator: team, cloud, [a-z0-9 ,]*live$" "Live" 10
expect_none "Keychain: saved" "not kept in a keychain"
quit
start_a --open "$FILE_A" --no-native-dialogs
ui_expect_new "Live: team subscribes through $TLS, prefix mitcad, as anonymous" "the next run has none: anonymous" 20
ui_expect_new "Live notice: Live updates need you to sign in to localhost" "asked again" 20
ui_expect_new "Live notice Not Now at " "its buttons placed"
ui_step "not now (the notice)"           ui_click_logged "Live notice Not Now"
ui_expect_new "Live notice: not now" "Not Now"
expect_re "Project indicator: team, cloud, [a-z0-9 ,]*live offline \(polling\)$" "polling" 10
quit

echo "--- A password over plain text is refused"
PLAIN="mqtt://127.0.0.1:$P3"
set_broker "$PLAIN"
start_a --open "$FILE_A" --no-native-dialogs
answer_trust "$PLAIN" connect
expect_re "Live: connection c[0-9]+ \($PLAIN\) failed \(auth_failed: " "refused" 10
ui_expect_new "Live notice: Live updates need you to sign in to 127.0.0.1" "the notice" 20
ui_expect_new "Live notice Sign In at " "its buttons placed"
ui_step "sign in (the notice)"           ui_click_logged "Live notice Sign In"
ui_focus_dialog '^Sign In to 127.0.0.1$'
ui_step "the user"                       type_text ada
ui_step "the password (Tab)"             ui_key Tab
xdotool type --delay 20 "s3cret word"
ui_step "sign in (Enter)"                ui_key Return
ui_expect_new "Live sign-in refused: a password is sent only over TLS: $PLAIN is plain text" "the core's reason"
ui_focus_dialog '^Sign In to 127.0.0.1$'
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Live sign-in cancelled" "cancelled"
quit

echo "--- The system's keychain (a Secret Service of the test's own)"
if ! command -v gnome-keyring-daemon > /dev/null || ! command -v dbus-daemon > /dev/null; then
  echo "note: no gnome-keyring-daemon or dbus-daemon: the keychain is not tested"
  ui_finish "UI live updates test"
  exit 0
fi
mkdir -p "$WORK/runtime"
chmod 700 "$WORK/runtime"
export XDG_RUNTIME_DIR=$WORK/runtime
dbus-daemon --session --fork --print-address=3 --print-pid=4 3> "$WORK/bus.address" 4> "$WORK/bus.pid" ||
  ui_fail "dbus-daemon"
KEYRING_PIDS+=($(cat "$WORK/bus.pid"))
export DBUS_SESSION_BUS_ADDRESS=$(cat "$WORK/bus.address")
printf 'test' | gnome-keyring-daemon --unlock --components=secrets > "$WORK/keyring.env" 2> "$WORK/keyring.log" ||
  { cat "$WORK/keyring.log"; ui_fail "gnome-keyring-daemon"; }
KEYRING_PIDS+=($(sed -n 's/^GNOME_KEYRING_PID=//p' "$WORK/keyring.env"))
set_broker "$TLS"
start_a --open "$FILE_A" --no-native-dialogs
ui_expect_new "Keychain: read localhost:$PT: not found" "the keychain answers: nothing yet" 20
sign_in ada "s3cret word" on
ui_expect_new "Live sign-in: $TLS as ada, remembered" "remembered"
ui_expect_new "Keychain: saved localhost:$PT" "kept in the keychain" 10
ui_focus_main
ui_expect_new "Live: team subscribed" "subscribed" 10
quit
start_a --open "$FILE_A" --no-native-dialogs
ui_expect_new "Keychain: read localhost:$PT: found, user ada" "read at the next start" 20
ui_expect_new "Live: team subscribes through $TLS, prefix mitcad, as ada" "signed in from the keychain"
ui_expect_new "Live: team subscribed" "no notice: subscribed" 10
expect_none "Live notice:" "no sign-in notice"
ui_mark
ui_step "preferences, cloud (search)"    ui_command "Preferences: Cloud"
ui_expect_new "$TLS (trusted, user ada)" "Known brokers: the user"
ui_focus_dialog '^Preferences$'
ui_step "the known brokers (Alt+K)"      ui_key alt+k
ui_step "the TLS broker (typed)"         xdotool type --delay 20 "mqtts"
ui_step "sign out (Alt+G)"               ui_key alt+g
ui_expect_new "Preferences: sign out of localhost:$PT" "Sign Out"
ui_expect_new "Keychain: removed localhost:$PT" "removed from the keychain" 10
ui_step "OK (Enter)"                     ui_key Return
ui_focus_main
ui_expect_new "Live: signed out of localhost:$PT" "the live controller follows"
ui_expect_new "Live: team subscribes through $TLS, prefix mitcad, as anonymous" "anonymous again"
ui_expect_new "Live notice: Live updates need you to sign in to localhost" "the notice again" 20
quit

ui_finish "UI live updates test"
