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
  entries. The `.ts` files are the source of truth for translations; nothing
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

   This writes `app/translations/mitcad_de.ts` with every text unfinished
   and the line `de <count>` in `languages.txt`.
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
  `tr()` is wrapped in it.
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
- **The model's messages** (Rust: errors, warnings, the timeline's feature
  names) come through the JSON API in English; translating them is a later
  phase of mitcad#1284.

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
that Qt's catalogue lacks (its file dialog's, its key names) are left out.
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
| feature | piirre | |
| sketch | luonnos | Create Sketch: Luo luonnos; Finish Sketch: Lopeta luonnos |
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
| configuration | konfiguraatio | option: vaihtoehto; row: rivi; rule: sääntö; binding: sidonta |
| table | taulukko | key column: avainsarake; parameter column: parametrisarake |
| extrude | pursotus (pursottaa) | |
| revolve | pyöräytys (pyöräyttää) | |
| sweep | pyyhkäisy | |
| loft | siirtymäpiirre | |
| fillet | pyöristys (pyöristää) | |
| chamfer | viiste (viistää) | |
| shell | kuorinta | |
| draft | päästö | taper angle: päästökulma |
| hole | reikä | |
| thread | kierre | |
| pattern | kuvio | rectangular: suorakulmainen kuvio; circular: ympyräkuvio |
| mirror | peilaus | |
| offset | siirtymä | Offset Face: Siirrä tahkoa |
| split | jako (jakaa) | |
| combine | yhdistä | join: liitä; cut: leikkaa |
| assembly | kokoonpano | Assemble group: Kokoonpano |
| joint | liitos | |
| sheet metal | ohutlevy | flat pattern: levitys; bend: taivutus |
| drawing | piirustus | |
| view (a drawing's) | kuvanto | base view: peruskuvanto; projected: projektiokuvanto; section: leikkauskuvanto; detail: detaljikuvanto; break: katkaisu; parent view: emokuvanto; the 3D view: näkymä |
| sheet (a drawing's) | arkki | template: pohja; title block: nimiö; scale: mittakaava; first/third angle: ensimmäinen/kolmas kulma |
| drawing symbols | symbolit | balloon: osanumero; center mark: keskiömerkki; centerline: keskiviiva; datum label: perustemerkki; feature control frame: toleranssikehys; surface finish: pinnanlaatu; hatching: viivoitus; hole table: reikätaulukko |
| drawing dimensions | mitoitus | ordinate: koordinaattimitoitus; baseline: rinnakkaismitoitus; chain: ketjumitoitus; aligned: suuntainen |
| annotation | merkintä | |
| note | huomautus | |
| geometric tolerance | geometrinen toleranssi | straightness: suoruus; flatness: tasomaisuus; circularity: ympyrämäisyys; cylindricity: lieriömäisyys; profile of a line, of a surface: viivan muoto, pinnan muoto; parallelism: yhdensuuntaisuus; perpendicularity: kohtisuoruus; angularity: kaltevuus; concentricity: samankeskisyys; runout: heitto; datum: peruste; fit: sovite; deviation: poikkeama; leader: viiteviiva |
| comment | kommentti | open: avoin; addressed (by an agent): käsitelty; resolved: ratkaistu; resolution: ratkaisu; retarget: siirtää; pin: nasta |
| bill of materials | osaluettelo | |
| cut list | leikkuuluettelo | cut sheet: leikkuuarkki |
| stock (cut list) | aihio | stock length: aihion pituus; offcut: hukkapala; kerf: sahausrako; running meters: juoksumetrit; grade (timber): lujuusluokka; finish: pintakäsittely; part number: osanumero |
| opening (Build) | aukko | sill: alareuna; partition: väliseinä; rough opening: karkea aukko; top plate, bottom plate: yläjuoksu, alajuoksu |
| material | materiaali | classes: Metal: metalli; Resin: hartsi; Wood: puu |
| material properties | materiaaliominaisuudet | material dataset: materiaaliaineisto; Young's modulus: kimmokerroin; Poisson's ratio: Poissonin luku; yield strength: myötöraja; a material's variant: muunnelma |
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
| formula | kaava | Formula Editor: Kaavaeditori; formula template: kaavamalli |
| expression | lauseke | |
| parent (component) | yläkomponentti | |
| body transfer | kappalesiirto | |
| hybrid assembly | hybridikokoonpano | |
| snap | kohdistaa | snapped: kohdistettu |
| preview | esikatselu | |
| pick | poimia | |
| select | valita | selection: valinta |
| navigation | navigointi | pan: panorointi; orbit: kierto; zoom: zoomaus; left, middle, right drag: vasen veto, keskiveto, oikea veto; snapshot: tilannekuva |
| render environment | renderöintiympäristö | light: valo; point, spot, area light: pistevalo, kohdevalo, pintavalo; sun: aurinko; ground: maa; exposure: valotus; samples: näytteet; denoise: kohinanpoisto; scene: kohtaus; render worker: renderöintityöprosessi |
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
| member (Build) | rakenneosa | timber: puutavara; stud: tolppa; plate: juoksu; joist: palkki; rafter: kattokannattaja; header: ylityspalkki; trimmer: pielitolppa |
| connector | liitin | junction: liitoskohta |
| fastener | kiinnike | wood screw: puuruuvi; nail: naula; set screw: pidätysruuvi |
| miter | jiiri | butt: puskuliitos |
| frame (coordinates) | kehys | mount frame: kiinnityskehys |
| joint motion | kierto, liuku | Turn Z: Kierto Z; Slide Z: Liuku Z; rest: lepoarvo |
| driver, follower | käyttävä, seuraaja | |
| gear | hammaspyörä | module: moduuli; pressure angle: rynnökulma; backlash: hammasvälys; profile shift: profiilinsiirto |
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
| key names | Shift, Ctrl, Alt, Enter | as on the keyboard, never translated (not Vaihto) |
| navigation bar, toolbar | navigointipalkki, työkalurivi | Pin to Navigation Bar: Kiinnitä navigointipalkkiin; Unpin from Navigation Bar: Irrota navigointipalkista; Pin to Toolbar: Kiinnitä työkaluriville |
