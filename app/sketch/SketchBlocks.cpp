// SPDX-License-Identifier: MIT
// Sketch blocks (mitcad#2110; app/COMMANDS.md, "Sketch blocks"; commands.md,
// "Sketch blocks"): Create Block turns the selected geometry into a named
// block of the design and an instance of it in place, its base point
// clicked or typed; Insert Block lists the design's blocks with a preview
// of each and places one where a click or typed X and Y say, turned by the
// typed angle; Explode Block makes an instance plain geometry; Edit Block
// opens an instance's block in place, Finish Block takes the edit (every
// instance follows) and Cancel Block leaves the block as it was. An
// instance is picked and hovered as one.
#include "SketchController.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include <QCursor>
#include <QIcon>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QMenu>
#include <QPainter>
#include <QPainterPath>
#include <QPixmap>
#include <QString>
#include <QStringList>
#include <QVector>

#include <QDebug>
#include <QtLogging>

#include "../framework/Theme.hpp"
#include "../framework/UserText.hpp"
#include "SketchTool.hpp"
#include "ToolSupport.hpp"

namespace mitcad::sketch {
namespace {

constexpr double kPi = 3.14159265358979323846;

V2 pointOf(const QJsonValue& value) {
  const QJsonArray at = value.toArray();
  return V2{at.at(0).toDouble(), at.at(1).toDouble()};
}

V2 turned(const V2& p, double angle) {
  const double c = std::cos(angle);
  const double s = std::sin(angle);
  return V2{c * p.x - s * p.y, s * p.x + c * p.y};
}

// The design's blocks (the `sketch_blocks` query's `blocks`); empty when
// the model refused the query.
QJsonArray designBlocks(SketchController& c) {
  try {
    return c.host()
        .queryObject({{QStringLiteral("query"), QStringLiteral("sketch_blocks")}})
        .value(QStringLiteral("blocks"))
        .toArray();
  } catch (const std::exception&) {
    return {};
  }
}

// A small picture of a block's outline, for Insert Block's list.
QIcon previewIcon(const QJsonObject& block) {
  constexpr int size = 48;
  QPixmap pixmap(size, size);
  pixmap.fill(Qt::transparent);
  const QJsonArray bounds = block.value(QStringLiteral("bounds")).toArray();
  const double x0 = bounds.at(0).toDouble();
  const double y0 = bounds.at(1).toDouble();
  const double width = std::max(bounds.at(2).toDouble() - x0, 1e-9);
  const double height = std::max(bounds.at(3).toDouble() - y0, 1e-9);
  const double scale = (size - 8) / std::max(width, height);
  QPainter painter(&pixmap);
  painter.setRenderHint(QPainter::Antialiasing);
  // The outline in the text color, so it reads on light and dark lists.
  painter.setPen(QPen(themeColor(ColorRole::Text), 1.5));
  for (const QJsonValue& line : block.value(QStringLiteral("outline")).toArray()) {
    QPainterPath path;
    bool first = true;
    for (const QJsonValue& p : line.toArray()) {
      const V2 at = pointOf(p);
      // y up, as in the sketch.
      const QPointF screen(4 + (at.x - x0) * scale + (size - 8 - width * scale) / 2,
                           size - 4 - (at.y - y0) * scale - (size - 8 - height * scale) / 2);
      if (first) {
        path.moveTo(screen);
        first = false;
      } else {
        path.lineTo(screen);
      }
    }
    painter.drawPath(path);
  }
  painter.end();
  return QIcon(pixmap);
}

// Create Block: the base point clicked, or typed X and Y with Enter; the
// name typed (Block<n> when left empty). Esc cancels.
class CreateBlockTool : public SketchTool {
public:
  CreateBlockTool(SketchController& controller, QStringList items)
      : SketchTool(controller), m_items(std::move(items)) {}

  QString name() const override { return QObject::tr("Create Block"); }

