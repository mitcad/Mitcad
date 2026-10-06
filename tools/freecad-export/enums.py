# SPDX-License-Identifier: MIT
#
# The enumerations of FreeCAD's object types, read from FreeCAD itself: an
# object of each type is made in a scratch document (PartDesign features in
# a Body) and the value lists of its enumeration properties are written to
# JSON, with its property names and types. Documents store enumerations as
# indices into these lists, which differ between versions (merge_enums.py
# makes core/freecad/data/enums.json of them). Lists that depend on another
# property's value (a hole's thread sizes and classes on its thread type)
# are written once per value, as "ThreadSize[ISOMetricProfile]".
#
# Macro (freecadcmd or freecad): ENUMS_OUT=<file.json> writes the tables and
# exits.

import json
import os
import traceback

import FreeCAD as App

TYPES = [
    # Part workbench
    "Part::Feature", "Part::Box", "Part::Cylinder", "Part::Sphere", "Part::Cone",
    "Part::Torus", "Part::Ellipsoid", "Part::Prism", "Part::Wedge", "Part::Helix",
    "Part::Spiral", "Part::Plane", "Part::Circle", "Part::Ellipse", "Part::Line",
    "Part::Point", "Part::RegularPolygon", "Part::Vertex", "Part::Extrusion",
    "Part::Revolution", "Part::Cut", "Part::Fuse", "Part::MultiFuse", "Part::Common",
    "Part::MultiCommon", "Part::Section", "Part::Fillet", "Part::Chamfer",
    "Part::Mirroring", "Part::Loft", "Part::Sweep", "Part::Thickness", "Part::Offset",
    "Part::Offset2D", "Part::Compound", "Part::Refine", "Part::Face", "Part::Scale",
    "Part::DatumPlane", "Part::DatumLine", "Part::DatumPoint", "Part::LocalCoordinateSystem",
    "Part::Part2DObject", "Part::FeatureExt",
    # PartDesign workbench (made in a Body)
    "PartDesign::Body", "PartDesign::Pad", "PartDesign::Pocket", "PartDesign::Revolution",
    "PartDesign::Groove", "PartDesign::Hole", "PartDesign::Fillet", "PartDesign::Chamfer",
    "PartDesign::Draft", "PartDesign::Thickness", "PartDesign::AdditiveLoft",
    "PartDesign::SubtractiveLoft", "PartDesign::AdditivePipe", "PartDesign::SubtractivePipe",
    "PartDesign::AdditiveHelix", "PartDesign::SubtractiveHelix", "PartDesign::AdditiveBox",
    "PartDesign::SubtractiveBox", "PartDesign::AdditiveCylinder",
    "PartDesign::SubtractiveCylinder", "PartDesign::AdditiveSphere",
    "PartDesign::SubtractiveSphere", "PartDesign::AdditiveCone", "PartDesign::SubtractiveCone",
    "PartDesign::AdditiveEllipsoid", "PartDesign::SubtractiveEllipsoid",
    "PartDesign::AdditiveTorus", "PartDesign::SubtractiveTorus", "PartDesign::AdditivePrism",
    "PartDesign::SubtractivePrism", "PartDesign::AdditiveWedge", "PartDesign::SubtractiveWedge",
    "PartDesign::Mirrored", "PartDesign::LinearPattern", "PartDesign::PolarPattern",
    "PartDesign::MultiTransform", "PartDesign::Scaled", "PartDesign::Boolean",
    "PartDesign::Plane", "PartDesign::Line", "PartDesign::Point",
    "PartDesign::CoordinateSystem", "PartDesign::ShapeBinder", "PartDesign::SubShapeBinder",
    "PartDesign::FeatureBase",
    # Sketcher, containers, links, data
    "Sketcher::SketchObject", "App::Part", "App::Link", "App::LinkGroup",
    "App::DocumentObjectGroup", "App::Origin", "App::Line", "App::Plane", "App::Point",
    "App::LocalCoordinateSystem", "App::VarSet", "Spreadsheet::Sheet",
    "Assembly::AssemblyObject", "Assembly::JointGroup",
]

BODY_FEATURES = ("PartDesign::",)


def table(obj):
    enums = {}
    properties = {}
    for prop in obj.PropertiesList:
        kind = obj.getTypeIdOfProperty(prop)
        properties[prop] = kind
        if kind == "App::PropertyEnumeration":
            try:
                enums[prop] = list(obj.getEnumerationsOfProperty(prop) or [])
            except Exception as e:
                enums[prop] = {"error": str(e)}
    return enums, properties


def thread_sizes(hole, types):
    """A hole's thread sizes and classes depend on its thread type: the
    lists of each type, as "ThreadSize[<type>]" and "ThreadClass[<type>]"."""
    out = {}
    for thread in types if isinstance(types, list) else []:
        try:
            hole.ThreadType = thread
        except Exception:
            # The type is set; what it updates may need a profile.
            pass
        for prop in ("ThreadSize", "ThreadClass"):
            try:
                out["%s[%s]" % (prop, thread)] = list(hole.getEnumerationsOfProperty(prop) or [])
            except Exception as e:
                out["%s[%s]" % (prop, thread)] = {"error": str(e)}
    return out


def collect():
    doc = App.newDocument("enums")
    body = doc.addObject("PartDesign::Body", "ScratchBody")
    result = {
        "freecad": ".".join(App.Version()[:3]),
        "program_version": App.Version()[3] if len(App.Version()) > 3 else None,
        "types": {},
        "properties": {},
        "missing": {},
    }
    for type_name in TYPES:
        try:
            if type_name.startswith(BODY_FEATURES) and type_name != "PartDesign::Body":
                obj = body.newObject(type_name, "Scratch")
            else:
                obj = doc.addObject(type_name, "Scratch")
        except Exception as e:
            result["missing"][type_name] = str(e).strip() or type(e).__name__
            continue
        if obj is None:
            result["missing"][type_name] = "not made"
            continue
        enums, properties = table(obj)
        if type_name == "PartDesign::Hole":
            enums.update(thread_sizes(obj, enums.get("ThreadType")))
        result["types"][type_name] = enums
        result["properties"][type_name] = properties
    # The document's own enumerations.
    enums = {}
    for prop in doc.PropertiesList:
        try:
            if doc.getTypeIdOfProperty(prop) == "App::PropertyEnumeration":
                enums[prop] = list(doc.getEnumerationsOfProperty(prop) or [])
        except Exception:
            pass
    result["document"] = enums
    App.closeDocument(doc.Name)
    return result


# Run as a macro (FreeCAD names it differently with and without the user
# interface, so the environment decides).
if os.environ.get("ENUMS_OUT"):
    try:
        with open(os.environ["ENUMS_OUT"], "w") as f:
            json.dump(collect(), f, indent=1, sort_keys=True)
            f.write("\n")
    except Exception:
        with open(os.environ["ENUMS_OUT"] + ".error", "w") as f:
            f.write(traceback.format_exc())
    finally:
        os._exit(0)
