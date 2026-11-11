# SPDX-License-Identifier: MIT
# check-all sources: app/webbrowser
"""Insert > Component from Web on a scaled display (mitcad#1654): the web
browser component's view at a scale of 1.65 (QT_SCALE_FACTOR, the hidden
Xvfb display), on a page of the test's own HTTP server: a green page with a
red button.

1. The page fills the view and the button is where the page puts it, on the
   screen; a real click at the button's place clicks it (the page goes on to
   its next page).
2. After the window's scale changes (the view's screenChanged, which Qt's
   DevicePixelRatioChange and the window's screenChanged call; CEF is asked
   for the screen's information again), the same.

The places are read from the screen (xwd), and the clicks are X input
(xdotool), as a user's. The pinned CEF paints its windowless frames at a
scale of 1 whatever the view tells it; the view marked them with its own
scale, so that the page filled only 1/1.65 of the view at its top left
while clicks landed where the page would have been at full size.

With --device-scale, CEF must also paint its frames at the view's scale
(the frame's pixel size the view's size times 1.65, sharp): the check for a
new CEF version (mitcad#1657). Neither the pinned 156.0.27 nor 156.0.28 does
(at 1.65 or 2, with GetScreenInfo giving the screen's rectangle too), so the
registered test runs without it.

Usage: ui-component-web-scale-test.py [--device-scale] <app> <output folder> <the built web browser component> [<Qt's bin>]
Runs on its own hidden X display (cmake/SelectionUiTests.cmake: xvfb-run).
"""
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time

SCALE = 1.65
# The button on the page, in the page's (CSS) pixels.
BUTTON = (120, 80, 160, 90)
TOOLS = Path(__file__).resolve().parent


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, TOOLS / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


web = load("component_web_test", "ui-component-web-test.py")
xwd = load("xwd2png", "xwd2png.py")


def page(next_page):
    left, top, width, height = BUTTON
    return ("<!doctype html><html><head><meta charset=\"utf-8\"><title>Scale</title></head>"
            "<body style=\"margin:0;background:#00c000;height:100vh;overflow:hidden\">"
            "<button id=\"go\" style=\"position:absolute;left:%dpx;top:%dpx;width:%dpx;height:%dpx;"
            "border:0;margin:0;padding:0;background:#e00000\" onclick=\"location.href='%s'\"></button>"
            "</body></html>\n" % (left, top, width, height, next_page))


def workflow(port, directory):
    host = "http://127\\.0\\.0\\.1:%d" % port
    frame = "^Component browser frame: [0-9]+x[0-9]+ px at scale [0-9.]+ for a view of [0-9]+x[0-9]+ at scale 1\\.65$"
    lines = [
        "wait-idle",
        "command insert.component_web",
        "focus-dialog '^Component from Web$'",
        "expect-log '^Component browser load end data:' 60",
        "focus-widget componentBrowserAddress",
        "type http://127.0.0.1:%d/page1.html" % port,
        "key Return",
        "expect-log '^Component browser page: %s/page1\\.html' 60" % host,
        # 1. The view at 1.65 (the start page's frame already).
        "expect-ever '%s' 30" % frame,
        "expect-file '%s' 120" % (directory / "phase1.done"),
        "expect-log '^Component browser navigates to %s/page2\\.html' 30" % host,
        "expect-log '^Component browser page: %s/page2\\.html' 60" % host,
        # 2. The window's scale changes.
        "mark",
        "invoke-widget componentBrowserView screenChanged",
        "expect-log '^Component browser scale: 1\\.65$' 10",
        "expect-file '%s' 120" % (directory / "phase2.done"),
        "expect-log '^Component browser navigates to %s/page3\\.html' 30" % host,
        "quit",
    ]
    return "\n".join(lines) + "\n"


def wait_log(log_path, pattern, after, timeout):
    """The number of the first line after `after` lines that contains
    `pattern`."""
    end = time.time() + timeout
    while time.time() < end:
        lines = log_path.read_text(encoding="utf-8", errors="replace").splitlines()
        for number in range(after, len(lines)):
            if pattern in lines[number]:
                return number
        time.sleep(0.2)
    raise RuntimeError("no '%s' in %s" % (pattern, log_path))


def color_box(rows, test, x0, y0, x1, y1):
    """The bounding box (x0, y0, x1, y1) of the pixels `test` accepts in a
    rectangle of the screen, or None."""
    found = None
    for y in range(y0, y1):
        row = rows[y]
        for x in range(x0, x1):
            if test(row[3 * x], row[3 * x + 1], row[3 * x + 2]):
                found = ((x, y, x + 1, y + 1) if found is None else
                         (min(found[0], x), found[1], max(found[2], x + 1), y + 1))
    return found


def boxes(rows, width, height):
    """The page's green on the screen, and the button's red in it (the
    window's own red, the address field's clear button, is outside)."""
    page = color_box(rows, lambda r, g, b: g > 150 and r < 80 and b < 80, 0, 0, width, height)
    if page is None:
        return None, None
    return page, color_box(rows, lambda r, g, b: r > 170 and g < 80 and b < 80, *page)


def near(actual, expected, what, slack=3):
    if abs(actual - expected) > slack:
        raise RuntimeError("%s is %s px on the screen, expected %.1f" % (what, actual, expected))


