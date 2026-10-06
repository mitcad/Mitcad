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
    QString error;  // the error, or the warnings
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

    bool isSketch() const { return type == QStringLiteral("sketch"); }
    bool isConstruction() const { return type.startsWith(QStringLiteral("construction_")); }
    // Evaluated without an error (perhaps with warnings).
    bool succeeded() const { return status == QStringLiteral("ok") || status == QStringLiteral("warning"); }
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
    // Where it places its component in the design: 4 rows of 4 numbers.
    QJsonArray world;
    QVector<Occurrence> children;
  };
  struct Component {
    QString uid;
    QString name;
    QStringList bodies; // at the marker
    bool linked = false;
  };
  struct Body {
    QString uid;
    QString name;
    QString component;
    bool visible = true;
  };

  QVector<Feature> features; // in timeline order
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
  // Saved in the document (P9): the root's Origin folder shown, and what
  // the view is isolated to (bodies and occurrences; none: everything).
  bool originShown = false;
  Selection isolation;
  // The answers the snapshot was made from: equal keys, equal snapshots.
  QByteArray key;

  static DocumentSnapshot read(const CommandContext& model);

  const Feature* feature(const QString& uid) const;
  const Component* component(const QString& uid) const;
  QString componentName(const QString& uid) const;
  // The first occurrence of a component ("" for the root), depth first.
  QString firstOccurrence(const QString& component) const;
  // Whether an occurrence path and every occurrence above it are shown.
  bool isOccurrenceShown(const QString& path) const;
  bool isShown(const QString& feature) const { return featureShown.value(feature, false); }
  // Failed features before the marker.
  QVector<const Feature*> failures() const;
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
