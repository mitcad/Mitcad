# SPDX-License-Identifier: MIT
# An Assembly (FreeCAD 1.0 and later): two Bodies of this document inserted
# as links, one grounded by a grounded joint, the other placed by its link;
# no solved joints. Mitcad takes the links' placements and the grounding.

if not at_least(1, 0):
    raise Unsupported("the Assembly workbench came with FreeCAD 1.0")

import JointObject

base = block_body("BasePlate", 40, 30, 4)
post = block_body("Post", 6, 6, 30)
hide(base)
hide(post)

assembly = doc.addObject("Assembly::AssemblyObject", "Assembly")
assembly.Label = "Assembly"
joints = assembly.newObject("Assembly::JointGroup", "Joints")

base_link = assembly.newObject("App::Link", "BasePlateLink")
base_link.Label = "BasePlateLink"
base_link.LinkedObject = base
post_link = assembly.newObject("App::Link", "PostLink")
post_link.Label = "PostLink"
post_link.LinkedObject = post
post_link.Placement = place((17, 12, 4), (0, 0, 1), 20)

ground = joints.newObject("App::FeaturePython", "GroundedJoint")
JointObject.GroundedJoint(ground, base_link)
