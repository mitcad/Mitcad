#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/MessageBand.cpp app/framework/MessageLog.cpp app/framework/FloatingLayout.cpp app/MainWindow.cpp app/browser/BrowserController.cpp tools/cli
# The message band and the message history (mitcad#1042) through the real
# UI, on a block whose second fillet fails (r 50):
#   Floating:
#   - A click on the failures summary: Fillet2's error in the band right
#     above the timeline, as wide as it, the Browser's card above it; it is
#     still shown 8 s later; its close button (x) turns it into the stub
#     of the latest message, in the band's place.
#   - The error again, then Extrude (E): the next command closes it.
#   - Esc cancels Extrude: the hint "Command canceled." fades after about
#     6 s and leaves its stub.
#   - A click on the stub opens the history: the newest message first, the
#     error below it; a click on the error's row selects Fillet2 (without
#     saying its error again) and closes the history. Esc closes it too.
#   Docked:
#   - The error on the status line; the status bar's history button opens
#     the same history, whose error row selects Fillet2.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-message-band-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-message-band.XXXXXX")
FILE=$WORK/block.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

# Sketch1 (F1) 30 x 20, Extrude1 (F2) 10 up; Fillet1 (F3) r 1 on a
# vertical edge; Fillet2 (F4) r 50 fails.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
    "edges": ["E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"], "radius": 1}},
  {"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
    "edges": ["E{F2:side(c3[c2,c4])|F2:side(c4[c3,c1])}"], "radius": 50}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# since_mark: the log's lines since the mark.
since_mark() { tail -n +$((UI_MARK + 1)) "$UI_LOG"; }

# expect_line regex description [seconds]: a line since the mark matches
# the extended regular expression; LINE is the last one.
expect_line() {
  local i
  for i in $(seq 1 $((${3:-6} * 5))); do
    LINE=$(since_mark | grep -E -- "$1" | tail -1)
    [ -n "$LINE" ] && { echo "ok   $2"; return; }
    sleep 0.2
  done
  ui_fail "$2: no line since the mark matches '$1'"
}

# not_since regex description: no line since the mark matches.
not_since() {
  if since_mark | grep -qE -- "$1"; then
    ui_fail "$2: '$(since_mark | grep -E -- "$1" | head -1)'"
  fi
  echo "ok   $2"
}

# row_text regex: the text of the last history row line matching it,
# without its place (what ui_click_logged takes).
row_text() {
  since_mark | grep -E -- "^Message history row [0-9]+: $1" | tail -1 | sed 's/ at [0-9]*,[0-9]*$//'
}

# row_number regex: the row's number in the history (1: the newest).
row_number() {
  since_mark | grep -E -- "^Message history row [0-9]+: $1" | tail -1 | sed 's/^Message history row \([0-9]*\):.*/\1/'
}

# select_from_history description: the history open (since the mark), a
# click on Fillet2's error row selects Fillet2 and closes it, without its
# error said again.
select_from_history() {
  local error
  expect_line "^Message history: [0-9]+ entries$" "$1: the history opened"
  expect_line "^Message history row [0-9]+: error Fillet2: .* at [0-9]+,[0-9]+$" "$1: Fillet2's error is in it"
  error=$(row_text "error Fillet2: ")
  ui_mark
  ui_step "click Fillet2's error"          ui_click_logged "$error"
  ui_expect_new "Message history: selected F4" "$1: the row is about Fillet2"
  ui_expect_new "Revealed Fillet2" "$1: Fillet2 selected"
  ui_expect_new "Message history: closed" "$1: the history closed"
  ui_sync
  not_since "^Message band: error Fillet2" "$1: the error is not said again"
}

ui_start_display
ui_start_app --open "$FILE" --chrome=floating
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Failed features: Fillet2: " "Fillet2 failed"
ui_expect_log "Failures button at" "the failures summary is placed"

echo "--- Floating: an error in the band above the timeline stays until it is closed"
ui_mark
ui_step "click the failures summary"       ui_click_logged "Failures button"
ui_expect_new "Revealed Fillet2" "the summary went to Fillet2"
expect_line "^Message band full: error Fillet2: " "the band shows Fillet2's error"
# The band was placed when Fillet2's error first showed on opening, before
# the mark, and a placement is logged only when it changes: the newest
# placement in the whole log.
LINE=$(grep -E "^Message band placed: " "$UI_LOG" | tail -1)
grep -q "^Message band placed: full " <<< "$LINE" || ui_fail "the band is placed: '$LINE'"
echo "ok   the band is placed"
read -r BX BY BW BH ROW TX TY TW TH <<< "$(sed -E \
  's/.*: full ([0-9-]+),([0-9-]+) ([0-9]+)x([0-9]+), row ([0-9]+), timeline ([0-9-]+),([0-9-]+) ([0-9]+)x([0-9]+)$/\1 \2 \3 \4 \5 \6 \7 \8 \9/' \
  <<< "$LINE")"
