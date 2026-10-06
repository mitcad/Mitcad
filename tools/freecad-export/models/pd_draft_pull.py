# SPDX-License-Identifier: MIT
# Drafts with a pull direction along the neutral plane's normal, one per
# Body:
#
# - PullEdge: the side faces of a 30 x 20 x 10 block drafted 5 degrees
#   about its bottom face, pulled along a vertical edge of the block.
# - PullReversed: the same, 4 degrees, pulled along another vertical
#   edge, reversed.
#
# (FreeCAD 0.21 and 1.0 fail to draft with an origin axis as the pull
# direction.)
#
# Mitcad expects: no fallback


def drafted(container, name, x, angle, corner, reversed_=False):
    base = block(container, name + "Base", 30, 20, 10, x, 0)
    doc.recompute()
    faces = []
    for i, face in enumerate(base.Shape.Faces):
        if abs(face.normalAt(0, 0).z) < 1e-9:
            faces.append("Face%d" % (i + 1))
    feature = container.newObject("PartDesign::Draft", name)
    feature.Base = (base, faces)
    feature.NeutralPlane = (base, [face_index(base.Shape, (0, 0, -1), 0)])
    a = (x + corner[0], corner[1], 0)
    feature.PullDirection = (base, [edge_index(base.Shape, a, (a[0], a[1], 10))])
    feature.Angle = angle
    feature.Reversed = reversed_
    return feature


drafted(body("PullEdge"), "PullEdgeDraft", 0, 5, (30, 20))
drafted(body("PullReversed"), "PullReversedDraft", 40, 4, (0, 0), reversed_=True)
