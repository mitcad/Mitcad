// SPDX-License-Identifier: MIT
#pragma once

#include <optional>

#include <QByteArray>
#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <QVector>

#include "../framework/Command.hpp"
#include "../framework/NoticeLevel.hpp"

namespace mitcad {

// What the browser, the timeline and the view show of the document's
// structure, read once per change from the model's queries (timeline,
// components, bodies, profiles, document).
struct DocumentSnapshot {
  struct Feature {
    QString uid;
    QString name;
    QString type;   // sketch, extrude, construction_plane, ...
    QString status; // ok, warning (ok with warnings, P9), error, suppressed, rolled_back, pending
    // The error, or the import note and the warnings: its notices' messages
    // (mitcad#1040).
    QString error;
    // Its level from the model (mitcad#1040): error, warning, info (an
    // import note only), or empty when it has nothing to say.
    QString level;
    // What the importer that made it said (mitcad#170): a fallback, an item
    // that came in partly, a placeholder of a skipped item; also in `error`.
    QString importNote;
    QString component; // C0 for the root
    int index = 0;
    // A sketch's degrees of freedom (P9: 0 is fully constrained); -1 when
    // it has no result or is no sketch.
    int dof = -1;
    bool suppressed = false;
    // A sketch's or construction feature's light bulb as the model decides
    // it (mitcad#7: a sketch is hidden while a feature uses it), and whether
    // the user set it (set_feature_visible) or it follows that default.
    bool visible = true;
    bool visibleSet = false;
    // The sketches it uses (profiles, paths, curves, points), in timeline
    // order: the browser lists it under each (mitcad#177).
    QStringList sketches;
    // The condition it is suppressed under (mitcad#800); empty: none.
    QString suppressedWhen;
    // How many of its warnings Auto Fix can write into its definition
    // (mitcad#1223: the timeline's `auto_fix`); 0: Auto Fix is not offered.
    int autoFix = 0;
    // How many of a sketch's linked projections Project Again repairs
    // (mitcad#1303: the timeline's `project_again`); 0: not offered.
    int projectAgain = 0;
    // The places its notices name (mitcad#1327: the faces of a fillet's
    // corner that folds, its farthest point), painted in the view while
    // the pointer is over the feature's notice; empty for none.
    QJsonArray places;

    // Whether it has something to tell as information: an import note, or
    // infos of a feature that built (mitcad#1106: a fillet built 0.1 %
    // smaller than asked), its level `info`.
    bool noted() const { return !importNote.isEmpty() || level == QStringLiteral("info"); }

    bool isSketch() const { return type == QStringLiteral("sketch"); }
    bool isConstruction() const { return type.startsWith(QStringLiteral("construction_")); }
    // Joints, as-built joints, joint origins and rigid groups (mitcad#55):
    // the browser's Joints folder.
    bool isJoint() const {
      return type == QStringLiteral("joint") || type == QStringLiteral("as_built_joint") ||
             type == QStringLiteral("joint_origin") || type == QStringLiteral("rigid_group") ||
             type == QStringLiteral("constraint_set");
    }
    // Motion links (mitcad#466): the browser's Motion Links folder.
    bool isMotionLink() const { return type == QStringLiteral("motion_link"); }
    // Evaluated without an error (perhaps with warnings).
    bool succeeded() const { return status == QStringLiteral("ok") || status == QStringLiteral("warning"); }
    // Its level for the UI (mitcad#1040): Error, Warning, Information, or
    // Hint when it has nothing to say.
    NoticeLevel noticeLevel() const { return noticeLevelFromModel(level, NoticeLevel::Hint); }
    // Before the marker and not suppressed: its result exists.
    bool isActive() const { return succeeded() || status == QStringLiteral("error"); }
  };
  struct Occurrence {
    QString uid;
    QString name;      // Component1:1
    QString path;      // occurrence uids from the root, "O1/O4"
    QString component; // C1
    bool grounded = false;
    bool visible = true;
    bool transparent = false; // drawn see-through with its contents (mitcad#1093)
    // Where it places its component in the design: 4 rows of 4 numbers.
    QJsonArray world;
    QVector<Occurrence> children;
    // The condition it is shown under (mitcad#800); empty: none.
    QString visibleWhen;
  };
  struct Component {
    QString uid;
    QString name;
    QStringList bodies; // at the marker
    bool linked = false;
    // Inserted from a library (its record, mitcad#64): it may have options
    // of its own (Edit Options…, mitcad#810).
    bool library = false;
    // A linked design shown by its envelope (mitcad#1271): its detail size
    // in mm; 0 when shown in full.
    double envelope = 0;
  };
  struct Body {
    QString uid;
    QString name;
    QString component;
    bool visible = true;
    bool transparent = false; // drawn see-through (mitcad#1093)
    bool mesh = false; // triangles without surfaces (mitcad#399)
    // The face that lies on the print bed (mitcad#731): its name as
    // assigned (empty: none), the faces it finds at the marker, and why it
    // cannot lie on the bed when it cannot.
    QString printBedFace;
    QStringList printBedFaces;
    QString printBedError;
    // Bindings (mitcad#800): the expressions its appearance, material and
    // visibility follow (empty: none), its faces' ({face: expression}),
    // and why one gives no value.
    QString appearanceExpr;
    QString materialExpr;
    QString visibleWhen;
    QJsonObject faceAppearanceExprs;
    QStringList warnings;
    bool bound() const {
      return !appearanceExpr.isEmpty() || !materialExpr.isEmpty() || !visibleWhen.isEmpty() ||
             !faceAppearanceExprs.isEmpty();
    }
  };