  void start() override {
    m_c.setFields({{QStringLiteral("name"), QObject::tr("Name"), FieldSpec::Kind::Text},
                   {QStringLiteral("x"), QObject::tr("X"), FieldSpec::Kind::Length},
                   {QStringLiteral("y"), QObject::tr("Y"), FieldSpec::Kind::Length}});
    m_c.hint(QObject::tr("Create Block: type its name, then click its base point or type X and Y and press "
                         "Enter. Esc cancels."));
  }

  void move(const Snap& snap) override {
    m_at = snap.at;
    m_c.setLive(QStringLiteral("x"), m_at.x);
    m_c.setLive(QStringLiteral("y"), m_at.y);
  }

  void press(const Snap& snap) override { m_done = make(target(snap.at)); }

  void release(const Snap& /*snap*/) override {
    if (m_done) {
      m_c.stopTool(); // this tool is deleted later
    }
  }

  bool confirm() override {
    if (make(target(m_at))) {
      m_c.stopTool();
    }
    return true;
  }

  void paint(ToolPreview& preview) const override { preview.points.push_back(target(m_at)); }

private:
  V2 target(const V2& at) const {
    return V2{m_c.fieldValue(QStringLiteral("x")).value_or(at.x), m_c.fieldValue(QStringLiteral("y")).value_or(at.y)};
  }

  bool make(const V2& base) {
    SketchOp op(m_c);
    QJsonObject cmd{{QStringLiteral("cmd"), QStringLiteral("add_sketch_block")},
                    {QStringLiteral("sketch"), m_c.uid()},
                    {QStringLiteral("items"), QJsonArray::fromStringList(m_items)},
                    {QStringLiteral("base"), xy(base)}};
    const QString typed = m_c.fieldText(QStringLiteral("name")).trimmed();
    if (!typed.isEmpty()) {
      cmd.insert(QStringLiteral("name"), typed);
    }
    QJsonObject result;
    if (!op.run(cmd, &result)) {
      const QString reason = op.error();
      op.rollback();
      m_c.error(QObject::tr("Could not create the block: %1").arg(reason));
      return false;
    }
    // The selection is the instance now: nothing stays selected.
    m_c.host().setSelection({});
    op.commit();
    const QString block = result.value(QStringLiteral("name")).toString();
    qDebug().noquote() << QStringLiteral("Created block %1 of %2 in %3 as instance %4, base point at %5")
                              .arg(block, m_items.join(QStringLiteral(", ")), m_c.model().name,
                                   result.value(QStringLiteral("instance")).toString(), text(base));
    m_c.notice(QObject::tr("Block %1 created: the selection is an instance of it.").arg(block));
    return true;
  }

  QStringList m_items;
  V2 m_at;
  bool m_done = false;
};

// Insert Block: the chosen block's outline at the pointer by its base
// point, turned by the typed angle (the tool's menu turns it a quarter);
// a click or typed X and Y with Enter places it. Esc cancels.
class InsertBlockTool : public SketchTool {
public:
  InsertBlockTool(SketchController& controller, QJsonObject block)
      : SketchTool(controller), m_block(std::move(block)) {
    for (const QJsonValue& line : m_block.value(QStringLiteral("outline")).toArray()) {
      std::vector<V2> points;
      for (const QJsonValue& p : line.toArray()) {
        points.push_back(pointOf(p));
      }
      if (points.size() >= 2) {
        m_outline.push_back(std::move(points));
      }
    }
  }

  QString name() const override { return QObject::tr("Insert Block"); }

  void start() override {
    m_c.setFields({{QStringLiteral("x"), QObject::tr("X"), FieldSpec::Kind::Length},
                   {QStringLiteral("y"), QObject::tr("Y"), FieldSpec::Kind::Length},
                   {QStringLiteral("angle"), QObject::tr("Rotate"), FieldSpec::Kind::Angle}});
    m_c.hint(QObject::tr("Insert Block %1: click where its base point goes, or type X and Y and press Enter; "
                         "type Rotate, or press Space, to turn it. Esc cancels.")
                 .arg(blockName()));
  }

