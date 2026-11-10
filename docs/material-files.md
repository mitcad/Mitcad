# Materials as JSON and CSV files

Mitcad exports and imports materials as JSON and CSV files (mitcad#1213):
the Materials window's **Export…** and **Import…**, the commands
`export_materials` and `import_materials`
([commands.md](../core/model-api/src/commands.md), "Material properties").
The bundled material dataset is kept in the same JSON format
([third_party/material-properties](../third_party/material-properties/README.md)),
and so are MitCad's own woods and panels beside it
(`core/model/data/wood_materials.json`, [below](#the-woods-and-panels)).
The model reads and writes them in `core/model/src/material_exchange.rs`;
[examples/materials.json](examples/materials.json) and
[examples/materials.csv](examples/materials.csv) are example files (the
model's tests read them).

Both formats hold the same fields and **state their units**: a value is
read in the unit the file names and converted into the model's (g/cm³,
MPa, °C, µm/(m·°C)). An export writes the design's unit system (a metric
design MPa, g/cm³ and µm/(m·°C), an inch design psi, lb/in³ and
µin/(in·°F)) and temperatures in the unit the user prefers.

## Fields

| Field | Unit of | What it is |
|---|---|---|
| `id` | | The material's id where it was exported from. Written for people and tools; an import names materials by their names and makes ids of its own. |
| `name` | | The material's name (required). |
| `class` | | `metal`, `plastic` (General Plastic), `fdm_plastic`, `resin`, `wood` (mitcad#1552) or `other`. |
| `density` | density | Required for a material the import adds. |
| `youngs_modulus` | stress | Young's modulus, above 0. |
| `poissons_ratio` | | Poisson's ratio, above -1 and at most 0.5. |
| `yield_strength` | stress | The yield strength, above 0. |
| `max_service_temperature` | temperature | The maximum service temperature, above absolute zero. |
| `thermal_expansion` | expansion | The coefficient of thermal expansion. |
| `extruder_temperature` | temperature | The print settings' nozzle temperature (`temperature`). |
| `extruder_temperature_min`, `extruder_temperature_max` | temperature | The recommended nozzle temperature range (`nozzle_temperature_range_low`, `_high`), whole degrees. |
| `bed_temperature` | temperature | The print settings' bed temperature. |
| `source`, `material`, `grade`, `revision` | | Where the values came from: a dataset or a supplier's data sheet, its material, grade and the data's revision (the material's `reference`). A file that names a material, grade or revision names its source too. |

A value that is not given is left out (JSON) or empty (CSV), never 0.
The printing values are the material's print settings, the same values as
the Materials window's Filament tab.

The units each quantity may be written in (any case, the symbols with
`µ`, `·`, `°` and `³` too):

| Quantity | Units |
|---|---|
| density | `g/cm3`, `kg/m3`, `lb/in3` |
| stress | `MPa`, `N/mm2`, `GPa`, `psi`, `ksi` |
| temperature | `C`, `F` |
| expansion | `um/(m*C)`, `um/(m*K)`, `1e-6/K`, `uin/(in*F)`, `1e-6/F` |

## JSON

An object with `"format": "mitcad-materials"`, its `version` (1),
`units` (the unit of each quantity the file's materials use) and
`materials`, a list of objects with the fields above; the source, material,
grade and revision as one object, `reference`:

```json
{
  "format": "mitcad-materials",
  "version": 1,
  "units": {"stress": "MPa", "density": "g/cm3", "temperature": "C", "expansion": "um/(m*C)"},
  "materials": [
    {"id": "custom1", "name": "Shop steel", "class": "metal", "density": 7.85,
     "youngs_modulus": 210000, "poissons_ratio": 0.3, "yield_strength": 235,
     "max_service_temperature": 400, "thermal_expansion": 12,
     "reference": {"source": "Mill sheet", "material": "S235JR", "grade": "JR"}},
    {"id": "custom2", "name": "Shop PLA", "class": "fdm_plastic", "density": 1.24,
     "max_service_temperature": 52, "thermal_expansion": 68,
     "extruder_temperature": 210, "extruder_temperature_min": 190, "extruder_temperature_max": 230,
     "bed_temperature": 60}
  ]
}
```

A newer version than the reader knows, another `format`, a field a
material does not have or a quantity's value without its unit is refused:
the file as a whole when it cannot be read at all, otherwise the material.

## CSV

A header of the fields above in any order, the name required; a column of
a quantity names its unit in brackets, `density [g/cm3]`, and is refused
without one. Commas or semicolons separate the fields (as Mitcad's other
CSV files), quotes as RFC 4180; one material per line:

```csv
id,name,class,density [g/cm3],youngs_modulus [MPa],poissons_ratio,yield_strength [MPa],max_service_temperature [C],thermal_expansion [um/(m*C)],extruder_temperature [C],extruder_temperature_min [C],extruder_temperature_max [C],bed_temperature [C],source,material,grade,revision
custom1,Shop steel,metal,7.85,210000,0.3,235,400,12,,,,,Mill sheet,S235JR,JR,
custom2,Shop PLA,fdm_plastic,1.24,,,,52,68,210,190,230,60,,,,
```

The same two materials in an inch design, temperatures in °F, as the
export writes them (12 significant digits):

```csv
name,density [lb/in3],youngs_modulus [psi],yield_strength [psi],max_service_temperature [F],thermal_expansion [uin/(in*F)]
Shop steel,0.283599242201,30457924.9233,34083.8683666,752,6.66666666667
```

## Import

Each material is checked on its own: one that is not valid (a value that
is not a number, out of its range, a quantity without its unit, no name, a
new material without a density) is reported with its line (CSV) or its
place in the list (JSON, from 1), its name and the reason, and the others
are imported. A material whose name is there already (the design's own
and the built-in ones it shows, or the library's items) is decided once
for the file: keep the one that is there, replace its values with the
file's (the values the file does not give stay; a built-in one gets a
material of the design in its place) or keep both (the file's numbered,
`Steel (2)`). The import is one undo step, "Import Materials".

## The woods and panels

MitCad's own part of the bundled dataset (mitcad#1552, GitHub
mitcad/Mitcad#211): 10 solid woods and 7 wood-based panels in the class
Wood, `core/model/data/wood_materials.json`, MIT like MitCad's code,
compiled into the model beside the third-party dataset
(`core/model/src/material_dataset.rs`). The entries came from a user's
contribution; each value was checked against the public source its
`reference` names (9.11.2026) before it went in, and a value the source
does not give is left out (no Young's modulus for the panels, no other
property for any of them). The values are typical: the application says
so, to be checked against the supplier's data.

| Id | Name | Density | Young's modulus | Source |
|---|---|---|---|---|
| `panel/chipboard` | Chipboard | 611 kg/m³ | | OEKOBAUDAT, "Particleboard, uncoated" (Sonae Arauco, EPD-SON-20160209-IBA1-EN, dataset `184c72f0-002d-42d3-895f-4f2b0f7e7de5`, version 00.02.000): gross density |
| `panel/mdf_raw` | MDF - raw | 736 kg/m³ | | OEKOBAUDAT, "Medium Density Fibreboards EGGER MDF" (EPD-EGG-20200250-IBC3-DE, dataset `d539acfd-9bcf-439d-9c38-18223eac7def`, version 00.07.000): gross density, uncoated |
| `panel/osb3` | OSB/3 | 610 kg/m³ | | EGGER OSB 3 JAS TOP brochure, technical data: mean density of a 15 mm board (±20 kg/m³) |
| `panel/plywood_birch` | Plywood - birch | 680 kg/m³ | | WISA, sizes, thicknesses and weights (wisa.com): birch plywood, ±50 kg/m³ |
| `panel/plywood_eucalyptus` | Plywood - eucalyptus | 600 kg/m³ | | S&J Woods (sj-woods.co), Eucalyptus Throughout Hardwood Plywood: average density, approx. |
| `panel/plywood_pine` | Plywood - pine | 500 kg/m³ | | Plytech, Teco Radiata Structural data sheet (plytech.co.nz): average density |
| `panel/plywood_spruce` | Plywood - spruce | 460 kg/m³ | | WISA, sizes, thicknesses and weights (wisa.com): spruce plywood, ±50 kg/m³ |
| `wood/birch` | Wood - Birch | 690 kg/m³ | 13.86 GPa | The Wood Database (wood-database.com), yellow birch (Betula alleghaniensis) |
| `wood/cedar` | Wood - Cedar | 370 kg/m³ | 7.66 GPa | The Wood Database, western red cedar (Thuja plicata) |
| `wood/cherry` | Wood - Cherry | 560 kg/m³ | 10.3 GPa | The Wood Database, black cherry (Prunus serotina) |
| `wood/douglas_fir` | Wood - Douglas Fir | 510 kg/m³ | 12.17 GPa | The Wood Database, Douglas fir (Pseudotsuga menziesii) |
| `wood/eucalyptus` | Wood - Eucalyptus | 820 kg/m³ | 18.76 GPa | The Wood Database, blue gum (Eucalyptus globulus) |
| `wood/maple` | Wood - Maple | 705 kg/m³ | 12.62 GPa | The Wood Database, hard maple (Acer saccharum) |
| `wood/oak` | Wood - Oak | 755 kg/m³ | 12.15 GPa | The Wood Database, American white oak (Quercus alba) |
| `wood/pine` | Wood - Pine | 550 kg/m³ | 10.08 GPa | The Wood Database, Scots pine (Pinus sylvestris) |
| `wood/teak` | Wood - Teak | 655 kg/m³ | 12.28 GPa | The Wood Database, teak (Tectona grandis) |
| `wood/walnut` | Wood - Walnut | 610 kg/m³ | 11.59 GPa | The Wood Database, black walnut (Juglans nigra) |

The Wood Database gives a wood's average dried weight and elastic modulus
at 12 % moisture content. The contribution named the particleboard
"Chipboard P5"; its source is an average of uncoated particleboard of all
use classes, not of class P5, so the entry is named Chipboard.

Each entry has a look of its own in the appearance library, colours only
(no textures): `panel_chipboard`, `panel_mdf`, `panel_osb`,
`plywood_birch`, `plywood_eucalyptus`, `plywood_pine`, `plywood_spruce`,
`wood_birch`, `wood_cedar`, `wood_cherry`, `wood_douglas_fir`,
`wood_eucalyptus`, `wood_maple`, `wood_oak`, `wood_pine`, `wood_teak`,
`wood_walnut`; all of the class Wood.

**Licence policy** ([development.md](development.md#licence-policy)): the
file holds single published figures, a density and a modulus per material,
with the source each comes from; no text, table layout or image of a source
is copied. The two OEKOBAUDAT datasets state their licence type as free
of charge for all users and uses, without access or use restrictions, and
OEKOBAUDAT allows its data to be passed on with the source credited, which
each entry's `reference` does. The manufacturers' figures (EGGER, WISA,
S&J Woods, Plytech) are the published properties of their products, cited
as such. The Wood Database reserves the rights to its pages and gives no
licence for its data; what the file takes from it is two measured physical
values per species (facts, not its expression), credited, not its text,
pictures or arrangement.
