#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Compares the bodies of .smbh blobs with the .smb bodies of the same
document, using the output of `test_brep_import --corpus [--history]`: a
body counts as the same when face count, area and volume agree to 1e-6.

With --history in the report, the .smbh bodies rolled back through their
ASM history (NAME@k: k states back) are compared too: for each top-level
.smbh body, the fewest states rolled back that give an .smb body, and for
each top-level .smb solid, whether some state of an .smbh body equals it.

Usage: smbh_compare.py <occt-report> [-v]
"""
import collections
import re
import sys

LINE = re.compile(r'^(f\d\d) (?:(\w+)/)?(BREP\.[0-9a-f]+)(h?)#(\d+)(?:@(\d+))?(o?) (\w+) (\w+) '
                  r'V=(\S+) raw=\S+ A=(\S+) F=(\d+) ')


def same(a, b):
    return (a['faces'] == b['faces']
            and abs(a['area'] - b['area']) <= 1e-6 * max(1.0, abs(b['area']))
            and abs(a['volume'] - b['volume']) <= 1e-6 * max(1.0, abs(b['volume'])))


def main():
    verbose = '-v' in sys.argv
    docs = collections.defaultdict(list)
    for text in open(sys.argv[1], encoding='utf-8'):
        m = LINE.match(text)
        if not m:
            continue
        fid, doc, blob, hist, record, step, owner, kind, _, volume, area, faces = m.groups()
        docs[(fid, doc)].append({
            'blob': blob, 'history': bool(hist), 'record': int(record), 'step': int(step or 0),
            'top': not owner, 'kind': kind, 'volume': float(volume), 'area': float(area),
            'faces': int(faces)})
    stats = collections.Counter()
    steps_needed = collections.Counter()
    for key, bodies in sorted(docs.items(), key=lambda kv: (kv[0][0], kv[0][1] or '')):
        smb = [b for b in bodies if not b['history']]
        states = collections.defaultdict(list)
        for b in bodies:
            if b['history'] and b['top']:
                states[(b['blob'], b['record'])].append(b)
        blobs = {blob for blob, _ in states}
        for blob in blobs:
            stats['smbh blobs'] += 1
            stats['smbh blobs with one top-level body'] += sum(1 for b, _ in states if b == blob) == 1
        for (blob, record), versions in sorted(states.items()):
            versions.sort(key=lambda b: b['step'])
            current = versions[0]
            if current['step'] != 0:
                continue
            stats['smbh top-level bodies'] += 1
            stats['  earlier states built (--history)'] += len(versions) - 1
            if any(same(current, s) for s in smb):
                stats['  identical to an .smb body'] += 1
                continue
            stats['  not found in the .smb'] += 1
            back = next((v['step'] for v in versions[1:] if any(same(v, s) for s in smb)), None)
            if back is not None:
                stats['    found in the .smb after rolling back'] += 1
                steps_needed[back] += 1
            elif verbose:
                print('not in .smb:', key[0], blob, record, current['kind'], current['faces'],
                      'states', len(versions))
        # .smb solids that are some state of an .smbh body.
        everything = [v for vs in states.values() for v in vs]
        for s in smb:
            if s['top'] and s['kind'] == 'solid':
                stats['smb top-level solids'] += 1
                if any(same(v, s) for v in everything):
                    stats['  equal to a state of an .smbh body'] += 1
    for k in ['smbh blobs', 'smbh blobs with one top-level body', 'smbh top-level bodies',
              '  earlier states built (--history)', '  identical to an .smb body',
              '  not found in the .smb', '    found in the .smb after rolling back',
              'smb top-level solids', '  equal to a state of an .smbh body']:
        print('%6d %s' % (stats[k], k))
    if steps_needed:
        print('states rolled back until an .smb body:',
              ', '.join('%d: %d' % kv for kv in sorted(steps_needed.items())))


if __name__ == '__main__':
    main()
