// SPDX-License-Identifier: MIT
#pragma once

// What the Import dialog says when the import worker stopped without
// saying why itself (mitcad#2085): in words, naming the item it was
// importing, never a raw line of its output that carries a model
// message's places tail, JSON or a face, edge or feature id
// (docs/ui-conventions.md "What a message says"); the raw lines go to the
// log only.

#include <QString>

namespace mitcad {

// A line of the worker's output a person may read as why it stopped: not
// a places tail ("[placed: {…}]", the second line of a model message the
// worker's trace printed) nor any other JSON; the text before a tail on
// the same line.
QString importWorkerLineShown(const QString& line);

// The message of a worker that stopped: `crashed` (a crash exit) or with
// `exitCode`; `item` the item it was importing as the progress shows it
// (empty when not known); `lastLine` its last line of other output, shown
// only for an exit with a code (a crash's last line is whatever the trace
// printed last, not why it stopped).
QString importWorkerStopped(bool crashed, int exitCode, const QString& item, const QString& lastLine);

} // namespace mitcad
