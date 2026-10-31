# SPDX-License-Identifier: MIT
# vcpkg's opencascade port (microsoft/vcpkg at the baseline in vcpkg.json)
# with Mitcad's patches 0006 and later (see ../README.md).
string(REPLACE "." "_" VERSION_STR "V${VERSION}")
vcpkg_from_github(
    OUT_SOURCE_PATH SOURCE_PATH
    REPO Open-Cascade-SAS/OCCT
    REF "${VERSION_STR}"
    SHA512 bbe7099071cbf5397940ebc6e66ec05f8023d5e5aae6142870e14b93aa6f8f94c30980ef421e717f0fbfbc23b3520c3ccfe8a939c4caba3ccbf325060e26eb52
    HEAD_REF master
    PATCHES
        0001-cmake-keep-build-use-vcpkg-explicit.patch
        0002-cmake-load-exported-package-dependencies.patch
        0003-image-remove-freeimage-msvc-autolink.patch
        0004-cmake-add-additional-path-extraction-for-OpenCASCADE.patch
        0005-drop-bin-letter.patch
        0006-shapeupgrade-unifysamedomain-seam-search-by-vertex.patch
        0007-brepcheck-analyzer-no-search-in-face.patch
        0008-brepcheck-wire-seam-edges-once-per-face.patch
        0009-bopalgo-face-face-adaptors-once-per-face.patch
        0010-brepblend-surfrst-line-on-both-restrictions.patch
        0011-chfi3d-fillet-edge-overflow.patch
        0012-chfi3d-closed-stripe-end-on-vertex.patch
        0013-chfi3d-overflow-across-faces-and-overlapping-roundings.patch
        0014-chfi3d-contact-along-face-edge.patch
        0015-brepblend-surfrst-smaller-step-on-failed-reframing.patch
        0016-chfi3d-rounding-rolls-on-anothers-contact-line.patch
        0017-chfi3d-contact-along-face-edge-ends-at-vertices.patch
        0018-chfi3d-corner-where-the-end-face-is-split-in-one-plane.patch
        0019-chfi3d-undone-moresurfdata-corner-fails.patch
        0020-chfi3d-crash-fixes.patch
        0021-chfi3d-corner-of-contour-closed-at-sharp-vertex.patch
        0022-chfi3d-round-edges-asked-for-near-tangent.patch
        0023-chfi3d-meeting-point-of-periodic-contact-lines.patch
        0024-chfi3d-order-of-periodic-kpart-surfaces.patch
        0025-chfi3d-end-cut-by-faces-around-a-cylinder-corner.patch
        0026-chfi3d-onsame-end-cut-by-several-faces.patch
        0030-standard-failed-allocation-throws.patch
        0031-osd-threadpool-failed-allocations-to-launching-thread.patch
        0032-ncollection-containers-intact-when-allocation-fails.patch
        0033-brepcheck-edge-curve-adaptor-per-context.patch
        0034-osd-signal-handler-recursive-mutex.patch
        0050-chfi3d-kpart-end-runs-into-end-face.patch
        0051-chfi3d-rolling-on-across-surfdatas.patch
        0052-chfi3d-rolling-start-beside-tangent-face.patch
        0053-chfi3d-corner-after-overflow-onto-face-beyond.patch
        0054-chfi3d-contact-along-a-part-of-a-face-edge.patch
        0055-chfi3d-kpart-blend-cut-by-face-beyond.patch
        0056-chfi3d-corner-cutting-the-last-two-walked-pieces.patch
        0057-chfi3d-kpart-gaps-in-order-across-closed-lines.patch
        0058-chfi3d-rounding-rolls-on-a-convex-far-edge.patch
        0059-chfi3d-kpart-blend-cut-by-the-face-at-the-end.patch
        0060-shapefix-intersecting-edges-once-per-face.patch
        0070-chfi3d-kpart-cut-to-end-where-the-narrow-face-vanishes.patch
        0071-chfi3d-kpart-cut-by-several-faces-beyond.patch
        0072-chfi3d-kpart-cuts-on-closed-spines.patch
        0073-chfi3d-kpart-pieces-over-a-hole-are-one-face.patch
        0074-chfi3d-walked-blend-cut-by-the-face-at-the-end.patch
        0075-blendfunc-constrad-rolling-ball-section.patch
        0076-chfi3d-extension-rolls-on-the-faces-own-surface.patch
        0077-chfi3d-walk-past-the-spine-ends-before-a-singular-section.patch
        0090-chfi3d-double-interference-at-vertex-kind.patch
        0091-topopebrepbuild-point-uses-within-array.patch
        0100-brepoffset-faces-vanishing-within-the-offset.patch
        0110-shapeupgrade-unifysamedomain-chain-not-concatenated.patch
        0120-brepcheck-face-wire-boxes-of-conics-past-their-range.patch
        0121-brepcheck-shell-orientation-edges-listed-once-per-face.patch
        0130-chfi3d-rounding-vanishing-at-a-tangent-end.patch
        0140-chfi3d-contact-along-a-closed-face-edge.patch
        0141-chfi3d-onsame-end-second-contact-beyond-the-edge.patch
        0142-chfi3d-kpart-cut-by-faces-beyond-that-grow.patch
        0143-chfi3d-chamfer-corner-beyond-the-common-face.patch
        0144-chfi3d-closed-contact-split-at-the-edges-vertex.patch
        0145-chfi3d-kpart-pieces-in-their-faces-periods.patch
        0146-chfi3d-kpart-pieces-one-face-round-a-closed-spine.patch
        0147-chfi3d-onsame-end-prolonged-along-a-seam.patch
        0148-chfi3d-chamfer-as-wide-as-a-ledge-along-a-line-and-an-arc.patch
        0149-chfi3d-end-snapped-onto-a-closed-edges-vertex-on-a-periodic-face.patch
        0150-chfi3d-onsame-corner-beside-a-split-face.patch
        0151-chfi3d-corner-on-an-end-face-split-in-one-plane.patch
        0152-chfi3d-closed-rolling-start-on-a-seam-vertex.patch
        0153-chfi3d-roundings-meeting-on-one-contact-line.patch
        0160-chfi3d-walked-blend-over-a-rounded-rim.patch
        0170-brepblend-surfrst-rolling-ball-and-pivot.patch
        0171-chfi3d-rolling-ball-turns-about-a-vertex.patch
        0172-chfi3d-rolled-blend-cut-by-the-face-at-the-end.patch
        0180-chfi3d-chains-stop-where-the-edges-turn.patch
        0190-chfi3d-consumed-face-round-a-closed-stripe.patch
        0200-chfi3d-walked-blend-about-a-standing-contact.patch
        0210-chfi3d-mitred-ends-at-a-short-sharp-edge.patch
        0211-chfi3d-end-cut-by-a-periodic-face-across-its-seam.patch
        0220-shapeupgrade-unifysamedomain-seams-half-a-period-apart.patch
        0230-chfi3d-contact-in-the-adjacent-face-period.patch
        0240-chfi3d-extension-on-the-faces-own-surface-away-from-its-seam.patch
        0241-chfi3d-roundings-meeting-across-a-notch.patch
        0250-chfi3d-walk-on-along-a-face-of-the-spine.patch
        0260-chfi3d-fillet-carried-across-a-notch-in-its-rim.patch
        0270-chfi3d-roundings-meeting-on-one-contact-line-of-a-face-going-on.patch
        0271-chfi3d-roundings-cut-by-each-other-where-their-contacts-cross.patch
        0280-chfi3d-onsame-end-where-the-face-at-the-end-turns-tangentially.patch
        0281-chfi3d-walk-round-the-vertex-past-a-short-edge.patch
        0282-chfi3d-end-cut-past-the-bounds-of-the-faces-at-the-end.patch
        0283-chfi3d-standing-end-piece-one-face-with-the-one-before.patch
        0290-chfi3d-walk-round-the-vertex-through-a-face-met-at-a-point.patch
        0291-chfi3d-end-past-the-spine-where-a-contact-ran-off-its-face.patch
        0300-chfi3d-prolonged-end-edge-across-the-opposite-faces-seam.patch
        0320-chfi3d-where-a-fillet-stops.patch
        0321-chfi3d-closed-rounding-consuming-its-face-ends-at-the-vertex.patch
        0322-chfi3d-open-stripe-end-at-the-vertex-of-the-edge-its-contact-runs-along.patch
        0330-chfi3d-which-corner-a-fillet-fails-at.patch
        0331-chfi3d-rounding-rolled-on-a-straight-edge-exactly.patch
        0340-chfi3d-onsame-end-cut-by-the-face-of-the-edge-the-ball-rolls-on.patch
        0341-chfi3d-rolling-blend-walked-again-across-a-seam-past-its-ends.patch
        0342-chfi3d-rounding-rolled-on-past-its-edge-cut-by-the-faces-at-the-ends.patch
        0350-chfi3d-regularity-only-where-the-faces-are-tangent.patch
        0351-chfi3d-closed-chain-consuming-a-face-all-round.patch
        0352-chfi3d-contact-on-a-face-beyond-at-an-acute-angle.patch
        0360-bopalgo-wiresplitter-tangent-edges-measured-at-one-distance.patch
        0380-chfi3d-chamfer-end-where-the-face-at-the-end-is-tangent-to-both-faces.patch
        0381-chfi3d-chamfer-guide-deformed-by-a-hundredth-of-its-distance-at-most.patch
        0390-chfi3d-rolling-ball-touches-the-next-edge-before-the-vertex.patch
        0391-brepblend-rstrst-rolling-ball-on-two-edges.patch
        0400-chfi3d-rounding-vanishing-at-its-end-stops-at-the-vertex.patch
        0401-chfi3d-end-cut-across-the-seam-line-of-a-periodic-face.patch
        0410-chfi3d-rounding-ending-in-the-face-it-runs-into.patch
        0420-chfi3d-kpart-round-a-closed-edge-on-a-face-with-a-stepped-seam.patch
        0430-chfi3d-rounding-rolls-before-another-roundings-face.patch
        0431-chfi3d-rolled-onsame-end-at-the-end-of-its-contact-curve.patch
        0432-chfi3d-roundings-crease-over-a-narrow-common-face.patch
        0440-chfi3d-contact-along-a-chain-of-edges-of-a-consumed-strip.patch
        0450-chfi3d-rounding-face-turning-tangentially-into-the-end-face.patch
        0460-chfi3d-onsame-end-on-an-edge-of-a-consumed-corner-face.patch
        0480-geomint-open-line-parts-beyond-its-vertices.patch
        0481-chfi3d-kpart-gap-across-a-closed-spines-start-split-there.patch
        0490-chfi3d-onsame-end-on-a-trimmed-face-past-its-bounds.patch
        0500-chfikpart-fillet-between-coaxial-surfaces-of-revolution.patch
        0501-chfi3d-kpart-cut-round-a-cut-out-under-an-overhanging-rim.patch
        0502-chfi3d-kpart-cut-by-the-end-face-over-a-gap.patch
        0503-chfi3d-two-roundings-meeting-on-an-edge-within-its-tolerance.patch
        0510-boptools-pcurve-into-the-period-inside-a-face-spanning-many.patch
        0511-inttools-fclass2d-every-copy-on-a-face-spanning-many-periods.patch
        0512-intcurvesurface-quadric-roots-sampled-by-knot-spans.patch
        0530-chfi3d-chamfer-guide-keeps-to-its-edges-at-the-blends-beside.patch
        0531-chfi3d-chamfer-wider-than-a-narrow-face-cut-by-the-faces-beyond.patch
        0540-chfi3d-rounding-running-on-into-an-earlier-roundings-tube.patch
        0550-chfi3d-end-over-pieces-where-the-end-faces-edge-is-in-two.patch
        0560-boptools-pcurve-checked-at-its-own-copy-in-the-face.patch
        0580-chfi3d-kpart-contact-along-a-concave-edge-off-its-face.patch
        0581-chfi3d-roundings-joined-across-a-rib-with-square-sides.patch
        0590-shapeupgrade-unifysamedomain-wire-repeating-itself-given-up.patch
        0591-adaptor3d-bspline-samples-within-the-adaptors-range.patch
        0592-shapeupgrade-unifysamedomain-planarity-once-per-surface.patch
        0593-extrema-genextps-grid-of-very-many-knot-spans-sampled-evenly.patch
        0600-chfi3d-onsame-end-cut-along-its-side-by-the-face-at-the-end.patch
        0620-chfi3d-contact-off-its-face-before-the-end-rolls-on-the-edge.patch
        0650-chfi3d-kpart-cut-by-a-long-chain-of-faces-beyond.patch
        0651-chfi3d-walk-from-a-seam-and-extension-ending-where-it-fails.patch
        0670-chfi3d-standing-ball-sphere-in-a-gap-between-pieces.patch
        0671-chfi3d-kpart-chamfer-cut-across-its-width-by-faces-beyond-at-both-ends.patch
        0680-shapeupgrade-unifysamedomain-faces-round-a-period-without-winding.patch
        0690-chfi3d-end-on-a-face-along-the-line-of-its-seam.patch
        0691-chfi3d-onsame-end-on-a-tangent-face-the-crossing-nearest-the-corner.patch
        0692-chfi3d-kpart-contact-end-within-a-vertex-tolerance-at-the-vertex.patch
        0720-chfi3d-face-at-the-end-passed-wholly-beyond-is-consumed.patch
        0721-chfi3d-onsame-prolonged-edge-own-curve-on-both-surfaces.patch
        0700-chfi3d-blends-cut-by-a-narrow-face-they-roll-over.patch
        0740-chfi3d-end-face-split-round-the-vertex-pieces-of-one-surface.patch
        0741-chfi3d-kpart-contact-end-at-a-vertex-on-the-edge-to-the-spines-end.patch
        0742-chfi3d-guide-past-an-open-end-along-the-end-edges-own-curve.patch
        0750-chfi3d-thread-crest-rounded-on-both-edges-cut-by-the-ends.patch
        0770-chfi3d-rolling-on-an-edge-into-the-end-vertex-where-it-vanishes.patch
        0771-chfi3d-contour-stops-where-the-rounding-vanishes.patch
        0772-chfi3d-stripes-joined-across-a-corner-of-the-solid.patch
        0790-chfi3d-tangent-end-face-cuts-the-blend-back-along-the-stripe.patch
        0800-chfi3d-onsame-cap-points-in-one-period-of-a-stored-cylinder.patch
        0801-chfi3d-tangent-faces-within-ten-times-the-edge-tolerance.patch
        0802-chfi3d-contact-on-its-own-edge-near-a-vertex-passes-it.patch
        0803-chfi3d-onsame-cap-leaving-the-prolongation-tangentially.patch
        0820-chfi3d-stripes-joined-across-a-corner-one-face-each.patch
        0821-chfi3d-lens-rounded-on-both-edges-s-shaped-section.patch
        0822-chfi3d-rounding-rolling-on-both-edges-into-a-lens-end.patch
        0840-chfi3d-contour-never-takes-its-own-edge-again.patch
        0860-chfi3d-contour-runs-on-into-a-near-tangent-edge-asked-for.patch
        0861-chfi3d-contact-at-the-vertex-where-the-blend-vanishes-cuts-the-own-edge.patch
        0880-chfi3d-start-across-a-strip-the-ball-cannot-touch.patch
        0890-chfi3d-onsame-end-across-a-sliver-at-the-vertex.patch
        0891-chfi3d-bridge-over-a-gap-round-a-closed-spine-and-across-its-start.patch
        0870-chfi3d-contact-off-a-narrow-face-across-its-tangent-edge-keeps-its-surface.patch
        0900-chfi3d-walked-onsame-piece-generated-by-the-spine-edges.patch
        0901-chfi3d-cap-intersection-ends-where-the-walk-first-passes-the-end.patch
        0902-chfi3d-rounding-vanishing-where-its-contact-crosses-onto-a-tangent-face.patch
        0903-chfi3d-rolling-contact-crossing-onto-the-next-spine-face-goes-on.patch
        0904-chfi3d-rounding-rolled-on-past-one-end-along-a-line-or-circle.patch
        0905-chfi3d-ball-turning-about-a-vertex-of-the-edges-it-rolls-on.patch
        0906-chfi3d-next-overflow-start-where-the-first-walk-fails.patch
        0907-brepblend-pivot-released-where-a-step-lands-on-the-tangency.patch
        0908-chfi3d-overflow-start-sampled-finer-where-none-is-found.patch
        0909-chfi3d-rounding-vanishing-a-hair-short-of-the-vertex-it-rolls-into.patch
        0920-chfi3d-radius-law-of-the-callers-along-a-contour.patch
        0910-chfi3d-onsame-cap-from-the-contacts-point-on-the-prolonged-edge.patch
        0911-chfi3d-onsame-end-where-the-face-at-the-end-goes-on-tangentially.patch
        0930-chfi3d-rounding-riding-anothers-contact-line-against-it.patch
        0931-chfi3d-seam-run-on-to-a-corners-cut-past-its-end.patch
        0932-chfi3d-walk-cut-before-a-singular-section-ends-there.patch
        0933-chfi3d-ends-joined-over-a-narrow-face-that-pokes-through-the-blend.patch
        0940-chfi3d-rounding-over-a-narrow-face-cut-by-the-face-beyond-its-concave-edge.patch
        0941-chfi3d-kpart-chain-crossings-in-the-period-after-the-gaps-start.patch
        0942-shapefix-intersecting-wires-cut-reported-only-where-cut.patch
        0950-chfi3d-roundings-consuming-the-face-between-them-meet-on-one-contact-edge.patch
        0951-chfi3d-rounding-ending-at-a-step-back-of-its-wall-closes-with-a-torus.patch
        0960-projlib-projection-boundary-bisection-stops-at-adjacent-parameters.patch
        0961-chfi3d-rounding-vanishing-where-its-walk-stopped-on-its-own-edge.patch
        0962-chfi3d-step-end-torus-cut-on-its-direct-frame-and-only-at-a-recess.patch
        0970-chfi3d-chain-past-a-wall-corner-tighter-than-the-ball-vertex-blend-and-torus.patch
        0980-chfi3d-rounding-across-slight-creases-of-its-faces-at-one-section.patch
        0990-chfi3d-rolled-end-cut-by-the-faces-at-the-end-past-the-balls-tangency.patch
        1000-chfi3d-chamfer-across-a-slight-concave-crease-over-a-narrow-face-grown-faces-continued.patch
        1001-chfi3d-guide-stretch-only-where-the-spine-turns-at-the-vertex.patch
        1010-chfi3d-kpart-closed-rim-gap-round-to-its-own-first-edge.patch
        1011-chfi3d-intersection-at-end-across-the-end-faces-seam-in-one-period.patch
        1020-chfi3d-kink-into-a-face-going-on-smoothly-into-the-end-face-also-at-a-pieces-start.patch
        1030-geomfill-guide-trihedron-ac-second-derivative-of-the-binormal.patch
        1040-shapeupgrade-unifysamedomain-signed-angle-to-a-reversed-axis.patch
        1050-shapefix-self-intersecting-wire-edges-read-once.patch
        1060-chfi3d-under-anothers-ball-not-past-its-other-face.patch
        1070-brepmesh-delabella-exact-signs.patch
        1080-brepmesh-delabella-exact-signs-in-place.patch
        1081-brepmesh-face-checker-wire-boxes.patch
        1090-chfi3d-sharp-edge-limited-only-by-a-common-point-on-it.patch
)

