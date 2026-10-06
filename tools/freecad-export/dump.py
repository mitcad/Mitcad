# SPDX-License-Identifier: MIT
#
# Reference dump of a FreeCAD document (JSON), for Mitcad's .FCStd import
# tests: every object with its type, label, state, visibility, links,
# placements, enumeration values (index, text and the list), and for
# shape-bearing objects the stored shape's measures: volume, area, centre of
# mass (in the object's container and in the world), counts of solids,
# shells, faces, edges and vertices, validity and bounding box. Links get
# the measures of what they show, per array element. Sketches also get
# their geometry, constraints and external geometry as FreeCAD reads them
# (sketch_info). With the user interface, the view data too (visibility,
# colours).
#
# The dump describes the file as saved: the measures are those of the shapes
# stored in it, read from the archive (FreeCAD does not recompute on open,
# but Python objects such as Draft's may rebuild their shapes when they are
# restored; "restored_differs" marks those). Each measured shape also has
# the area and volume of a fine mesh of it, so that shapes FreeCAD itself
# cannot measure consistently can be told apart.
#
# Expressions and parameters: every object's bound expressions with
# FreeCAD's value of each, spreadsheets' cells with their computed values
# and VarSets' properties (a value is the number in FreeCAD's units,
# millimetres and degrees, with the powers of length and angle of its
# unit).
#
# As a module: dump.write(doc, path). As a macro (freecadcmd or freecad):
# FCSTD=<file> JSON=<output> dumps one file and exits.

import json
import os
import shutil
import tempfile
import xml.etree.ElementTree as ElementTree
import zipfile

import FreeCAD as App

FORMAT = "mitcad-freecad-dump"
FORMAT_VERSION = 4


class StoredShapes:
    """The shapes stored in a document's file, by object name, and the
    spreadsheets' cell addresses."""

    def __init__(self, path):
        self.files = {}
        self.cells = {}
        self.archive = None
        self.folder = tempfile.mkdtemp(prefix="fcstd-dump-")
        try:
            self.archive = zipfile.ZipFile(path)
            root = ElementTree.fromstring(self.archive.read("Document.xml"))
        except Exception:
            return
        for data in root.iter("ObjectData"):
            for obj in data.findall("Object"):
                for prop in obj.iter("Property"):
                    if prop.get("type") == "Spreadsheet::PropertySheet":
                        self.cells[obj.get("name")] = [c.get("address") for c in prop.iter("Cell")]
                    if prop.get("type") != "Part::PropertyPartShape" or prop.get("name") != "Shape":
                        continue
                    part = prop.find("Part")
                    if part is not None and part.get("file"):
                        self.files[obj.get("name")] = part.get("file")

    def shape(self, name):
        import Part

        entry = self.files.get(name)
        if entry is None or self.archive is None:
            return None
        data = self.archive.read(entry)
        if not data:
            return None
        path = os.path.join(self.folder, "shape" + os.path.splitext(entry)[1])
        with open(path, "wb") as f:
            f.write(data)
        shape = Part.Shape()
        if entry.endswith(".bin"):
            shape.importBinary(path)
        else:
            shape.importBrep(path)
        return shape

    def close(self):
        shutil.rmtree(self.folder, ignore_errors=True)


def gui_up():
    return bool(getattr(App, "GuiUp", False))


def vector(v):
    return [v.x, v.y, v.z]


def placement(p):
    q = p.Rotation.Q
    return {"position": vector(p.Base), "rotation": [q[0], q[1], q[2], q[3]]}


def edge_moment(edge, count=4000):
    """An edge's length and first moment from a fine polyline of it (OCCT's
    centre of mass of an edge is off by up to a thousandth of its size for
    ellipses)."""
    try:
        points = edge.discretize(Number=count)
    except Exception:
        # Degenerate edges and curves FreeCAD cannot sample.
        try:
            return edge.Length, edge.CenterOfMass * edge.Length
        except Exception:
            return 0.0, App.Vector()
    length = 0.0
    moment = App.Vector()
    for a, b in zip(points, points[1:]):
        d = (b - a).Length
        length += d
        moment += (a + b) * (0.5 * d)
    return length, moment


def center_of(shape):
    """The centre of mass: of the solids by volume (an inside-out solid's
    negative volume counts as positive), else of the faces by area, else of
    the edges by length (as Mitcad measures bodies and sketches)."""
    for parts, measure in ((shape.Solids, "Volume"), (shape.Faces, "Area")):
        total = 0.0
        weighted = App.Vector()
        for item in parts:
            amount = abs(getattr(item, measure))
            weighted += item.CenterOfMass * amount
            total += amount
        if total > 0.0:
            return weighted * (1.0 / total), measure.lower()
    total = 0.0
    weighted = App.Vector()
    for edge in shape.Edges:
        length, moment = edge_moment(edge)
        total += length
        weighted += moment
    if total > 0.0:
        return weighted * (1.0 / total), "length"
    return None, None


