# SPDX-License-Identifier: MIT
#
# Makes one FreeCAD reference model: runs _prelude.py and the model script
# (environment MODEL) in a new document, recomputes and saves it as
# OUT/<id>.FCStd, then opens the saved file again and writes its dump
# (dump.py) to OUT/<id>.json. Other documents the model saved
# (save_extra) and its variants (variant: the model changed, saved as
# OUT/<id>_<suffix>.FCStd) are dumped too. The log goes to OUT/<id>.log, and
# OUT/<id>.status says "ok", "skipped: <why>" or "failed: <why>".
#
# Run by run_all.sh with FreeCAD's user interface on a hidden display
# (freecad run_model.py), so that the documents keep their view data
# (GuiDocument.xml: visibility, colours); also works with freecadcmd.
#
# Environment: MODEL (path of the model script), OUT (output folder),
# TOOLS (this folder).

import os
import sys
import traceback

import FreeCAD as App

try:
    import FreeCADGui as Gui

    if not App.GuiUp:
        Gui = None
except ImportError:
    Gui = None

MODEL = os.environ["MODEL"]
OUT = os.environ["OUT"]
TOOLS = os.environ.get("TOOLS", os.path.dirname(os.path.abspath(MODEL)) + "/..")
ID = os.path.splitext(os.path.basename(MODEL))[0]
LOG = open(os.path.join(OUT, ID + ".log"), "w")


def log(*parts):
    LOG.write(" ".join(str(p) for p in parts) + "\n")
    LOG.flush()


def status(text):
    with open(os.path.join(OUT, ID + ".status"), "w") as f:
        f.write(text + "\n")


def version():
    numbers = []
    for part in App.Version()[:3]:
        digits = "".join(c for c in part if c.isdigit())
        numbers.append(int(digits or 0))
    return tuple(numbers)


def close_all():
    for name in list(App.listDocuments()):
        App.closeDocument(name)


def run():
    sys.path.insert(0, TOOLS)
    import dump

    # No backup files (.FCBak) when a model saves its document twice.
    App.ParamGet("User parameter:BaseApp/Preferences/Document").SetBool("CreateBackupFiles", False)

    namespace = {
        "__name__": "freecad_model",
        "App": App,
        "Gui": Gui,
        "OUT": OUT,
        "VERSION": version(),
    }
    doc = App.newDocument(ID)
    namespace["doc"] = doc
    for path in (os.path.join(TOOLS, "_prelude.py"), MODEL):
        with open(path) as f:
            code = compile(f.read(), path, "exec")
        try:
            exec(code, namespace)
        except namespace["Unsupported"] as why:
            status("skipped: %s" % why)
            log("skipped:", why)
            return
    doc.recompute()
    for obj in doc.Objects:
        state = [s for s in obj.State if s in ("Invalid", "Error", "Touched")]
        if state:
            log("object", obj.Name, "state", state)
    target = os.path.join(OUT, ID + ".FCStd")
    doc.saveAs(target)
    extra = list(namespace.get("EXTRA_DOCUMENTS", []))
    # Variants: the model changed and saved as copies (the model's own
    # file is saved already).
    for suffix, change in namespace.get("VARIANTS", []):
        change()
        doc.recompute()
        # FreeCAD's own recompute of the change must succeed (the
        # comparison needs its results).
        failed = [o.Name for o in doc.Objects if "Invalid" in o.State or "Error" in o.State]
        if failed:
            raise RuntimeError("variant %s: %s failed in FreeCAD" % (suffix, ", ".join(failed)))
        path = os.path.join(OUT, ID + "_" + suffix + ".FCStd")
        doc.saveCopy(path)
        extra.append(path)
    close_all()
    # The dumps describe the files as saved: opened again, not recomputed.
    for path in [target] + extra:
        opened = App.openDocument(path)
        dump.write(opened, os.path.splitext(path)[0] + ".json")
        log("dumped", path)
        close_all()
    status("ok")


try:
    run()
except Exception:
    log(traceback.format_exc())
    status("failed: " + traceback.format_exc().strip().splitlines()[-1])
finally:
    LOG.close()
    sys.stdout.flush()
    os._exit(0)
