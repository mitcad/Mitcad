# SPDX-License-Identifier: MIT
#
# Shared prelude of the FreeCAD reference models. run_model.py runs this file
# and then a model script in one namespace, in which these names exist:
# App (FreeCAD), Gui (FreeCADGui or None without a user interface), Part,
# Sketcher, doc (a new document named after the model), OUT (the output
# folder) and VERSION (FreeCAD's version as a tuple of ints, e.g. (1, 0, 2)).
#
# Conventions:
# - Millimetres and degrees, as in FreeCAD.
# - Everything is deterministic and named explicitly (labels are the names).
# - A model that cannot be made with this version raises Unsupported; the
#   driver records it as skipped.
# - The helpers hide what differs between FreeCAD 0.21, 1.0 and 1.1
#   (attachment property names, origin features, colours).

import math

import Part
import Sketcher


class Unsupported(Exception):
    """The model needs something this FreeCAD version does not have."""


def V(x=0.0, y=0.0, z=0.0):
    return App.Vector(x, y, z)


def place(pos=(0, 0, 0), axis=(0, 0, 1), angle=0.0):
    """A placement: a position and a rotation of `angle` degrees about `axis`."""
    return App.Placement(V(*pos), App.Rotation(V(*axis), angle))


def at_least(*version):
    return VERSION >= tuple(version)


def named(obj, name):
    obj.Label = name
    return obj


# ---- origin and attachment --------------------------------------------------


def origin_feature(container, role):
    """The origin feature of a Body or App::Part by its role (XY_Plane, X_Axis...)."""
    for feature in container.Origin.OriginFeatures:
        if getattr(feature, "Role", "") == role:
            return feature
    raise RuntimeError("no origin feature %s in %s" % (role, container.Name))


def attach(obj, references, mode="FlatFace"):
    """Attaches a sketch or datum: references are (object, element) pairs."""
    if hasattr(obj, "AttachmentSupport"):
        obj.AttachmentSupport = references
    else:
        obj.Support = references
    obj.MapMode = mode


# ---- sketches ---------------------------------------------------------------


def polygon_sketch(body, points, name, plane="XY_Plane", closed=True, offset=None):
    """A sketch of line segments through the points (x, y), closed with
    coincident constraints, on an origin plane of the body."""
    sketch = body.newObject("Sketcher::SketchObject", name)
    attach(sketch, [(origin_feature(body, plane), "")])
    if offset is not None:
        sketch.AttachmentOffset = offset
    count = len(points) if closed else len(points) - 1
    for i in range(count):
        a = points[i]
        b = points[(i + 1) % len(points)]
        sketch.addGeometry(Part.LineSegment(V(a[0], a[1], 0), V(b[0], b[1], 0)), False)
    for i in range(count - (0 if closed else 1)):
        sketch.addConstraint(Sketcher.Constraint("Coincident", i, 2, (i + 1) % count, 1))
    return sketch


# Sketcher's special geometry indices and point positions.
H_AXIS, V_AXIS = -1, -2
START, END, MID = 1, 2, 3


def sketch_on(container, name, plane="XY_Plane", offset=None):
    """An empty sketch in a Body or App::Part, attached to one of its origin
    planes (moved by `offset`, a placement in the plane's frame)."""
    sketch = container.newObject("Sketcher::SketchObject", name)
    sketch.Label = name
    attach(sketch, [(origin_feature(container, plane), "")])
    if offset is not None:
        sketch.AttachmentOffset = offset
    return sketch


def add(sketch, geometry, construction=False):
    """Adds a curve or point to a sketch; its geometry index."""
    return sketch.addGeometry(geometry, construction)


def constrain(sketch, *args, **options):
    """Adds Sketcher.Constraint(*args); options name, driving (False: a
    reference dimension) and active (False: switched off). Its index."""
    index = sketch.addConstraint(Sketcher.Constraint(*args))
    if options.get("name"):
        sketch.renameConstraint(index, options["name"])
    if options.get("driving", True) is False:
        sketch.setDriving(index, False)
    if options.get("active", True) is False:
        sketch.setActive(index, False)
    return index


def line(a, b):
    return Part.LineSegment(V(a[0], a[1], 0), V(b[0], b[1], 0))


def circle(center, radius):
    return Part.Circle(V(center[0], center[1], 0), V(0, 0, 1), radius)


def arc(center, radius, start_degrees, end_degrees):
    """A counter-clockwise arc between two angles in degrees."""
    return Part.ArcOfCircle(circle(center, radius), math.radians(start_degrees), math.radians(end_degrees))


def point(at):
    return Part.Point(V(at[0], at[1], 0))


def polyline(sketch, points, closed=False, construction=False):
    """Line segments through the points, joined by coincident constraints;
    the lines' geometry indices."""
    count = len(points) if closed else len(points) - 1
    lines = [add(sketch, line(points[i], points[(i + 1) % len(points)]), construction) for i in range(count)]
    for i in range(count - (0 if closed else 1)):
        constrain(sketch, "Coincident", lines[i], END, lines[(i + 1) % count], START)
    return lines


