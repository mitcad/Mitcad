#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""OpenUSD's own reading of an animated USDZ file (mitcad#1597).

Usage: check-animation.py <file.usdz> <expected.json>

Needs OpenUSD's Python package (`pip install usd-core`, in a virtual
environment of an isolated build environment; docs/development.md, "USDZ
files"). It opens the package with OpenUSD and checks, against the
expectations:

- the stage's time: `fps` (timeCodesPerSecond and framesPerSecond),
  `start` and `end` (the authored time code range);
- for each prim listed under `prims`: its `xformOp:transform` has one time
  sample per entry of `times`, at those time codes, and its local
  transformation at the first, middle and last samples is the expected
  pose there (`poses`: MitCad's layout, rows of the rotation with the
  translation last, p' = M p), within 1e-9;
- OpenUSD's validators (usdchecker's: the UsdValidation framework, or
  UsdUtils.ComplianceChecker in versions before it) find no error.

Prints OK, or each difference, and exits with 1 on any.

expected.json: {"fps": 24, "start": 0, "end": 2,
                "prims": {"/root/Arm_1": {"times": [0, 1, 2],
                                          "poses": [[[1, 0, 0, 0], ...], ...]}}}
"""

import json
import sys

from pxr import Usd, UsdGeom

TOLERANCE = 1e-9


def usd_matrix(rows):
    """USD's row-vector matrix (p' = p M) of a pose in MitCad's layout."""
    full = [list(map(float, row)) for row in rows]
    if len(full) == 3:
        full.append([0.0, 0.0, 0.0, 1.0])
    return [[full[c][r] for c in range(4)] for r in range(4)]


def local_matrix(xformable, time):
    value = xformable.GetLocalTransformation(Usd.TimeCode(time))
    # Older bindings return (matrix, resetsXformStack).
    if isinstance(value, tuple):
        value = value[0]
    return [[value[r][c] for c in range(4)] for r in range(4)]


def close(a, b):
    return abs(a - b) <= TOLERANCE * (1.0 + max(abs(a), abs(b)))


def check(path, expected):
    errors = []
    stage = Usd.Stage.Open(path)
    if stage is None:
        return [f"OpenUSD cannot open {path}"]
    if not stage.HasAuthoredTimeCodeRange():
        errors.append("the stage has no authored time code range")
    for name, value in [
        ("timeCodesPerSecond", stage.GetTimeCodesPerSecond()),
        ("framesPerSecond", stage.GetFramesPerSecond()),
    ]:
        if not close(value, expected["fps"]):
            errors.append(f"{name} {value}, expected {expected['fps']}")
    for name, value in [
        ("startTimeCode", stage.GetStartTimeCode()),
        ("endTimeCode", stage.GetEndTimeCode()),
    ]:
        want = expected["start" if name.startswith("start") else "end"]
        if not close(value, want):
            errors.append(f"{name} {value}, expected {want}")
    for prim_path, want in expected["prims"].items():
        prim = stage.GetPrimAtPath(prim_path)
        if not prim or not prim.IsValid():
            errors.append(f"{prim_path}: no such prim")
            continue
        attribute = prim.GetAttribute("xformOp:transform")
        if not attribute:
            errors.append(f"{prim_path}: no xformOp:transform")
            continue
        times = list(attribute.GetTimeSamples())
        if len(times) != len(want["times"]) or not all(
            close(a, b) for a, b in zip(times, want["times"])
        ):
            errors.append(f"{prim_path}: time samples at {times}, expected {want['times']}")
            continue
        xformable = UsdGeom.Xformable(prim)
        order = [op.GetOpName() for op in xformable.GetOrderedXformOps()]
        if order != ["xformOp:transform"]:
            errors.append(f"{prim_path}: xformOpOrder {order}")
        count = len(times)
        for label, index in [("first", 0), ("middle", count // 2), ("last", count - 1)]:
            got = local_matrix(xformable, times[index])
            pose = usd_matrix(want["poses"][index])
            if not all(close(got[r][c], pose[r][c]) for r in range(4) for c in range(4)):
                errors.append(
                    f"{prim_path}: the {label} sample (time code {times[index]}) is {got}, "
                    f"expected {pose}"
                )
    errors.extend(validate(stage, path))
    return errors


def validate(stage, path):
    """OpenUSD's validators on the package: the validation framework
    (UsdValidation, which usdchecker uses since it replaced
    UsdUtils.ComplianceChecker), else the older compliance checker."""
    try:
        from pxr import UsdValidation
    except ImportError:
        UsdValidation = None
    if UsdValidation is not None:
        registry = UsdValidation.ValidationRegistry()
        validators = registry.GetOrLoadAllValidators()
        found = UsdValidation.ValidationContext(validators).Validate(stage)
        print(f"OpenUSD's validators: {len(validators)} run")
        return [
            f"validation: {e.GetIdentifier()}: {e.GetMessage()}"
            for e in found
            if e.GetType() == UsdValidation.ValidationErrorType.Error
        ]
    from pxr import UsdUtils

    checker = UsdUtils.ComplianceChecker()
    checker.CheckCompliance(path)
    return [f"compliance: {e}" for e in checker.GetErrors()] + [
        f"compliance: {e}" for e in checker.GetFailedChecks()
    ]


def main():
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    with open(sys.argv[2], encoding="utf-8") as f:
        expected = json.load(f)
    errors = check(sys.argv[1], expected)
    for error in errors:
        print(error)
    if errors:
        return 1
    print("OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