if (VCPKG_LIBRARY_LINKAGE STREQUAL "dynamic")
    set(BUILD_TYPE "Shared")
else()
    set(BUILD_TYPE "Static")
endif()

vcpkg_check_features(OUT_FEATURE_OPTIONS FEATURE_OPTIONS
    FEATURES
        freeimage   USE_FREEIMAGE
        freetype    USE_FREETYPE
        rapidjson   USE_RAPIDJSON
        tbb         USE_TBB
        vtk         USE_VTK
)

# We turn off BUILD_MODULE_Draw as it requires TCL 8.6 and TK 8.6 specifically which conflicts with vcpkg only having TCL 9.0 
# And pre-built ActiveTCL binaries are behind a marketing wall :(
# We use the Unix install layout for Windows as it matches vcpkg
vcpkg_cmake_configure(
    SOURCE_PATH "${SOURCE_PATH}"
    OPTIONS
        ${FEATURE_OPTIONS}
        -DBUILD_LIBRARY_TYPE=${BUILD_TYPE}
        -DBUILD_MODULE_Draw=OFF
        -DBUILD_DOC_Overview=OFF
        -DINSTALL_DIR_LAYOUT=Unix
        -DINSTALL_DIR_DOC=share/trash
        -DINSTALL_DIR_SCRIPT=share/trash # not relocatable
        -DINSTALL_TEST_CASES=OFF
        -DUSE_TK=OFF
)