def direction_angle(a, b):
    """The angle of the direction from a to b, radians (Sketcher's angle of
    a line, and between lines: from the first's direction to the second's)."""
    return math.atan2(b[1] - a[1], b[0] - a[0])


def face_index(shape, normal, height=None):
    """FreeCAD's name of the planar face of a shape facing `normal` (at
    `height` along it when given): Face<n>."""
    for i, face in enumerate(shape.Faces):
        if face.Surface.TypeId != "Part::GeomPlane":
            continue
        n = face.normalAt(0, 0)
        if (n - V(*normal)).Length > 1e-9:
            continue
        if height is not None and abs(face.CenterOfMass.dot(V(*normal)) - height) > 1e-9:
            continue
        return "Face%d" % (i + 1)
    raise RuntimeError("no face facing %s" % (normal,))


def edge_index(shape, a, b):
    """FreeCAD's name of the straight edge of a shape from a to b (3D
    points, either way round): Edge<n>."""
    for i, edge in enumerate(shape.Edges):
        ends = [v.Point for v in edge.Vertexes]
        if len(ends) != 2:
            continue
        if ((ends[0] - V(*a)).Length < 1e-9 and (ends[1] - V(*b)).Length < 1e-9) or (
            (ends[0] - V(*b)).Length < 1e-9 and (ends[1] - V(*a)).Length < 1e-9
        ):
            return "Edge%d" % (i + 1)
    raise RuntimeError("no edge from %s to %s" % (a, b))


def vertex_index(shape, at):
    """FreeCAD's name of the vertex of a shape at a 3D point: Vertex<n>."""
    for i, vertex in enumerate(shape.Vertexes):
        if (vertex.Point - V(*at)).Length < 1e-9:
            return "Vertex%d" % (i + 1)
    raise RuntimeError("no vertex at %s" % (at,))


def rectangle_sketch(body, width, height, name, x=0.0, y=0.0, plane="XY_Plane"):
    return polygon_sketch(
        body, [(x, y), (x + width, y), (x + width, y + height), (x, y + height)], name, plane
    )


def circle_sketch(body, radius, name, cx=0.0, cy=0.0, plane="XY_Plane", offset=None):
    sketch = body.newObject("Sketcher::SketchObject", name)
    attach(sketch, [(origin_feature(body, plane), "")])
    if offset is not None:
        sketch.AttachmentOffset = offset
    sketch.addGeometry(Part.Circle(V(cx, cy, 0), V(0, 0, 1), radius), False)
    sketch.addConstraint(Sketcher.Constraint("Radius", 0, radius))
    return sketch


# ---- features ---------------------------------------------------------------


def body(name, placement=None, parent=None):
    b = doc.addObject("PartDesign::Body", name)
    if placement is not None:
        b.Placement = placement
    if parent is not None:
        parent.addObject(b)
    return b


def pad(body, sketch, length, name, reversed_=False):
    feature = body.newObject("PartDesign::Pad", name)
    feature.Profile = sketch
    feature.Length = length
    feature.Reversed = reversed_
    sketch.Visibility = False
    return feature


def pocket(body, sketch, name, length=None, through_all=False):
    feature = body.newObject("PartDesign::Pocket", name)
    feature.Profile = sketch
    if through_all:
        feature.Type = "ThroughAll"
    else:
        feature.Length = length
    sketch.Visibility = False
    return feature


def block_body(name, width, depth, height, placement=None, parent=None):
    """A Body with one Pad of a width x depth rectangle, height high."""
    b = body(name, placement, parent)
    sketch = rectangle_sketch(b, width, depth, name + "Sketch")
    pad(b, sketch, height, name + "Pad")
    return b


def part(name, placement=None, parent=None):
    p = doc.addObject("App::Part", name)
    if placement is not None:
        p.Placement = placement
    if parent is not None:
        parent.addObject(p)
    return p


def link(name, target, placement=None, transform=None, parent=None):
    lnk = doc.addObject("App::Link", name)
    lnk.setLink(target)
    if transform is not None:
        lnk.LinkTransform = transform
    if placement is not None:
        lnk.Placement = placement
    if parent is not None:
        parent.addObject(lnk)
    return lnk


# ---- feature options (stage 3 models) ---------------------------------------


def sides(feature, mode):
    """A Pad's or Pocket's sides: "symmetric" (Midplane; 1.1: SideType
    Symmetric) or "two" (Type TwoLengths; 1.1: SideType Two sides)."""
    if hasattr(feature, "SideType"):
        feature.SideType = {"symmetric": "Symmetric", "two": "Two sides"}[mode]
    elif mode == "symmetric":
        feature.Midplane = True
    else:
        feature.Type = "TwoLengths"


