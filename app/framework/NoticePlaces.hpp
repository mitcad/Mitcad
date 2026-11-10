// SPDX-License-Identifier: MIT
#pragma once

// The places of the model's notices (mitcad#1327, docs/ui-conventions.md
// "Messages and severity"): a notice's message says what is wrong without
// topological names; the faces, edges, vertices, points and sketch regions
// it is about are its `places` (core/model-api/src/commands.md, "Notices"), which the view
// paints while the pointer is over the notice (the command panel's message,
// a Timeline item, a Browser row, the message band, the Docked status line).
// Plain Qt Core.

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>

namespace mitcad {

// The places of these notices (each one's `places`), each once, in order.
QJsonArray noticePlaces(const QJsonArray& notices);

// The messages of a result's notices of a level ("warning", "error"), in
// order; none where it has no notices.
QStringList noticeMessages(const QJsonObject& result, const QString& level);

// The places for the log: "face F7:fillet(E{…}) of F2.b0, point 1.000,2.000,3.000",
// "region r{…} of F1" (its sketch), "none" for no places.
QString placesText(const QJsonArray& places);

// The log's words for what a notice's hover paints, after "Notice hover
// highlighted: ": "<source>: <places> (<n> drawn)"; "(drawn when the job
// ends)" for a negative count, a hover while a job computes the model,
// which is not read then (mitcad#1600); "none" for no places.
QString noticeHoverText(const QString& source, const QJsonArray& places, int drawn);

// Which feature's notice places a message under the pointer paints
// (mitcad#1609): the message band's text or stub and the Docked status
// line remember the feature they last gave, so that a message shown in
// place of the hovered one (undo, another feature's warning, a job that
// ends) gives its own feature, or none, without the pointer moving.
class NoticeHover {
public:
  // The pointer is over the message (`over`), which is about `shown` (a
  // feature's uid; empty for none). True when the feature whose places
  // are painted changed: feature() is then to be given (empty: none).
  bool update(bool over, const QString& shown) {
    const QString next = over ? shown : QString();
    if (next == m_feature) {
      return false;
    }
    m_feature = next;
    return true;
  }
  const QString& feature() const { return m_feature; }

private:
  QString m_feature;
};

} // namespace mitcad
