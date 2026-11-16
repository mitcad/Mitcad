# SPDX-License-Identifier: MIT
# check-all sources: app/webbrowser app/report
"""WebGL in Insert > Component from Web's browser (mitcad#1861): a page of
the test's own HTTP server makes a WebGL context, clears it green, draws a
red triangle over its middle and reads two pixels back; it then goes to a
result page whose address says what it found (webgl=drawn, or none, wrong,
noprogram with the reason), which the browser's log and the server's log
show. Through the GPU where the machine has one, else in software
(SwiftShader): the test machines' hidden displays (Xvfb, the Windows VM's
session 0) have none, so there it is the software path.

The GPU path's fallback and the feedback diagnostics (mitcad#1906), in two
sessions with one browsing profile:
  1. A failure of the GPU process as after WebGL had been drawn on the GPU
     (MITCAD_TEST_WEB_GPU_FAILURE, as a line of CEF's log: the test
     machines have no GPU to fail): the profile's marker is written, the
     window says WebGL is drawn in software from the next start, and Help >
     Send Feedback's diagnostics carry how WebGL was drawn, the GPU
     process's failure and the WebGL page's own console error.
  2. The next start follows the marker: WebGL in software only, and the
     page still draws; the diagnostics say so.

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
  // The page's own error, for the feedback diagnostics (mitcad#1906).
  console.error("A test page error from the WebGL page");
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


# The failure of the GPU process the first session is given (as a line of
# CEF's log), and the page's own console error.
FAILURE = "GPU process exited unexpectedly: exit_code=139"
PAGE_ERROR = "A test page error from the WebGL page"
# The sessions: the GPU's failure, then the next start in software only.
SESSIONS = (1, 2)


def feedback(session):
    """Help > Send Feedback's preview: the diagnostics carry the browser."""
    if session == 1:
        webgl = "(on the GPU|in software)"
        state = "Component browser GPU process: failures in CEF.s log: [0-9]+; WebGL in software from the next start"
    else:
        webgl = "in software"
        state = "Component browser: CEF [^ ]+, WebGL in software only \\(the GPU failed in an earlier session"
    return [
        "focus-main",
        "mark",
        "command help.send_feedback",
        "expect-log '^Send Feedback: the form shows$' 20",
        "focus-dialog '^Send Feedback$'",
        "button '^Preview.*$'",
        "focus-dialog '^Review Report$'",
        "expect-log '^Report section diagnostics: .*Component browser WebGL: %s, WebGL [12], ' 20" % webgl,
        "expect-log '^Report section diagnostics: .*%s' 2" % state,
        "expect-log '^Report section diagnostics: .*Component browser page error: 127\\.0\\.0\\.1: %s' 2" % PAGE_ERROR,
        "button '^Cancel$'",
        "focus-dialog '^Send Feedback$'",
        "button '^Cancel$'",
    ]


def workflow(port, session=1):
    """The in-process test driver's steps (MITCAD_TEST_INPUT) of a session."""
    host = "http://127\\.0\\.0\\.1:%d" % port
    lines = [
        "wait-idle",
        "command insert.component_web",
        "focus-dialog '^Component from Web$'",
        "expect-log '^Component browser opened: 0 site\\(s\\)$' 20",
    ]
    if session == 2:
        # The marker of the first session followed: no GPU this run.
        lines.append("expect-ever '^Component browser: WebGL in software only \\(the GPU failed in an earlier "
                     "session \\([0-9-]+\\): %s\\)$' 5" % FAILURE)
    lines += [
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
        # The probe asked the page how it draws WebGL (mitcad#1906).
        "expect-ever '^Component browser WebGL: (on the GPU|in software), WebGL [12], ' 30",
    ]
    if session == 1:
        # The GPU's failure given: the marker written, the window says so.
        lines += [
            "expect-ever '^Component browser: the GPU failed \\(%s\\); WebGL in software from the next start$' 30"
            % FAILURE,
            "expect-ever '^Component browser: the GPU note shows$' 10",
        ]
    lines += feedback(session)
    lines += [
        "focus-main",
        "quit",
    ]
    return "\n".join(lines) + "\n"


