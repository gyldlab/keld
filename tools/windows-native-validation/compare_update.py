"""Read-only Update source census; hashes are observations, never edit approval."""
import json
import sys
from pathlib import Path
from preview_safety import read_plain, reject_alias_components, sha

def main():
    if len(sys.argv) != 2:
        raise ValueError('Expected repository path')
    repo = Path(sys.argv[1])
    if not repo.is_absolute():
        raise ValueError('Absolute repository required')
    reject_alias_components(repo)
    index = json.loads(read_plain(Path(__file__).absolute().parent / 'update-candidates.json'))
    if index.get('schema') != 1 or len(index['files']) > 64:
        raise ValueError('Unsupported bounded census')
    for name, candidates in index['files'].items():
        path = Path(name)
        if path.is_absolute() or '..' in path.parts or not name.startswith('crates/keld-update/'):
            raise ValueError('Census path outside Update')
        try:
            raw = read_plain(repo / path)
        except FileNotFoundError:
            print(json.dumps({'path': name, 'absent': True}), flush=True)
            continue
        digest, lf = sha(raw), sha(raw.replace(b'\r\n', b'\n'))
        if read_plain(repo / path) != raw:
            raise ValueError('Source changed during census: ' + name)
        print(json.dumps({'path': name, 'bytes': len(raw), 'raw': digest, 'lf': lf,
                          'candidates': candidates.get(lf, [])}), flush=True)

if __name__ == '__main__':
    main()
