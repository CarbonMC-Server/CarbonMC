"""Disposable packaged save/upgrade and process-crash acceptance (not power loss)."""
import copy
import hashlib
import json
import os
import platform
import shutil
import subprocess
import time


def sample_save():
    stack = {'kind': 'diamond_pickaxe', 'count': 1, 'damage': 17}
    return {
        'version': 2, 'generator_version': 1,
        'blocks': [{'dimension': dim, 'x': x, 'y': 150, 'z': 48, 'kind': kind}
                   for dim, x, kind in [('minecraft:overworld', 48, 'diamond_ore'),
                                        ('minecraft:the_nether', 49, 'obsidian'),
                                        ('minecraft:the_end', 50, 'end_stone'),
                                        ('minecraft:overworld', 51, 'chest'),
                                        ('minecraft:overworld', 52, 'furnace')]],
        'inventories': [{'player_id': '00000000-0000-4000-8000-000000000123',
                         'slots': [stack] + [None] * 35, 'health': 13.0, 'food': 9,
                         'saturation': 2.0, 'armor': [None] * 4, 'selected_slot': 0,
                         'sharpness_levels': [0] * 36,
                         'off_hand': {'kind': 'shield', 'count': 1, 'damage': 9},
                         'effects': [{'kind': 'fire_resistance', 'amplifier': 0, 'remaining_ticks': 72000}],
                         'location': {'dimension': 'minecraft:overworld', 'x': 48, 'y': 151, 'z': 48}}],
        'chests': [{'dimension': 'minecraft:overworld', 'x': 51, 'y': 150, 'z': 48,
                    'slots': [stack] + [None] * 26, 'sharpness_levels': [0] * 27}],
        'furnaces': [{'dimension': 'minecraft:overworld', 'x': 52, 'y': 150, 'z': 48,
                      'input': None, 'fuel': None, 'output': {'kind': 'iron_ingot', 'count': 3, 'damage': 0},
                      'burn_remaining': 0, 'burn_total': 0, 'cook_progress': 0}]}


def verify_state(value):
    expected = sample_save()
    if (value['version'], value['generator_version']) != (2, 1):
        raise AssertionError('Save did not migrate to schema 2 / generator 1')
    for name in ('blocks', 'chests', 'furnaces'):
        def order(row):
            return row.get('dimension', ''), row['x'], row['y'], row['z']
        if sorted(value[name], key=order) != sorted(expected[name], key=order):
            raise AssertionError(f'{name} witness changed: {value[name]}')
    actual = copy.deepcopy(value['inventories'])
    if len(actual) != 1:
        raise AssertionError('Inventory witness missing')
    effects = actual[0]['effects']
    if len(effects) != 1 or not 71000 <= effects[0]['remaining_ticks'] <= 72000:
        raise AssertionError('Effect witness lost or duration increased')
    effects[0]['remaining_ticks'] = 72000
    if actual != expected['inventories']:
        raise AssertionError(f'Player/item/equipment/location witness changed: {actual}')


