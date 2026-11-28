# SPDX-License-Identifier: MIT
"""The design files of the unreadable CAM UI tests (mitcad#2587), for the
Linux and macOS runners (the Windows ones do the same in PowerShell):

  add <file> [newer]          adds a CAM section this MitCad cannot read (two
                              setups with one id) to a design file, and with
                              `newer` a field a newer MitCad wrote
                              (future_notes)
  features <file>             prints the number of its features
  dropped <file> <features>   checks a file saved with Save and Drop Them:
                              no CAM, no future_notes, <features> features
"""
import json
import sys


def main(argv):
    if len(argv) >= 2 and argv[0] == 'add':
        path = argv[1]
        with open(path, encoding='utf-8') as f:
            doc = json.load(f)
        doc['cam'] = {'version': 1, 'setups': [{'id': 1, 'name': 'Setup 1'}, {'id': 1, 'name': 'Setup 2'}]}
        if argv[2:] == ['newer']:
            doc['future_notes'] = [{'id': 'N1', 'text': 'Check the wall'}]
        with open(path, 'w', encoding='utf-8') as f:
            json.dump(doc, f, indent=2)
            f.write('\n')
        return 0
    if len(argv) == 2 and argv[0] == 'features':
        with open(argv[1], encoding='utf-8') as f:
            print(len(json.load(f).get('features', [])))
        return 0
    if len(argv) == 3 and argv[0] == 'dropped':
        with open(argv[1], encoding='utf-8') as f:
            doc = json.load(f)
        left = [field for field in ('cam', 'future_notes') if field in doc]
        if left:
            print('the saved file still has %s' % ', '.join(left))
            return 1
        features = len(doc.get('features', []))
        if features != int(argv[2]):
            print('the saved file has %d features, not %s' % (features, argv[2]))
            return 1
        print('ok   saved without the CAM and the newer field, with %d features' % features)
        return 0
    print(__doc__)
    return 2


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