  void move(const Snap& snap) override {
    m_at = snap.at;
    m_c.setLive(QStringLiteral("x"), m_at.x);
    m_c.setLive(QStringLiteral("y"), m_at.y);
  }

  void press(const Snap& snap) override { m_placed = place(target(snap.at)); }

  void release(const Snap& /*snap*/) override {
    if (m_placed) {
      m_c.stopTool();
    }
  }

  bool confirm() override {
    if (place(target(m_at))) {
      m_c.stopTool();
    }
    return true;
  }

  std::vector<ToolMenuEntry> menu(const Snap& /*snap*/) override {
    ToolMenuEntry quarter;
    quarter.text = QObject::tr("Rotate 90°");
    quarter.run = [this] { turnQuarter(); };
    return {quarter};
  }

  // Space turns it a quarter, as the menu's Rotate 90° does.
  bool key(int key, Qt::KeyboardModifiers modifiers) override {
    if (key != Qt::Key_Space || (modifiers & ~Qt::KeypadModifier) != Qt::NoModifier) {
      return false;
    }
    turnQuarter();
    return true;
  }

  void paint(ToolPreview& preview) const override {
    const V2 at = target(m_at);
    const double a = angle();
    for (const std::vector<V2>& line : m_outline) {
      std::vector<V2> placed;
      placed.reserve(line.size());
      for (const V2& p : line) {
        placed.push_back(at + turned(p, a));
      }
      preview.add(std::move(placed));
    }
    preview.points.push_back(at);
  }

private:
  QString blockName() const { return m_block.value(QStringLiteral("name")).toString(); }

  void turnQuarter() {
    m_turn += kPi / 2.0;
    qDebug().noquote() << QStringLiteral("Insert Block turned to %1°").arg(std::lround(angle() * 180.0 / kPi));
  }

  double angle() const { return m_c.fieldValue(QStringLiteral("angle")).value_or(0.0) + m_turn; }

  V2 target(const V2& at) const {
    return V2{m_c.fieldValue(QStringLiteral("x")).value_or(at.x), m_c.fieldValue(QStringLiteral("y")).value_or(at.y)};
  }

  bool place(const V2& at) {
    SketchOp op(m_c);
    QJsonObject cmd = op.command(QStringLiteral("sketch.insert_block"));
    cmd.insert(QStringLiteral("block"), blockName());
    cmd.insert(QStringLiteral("at"), xy(at));
    cmd.insert(QStringLiteral("angle"), angle());
    QJsonObject result;
    if (!op.run(cmd, &result)) {
      const QString reason = op.error();
      op.rollback();
      m_c.error(QObject::tr("Could not insert the block: %1").arg(reason));
      return false;
    }
    op.commit();
    qDebug().noquote() << QStringLiteral("Inserted block %1 into %2 as instance %3 at %4, turned %5°")
                              .arg(blockName(), m_c.model().name, result.value(QStringLiteral("instance")).toString(),
                                   text(at))
                              .arg(std::lround(angle() * 180.0 / kPi));
    return true;
  }