def run_save_acceptance(bundle, run, env):
    env = env.copy()
    env.pop('RUST_LOG', None)
    root = run / 'save-acceptance'
    root.mkdir()
    exe_name = 'carbon.exe' if os.name == 'nt' else 'carbon'
    launcher_name = 'start-carbon.cmd' if os.name == 'nt' else 'start-carbon.sh'
    report = {'platform': platform.platform(), 'binary_sha256': hashlib.sha256((bundle / exe_name).read_bytes()).hexdigest(),
              'checks': [], 'status': 'running',
              'scope': 'Real process kills and packaged restore/upgrade tests; not hardware power-loss or real-client acceptance.'}

    def write(path, value):
        path.write_text(json.dumps(value), encoding='utf-8')

    def fresh(name, value=None):
        case = root / name
        case.mkdir()
        for filename in (exe_name, launcher_name, 'Carbon.toml'):
            shutil.copy2(bundle / filename, case / filename)
        config = (case / 'Carbon.toml').read_text(encoding='utf-8').replace('127.0.0.1:25565', '127.0.0.1:0')
        (case / 'Carbon.toml').write_text(config, encoding='utf-8')
        if value is not None:
            write(case / 'world-save.json', value)
        return case

    def saved(case):
        return json.loads((case / 'world-save.json').read_text(encoding='utf-8'))

    def files(case):
        return {p.name: p.read_bytes() for p in case.glob('world-save*') if p.is_file()}

    def start_stop(case, success=True):
        result = subprocess.run([str(case / launcher_name)], cwd=root, env=env,
                                input='stop\n', capture_output=True, text=True, encoding='utf-8', errors='replace', timeout=30)
        output = result.stdout + result.stderr
        report['checks'].append({'case': case.name, 'action': 'start_stop', 'exit': result.returncode, 'output': output})
        if (result.returncode == 0) != success:
            raise AssertionError(f'{case.name}: unexpected exit {result.returncode}: {output}')
        if success:
            if 'server stopped cleanly' not in output:
                raise AssertionError('Missing clean-shutdown acknowledgement')
            verify_state(saved(case))

    def crash(case, after_autosave):
        log = case / 'crash.log'
        with log.open('w', encoding='utf-8') as output:
            process = subprocess.Popen([str(case / exe_name)], cwd=case, env=env,
                                       stdin=subprocess.PIPE, stdout=output, stderr=subprocess.STDOUT)
            try:
                deadline = time.monotonic() + 35
                while True:
                    if process.poll() is not None:
                        raise AssertionError('Server exited before crash: ' + log.read_text(encoding='utf-8'))
                    ready = 'server started' in log.read_text(encoding='utf-8')
                    if after_autosave:
                        try:
                            ready = ready and saved(case).get('version') == 2
                        except (OSError, json.JSONDecodeError):
                            ready = False
                    if ready:
                        break
                    if time.monotonic() > deadline:
                        raise AssertionError('Timed out waiting for startup/autosave')
                    time.sleep(0.025)
                process.kill()
                code = process.wait(timeout=10)
                if code == 0:
                    raise AssertionError('Crash exited successfully')
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait(timeout=10)
                process.stdin.close()
        output = log.read_text(encoding='utf-8')
        if 'server stopped cleanly' in output:
            raise AssertionError('Process unexpectedly shut down gracefully')
        report['checks'].append({'case': case.name, 'action': 'forced_process_kill', 'after_autosave': after_autosave,
                                 'exit': code, 'output': output})

    try:
        legacy = sample_save()
        legacy['version'] = 1
        del legacy['generator_version']
        case = fresh('legacy-upgrade', legacy)
        start_stop(case)
        start_stop(case)
        snapshot = root / 'independent-snapshot'
        shutil.copytree(case, snapshot)
        snapshot_files = files(snapshot)
        restored = root / 'snapshot-restored'
        shutil.copytree(snapshot, restored)
        start_stop(restored)
        if files(snapshot) != snapshot_files:
            raise AssertionError('Independent snapshot was modified')
        report['checks'].append({'case': 'snapshot-restored', 'action': 'snapshot_unchanged', 'passed': True})
        for name, primary in [('corrupt-primary', b'{torn'), ('missing-primary', None)]:
            case = fresh(name)
            if primary is not None:
                (case / 'world-save.json').write_bytes(primary)
            write(case / 'world-save.json.bak', sample_save())
            uncommitted = sample_save()
            uncommitted['blocks'][0]['kind'] = 'dirt'
            write(case / 'world-save.json.tmp', uncommitted)
            start_stop(case)
            if primary is not None:
                evidence = list(case.glob('world-save.json.corrupt-*'))
                if len(evidence) != 1 or evidence[0].read_bytes() != primary:
                    raise AssertionError('Corrupt primary evidence was lost')
            start_stop(case)
        for name, header in [('future-schema', {'version': 3}), ('future-generator', {'generator_version': 2})]:
            value = sample_save()
            value.update(header)
            case = fresh(name, value)
            write(case / 'world-save.json.bak', sample_save())
            before = files(case)
            start_stop(case, success=False)
            if files(case) != before:
                raise AssertionError('Incompatible save files were changed')
        case = fresh('no-valid-backup')
        (case / 'world-save.json').write_bytes(b'{torn')
        (case / 'world-save.json.bak').write_bytes(b'{also-torn')
        before = files(case)
        start_stop(case, success=False)
        if files(case) != before:
            raise AssertionError('Failed recovery changed evidence')
        for after_autosave in (False, True):
            case = fresh('crash-after-autosave' if after_autosave else 'crash-before-autosave', legacy)
            before = (case / 'world-save.json').read_bytes()
            crash(case, after_autosave)
            if not after_autosave and (case / 'world-save.json').read_bytes() != before:
                raise AssertionError('Pre-autosave crash unexpectedly committed state')
            start_stop(case)
            start_stop(case)
        report['status'] = 'passed'
    except Exception as error:
        report['status'] = 'failed'
        report['error'] = str(error)
        raise
    finally:
        (run / 'save-acceptance.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    return report
