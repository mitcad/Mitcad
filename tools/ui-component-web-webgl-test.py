# SPDX-License-Identifier: MIT
# check-all sources: app/webbrowser
"""WebGL in Insert > Component from Web's browser (mitcad#1861): a page of
the test's own HTTP server makes a WebGL context, clears it green, draws a
red triangle over its middle and reads two pixels back; it then goes to a
result page whose address says what it found (webgl=drawn, or none, wrong,
noprogram with the reason), which the browser's log and the server's log
show. Through the GPU where the machine has one, else in software
(SwiftShader): the test machines' hidden displays (Xvfb, the Windows VM's
session 0) have none, so there it is the software path.

Usage: ui-component-web-webgl-test.py <app> <output folder> [<Qt's bin> [<the built web browser component>]]
       ui-component-web-webgl-test.py prepare <output folder>
       ui-component-web-webgl-test.py serve <output folder>   (until its stop file)
       ui-component-web-webgl-test.py verify <output folder>
"""
import functools
import http.server
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import time

PAGE = """<!doctype html>
<html><head><meta charset="utf-8"><title>WebGL</title></head>
<body><h1>WebGL</h1><canvas id="view" width="64" height="64"></canvas>
<script>
function report(state, detail) {
  location.replace("result.html?webgl=" + state + "&detail=" + encodeURIComponent(detail));
}
window.addEventListener("load", function () {
  var canvas = document.getElementById("view");
  var why = "";
  canvas.addEventListener("webglcontextcreationerror", function (event) {
    why = event.statusMessage || "";
  });
  var gl = canvas.getContext("webgl", {antialias: false, preserveDrawingBuffer: true});
  if (!gl) {
    report("none", why || "no WebGL context");
    return;
  }
  function shader(kind, source) {
    var made = gl.createShader(kind);
    gl.shaderSource(made, source);
    gl.compileShader(made);
    return made;
  }
  var program = gl.createProgram();
  gl.attachShader(program, shader(gl.VERTEX_SHADER,
      "attribute vec2 p; void main() { gl_Position = vec4(p, 0.0, 1.0); }"));
  gl.attachShader(program, shader(gl.FRAGMENT_SHADER,
      "precision mediump float; void main() { gl_FragColor = vec4(1.0, 0.0, 0.0, 1.0); }"));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    report("noprogram", gl.getProgramInfoLog(program) || "the program did not link");
    return;
  }
  gl.useProgram(program);
  gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-0.5, -0.5, 0.5, -0.5, 0.0, 0.5]), gl.STATIC_DRAW);
  var p = gl.getAttribLocation(program, "p");
  gl.enableVertexAttribArray(p);
  gl.vertexAttribPointer(p, 2, gl.FLOAT, false, 0, 0);
  gl.viewport(0, 0, 64, 64);
  gl.clearColor(0.0, 1.0, 0.0, 1.0);
  gl.clear(gl.COLOR_BUFFER_BIT);
  gl.drawArrays(gl.TRIANGLES, 0, 3);
  // Inside the triangle (16..48 px across, 16..48 up) and in a corner.
  var middle = new Uint8Array(4);
  var corner = new Uint8Array(4);
  gl.readPixels(32, 28, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, middle);
  gl.readPixels(2, 2, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, corner);
  var info = gl.getExtension("WEBGL_debug_renderer_info");
  var renderer = info ? gl.getParameter(info.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER);
  var drawn = middle[0] > 200 && middle[1] < 60 && corner[1] > 200 && corner[0] < 60;
  report(drawn ? "drawn" : "wrong", "middle " + Array.from(middle).join(",") + " corner " +
         Array.from(corner).join(",") + " renderer " + renderer);
});
</script></body></html>
"""

RESULT = """<!doctype html>
<html><head><meta charset="utf-8"><title>WebGL result</title></head>
<body><p>The WebGL page's result is in this page's address.</p></body></html>
"""


def prepare(directory):
    """The site: the WebGL page and the result page it goes to."""
    directory.mkdir(parents=True, exist_ok=True)
    for name in ("site", "downloads", "profile"):
        shutil.rmtree(directory / name, ignore_errors=True)
    site = directory / "site"
    site.mkdir()
    (site / "webgl.html").write_text(PAGE, encoding="utf-8")
    (site / "result.html").write_text(RESULT, encoding="utf-8")


