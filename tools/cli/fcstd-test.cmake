# SPDX-License-Identifier: MIT
# FreeCAD .FCStd import through mitcad-cli on a small document made here: a
# Document.xml in FreeCAD's form with a box (hidden), a part turned and
# moved holding a cylinder, a link of the cylinder and a sketch (two lines
# from the root point and a circle, with a named length and signed
# distances), the shapes OCCT B-rep written by mitcad-geomtool, packed with
# cmake -E tar. The import is checked against a reference in the form of
# tools/freecad-export/dump.py (FreeCAD's measures, worked out by hand),
# saved and opened again; a wrong reference fails; --bodies-only leaves
# the sketch out.
#
# cmake -DCLI=<mitcad-cli> -DGEOMTOOL=<mitcad-geomtool> -DWORK=<folder> -P fcstd-test.cmake

foreach(var CLI GEOMTOOL WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "fcstd-test.cmake needs -D${var}=...")
  endif()
endforeach()

function(run expect)
  execute_process(COMMAND ${ARGN} WORKING_DIRECTORY "${WORK}" RESULT_VARIABLE status
                  OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(expect STREQUAL "ok" AND NOT status EQUAL 0)
    message(FATAL_ERROR "${ARGN} failed (${status}):\n${out}${err}")
  elseif(expect STREQUAL "fail" AND NOT status EQUAL 1)
    message(FATAL_ERROR "${ARGN} should fail with 1 (${status}):\n${out}${err}")
  endif()
  set(out "${out}${err}" PARENT_SCOPE)
endfunction()

# The output has each pattern (regular expressions).
function(expect what)
  foreach(pattern ${ARGN})
    if(NOT out MATCHES "${pattern}")
      message(FATAL_ERROR "${what}: no '${pattern}' in:\n${out}")
    endif()
  endforeach()
endfunction()

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}/doc")
run(ok "${GEOMTOOL}" make box 30 20 10 "${WORK}/doc/PartShape.brp")
run(ok "${GEOMTOOL}" make cylinder 5 10 "${WORK}/doc/PartShape1.brp")

