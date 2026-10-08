# Component libraries

Mitcad's component libraries (fasteners and other standard parts) and the
community library are plain git repositories. No server is needed: a
library is published by pushing it to any git host (or a shared folder),
and a community index is just another repository that lists libraries.
This page documents the formats, so that anyone can make and publish a
library; the application's side is in the
[user guide](user-guide.md#component-libraries) and the commands in
[commands.md](../core/model/src/api/commands.md#component-libraries).

## A library repository

```
my-parts/                        a git repository, also a Mitcad project
  mitcad-library.json            the manifest (required)
  LICENSE                        the licence's text or a note naming it
  README.md
  SOURCES.md                     where data such as dimension tables came from
  .mitcad/project.json           Mitcad's project marker
  screws/iso4762.mitcad          the components' designs
  previews/iso4762.png           preview images (PNG, at most 512 KB)
```

The designs are ordinary Mitcad project files. A design may have a
configuration table (below) so that one design stands for a family of
sizes. A library holds data only: nothing in it is run, symbolic links
are refused, and paths stay inside the repository.

### `mitcad-library.json`

```json
{
  "format": "mitcad-library",
  "version": 1,
  "id": "mitcad-fasteners",
  "name": "Mitcad metric fasteners",
  "description": "ISO metric screws, nuts and washers, M3 to M12.",
  "library_version": "1.0.0",
  "license": "CC0-1.0",
  "authors": ["Mitcad contributors"],
  "homepage": "",
  "sources": ["ISO 4762", "ISO 4032", "Tabulated for this library; see SOURCES.md"],
  "units": "mm",
  "categories": [{"id": "screws", "name": "Screws"}, {"id": "nuts", "name": "Nuts"}],
  "components": [
    {"id": "iso4762", "path": "screws/iso4762.mitcad", "category": "screws",
     "name": "Hexagon socket head cap screw", "standard": "ISO 4762",
     "keywords": ["socket head", "cap screw", "DIN 912"],
     "preview": "previews/iso4762.png", "designation": "ISO 4762 {Size}x{Length}",
     "description": "36 sizes, M3x5 to M12x120."}
  ]
}
```

| Field | Meaning |
|---|---|
| `format`, `version` | `mitcad-library`, 1 |
| `id` | 2 to 64 of `a-z`, `0-9` and `-`; the same for every version |
| `name`, `description` | for people |
| `library_version` | the library's own version, matching its tag (`v1.0.0`) |
| `license` | an SPDX licence expression for every component without its own |
| `authors`, `homepage`, `sources` | for the attribution and the data's origin |
| `units` | `mm` |
| `categories` | `id` and `name` of the groups components belong to |
| `components` | `id`, `path` (a `.mitcad` file), `name` (required); `category`, `standard`, `description`, `keywords` (or `tags`), `preview` (a PNG), `license` (instead of the library's), `designation` |

`designation` is how a bill of materials names a part: `{standard}`,
`{name}`, `{config}` and the selectors' names in braces (`{Size}`) are
replaced; without it `{standard} {config}`. A configuration row's own
`designation` comes first.

Unknown fields are ignored, so newer libraries stay readable.

### Configuration tables

A design's top-level `configurations` (the `set_configurations` command
writes it):

```json
"configurations": {
  "selectors": ["Size", "Length"],
  "parameters": ["d", "dk", "k", "s", "t", "L"],
  "default": "M5x16",
  "rows": [
    {"name": "M5x16", "select": {"Size": "M5", "Length": "16"},
     "values": {"d": "5 mm", "dk": "8.5 mm", "k": "5 mm", "s": "4 mm", "t": "2.5 mm", "L": "16 mm"}}
  ]
}
```

Each row sets the expressions of the table's parameters (parameters of the
design, usually user parameters that its features use). Selectors are the
cascaded choices the application shows (Size, then Length); each row's
combination is unique. Applying a row only changes parameter
expressions, as Change Parameters does.

### Versions

A library's versions are its commits; released ones are tags
(`v1.0.0`). Mitcad lists the tags, newest first, and the default branch's
tip as "latest, unreleased". A design records, for each part, the commit
of the version the user chose; opening the design reads that commit even
after the library has moved on, and only Library Parts › Update changes
it. Keep old tags: designs refer to their commits.

## A community index

```
community-index/
  mitcad-index.json              {"format": "mitcad-index", "version": 1, "name": "..."}
  libraries/<id>.json            one entry per library
```

```json
{
  "format": "mitcad-index-entry", "version": 1,
  "id": "my-parts", "name": "My parts", "description": "...",
  "url": "https://example.org/you/my-parts.git",
  "homepage": "", "license": "CC-BY-4.0", "maintainers": ["You"],
  "tags": ["brackets"],
  "components": [{"id": "bracket-20", "name": "Bracket 20", "tags": ["corner"]}],
  "reviewed": [{"rev": "<40 hex digits>", "label": "v1.0.0", "date": "2026-10-05"}]
}
```

`components` lets a library be found before it is fetched; `reviewed`
lists the versions the index's maintainers looked at. An index only
lists: the libraries stay where their authors keep them, and an index's
checks (if it has any) only check entries. `mitcad-cli library
index-entry <folder> --url <url>` writes an entry for a library.

## Licences

Every item needs a licence; items without one are hidden unless the user
asks for them. A community index accepts these (not legal advice):

| SPDX | What users should know |
|---|---|
| CC0-1.0 | no conditions |
| CC-BY-4.0 | credit the authors when sharing designs that contain it |
| CC-BY-SA-4.0 | credit the authors; designs shared with it may need the same licence |
| MIT, BSD-2-Clause, BSD-3-Clause | keep the copyright and licence notice |
| Apache-2.0 | keep the copyright, licence and notice files |
| CERN-OHL-P-2.0, -W-2.0, -S-2.0 | permissive, weakly and strongly reciprocal hardware licences |

The licence, the library, its URL and the version are recorded with each
part in the design (also for a copied part), and Library Parts › Parts
List shows them.

## Making and publishing a library

In the application: Tools › Libraries › Publish to Library adds the open
design to a library folder of your own (made when new: its manifest, a
licence note and a git repository), with an image of the view as its
preview, records it as a version, pushes it to a remote you give, and
writes the entry for a community index. From the command line:

```
mitcad-cli library init my-parts --id my-parts --name "My parts" --license CC-BY-4.0 --author "You"
mitcad-cli library add my-parts bracket.mitcad --id bracket-20 --name "Bracket 20" --category brackets \
    --preview bracket.png
mitcad-cli library check my-parts
mitcad-cli version save my-parts/mitcad-library.json my-parts/brackets/bracket-20.mitcad ...
git -C my-parts tag v1.0.0
mitcad-cli remote add my-parts https://example.org/you/my-parts.git
mitcad-cli library index-entry my-parts --url https://example.org/you/my-parts.git
```

## Mitcad's fastener library

`tools/libraries/make-fastener-library.py` builds the metric fastener
library from the standards' dimension tables with `mitcad-cli`: ISO 4762,
ISO 4017 and ISO 10642 screws, ISO 4032 nuts and ISO 7089 washers, M3 to
M12 in their preferred lengths, simplified for assemblies (no threads,
chamfers or fillets), with preview images drawn from the dimensions, the
manifest, a CC0-1.0 note and `SOURCES.md`; `--commit` records and tags
it. The library itself is kept in a repository of its own.

## Where libraries are kept

Fetched libraries are bare clones in the user's local data folder
(`libraries`; `MITCAD_LIBRARIES_DIR` overrides it), one per URL, made and
updated with the system's git only when the user asks (Libraries ›
Fetch, Library Parts › Check for Newer Versions, Get Library). Updates
add branches and tags without removing anything, and git's automatic
garbage collection is off there, so the versions designs use stay
readable. A design whose library is not on the computer keeps the bodies
saved with it and says so; Library Parts › Get Missing Libraries fetches
them after showing their addresses.
