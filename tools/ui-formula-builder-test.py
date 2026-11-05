# SPDX-License-Identifier: MIT
# check-all sources: app/framework/FormulaEditor app/framework/ParameterField app/browser/ParametersDialog core/expr core/model-api/src/formula_help
"""Actual formula editor, model evaluation and limits in both Qt layouts."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    app, cli, directory = sys.argv[1:4]
    out = Path(directory).resolve()
    out.mkdir(parents=True, exist_ok=True)
    environment = dict(os.environ)
    if len(sys.argv) > 4:
        environment["PATH"] = sys.argv[4] + os.pathsep + environment.get("PATH", "")
    commands = [{"cmd": "add_parameter", "name": name, "expression": expression}
                for name, expression in (("length", "100 mm"), ("pitch", "8 mm"))]
    commands.append({"cmd": "add_parameter", "name": "result", "unit": "", "expression": "1"})
    model = out / "formula.json"
    model.write_text(json.dumps(commands), encoding="utf-8")
    design = out / "formula.mitcad"
    with (out / "setup.log").open("wb") as log:
        subprocess.run([cli, "run", str(model), "--save", str(design)], env=environment,
                       stdout=log, stderr=subprocess.STDOUT, check=True, timeout=90)
    for chrome in ("floating", "docked"):
        shots = out / chrome
        shots.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix=chrome + "-", dir=out) as temporary:
            env = dict(environment)
            for variable in ("SETTINGS", "AUTOSAVE", "PROJECTS", "THUMBNAIL", "CRASH"):
                env["MITCAD_" + variable + "_DIR"] = str(Path(temporary) / variable.lower())
            env.update(MITCAD_TEST_INPUT=str(Path(__file__).with_name("ui-scripts") / "formula-builder.mitcad-ui"),
                       MITCAD_TEST_SHOTS=str(shots), MITCAD_RESULT_STORE="off",
                       MITCAD_START_PAGE="0", MITCAD_NO_UPDATE_CHECK="1", QT_FORCE_STDERR_LOGGING="1")
            log_path = out / (chrome + ".log")
            with log_path.open("wb") as log:
                result = subprocess.run([app, "--ui=qt", "--chrome=" + chrome, "--no-recovery",
                                         "--no-mcp", "--open", str(design)], env=env,
                                        stdout=log, stderr=subprocess.STDOUT, timeout=180)
            text = log_path.read_text(encoding="utf-8", errors="replace")
            failures = [line for line in text.splitlines() if line.startswith("TestDriver: FAIL")]
            if result.returncode or failures or "TestDriver: done" not in text:
                raise RuntimeError(chrome + " formula builder failed; see " + str(log_path))
    print("PASS: formula signature, catalog, subexpression, template, limits and field entry in both layouts")


if __name__ == "__main__":
    main()
