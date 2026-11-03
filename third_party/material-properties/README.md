# Material dataset

The material dataset Mitcad bundles (mitcad#1213, GitHub mitcad/Mitcad#120):
Browse material dataset… in the Materials manager searches it offline, and
a material picked from it is created as the user's own with its values
(`create_material`'s `dataset`, [commands.md](../../core/model-api/src/commands.md),
"Material properties"). The model compiles `materials.json` in
(`core/model/src/material_dataset.rs`) and reads it with the materials
import (`core/model/src/material_exchange.rs`), converting its units.

- Source: https://github.com/KittyCAD/material-properties, commit
  `61088092d5a90bcf4e0beee0c6a36783776a3a88` (8.10.2024).
- Licence: Apache-2.0 (`LICENSE`, copied from the repository, SHA-256 of its bytes in git
  `c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4`). The
  repository has no NOTICE file; `NOTICE` here gives the attribution and
  the changes (Apache-2.0, section 4) and ships with the packages beside
  the other licence texts.
- Licence policy ([development.md](../../docs/development.md#licence-policy)):
  Apache-2.0 is a permissive licence the policy allows; the data is used
  under it, kept here with its licence and notice, and compiled into the
  model as data. Nothing of the source repository is executed: its helper
  script was not run, and the conversion read only its JSON files.

## Files

| Here | From | SHA-256 of the source file |
|---|---|---|
| `materials.json` | `materials/aluminum.json` | `5f3ca080e45c09935da6ba4eb5d40f7a052d6d037e67c5a9630b29f7b91d3f5f` |
| | `materials/carbonsteel.json` | `5d8b4cb6fa2fa66a851bf0bf16c665aee4391e9cf1ec00afc8bd9f021d3fbcb7` |
| | `materials/castiron.json` | `3a8a36e8d9e701565e37c613a858bb74eb3a5011f24d0ac7678ef7baf1adec28` |
| | `materials/copper.json` | `9b34021e5e196b53b35c62474f54935b3c283fb085e1417b30137eb76e9802a9` |
| | `materials/lowalloysteel.json` | `da9d40932ecae3350ebca542cc69796848027952ab54134a0b023c6e257da859` |
| | `materials/nickelalloys.json` | `d490aa8d840b448fc98647a632b3c2f5b93d0e141a6ed4bb6149536abb0f1fc8` |
| | `materials/stainlesssteel.json` | `b221628eca16c2892178767cb6d365f3a3238ca5a5b7bdee5dbac936d1c379ff` |
| | `materials/titanium.json` | `0312c531e1cea6b9f7e7041d541e01e32b56960e96d0d0435c0fef860f436712` |
| | `materials/3dprinting.json` | `57b5fa147c1aedab40ff9688ca2773304c7b9d54a8b58527cc4b16f6bd9c5c83` |

## Changes

- One file in Mitcad's materials exchange format (format
  `mitcad-materials`, version 1, [material-files.md](../../docs/material-files.md)),
  in the dataset's own units, which the file states: psi, lb/in³, °F and
  µin/(in·°F). The model converts them when it reads the file.
- Each material is named after its file (Aluminum, Carbon steel, Cast iron,
  Low-alloy steel, Stainless steel, Titanium; none for the copper and nickel
  alloys and the printing materials, whose names say it), its material and
  its grade (none for `Default`), underscores as spaces; its id is the
  file, material and grade in lower case (`aluminum/2014/t6`). Its
  `reference` keeps the dataset's material and grade as they are written
  there and the commit.
- The modulus of elasticity is the Young's modulus, `pr` the Poisson's
  ratio, `yield_str` the yield strength, `max_service_temp` the maximum
  service temperature and `coef_thermal_exp` the coefficient of thermal
  expansion (µin/(in·°F), as the dataset's units table gives it).
- The printing materials' minimum and maximum extruder temperatures are the
  extruder temperature range (the print settings' lowest and highest
  recommended nozzle temperature); the midpoints of the extruder and the
  bed ranges are the extruder and the bed temperature. The metals are of the
  class Metal, the printing materials FDM Plastic.
- The ultimate strength and the elongation are left out: Mitcad's materials
  have no such properties.