def sketch_on_face(container, name, feature, normal, height=None):
    """An empty sketch on the planar face of a feature facing `normal`."""
    container.Document.recompute()
    sketch = container.newObject("Sketcher::SketchObject", name)
    sketch.Label = name
    attach(sketch, [(feature, face_index(feature.Shape, normal, height))])
    container.Document.recompute()
    return sketch


def rectangle(sketch, x, y, width, height):
    """A closed rectangle of four lines in a sketch; their indices."""
    return polyline(sketch, [(x, y), (x + width, y), (x + width, y + height), (x, y + height)], closed=True)


def block(container, name, width, depth, height, x=0.0, y=0.0):
    """A Pad of a width x depth rectangle at (x, y) on the XY plane."""
    sketch = sketch_on(container, name + "Sketch")
    rectangle(sketch, x, y, width, depth)
    return pad(container, sketch, height, name)


def edges_between(shape, a, b):
    """The Edge<n> names of a shape's straight edges from a to b."""
    return [edge_index(shape, a, b)]


def edges_where(shape, test):
    """The Edge<n> names of the edges whose middle passes the test."""
    names = []
    for i, edge in enumerate(shape.Edges):
        middle = edge.valueAt((edge.FirstParameter + edge.LastParameter) / 2.0)
        if test(middle, edge):
            names.append("Edge%d" % (i + 1))
    return names


# ---- expressions (stage 4 models) -------------------------------------------


def spreadsheet(name, cells):
    """A spreadsheet with cells (address, content) or (address, content,
    alias); contents as typed in FreeCAD (`=Width / 2`, `12`, `'text`)."""
    sheet = doc.addObject("Spreadsheet::Sheet", name)
    sheet.Label = name
    for cell in cells:
        sheet.set(cell[0], cell[1])
        if len(cell) > 2:
            sheet.setAlias(cell[0], cell[2])
    doc.recompute()
    return sheet


def varset(name, properties):
    """A VarSet (FreeCAD 1.0 and later) with properties (type, name, value);
    a value "=…" is an expression bound to the property."""
    if not at_least(1, 0):
        raise Unsupported("App::VarSet is new in FreeCAD 1.0")
    variables = doc.addObject("App::VarSet", name)
    variables.Label = name
    for kind, prop, value in properties:
        variables.addProperty("App::Property" + kind, prop, "Parameters")
        if isinstance(value, str) and value.startswith("="):
            variables.setExpression(prop, value[1:])
        else:
            setattr(variables, prop, value)
    doc.recompute()
    return variables


def bind(obj, path, expression):
    """Binds an expression to a property path (`Length`,
    `.Constraints.width`, `.AttachmentOffset.Base.z`)."""
    obj.setExpression(path, expression)


VARIANTS = []


def variant(change, suffix="changed"):
    """Another document of the model: run_model.py saves the model, then
    applies `change` (a function), recomputes and saves a copy as
    <id>_<suffix>.FCStd, which it dumps too. The script's "# Mitcad
    change: <parameter> = <expression>" lines say the same change in
    Mitcad's terms: fcstd-corpus.cmake imports the model, makes the
    change and compares the result with <id>_changed's dump."""
    VARIANTS.append((suffix, change))


# ---- display ----------------------------------------------------------------


def hide(obj):
    obj.Visibility = False
    if Gui is not None and obj.ViewObject is not None:
        obj.ViewObject.Visibility = False


def color(obj, rgb):
    """Sets the shape colour (r, g, b in 0..1); needs the user interface."""
    if Gui is None or obj.ViewObject is None:
        return
    view = obj.ViewObject
    if hasattr(view, "ShapeAppearance"):
        material = App.Material()
        material.DiffuseColor = tuple(rgb)
        view.ShapeAppearance = [material]
    else:
        view.ShapeColor = tuple(rgb)


def face_colors(obj, colors):
    """Colours per face (r, g, b), in the shape's face order."""
    if Gui is None or obj.ViewObject is None:
        return
    view = obj.ViewObject
    if hasattr(view, "ShapeAppearance"):
        materials = []
        for rgb in colors:
            material = App.Material()
            material.DiffuseColor = tuple(rgb)
            materials.append(material)
        view.ShapeAppearance = materials
    else:
        view.DiffuseColor = [tuple(c) + (0.0,) for c in colors]


# ---- other documents --------------------------------------------------------

EXTRA_DOCUMENTS = []


def save_model():
    """Saves the model's document into OUT already (links to other files
    need their owner saved; the driver saves it again at the end)."""
    import os

    doc.recompute()
    doc.saveAs(os.path.join(OUT, doc.Name + ".FCStd"))


def save_extra(document, name):
    """Saves another document of the model into OUT as <name>.FCStd (before
    links to it are made, so that they refer to it by a relative path)."""
    import os

    path = os.path.join(OUT, name + ".FCStd")
    document.recompute()
    document.saveAs(path)
    EXTRA_DOCUMENTS.append(path)
    return path
