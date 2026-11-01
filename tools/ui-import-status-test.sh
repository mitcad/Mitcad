#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/files core/import-replay/src/timeline_notes.rs core/import-fcstd core/import-base/src/notes.rs
# The import's outcomes on the timeline (mitcad#170), on a FreeCAD document
# made here and opened with --open (the import process):
#   - a PartDesign Body whose sketch is padded (replayed), then a feature
#     suppressed in FreeCAD, then a helix Mitcad cannot build (its stored
#     shape comes in as a base feature, the fallback);
#   - the fallback's base feature has an import note, information
#     (mitcad#1040: the information badge), its tooltip says which item it
#     stands in for and why, as the import report does; the suppressed
#     feature is a suppressed placeholder at its place, its tooltip the
#     file's state;
#   - the failures badge stays hidden (nothing failed), so does the warnings
#     badge (notes are no warnings); the import's own warnings (the fallback,
#     the skipped item) are a status line warning with their count;
#   - saved and opened again, the timeline says the same.
#
# Runs headless on Xvfb (see ui-test-lib.sh); Qt's own file dialogs
# (--no-native-dialogs), so that paths can be typed.
# Usage: tools/ui-import-status-test.sh
# check-all sources: core/model/src/document/import_notes.rs core/model/src/features/placeholder.rs tools/cli

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-import-status.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# The stored shapes: the pad (30 x 20 x 10 mm) and the helix's result (the
# box 12 mm high).
mkdir -p "$WORK/fc"
cat > "$WORK/fc/shapes.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 30, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 30, "width": 20, "height": 12, "operation": "new_body"}},
  {"cmd": "export", "path": "Pad.brp", "bodies": ["F1.b0"]},
  {"cmd": "export", "path": "Helix.brp", "bodies": ["F2.b0"]}
]
EOF
(cd "$WORK/fc" && "$CLI" run shapes.json) > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "the stored shapes"; }
cat > "$WORK/fc/Document.xml" << 'EOF'
<?xml version='1.0' encoding='utf-8'?>
<Document SchemaVersion="4" ProgramVersion="1.0R39319 (Git)" FileVersion="1">
  <Properties Count="0"></Properties>
  <Objects Count="5" Dependencies="1">
    <Object type="PartDesign::Body" name="Body" id="1"/>
    <Object type="Sketcher::SketchObject" name="Sketch" id="2"/>
    <Object type="PartDesign::Pad" name="Pad" id="3"/>
    <Object type="PartDesign::FeaturePython" name="Gear" id="4"/>
    <Object type="PartDesign::AdditiveHelix" name="Helix" id="5"/>
  </Objects>
  <ObjectData Count="5">
    <Object name="Body"><Properties Count="4">
      <Property name="Label" type="App::PropertyString"><String value="Plate"/></Property>
      <Property name="Group" type="App::PropertyLinkList"><LinkList count="4"><Link value="Sketch"/><Link value="Pad"/><Link value="Gear"/><Link value="Helix"/></LinkList></Property>
      <Property name="Tip" type="App::PropertyLink"><Link value="Helix"/></Property>
      <Property name="Shape" type="Part::PropertyPartShape"><Part file="Helix.brp"/></Property>
    </Properties></Object>
    <Object name="Sketch"><Properties Count="2">
      <Property name="Geometry" type="Part::PropertyGeometryList"><GeometryList count="4">
        <Geometry type="Part::GeomLineSegment"><LineSegment StartX="0" StartY="0" StartZ="0" EndX="30" EndY="0" EndZ="0"/><Construction value="0"/></Geometry>
        <Geometry type="Part::GeomLineSegment"><LineSegment StartX="30" StartY="0" StartZ="0" EndX="30" EndY="20" EndZ="0"/><Construction value="0"/></Geometry>
        <Geometry type="Part::GeomLineSegment"><LineSegment StartX="30" StartY="20" StartZ="0" EndX="0" EndY="20" EndZ="0"/><Construction value="0"/></Geometry>
        <Geometry type="Part::GeomLineSegment"><LineSegment StartX="0" StartY="20" StartZ="0" EndX="0" EndY="0" EndZ="0"/><Construction value="0"/></Geometry>
      </GeometryList></Property>
      <Property name="Constraints" type="Sketcher::PropertyConstraintList"><ConstraintList count="4">
        <Constrain Name="" Type="1" Value="0" First="0" FirstPos="2" Second="1" SecondPos="1" Third="-2000" ThirdPos="0" IsDriving="1" IsActive="1"/>
        <Constrain Name="" Type="1" Value="0" First="1" FirstPos="2" Second="2" SecondPos="1" Third="-2000" ThirdPos="0" IsDriving="1" IsActive="1"/>
        <Constrain Name="" Type="1" Value="0" First="2" FirstPos="2" Second="3" SecondPos="1" Third="-2000" ThirdPos="0" IsDriving="1" IsActive="1"/>
        <Constrain Name="" Type="1" Value="0" First="3" FirstPos="2" Second="0" SecondPos="1" Third="-2000" ThirdPos="0" IsDriving="1" IsActive="1"/>
      </ConstraintList></Property>
    </Properties></Object>
    <Object name="Pad"><Properties Count="4">
      <Property name="Profile" type="App::PropertyLinkSub"><LinkSub value="Sketch" count="0"></LinkSub></Property>
      <Property name="Length" type="App::PropertyLength"><Float value="10"/></Property>
      <Property name="Type" type="App::PropertyEnumeration"><Integer value="0"/></Property>
      <Property name="Shape" type="Part::PropertyPartShape"><Part file="Pad.brp"/></Property>
    </Properties></Object>
    <Object name="Gear"><Properties Count="3">
      <Property name="BaseFeature" type="App::PropertyLink"><Link value="Pad"/></Property>
      <Property name="Suppressed" type="App::PropertyBool"><Bool value="true"/></Property>
      <Property name="Shape" type="Part::PropertyPartShape"><Part file="Pad.brp"/></Property>
    </Properties></Object>
    <Object name="Helix"><Properties Count="3">
      <Property name="BaseFeature" type="App::PropertyLink"><Link value="Gear"/></Property>
      <Property name="Height" type="App::PropertyLength"><Float value="12"/></Property>
      <Property name="Shape" type="Part::PropertyPartShape"><Part file="Helix.brp"/></Property>
    </Properties></Object>
  </ObjectData>
