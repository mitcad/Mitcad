# SPDX-License-Identifier: MIT
# One PartDesign Body: a 40 x 30 rectangle padded 10 mm, a circle of radius
# 6 pocketed through all, and a smaller pad on top. The Body's shape (its
# Tip) is the one body.

part_body = body("Plate")
base = rectangle_sketch(part_body, 40, 30, "PlateSketch")
pad(part_body, base, 10, "PlatePad")
hole = circle_sketch(part_body, 6, "HoleSketch", 20, 15)
pocket(part_body, hole, "HolePocket", through_all=True)
boss = circle_sketch(part_body, 4, "BossSketch", 8, 8, offset=place((0, 0, 10)))
pad(part_body, boss, 5, "BossPad")
color(part_body, (0.8, 0.6, 0.2))
