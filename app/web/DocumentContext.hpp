// SPDX-License-Identifier: MIT
#pragma once

// A design loaded without a window (a model Document), read as the main
// window answers the model's queries: the web page of a design file
// (web/WebPage.hpp, writePageOfFile), `mitcad-cli export` to .glb and
// .gltf (mitcad#2210, tools/cli/gltf.cpp) and its test read the model
// through it. Header only: the command line tool and its tests build it
// without the application's other sources.

#include <memory>

#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>

#include <TopoDS_Shape.hxx>

#include "framework/Command.hpp"
#include "framework/Json.hpp"
#include "mitcad_bridge/lib.h"
#include "rust/cxx.h"

namespace mitcad::web {

class DocumentContext : public CommandContext {
public:
  explicit DocumentContext(const Document& document) : m_document(document) {}

  QJsonValue query(const QJsonObject& request) const override {
    const QJsonDocument answer = parseJson(m_document.query(rustStr(compactJson(request))));
    return answer.isArray() ? QJsonValue(answer.array()) : QJsonValue(answer.object());
  }
  std::shared_ptr<geometry::Shape> bodyShape(const QString& uid) const override {
    return m_document.body_shape(uid.toStdString());
  }
  TopoDS_Shape itemShape(const SelectionItem& /*item*/) const override { return TopoDS_Shape(); }
  TopoDS_Shape analysisShape(const QJsonObject& /*request*/) const override { return TopoDS_Shape(); }

private:
  const Document& m_document;
};

} // namespace mitcad::web
