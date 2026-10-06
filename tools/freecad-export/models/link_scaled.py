# SPDX-License-Identifier: MIT
# A scaled link: a cylinder linked at twice its size (ScaleVector), which
# Mitcad cannot place as an occurrence (placements are rigid), next to a
# plain link of the same cylinder.

rod = named(doc.addObject("Part::Cylinder", "Rod"), "Rod")
rod.Radius, rod.Height = 2, 10
hide(rod)
link("RodPlain", rod, place((10, 0, 0)))
big = link("RodBig", rod, place((30, 0, 0)))
big.ScaleVector = V(2, 2, 2)
