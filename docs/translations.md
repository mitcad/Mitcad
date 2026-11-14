# Interface languages

MitCad's interface is written in English, the source language of every
text, and can show another language: Finnish today, more later
(mitcad#1284, mitcad#1460). This page is for those who build, translate or
add a language. Users choose the language in Preferences › General ›
Language ([user guide](user-guide.md#language)).

## How it works

- **Texts.** Every user-facing text in `app/` goes through Qt's `tr()`,
  `QCoreApplication::translate()` or `QT_TRANSLATE_NOOP()`. The ribbon's
  tabs, groups and menu subgroups are named by ids in code ("SHEET METAL",
  "Machine Elements"); their shown names are the context `RibbonName`
  (`displayName` in `app/framework/Ribbon.cpp`).
- **Catalogues.** Each language has a Qt Linguist file
  `app/translations/mitcad_<code>.ts` and a line in
  `app/translations/languages.txt`: its code and its baseline of unfinished
  entries; its `model_<code>.ts` holds the model's messages (below). The `.ts` files are the source of truth for translations; nothing
  else holds translated text.
- **Build.** `cmake/Translations.cmake` reads the list, prepares each `.ts`
  (`tools/translations`, `prepare`: a context-free copy of every translated
  message is added, see below) and compiles it with Qt's `lrelease`
  (finished entries only) into `<build>/translations/mitcad_<code>.qm`,
  beside Qt's own `qtbase_<code>.qm` (Qt's standard dialogs and buttons,
  from Qt's translations module, BSD-3-Clause) where Qt has one. A build
  tree copies them into `translations/` beside `mitcad`; the packages carry
  them: `bin/translations` in the Windows installer and zip,
  `usr/share/mitcad/translations` in the AppImage,
  `Contents/Resources/translations` in the macOS bundle.
- **Start-up.** `app/framework/Language.cpp` chooses the language before
  any window is made: `--language <code>`, else the environment variable
  `MITCAD_LANGUAGE`, else the preference (`interface/language`), else
  Automatic: the system's first UI language when MitCad has it, otherwise
  English. A code without a catalogue is English. It installs MitCad's
  catalogue and Qt's, and logs `Language: fi (--language)` (or
  `(MITCAD_LANGUAGE)`, `(preference)`, `(automatic: the system's first
  language is fi-FI)`). A change of the preference applies at the next
  start.
- **English fallback.** A text the catalogue has not translated shows in
  English, so a partly translated language works.
- **Contexts.** Qt looks a text up by its context (the class of `tr()`)
  and its source. Where the context at run time is not the one `lupdate`
  saw in the source (a class without `Q_OBJECT` whose `tr()` is its base
  class's, a file's own `tr()` helper with a fixed context), the text is
  still found: each compiled catalogue has a context-free copy of its
  translations (`MitCad.AnyContext`), which MitCad asks after Qt's own
  catalogue. Key names (`QShortcut`, `QKeySequence`) never take it. Where
  the same source has different translations in different contexts, a
  call that names its context explicitly
  (`QCoreApplication::translate("MaterialTool", "Other")`) is filed under
  that context and finds its own. Code outside a `Q_OBJECT` class that
  translates many texts declares a class of its own with the context it
  wants and calls its `tr()` by name, which `lupdate` follows
  (mitcad#1472):

  ```cpp
  struct Text {
    Q_DECLARE_TR_FUNCTIONS(DrawingTools)
  };
  const QString name = Text::tr("Hole Table");
  ```

  Not a free function named `tr()` with a fixed context: `lupdate` files
  its texts under the enclosing namespace, and they are found only through
  the context-free copy. A class without `Q_OBJECT` whose own code calls
  `tr()` (a `QDialog` or `QWidget` of a single file) declares the context
  `lupdate` gives it, its name with its namespaces
  (`Q_DECLARE_TR_FUNCTIONS(mitcad::CommentsWindow)`), or its `tr()` is its
  Qt base class's (mitcad#1473). A text kept in a table and translated
  later is marked `QT_TRANSLATE_NOOP("<context>", "…")` with the context
  the later call looks it up in.
- **Fixed choices.** The labels of a command's fixed choices (a panel's
  drop-down or buttons) are given in English in the command definitions
  and translated as the panel shows them, in the context `Choice`; each is
  listed for the catalogues in `kChoiceNames`
  (`app/framework/CommandPanel.cpp`). A label the definition translated
  already (its `tr()`) is shown as it is.
- **English names.** MitCad remembers the English source of each text the
  catalogue translated (`englishText`): the command search finds a command
  by its shown name and by its English name.
- **Tests.** The UI test libraries (`tools/ui-test-lib.sh`,
  `tools/ui-windows-lib.ps1`, `tools/ui-macos-lib.sh`) and the Python UI
  tests set `MITCAD_LANGUAGE=en`: the tests read English texts whatever the
  machine's language. `tools/ui-language-test.sh` and
  `app.ui-windows-language` start MitCad in English, in Finnish and in the
  pseudo-language. `tools/ui-language-walk-test.sh` walks the Finnish
  interface (below).

## Adding a language

1. Start its file and its line in the list (German, `de`, as the example;
   a code such as `pt_BR` for a regional variant):

   ```bash
   cmake -DLUPDATE=<Qt>/bin/lupdate -DTOOL=<build>/tools/translations/mitcad-translations \
         -DWORK=<build>/translations-work -DADD_LANGUAGE=de -P tools/translations-update.cmake
   ```

   This writes `app/translations/mitcad_de.ts` and `model_de.ts` (the
   model's messages, [below](#the-models-messages)) with every text
   unfinished and the line `de <count>` in `languages.txt`.
2. Translate (below). The first message to translate is the language's own
   name: the context `LanguageName`, source `English`, translated as the
   language names itself (`Deutsch`). Preferences lists the language by it.
3. Add a column to the [glossary](#glossary) and keep to it.
4. Build: the catalogue is compiled, installed and listed in Preferences
   with no change of code. Check it with `--language de`, and the layouts
   with the pseudo-language (below).

## Updating and translating

- **New texts.** After code adds or changes texts, refresh the files:
  `cmake --build <build> --target update_translations`
  (`tools/translations-update.cmake`: `lupdate` over `app/` without its
  tests and the paused AppKit frontend, its Objective-C++ files of the
  shared UI included (the macOS render client), `-no-obsolete`, no source
  locations, then the new counts written into `languages.txt`). New texts
  come in unfinished and show in English until translated.
- **Translating.** Open the `.ts` file in Qt Linguist, or edit the XML: fill
  `<translation>` and remove `type="unfinished"` from an entry that is
  done. Only finished entries are compiled.
- **Keep:** placeholders (`%1`, `%L2`; `repo.translations` fails on a
  translation whose placeholders differ from the source's), the accelerator
  (one `&` before a letter when the source has one; `&&` is a literal
  ampersand), plural forms (a message with `%n` has the language's forms,
  `<numerusform>`; a form may say the number in words), units and their
  symbols (`mm`, `°`, `m³`), key names and shortcuts (`Ctrl+S`, `Esc`,
  `Tab`, `Enter`), product and format names (MitCad, STEP, DXF, 3MF), rich
  text tags (`<b>`, `<br>`) and file filters' patterns (`(*.stl *.obj)`).
- **Not translated:** log lines (`qDebug`, `qInfo`, the texts tests read),
  JSON keys and values of the API, command ids (`solid.extrude`),
  file-format keys, settings keys, file names, formulas, parameter names,
  names users give, the test driver's texts, and texts written with
  `QStringLiteral` for these reasons. A user-facing text found without
  `tr()` is wrapped in it. Names stored in a design are its data and keep
  the language they were made in: the default names of new items (Body1,
  Sketch1, Component1:1) and the names of materials and appearances, which
  the built-in library gives in English and a design keeps as it got them
  (mitcad#1674). The built-in materials' names are the exception
  (mitcad#1689): they are stored in English, in the design, its files,
  exports and the MCP results, and shown translated (the glossary's
  *built-in materials*), in the context `MaterialName`
  (`shownMaterialName` in `app/framework/Appearances.cpp`): in the
  material lists, the Material and Appearance tools' pickers, `Default
  (%1)`, the Browser's Material menu and Send to Slicer's material rows. A
  name the user gave, in the design or a library of the user's or the
  project's, is shown as it is, also when it reads as a built-in one's;
  appearances' names stay as stored.
- **Texts an agent reads** stay English in every language
  (`QStringLiteral`, mitcad#1471): the MCP tools' refusals, busy answers
  and errors (`MainWindowMcp.cpp`, `MainWindowMcpUi.cpp`, the path checks
  of `mcp/AgentFolders.cpp`, `mcp/LiveServer.cpp`) and the chat's deny
  reasons returned as tool results (`agent/ChatController.cpp`). A text an
  assistant's application may show to the user is translated: the refusal
  of a blocked assistant (`assistantRefusal`) and the listener's failure
  in the agent indicator's menu.
- **Words put into sentences.** A log line names a pick by its English id
  (`SelectionItem::kindName`: `face`, `edges`; the column names of the
  Parameters and Configurations logs likewise), a user-facing text by its
  translated word (`SelectionItem::kindText`, `countText` and
  `summarizeText`, the context `SelectionKind`: alone in the singular and
  plural, and after a count, where Finnish takes the partitive, `%1
  tahkoa`). A translation that puts such a word at the start of a sentence
  or needs its case rephrases around it ("Valittu %1 kuuluu…") rather than
  bracketing it. A list joined in code gets its conjunction from a whole
  translated text (`walls %1 and %2`), never a joined `" and "`.
- **The model's messages** (Rust, `core/`: refusals, errors, warnings,
  notes) are translated as they are shown: [below](#the-models-messages).
- **English plurals** (mitcad#1756). A text with a count is written once,
  `tr("%1: %n degree(s) of freedom", nullptr, dof)`; English has its own
  catalogue of those texts only, `app/translations/mitcad_en.ts` (listed in
  `languages.txt` as `en`, refreshed with `lupdate -pluralonly`), whose two
  forms the English interface shows: "1 degree of freedom", "17 degrees of
  freedom", a verb or pronoun after the count agreeing ("1 waiting message
  was not sent"). Every language asks it last, so a count another language
  has not translated yet is still good English. It is no language to
  choose (`availableLanguages` lists English once). A new `%n` text comes
  into `mitcad_en.ts` unfinished at the next `update_translations`;
  `repo.translations` then asks for its two forms, and refuses a form that
  keeps a bracketed plural ("face(s)") or an entry that is no plural. A
  count built outside `tr()`, `.arg(count)` after a "(s)", shows the
  brackets in every language: give it `%n` (`repo.ui-strings` refuses
  it, mitcad#1758; a text with two counts is made of parts, each with
  its own `%n`).

## The model's messages

The model (Rust, `core/`) writes its refusals of commands, the errors and
warnings of features, import notes, project and version errors and its
other texts for the user in English, and the JSON API, MCP and the command
line keep them English: agents and scripts read them. The application
translates what it shows the user, as it shows it (mitcad#1284).

- **Ids.** A message's id is its *template*: the English text of the Rust
  source with its values as `%1`, `%2`, ... (`format!("feature {uid} does
  not exist")` is `feature %1 does not exist`), as Qt names a text by its
  English source. `mitcad-translations messages` (`tools/translations`)
  finds them: every string literal of `core/`'s Rust sources with two words
  side by side, outside comments, test modules (`#[cfg(test)] mod`, test
  files and crates), the arguments of panics, assertions, `expect` and log
  macros, and the texts only agents and the command line read (`mcp`,
  `chat.rs`, `cli.rs`, `bin/`). They are listed in
  `app/framework/ModelMessageTemplates.cpp` (generated: 10090 templates
  when this was written), which lupdate reads into the context
  `ModelMessage` of each language's `app/translations/model_<code>.ts`. That
  file is compiled with `mitcad_<code>.ts` into one catalogue, is checked by
  `repo.translations` with it and counts in its baseline; it is apart only
  because one file would pass the size the repository takes.
- **On display.** `modelMessage` (`app/framework/ModelMessages.hpp`)
  matches a message to its template (the whole message; of several that
  match, the one with the most fixed text), takes its values, translates
  the template and puts the values back: names, numbers with units and
  references stay as they are, and a value that is a message of its own
  (`%1: %2`, `the geometry kernel does not support %1`) is translated in
  turn. A message that is no template is translated line by line and after
  a lead-in of the application's that ends in `: `. A template the
  catalogue has not translated, and a text of no template, stay English.
  English shows the model's texts untouched. Where it is applied: the
  status line and the message band, a command panel's message, the warning
  and error boxes, the browser's and the timeline's feature errors and
  notices, and wherever the application keeps a model's text to show it:
  a refusal caught from a command or query (`modelMessage(e.what())`), the
  host's `lastError()` as a dialog or panel shows it, and the `error`,
  `warning`, `reason`, `problem` and `note` texts and `warnings` of the
  model's answers (imports, exports, versions, projects and live updates,
  libraries, configurations and tables, drawings, BUILD and the machine
  elements). The texts the application matches against the model's English
  (the print library's known refusals, the binding warnings' prefixes, a
  linked component's refusal) are matched first and translated after. A
  log line written where only the English is at hand keeps it; one written
  from the shown text shows the language.
- **Cleaned forms** (mitcad#1635). A message that names topology, a body's
  uid or an internal id is shown without them (mitcad#1327, #1612;
  `plain_message` and `FeatureError::from_text` in
  `core/model-base/src/notice.rs`): `line c4 does not exist` is shown as
  `a line does not exist`, `body F2.b0 (from Extrude1) belongs to …` as `a
  body (from Extrude1) belongs to …`. So `mitcad-translations messages`
  lists beside each template its cleaned form, made as that cleaner makes
  it: a value after the word of a kind (`line c%1`, `body %1`, a list
  `faces %1 and %2`) is taken out with the word kept and "a" or "an"
  before a single one unless its noun phrase has a word like "the", "its"
  or a possessive (mitcad#1638: `the driven dimension %1` is shown as `the
  driven dimension`, `the result's face %1` as `the result's face`); without
  one the article goes before the phrase's modifiers (`meets boundary curve
  %1`: `meets a boundary curve`, `Bend edge %1`: `A bend edge`), or before
  the word of the kind after a word a verb follows (`does not fit face %1`:
  `does not fit a face`). An id elsewhere (`c%1`, `p%1`) becomes its kind
  ("a curve", "points"); a value followed by ` mm` is a length and stays.
  A value whose format argument names a kind of topology (`{face}`,
  `{vertex}`, `{seed_edge}`) where no word of a kind is before it is listed
  with that kind put in as the cleaner shows such a name (`plane {face}: …`
  gives `plane a face: …`, mitcad#1638), so it is translated whole and
  not as `plane %1` with English words in its value; a positional `{}` does
  not say its kind, so such a message names its argument when it can hold
  a topological name (mitcad#1650). A value that is a list of such names
  is named so on purpose, `<kind>_names` (`{region_names}`,
  `{split_face_names}`: `split into {region_names}` gives `split into
  regions`); a plural alone (`{edges} edges`) is a count and stays a value,
  as does a value followed by its own kind's word. One that may be a name of
  any kind, a face, an edge or a vertex (a projection's source), is named
  `{topo}` or `*_topo` and listed once with each kind (`the projection of
  {topo} …` gives `the projection of a face …`, `… an edge …`, `… a
  vertex …`). The values left are numbered again (`a body (from %1)
  belongs to %2`). The cleaned forms are
  translated as whole messages like any other template; some of them are
  never shown (a value after such a word that is no name, a line number),
  and their translations only keep the list complete. The cleaner's test
  `cleaned_templates_are_listed` checks that the list has every cleaned
  form the cleaner itself makes.
- **New and changed messages.** A message added or changed in `core/` makes
  the ctest `repo.model-messages` fail until the list is written again:
  `cmake --build <build> --target update_translations` writes
  `ModelMessageTemplates.cpp` (`mitcad-translations messages <source root>
  --write`) and then the `.ts` files, where the new templates come in
  unfinished, to be translated as any other text. A message keeps its
  translation as long as its template's text stays the same.
- **Translating them.** As the interface's texts (above). Keep every `%N`
  and never give one a case ending: a value is a name, a number or another
  message, so a translation puts a noun before it ("piirrettä %1", "arvo
  %1") or a colon after it. A template that is a fragment put after another
  text (" and along their sides") stays a fragment with its leading space.
  A template whose value is an English word ending (`%1 occurrence%2`)
  cannot be translated well; such messages are worded in `core/` without
  it as they are touched. The same holds for a value that is an English
  word or phrase of no template of its own (`the curve is a %1, not a line`
  with `spline`, `along the %1` with `x axis`, `a curve ends %1` with
  `at`) and for a message joined of pieces (`format!("{}{said}", …)`),
  which matches only a general template such as `%1 is not %2`: `core/`
  writes one whole message per case, with only names, numbers and
  messages of their own as values (`format!("{what} is not a plane")`,
  one literal per curve kind; mitcad#1626). A lead-in of one word and a
  value (`format!("section {}: {e}", …)`) is no template of its own (a
  template has two words side by side), so it would stay English before a
  translated rest; where such a message is common, its lead-in is part of
  each whole message (`section %1: an edge found again after a change in
  %2`, a loft's renamed references; mitcad#1688). A count is no `%n`
  here: the template cannot give the catalogues a count, so a count
  before a bracketed plural (`{n} key(s) of the new scan`) would show the
  brackets in every language. `core/` writes one whole message for one
  and for several (`a key of the new scan was not shown`, `{n} keys of
  the new scan were not shown`; zero gets its own where it is shown) or
  puts the count after a colon (`files written: {}`), and `mitcad-translations
  messages` (`repo.model-messages`) refuses a template with a count before
  a word with "(s)", "(es)" or "(ies)", up to two words between and none
  of them a determiner or a word that ends a noun phrase (mitcad#1759).
- **Tests.** `test_app`'s model message tests check the matching, the
  values and the English fallback, and that every template has its Finnish
  and its pseudo-language translation. The pseudo-language marks the
  model's messages too, so a model message left English shows without
  brackets; `MITCAD_LOG_UNTRANSLATED=1` logs each shown text that stayed
  English (`Untranslated model message: <text>`; the application's own
  texts that pass the same displays may show there too). The language UI
  tests show the model's refusal of a feature that does not exist in
  English, Finnish and the pseudo-language.

## The check

The ctest `repo.translations` (`tools/translations`, `check`) reads every
language of `languages.txt`: its `.ts` file is there and every
`mitcad_*.ts` is listed; no obsolete or vanished entries; each finished
translation keeps its source's placeholders and accelerator; and its count
of unfinished entries is its baseline. A count above the baseline fails
(new texts came in without the refresh writing the count); a count below
it fails too and names the lower baseline to write: the baseline only goes
down as the language is completed, to 0. `repo.translations-self-test`
checks the scanner on planted catalogues.

## The pseudo-language

A test build (`BUILD_TESTING`) also makes `mitcad_xx.qm` from the first
language's file (`tools/translations`, `pseudo`): every text as its source
with accented letters, a third longer and in brackets
(`Extrude` → `[Éxtrúdé ··]`), placeholders, accelerators, markup and a
trailing ellipsis or colon kept. It is never installed. Start MitCad with
`--language xx`: a text without brackets did not go through a catalogue (a
text without `tr()`, or one built from pieces), a cut label or a fixed
width shows where a longer language would not fit. `MITCAD_LOG_UNTRANSLATED=1`
logs each text no catalogue translated (`Untranslated: <context>: <text>`)
and each text found only through the context-free copy
(`Fallback: <context>: <text>`: its context at run time is not the one
`lupdate` recorded), in any language. `tools/ui-language-test.sh` checks that the tabs show the
pseudo-language and that no group caption of the ribbon is cut.

## The walk

`tools/ui-language-walk-test.sh` starts MitCad in Finnish with
`MITCAD_LOG_UNTRANSLATED=1` and walks its interface
(`tools/ui-scripts/language-walk.mitcad-ui`, the test steps of
`app/framework/TestDriverWalk.cpp`): every menu filled as when it opens
(`walk-menus`), the context menu of each row of the Browser and the other
visible lists (`walk-context-menus`), every command opened as its button
would and closed again, its dialogs and windows closed and its panel
cancelled (`walk-commands`), then the sketch's commands in a sketch and a
drawing's in a drawing. It fails on each `Untranslated:` or `Fallback:`
line of a text of MitCad's, naming the step that showed it; Qt's own texts
that Qt's catalogue lacks (its key names) are left out, but not the labels of
Qt's file dialog (Look in, Files of type, File name), which Qt's Finnish
catalogue has: one left English means `qtbase_fi.qm` was not loaded
(mitcad#1674).
A text it reports is marked for `lupdate` in its context and translated
(above), or, a text for logs or agents, taken out of `tr()`. The walk
reaches what an empty design with a body, a sketch and a drawing shows;
the panels of a feature being edited, the dialogs a command opens only on
a pick and the texts of errors are not walked.

## Contributions

The `.ts` files are plain Qt Linguist files: a translation platform that
reads them (Weblate, for example) can be connected to the repository later
without a change of the application (mitcad#1284, item 5). The glossary
below is the reference for its translators too.

## Glossary

One term per concept; a column per language. Finnish follows established
Finnish CAD and engineering usage.

Finnish style (mitcad#1475): a "Click to …" text (and "Double-click to",
"Press … to", "Drag … to", "Hold … to") says the result first and the
action as *-malla*, not with *-kseen* or *-ksesi*: "Click the first point
to close" is "sulje napsauttamalla ensimmäistä pistettä". What *napsauta*
clicks is in the partitive: "napsauta keskipistettä", "napsauta toista
päätä", never "napsauta keskipiste".

| English | Finnish (fi) | Note |
|---|---|---|
| design | malli | the document; a new design: uusi malli |
| project | projekti | |
| Cloud project, Local project | pilviprojekti, paikallinen projekti | |
| version | versio | record a version: kirjata versio; Version History: Versiohistoria |
| repository | repositorio | remote: etärepositorio |
| sync | synkronoida | Sync: Synkronoi; a sync: synkronointi |
| edit lock | muokkauslukko | read-only: vain luku |
| live updates | reaaliaikapäivitykset | broker: välityspalvelin |
| component | komponentti | |
| occurrence | esiintymä | |
| body | kappale | solid body: solidi; surface body: pintakappale; mesh body: verkkokappale |
| mesh | verkko | never verkkomalli (malli is a design); Convert Mesh: Muunna verkko; Save as Mesh: Tallenna verkkona |
| feature | piirre | |
| sketch | luonnos | Create Sketch: Luo luonnos; Finish Sketch: Lopeta luonnos |
| 3D sketch | 3D-luonnos | Create 3D Sketch: Luo 3D-luonnos; Convert to 3D Sketch: Muunna 3D-luonnokseksi; 3D Sketch Palette: 3D-luonnospaletti; route (a 3D sketch's path): reitti |
| relation (3D sketch) | suhde | a 3D sketch's constraint; Relations: Suhteet; In plane: Tasossa; in-plane relation, Fix, dimension: tasosuhde, tasokiinnitys, tasomitta; Align X: Suuntaa X; Keep Axis, Keep Plane: Pidä akselilla, Pidä tasossa |
| On Sketch Plane | Luonnostasolla | the relation; Release from Plane: Vapauta tasosta; Release Plane: Vapauta taso |
| attach, detach (3D sketch) | linkittää, irrottaa | Attach: Linkitä; Detach: Irrota; linked reference: linkitetty viittaus; source: lähde; Replace Reference: Korvaa viittaus; reference curve: viitekäyrä; On Face (a point on a face): Tahkolla |
| handle (3D sketch) | kahva | tangent handle: tangenttikahva; anchor: ankkuri; Pick Normal: Poimi normaali; seen edge-on: näkyy syrjittäin |
| wall (grown from sketch curves) | seinämä | thin walls, ribs, webs; sketch plane: luonnostaso; Thin Extrude: Ohut pursotus |
| profile | profiili | |
| curve | käyrä | |
| line | viiva | |
| arc | kaari | |
| circle | ympyrä | |
| rectangle | suorakulmio | |
| slot | ura | |
| spline | splini | |
| point | piste | |
| vertex | kärkipiste | |
| edge | särmä | |
| face | tahko | a body's face |
| surface | pinta | the surface of a face; Surface group: Pinta |
| plane | taso | construction plane: aputaso |
| axis | akseli | construction axis: apuakseli |
| construction geometry | apugeometria | Construct group: Apugeometria |
| constraint | rajoite | |
| dimension | mitta | Sketch Dimension: Luonnosmitta |
| parameter | parametri | user parameter: käyttäjäparametri |
| configuration | konfiguraatio | option: vaihtoehto; row: rivi; rule: sääntö; binding: sidonta (a configuration's only; input bindings are näppäinmääritykset) |
| table | taulukko | key column: avainsarake; parameter column: parametrisarake |
| extrude | pursotus (pursottaa) | |
| revolve | pyöräytys (pyöräyttää) | |
| sweep | pyyhkäisy | |
| loft | siirtymäpiirre | |
| fillet | pyöristys (pyöristää) | |
| chamfer | viiste (viistää) | |
| shell | kuorinta | |
| draft | päästö | taper angle: päästökulma |
| hole | reikä | clearance hole: välysreikä; tapped: kierteitetty; press fit: puristussovite |
| thread | kierre | |
| pattern | kuvio | rectangular: suorakulmainen kuvio; circular: ympyräkuvio |
| mirror | peilaus | |
| offset | siirtymä | Offset Face: Siirrä tahkoa |
| split | jako (jakaa) | |
| combine | yhdistä | join: liitä; cut: leikkaa |
| assembly | kokoonpano | Assemble group: Kokoonpano |
| joint | liitos | |
| ground, unground (an occurrence) | kiinnittää, vapauttaa kiinnitys | Ground: Kiinnitä; Unground: Vapauta kiinnitys; grounded: kiinnitetty; never maadoittaa (electrical earthing); the render environment's ground: maa |
| sheet metal | ohutlevy | flat pattern: levitys; bend: taivutus |
| drawing | piirustus | |
| view (a drawing's) | kuvanto | base view: peruskuvanto; projected: projektiokuvanto; section: leikkauskuvanto; detail: detaljikuvanto; break: katkaisu; parent view: emokuvanto; the 3D view: näkymä |
| orientation cube | suuntakuutio | its faces, in capitals: ETU, TAKA, VASEN, OIKEA, YLÄ, ALA (FRONT, BACK, LEFT, RIGHT, TOP, BOTTOM) |
| standard view | näkymä + direction | Front View: Näkymä edestä; Back View: Näkymä takaa; Left View, Right View: Näkymä vasemmalta, Näkymä oikealta; Top View, Bottom View: Näkymä ylhäältä, Näkymä alhaalta; Isometric View: Isometrinen näkymä; each a whole text, never a name put into "%1 View" (mitcad#1687) |
| sheet (a drawing's) | arkki | template: pohja; title block: nimiö; scale: mittakaava; first/third angle: ensimmäinen/kolmas kulma |
| drawing symbols | symbolit | balloon: osanumero; center mark: keskiömerkki; centerline: keskiviiva; datum label: perustemerkki; feature control frame: toleranssikehys; surface finish: pinnanlaatu; hatching: viivoitus; hole table: reikätaulukko |
| drawing dimensions | mitoitus | ordinate: koordinaattimitoitus; baseline: rinnakkaismitoitus; chain: ketjumitoitus; aligned: suuntainen |
| annotation | merkintä | |
| note | huomautus | |
| geometric tolerance | geometrinen toleranssi | straightness: suoruus; flatness: tasomaisuus; circularity: ympyrämäisyys; cylindricity: lieriömäisyys; profile of a line, of a surface: viivan muoto, pinnan muoto; parallelism: yhdensuuntaisuus; perpendicularity: kohtisuoruus; angularity: kaltevuus; concentricity: samankeskisyys; runout: heitto; datum: peruste; fit: sovite; deviation: poikkeama; leader: viiteviiva; characteristic (GD&T): toleranssityyppi |
| comment | kommentti | open: avoin; addressed (by an agent): käsitelty; resolved: ratkaistu; resolution: ratkaisu; retarget: siirtää; pin: nasta |
| bill of materials | osaluettelo | |
| cut list | leikkuuluettelo | cut sheet: leikkuuarkki |
| stock (cut list) | aihio | stock length: aihion pituus; offcut: hukkapala; kerf: sahausrako; running meters: juoksumetrit; grade (timber): lujuusluokka; finish: pintakäsittely; part number: osanumero |
| opening (Build) | aukko | sill: alareuna; partition: väliseinä; rough opening: karkea aukko; top plate, bottom plate: yläjuoksu, alajuoksu |
| material | materiaali | classes: Metal: metalli; Resin: hartsi; Wood: puu |
| material properties | materiaaliominaisuudet | material dataset: materiaaliaineisto; Young's modulus: kimmokerroin; Poisson's ratio: Poissonin luku; yield strength: myötöraja; a material's variant: muunnelma |
| built-in materials (their names) | sisäänrakennetut materiaalit | Steel: Teräs; Stainless Steel: Ruostumaton teräs; Cast Iron: Valurauta; Aluminum: Alumiini; Copper: Kupari; Brass: Messinki; Titanium: Titaani; ABS Plastic: ABS-muovi; PLA Plastic: PLA-muovi; Nylon: Nailon; Polycarbonate: Polykarbonaatti; Glass: Lasi; Water: Vesi; Resin - Standard: Hartsi - vakio; the base library's PA (Nylon): PA (nailon); ABS, ASA, PETG, PLA and TPU as they are; shown translated, stored in English (mitcad#1689) |
| appearance | ulkoasu | |
| texture | tekstuuri | base color: perusväri; roughness: karheus; opacity: peittävyys; emission: hehku |
| property set | ominaisuusjoukko | property type: ominaisuustyyppi |
| design policy | mallin käytäntö | violation: rikkomus; sketch attachment: luonnosten kiinnitys |
| timeline | aikajana | |
| Browser | Selain | the panel |
| Preferences | Asetukset | |
| command | komento | |
| command panel | komentopaneeli | float the panel: irrota paneeli; dock it: telakoi paneeli |
| AI assistant | tekoälyavustaja | AI Assistants (Preferences page): Tekoälyavustajat |
| agent | agentti | custom agent: oma agentti; custom command: oma komento |
| chat, conversation | keskustelu | New Conversation: Uusi keskustelu; chat history: keskusteluhistoria |
| model (an AI's) | kielimalli | never malli, which is a design |
| sign in | kirjautua | sign-in: kirjautuminen; signed in: kirjautunut |
| API key | API-avain | |
| configuration (an assistant's) | määritys | a design's configuration: konfiguraatio |
| extension (an assistant's) | laajennus | |
| listener | kuuntelija | MitCad's listener for assistants |
| cache | välimuisti | memory cache: muistivälimuisti; disk cache: levyvälimuisti |
| formula | kaava | Formula Editor: Kaavaeditori; formula template: kaavapohja (malli is a design) |
| expression | lauseke | |
| parent (component) | yläkomponentti | |
| body transfer | kappalesiirto | |
| hybrid assembly | hybridikokoonpano | |
| snap | kohdistaa | snapped: kohdistettu |
| preview | esikatselu | |
| pick | poimia | |
| select | valita | selection: valinta |
| navigation | navigointi | pan: panorointi; orbit: kierto; zoom: zoomaus; left, middle, right drag: vasen veto, keskiveto, oikea veto; middle button: keskipainike; trackpad: ohjauslevy; snapshot: tilannekuva |
| input bindings | näppäinmääritykset | navigation bindings: navigoinnin näppäinmääritykset; modifier keys: muokkausnäppäimet; a configuration's binding: sidonta |
| typed (a value) | syötetty | a typed value: syötetty arvo; the verb stays: type the radius: kirjoita säde |
| exit code, exit status | paluukoodi | |
| web viewer (a project's) | verkkokatselin | Update Web Viewer: Päivitä verkkokatselin |
| layer (DXF, DWG, 3D print) | kerros | never taso, which is a plane; layer height: kerroskorkeus |
| view cube, orientation cube | suuntakuutio | its faces: ETU, TAKA, VASEN, OIKEA, YLÄ, ALA |
| root component | pääkomponentti | the root's default name, Root: Juuri |
| render environment | renderöintiympäristö | light: valo; point, spot, area light: pistevalo, kohdevalo, pintavalo; sun: aurinko; ground: maa; exposure: valotus; samples: näytteet; denoise: kohinanpoisto; scene: kohtaus; render worker: renderöintiprosessi |
| motion study | liiketutkimus | key: avainkohta; step: askel |
| update | päivitys | release: julkaisu; pre-release: esijulkaisu; release notes: julkaisutiedot; channel: kanava |
| report (to the developers) | raportti | feedback: palaute; bug report: virheraportti; issue form: ilmoituslomake; debug log: vianetsintäloki |
| render | renderöidä | |
| print (3D) | 3D-tulostus | slicer: viipaloija; print bed: tulostusalusta |
| printer, process (3D print) | tulostin, prosessi | plate: alusta; print value: tulostusarvo; nozzle: suutin; filament: filamentti; purge volumes: huuhtelutilavuudet |
| preset (a slicer's) | esiasetus | filament preset: filamenttiasetus; variant: muunnelma; built-in: sisäänrakennettu; My library: Oma kirjasto |
| print (paper) | tulostaa | page: sivu; fit to page: sovita sivulle; portrait/landscape: pysty/vaaka |
| tolerance | toleranssi | |
| export | viedä | Export…: Vie…; import: tuoda |
| stored bodies | tallennetut kappaleet | an imported file's; base feature: peruspiirre; replay: toistaa |
| recover | palauttaa | autosave: automaattinen tallennus; discard: hylätä |
| library | kirjasto | library component: kirjastokomponentti; supplier: toimittaja |
| Start, Solid, Build, Make | Aloitus, Solidi, Rakenna, Valmistus | the tabs |
| Create, Modify, Inspect, Insert, Select | Luo, Muokkaa, Tarkastele, Lisää, Valitse | the groups |
| Auto | Auto | an input's automatic value: Auto (%1) |
| Cancel | Peruuta | every Cancel button, Qt's own dialogs too (Qt's catalogue says Peru) |
| member (Build) | rakenneosa | timber: puutavara; stud: pylväs; plate: juoksu; joist: palkki; rafter: kattokannattaja; header: ylityspalkki; trimmer: pielipylväs |
| connector | liitin | junction: liitoskohta |
| fastener | kiinnike | wood screw: puuruuvi; nail: naula; set screw: pidätysruuvi |
| miter | jiiri | butt: puskuliitos |
| frame (coordinates) | kehys | mount frame: kiinnityskehys |
| joint motion | kierto, liuku | Turn Z: Kierto Z; Slide Z: Liuku Z; rest: lepoarvo |
| driver, follower | käyttävä, seuraaja | |
| Machine Elements | Koneenosat | the Create menu's group of gears, pulleys and bearings; plainer than koneenelimet (mitcad#1687) |
| gear | hammaspyörä | module: moduuli; pressure angle: ryntökulma; mesh (gears): ryntö; backlash: hammasvälys; profile shift: profiilinsiirto |
| pulley | hihnapyörä | belt: hihna; travel: liikevara |
| bearing | laakeri | rolling element: vierintäelin |
| bore, hub, keyway | akselireikä, napa, kiilaura | |
| mortise and tenon | kolo ja tappi | the command: Tappiliitos |
| equal spacing | tasajako | |
| rail | kisko | guide rail: ohjauskisko |
| section (loft) | poikkileikkaus | |
| relief (sheet metal) | vapautusleikkaus | bend relief: taivutuksen vapautusleikkaus |
| hem | reunataitos | |
| K-factor | K-kerroin | |
| stitch, unstitch | ompele, pura ompelu | |
| patch | paikkaus | |
| thicken | paksunna | |
| boss, rib (plastic) | torni, ripa | |
| Anaglyph 3D | Anaglyfi-3D | stereo depth: stereosyvyys; convergence: konvergenssi |
| suppressed (a feature) | vaimennettu | |
| pattern instance | kopio | not esiintymä, an occurrence |
| helix, coil | kierukka | |
| replay (an import's history) | toisto | translated (an imported item): muunnettu; not translated: ei muunneta; decoded: purettu |
| counterbore, countersink, counterdrill | lieriöupotus, kartioupotus, porausupotus | |
| heat-set insert | lämpöupotettava kierreholkki | its hole: holkin reikä |
| nut pocket | mutteritasku | width across flats: avainväli |
| print bridging (of a hole) | tulostussiltaus | bridged: siltattu |
| stair run, riser, tread | syöksy, nousu, askelma | landing: tasanne |
| prime tower (3D print) | esitäyttötorni | brim: reunus; skirt: helma; raft: lautta; ironing: silitys; retraction: takaisinveto |
| key names | Shift, Ctrl, Alt, Enter | as on the keyboard, never translated (not Vaihto) |
| navigation bar, toolbar | navigointipalkki, työkalurivi | Pin to Navigation Bar: Kiinnitä navigointipalkkiin; Unpin from Navigation Bar: Irrota navigointipalkista; Pin to Toolbar: Kiinnitä työkaluriville |
| setup (CAM) | kiinnitys | New Setup: Uusi kiinnitys; never asetus, which is a setting |
| operation (CAM) | työstövaihe | Facing: Tasaus; Contour: Ääriviiva; Pocket: Tasku; Drill: Poraus; Adaptive Clearing: Mukautuva rouhinta; Rest Machining: Jäännöstyöstö; contour (the operation) in a sentence: ääriviivatyöstö |
| toolpath | työstörata | Generate: Laske; Simulate: Simuloi; Post Process…: Jälkikäsittele…; the program: ohjelma |
| stock (CAM) | aihio | stock body: aihiokappale; margin: vara; stock to leave: jätettävä työvara |
| work zero, work offset | nollapiste, työkoordinaatisto | probe the work zero: mittaa nollapiste anturilla; G54–G59 as they are |
| machine (CNC) | kone | controller: ohjain; travel: liikealue; tool changer: työkalunvaihtaja; router: jyrsinkone; rapid: pikaliike; machine library: konekirjasto |
| tool (CAM) | työkalu | flat end mill: varsijyrsin; ball end mill: pallopääjyrsin; bull nose end mill: nurkkapyöristetty varsijyrsin; drill: pora; chamfer mill: viistejyrsin; engraving cutter: kaiverrusterä; cutter: terä; tool library: työkalukirjasto |
| flute length | leikkuupituus | flutes: terät; stick-out: ulkonema; corner radius: nurkkasäde; point angle: kärkikulma; shank: varsi; holder: pidin; collet: kiristysholkki; collet nut: kiristysmutteri |
| spindle | kara | spindle speed: karan nopeus; coolant: jäähdytys; flood: tulvajäähdytys; mist: sumu; air blast: ilmapuhallus |
| feed | syöttö | plunge feed: pistosyöttö; feeds and speeds: syötöt ja nopeudet; climb, conventional: myötäjyrsintä, vastajyrsintä |
| heights, depths (CAM) | korkeudet, syvyydet | clearance height: siirtokorkeus; retract height: vetäytymiskorkeus; safe height: turvakorkeus; step down: askelsyvyys; stepover: sivuaskel |
| entry (CAM) | sisääntulo | plunge: pisto; ramp: ramppi; helix: heliksi (a helix curve is kierukka); lead in and out: sisään- ja ulosajokaaret; engagement: kosketuskulma |
| holding tab | pidike | peck drilling: askelporaus; canned cycle: ohjaimen porauskierto |