  QVector<Feature> features; // in timeline order
  // The features' places in `features` by uid, for feature(): the browser
  // and the view look up a feature per row, body and profile, which a
  // search through the timeline made quadratic in a large design (an
  // imported building's thousands of features, mitcad#833).
  QHash<QString, qsizetype> featureIndex;
  int marker = 0;
  // Timeline groups (P9): a name and its features, a run in timeline order.
  struct Group {
    QString name;
    QStringList features;
  };
  QVector<Group> groups;
  QVector<Component> components; // the root first
  QString activeComponent = QStringLiteral("C0");
  QVector<Occurrence> occurrences; // placed in the root
  QHash<QString, Body> bodies;
  QString lengthUnit = QStringLiteral("mm");
  // Shown sketches and construction features, by uid: the model's `visible`
  // (what the user set, else a sketch while no feature uses it and
  // construction geometry always).
  QHash<QString, bool> featureShown;
  // The document's named views (U5): the `named_views` query's entries.
  QJsonArray namedViews;
  // Analyses kept in the document (mitcad#41): the `analyses` query's
  // entries (name, type, visible, the definition, and the section's plane
  // at the marker or why it cannot be found).
  QJsonArray analyses;
  // Joints between occurrences (mitcad#55): the `joints` query's answer
  // (joints, rigid groups, degrees of freedom per component).
  QJsonObject joints;
  // Sheet-metal bodies (mitcad#210): the `sheet_bodies` query's entries
  // (body, rules and their name and values, Flat Pattern state).
  QJsonArray sheets;
  // Motion studies and contact sets (mitcad#454): the `motion_studies` and
  // `contact_sets` queries' entries.
  QJsonArray motionStudies;
  QJsonArray contactSets;
  // Measurement objects (mitcad#649): the `measurements` query's entries
  // (name, kind, colour, light bulb, picks, and their value and drawing at
  // the marker or why they cannot be measured there).
  QJsonArray measurements;
  // A measurement by name, or an empty object.
  QJsonObject measurement(const QString& name) const;
  // Drawings kept in the design (mitcad#707): the `drawings` query's
  // entries (drawing, name, component, sheets), and those of a component.
  QJsonArray drawings;
  QJsonArray drawingsOf(const QString& component) const;
  // Image references (mitcad#782): the `image_references` query's entries
  // (settings, pixels, size, digest, and the corners at the marker or why
  // the plane is not there), and one by name (an empty object when none).
  QJsonArray imageReferences;
  QJsonObject imageReference(const QString& name) const;
  // Saved in the document (P9): the root's Origin folder shown, and what
  // the view is isolated to (bodies and occurrences; none: everything).
  bool originShown = false;
  Selection isolation;
  // View › Show of the whole design (mitcad#1287): the kinds shown, from the
  // `document` query's display.show (everything while it has none). The
  // light bulbs (featureShown, originShown) stay as they are.
  bool showOrigins = true;
  bool showConstruction = true;
  bool showSketches = true;
  // Whether View › Show hides a feature's kind: a sketch, construction
  // geometry or a joint origin.
  bool kindHidden(const Feature& feature) const;
  // Whether View › Show hides the feature of this uid (false for none).
  bool kindHidden(const QString& feature) const;
  // A sketch or a construction feature the view draws: its light bulb on
  // and its kind shown.
  bool isDrawn(const QString& feature) const { return isShown(feature) && !kindHidden(feature); }
  // The Center of Mass marker (mitcad#1094): the `center_of_mass` query's
  // answer while the design has one (its `marker` with the light bulb;
  // `error` when the query failed), else empty. Read only while there is
  // a marker (view/CenterOfMass.hpp reads it).
  QJsonObject centerOfMass;
  // The values of the parameters that Show When conditions use
  // (mitcad#863), by name, as an expression writes them ('No', 12.5 mm):
  // the light bulbs' tooltips of conditioned rows. Read only while a body
  // or an occurrence has a condition.
  QHash<QString, QString> conditionValues;
  // The parameters a condition names, with their values ("Holes is 'No'"),
  // in the order it names them; those not in conditionValues left out.
  QStringList conditionParts(const QString& condition) const;
  // The answers the snapshot was made from: equal keys, equal snapshots.
  QByteArray key;