class Handler(http.server.SimpleHTTPRequestHandler):
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
    lines = [
        "wait-idle",
        "command insert.component_web",
        "focus-dialog '^Component from Web$'",
        "expect-log '^Component browser opened: 0 site\\(s\\)$' 20",
        # The start page loaded first, so that nothing of it can come after the
        # address typed below.
        "expect-log '^Component browser load end data:' 60",
        "mark",
        "focus-widget componentBrowserAddress",
        "type http://127.0.0.1:%d/webgl.html" % port,
        "key Return",
        # The WebGL page went on to its result: first that it got there at
        # all, then that the triangle was drawn (else the address, with the
        # reason, is in the log above the failure).
        "expect-log '^Component browser load end %s/result\\.html\\?webgl=' 90" % host,
        "expect-log '^Component browser load end %s/result\\.html\\?webgl=drawn&' 2" % host,
        "focus-main",
        "quit",
    ]
    return "\n".join(lines) + "\n"


def serve(directory):
    """For the macOS wrapper: serves until <output>/stop exists, the
    driver's steps written once the port is known."""
    directory = Path(directory).resolve()
    (directory / "stop").unlink(missing_ok=True)
    server = start_server(directory)
    (directory / "webgl.mitcad-ui").write_text(workflow(server.server_address[1]), encoding="utf-8")
    try:
        while not (directory / "stop").exists():
            time.sleep(0.2)
    finally:
        server.shutdown()


def verify(directory):
    directory = Path(directory)
    served = (directory / "server.log").read_text(encoding="utf-8")
    if "GET /webgl.html" not in served:
        raise RuntimeError("the test's server was not asked for the WebGL page: " + served)
    results = [line for line in served.splitlines() if "GET /result.html?" in line]
    if not results:
        raise RuntimeError("the WebGL page did not go on to its result: " + served)
    if not any("GET /result.html?webgl=drawn&" in line for line in results):
        raise RuntimeError("WebGL did not draw in the component browser: " + " | ".join(results))
    print("WebGL: " + results[-1])


def environment_for(directory, temporary, qt_bin=None, component=None):
    environment = dict(os.environ)
    if qt_bin:
        environment["PATH"] = qt_bin + os.pathsep + environment.get("PATH", "")
    for variable in ("SETTINGS", "AUTOSAVE", "PROJECTS", "THUMBNAIL", "CRASH"):
        environment["MITCAD_" + variable + "_DIR"] = str(Path(temporary) / variable.lower())
    environment.update(MITCAD_RESULT_STORE="off", MITCAD_START_PAGE="0", MITCAD_NEW_DESIGN_VIEW="0", MITCAD_LANGUAGE="en",
                       MITCAD_NO_UPDATE_CHECK="1", QT_FORCE_STDERR_LOGGING="1",
                       MITCAD_WEB_DOWNLOADS_DIR=str(directory / "downloads"),
                       MITCAD_WEB_PROFILE_DIR=str(directory / "profile"),
                       # Never the user's installed components.
                       MITCAD_BROWSER_COMPONENTS_DIR=str(directory / "components"))
    # The web browser component the build made (no download).
    if component:
        environment["MITCAD_BROWSER_COMPONENT_DIR"] = str(component)
    return environment


def run(app, directory, qt_bin=None, component=None):
    directory = Path(directory).resolve()
    prepare(directory)
    (directory / "server.log").unlink(missing_ok=True)
    server = start_server(directory)
    log_path = directory / "webgl.log"
    try:
        script = directory / "webgl.mitcad-ui"
        script.write_text(workflow(server.server_address[1]), encoding="utf-8")
        with tempfile.TemporaryDirectory(prefix="settings-", dir=directory) as temporary:
            environment = environment_for(directory, temporary, qt_bin, component)
            environment["MITCAD_TEST_INPUT"] = str(script)
            with log_path.open("wb") as log:
                result = subprocess.run([app, "--ui=qt", "--no-native-dialogs", "--no-recovery", "--no-mcp"],
                                        env=environment, stdout=log, stderr=subprocess.STDOUT, timeout=240)
            text = log_path.read_text(encoding="utf-8", errors="replace")
            failed = (result.returncode or any(line.startswith("TestDriver: FAIL") for line in text.splitlines())
                      or "TestDriver: quit" not in text)
    finally:
        server.shutdown()
    if failed:
        # The page's own result (no context, or what it read back) first.
        try:
            verify(directory)
        except (OSError, RuntimeError) as error:
            raise RuntimeError("Component from Web WebGL workflow failed (%s): %s" % (error, log_path))
        raise RuntimeError("Component from Web WebGL workflow failed: " + str(log_path))
    verify(directory)
    print("PASS: Component from Web: a page's WebGL context made, a triangle drawn and read back")


if __name__ == "__main__":
    if sys.argv[1] == "prepare":
        prepare(Path(sys.argv[2]).resolve())
    elif sys.argv[1] == "serve":
        serve(sys.argv[2])
    elif sys.argv[1] == "verify":
        verify(Path(sys.argv[2]))
    else:
        run(*sys.argv[1:])