def script_path(directory, session):
    return Path(directory) / ("webgl-%d.mitcad-ui" % session)


def serve(directory):
    """For the macOS wrapper: serves until <output>/stop exists, the
    sessions' steps written once the port is known (the second last)."""
    directory = Path(directory).resolve()
    (directory / "stop").unlink(missing_ok=True)
    server = start_server(directory)
    for session in SESSIONS:
        script_path(directory, session).write_text(workflow(server.server_address[1], session), encoding="utf-8")
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
    drawn = [line for line in results if "GET /result.html?webgl=drawn&" in line]
    if len(drawn) < len(SESSIONS):
        raise RuntimeError("WebGL did not draw in the component browser in every session (%d of %d): %s"
                           % (len(drawn), len(SESSIONS), " | ".join(results)))
    # The first session's GPU failure left the marker the second followed.
    marker = directory / "profile" / "gpu-software.json"
    if not marker.is_file() or '"cef"' not in marker.read_text(encoding="utf-8"):
        raise RuntimeError("the GPU's failure left no marker in the browsing profile: " + str(marker))
    print("WebGL: " + " | ".join(drawn))


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
    # The GPU's choice is the profile marker's (mitcad#1906).
    environment.pop("MITCAD_WEB_GPU", None)
    environment.pop("MITCAD_TEST_WEB_GPU_FAILURE", None)
    # The web browser component the build made (no download).
    if component:
        environment["MITCAD_BROWSER_COMPONENT_DIR"] = str(component)
    return environment


def run_session(app, directory, session, port, qt_bin=None, component=None):
    """One session of the application: the path of its log, and whether it
    failed."""
    log_path = directory / ("webgl-%d.log" % session)
    script = script_path(directory, session)
    script.write_text(workflow(port, session), encoding="utf-8")
    with tempfile.TemporaryDirectory(prefix="settings-", dir=directory) as temporary:
        environment = environment_for(directory, temporary, qt_bin, component)
        environment["MITCAD_TEST_INPUT"] = str(script)
        if session == 1:
            environment["MITCAD_TEST_WEB_GPU_FAILURE"] = FAILURE
        with log_path.open("wb") as log:
            result = subprocess.run([app, "--ui=qt", "--no-native-dialogs", "--no-recovery", "--no-mcp"],
                                    env=environment, stdout=log, stderr=subprocess.STDOUT, timeout=210)
        text = log_path.read_text(encoding="utf-8", errors="replace")
        failed = (result.returncode or any(line.startswith("TestDriver: FAIL") for line in text.splitlines())
                  or "TestDriver: quit" not in text)
    return log_path, failed


def run(app, directory, qt_bin=None, component=None):
    directory = Path(directory).resolve()
    prepare(directory)
    (directory / "server.log").unlink(missing_ok=True)
    server = start_server(directory)
    failure = None
    try:
        for session in SESSIONS:
            log_path, failed = run_session(app, directory, session, server.server_address[1], qt_bin, component)
            if failed:
                failure = (session, log_path)
                break
    finally:
        server.shutdown()
    if failure:
        session, log_path = failure
        # The page's own result (no context, or what it read back) first.
        try:
            verify(directory)
        except (OSError, RuntimeError) as error:
            raise RuntimeError("Component from Web WebGL workflow failed in session %d (%s): %s"
                               % (session, error, log_path))
        raise RuntimeError("Component from Web WebGL workflow failed in session %d: %s" % (session, log_path))
    verify(directory)
    print("PASS: Component from Web: a page's WebGL context made, a triangle drawn and read back; after the GPU's "
          "failure the next start in software only; the feedback diagnostics carry the browser")


if __name__ == "__main__":
    if sys.argv[1] == "prepare":
        prepare(Path(sys.argv[2]).resolve())
    elif sys.argv[1] == "serve":
        serve(sys.argv[2])
    elif sys.argv[1] == "verify":
        verify(Path(sys.argv[2]))
    else:
        run(*sys.argv[1:])
