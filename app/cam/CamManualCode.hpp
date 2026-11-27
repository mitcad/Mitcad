// SPDX-License-Identifier: MIT
// A lathe setup's manual G-code (mitcad#2506, docs/cam.md "Manual G-code"):
// the setup panel's list of blocks, each with its name, the operation it
// follows and its lines, in their order, and where the Browser lists each
// block among the setup's operations. Plain Qt Core, so that the app's unit
// tests take it without the window.
#pragma once

#include <QJsonArray>
#include <QJsonObject>
#include <QPair>
#include <QString>
#include <QVector>

namespace mitcad {

class CommandState;

namespace cam {

// The setup panel's list of blocks and its rows' inputs.
inline QString manualList() { return QStringLiteral("manual_code"); }
inline QString manualName() { return QStringLiteral("name"); }
inline QString manualAfter() { return QStringLiteral("after"); }
inline QString manualText() { return QStringLiteral("code"); }
// The place before the first operation, the program's start.
inline QString programStart() { return QStringLiteral("__start"); }

// The places a block may follow: the program's start, then the setup's
// operations (cam_setups' `operations`, each with its `id` and `name`) in
// their order, by their ids.
QVector<QPair<QString, QString>> manualPlaces(const QJsonObject& setup);

// The setup's blocks (`lathe.manual_code`) as the panel's rows, in order.
void loadManualCode(CommandState& state, const QJsonArray& blocks);

// The panel's rows as `lathe.manual_code`, in their order: a row's name
// when it has one (the model's default otherwise), `after` left out at the
// program's start, its text. A row without code gives `why` and `input`
// (the row's G-code field).
QJsonArray manualCodeOf(const CommandState& state, QString* why, QString* input);

// Where the Browser lists a setup's blocks (cam_setups' setup): for each
// block in order, how many of the setup's operations come before it (0:
// before the first). A block after an operation the setup does not have
// comes last.
QVector<int> manualPlacesInOrder(const QJsonObject& setup);

// A block's place in its setup's `lathe.manual_code` (from 0) from its
// Browser row's uid "<setup>:<n>" (mitcad#2532); -1 when it has none.
int manualBlockOf(const QString& uid);

// A setup's `lathe` without the blocks `blocks` (places in its
// `manual_code`, mitcad#2532: the Browser's Delete), the others in their
// order; `manual_code` left out when none stays.
QJsonObject manualCodeWithout(const QJsonObject& lathe, const QVector<int>& blocks);

// Where a block dragged in the Browser is let go (mitcad#2532), among the
// rows of its own setup: on the setup's row (the program's start, before
// every other block there), before or after one of the setup's operations,
// or before or after another of its blocks.
enum class ManualDrop { ProgramStart, BeforeOperation, AfterOperation, BeforeBlock, AfterBlock };

// The setup's (cam_setups' setup) `lathe` with its block `block` moved
// where `drop` puts it (`index`: the operation's place in the setup's
// operations or the other block's in `manual_code`): the operation it
// follows becomes the one before that place in the program (none: the
// program's start), and among the blocks that follow the same operation it
// takes the place the Browser shows; the other blocks keep their order.
// Empty when it stays where it is or there is no such block or place.
QJsonObject manualCodeMoved(const QJsonObject& setup, int block, ManualDrop drop, int index);

} // namespace cam
} // namespace mitcad
