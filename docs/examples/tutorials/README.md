<!-- SPDX-License-Identifier: MIT -->
# MitCad tutorials

Short lessons for [MitCad](https://github.com/mitcad/Mitcad), shown beside
the design in MitCad's tutorial pane. Each step points at what to use
next, and MitCad holds back other input until the step is done.

This repository is MitCad's default tutorials source: MitCad shows its
tutorials as cards on its Start page (Tutorials, or Help › Tutorials…) and
installs the one you start.

## Layout

```
mitcad-tutorials.json      the repository's manifest
tutorials/
  sketch-extrude/          one folder per tutorial, named by its id
    tutorial.json          the tutorial's manifest
    index.html             its first page
    images/result.png      the picture of its result on its card
tools/check-tutorials.py   checks every manifest (run by the CI)
```

The format of the manifests, the pages, the bridge `window.mitcad` and
the guide steps is described in MitCad's
[docs/tutorials.md](https://github.com/mitcad/Mitcad/blob/main/docs/tutorials.md).

## Contributing a tutorial

1. Fork this repository and add a folder `tutorials/<id>/`, with lower
   case letters, digits and hyphens in the id. Put `tutorial.json` and
   the pages in it; `tutorials/sketch-extrude/` is a small example to
   start from.
2. Name the steps' targets by MitCad's command ids and input keys, never
   by places on the screen.
3. Try it: in MitCad, the Start page's Tutorials › Sources › Add Source…
   with your clone's folder (commit your changes first: MitCad reads the
   committed version), then start your tutorial from its card and take
   every step to its end.
4. Check the manifests: `python3 tools/check-tutorials.py` (Python 3.9 or
   newer, nothing to install).
5. Open a pull request. The CI runs the same check.

A tutorial's text and media carry the licence its `tutorial.json` names
(an SPDX id, such as `CC0-1.0` or `CC-BY-4.0`). The repository's own
files, such as the check, are under the MIT licence in [LICENSE](LICENSE).
