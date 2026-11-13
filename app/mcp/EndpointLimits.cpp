// SPDX-License-Identifier: MIT
#include "EndpointLimits.hpp"

#include <algorithm>
#include <cstddef>

#include <QByteArray>
#include <QCryptographicHash>

namespace mitcad::mcp {

QString echoed(const QString& text, qsizetype most) {
  if (text.size() <= most) {
    return text;
  }
  qsizetype cut = most;
  // Not between the halves of a surrogate pair.
  if (cut > 0 && text.at(cut - 1).isHighSurrogate()) {
    --cut;
  }
  return text.left(cut) + QStringLiteral("… (%1 more characters)").arg(text.size() - cut);
}

int pendingToClose(const std::vector<quint64>& pendingSince, int most) {
  if (pendingSince.empty() || static_cast<qsizetype>(pendingSince.size()) < static_cast<qsizetype>(most)) {
    return -1;
  }
  std::size_t oldest = 0;
  for (std::size_t index = 1; index < pendingSince.size(); ++index) {
    if (pendingSince[index] < pendingSince[oldest]) {
      oldest = index;
    }
  }
  return static_cast<int>(oldest);
}

int pendingToClose(const std::vector<quint64>& pendingSince, const std::vector<qint64>& waitedMs, int most,
                   qint64 graceMs, int hardMost) {
  const auto count = static_cast<qsizetype>(pendingSince.size());
  if (count < static_cast<qsizetype>(most) || waitedMs.size() != pendingSince.size()) {
    return pendingToClose(pendingSince, most);
  }
  if (count >= static_cast<qsizetype>(hardMost)) {
    return pendingToClose(pendingSince, 1);
  }
  int oldest = -1;
  for (std::size_t index = 0; index < pendingSince.size(); ++index) {
    if (waitedMs[index] >= graceMs &&
        (oldest < 0 || pendingSince[index] < pendingSince[static_cast<std::size_t>(oldest)])) {
      oldest = static_cast<int>(index);
    }
  }
  return oldest;
}

bool clientSlotFree(int authenticated, int most) { return authenticated < most; }

bool sameToken(const QString& given, const QString& expected) {
  const QByteArray a = QCryptographicHash::hash(given.toUtf8(), QCryptographicHash::Sha256);
  const QByteArray b = QCryptographicHash::hash(expected.toUtf8(), QCryptographicHash::Sha256);
  unsigned int difference = a.size() == b.size() ? 0U : 1U;
  const qsizetype length = std::min(a.size(), b.size());
  for (qsizetype index = 0; index < length; ++index) {
    difference |= static_cast<unsigned int>(static_cast<unsigned char>(a.at(index)) ^
                                            static_cast<unsigned char>(b.at(index)));
  }
  return difference == 0U && !expected.isEmpty();
}

} // namespace mitcad::mcp