def shape_info(shape, to_world=None):
    """Measures of a shape; `to_world` maps its coordinates to the world's."""
    if shape is None or shape.isNull():
        return {"null": True}
    info = {
        "null": False,
        "type": shape.ShapeType,
        "valid": shape.isValid(),
        "volume": shape.Volume,
        "area": shape.Area,
        # The edges' lengths (the shape's own Length integrates ellipses
        # less exactly).
        "length": sum(e.Length for e in shape.Edges),
        "solids": len(shape.Solids),
        "shells": len(shape.Shells),
        "faces": len(shape.Faces),
        "edges": len(shape.Edges),
        "vertexes": len(shape.Vertexes),
    }
    box = shape.BoundBox
    if box.isValid():
        info["bound_box"] = [box.XMin, box.YMin, box.ZMin, box.XMax, box.YMax, box.ZMax]
    center, by = center_of(shape)
    if center is not None:
        info["center"] = vector(center)
        info["center_by"] = by
        if to_world is not None:
            info["world_center"] = vector(to_world.multVec(center))
    # A mesh of the shape within a thousandth of its size.
    if info["faces"] > 0 and box.isValid() and box.DiagonalLength < 1e9:
        try:
            import Mesh

            mesh = Mesh.Mesh(shape.tessellate(max(box.DiagonalLength * 1e-3, 1e-3)))
            info["mesh_area"] = mesh.Area
            if info["solids"] > 0:
                info["mesh_volume"] = mesh.Volume
        except Exception as e:
            info["mesh_error"] = str(e)
    return info


def container_placement(obj):
    """The placement of the object's container in the world: the global
    placement of the geometric group it is in (App::Part, Body, Assembly),
    else the identity. The stored shape of an object is in its container's
    coordinates."""
    try:
        parent = obj.getParentGeoFeatureGroup()
    except Exception:
        parent = None
    if parent is None:
        return App.Placement()
    return parent.getGlobalPlacement()


def enumerations(obj):
    out = {}
    for prop in obj.PropertiesList:
        try:
            if obj.getTypeIdOfProperty(prop) != "App::PropertyEnumeration":
                continue
            values = list(obj.getEnumerationsOfProperty(prop) or [])
            value = getattr(obj, prop)
            out[prop] = {
                "value": value,
                "index": values.index(value) if value in values else None,
                "values": values,
            }
        except Exception as e:
            out[prop] = {"error": str(e)}
    return out


def color(c):
    return [c[0], c[1], c[2]] + ([c[3]] if len(c) > 3 else [])


def view_info(obj):
    view = getattr(obj, "ViewObject", None)
    if view is None:
        return None
    info = {"visible": bool(view.Visibility)}
    for prop in ("ShapeColor", "Transparency", "LineColor"):
        if hasattr(view, prop):
            value = getattr(view, prop)
            info[prop] = color(value) if isinstance(value, tuple) else value
    if hasattr(view, "DiffuseColor"):
        info["DiffuseColor"] = [color(c) for c in view.DiffuseColor]
    if hasattr(view, "ShapeAppearance"):
        info["ShapeAppearance"] = [color(m.DiffuseColor) for m in view.ShapeAppearance]
    return info


def link_info(obj, to_world):
    import Part

    linked = obj.LinkedObject
    info = {
        "linked": linked.Name if linked is not None else None,
        "linked_document": (
            os.path.basename(linked.Document.FileName)
            if linked is not None and linked.Document != obj.Document
            else None
        ),
        "transform": bool(getattr(obj, "LinkTransform", False)),
        "element_count": int(getattr(obj, "ElementCount", 0) or 0),
    }
    if hasattr(obj, "ShowElement"):
        info["show_element"] = bool(obj.ShowElement)
    if getattr(obj, "ElementList", None):
        info["element_list"] = [o.Name for o in obj.ElementList]
    if hasattr(obj, "ScaleVector"):
        info["scale"] = vector(obj.ScaleVector)
    if hasattr(obj, "VisibilityList"):
        info["visibility_list"] = [bool(v) for v in obj.VisibilityList]
    if hasattr(obj, "PlacementList"):
        info["placement_list"] = [placement(p) for p in obj.PlacementList]
    try:
        info["shape"] = shape_info(Part.getShape(obj), to_world)
    except Exception as e:
        info["shape"] = {"error": str(e)}
    elements = []
    for i in range(info["element_count"]):
        try:
            elements.append(shape_info(Part.getShape(obj, "%d." % i), to_world))
        except Exception as e:
            elements.append({"error": str(e)})
    if elements:
        info["elements"] = elements
    return info


