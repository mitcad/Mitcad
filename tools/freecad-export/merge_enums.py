# SPDX-License-Identifier: MIT
#
# Merges the enumeration tables enums.py wrote for each FreeCAD version into
# the data file of Mitcad's reader (core/freecad/data/enums.json): per
# major.minor version, per object type, the value lists of its enumeration
# properties, and the document's own.
#
# Usage (any Python 3, e.g. FreeCAD's bundled one in the isolated distro):
#   python3 merge_enums.py <out.json> <enums-0.21.2.json> <enums-1.0.2.json> ...

import json
import sys


def main(out, inputs):
    versions = {}
    for path in inputs:
        with open(path) as f:
            data = json.load(f)
        release = data["freecad"]
        minor = ".".join(release.split(".")[:2])
        types = {}
        for type_name, enums in sorted(data["types"].items()):
            clean = {p: v for p, v in sorted(enums.items()) if isinstance(v, list)}
            if clean:
                types[type_name] = clean
        versions[minor] = {
            "freecad": release,
            "document": {p: v for p, v in sorted(data.get("document", {}).items())},
            "types": types,
        }
    # One object type per line: readable diffs between runs.
    def compact(value):
        return json.dumps(value, ensure_ascii=False, separators=(", ", ": "))

    lines = [
        "{",
        ' "format": "mitcad-freecad-enums",',
        ' "version": 1,',
        ' "source": "tools/freecad-export/enums.py, run with each FreeCAD release",',
        ' "versions": {',
    ]
    for i, (minor, table) in enumerate(sorted(versions.items())):
        lines.append("  %s: {" % compact(minor))
        lines.append('   "freecad": %s,' % compact(table["freecad"]))
        lines.append('   "document": %s,' % compact(table["document"]))
        lines.append('   "types": {')
        types = list(table["types"].items())
        for j, (type_name, enums) in enumerate(types):
            comma = "," if j + 1 < len(types) else ""
            lines.append("    %s: %s%s" % (compact(type_name), compact(enums), comma))
        lines.append("   }")
        lines.append("  }" + ("," if i + 1 < len(versions) else ""))
    lines += [" }", "}"]
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    json.load(open(out, encoding="utf-8"))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2:])