vcpkg_cmake_install()
vcpkg_copy_pdbs()
vcpkg_cmake_config_fixup(CONFIG_PATH lib/cmake/opencascade)

#make occt includes relative to source_file
file(GLOB extra_headers
    LIST_DIRECTORIES false
    RELATIVE "${CURRENT_PACKAGES_DIR}/include/opencascade"
    "${CURRENT_PACKAGES_DIR}/include/opencascade/*.h"
)
list(JOIN extra_headers "|" extra_headers)
file(GLOB files "${CURRENT_PACKAGES_DIR}/include/opencascade/*.[hgl]xx")
foreach(file_name IN LISTS files)
    vcpkg_replace_string("${file_name}" "(# *include) <([a-zA-Z0-9_]*[.][hgl]xx|${extra_headers})>" [[\1 "\2"]] REGEX IGNORE_UNCHANGED)
endforeach()

if(VCPKG_LIBRARY_LINKAGE STREQUAL "static")
    vcpkg_replace_string("${CURRENT_PACKAGES_DIR}/include/opencascade/Standard_Macro.hxx" "defined(OCCT_STATIC_BUILD)" "(1)")
endif()

file(REMOVE_RECURSE "${CURRENT_PACKAGES_DIR}/debug/include")
file(REMOVE_RECURSE "${CURRENT_PACKAGES_DIR}/debug/share")
file(REMOVE_RECURSE "${CURRENT_PACKAGES_DIR}/share/opencascade/samples/qt")
file(REMOVE_RECURSE "${CURRENT_PACKAGES_DIR}/share/trash")

vcpkg_install_copyright(
    FILE_LIST
        "${SOURCE_PATH}/LICENSE_LGPL_21.txt"
        "${SOURCE_PATH}/OCCT_LGPL_EXCEPTION.txt"
)