[ -n "$TH" ] || ui_fail "the band's place: '$LINE'"
[ "$BX" = "$TX" ] && [ "$BW" = "$TW" ] || ui_fail "the band is not as wide as the timeline: '$LINE'"
echo "ok   as wide as the timeline ($BW px)"
[ $((BY + BH)) -le "$TY" ] || ui_fail "the band is not above the timeline: '$LINE'"
[ "$ROW" -ge "$TH" ] || ui_fail "the band's row is lower than the timeline: '$LINE'"
echo "ok   right above the timeline, its row $ROW px (the timeline $TH px)"
ui_sync
CARDS=$(grep "^Floating cards: " "$UI_LOG" | tail -1)
read -r CY CH <<< "$(sed -nE 's/.*; browserDock [0-9-]+,([0-9-]+) [0-9]+x([0-9]+).*/\1 \2/p' <<< "$CARDS")"
[ -n "$CH" ] || ui_fail "the Browser's card is not in '$CARDS'"
[ $((CY + CH)) -le "$BY" ] || ui_fail "the Browser's card reaches into the band: $CY + $CH > $BY"
echo "ok   the Browser's card ends above the band"
ui_expect_log "Message band close at" "the close button is placed"
sleep 8
ui_sync
not_since "^Message band stub: " "the error is still shown 8 s later"
ui_mark
ui_step "close the message (x)"            ui_click_logged "Message band close"
expect_line "^Message band stub: error Fillet2: .* \(closed\)$" "closed: the stub of the error"
expect_line "^Message band placed: stub " "the stub is placed"
grep -q "^Message band placed: stub .*, row $ROW, timeline $TX,$TY ${TW}x$TH$" <<< "$LINE" ||
  ui_fail "the stub does not keep the band's row: '$LINE'"
echo "ok   the stub keeps the band's row: the cards above do not move"

echo "--- The next command closes an error"
ui_mark
ui_step "click the failures summary"       ui_click_logged "Failures button"
expect_line "^Message band full: error Fillet2: " "the error shown again"
ui_step "clear the selection (Esc)"        ui_key Escape
ui_mark
ui_step "extrude (E)"                      ui_key e
ui_expect_new "Command Extrude started" "Extrude started"
expect_line "^Message band stub: error Fillet2: .* \(next command\)$" "the next command closed the error"

echo "--- A hint fades after about 6 s and leaves its stub"
ui_mark
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Extrude cancelled" "Extrude cancelled"
expect_line "^Message band full: hint Command canceled\.$" "the hint in the band"
SHOWN=$SECONDS
sleep 3
ui_sync
not_since "^Message band stub: " "the hint is still shown 3 s later"
expect_line "^Message band stub: hint .* \(faded\)$" "the hint faded to its stub" 12
FADED=$((SECONDS - SHOWN))
[ "$FADED" -ge 4 ] && [ "$FADED" -le 10 ] || ui_fail "the hint faded after $FADED s, not about 6 s"
echo "ok   faded after about $FADED s"
ui_expect_log "Message stub at" "the stub is placed"

echo "--- The history: newest first; a feature's row selects it"
ui_mark
ui_step "click the stub"                   ui_click_logged "Message stub"
expect_line "^Message history row 1: hint Command canceled\. at " "the newest message is row 1"
expect_line "^Message history row [0-9]+: error Fillet2: " "Fillet2's error is in it"
[ "$(row_number "error Fillet2: ")" -gt 1 ] || ui_fail "the error is not below the newer hint"
echo "ok   the error below the newer hint"
select_from_history "Floating"
ui_mark
ui_step "click the stub"                   ui_click_logged "Message stub"
ui_expect_new "Message history: " "the history opened again"
ui_step "close it (Esc)"                   ui_key Escape
ui_expect_new "Message history: closed" "Esc closed it"
ui_stop_app

echo "--- Docked: the status bar's history button"
ui_start_app --open "$FILE" --chrome=docked
ui_expect_log "Failures button at" "the failures summary is placed"
ui_expect_log "Message history button at" "the history button is in the status bar"
ui_mark
ui_step "click the failures summary"       ui_click_logged "Failures button"
ui_expect_new "Status error: Fillet2: " "the status line says it as an error"
expect_line "^Message band: error Fillet2: " "the message is recorded"
ui_step "clear the selection (Esc)"        ui_key Escape
ui_mark
ui_step "click the history button"         ui_click_logged "Message history button"
select_from_history "Docked"

ui_finish "UI message band test"