  QJsonObject m_block;
  std::vector<std::vector<V2>> m_outline;
  V2 m_at;
  double m_turn = 0.0;
  bool m_placed = false;
};

} // namespace

QStringList SketchController::instanceMembers(const QString& entity) const {
  const BlockInstanceData* instance = m_model.instanceOf(entity);
  if (instance == nullptr || instance->open) {
    return {};
  }
  return instance->entities;
}

Selection SketchController::instanceSelection(const SelectionItem& item) const {
  Selection out;
  if (item.owner != m_uid ||
      (item.kind != SelectKind::SketchCurve && item.kind != SelectKind::SketchPoint)) {
    return out;
  }
  for (const QString& id : instanceMembers(item.name)) {
    SelectionItem member;
    if (selectionItemOf(id, member)) {
      out.append(member);
    }
  }
  return out;
}

const BlockInstanceData* SketchController::selectedInstance() const {
  for (const SelectionItem& item : m_host.selection()) {
    if (item.owner != m_uid) {
      continue;
    }
    if (const BlockInstanceData* instance = m_model.instanceOf(item.name)) {
      return instance;
    }
  }
  return nullptr;
}

const BlockInstanceData* SketchController::openInstance() const {
  for (const BlockInstanceData& b : m_model.blocks) {
    if (b.open) {
      return &b;
    }
  }
  return nullptr;
}

void SketchController::createBlock() {
  QStringList items;
  for (const SelectionItem& item : m_host.selection()) {
    const bool geometry = (item.kind == SelectKind::SketchCurve || item.kind == SelectKind::SketchPoint) &&
                          !isAxisSelection(item);
    if (item.owner == m_uid && geometry && !items.contains(item.name)) {
      items << item.name;
    }
  }
  if (items.isEmpty()) {
    hint(tr("Create Block: select the sketch points and curves that make the block first."));
    return;
  }
  startTool(std::make_unique<CreateBlockTool>(*this, items));
}

void SketchController::insertBlock() {
  const QJsonArray blocks = designBlocks(*this);
  if (blocks.isEmpty()) {
    hint(tr("The design has no blocks yet: select geometry and use Create Block first."));
    return;
  }
  QJsonObject chosen;
  if (blocks.size() == 1) {
    chosen = blocks.first().toObject();
  } else {
    // The design's blocks, each with a small picture of it.
    QMenu menu(&m_viewer);
    QVector<QAction*> actions;
    for (const QJsonValue& value : blocks) {
      const QJsonObject block = value.toObject();
      actions.append(menu.addAction(previewIcon(block), block.value(QStringLiteral("name")).toString()));
    }
    QStringList names;
    for (const QAction* action : std::as_const(actions)) {
      names << action->text();
    }
    qDebug().noquote() << QStringLiteral("Insert Block list: %1").arg(names.join(QStringLiteral(" | ")));
    const QAction* picked = menu.exec(QCursor::pos());
    if (picked == nullptr) {
      return;
    }
    chosen = blocks.at(static_cast<int>(actions.indexOf(const_cast<QAction*>(picked)))).toObject();
  }
  startTool(std::make_unique<InsertBlockTool>(*this, chosen));
}

void SketchController::explodeBlock() {
  const BlockInstanceData* instance = selectedInstance();
  if (instance == nullptr || instance->open) {
    hint(tr("Explode Block: select an instance of a block first."));
    return;
  }
  const QString id = instance->id;
  const QString block = instance->name;
  SketchOp op(*this);
  QJsonObject cmd = op.command(QStringLiteral("sketch.explode_block"));
  cmd.insert(QStringLiteral("instance"), id);
  QJsonObject result;
  if (!op.run(cmd, &result)) {
    const QString reason = op.error();
    op.rollback();
    error(tr("Could not explode the block: %1").arg(reason));
    return;
  }
  m_host.setSelection({});
  op.commit();
  qDebug().noquote() << QStringLiteral("Exploded instance %1 of block %2 in %3").arg(id, block, m_model.name);
  const QStringList notes = shownTexts(result, QStringLiteral("warnings"), QStringLiteral("warning_messages"));
  if (!notes.isEmpty()) {
    notice(tr("Exploded: %1.").arg(notes.join(QStringLiteral("; "))));
  }
}

void SketchController::editBlock() {
  if (openInstance() != nullptr) {
    hint(tr("A block is being edited in this sketch: Finish Block first."));
    return;
  }
  const BlockInstanceData* instance = selectedInstance();
  if (instance == nullptr) {
    hint(tr("Edit Block: select an instance of a block first, or edit it from the Browser's Sketch Blocks."));
    return;
  }
  const QString id = instance->id;
  const QString block = instance->name;
  SketchOp op(*this);
  const QJsonObject cmd{{QStringLiteral("cmd"), QStringLiteral("edit_sketch_block")},
                        {QStringLiteral("block"), block},
                        {QStringLiteral("sketch"), m_uid},
                        {QStringLiteral("instance"), id}};
  QJsonObject result;
  if (!op.run(cmd, &result)) {
    const QString reason = op.error();
    op.rollback();
    error(tr("Could not edit the block: %1").arg(reason));
    return;
  }
  m_host.setSelection({});
  op.commit();
  qDebug().noquote() << QStringLiteral("Editing block %1 in place at instance %2 of %3").arg(block, id, m_model.name);
  hint(tr("Editing block %1: change its geometry, then Finish Block; Cancel Block leaves it as it was.").arg(block));
}

bool SketchController::openBlockIsTemporary() const {
  const BlockInstanceData* open = openInstance();
  return open != nullptr && open->temporary;
}

void SketchController::finishBlock(bool cancel) {
  const BlockInstanceData* open = openInstance();
  if (open == nullptr) {
    hint(tr("No block is being edited in this sketch."));
    return;
  }
  const QString block = open->name;
  if (open->temporary) {
    // The block's own sketch goes with the edit: the sketch is left first,
    // and the edit ends then (MainWindow::finishSketch).
    m_cancelBlock = cancel;
    m_host.trigger(QStringLiteral("sketch.finish"));
    return;
  }
  SketchOp op(*this);
  const QJsonObject cmd{{QStringLiteral("cmd"), QStringLiteral("finish_sketch_block")},
                        {QStringLiteral("cancel"), cancel}};
  QJsonObject result;
  if (!op.run(cmd, &result)) {
    const QString reason = op.error();
    op.rollback();
    error(tr("Could not finish the block: %1").arg(reason));
    return;
  }
  op.commit();
  logBlockInstances(cancel ? QStringLiteral("Cancelled") : QStringLiteral("Finished"), block);
  const QStringList notes = shownTexts(result, QStringLiteral("warnings"), QStringLiteral("warning_messages"));
  if (!notes.isEmpty()) {
    notice(tr("Block %1: %2.").arg(block, notes.join(QStringLiteral("; "))), NoticeLevel::Warning);
  }
}

void SketchController::logBlockInstances(const QString& what, const QString& block) const {
  qDebug().noquote() << QStringLiteral("%1 block %2").arg(what, block);
  // Each instance of it here, with how long its points reach along its
  // direction, for UI tests.
  for (const BlockInstanceData& instance : m_model.blocks) {
    if (instance.name != block) {
      continue;
    }
    const std::optional<V2> base = m_model.pointAt(instance.base);
    const CurveData* direction = m_model.curve(instance.direction);
    if (!base || direction == nullptr) {
      continue;
    }
    const std::vector<V2> along = direction->curve.sample(1.0);
    if (along.size() < 2) {
      continue;
    }
    V2 d = along.back() - along.front();
    const double length = std::hypot(d.x, d.y);
    if (length <= 0.0) {
      continue;
    }
    d = V2{d.x / length, d.y / length};
    double low = 0.0;
    double high = 0.0;
    for (const QString& id : instance.entities) {
      if (id == instance.base || m_model.point(id) == nullptr || id == direction->end) {
        continue;
      }
      if (const std::optional<V2> at = m_model.pointAt(id)) {
        const double t = (at->x - base->x) * d.x + (at->y - base->y) * d.y;
        low = std::min(low, t);
        high = std::max(high, t);
      }
    }
    qDebug().noquote() << QStringLiteral("Block instance %1 of %2: %3 mm along its direction")
                              .arg(instance.id, block, QString::number(high - low, 'f', 3));
  }
}

} // namespace mitcad::sketch
