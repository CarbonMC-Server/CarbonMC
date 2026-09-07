"""Verify documented CLI/working-directory behavior using an isolated local bundle.
Run: python tools/verify_release_docs.py --binary target/release/carbon.exe
Only disposable directories below work/release-docs are used; no public listener.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument('--binary', required=True)
args = parser.parse_args()
binary = Path(args.binary).resolve(strict=True)
base = ROOT / 'work' / 'release-docs'
base.mkdir(parents=True, exist_ok=True)
run = Path(tempfile.mkdtemp(prefix='verified-', dir=base))
bundle = run / 'bundle'
bundle.mkdir()
exe = bundle / binary.name
shutil.copy2(binary, exe)
docs = ['README.md', 'RELEASE_NOTES.md', 'OPERATIONS.md', 'SUPPORT.md',
        'SAVE_COMPATIBILITY.md', 'RELEASE_CHECKLIST.md', 'RELEASE_READINESS.md',
        'REFERENCE_POLICY.md', 'ROADMAP.md', 'LICENSE', 'Carbon.toml']
for name in docs:
    shutil.copy2(ROOT / name, bundle / name)
checks = []
env = os.environ.copy()
env.pop('RUST_LOG', None)

def invoke(arguments, cwd=bundle, commands=None, ok=True, contains=None):
    result = subprocess.run([str(exe), *arguments], cwd=cwd, input=commands,
                            capture_output=True, text=True, encoding='utf-8', timeout=30, env=env)
    text = result.stdout + result.stderr
    assert (result.returncode == 0) == ok, (arguments, result.returncode, text)
    if contains:
        assert contains in text, (contains, text)
    checks.append({'arguments': arguments, 'cwd': str(cwd), 'exit': result.returncode,
                   'expected_success': ok, 'output': text})
    return text

invoke(['--help'], contains='Usage: carbon')
invoke(['--config', 'Carbon.toml', '--check'], contains='Configuration is valid')
assert not (bundle / 'world-save.json').exists()
invoke(['--config', 'missing.toml', '--check'], ok=False, contains='failed to load')
invoke(['--version'], ok=False, contains='unknown argument')
invalid = run / 'unknown.toml'
invalid.write_text('[server]\nunknown_setting = true\n', encoding='utf-8')
invoke(['--config', str(invalid), '--check'], ok=False, contains='unknown field')
invalid.write_text('[server]\nmax_players = 0\n', encoding='utf-8')
invoke(['--config', str(invalid), '--check'], ok=False, contains='max_players')
# Ephemeral loopback port avoids the live server and fixed-port conflicts.
config = run / 'isolated.toml'
config.write_text((bundle / 'Carbon.toml').read_text().replace('127.0.0.1:25565', '127.0.0.1:0'), encoding='utf-8')
console = invoke(['--config', str(config)], commands='version\nhelp\nlist\nstop\n', contains='server stopped cleanly')
assert 'Carbon 0.1.0 (original Rust implementation)' in console
assert 'Lists available commands' in console
assert 'There are no connected players.' in console
primary = bundle / 'world-save.json'
saved = json.loads(primary.read_text())
assert (saved['version'], saved['generator_version']) == (2, 1)
assert not (run / 'world-save.json').exists(), 'Save incorrectly followed config directory'
original = primary.read_bytes()
invoke(['--config', str(config)], commands='stop\n', contains='server stopped cleanly')
assert (bundle / 'world-save.json.bak').read_bytes() == original
# Verify documented backup restore on this executable, not just the reader unit test.
snapshot = run / 'snapshot'
shutil.copytree(bundle, snapshot)
primary.write_text('{torn', encoding='utf-8')
invoke(['--config', str(config)], commands='stop\n', contains='server stopped cleanly')
assert json.loads(primary.read_text())['version'] == 2
assert list(bundle.glob('world-save.json.corrupt-*'))
restored = run / 'restored'
shutil.copytree(snapshot, restored)
invoke(['--config', str(config)], cwd=restored, commands='stop\n', contains='server stopped cleanly')
assert json.loads((restored / 'world-save.json').read_text()) == saved
report = {'platform': platform.platform(), 'binary_sha256': hashlib.sha256(exe.read_bytes()).hexdigest(),
          'source_binary': str(binary), 'bundle': str(bundle), 'checks': checks,
          'assertions': ['--check creates no save', 'save uses working directory, not config directory',
                         'schema 2/generator 1 on clean stop', 'restart rotates backup',
                         'corrupt primary recovery and quarantine', 'snapshot copy restored and restarted'],
          'scope': 'Local documentation verification; not fresh-machine, gameplay, power-loss, or distribution acceptance'}
(run / 'verification.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
print(json.dumps({k:v for k,v in report.items() if k != 'checks'}, indent=2))
print(f'{len(checks)} process invocations passed. Evidence: {run / "verification.json"}')
