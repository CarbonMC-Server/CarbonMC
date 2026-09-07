"""Verify an extracted package without Cargo/Rust/LLVM on the child process PATH."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import subprocess
import tempfile
import zipfile


def digest(data):
    return hashlib.sha256(data).hexdigest()


def unpack(archive, destination):
    with zipfile.ZipFile(archive) as source:
        entries = source.infolist()
        names = [entry.filename for entry in entries]
        if len(set(names)) != len(names):
            raise ValueError('Duplicate archive members')
        for entry in entries:
            name = PurePosixPath(entry.filename)
            if name.is_absolute() or '..' in name.parts or '\\' in entry.filename or ':' in entry.filename:
                raise ValueError('Unsafe archive path')
            if (entry.external_attr >> 16) & 0o170000 not in (0, 0o100000):
                raise ValueError('Only regular files are accepted')
        roots = {PurePosixPath(name).parts[0] for name in names}
        if len(roots) != 1:
            raise ValueError('Expected one package directory')
        for entry in entries:
            path = destination / entry.filename
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(source.read(entry))
            if os.name != 'nt':
                path.chmod((entry.external_attr >> 16) & 0o777)
    return destination / roots.pop()


def verify_files(bundle):
    manifest = {}
    for line in (bundle / 'FILES.sha256').read_text().splitlines():
        expected, name = line.split('  ', 1)
        if name in manifest:
            raise ValueError('Duplicate hash entry')
        manifest[name] = expected
    actual = {p.relative_to(bundle).as_posix() for p in bundle.rglob('*') if p.is_file()}
    if actual != set(manifest) | {'FILES.sha256'}:
        raise ValueError('Unlisted or missing package file')
    for name, expected in manifest.items():
        if digest((bundle / name).read_bytes()) != expected:
            raise ValueError(f'Hash mismatch: {name}')
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifacts', type=Path, required=True)
    parser.add_argument('--work', type=Path, required=True)
    args = parser.parse_args()
    artifacts = args.artifacts.resolve()
    sums = {}
    for line in (artifacts / 'SHA256SUMS').read_text().splitlines():
        expected, name = line.split('  ', 1)
        if Path(name).name != name or '/' in name or '\\' in name or name in sums:
            raise ValueError('Unsafe checksum entry')
        sums[name] = expected
        if digest((artifacts / name).read_bytes()) != expected:
            raise ValueError(f'Archive hash mismatch: {name}')
    packages = [name for name in sums if name.endswith('.zip') and not name.endswith('-source.zip')]
    if len(packages) != 1:
        raise ValueError('Expected exactly one binary archive')
    args.work.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix='package smoke ', dir=args.work.resolve()))
    bundle = unpack(artifacts / packages[0], run)
    manifest = verify_files(bundle)
    info = json.loads((bundle / 'BUILD_INFO.json').read_text())
    config = bundle / 'Carbon.toml'
    if '127.0.0.1:25565' not in config.read_text() or 'online_mode = false' not in config.read_text():
        raise ValueError('Packaged config must explicitly use offline loopback defaults')
    exe = bundle / ('carbon.exe' if os.name == 'nt' else 'carbon')
    launcher = bundle / ('start-carbon.cmd' if os.name == 'nt' else 'start-carbon.sh')
    env = {k:v for k,v in os.environ.items() if k not in ('RUST_LOG', 'RUSTFLAGS', 'LD_LIBRARY_PATH', 'DYLD_LIBRARY_PATH')}
    system_root = os.environ.get('SystemRoot', '')
    env['PATH'] = os.pathsep.join([os.path.join(system_root, 'System32'), system_root]) if os.name == 'nt' else '/usr/bin:/bin'
    checks = []

    def invoke(command, commands=None, ok=True, expected=None):
        result = subprocess.run(command, cwd=run, env=env, input=commands, capture_output=True,
                                text=True, encoding='utf-8', errors='replace', timeout=30)
        output = result.stdout + result.stderr
        checks.append({'command': command, 'exit': result.returncode, 'output': output})
        if (result.returncode == 0) != ok or (expected and expected not in output):
            raise AssertionError(checks[-1])
        return output

    invoke([str(exe), '--help'], expected='Usage: carbon')
    invoke([str(launcher), '--check'], expected='Configuration is valid')
    assert not (bundle / 'world-save.json').exists()
    invoke([str(launcher), '--config', 'missing.toml', '--check'], ok=False, expected='failed to load')
    # Check failing config through the shipped launcher, including exit propagation.
    original = config.read_bytes()
    config.write_text('[server]\nmax_players = 0\n', encoding='utf-8')
    invoke([str(launcher), '--check'], ok=False, expected='max_players')
    config.write_bytes(original.replace(b'127.0.0.1:25565', b'127.0.0.1:0'))
    output = invoke([str(launcher)], commands='version\nlist\nstop\n', expected='server stopped cleanly')
    assert f'Carbon {info["package_version"]}' in output
    assert 'There are no connected players.' in output
    save = bundle / 'world-save.json'
    assert json.loads(save.read_text())['version'] == 2
    assert not (run / 'world-save.json').exists(), 'Launcher did not set package working directory'
    invoke([str(launcher)], commands='stop\n', expected='server stopped cleanly')
    assert (bundle / 'world-save.json.bak').is_file()
    save.write_text('{torn', encoding='utf-8')
    invoke([str(launcher)], commands='stop\n', expected='server stopped cleanly')
    assert json.loads(save.read_text())['generator_version'] == 1
    assert list(bundle.glob('world-save.json.corrupt-*'))
    report = {'platform': platform.platform(), 'package': packages[0], 'archive_sha256': sums[packages[0]],
              'build': info, 'file_count': len(manifest), 'checks': checks,
              'scope': 'Extracted artifact, sanitized runtime PATH; no real-client or power-loss acceptance.'}
    (run / 'result.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    print(f'PASS: {len(checks)} package process checks; {len(manifest)} file hashes; evidence {run / "result.json"}')


if __name__ == '__main__':
    main()
