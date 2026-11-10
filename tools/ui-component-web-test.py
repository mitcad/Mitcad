# SPDX-License-Identifier: MIT
# check-all sources: app/webbrowser app/files/FileFormats.cpp core/ffi/src/web_downloads.rs core/model/src/document/library_parts.rs tools/cli
"""Insert > Component from Web (mitcad#1579) against a page of a small HTTP
server of the test's own: a STEP file comes in as a component named after it
with its part number and in Move; a zip of two STEP files lists both and
brings in the one picked; a PDF is refused with its note. The downloads'
temporary folders are gone afterwards.

Usage: ui-component-web-test.py <app> <cli> <output folder> [<Qt's bin> [<the built web browser component>]]
       ui-component-web-test.py prepare <cli> <output folder>
       ui-component-web-test.py serve <output folder>   (until its stop file)
       ui-component-web-test.py verify <output folder>
"""
import functools
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from urllib.parse import unquote
import zipfile

STEP = "6494K13_NO THREADS_Bearing.step"
PINS = ("PA100_pin.step", "PB200_pin.step")


def prepare(cli, directory, environment=None):
    """The site's files: three STEP files MitCad exports, a zip of two of
    them with a text file, a PDF and the page that links them."""
    directory.mkdir(parents=True, exist_ok=True)
    for name in ("site", "downloads", "profile"):
        shutil.rmtree(directory / name, ignore_errors=True)
    site = directory / "site"
    site.mkdir()
    parts = directory / "parts"
    parts.mkdir(exist_ok=True)
    commands = []
    for index, (name, length) in enumerate(((STEP, 30), (PINS[0], 10), (PINS[1], 20)), start=1):
        commands += [{"cmd": "add_feature", "def": {
                         "type": "box", "plane": {"type": "origin", "plane": "xy"},
                         "corner": [0, 0], "length": length, "width": 10, "height": 10,
                         "operation": "new_body"}},
                     {"cmd": "export", "path": str(parts / name), "bodies": ["F%d.b0" % index],
                      "format": "step"}]
    fixture = directory / "parts.json"
    fixture.write_text(json.dumps(commands), encoding="utf-8")
    with (directory / "setup.log").open("wb") as log:
        subprocess.run([cli, "run", str(fixture)], check=True, env=environment, stdout=log,
                       stderr=subprocess.STDOUT, timeout=120)
    shutil.copyfile(parts / STEP, site / STEP)
    with zipfile.ZipFile(site / "pins.zip", "w", zipfile.ZIP_DEFLATED) as archive:
        for name in PINS:
            archive.write(parts / name, "pins/" + name)
        archive.writestr("pins/readme.txt", "Two pins.\n")
    (site / "datasheet.pdf").write_bytes(b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog >>\nendobj\n%%EOF\n")
    (site / "index.html").write_text(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Parts</title></head><body>"
        "<h1>Parts</h1><ul>"
        "<li><a href=\"6494K13_NO%20THREADS_Bearing.step\">Bearing 6494K13</a></li>"
        "<li><a href=\"pins.zip\">Pins (zip)</a></li>"
        "<li><a href=\"datasheet.pdf\">Datasheet</a></li>"
        "</ul></body></html>\n", encoding="utf-8")


class Handler(http.server.SimpleHTTPRequestHandler):
    """The site's files, each but the page as an attachment, as a supplier's
    download links send them."""

    def end_headers(self):
        name = self.path.split("?")[0].rsplit("/", 1)[-1]
        if name and not name.endswith(".html"):
            self.send_header("Content-Disposition", 'attachment; filename="%s"' % unquote(name))
        super().end_headers()

    def log_message(self, format, *args):
        with open(self.server.log_path, "a", encoding="utf-8") as log:
            log.write("%s\n" % (format % args))


def start_server(directory):
    server = http.server.ThreadingHTTPServer(
        ("127.0.0.1", 0), functools.partial(Handler, directory=str(directory / "site")))
    server.log_path = str(directory / "server.log")
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server


def workflow(port):
    """The in-process test driver's steps (MITCAD_TEST_INPUT)."""
    host = "http://127\\.0\\.0\\.1:%d" % port
    step = STEP.replace(".", "\\.")
    name = STEP[:-len(".step")]
    lines = [
        "wait-idle",
        "capture-undo-depth before",
        "mark",
        "command insert.component_web",
        "focus-dialog '^Component from Web$'",
        "expect-log '^Component browser opened: 0 site\\(s\\)$' 20",
        # The start page loaded first, so that nothing of it can come after the
        # address typed below.
        "expect-log '^Component browser load end data:' 60",
        # The page of the test's server.
        "mark",
        "focus-widget componentBrowserAddress",
        "type http://127.0.0.1:%d/index.html" % port,
        "key Return",
        "expect-log '^Component browser page: %s/index\\.html \\(3 links\\)$' 60" % host,
        # The STEP file: a component named after it, its part number, in Move.
        "mark",
        "invoke-widget componentBrowser clickLink 'Bearing 6494K13'",
        "expect-log '^Component browser download: %s from %s/' 30" % (step, host),
        "expect-log '^Component from Web: inserted %s as component %s \\(C[0-9]+, occurrence O[0-9]+, "
        "1 body\\(ies\\)\\), part number 6494K13$' 60" % (step, name),
        "expect-log '^Command Move/Copy started$' 20",
        "expect-log '^Component browser removed the temporary files of %s$' 20" % step,
        # (the bodies and their total volume: the 30 x 10 x 10 mm block)
        "expect-model-bodies 1 3000",
        "expect-undo-depth ${before} 1",
        "focus-main",
        "key Escape",
        "expect-log '^Command Move/Copy cancelled$' 20",
        # The zip: both STEP files listed (not its text file), the one picked
        # brought in.
        "focus-dialog '^Component from Web$'",
        "mark",
        "invoke-widget componentBrowser clickLink 'Pins (zip)'",
        "expect-log '^Component browser zip pins\\.zip: 2 file\\(s\\) MitCad imports: "
        "pins/PA100_pin\\.step \\([0-9]+ bytes\\) \\| pins/PB200_pin\\.step \\([0-9]+ bytes\\); 1 other\\(s\\)$' 60",
        "focus-dialog '^Files in pins\\.zip$' 20",
        "expect-widget-property componentBrowserZipFiles topLevelItemCount 2",
        "focus-widget componentBrowserZipFiles",
        "key Down",
        "key space",
        "expect-log '^Component browser zip picked: pins/PB200_pin\\.step$'",
        "button '^Import$'",
        "expect-log '^Component from Web: inserted PB200_pin\\.step as component PB200_pin \\(C[0-9]+, "
        "occurrence O[0-9]+, 1 body\\(ies\\)\\), part number PB200$' 60",
        "expect-no-log 'inserted PA100_pin'",
        "expect-log '^Command Move/Copy started$' 20",
        "expect-log '^Component browser removed the temporary files of pins\\.zip$' 20",
        "expect-model-bodies 2 5000",
        "expect-undo-depth ${before} 2",
        "focus-main",
        "key Escape",
        "expect-log '^Command Move/Copy cancelled$' 20",
        # The PDF: refused with its note, nothing imported.
        "focus-dialog '^Component from Web$'",
        "mark",
        "invoke-widget componentBrowser clickLink 'Datasheet'",
        "expect-log '^Component browser refused datasheet\\.pdf from %s/datasheet\\.pdf: "
        "MitCad cannot import PDF documents \\(\\.pdf\\)\\.$' 30" % host,
        "expect-widget-property componentBrowserNote text 'MitCad cannot import PDF documents (.pdf).'",
        "expect-no-log '^Component browser download: datasheet'",
        "focus-main",
        "expect-model-bodies 2 5000",
        "expect-undo-depth ${before} 2",
        "quit",
    ]
    return "\n".join(lines) + "\n"


def serve(directory):
    """For the macOS wrapper: serves until <output>/stop exists, the
    driver's steps written once the port is known."""
    directory = Path(directory).resolve()
    (directory / "stop").unlink(missing_ok=True)
    server = start_server(directory)
    (directory / "web.mitcad-ui").write_text(workflow(server.server_address[1]), encoding="utf-8")
    try:
        while not (directory / "stop").exists():
            time.sleep(0.2)
    finally:
        server.shutdown()


def verify(directory):
    directory = Path(directory)
    served = (directory / "server.log").read_text(encoding="utf-8")
    for request in ("GET /index.html", "GET /6494K13_NO%20THREADS_Bearing.step", "GET /pins.zip",
                    "GET /datasheet.pdf"):
        if request not in served:
            raise RuntimeError("the test's server was not asked for " + request + ": " + served)
    downloads = directory / "downloads"
    left = [str(path) for path in downloads.rglob("*")] if downloads.exists() else []
    if left:
        raise RuntimeError("the downloads' temporary files were left behind: " + ", ".join(left))


def environment_for(directory, temporary, qt_bin=None, component=None):
    environment = dict(os.environ)
    if qt_bin:
        environment["PATH"] = qt_bin + os.pathsep + environment.get("PATH", "")
    for variable in ("SETTINGS", "AUTOSAVE", "PROJECTS", "THUMBNAIL", "CRASH"):
        environment["MITCAD_" + variable + "_DIR"] = str(Path(temporary) / variable.lower())
    environment.update(MITCAD_RESULT_STORE="off", MITCAD_START_PAGE="0", MITCAD_LANGUAGE="en",
                       MITCAD_NO_UPDATE_CHECK="1", QT_FORCE_STDERR_LOGGING="1",
                       MITCAD_WEB_DOWNLOADS_DIR=str(directory / "downloads"),
                       MITCAD_WEB_PROFILE_DIR=str(directory / "profile"),
                       # Never the user's installed components.
                       MITCAD_BROWSER_COMPONENTS_DIR=str(directory / "components"))
    # The web browser component the build made (no download): CEF paints in
    # software and runs without its sandbox there.
    if component:
        environment["MITCAD_BROWSER_COMPONENT_DIR"] = str(component)
    return environment


def run(app, cli, directory, qt_bin=None, component=None):
    directory = Path(directory).resolve()
    prepare(cli, directory, environment_for(directory, directory, qt_bin))
    (directory / "server.log").unlink(missing_ok=True)
    server = start_server(directory)
    try:
        script = directory / "web.mitcad-ui"
        script.write_text(workflow(server.server_address[1]), encoding="utf-8")
        with tempfile.TemporaryDirectory(prefix="settings-", dir=directory) as temporary:
            environment = environment_for(directory, temporary, qt_bin, component)
            environment["MITCAD_TEST_INPUT"] = str(script)
            log_path = directory / "web.log"
            with log_path.open("wb") as log:
                result = subprocess.run([app, "--ui=qt", "--no-native-dialogs", "--no-recovery", "--no-mcp"],
                                        env=environment, stdout=log, stderr=subprocess.STDOUT, timeout=300)
            text = log_path.read_text(encoding="utf-8", errors="replace")
            if (result.returncode or any(line.startswith("TestDriver: FAIL") for line in text.splitlines())
                    or "TestDriver: quit" not in text):
                raise RuntimeError("Component from Web UI workflow failed: " + str(log_path))
    finally:
        server.shutdown()
    verify(directory)
    print("PASS: Component from Web: a STEP as a numbered component in Move, a zip's picked file, "
          "a PDF refused, temporary files removed")


if __name__ == "__main__":
    if sys.argv[1] == "prepare":
        prepare(sys.argv[2], Path(sys.argv[3]).resolve())
    elif sys.argv[1] == "serve":
        serve(sys.argv[2])
    elif sys.argv[1] == "verify":
        verify(Path(sys.argv[2]))
    else:
        run(*sys.argv[1:])
