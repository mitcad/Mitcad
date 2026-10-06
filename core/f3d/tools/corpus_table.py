#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Per-file table for CORPUS_REPORT.md.

Usage: corpus_table.py <inspector> <occt-report> <corpus-dir>

<inspector> is the mitcad-f3d-inspect binary, <occt-report> the output of
`test_brep_import --corpus` for the same directory. Files are named f01,
f02, ... in sorted path order, as test_brep_import does.
"""
import collections
import os
import re
import subprocess
import sys


def corpus_files(root):
    out = []
    for dirpath, _, files in os.walk(root):
        for f in files:
            if f.endswith('.f3d') or f.endswith('.f3z'):
                out.append(os.path.join(dirpath, f))
    return sorted(out)


def main():
    inspector, report, root = sys.argv[1:4]
    files = corpus_files(root)
    line = re.compile(r'^(f\d\d) (?:\w+/)?BREP\.[0-9a-f]+(h?)#\d+(o?) (\w+) (\w+) ')
    per = collections.defaultdict(collections.Counter)
    for text in open(report, encoding='utf-8'):
        m = line.match(text)
        if not m:
            continue
        fid, hist, owner, kind, valid = m.groups()
        c = per[fid]
        c['bodies'] += 1
        c['owner'] += bool(owner)
        c[kind] += 1
        c[kind + '_valid'] += valid == 'valid'
        c['issues'] += ' issues' in text
    print('| File | ASM | Format | Blobs (smb/smbh) | Bodies (top-level) | Solids valid | Sheets valid | Converter issues |')
    print('|---|---|---|---|---|---|---|---|')
    for i, path in enumerate(files):
        fid = 'f%02d' % (i + 1)
        info = subprocess.run([inspector, 'info', path], capture_output=True, text=True).stdout
        versions = sorted(set(re.findall(r'"ASM (\d+\.\d+)', info)))
        formats = sorted(set(re.findall(r'BinaryFile(\d)', info)))
        smb = len(re.findall(r' smb: ', info))
        smbh = len(re.findall(r' smbh: ', info))
        c = per[fid]
        print('| %s | %s | %s | %d/%d | %d (%d) | %d/%d | %d/%d | %d |' % (
            fid, ', '.join(versions) or '-', '/'.join(formats) or '-', smb, smbh,
            c['bodies'], c['bodies'] - c['owner'], c['solid_valid'], c['solid'],
            c['sheet_valid'], c['sheet'], c['issues']))


if __name__ == '__main__':
    main()
