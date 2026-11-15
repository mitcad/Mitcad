<!-- SPDX-License-Identifier: MIT -->
# The patches against OCCT master

Each of the port's 311 patch files compared with OCCT's current master
(mitcad#1875, part 2), as a starting point for a move to OCCT 8.1 or for
taking single fixes from upstream. Back to the [port's README](../README.md).

- **Date:** 15.11.2026.
- **Compared with:** Open-Cascade-SAS/OCCT `master` at
  `4dbfdcc448c0b3e565488afcd626c84483172db9` (OCCT#1741, 11.11.2026); our
  base is the tag `V8_0_1` (30.7.2026).
- **Method:** every file the patches touch was fetched at `V8_0_1` and at
  that master commit. The patches were replayed in the portfile's order on
  the 8.0.1 files, recording for each line whether it is OCCT's or which
  patch added it. A patch that neither removes an OCCT line nor inserts
  next to one patches only code of our earlier patches (*ours only*). For
  every other patch, the 8.0.1-to-master changes within a dozen lines of
  the OCCT lines it touches were read, with the patch's header and hunks;
  where master changed the patched function, the upstream change was read
  and named from the files' commit history since 8.0.1. Nothing was built
  or run: *fixed upstream* means master addresses the same fault in the
  same code, read, not tested; the one-off runs of OCCT's own tests
  (part 1 of the ticket) are the check. A fault master might fix somewhere
  far from the patched code is not found this way.

## Summary

| Class | Patches |
|---|---|
| fixed upstream | 10 |
| partly | 2 |
| still needed | 234 |
| ours only | 65 |

- **Fixed upstream:** 0004 (package config relocation, OCCT#1644), 0014
  (a contact along a face's far edge, the consumed face, OCCT#1293 and
  #1449: the cases bug25478_1/2), 0022 and 0860 (near-tangent edges asked
  for, OCCT#1620), 0032 (collections left valid when an allocation throws,
  OCCT#1446), 0033 (the checker's parallel edge checks, OCCT#1640), 0950
  (coincident contacts of two roundings, OCCT#1449), 1284 (the solid
  builder's edge map, OCCT#1620), 1522 (SplitSurf's split test, OCCT#1620)
  and 1690 (BRepBlend_Line's indexed points, OCCT#1634). Several of them
  differ from ours in a tolerance (noted in the rows).
- **Partly:** 0090 (the interference's kind, not its lookup) and 1283 (the
  classifier is lazy, but points out of range are still classified).
- **Still needed** is mostly the fillet's own cases (ChFi3d, BRepBlend,
  BlendFunc): master has not changed the patched code, or changed code
  near it for another fault. Four of them (0001 to 0003, 0005) are
  vcpkg's own port patches; vcpkg's fifth, 0004, is fixed upstream.
- **Ours only** patches extend earlier patches of ours; those of 0014 and
  0950 (0054, 0149, 0230, 0352, 1110, 1655, ...) need a new look if those
  two are dropped for upstream's fixes.

## The patches

| Patch | Title | Class | Upstream reference or reason |
|---|---|---|---|
| 0001 | cmake keep build use vcpkg explicit | still needed | vcpkg port patch; master still turns BUILD_USE_VCPKG on from the toolchain file |
| 0002 | cmake load exported package dependencies | still needed | vcpkg port patch; master's package config still finds no dependencies |
| 0003 | image remove freeimage msvc autolink | still needed | vcpkg port patch; master still autolinks FreeImage.lib on MSVC |
| 0004 | cmake add additional path extraction for OpenCASCADE | fixed upstream | OCCT#1644 (71658f6c8f): the package config takes its prefix from @PACKAGE_INIT@ |
| 0005 | drop bin letter | still needed | vcpkg port patch; master still appends BIN_LETTER to the output folders |
| 0006 | shapeupgrade unifysamedomain seam search by vertex | still needed | master unchanged at the patched code |
| 0007 | brepcheck analyzer no search in face | still needed | master unchanged at the patched code |
| 0008 | brepcheck wire seam edges once per face | still needed | master unchanged at the patched code |
| 0009 | bopalgo face face adaptors once per face | still needed | master unchanged at the patched code |
| 0010 | brepblend surfrst line on both restrictions | still needed | master only refactored near the patched code |
| 0011 | chfi3d fillet edge overflow | still needed | master only refactored near the patched code |
| 0012 | chfi3d closed stripe end on vertex | still needed | master unchanged at the patched code |
| 0013 | chfi3d overflow across faces and overlapping roundings | still needed | master's collapsed-section handling near it is another fault |
| 0014 | chfi3d contact along face edge | fixed upstream | OCCT#1293 (cbca11e975) and #1449 (f713b1305f): a tangency line on the face's boundary keeps its segments, the restriction edge is reused and the consumed face dropped; its follow-ups (0017, 0054, 0140, ...) remain ours |
| 0015 | brepblend surfrst smaller step on failed reframing | still needed | master only refactored near the patched code |
| 0016 | chfi3d rounding rolls on anothers contact line | still needed | only the corner's touching-corks check is in OCCT#1293/#1449 (only transversal crossings fail); the ride on the other's contact line is not |
| 0017 | chfi3d contact along face edge ends at vertices | still needed | master unchanged at the patched code |
| 0018 | chfi3d corner where the end face is split in one plane | still needed | master unchanged at the patched code |
| 0019 | chfi3d undone moresurfdata corner fails | still needed | master only refactored near the patched code |
| 0020 | chfi3d crash fixes | still needed | master unchanged at the patched code |
| 0021 | chfi3d corner of contour closed at sharp vertex | still needed | master reworked the plate orientation near it, another fault |
| 0022 | chfi3d round edges asked for near tangent | fixed upstream | OCCT#1620 (3f3d32d9f3): PerformElement refuses an edge only when it is G1 or tangent within Precision::Angular (isRegularEdge) |
| 0023 | chfi3d meeting point of periodic contact lines | still needed | master unchanged at the patched code |
| 0024 | chfi3d order of periodic kpart surfaces | still needed | master unchanged at the patched code |
| 0025 | chfi3d end cut by faces around a cylinder corner | still needed | master unchanged at the patched code |
| 0026 | chfi3d onsame end cut by several faces | still needed | master only refactored near the patched code |
| 0030 | standard failed allocation throws | still needed | master unchanged at the patched code |
| 0031 | osd threadpool failed allocations to launching thread | still needed | master unchanged at the patched code |
| 0032 | ncollection containers intact when allocation fails | fixed upstream | OCCT#1446 (0f7ab882d5): the reworked Array1, LocalArray and indexed maps allocate before they free or relink |
| 0033 | brepcheck edge curve adaptor per context | fixed upstream | OCCT#1640 (8f817f5da5): BRepCheck_Edge evaluates a ShallowCopy of the edge's adaptor in parallel face checks |
| 0034 | osd signal handler recursive mutex | still needed | master unchanged at the patched code |
| 0050 | chfi3d kpart end runs into end face | still needed | master unchanged at the patched code |
| 0051 | chfi3d rolling on across surfdatas | still needed | master unchanged at the patched code |
| 0052 | chfi3d rolling start beside tangent face | still needed | master only refactored near the patched code |
| 0053 | chfi3d corner after overflow onto face beyond | still needed | master only refactored near the patched code |
| 0054 | chfi3d contact along a part of a face edge | ours only | patches code of 0014 (0014 fixed upstream) |
| 0055 | chfi3d kpart blend cut by face beyond | still needed | master unchanged at the patched code |
| 0056 | chfi3d corner cutting the last two walked pieces | still needed | master only refactored near the patched code |
| 0057 | chfi3d kpart gaps in order across closed lines | ours only | patches code of 0055 |
| 0058 | chfi3d rounding rolls on a convex far edge | ours only | patches code of 0013, 0052 |
| 0059 | chfi3d kpart blend cut by the face at the end | still needed | master's collapsed-section handling near it is another fault |
| 0060 | shapefix intersecting edges once per face | still needed | master unchanged at the patched code |
| 0070 | chfi3d kpart cut to end where the narrow face vanishes | still needed | OCCT#1449's hatching change near it is another fault |
| 0071 | chfi3d kpart cut by several faces beyond | still needed | master unchanged at the patched code |
| 0072 | chfi3d kpart cuts on closed spines | ours only | patches code of 0055, 0057 |
| 0073 | chfi3d kpart pieces over a hole are one face | still needed | OCCT#1449's vertex kind near it (see 0090) is another fault |
| 0074 | chfi3d walked blend cut by the face at the end | still needed | master unchanged at the patched code |
| 0075 | blendfunc constrad rolling ball section | still needed | master only refactored near the patched code |
| 0076 | chfi3d extension rolls on the faces own surface | still needed | master unchanged at the patched code |
| 0077 | chfi3d walk past the spine ends before a singular section | still needed | master unchanged at the patched code |
| 0090 | chfi3d double interference at vertex kind | partly | OCCT#1449 (f713b1305f) gives the double interference the vertex kind (V.IsVertex()); its lookup (ChFi3d_Contains) still compares points only |
| 0091 | topopebrepbuild point uses within array | still needed | master unchanged at the patched code |
| 0100 | brepoffset faces vanishing within the offset | still needed | master only refactored near the patched code |
| 0110 | shapeupgrade unifysamedomain chain not concatenated | still needed | master unchanged at the patched code |
| 0120 | brepcheck face wire boxes of conics past their range | still needed | master unchanged at the patched code |
| 0121 | brepcheck shell orientation edges listed once per face | still needed | master only refactored near the patched code |
| 0130 | chfi3d rounding vanishing at a tangent end | still needed | master only refactored near the patched code |
| 0140 | chfi3d contact along a closed face edge | still needed | OCCT#1449's hatching change near it is another fault |
| 0141 | chfi3d onsame end second contact beyond the edge | still needed | master unchanged at the patched code |
| 0142 | chfi3d kpart cut by faces beyond that grow | still needed | master unchanged at the patched code |
| 0143 | chfi3d chamfer corner beyond the common face | still needed | master unchanged at the patched code |
| 0144 | chfi3d closed contact split at the edges vertex | still needed | master only refactored near the patched code |
| 0145 | chfi3d kpart pieces in their faces periods | ours only | patches code of 0072 |
| 0146 | chfi3d kpart pieces one face round a closed spine | still needed | master unchanged at the patched code |
| 0147 | chfi3d onsame end prolonged along a seam | still needed | master only refactored near the patched code |
| 0148 | chfi3d chamfer as wide as a ledge along a line and an arc | ours only | patches code of 0014, 0140 (0014 fixed upstream) |
| 0149 | chfi3d end snapped onto a closed edges vertex on a periodic face | ours only | patches code of 0014 (0014 fixed upstream) |
| 0150 | chfi3d onsame corner beside a split face | still needed | master unchanged at the patched code |
| 0151 | chfi3d corner on an end face split in one plane | still needed | master only refactored near the patched code |
| 0152 | chfi3d closed rolling start on a seam vertex | still needed | master unchanged at the patched code |
| 0153 | chfi3d roundings meeting on one contact line | ours only | patches code of 0016, 0013 |
| 0160 | chfi3d walked blend over a rounded rim | still needed | master unchanged at the patched code |
| 0170 | brepblend surfrst rolling ball and pivot | still needed | master only refactored near the patched code |
| 0171 | chfi3d rolling ball turns about a vertex | still needed | master unchanged at the patched code |
| 0172 | chfi3d rolled blend cut by the face at the end | still needed | master unchanged at the patched code |
| 0180 | chfi3d chains stop where the edges turn | still needed | master only refactored near the patched code |
| 0190 | chfi3d consumed face round a closed stripe | ours only | patches code of 0160, 0055 |
| 0200 | chfi3d walked blend about a standing contact | still needed | master changed the sampling loop in StoreData, not the case of a side that is a point |
| 0210 | chfi3d mitred ends at a short sharp edge | still needed | master only refactored near the patched code |
| 0211 | chfi3d end cut by a periodic face across its seam | still needed | master unchanged at the patched code |
| 0220 | shapeupgrade unifysamedomain seams half a period apart | still needed | master unchanged at the patched code |
| 0230 | chfi3d contact in the adjacent face period | ours only | patches code of 0014 (0014 fixed upstream) |
| 0240 | chfi3d extension on the faces own surface away from its seam | ours only | patches code of 0076 |
| 0241 | chfi3d roundings meeting across a notch | still needed | master only refactored near the patched code |
| 0250 | chfi3d walk on along a face of the spine | ours only | patches code of 0160, 0011 |
| 0260 | chfi3d fillet carried across a notch in its rim | still needed | master only refactored near the patched code |
| 0270 | chfi3d roundings meeting on one contact line of a face going on | ours only | patches code of 0153, 0016 |
| 0271 | chfi3d roundings cut by each other where their contacts cross | still needed | master only refactored near the patched code |
| 0280 | chfi3d onsame end where the face at the end turns tangentially | still needed | master unchanged at the patched code |
| 0281 | chfi3d walk round the vertex past a short edge | still needed | master unchanged at the patched code |
| 0282 | chfi3d end cut past the bounds of the faces at the end | still needed | master unchanged at the patched code |
| 0283 | chfi3d standing end piece one face with the one before | still needed | master unchanged at the patched code |
| 0290 | chfi3d walk round the vertex through a face met at a point | still needed | master unchanged at the patched code |
| 0291 | chfi3d end past the spine where a contact ran off its face | still needed | master unchanged at the patched code |
| 0300 | chfi3d prolonged end edge across the opposite faces seam | still needed | master unchanged at the patched code |
| 0320 | chfi3d where a fillet stops | still needed | master only refactored near the patched code |
| 0321 | chfi3d closed rounding consuming its face ends at the vertex | ours only | patches code of 0148 |
| 0322 | chfi3d open stripe end at the vertex of the edge its contact runs along | ours only | patches code of 0321, 0148 |
| 0330 | chfi3d which corner a fillet fails at | still needed | master only refactored near the patched code |
| 0331 | chfi3d rounding rolled on a straight edge exactly | still needed | master only refactored near the patched code |
| 0340 | chfi3d onsame end cut by the face of the edge the ball rolls on | still needed | master only refactored near the patched code |
| 0341 | chfi3d rolling blend walked again across a seam past its ends | still needed | master unchanged at the patched code |
| 0342 | chfi3d rounding rolled on past its edge cut by the faces at the ends | still needed | master only refactored near the patched code |
| 0350 | chfi3d regularity only where the faces are tangent | still needed | master unchanged at the patched code |
| 0351 | chfi3d closed chain consuming a face all round | still needed | OCCT#1449's hatching change near it is another fault |
| 0352 | chfi3d contact on a face beyond at an acute angle | ours only | patches code of 0014 (0014 fixed upstream) |
| 0360 | bopalgo wiresplitter tangent edges measured at one distance | still needed | master unchanged at the patched code |
| 0380 | chfi3d chamfer end where the face at the end is tangent to both faces | still needed | master unchanged at the patched code |
| 0381 | chfi3d chamfer guide deformed by a hundredth of its distance at most | still needed | master's change near it (closed-guide continuity, OCCT#1501) is another fault |
| 0390 | chfi3d rolling ball touches the next edge before the vertex | still needed | master only refactored near the patched code |
| 0391 | brepblend rstrst rolling ball on two edges | still needed | master unchanged at the patched code |
| 0400 | chfi3d rounding vanishing at its end stops at the vertex | still needed | master unchanged at the patched code |
| 0401 | chfi3d end cut across the seam line of a periodic face | still needed | master only refactored near the patched code |
| 0410 | chfi3d rounding ending in the face it runs into | still needed | master only refactored near the patched code |
| 0420 | chfi3d kpart round a closed edge on a face with a stepped seam | still needed | OCCT#1449's hatching change near it is another fault |
| 0430 | chfi3d rounding rolls before another roundings face | still needed | master only refactored near the patched code |
| 0431 | chfi3d rolled onsame end at the end of its contact curve | still needed | master unchanged at the patched code |
| 0432 | chfi3d roundings crease over a narrow common face | still needed | master only refactored near the patched code |
| 0440 | chfi3d contact along a chain of edges of a consumed strip | ours only | patches code of 0148, 0014 (0014 fixed upstream) |
| 0450 | chfi3d rounding face turning tangentially into the end face | still needed | master unchanged at the patched code |
| 0460 | chfi3d onsame end on an edge of a consumed corner face | still needed | master only refactored near the patched code |
| 0480 | geomint open line parts beyond its vertices | still needed | master unchanged at the patched code |
| 0481 | chfi3d kpart gap across a closed spines start split there | ours only | patches code of 0072, 0145 |
| 0490 | chfi3d onsame end on a trimmed face past its bounds | still needed | master only refactored near the patched code |
| 0500 | chfikpart fillet between coaxial surfaces of revolution | still needed | master only refactored near the patched code |
| 0501 | chfi3d kpart cut round a cut out under an overhanging rim | still needed | master unchanged at the patched code |
| 0502 | chfi3d kpart cut by the end face over a gap | ours only | patches code of 0059, 0160 |
| 0503 | chfi3d two roundings meeting on an edge within its tolerance | still needed | master reworked the plate orientation near it, another fault |
| 0510 | boptools pcurve into the period inside a face spanning many | still needed | master unchanged at the patched code |
| 0511 | inttools fclass2d every copy on a face spanning many periods | still needed | master unchanged at the patched code |
| 0512 | intcurvesurface quadric roots sampled by knot spans | still needed | master unchanged at the patched code |
| 0530 | chfi3d chamfer guide keeps to its edges at the blends beside | still needed | master's change near it (closed-guide continuity, OCCT#1501) is another fault |
| 0531 | chfi3d chamfer wider than a narrow face cut by the faces beyond | still needed | master only refactored near the patched code |
| 0540 | chfi3d rounding running on into an earlier roundings tube | still needed | master only refactored near the patched code |
| 0550 | chfi3d end over pieces where the end faces edge is in two | still needed | master only refactored near the patched code |
| 0560 | boptools pcurve checked at its own copy in the face | still needed | master unchanged at the patched code |
| 0580 | chfi3d kpart contact along a concave edge off its face | still needed | OCCT#1449 changes the same hatching the other way (a line only on the boundary keeps its segments) |
| 0581 | chfi3d roundings joined across a rib with square sides | ours only | patches code of 0241 |
| 0590 | shapeupgrade unifysamedomain wire repeating itself given up | still needed | master unchanged at the patched code |
| 0591 | adaptor3d bspline samples within the adaptors range | still needed | master unchanged at the patched code |
| 0592 | shapeupgrade unifysamedomain planarity once per surface | still needed | master unchanged at the patched code |
| 0593 | extrema genextps grid of very many knot spans sampled evenly | still needed | master unchanged at the patched code |
| 0600 | chfi3d onsame end cut along its side by the face at the end | ours only | patches code of 0531, 0341 |
| 0620 | chfi3d contact off its face before the end rolls on the edge | still needed | master unchanged at the patched code |
| 0650 | chfi3d kpart cut by a long chain of faces beyond | still needed | master unchanged at the patched code |
| 0651 | chfi3d walk from a seam and extension ending where it fails | still needed | master only refactored near the patched code |
| 0670 | chfi3d standing ball sphere in a gap between pieces | still needed | master unchanged at the patched code |
| 0671 | chfi3d kpart chamfer cut across its width by faces beyond at both ends | still needed | master unchanged at the patched code |
| 0680 | shapeupgrade unifysamedomain faces round a period without winding | still needed | master unchanged at the patched code |
| 0690 | chfi3d end on a face along the line of its seam | still needed | OCCT#1293/#1449's corner check near it (only transversal crossings fail) is another fault |
| 0691 | chfi3d onsame end on a tangent face the crossing nearest the corner | still needed | master only refactored near the patched code |
| 0692 | chfi3d kpart contact end within a vertex tolerance at the vertex | still needed | master unchanged at the patched code |
| 0700 | chfi3d blends cut by a narrow face they roll over | still needed | master only refactored near the patched code |
| 0720 | chfi3d face at the end passed wholly beyond is consumed | still needed | master unchanged at the patched code |
| 0721 | chfi3d onsame prolonged edge own curve on both surfaces | still needed | master unchanged at the patched code |
| 0740 | chfi3d end face split round the vertex pieces of one surface | still needed | master unchanged at the patched code |
| 0741 | chfi3d kpart contact end at a vertex on the edge to the spines end | still needed | master unchanged at the patched code |
| 0742 | chfi3d guide past an open end along the end edges own curve | still needed | master only refactored near the patched code |
| 0750 | chfi3d thread crest rounded on both edges cut by the ends | still needed | master's change near it (closed-guide continuity, OCCT#1501) is another fault |
| 0770 | chfi3d rolling on an edge into the end vertex where it vanishes | still needed | master unchanged at the patched code |
| 0771 | chfi3d contour stops where the rounding vanishes | still needed | master only refactored near the patched code |
| 0772 | chfi3d stripes joined across a corner of the solid | still needed | master only refactored near the patched code |
| 0790 | chfi3d tangent end face cuts the blend back along the stripe | still needed | master only refactored near the patched code |
| 0800 | chfi3d onsame cap points in one period of a stored cylinder | still needed | master unchanged at the patched code |
| 0801 | chfi3d tangent faces within ten times the edge tolerance | still needed | master keeps 1.5 x the edge tolerance for the loose tangency test (ConstructionContinuityTolerance) |
| 0802 | chfi3d contact on its own edge near a vertex passes it | still needed | master only refactored near the patched code |
| 0803 | chfi3d onsame cap leaving the prolongation tangentially | still needed | master only refactored near the patched code |
| 0820 | chfi3d stripes joined across a corner one face each | still needed | master unchanged at the patched code |
| 0821 | chfi3d lens rounded on both edges s shaped section | still needed | master only refactored near the patched code |
| 0822 | chfi3d rounding rolling on both edges into a lens end | still needed | master unchanged at the patched code |
| 0840 | chfi3d contour never takes its own edge again | still needed | master unchanged at the patched code |
| 0860 | chfi3d contour runs on into a near tangent edge asked for | fixed upstream | OCCT#1620 (3f3d32d9f3): FaceTangency stops only at an edge G1 or tangent within Precision::Angular, so a contour goes on into a near-tangent edge |
| 0861 | chfi3d contact at the vertex where the blend vanishes cuts the own edge | still needed | master only refactored near the patched code |
| 0870 | chfi3d contact off a narrow face across its tangent edge keeps its surface | ours only | patches code of 0700, 0772 |
| 0880 | chfi3d start across a strip the ball cannot touch | ours only | patches code of 0013, 0160 |
| 0890 | chfi3d onsame end across a sliver at the vertex | still needed | master only refactored near the patched code |
| 0891 | chfi3d bridge over a gap round a closed spine and across its start | ours only | patches code of 0055 |
| 0900 | chfi3d walked onsame piece generated by the spine edges | ours only | patches code of 0600 |
| 0901 | chfi3d cap intersection ends where the walk first passes the end | still needed | master unchanged at the patched code |
| 0902 | chfi3d rounding vanishing where its contact crosses onto a tangent face | still needed | master only refactored near the patched code |
| 0903 | chfi3d rolling contact crossing onto the next spine face goes on | ours only | patches code of 0011, 0822 |
| 0904 | chfi3d rounding rolled on past one end along a line or circle | still needed | master unchanged at the patched code |
| 0905 | chfi3d ball turning about a vertex of the edges it rolls on | still needed | master unchanged at the patched code |
| 0906 | chfi3d next overflow start where the first walk fails | still needed | master unchanged at the patched code |
| 0907 | brepblend pivot released where a step lands on the tangency | still needed | master unchanged at the patched code |
| 0908 | chfi3d overflow start sampled finer where none is found | ours only | patches code of 0011 |
| 0909 | chfi3d rounding vanishing a hair short of the vertex it rolls into | still needed | master unchanged at the patched code |
| 0910 | chfi3d onsame cap from the contacts point on the prolonged edge | still needed | master unchanged at the patched code |
| 0911 | chfi3d onsame end where the face at the end goes on tangentially | still needed | master only refactored near the patched code |
| 0920 | chfi3d radius law of the callers along a contour | still needed | master unchanged at the patched code |
| 0930 | chfi3d rounding riding anothers contact line against it | still needed | master only refactored near the patched code |
| 0931 | chfi3d seam run on to a corners cut past its end | still needed | master only refactored near the patched code |
| 0932 | chfi3d walk cut before a singular section ends there | still needed | master unchanged at the patched code |
| 0933 | chfi3d ends joined over a narrow face that pokes through the blend | ours only | patches code of 0700 |
| 0940 | chfi3d rounding over a narrow face cut by the face beyond its concave edge | ours only | patches code of 0531, 0390 |
| 0941 | chfi3d kpart chain crossings in the period after the gaps start | ours only | patches code of 0071 |
| 0942 | shapefix intersecting wires cut reported only where cut | still needed | master unchanged at the patched code |
| 0950 | chfi3d roundings consuming the face between them meet on one contact edge | fixed upstream | OCCT#1449 (f713b1305f): StripeEdgeInter merges completely coincident contacts and their end points in the DS instead of failing (coincidence within tol2d, ours within 1 % of the ranges) |
| 0951 | chfi3d rounding ending at a step back of its wall closes with a torus | still needed | master only refactored near the patched code |
| 0960 | projlib projection boundary bisection stops at adjacent parameters | still needed | master only refactored near the patched code |
| 0961 | chfi3d rounding vanishing where its walk stopped on its own edge | ours only | patches code of 0770, 0130 |
| 0962 | chfi3d step end torus cut on its direct frame and only at a recess | still needed | master unchanged at the patched code |
| 0970 | chfi3d chain past a wall corner tighter than the ball vertex blend and torus | still needed | master only refactored near the patched code |
| 0980 | chfi3d rounding across slight creases of its faces at one section | still needed | OCCT#1449's vertex kind near it (see 0090) is another fault |
| 0990 | chfi3d rolled end cut by the faces at the end past the balls tangency | still needed | master unchanged at the patched code |
| 1000 | chfi3d chamfer across a slight concave crease over a narrow face grown faces continued | still needed | master only refactored near the patched code |
| 1001 | chfi3d guide stretch only where the spine turns at the vertex | ours only | patches code of 0980 |
| 1010 | chfi3d kpart closed rim gap round to its own first edge | ours only | patches code of 0501 |
| 1011 | chfi3d intersection at end across the end faces seam in one period | ours only | patches code of 0026, 0025 |
| 1020 | chfi3d kink into a face going on smoothly into the end face also at a pieces start | still needed | master only refactored near the patched code |
| 1030 | geomfill guide trihedron ac second derivative of the binormal | still needed | master still takes \|d\|^2 from the unit binormal (TN2 = TN.SquareMagnitude()) |
| 1040 | shapeupgrade unifysamedomain signed angle to a reversed axis | still needed | master unchanged at the patched code |
| 1050 | shapefix self intersecting wire edges read once | still needed | master unchanged at the patched code |
| 1060 | chfi3d under anothers ball not past its other face | ours only | patches code of 0700 |
| 1070 | brepmesh delabella exact signs | still needed | master only refactored near the patched code |
| 1080 | brepmesh delabella exact signs in place | still needed | master only refactored near the patched code |
| 1081 | brepmesh face checker wire boxes | still needed | master unchanged at the patched code |
| 1090 | chfi3d sharp edge limited only by a common point on it | still needed | master only refactored near the patched code |
| 1100 | chfi3d two corner common points at a vertex of the pivot | still needed | master unchanged at the patched code |
| 1110 | chfi3d first end snapped onto a closed edges vertex on a periodic face | ours only | patches code of 0014, 0149 (0014 fixed upstream) |
| 1120 | chfi3d guide wholly past an end of the spine is its straight extension | still needed | master's change near it (closed-guide continuity, OCCT#1501) is another fault |
| 1130 | brepoffset offsets tangent past the end keep the branch along the crease | still needed | master only refactored near the patched code |
| 1140 | chfi3d cut to end past the vertex along a tangent edge | ours only | patches code of 0059, 0070 |
| 1160 | brepoffset images meeting along a tangential edge meet where they cross its offset | still needed | master unchanged at the patched code |
| 1170 | chfi3d cut to end past the vertex along faces meeting on | ours only | patches code of 1140, 0059 |
| 1180 | brepoffset extended edge keeps a pcurve on its curve where reprojection fails | still needed | master unchanged at the patched code |
| 1190 | brepoffset offsets tangent inside keep the branch and its continuation | still needed | master only refactored near the patched code |
| 1200 | brepoffset c2 extension across a tangential side | still needed | master unchanged at the patched code |
| 1210 | brepfilletapi rounding over slot mouths touching the top along a crest | still needed | master unchanged at the patched code |
| 1220 | chfi3d ball pivots about a crease at the end | still needed | master unchanged at the patched code |
| 1230 | chfi3d rolls before a blend curving away as fast as the ball | still needed | OCCT#1449's vertex kind near it (see 0090) is another fault |
| 1240 | chfi3d short end walked on until both contacts leave | still needed | master unchanged at the patched code |
| 1270 | shapefix shell closed shell found as faces are added | still needed | master unchanged at the patched code |
| 1280 | bopalgo stick vertices of face pairs by an index | still needed | master changed the section vertices near it, not the scan of the interferences |
| 1281 | brepclass3d edge faces mapped only where the line meets an edge | still needed | master unchanged at the patched code |
| 1282 | brepclass3d face intersectors made beside each other | still needed | master unchanged at the patched code |
| 1283 | intcurvesface only points in range classified | partly | OCCT#1045 (ecfb7ada2c) builds the face's topology tool lazily; master still classifies the points outside [parinf, parsup] |
| 1284 | bopalgo buildersolid solid edges mapped once for its holes | fixed upstream | OCCT#1620 (3f3d32d9f3): BOPAlgo_BuilderSolid maps a solid's edges once for all its holes (BOPAlgo_SolidEdgesMap) |
| 1285 | brepclass3d point classified along fixed lines once the segments run out | still needed | master unchanged at the patched code |
| 1290 | chfi3d cut to end past the vertex not on a face closed round its axis | ours only | patches code of 1140, 1170 |
| 1300 | chfi3d concave crease beyond judged at the edge | ours only | patches code of 0980 |
| 1301 | chfi3d walk ends where a contact reaches the next analytic piece | still needed | master unchanged at the patched code |
| 1302 | chfi3d narrow parts pieces generated by their spine edges | ours only | patches code of 0531 |
| 1310 | chfi3d kpart blend runs past the end where the end face goes on tangentially | ours only | patches code of 0059 |
| 1311 | chfi3d onsame end walked on past the end of an extruded stretch | ours only | patches code of 0600, 0055 |
| 1320 | blend walking vanishing end keeps the walk parameter | still needed | master unchanged at the patched code |
| 1330 | chfi3d onsame prolongation along a seam not split at its start | still needed | master unchanged at the patched code |
| 1331 | chfi3d onsame opposite contact ending on the prolonged edge cuts it | still needed | master unchanged at the patched code |
| 1332 | chfi3d periodic contact end kept at its end of a whole period | still needed | master unchanged at the patched code |
| 1340 | chfi3d rounding rides on a part of another contact | still needed | master only refactored near the patched code |
| 1350 | chfi3d rounding over a slight concave crease cut by the face beyond | still needed | master unchanged at the patched code |
| 1351 | chfi3d splitkpart last end keeps its pieces as the first | still needed | master unchanged at the patched code |
| 1352 | chfi3d kpart past end piece one face with the end piece | ours only | patches code of 0059, 1170 |
| 1370 | chfi3d roll over a sliver past the seam of the other contacts face | still needed | master only refactored near the patched code |
| 1380 | chfi3d concave chain through a corner tighter than the ball closed by the corner s own ball | still needed | master only refactored near the patched code |
| 1381 | chfi3d wide surface of a trimmed surface is its basis | still needed | master unchanged at the patched code |
| 1390 | blendfunc constrad centre picks the ball where the mid point lies on the guides axis | still needed | master unchanged at the patched code |
| 1400 | chfi3d convex and concave roundings crossing before a smooth common end overlap | ours only | patches code of 0271, 0013 |
| 1401 | chfi3d vanishing end on a periodic faces seam kept in the contacts period | still needed | master only refactored near the patched code |
| 1402 | chfi3d end walked into a vanishing end taken for it at any radius | ours only | patches code of 0771, 0400 |
| 1410 | geomfill constrainedfilling field not judged where it vanishes | still needed | master unchanged at the patched code |
| 1420 | chfi3d strip edge rolled on past its end cut by a transition piece | ours only | patches code of 0904, 0011 |
| 1430 | chfi3d ball rolled on a convex edge into the end face cut from its turn | still needed | master unchanged at the patched code |
| 1440 | chfi3d walk guided along the balls centres where the spine folds | still needed | master only refactored near the patched code |
| 1460 | chfi3d end crossing of a periodic edge taken in the edges period | still needed | master unchanged at the patched code |
| 1470 | chfi3d periodic contact endpoints snapped before splitting | still needed | master unchanged at the patched code |
| 1480 | chfi3d and brepoffset use current occt apis | ours only | patches code of 0904, 0342 |
| 1490 | chfi3d walk back to the spines first end stepped by its own stretch | still needed | master unchanged at the patched code |
| 1500 | chfi3d convex chain into a mitre of tighter roundings closed by their balls torus | ours only | patches code of 0970, 1380 |
| 1520 | blendfunc constrad ball staying put between tubes walked in the guides normal plane | still needed | master only refactored near the patched code |
| 1521 | chfi3d ball staying put at a plane walked its point contact along no edge | still needed | master unchanged at the patched code |
| 1522 | chfi3d walked blend split only where its contacts meet | fixed upstream | OCCT#1620 (3f3d32d9f3): SplitSurf splits only where the contacts' distance is under tolapp3d (ours: 100 x tolapp3d) |
| 1523 | blendfunc constrad ball staying put between tubes within a tolerance | still needed | master unchanged at the patched code |
| 1530 | chfi3d walk onto a tight tangent face at the end rolls on cut to a point | still needed | master only refactored near the patched code |
| 1540 | chfi3d tight tangent end walked and cut pieces one face | ours only | patches code of 0074 |
| 1560 | chfi3d rim contact split where it runs along a foot roundings edge | still needed | master only refactored near the patched code |
| 1570 | chfi3d point end on an edge at the vertex runs the seam on | still needed | master only refactored near the patched code |
| 1571 | chfi3d kpart cut end on a periodic face in the period of its other end | ours only | patches code of 0059 |
| 1572 | chfi3d kpart cut piece on the end edge one face with its rest | ours only | patches code of 1352 |
| 1580 | chfi3d blend run on past the end over both faces cut by the faces round the end | still needed | master unchanged at the patched code |
| 1581 | chfi3d end cut on a face the way round inside it | still needed | master unchanged at the patched code |
| 1590 | chfi3d tight corners at a cylinders rim and at an obtuse wall closed by the rims ball | still needed | master only refactored near the patched code |
| 1600 | chfi3d walk ends at the next analytic piece only where both contacts reach it | ours only | patches code of 1301 |
| 1601 | chfi3d two stripes meet at the section of an earlier roundings ball that stays put | still needed | master only refactored near the patched code |
| 1610 | chfi3d standing ball section where the normal planes pass forwards | ours only | patches code of 0670 |
| 1611 | chfi3d two corner cut on a periodic pcurve not cut to its period | still needed | master unchanged at the patched code |
| 1620 | chfi3d ball turned about a crease at its end vertex into the end face | ours only | patches code of 1220 |
| 1640 | chfi3d ride on a contact line the other way its side on the face and its corners | still needed | master only refactored near the patched code |
| 1641 | chfi3d rider ending where the line ends on the arc from its vertex | ours only | patches code of 1340, 0016 |
| 1642 | chfi3d face consumed between rides with corners at both ends | ours only | patches code of 1340 |
| 1650 | chfi3d rounding of an edge another rolls on all along dropped | ours only | patches code of 0700, 0013 |
| 1651 | chfi3d corner of a rolling blend and a blend on the face it consumes | still needed | OCCT#1293/#1449's corner check near it (only transversal crossings fail) is another fault |
| 1652 | chfi3d contact cut to one point by the corners at both ends | ours only | patches code of 1651 |
| 1653 | chfi3d three blends cut by each other where one rolls on the consumed faces edge | still needed | master only refactored near the patched code |
| 1654 | chfi3d plate of a corner with a rider oriented from a stripe riding on no line | still needed | master reworked the plate's orientation (OCCT#1640), but the fault comes from our riders (0016, 1640) |
| 1655 | chfi3d roundings consuming a face whose edges are all rounded meet on one contact edge | ours only | patches code of 0950 (0950 fixed upstream) |
| 1656 | chfi3d curve left by a stripe cut to nothing in the period of the corner surface | ours only | patches code of 1640 |
| 1657 | chfi3d corner of three curves marched the way keeping inside the contacts | ours only | patches code of 1653 |
| 1658 | chfi3d the rounding whose contact lies nearer its edge rides a ride of no width drops the rider | ours only | patches code of 0016, 1640 |
| 1659 | chfi3d corner of three cut by each other where a stripe is cut at a ride line at both ends | ours only | patches code of 1653, 1656 |
| 1661 | brepfill trim shell corner each seam ends where it crosses the other side | still needed | master only refactored near the patched code |
| 1662 | brepoffsetapi pipe shell mitred corners from a ten thousandth radian | still needed | master unchanged at the patched code |
| 1663 | brepfill trim shell corner mitre along the bisector plane where the sides intersection breaks up | still needed | master only refactored near the patched code |
| 1664 | brepfill sweep sides extended past a corner as the prisms of their end sections | still needed | master unchanged at the patched code |
| 1665 | bopds paves of an edge whose blocks do not chain | still needed | master unchanged at the patched code |
| 1666 | brepfill trim shell corner round along the bisector plane where the sides intersection breaks up | still needed | master only refactored near the patched code |
| 1670 | chfi3d end crossing on the next edge of the faces common boundary | still needed | master unchanged at the patched code |
| 1680 | chfi3d corner join face classifier set up once per face | ours only | patches code of 0772, 0820 |
| 1690 | brepblend line points read by index in constant time | fixed upstream | OCCT#1634 (ec0167b409): BRepBlend_Line stores its points in an NCollection_LinearVector |
| 1691 | intcurvesurface surface polyhedra made once per face | still needed | master unchanged at the patched code |
| 1695 | inttools wline decomposition reads no point past the line ends | still needed | master unchanged at the patched code |
