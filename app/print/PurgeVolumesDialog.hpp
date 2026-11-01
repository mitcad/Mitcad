// SPDX-License-Identifier: MIT
#pragma once

#include <QDialog>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>

class QLabel;
class QPushButton;
class QTableWidget;

namespace mitcad {

class DocumentHost;

// Purge Volumes (mitcad#705): the table of how much the printer purges
// when it changes filaments, for the filament slots of the bodies 3D Print
// sends (the model's `purge_volumes` query: a slot per material, in the
// order of first use). A row per filament changed from ("From PLA"), a
// column per filament changed to ("To PETG"), each header with the
// material's colour; each cell its volume in mm³, nothing from a filament
// to itself. A cell shows the computed volume (the materials' values and
// colours times the process' multiplier) until it is edited: an edited
// cell is the design's own for that pair of materials, shown bold on a
// tinted ground, its tooltip the computed volume. Reset gives the selected
// edited cells their computed volumes again. Each edit and each Reset is
// one model command and undo step (`set_purge_volumes`); the window
// follows the model. The cells are data, without a painting delegate
// (mitcad#1078): the AppKit frontend shows the same texts, the rows'
// labels as a leading column; a typed text commits through cellEdited in
// both. Their NativeControlIdentifierRole is `purgeVolumes.<row>.<column>`,
// a row label's `purgeVolumes.from.<row>`.
//
// Logs `Purge volumes opened: <materials>`, each pair as `Purge volume
// <from> → <to> = <n> mm³` with `(set by hand)` when edited, `Purge
// volume set: …`, `Purge volumes reset: <n>`, refusals `Purge volumes
// refused: <reason>`, and the places `Purge volumes cell <row>.<column>
// at x,y` (from 0), `Purge volumes reset at x,y`, `Purge volumes close at
// x,y`.
class PurgeVolumesDialog : public QDialog {
  Q_OBJECT

public:
  PurgeVolumesDialog(DocumentHost& host, QWidget* parent = nullptr);
  ~PurgeVolumesDialog() override;

  // The bodies printed (uids, in order), whose slots the table shows.
  void setBodies(const QStringList& bodies);
  // Shows the table as the model has it now.
  void refresh();

protected:
  void showEvent(QShowEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;

private:
  // A cell typed: the pair's own volume.
  void cellEdited(int row, int column);
  void resetSelected();
  void updateReset();
  // Runs `set_purge_volumes` with `volumes`; false when refused.
  bool run(const QJsonArray& volumes);
  void setMessage(const QString& message, bool error);
  QString slotName(int slot) const;
  QString slotMaterial(int slot) const;
  void fitGrid();
  void scheduleLogPlaces();
  void logPlaces();

  DocumentHost& m_host;
  QStringList m_bodies;
  QJsonObject m_table; // the `purge_volumes` query's answer
  QTableWidget* m_grid = nullptr;
  QLabel* m_message = nullptr;
  QPushButton* m_reset = nullptr;
  QPushButton* m_close = nullptr;
  QString m_loggedCells;
  QString m_loggedPlaces;
  bool m_filling = false;
};

} // namespace mitcad
