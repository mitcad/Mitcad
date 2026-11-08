// SPDX-License-Identifier: MIT
#include "ProjectRowText.hpp"

#include <algorithm>

#include <QCoreApplication>
#include <QFontMetrics>
#include <QStringList>

#include "ProjectsTree.hpp"

namespace mitcad {
namespace {

// The file's texts in the context ProjectsPanel, where lupdate records them
// too (mitcad#1472).
struct Text {
  Q_DECLARE_TR_FUNCTIONS(ProjectsPanel)
};

QString dot() { return QStringLiteral(" ") + QChar(0x00B7) + QStringLiteral(" "); }

const QString kSuffix = QStringLiteral(".mitcad");

} // namespace

QString RowMarks::logged() const {
  QStringList parts;
  if (!text.isEmpty()) {
    parts << text;
  }
  if (!icon.isEmpty()) {
    parts << QStringLiteral("(%1)").arg(icon);
  }
  return parts.isEmpty() ? QStringLiteral("none") : parts.join(QLatin1Char(' '));
}

RowMarks designMarks(const ProjectDesignRow& design) {
  RowMarks marks;
  QStringList full;
  QStringList words;
  if (design.version > 0) {
    marks.text = QStringLiteral("v%1").arg(design.version);
    full << Text::tr("v%1").arg(design.version);
    words << QStringLiteral("v%1").arg(design.version);
  } else {
    marks.text = Text::tr("new");
    full << Text::tr("Not recorded");
    words << QStringLiteral("not recorded");
  }
  if (design.recorded && design.changed) {
    marks.text += QLatin1Char('*');
    full << Text::tr("Changed");
    words << QStringLiteral("changed");
  }
  if (design.locked) {
    if (design.lockMine) {
      marks.icon = QStringLiteral("edit");
      full << Text::tr("Editing");
      words << QStringLiteral("editing");
    } else if (design.lockSameOwner) {
      marks.icon = QStringLiteral("lock");
      full << Text::tr("Editing elsewhere");
      words << QStringLiteral("editing elsewhere");
    } else {
      marks.icon = QStringLiteral("lock");
      full << Text::tr("Locked: %1").arg(design.lockOwner);
      words << QStringLiteral("locked by %1").arg(design.lockOwner);
    }
  }
  marks.full = full.join(dot());
  marks.words = words.join(QStringLiteral(", "));
  return marks;
}

RowMarks projectMarks(const ProjectRow& row, int ahead, int behind, const QString& problem, bool conflict) {
  RowMarks marks;
  switch (row.kind) {
  case ProjectRow::Kind::Missing:
    marks.icon = QStringLiteral("warning");
    marks.full = Text::tr("Missing");
    marks.words = QStringLiteral("missing");
    return marks;
  case ProjectRow::Kind::Unreadable:
    marks.icon = QStringLiteral("warning");
    marks.full = Text::tr("Cannot be read");
    marks.words = QStringLiteral("unreadable");
    return marks;
  case ProjectRow::Kind::NoHistory:
    marks.text = QString(QChar(0x2013));
    marks.full = Text::tr("No versions");
    marks.words = QStringLiteral("no versions");
    return marks;
  case ProjectRow::Kind::Local:
    if (row.noRemoteChosen) {
      marks.icon = QStringLiteral("cloud");
      marks.text = QStringLiteral("?");
      marks.full = Text::tr("Cloud") + dot() + Text::tr("No remote chosen");
      marks.words = QStringLiteral("cloud, no remote chosen");
      return marks;
    }
    // The row's icon tells Local already.
    marks.full = Text::tr("Local");
    marks.words = QStringLiteral("local");
    return marks;
  case ProjectRow::Kind::Cloud:
    break;
  }
  const QString up = QString(QChar(0x2191));
  const QString down = QString(QChar(0x2193));
  QStringList full{Text::tr("Cloud")};
  QStringList words{QStringLiteral("cloud")};
  QStringList text;
  marks.icon = QStringLiteral("cloud");
  if (conflict) {
    marks.icon = QStringLiteral("warning");
    full << Text::tr("Conflict");
    words << QStringLiteral("conflict");
  } else if (problem == QLatin1String("network") || problem == QLatin1String("timed_out")) {
    marks.icon = QStringLiteral("cloud-offline");
    full << (ahead > 0 ? Text::tr("Offline, %1%2 waiting").arg(up).arg(ahead) : Text::tr("Offline"));
    words << QStringLiteral("offline");
    if (ahead > 0) {
      text << up + QString::number(ahead);
    }
  } else if (problem == QLatin1String("auth_failed") || problem == QLatin1String("host_key_unknown")) {
    marks.icon = QStringLiteral("warning");
    full << Text::tr("Sign-in needed");
    words << QStringLiteral("sign-in needed");
  } else {
    if (ahead > 0) {
      full << up + QString::number(ahead);
      words << QStringLiteral("ahead %1").arg(ahead);
      text << up + QString::number(ahead);
    }
    if (behind > 0) {
      full << down + QString::number(behind);
      words << QStringLiteral("behind %1").arg(behind);
      text << down + QString::number(behind);
    }
    if (ahead == 0 && behind == 0) {
      marks.icon = QStringLiteral("cloud-synced");
      full << QString(QChar(0x2713));
      words << QStringLiteral("synced");
    } else if (ahead <= 0 && behind <= 0) {
      full << Text::tr("Not synced yet");
      words << QStringLiteral("not synced");
    }
  }
  marks.text = text.join(QLatin1Char(' '));
  marks.full = full.join(dot());
  marks.words = words.join(QStringLiteral(", "));
  return marks;
}

QString elideMiddle(const QString& text, const QFontMetrics& metrics, int width) {
  if (metrics.horizontalAdvance(text) <= width) {
    return text;
  }
  return metrics.elidedText(text, Qt::ElideMiddle, std::max(0, width));
}

QString designDisplayName(const QString& path) {
  const QString file = path.mid(path.lastIndexOf(QLatin1Char('/')) + 1);
  return file.endsWith(kSuffix, Qt::CaseInsensitive) ? file.left(file.size() - kSuffix.size()) : file;
}

} // namespace mitcad
