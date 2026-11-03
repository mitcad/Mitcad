# Materials as JSON and CSV files

Mitcad exports and imports materials as JSON and CSV files (mitcad#1213):
the Materials window's **Export…** and **Import…**, the commands
`export_materials` and `import_materials`
([commands.md](../core/model-api/src/commands.md), "Material properties").
The bundled material dataset is kept in the same JSON format
([third_party/material-properties](../third_party/material-properties/README.md)).
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
| `class` | | `metal`, `plastic` (General Plastic), `fdm_plastic`, `resin` or `other`. |
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
