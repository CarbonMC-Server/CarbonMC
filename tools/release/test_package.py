import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path
import zipfile

from package import SOURCE_FILES, archive, canonical, sha, source_files
from smoke import unpack, verify_files


class PackageTests(unittest.TestCase):
    def test_archive_is_identical_regardless_of_input_order(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive(root / 'one.zip', {'pkg/b': b'b', 'pkg/carbon': b'a'})
            archive(root / 'two.zip', {'pkg/carbon': b'a', 'pkg/b': b'b'})
            self.assertEqual((root / 'one.zip').read_bytes(), (root / 'two.zip').read_bytes())
            with zipfile.ZipFile(root / 'one.zip') as result:
                self.assertEqual(result.getinfo('pkg/carbon').external_attr >> 16 & 0o777, 0o755)

    def test_source_selection_excludes_private_and_reference_directories(self):
        files = source_files()
        self.assertIn('Cargo.lock', files)
        self.assertIn('crates/carbon-protocol/assets/configuration-26.2.bin', files)
        for name in files:
            self.assertNotIn(name.split('/')[0], {'server', 'Pumpkin-ref', 'logs', 'target', 'dist', 'work', 'outputs'})
            self.assertFalse(name.startswith('world-save'))

    def test_release_selection_is_independent_of_agent_instructions(self):
        with tempfile.TemporaryDirectory() as temporary:
            # Match production ROOT after Windows short-name/junction resolution.
            root = Path(temporary).resolve()
            for name in SOURCE_FILES:
                if name == 'AGENTS.md':
                    continue
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('release input', encoding='utf-8')
            with patch('package.ROOT', root):
                without_guidance = source_files()
                self.assertNotIn('AGENTS.md', without_guidance)
                (root / 'AGENTS.md').write_text('local-only guidance', encoding='utf-8')
                self.assertEqual(source_files(), without_guidance)

    def test_missing_required_build_input_still_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            # Match production ROOT after Windows short-name/junction resolution.
            root = Path(temporary).resolve()
            for name in SOURCE_FILES:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('release input', encoding='utf-8')
            (root / 'Cargo.lock').unlink()
            with patch('package.ROOT', root):
                with self.assertRaisesRegex(ValueError, 'Unsafe or missing source input: Cargo.lock'):
                    source_files()

    def test_tampering_and_unlisted_files_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'carbon').write_bytes(b'original')
            (root / 'FILES.sha256').write_text(f'{sha(b"original")}  carbon\n')
            verify_files(root)
            (root / 'carbon').write_bytes(b'changed')
            with self.assertRaises(ValueError):
                verify_files(root)
            (root / 'carbon').write_bytes(b'original')
            (root / 'world-save.json').write_text('{}')
            with self.assertRaises(ValueError):
                verify_files(root)

    def test_archive_traversal_and_symlink_are_rejected(self):
        for name, mode in [('pkg/../../escape', 0o100644), ('/absolute', 0o100644), ('pkg/link', 0o120777)]:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                with zipfile.ZipFile(root / 'bad.zip', 'w') as output:
                    info = zipfile.ZipInfo(name)
                    info.external_attr = mode << 16
                    output.writestr(info, b'target')
                with self.assertRaises(ValueError):
                    unpack(root / 'bad.zip', root / 'extracted')
                self.assertFalse((root / 'extracted').exists())

    def test_source_digest_detects_content_and_name_changes(self):
        self.assertNotEqual(sha(canonical({'a': sha(b'1')})), sha(canonical({'a': sha(b'2')})))
        self.assertNotEqual(sha(canonical({'a': sha(b'1')})), sha(canonical({'b': sha(b'1')})))


if __name__ == '__main__':
    unittest.main()