def last_frame(log_path):
    """The last frame's scale and the view's size in logical pixels, from
    "Component browser frame: 1584x891 px at scale 1.65 for a view of 960x540
    at scale 1.65"."""
    lines = [line for line in log_path.read_text(encoding="utf-8", errors="replace").splitlines()
             if line.startswith("Component browser frame: ")]
    if not lines:
        raise RuntimeError("no frame logged")
    words = lines[-1].split()
    size = words[words.index("view") + 2]
    return float(words[words.index("px") + 3]), tuple(int(v) for v in size.split("x"))


def check_and_click(directory, log_path, phase, device_scale):
    """The page fills the view and the button is where the page puts it,
    on the screen; then a click there."""
    dump = directory / ("phase%d.xwd" % phase)
    last = ""
    for _ in range(20):
        with dump.open("wb") as out:
            subprocess.run(["xwd", "-root", "-silent"], stdout=out, check=True)
        width, height, rows = xwd.read_xwd(str(dump))
        page_box, button_box = boxes(rows, width, height)
        scale, view = last_frame(log_path)
        if device_scale and abs(scale - SCALE) > 0.02:
            last = "CEF painted its frame at a scale of %.2f, not the view's %.2f" % (scale, SCALE)
        elif page_box and button_box:
            gx0, gy0, gx1, gy1 = page_box
            try:
                near(gx1 - gx0, view[0] * SCALE, "the page's width")
                near(gy1 - gy0, view[1] * SCALE, "the page's height")
                left, top, bw, bh = BUTTON
                near(button_box[0] - gx0, left * SCALE, "the button's left")
                near(button_box[1] - gy0, top * SCALE, "the button's top")
                near(button_box[2] - button_box[0], bw * SCALE, "the button's width")
                near(button_box[3] - button_box[1], bh * SCALE, "the button's height")
            except RuntimeError as error:
                last = str(error)
            else:
                xwd.write_png(str(directory / ("phase%d.png" % phase)), width, height, rows)
                x = (button_box[0] + button_box[2]) // 2
                y = (button_box[1] + button_box[3]) // 2
                subprocess.run(["xdotool", "mousemove", str(x), str(y), "click", "1"], check=True)
                print("ok   phase %d: CEF at scale %.2f, the page fills the view (%dx%d px on the screen for %dx%d), "
                      "the button at (%d, %d) clicked" % (phase, scale, gx1 - gx0, gy1 - gy0, view[0], view[1], x, y))
                return
        else:
            last = "no green page and red button on the screen"
        time.sleep(0.5)
    xwd.write_png(str(directory / ("phase%d.png" % phase)), width, height, rows)
    raise RuntimeError("phase %d: %s (the screen: %s)" % (phase, last, directory / ("phase%d.png" % phase)))


def run(app, directory, component, qt_bin=None, device_scale=False):
    directory = Path(directory).resolve()
    site = directory / "site"
    site.mkdir(parents=True, exist_ok=True)
    for name, following in (("page1.html", "page2.html"), ("page2.html", "page3.html"),
                            ("page3.html", "page3.html")):
        (site / name).write_text(page(following), encoding="utf-8")
    for name in ("phase1.done", "phase2.done", "server.log"):
        (directory / name).unlink(missing_ok=True)
    server = web.start_server(directory)
    port = server.server_address[1]
    failure = []
    try:
        script = directory / "scale.mitcad-ui"
        script.write_text(workflow(port, directory), encoding="utf-8")
        with tempfile.TemporaryDirectory(prefix="settings-", dir=directory) as temporary:
            environment = web.environment_for(directory, temporary, qt_bin, component)
            environment.update(MITCAD_TEST_INPUT=str(script), QT_SCALE_FACTOR=str(SCALE))
            log_path = directory / "scale.log"
            with log_path.open("wb") as log:
                app_run = subprocess.Popen([app, "--ui=qt", "--no-native-dialogs", "--no-recovery", "--no-mcp"],
                                           env=environment, stdout=log, stderr=subprocess.STDOUT)

                def outside():
                    try:
                        wait_log(log_path, "Component browser page: http://127.0.0.1:%d/page1.html" % port, 0, 120)
                        check_and_click(directory, log_path, 1, device_scale)
                        (directory / "phase1.done").touch()
                        page2 = wait_log(log_path, "Component browser page: http://127.0.0.1:%d/page2.html" % port,
                                         0, 120)
                        wait_log(log_path, "Component browser scale: 1.65", page2, 60)
                        check_and_click(directory, log_path, 2, device_scale)
                        (directory / "phase2.done").touch()
                    except Exception as error:  # reported after the run
                        failure.append(str(error))
                        app_run.kill()

                helper = threading.Thread(target=outside, daemon=True)
                helper.start()
                try:
                    app_run.wait(timeout=360)
                except subprocess.TimeoutExpired:
                    app_run.kill()
                    failure.append("the app did not end")
                helper.join(5)
            text = log_path.read_text(encoding="utf-8", errors="replace")
            if (failure or app_run.returncode or any(line.startswith("TestDriver: FAIL") for line in text.splitlines())
                    or "TestDriver: quit" not in text):
                raise RuntimeError("Component from Web at scale 1.65 failed: %s (%s)"
                                   % ("; ".join(failure) or "see the log", log_path))
    finally:
        server.shutdown()
    print("PASS: Component from Web at scale 1.65: the page fills the view and a click at the button's place on "
          "the screen clicks it, before and after the window's scale changes")


if __name__ == "__main__":
    arguments = sys.argv[1:]
    device_scale = "--device-scale" in arguments
    arguments = [argument for argument in arguments if argument != "--device-scale"]
    if len(arguments) < 3:
        raise SystemExit(__doc__)
    run(*arguments[:4], device_scale=device_scale)
