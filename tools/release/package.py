"""Build native, checksummed review packages from an explicit source allowlist.
Python 3.11+; no third-party Python dependencies. Never publishes or commits.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[2]
DOCS = ['README.md', 'LICENSE', 'LICENSE-MIT-HISTORY', 'Carbon.toml', 'OPERATIONS.md', 'SUPPORT.md',
        'RELEASE_NOTES.md', 'RELEASE_CHECKLIST.md', 'RELEASE_READINESS.md',
        'SAVE_COMPATIBILITY.md', 'ROADMAP.md', 'REFERENCE_POLICY.md', 'RELEASE_BUILD.md']
SOURCE_ROOTS = ['crates', 'extensions', '.github', 'tools/release']
# Agent instructions are local development guidance, not release/build inputs.
SOURCE_FILES = DOCS + ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.gitignore',
                      '.gitattributes', 'tools/verify_release_docs.py']


def sha(data):
    return hashlib.sha256(data).hexdigest()


def canonical(value):
    return (json.dumps(value, indent=2, sort_keys=True) + '\n').encode()


def command(args, **kwargs):
    return subprocess.check_output(args, cwd=ROOT, text=True, encoding='utf-8', **kwargs).strip()


def source_files():
    files = set(SOURCE_FILES)
    for directory in SOURCE_ROOTS:
        for path in (ROOT / directory).rglob('*'):
            if path.is_file() and '__pycache__' not in path.parts:
                if path.suffix not in {'.rs', '.toml', '.lock', '.bin', '.md', '.yml', '.yaml', '.py', '.sh', '.cmd'}:
                    raise ValueError(f'Unexpected source input: {path}')
                files.add(path.relative_to(ROOT).as_posix())
    for name in files:
        path = ROOT / name
        if not path.is_file() or path.is_symlink() or not path.resolve().is_relative_to(ROOT):
            raise ValueError(f'Unsafe or missing source input: {name}')
    return sorted(files)


def archive(path, members):
    # Stored ZIP avoids zlib-version differences; fixed times/order/modes.
    with zipfile.ZipFile(path, 'x', compression=zipfile.ZIP_STORED) as output:
        for name, data in sorted(members.items()):
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            mode = 0o755 if name.endswith('/carbon') or name.endswith('.sh') else 0o644
            info.external_attr = (0o100000 | mode) << 16
            output.writestr(info, data)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True, help='New empty output directory')
    parser.add_argument('--build-dir', type=Path, required=True)
    parser.add_argument('--allow-unversioned', action='store_true', help='Explicitly label local review snapshot; never a release baseline')
    parser.add_argument('--llvm-notice', type=Path, help='Required LLVM/MinGW notice for local gnullvm static runtime')
    args = parser.parse_args()
    settings = tomllib.loads((ROOT / 'tools/release/settings.toml').read_text(encoding='utf-8-sig'))['release']
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']
    rustc = command(['rustc', '-vV'])
    fields = dict(line.split(': ', 1) for line in rustc.splitlines() if ': ' in line)
    target = fields['host']
    if fields['release'] != settings['rust_version'] or target not in settings['targets']:
        raise ValueError(f'Need Rust {settings["rust_version"]} on a configured native target; got {fields}')
    if target.endswith('gnullvm') and not args.llvm_notice:
        raise ValueError('Local gnullvm packages require --llvm-notice for the static LLVM runtime')
    files = source_files()
    original = {name: (ROOT / name).read_bytes() for name in files}
    hashes = {name: sha(data) for name, data in original.items()}
    source_hash = sha(canonical(hashes))
    revision = subprocess.run(['git', 'rev-parse', '--verify', 'HEAD'], cwd=ROOT, capture_output=True, text=True)
    dirty = command(['git', 'status', '--porcelain', '--untracked-files=all'])
    if (revision.returncode != 0 or dirty) and not args.allow_unversioned:
        raise ValueError('Release builds require a committed clean source tree. Use --allow-unversioned only for local review.')
    # Clean Git is not a human-review claim; the release owner records approval separately.
    baseline = 'local-review' if args.allow_unversioned else 'committed'
    commit = revision.stdout.strip() if revision.returncode == 0 else None
    if not args.allow_unversioned:
        for name in files:
            subprocess.run(['git', 'ls-files', '--error-unmatch', name], cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
    label = f'carbon-{version}-{baseline}-{source_hash[:12]}-{target}'
    output = args.output.resolve()
    if output.exists() and any(output.iterdir()):
        raise ValueError('Output directory must be empty; existing artifacts are never overwritten')
    output.mkdir(parents=True, exist_ok=True)
    build_dir = args.build_dir.resolve()
    if not build_dir.is_relative_to(ROOT / 'work'):
        raise ValueError('Build directory must be below this checkout/work')
    flags = [f'--remap-path-prefix={ROOT}=/carbon', f'--remap-path-prefix={build_dir}=/carbon-build']
    if target.endswith('msvc'):
        flags += ['-C', 'target-feature=+crt-static', '-C', 'link-arg=/Brepro']
    elif target.endswith('gnullvm'):
        flags += ['-C', 'target-feature=+crt-static', '-C', 'link-arg=-static', '-C', 'link-arg=-Wl,--no-insert-timestamp', '-C', 'link-arg=-Wno-unused-command-line-argument']
    else:
        flags += ['-C', 'link-arg=-Wl,--build-id=none']
    env = os.environ.copy()
    env.pop('RUSTFLAGS', None)
    env['CARGO_ENCODED_RUSTFLAGS'] = '\x1f'.join(flags)
    subprocess.run(['cargo', 'build', '--release', '--locked', '--bin', 'carbon',
                    '--target', target, '--target-dir', str(build_dir)], cwd=ROOT, env=env, check=True)
    executable = 'carbon.exe' if 'windows' in target else 'carbon'
    members = {name: original[name] for name in DOCS}
    members[executable] = (build_dir / target / 'release' / executable).read_bytes()
    launcher = 'start-carbon.cmd' if 'windows' in target else 'start-carbon.sh'
    members[launcher] = original['tools/release/' + launcher]
    metadata = json.loads(command(['cargo', 'metadata', '--locked', '--format-version', '1', '--filter-platform', target]))
    dependencies = []
    for package in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
        if package['source'] is None:
            continue
        directory = Path(package['manifest_path']).parent
        notices = []
        for path in sorted(directory.iterdir()):
            if path.is_file() and path.name.upper().startswith(('LICENSE', 'COPYING', 'NOTICE', 'COPYRIGHT')):
                name = f'licenses/{package["name"]}-{package["version"]}/{path.name}'
                members[name] = path.read_bytes()
                notices.append(name)
        if package.get('license_file'):
            path = (directory / package['license_file']).resolve()
            if not path.is_relative_to(directory):
                raise ValueError(f'Unexpected license_file outside dependency: {path}')
            name = f'licenses/{package["name"]}-{package["version"]}/{path.name}'
            members[name] = path.read_bytes()
            if name not in notices:
                notices.append(name)
        dependencies.append({'name': package['name'], 'version': package['version'],
                             'license': package.get('license'), 'source': package['source'], 'notice_files': notices})
    # Compiler-provided runtime notices, without depending on a rust-docs component.
    sysroot = Path(command(['rustc', '--print', 'sysroot']))
    rust_docs = sysroot / 'share/doc/rust'
    for path in sorted(rust_docs.rglob('*')):
        if path.is_file() and ('licenses' in path.parts or path.name.startswith('COPYRIGHT')):
            members['licenses/rust/' + path.relative_to(rust_docs).as_posix()] = path.read_bytes()
    if not any(name.startswith('licenses/rust/') for name in members):
        raise ValueError('Rust runtime notices missing from toolchain; install a complete toolchain')
    if args.llvm_notice:
        members['licenses/llvm-mingw/LICENSE.TXT'] = args.llvm_notice.read_bytes()
    members['DEPENDENCIES.json'] = canonical(dependencies)
    members['THIRD_PARTY_NOTICES.md'] = (
        '# Collected dependency notices\n\nSee DEPENDENCIES.json and licenses/ for Cargo dependency and Rust runtime notices. '
        'This inventory includes target/build dependencies and may be broader than the linked binary. '
        'It is not legal clearance. Embedded protocol fixtures and the full distribution provenance review remain gate 1 work. '
        'This is a review artifact, not authorization to redistribute.\n').encode()
    members['SOURCE_MANIFEST.json'] = canonical(hashes)
    members['BUILD_INFO.json'] = canonical({'package_version': version, 'target': target,
        'source_sha256': source_hash, 'git_commit': commit, 'baseline': baseline,
        'rustc': rustc, 'cargo': command(['cargo', '--version']), 'lock_sha256': hashes['Cargo.lock'],
        'reproducibility': 'Fixed archive metadata; independent binary reproducibility must be tested.',
        'distribution_approved': False})
    members['FILES.sha256'] = ''.join(f'{sha(data)}  {name}\n' for name,data in sorted(members.items())).encode()
    if source_files() != files or any((ROOT / name).read_bytes() != data for name,data in original.items()):
        raise ValueError('Source changed during build; retry from a stable snapshot')
    archive(output / (label + '.zip'), {f'{label}/{name}': data for name,data in members.items()})
    archive(output / (label + '-source.zip'), {f'{label}-source/{name}': data for name,data in original.items()})
    sums = ''.join(f'{sha(path.read_bytes())}  {path.name}\n' for path in sorted(output.glob('*.zip')))
    (output / 'SHA256SUMS').write_text(sums, encoding='utf-8', newline='\n')
    print(canonical({'package': label, 'output': str(output), 'source_sha256': source_hash,
                     'baseline': baseline, 'dependency_count': len(dependencies)}).decode())


if __name__ == '__main__':
    main()