</Document>
EOF
(cd "$WORK/fc" && cmake -E tar cf "$WORK/status.FCStd" --format=zip Document.xml Pad.brp Helix.brp) ||
  ui_fail "cannot pack the FreeCAD document"

# expect_notes: the placeholder's and the fallback's notes are information
# (mitcad#1040): their cells have the information badge, the warnings
# badge stays hidden (it would be logged).
expect_notes() {
  ui_expect_log "Timeline cell Gear: info, " "the placeholder's note is information"
  ui_expect_log "Timeline cell Helix: info, badge" "the fallback's note is information, not yellow"
  if grep "Warnings badge: " "$UI_LOG" | grep -qv "Warnings badge: none"; then
    ui_fail "an import note counted as a warning: $(grep "Warnings badge: " "$UI_LOG" | tail -1)"
  fi
  echo "ok   no warnings badge for import notes"
  # (Logged when it changes: never, while nothing fails.)
  if grep "Failed features: " "$UI_LOG" | grep -qv "Failed features: none"; then
    ui_fail "a failed feature: $(grep "Failed features: " "$UI_LOG" | tail -1)"
  fi
  echo "ok   nothing failed: no failures badge"
}

ui_start_display
ui_start_app --open "$WORK/status.FCStd" --no-native-dialogs
ui_expect_log "Import report: status.FCStd: " "imported in the import process"
ui_expect_log "1 fallback" "the helix is a fallback in the report"
ui_focus_dialog '^Import Report$'
ui_step "close the report (Enter)"         ui_key Return
ui_focus_main
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Timeline Helix at" "the timeline is logged"
# The import's warnings as every import's (mitcad#1040): a status line
# warning with their count, the details in the report.
ui_expect_log "Status warning: Imported status.FCStd. " "the import's warnings on the status line"
expect_notes

echo "--- Tooltips"
ui_mark
ui_step "hover over Helix"                 xdotool mousemove $(ui_logged_at "Timeline Helix")
ui_expect_new "Timeline tooltip: Helix: Stands in for Helix (not replayed: " "the fallback's tooltip says why"
ui_mark
ui_step "hover over Gear"                  xdotool mousemove $(ui_logged_at "Timeline Gear")
ui_expect_new "Timeline tooltip: Gear: Suppressed: Suppressed in FreeCAD; not replayed: " \
  "the placeholder's tooltip has the file's state"

echo "--- Saved and opened again"
ui_mark
ui_step "save as (Ctrl+Shift+S)"           ui_key ctrl+shift+s
ui_step "type the path, Enter"             ui_type_path "Save As" "$WORK/status.mitcad"
ui_expect_new "Saved $WORK/status.mitcad" "saved"
grep -q '"import_note": "Stands in for Helix (not replayed: ' "$WORK/status.mitcad" ||
  ui_fail "the note is not in the saved file"
echo "ok   the notes are in the design file"
ui_stop_app
ui_start_app --open "$WORK/status.mitcad" --no-native-dialogs
ui_expect_log "Opened $WORK/status.mitcad" "opened again"
expect_notes

ui_finish "UI import status test"
