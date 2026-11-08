// SPDX-License-Identifier: MIT
// CommandSession: what a pick on a feature's preview names (mitcad#1458).
// While a feature command's panel is open the view draws its preview, so a
// click names a face, edge or vertex of the feature's result; the feature's
// definition names those of the bodies before it. An item the preview
// changed (a rib's foot edge that two side fillets split) is taken as the
// item it comes from (geometry/item_origin.hpp); one the preview made (a
// fillet's rounding) is not, and the status line says why and that Ctrl
// (Cmd on macOS) held shows the part's own items (mitcad#1421). The rest of
// the session is in CommandSession.cpp.
#include <memory>
#include <optional>
#include <string>
#include <utility>

#include <QString>

#include "CommandSession.hpp"
#include "mitcad/geometry/item_origin.hpp"

namespace mitcad {
namespace {

// The key held to hide the preview, as the platform names it (Qt's Ctrl is
// the Command key on macOS).
QString holdKey() {
#ifdef Q_OS_MACOS
  return QStringLiteral("Cmd");
#else
  return QStringLiteral("Ctrl");
#endif
}

// The item_origin kind letter of a face, edge or vertex; 0 for another.
char originKind(SelectKind kind) {
  switch (kind) {
  case SelectKind::Face:
    return 'F';
  case SelectKind::Edge:
    return 'E';
  case SelectKind::Vertex:
    return 'V';
  default:
    return 0;
  }
}

} // namespace

std::shared_ptr<geometry::Shape> CommandSession::previewedShape(const SelectionItem& item) const {
  if (!m_drawnPreview || m_previewHidden) {
    return nullptr;
  }
  const Preview& preview = m_drawnPreview->preview;
  for (const BodyDisplay& body : preview.bodies) {
    if (body.shape && QString::fromStdString(body.uid) == item.owner &&
        QString::fromStdString(body.occurrence) == item.occurrence &&
        preview.previewed.contains(previewBodyKey(body.uid, body.occurrence))) {
      return body.shape;
    }
  }
  return nullptr;
}

geometry::ItemOrigin CommandSession::previewOrigin(const SelectionItem& item, char kind,
                                                  const std::shared_ptr<geometry::Shape>& before,
                                                  const std::shared_ptr<geometry::Shape>& after) const {
  PreviewOrigins& body = m_previewOrigins[std::make_pair(item.owner, item.occurrence)];
  if (body.before.lock() != before || body.after.lock() != after) {
    body.before = before;
    body.after = after;
    body.items.clear();
  }
  const std::string name = item.name.toStdString();
  const auto key = std::make_pair(kind, name);
  const auto known = body.items.find(key);
  if (known != body.items.end()) {
    return known->second;
  }
  const geometry::ItemOrigin origin = geometry::item_origin(*before, after.get(), kind, name);
  body.items.emplace(key, origin);
  return origin;
}

std::optional<SelectionItem> CommandSession::referable(const SelectionItem& item) const {
  const char kind = originKind(item.kind);
  if (kind == 0) {
    if (item.kind == SelectKind::Body) {
      return m_host.bodyShape(item.owner) != nullptr ? std::optional<SelectionItem>(item) : std::nullopt;
    }
    return item;
  }
  const std::shared_ptr<geometry::Shape> before = m_host.bodyShape(item.owner);
  if (!before) {
    return std::nullopt;
  }
  // The preview's shape only for a body it changes: elsewhere the name is
  // the body's own, and whether the body has it is enough, with no
  // geometry to look at (mitcad#1464: item_origin measured the 63 pieces
  // of a modelled thread's face for every item the pick filter was
  // offered, and the view stopped answering).
  const std::shared_ptr<geometry::Shape> after = previewedShape(item);
  if (!after) {
    const std::string name = item.name.toStdString();
    const bool has = kind == 'F'   ? !before->matching_faces(name).empty()
                     : kind == 'E' ? !before->find_edges(name).empty()
                                   : !before->find_vertices(name).empty();
    return has ? std::optional<SelectionItem>(item) : std::nullopt;
  }
  const geometry::ItemOrigin origin = previewOrigin(item, kind, before, after);
  if (origin.kind != geometry::ItemOrigin::Kind::Same && origin.kind != geometry::ItemOrigin::Kind::Modified) {
    return std::nullopt;
  }
  SelectionItem named = item;
  named.name = QString::fromStdString(origin.name);
  return named;
}

QString CommandSession::previewRefusal(const SelectionItem& item) const {
  const char kind = originKind(item.kind);
  if (kind == 0 || !m_drawnPreview || m_previewHidden || referable(item)) {
    return QString();
  }
  const QString one = SelectionItem::kindText(item.kind);
  const QString own = SelectionItem::kindText(item.kind, 2);
  bool made = true;
  if (const std::shared_ptr<geometry::Shape> before = m_host.bodyShape(item.owner)) {
    const std::shared_ptr<geometry::Shape> after = previewedShape(item);
    const geometry::ItemOrigin origin = after ? previewOrigin(item, kind, before, after)
                                              : geometry::item_origin(*before, nullptr, kind, item.name.toStdString());
    made = origin.kind == geometry::ItemOrigin::Kind::Made;
  }
  if (made) {
    return tr("%1: this %2 is part of the preview, made by the %1 itself. Hold %3 to hide the preview and "
              "pick the part's own %4.")
        .arg(m_def.name, one, holdKey(), own);
  }
  return tr("%1: the preview changed this %2, and which %2 of the part it comes from cannot be told. Hold %3 "
            "to hide the preview and pick the part's own %4.")
      .arg(m_def.name, one, holdKey(), own);
}

} // namespace mitcad