def sketch_info(obj):
    """A sketch as FreeCAD reads it: each geometry's type, construction
    flag and points (start, end and centre as getPoint gives them, in the
    sketch's coordinates); each constraint's type by name, its references,
    value (millimetres, radians), name and flags; the external geometry; and
    how far FreeCAD's solver moves the saved geometry (it should not)."""
    geometry = []
    positions = []
    for i, g in enumerate(obj.Geometry):
        entry = {"type": g.TypeId}
        try:
            entry["construction"] = bool(obj.getConstruction(i))
        except Exception:
            pass
        points = {}
        for pos, key in ((1, "start"), (2, "end"), (3, "mid")):
            try:
                p = obj.getPoint(i, pos)
                points[key] = vector(p)
                positions.append((i, pos, p))
            except Exception:
                pass
        entry["points"] = points
        for attr in ("Radius", "MajorRadius", "MinorRadius", "Degree", "NbPoles", "Focal"):
            if hasattr(g, attr):
                try:
                    entry[attr[0].lower() + attr[1:]] = getattr(g, attr)
                except Exception:
                    pass
        if g.TypeId == "Part::GeomBSplineCurve":
            entry["periodic"] = bool(g.isPeriodic())
            entry["poles"] = [vector(p) for p in g.getPoles()]
            entry["knots"] = list(g.getKnots())
            entry["mults"] = list(g.getMultiplicities())
            entry["weights"] = list(g.getWeights())
        if hasattr(g, "length"):
            try:
                entry["length"] = g.length()
            except Exception:
                pass
        geometry.append(entry)
    constraints = []
    for c in obj.Constraints:
        entry = {
            "type": c.Type,
            "name": c.Name,
            "value": c.Value,
            "first": c.First,
            "first_pos": c.FirstPos,
            "second": c.Second,
            "second_pos": c.SecondPos,
            "third": c.Third,
            "third_pos": c.ThirdPos,
            "driving": bool(c.Driving),
        }
        for attr, key in (("IsActive", "active"), ("InVirtualSpace", "virtual_space")):
            if hasattr(c, attr):
                entry[key] = bool(getattr(c, attr))
        if c.Type == "InternalAlignment":
            entry["alignment_index"] = getattr(c, "InternalAlignmentIndex", None)
        constraints.append(entry)
    external = []
    for linked, subs in obj.ExternalGeometry:
        for sub in subs if isinstance(subs, (list, tuple)) else [subs]:
            external.append([linked.Name, sub])
    info = {"geometry": geometry, "constraints": constraints, "external": external}
    try:
        info["solve"] = obj.solve()
        moved = 0.0
        for i, pos, before in positions:
            moved = max(moved, (obj.getPoint(i, pos) - before).Length)
        info["solver_moves"] = moved
    except Exception as e:
        info["solve_error"] = str(e)
    return info


def quantity(value):
    """A computed value: the number in FreeCAD's units (millimetres,
    degrees) and the powers of length and angle of its unit (other: the
    unit has other base units); None for text and other values."""
    if hasattr(value, "Unit") and hasattr(value, "Value"):
        signature = tuple(value.Unit.Signature)
        return {
            "value": value.Value,
            "length": signature[0],
            "angle": signature[7] if len(signature) > 7 else 0,
            "other": any(signature[1:7]),
        }
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return None
    return {"value": float(value), "length": 0, "angle": 0, "other": False}


def expressions_info(obj):
    """The expressions bound to the object's properties, each with its value
    as FreeCAD computes it."""
    out = []
    for path, expression in getattr(obj, "ExpressionEngine", None) or []:
        entry = {"path": path, "expression": expression}
        try:
            entry["value"] = quantity(obj.evalExpression(expression))
        except Exception as e:
            entry["error"] = str(e)
        out.append(entry)
    return out


def cells_info(sheet, addresses):
    """A spreadsheet's cells: content, alias and computed value."""
    out = []
    for address in addresses:
        entry = {"address": address, "content": sheet.getContents(address)}
        try:
            alias = sheet.getAlias(address)
            if alias:
                entry["alias"] = alias
        except Exception:
            pass
        try:
            value = sheet.get(address)
            entry["value"] = quantity(value)
            if isinstance(value, str):
                entry["text"] = value
        except Exception as e:
            entry["error"] = str(e)
        out.append(entry)
    return out