  static DocumentSnapshot read(const CommandContext& model);

  const Feature* feature(const QString& uid) const;
  const Component* component(const QString& uid) const;
  QString componentName(const QString& uid) const;
  // The first occurrence of a component ("" for the root), depth first:
  // the first one shown (with every occurrence above it), else the first.
  QString firstOccurrence(const QString& component) const;
  // Whether an occurrence path and every occurrence above it are shown.
  bool isOccurrenceShown(const QString& path) const;
  // The component an occurrence path leads to (the root's for ""), or
  // empty when the path leads nowhere.
  QString occurrenceComponent(const QString& path) const;
  bool isShown(const QString& feature) const { return featureShown.value(feature, false); }
  // Failed features before the marker.
  QVector<const Feature*> failures() const;
  // Features with warnings that did not fail (mitcad#170): those that built
  // with warnings and those an importer left a note on (a fallback's base
  // feature, an item that came in partly, a placeholder of a skipped item).
  QVector<const Feature*> warnings() const;
  // The analysis shown (one at a time), or an empty object.
  QJsonObject shownAnalysis() const;
  // An analysis by name, or an empty object.
  QJsonObject analysis(const QString& name) const;
  // A joint or as-built joint as the `joints` query has it, a constraint
  // set as one of the kind "constraint" (mitcad#148), or an empty object.
  QJsonObject joint(const QString& uid) const;
  // The degrees of freedom of a component with joints (the `joints`
  // query's `dof` entry), or an empty object.
  QJsonObject jointDof(const QString& component) const;
  // The degrees of freedom of an occurrence placed in a component with
  // joints (what it can do with the others held), or -1.
  int occurrenceDof(const QString& uid) const;
  // A sheet-metal body's entry of `sheets`, or an empty object.
  QJsonObject sheet(const QString& body) const;
  // A motion study or a contact set by name (mitcad#454), or an empty
  // object.
  QJsonObject motionStudy(const QString& name) const;
  QJsonObject contactSet(const QString& name) const;
};

// The icon (app/icons) of a feature type in the timeline and the browser.
QString featureIcon(const QString& type);

// The bodies at the marker with their volumes, by uid (the `bodies` query
// with volumes), and the log lines of what changed since (UI tests read
// "Body Body1 (F2.b0): volume a -> b mm3", "New body", "Removed body");
// only with MITCAD_LOG_VOLUMES (framework/Diagnostics.hpp), else empty.
QJsonObject bodyVolumes(const CommandContext& model);
void logVolumeChanges(const CommandContext& model, const QJsonObject& before);

} // namespace mitcad
