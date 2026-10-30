#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowMcp.cpp app/MainWindowMcpUi.cpp app/mcp core/mcp core/ffi/src/mcp.rs
# check-all sources: app/framework/CommandSessionScript.cpp app/framework/CommandPanelState.cpp
# check-all sources: app/sketch/SketchReveal.cpp app/view/ViewReveal.cpp app/files/ProjectIndicator.cpp
# check-all sources: core/model-api/src/classes.rs tools/cli tools/ui-mcp-live.py
# check-all sources: app/framework/TestDriverAssistant.cpp app/tests/assistant_client.cpp tools/ui-assistant-setup-test.py
# Live MCP for debugging (mitcad#265) through the real UI on Xvfb and the
# actual bridge, `mitcad-cli mcp --connect`: the application opens a design
# whose sketch F4 holds a projection that no longer follows its face
# (tools/ui-mcp-live.py makes it with mitcad-cli), and the flow of
# tools/ui-mcp-live.py runs against it: queries with the sketch open,
# sketch.add_line through the sketch session and undo, ui_state with the
# palette's warning, ui_select, ui_screenshot, ui_command Clean Up Stale
# Projections and Finish Sketch, document_save, ui_log; before it, the
# bridge authenticates with eight idle connections held open, and 7 MiB
# inputs neither grow the memory nor the log lines (mitcad#935). The agent indicator
# (mitcad#710) names the initialized client while the bridge is connected;
# with none, a click on it opens Connect AI Assistant and its menu offers
# Connect AI Assistant and Folders for Designs. Preferences > AI Assistants'
# Connect AI Assistant... opens the dialog over the modal Preferences, and
# real clicks choose assistants there while Preferences is open (mitcad#760).
# Then the shared setup-dialog flow checks isolated file and stub-CLI
# registration, failures, concurrent edits, folder consent and both layouts.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-mcp-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
ASSISTANT_CLIENT=${UI_ASSISTANT_CLIENT:-$(dirname "$UI_APP")/test_assistant_client}
LIVE="$(dirname "$0")/ui-mcp-live.py"
WORK=$(mktemp -d /tmp/mitcad-ui-mcp.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
[ -x "$ASSISTANT_CLIENT" ] || ui_fail "assistant client stub is missing: build the test_assistant_client target"

# settled "logged place": waits until the place logged last for it stays
# the same over a sync and a short wait (a dialog just shown is laid out
# again; a click must not use the place it had before).
settled() {
  local before after
  for _ in $(seq 1 20); do
    before=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    ui_sync
    sleep 0.3
    after=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    [ -n "$after" ] && [ "$before" = "$after" ] && return 0
  done
  ui_fail "the place of '$1' did not settle"
}

# The listener's record in a private folder of the test's own; the app makes
# it (it must not exist yet).
export MITCAD_MCP_DISCOVERY_DIR=$WORK/discovery
FILE=$(python3 -I -c 'import importlib.util, sys
spec = importlib.util.spec_from_file_location("live", sys.argv[1])
live = importlib.util.module_from_spec(spec)
spec.loader.exec_module(live)
print(live.make_design(sys.argv[2], sys.argv[3]))' "$LIVE" "$CLI" "$WORK") || ui_fail "mitcad-cli did not make the design"

ui_start_display
ui_start_app --mcp-folder "$WORK" --open "$FILE"
ui_expect_log "MCP live instance:" "the application's MCP listener is up"
ui_expect_log "Agent indicator: no agent" "the agent indicator says the window accepts agents"
ui_expect_log "Timeline Sketch2 at" "the design is open"

echo "--- The live debugging flow through the bridge"
ui_mark
python3 -I "$LIVE" "$CLI" "$FILE" || ui_fail "the live MCP flow failed (above)"
ui_expect_new "Agent connected" "the window said an agent was connected"
ui_expect_new "Agent indicator: connected: mitcad-live-debug" "the agent indicator named the client"
ui_expect_new "Agent disconnected" "the bridge's end disconnected it"
ui_expect_new "Agent indicator: no agent" "the agent indicator says no agent again"
ui_expect_new "Saved $FILE" "the agent saved the design"

echo "--- The agent indicator: a click connects an agent; its menu"
ui_mark
ui_step "the agent indicator"            ui_click_logged "Agent indicator"
ui_expect_new "Agent indicator chose help.connect_assistant" "a click with no agent connects one"
ui_focus_dialog '^Connect AI Assistant$'
ui_step "close Connect AI Assistant (Esc)" ui_key Escape
ui_focus_main
ui_mark
ui_step "its menu (right click)"         ui_click_logged "Agent indicator" 3
# One window listens: no instance line and no Copy Command for This Window
# (those are in the tooltip, and for several windows).
ui_expect_new "Agent indicator menu: Connect AI Assistant... | Folders for Designs..." "the menu: Connect AI Assistant, Folders for Designs"
ui_step "close the menu (Esc)"           ui_key Escape

echo "--- Connect AI Assistant from Preferences > AI Assistants, a modal dialog (mitcad#760)"
ui_focus_main
ui_mark
ui_step "Preferences: AI Assistants"     ui_command "Preferences: AI Assistants"
ui_expect_new "Preferences connect assistant at" "the AI Assistants page"
ui_focus_dialog '^Preferences$'
settled "Preferences connect assistant"
ui_mark
ui_step "Connect AI Assistant..."        ui_click_logged "Preferences connect assistant"
ui_expect_new "Connect AI Assistant opened over Preferences" "the dialog opens over Preferences"
ui_expect_new "Connect AI Assistant choice Other at" "its choices are placed"
ui_focus_dialog '^Connect AI Assistant$'
settled "Connect AI Assistant choice Other"
ui_mark
ui_step "Other"                          ui_click_logged "Connect AI Assistant choice Other"
ui_expect_new "Connect AI Assistant: Other, manual: Copy the configuration into your assistant's settings" \
  "a click chooses Other and the state line follows, while Preferences is open"
settled "Connect AI Assistant choice Claude Code"
ui_mark
ui_step "Claude Code"                    ui_click_logged "Connect AI Assistant choice Claude Code"
ui_expect_new "Connect AI Assistant: Claude Code, " "another click chooses Claude Code"
ui_step "close Connect AI Assistant (Esc)" ui_key Escape
ui_focus_dialog '^Preferences$'
ui_step "close Preferences (Esc)"        ui_key Escape
ui_focus_main
ui_crashed && ui_fail "the application crashed"

ui_finish "UI live MCP test"

# Keep this test's private display while the harness starts its own isolated
# application instances. The normal Mitcad build also builds the client stub.
ui_stop_app
ASSISTANT_LOG_DIR=${MITCAD_UI_LOG_DIR:-$WORK}/assistant-setup
python3 -I "$(dirname "$0")/ui-assistant-setup-test.py" "$UI_APP" "$CLI" "$ASSISTANT_CLIENT" "$ASSISTANT_LOG_DIR" ||
  ui_fail "assistant setup UI failed; see $ASSISTANT_LOG_DIR"