def variables_info(obj):
    """A VarSet's own (dynamic) properties' values."""
    out = {}
    for prop in obj.PropertiesList:
        if prop in ("Label", "Label2", "Visibility", "ExpressionEngine"):
            continue
        try:
            value = quantity(getattr(obj, prop))
        except Exception:
            continue
        if value is not None:
            out[prop] = value
    return out


def object_info(obj, stored):
    info = {
        "name": obj.Name,
        "label": obj.Label,
        "type": obj.TypeId,
        "id": getattr(obj, "ID", None),
        "state": list(obj.State),
        "in_list": sorted(o.Name for o in obj.InList),
        "out_list": sorted(o.Name for o in obj.OutList),
    }
    if hasattr(obj, "Visibility"):
        info["visible"] = bool(obj.Visibility)
    if hasattr(obj, "Group"):
        info["group"] = [o.Name for o in obj.Group]
    to_world = container_placement(obj)
    if hasattr(obj, "Placement"):
        info["placement"] = placement(obj.Placement)
        info["global_placement"] = placement(to_world.multiply(obj.Placement))
    enums = enumerations(obj)
    if enums:
        info["enums"] = enums
    if obj.TypeId == "App::Link" or obj.isDerivedFrom("App::Link"):
        info["link"] = link_info(obj, to_world)
    elif hasattr(obj, "Shape") and hasattr(obj.Shape, "isNull"):
        shape = None
        try:
            shape = stored.shape(obj.Name)
        except Exception as e:
            info["stored_error"] = str(e)
        if shape is None:
            info["shape"] = shape_info(obj.Shape, to_world)
            info["shape"]["source"] = "object"
        else:
            info["shape"] = shape_info(shape, to_world)
            info["shape"]["source"] = "stored"
            restored = obj.Shape
            if not restored.isNull() and restored.BoundBox.isValid() and shape.BoundBox.isValid():
                a, b = restored.BoundBox, shape.BoundBox
                size = max(a.DiagonalLength, b.DiagonalLength, 1.0)
                corners = (a.XMin - b.XMin, a.YMin - b.YMin, a.ZMin - b.ZMin, a.XMax - b.XMax, a.YMax - b.YMax, a.ZMax - b.ZMax)
                if max(abs(c) for c in corners) > 1e-6 * size:
                    info["restored_differs"] = True
    elif obj.isDerivedFrom("App::Part"):
        try:
            import Part

            info["shape"] = shape_info(Part.getShape(obj), to_world)
        except Exception as e:
            info["shape"] = {"error": str(e)}
    if gui_up():
        view = view_info(obj)
        if view is not None:
            info["view"] = view
    if obj.TypeId.startswith("Sketcher::SketchObject"):
        try:
            info["sketch"] = sketch_info(obj)
        except Exception as e:
            info["sketch"] = {"error": str(e)}
    expressions = expressions_info(obj)
    if expressions:
        info["expressions"] = expressions
    if obj.TypeId == "Spreadsheet::Sheet":
        info["cells"] = cells_info(obj, stored.cells.get(obj.Name, []))
    if obj.TypeId == "App::VarSet":
        info["values"] = variables_info(obj)
    return info


def document_info(doc):
    properties = {}
    for prop in doc.PropertiesList:
        try:
            value = getattr(doc, prop)
        except Exception:
            continue
        if isinstance(value, (str, int, float, bool)) and prop not in ("FileName", "TransientDir"):
            properties[prop] = value
    try:
        program = doc.getProgramVersion()
    except Exception:
        program = None
    stored = StoredShapes(doc.FileName)
    try:
        objects = [object_info(o, stored) for o in doc.Objects]
    finally:
        stored.close()
    return {
        "format": FORMAT,
        "version": FORMAT_VERSION,
        "freecad": ".".join(App.Version()[:3]),
        "program_version": program,
        "file": os.path.basename(doc.FileName),
        "label": doc.Label,
        "properties": properties,
        "objects": objects,
    }


def write(doc, path):
    info = document_info(doc)
    with open(path, "w") as f:
        json.dump(info, f, indent=1)
        f.write("\n")


# Run as a macro (FreeCAD names it differently with and without the user
# interface, so the environment decides).
if os.environ.get("FCSTD") and os.environ.get("JSON"):
    try:
        write(App.openDocument(os.environ["FCSTD"]), os.environ["JSON"])
    except Exception:
        import traceback

        with open(os.environ["JSON"] + ".error", "w") as f:
            f.write(traceback.format_exc())
    finally:
        os._exit(0)