# 90 degrees about Z: the quaternion (0, 0, sin 45, cos 45).
set(turn "Q0=\"0\" Q1=\"0\" Q2=\"0.7071067811865476\" Q3=\"0.7071067811865476\" A=\"1.5707963267948966\" Ox=\"0\" Oy=\"0\" Oz=\"1\"")
set(still "Q0=\"0\" Q1=\"0\" Q2=\"0\" Q3=\"1\" A=\"0\" Ox=\"0\" Oy=\"0\" Oz=\"1\"")
file(WRITE "${WORK}/doc/Document.xml" "<?xml version='1.0' encoding='utf-8'?>
<Document SchemaVersion=\"4\" ProgramVersion=\"1.0R39319 (Git)\" FileVersion=\"1\">
  <Properties Count=\"1\" TransientCount=\"0\">
    <Property name=\"Label\" type=\"App::PropertyString\"><String value=\"sample\"/></Property>
  </Properties>
  <Objects Count=\"5\" Dependencies=\"1\">
    <Object type=\"Part::Box\" name=\"Block\" id=\"1\"/>
    <Object type=\"App::Part\" name=\"Holder\" id=\"2\"/>
    <Object type=\"Part::Cylinder\" name=\"Peg\" id=\"3\"/>
    <Object type=\"App::Link\" name=\"PegCopy\" id=\"4\"/>
    <Object type=\"Sketcher::SketchObject\" name=\"Sketch\" id=\"5\"/>
  </Objects>
  <ObjectData Count=\"5\">
    <Object name=\"Block\"><Properties Count=\"3\">
      <Property name=\"Placement\" type=\"App::PropertyPlacement\"><PropertyPlacement Px=\"0\" Py=\"0\" Pz=\"0\" ${still}/></Property>
      <Property name=\"Shape\" type=\"Part::PropertyPartShape\"><Part file=\"PartShape.brp\"/></Property>
      <Property name=\"Visibility\" type=\"App::PropertyBool\"><Bool value=\"false\"/></Property>
    </Properties></Object>
    <Object name=\"Holder\"><Properties Count=\"2\">
      <Property name=\"Group\" type=\"App::PropertyLinkList\"><LinkList count=\"1\"><Link value=\"Peg\"/></LinkList></Property>
      <Property name=\"Placement\" type=\"App::PropertyPlacement\"><PropertyPlacement Px=\"100\" Py=\"0\" Pz=\"0\" ${turn}/></Property>
    </Properties></Object>
    <Object name=\"Peg\"><Properties Count=\"2\">
      <Property name=\"Label\" type=\"App::PropertyString\"><String value=\"Peg\"/></Property>
      <Property name=\"Shape\" type=\"Part::PropertyPartShape\"><Part file=\"PartShape1.brp\"/></Property>
    </Properties></Object>
    <Object name=\"PegCopy\" Extensions=\"True\">
      <Extensions Count=\"1\"><Extension type=\"App::LinkExtension\" name=\"LinkExtension\"/></Extensions>
      <Properties Count=\"3\">
      <Property name=\"LinkedObject\" type=\"App::PropertyXLink\"><XLink file=\"\" stamp=\"\" name=\"Peg\"/></Property>
      <Property name=\"LinkTransform\" type=\"App::PropertyBool\"><Bool value=\"false\"/></Property>
      <Property name=\"Placement\" type=\"App::PropertyPlacement\"><PropertyPlacement Px=\"0\" Py=\"50\" Pz=\"0\" ${still}/></Property>
    </Properties></Object>
    <Object name=\"Sketch\"><Properties Count=\"3\">
      <Property name=\"Placement\" type=\"App::PropertyPlacement\"><PropertyPlacement Px=\"0\" Py=\"0\" Pz=\"20\" ${still}/></Property>
      <Property name=\"Geometry\" type=\"Part::PropertyGeometryList\"><GeometryList count=\"3\">
        <Geometry type=\"Part::GeomLineSegment\"><LineSegment StartX=\"0\" StartY=\"0\" StartZ=\"0\" EndX=\"30\" EndY=\"0\" EndZ=\"0\"/><Construction value=\"0\"/></Geometry>
        <Geometry type=\"Part::GeomLineSegment\"><LineSegment StartX=\"30\" StartY=\"0\" StartZ=\"0\" EndX=\"30\" EndY=\"20\" EndZ=\"0\"/><Construction value=\"0\"/></Geometry>
        <Geometry type=\"Part::GeomCircle\"><Circle CenterX=\"10\" CenterY=\"10\" CenterZ=\"0\" NormalX=\"0\" NormalY=\"0\" NormalZ=\"1\" AngleXU=\"0\" Radius=\"5\"/><Construction value=\"0\"/></Geometry>
      </GeometryList></Property>
      <Property name=\"Constraints\" type=\"Sketcher::PropertyConstraintList\"><ConstraintList count=\"7\">
        <Constrain Name=\"\" Type=\"1\" Value=\"0\" First=\"0\" FirstPos=\"1\" Second=\"-1\" SecondPos=\"1\" Third=\"-2000\" ThirdPos=\"0\" IsDriving=\"1\" IsActive=\"1\"/>
        <Constrain Name=\"\" Type=\"1\" Value=\"0\" First=\"0\" FirstPos=\"2\" Second=\"1\" SecondPos=\"1\" Third=\"-2000\" ThirdPos=\"0\" IsDriving=\"1\" IsActive=\"1\"/>
        <Constrain Name=\"\" Type=\"2\" Value=\"0\" First=\"0\" FirstPos=\"0\" Second=\"-2000\" SecondPos=\"0\" Third=\"-2000\" ThirdPos=\"0\" IsDriving=\"1\" IsActive=\"1\"/>
        <Constrain Name=\"width\" Type=\"6\" Value=\"30\" First=\"0\" FirstPos=\"0\" Second=\"-2000\" SecondPos=\"0\" Third=\"-2000\" ThirdPos=\"0\" IsDriving=\"1\" IsActive=\"1\"/>
        <Constrain Name=\"\" Type=\"11\" Value=\"5\" First=\"2\" FirstPos=\"0\" Second=\"-2000\" SecondPos=\"0\" Third=\"-2000\" ThirdPos=\"0\" IsDriving=\"1\" IsActive=\"1\"/>
        <Constrain Name=\"\" Type=\"7\" Value=\"10\" First=\"2\" FirstPos=\"3\" Second=\"-2000\" SecondPos=\"0\" Third=\"-2000\" ThirdPos=\"0\" IsDriving=\"1\" IsActive=\"1\"/>
        <Constrain Name=\"\" Type=\"8\" Value=\"-10\" First=\"2\" FirstPos=\"3\" Second=\"0\" SecondPos=\"1\" Third=\"-2000\" ThirdPos=\"0\" IsDriving=\"1\" IsActive=\"1\"/>
      </ConstraintList></Property>
    </Properties></Object>
  </ObjectData>
</Document>
")
run(ok "${CMAKE_COMMAND}" -E chdir "${WORK}/doc"
    "${CMAKE_COMMAND}" -E tar cf "${WORK}/sample.FCStd" --format=zip Document.xml PartShape.brp PartShape1.brp)

# FreeCAD's measures: the box 30 x 20 x 10; the cylinder of radius 5 and
# height 10 (volume 250 pi, area 150 pi) at (100, 0, 5) in the turned part,
# and at (0, 50, 5) through the link.
function(reference file volume)
  file(WRITE "${file}" "{\"format\": \"mitcad-freecad-dump\", \"version\": 1, \"objects\": [
  {\"name\": \"Block\", \"shape\": {\"solids\": 1, \"faces\": 6, \"volume\": 6000.0, \"area\": 2200.0,
   \"world_center\": [15.0, 10.0, 5.0]}},
  {\"name\": \"Holder\", \"shape\": {\"solids\": 1, \"faces\": 3, \"volume\": ${volume}, \"area\": 471.2388980384690,
   \"world_center\": [100.0, 0.0, 5.0]}},
  {\"name\": \"Peg\", \"shape\": {\"solids\": 1, \"faces\": 3, \"volume\": 785.3981633974483, \"area\": 471.2388980384690,
   \"world_center\": [100.0, 0.0, 5.0]}},
  {\"name\": \"PegCopy\", \"link\": {\"shape\": {\"solids\": 1, \"faces\": 3, \"volume\": 785.3981633974483,
   \"area\": 471.2388980384690, \"world_center\": [0.0, 50.0, 5.0]}}}]}
")
endfunction()
reference("${WORK}/reference.json" 785.3981633974483)
run(ok "${CLI}" import-fcstd "${WORK}/sample.FCStd" --reference "${WORK}/reference.json"
    --save "${WORK}/sample.mitcad" --report "${WORK}/report.json")
expect("import-fcstd"
  "sample\\.FCStd \\(FreeCAD 1\\.0R39319 \\(Git\\), schema 4\\): 5 objects: 2 bodies, 1 components, 1 links, 1 parametric, 0 partial, 0 fallback, 0 included, 0 skipped"
  "Block +Part::Box +body +-> F1\\.b0"
  "Peg +Part::Cylinder +body +-> F2\\.b0 in Peg"
  "Sketch +Sketcher::SketchObject +parametric: a sketch \\(on xy\\)"
  "sketch Sketch \\(F3\\): parametric on xy, 3 geometries and 7 constraints -> 7 entities, 1 constraints, 4 dimensions, error 0\\.0e0 mm"
  "body Block \\(F1\\.b0\\): solid, volume 6000\\.000 mm3, area 2200\\.000 mm2, hidden"
  "reference: 12 measures compared, 0 differ")
file(READ "${WORK}/report.json" report)
string(JSON outcome GET "${report}" items 3 outcome)
if(NOT outcome STREQUAL "occurrence")
  message(FATAL_ERROR "PegCopy came in as ${outcome}")
endif()
# The named length keeps its name for the parameters; the negative
# vertical distance runs the other way, 10 mm.
string(JSON name GET "${report}" sketches 0 dimension_sources 0 name)
string(JSON last GET "${report}" sketches 0 dimension_sources 3 mitcad)
if(NOT name STREQUAL "width" OR NOT last MATCHES "^10(\\.0*)?$")
  message(FATAL_ERROR "dimension sources: ${name}, ${last}")
endif()

# The bodies only: the sketch left out.
run(ok "${CLI}" import-fcstd "${WORK}/sample.FCStd" --bodies-only)
expect("import-fcstd --bodies-only" "Sketch +Sketcher::SketchObject +skipped +: a sketch")

# The saved project opens with the same bodies.
run(ok "${CLI}" info "${WORK}/sample.mitcad")
expect("info" "Block \\(F1\\.b0\\): volume 6000\\.000 mm3" "Peg \\(F2\\.b0\\): volume 785\\.398 mm3")

# A reference that differs fails, and the JSON report says so.
reference("${WORK}/wrong.json" 785.4)
run(fail "${CLI}" import-fcstd "${WORK}/sample.FCStd" --reference "${WORK}/wrong.json" --json)
expect("wrong reference" "\"pass\": false" "Holder: volume 785\\.398163397448[0-9]* against FreeCAD's 785\\.4")

# Not a FreeCAD document.
run(fail "${CLI}" import-fcstd "${WORK}/doc/Document.xml")
expect("not an archive" "not a zip file")

# The history: a Body whose sketch of a 30 x 20 rectangle is padded 10 mm
# (the pad's and the Body's stored shapes are that box). The pad is
# replayed as an extrusion and checked against its stored shape; a pocket
# of a type Mitcad has not (a helix) falls back to its stored shape.
file(MAKE_DIRECTORY "${WORK}/history")
run(ok "${GEOMTOOL}" make box 30 20 10 "${WORK}/history/Pad.brp")
run(ok "${GEOMTOOL}" make box 30 20 12 "${WORK}/history/Helix.brp")
set(line "<Geometry type=\"Part::GeomLineSegment\"><LineSegment StartX=\"@0\" StartY=\"@1\" StartZ=\"0\" EndX=\"@2\" EndY=\"@3\" EndZ=\"0\"/><Construction value=\"0\"/></Geometry>")
set(lines "")
foreach(segment "0;0;30;0" "30;0;30;20" "30;20;0;20" "0;20;0;0")
  set(xml "${line}")
  foreach(i RANGE 3)
    list(GET segment ${i} v)
    string(REPLACE "@${i}" "${v}" xml "${xml}")
  endforeach()
  string(APPEND lines "${xml}")
endforeach()
set(joins "")
foreach(pair "0;1" "1;2" "2;3" "3;0")
  list(GET pair 0 a)
  list(GET pair 1 b)
  string(APPEND joins "<Constrain Name=\"\" Type=\"1\" Value=\"0\" First=\"${a}\" FirstPos=\"2\" Second=\"${b}\" SecondPos=\"1\" Third=\"-2000\" ThirdPos=\"0\" IsDriving=\"1\" IsActive=\"1\"/>")
endforeach()
file(WRITE "${WORK}/history/Document.xml" "<?xml version='1.0' encoding='utf-8'?>
<Document SchemaVersion=\"4\" ProgramVersion=\"1.0R39319 (Git)\" FileVersion=\"1\">
  <Properties Count=\"0\"></Properties>
  <Objects Count=\"4\" Dependencies=\"1\">
    <Object type=\"PartDesign::Body\" name=\"Body\" id=\"1\"/>
    <Object type=\"Sketcher::SketchObject\" name=\"Sketch\" id=\"2\"/>
    <Object type=\"PartDesign::Pad\" name=\"Pad\" id=\"3\"/>
    <Object type=\"PartDesign::AdditiveHelix\" name=\"Helix\" id=\"4\"/>
  </Objects>
  <ObjectData Count=\"4\">
    <Object name=\"Body\"><Properties Count=\"4\">
      <Property name=\"Label\" type=\"App::PropertyString\"><String value=\"Plate\"/></Property>
      <Property name=\"Group\" type=\"App::PropertyLinkList\"><LinkList count=\"3\"><Link value=\"Sketch\"/><Link value=\"Pad\"/><Link value=\"Helix\"/></LinkList></Property>
      <Property name=\"Tip\" type=\"App::PropertyLink\"><Link value=\"Helix\"/></Property>
      <Property name=\"Shape\" type=\"Part::PropertyPartShape\"><Part file=\"Helix.brp\"/></Property>
    </Properties></Object>
    <Object name=\"Sketch\"><Properties Count=\"2\">
      <Property name=\"Geometry\" type=\"Part::PropertyGeometryList\"><GeometryList count=\"4\">${lines}</GeometryList></Property>
      <Property name=\"Constraints\" type=\"Sketcher::PropertyConstraintList\"><ConstraintList count=\"4\">${joins}</ConstraintList></Property>
    </Properties></Object>
    <Object name=\"Pad\"><Properties Count=\"4\">
      <Property name=\"Profile\" type=\"App::PropertyLinkSub\"><LinkSub value=\"Sketch\" count=\"0\"></LinkSub></Property>
      <Property name=\"Length\" type=\"App::PropertyLength\"><Float value=\"10\"/></Property>
      <Property name=\"Type\" type=\"App::PropertyEnumeration\"><Integer value=\"0\"/></Property>
      <Property name=\"Shape\" type=\"Part::PropertyPartShape\"><Part file=\"Pad.brp\"/></Property>
    </Properties></Object>
    <Object name=\"Helix\"><Properties Count=\"2\">
      <Property name=\"BaseFeature\" type=\"App::PropertyLink\"><Link value=\"Pad\"/></Property>
      <Property name=\"Shape\" type=\"Part::PropertyPartShape\"><Part file=\"Helix.brp\"/></Property>
    </Properties></Object>
  </ObjectData>
</Document>
")
run(ok "${CMAKE_COMMAND}" -E chdir "${WORK}/history"
    "${CMAKE_COMMAND}" -E tar cf "${WORK}/history.FCStd" --format=zip Document.xml Pad.brp Helix.brp)
run(ok "${CLI}" import-fcstd "${WORK}/history.FCStd" --save "${WORK}/history.mitcad")
expect("import-fcstd history"
  "history\\.FCStd \\(FreeCAD 1\\.0R39319 \\(Git\\), schema 4\\): 4 objects: 1 bodies, 0 components, 0 links, 2 parametric, 0 partial, 1 fallback, 0 included, 0 skipped"
  "feature Pad \\(F2\\): parametric extrude PartDesign::Pad, volume 6000\\.000 mm3 against FreeCAD's 6000\\.000, 1 solids, off by 0\\.0e0"
  "feature Helix \\(F3\\): fallback base PartDesign::AdditiveHelix"
  "body Plate \\(F2\\.b0\\): solid, volume 7200\\.000 mm3")
run(ok "${CLI}" info "${WORK}/history.mitcad")
expect("info history" "F2 Pad \\(extrude\\): ok" "F3 Helix \\(base\\): ok" "Plate \\(F2\\.b0\\): volume 7200\\.000 mm3")
