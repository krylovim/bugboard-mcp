"""Package-boundary regressions; synthetic data only, no network/browser/session.
Added by krylovim, 2026. Repository LICENSE applies.
"""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location('package_builder', Path(__file__).with_name('build-plugin.py'))
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)


class PackageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='bugboard-package-test-')
        self.root = Path(self.temp.name)
        self.repo = self.root / 'source'
        self.repo.mkdir()
        for relative in builder.TEMPLATES:
            path = self.repo / 'plugin' / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('{}' if relative.endswith('.json') else 'reviewed template\n', encoding='utf-8')
        (self.repo / 'scripts').mkdir()
        for name in ['browser-login.cjs', 'configure-plugin.py', 'register-plugin.py']:
            (self.repo / 'scripts' / name).write_text('reviewed helper\n', encoding='utf-8')
        (self.repo / 'LICENSE').write_text('preserved upstream terms\n', encoding='utf-8')
        (self.repo / 'private.env').write_text('PRIVATE_REPOSITORY_SENTINEL', encoding='utf-8')
        self.binary = self.root / 'server.exe'
        self.binary.write_bytes(b'MZsynthetic test executable')
        self.skills = self.root / 'skills'
        self.skills.mkdir()
        self.git('init', '-q')
        self.git('config', 'user.name', 'Package Test')
        self.git('config', 'user.email', 'package-test@example.invalid')
        skill = self.skills / builder.SKILL_PREFIX
        skill.mkdir(parents=True)
        for name in builder.SKILL_FILES:
            target = skill / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text('COMMITTED_RUNTIME_CONTENT\n', encoding='utf-8')
        (skill / 'scripts/test_binding.py').write_text('TEST_FIXTURE_SENTINEL', encoding='utf-8')
        (self.skills / 'unrelated-private.txt').write_text('PRIVATE_OTHER_SKILL_SENTINEL', encoding='utf-8')
        self.git('add', '.')
        self.git('commit', '-qm', 'fixture')
        self.skill_commit = self.git('rev-parse', 'HEAD').strip()
        (skill / 'SKILL.md').write_text('UNCOMMITTED_SENTINEL', encoding='utf-8')
        (skill / 'private-cookie.env').write_text('UNTRACKED_SECRET_SENTINEL', encoding='utf-8')

    def tearDown(self):
        self.temp.cleanup()

    def git(self, *args):
        result = subprocess.run(['git', '-C', str(self.skills), *args], check=True, capture_output=True)
        return result.stdout.decode('utf-8')

    def build(self, name='out'):
        def synthetic_dependencies(auth):
            builder.write_file(auth, 'node_modules/playwright/LICENSE', b'upstream notice')
        return builder.build(self.binary, 'a' * 40, self.skills, self.root / name,
                             repo=self.repo, dependency_installer=synthetic_dependencies)

    def test_snapshot_allowlist_provenance_and_no_private_files(self):
        output = self.build()
        manifest = builder.verify_bundle(output / 'plugin')
        self.assertEqual(manifest['runtime_commit'], 'a' * 40)
        self.assertEqual(manifest['skill_commit'], self.skill_commit)
        self.assertEqual((output / 'plugin/skills/bugboard-diagnostics/SKILL.md').read_text(), 'COMMITTED_RUNTIME_CONTENT\n')
        all_bytes = b''.join(p.read_bytes() for p in output.rglob('*') if p.is_file())
        for secret in [b'PRIVATE_REPOSITORY_SENTINEL', b'PRIVATE_OTHER_SKILL_SENTINEL',
                       b'UNCOMMITTED_SENTINEL', b'UNTRACKED_SECRET_SENTINEL', b'TEST_FIXTURE_SENTINEL']:
            self.assertNotIn(secret, all_bytes)
        self.assertIn('scripts/configure-plugin.py', manifest['files'])

    def test_hash_tampering_and_extra_file_are_rejected(self):
        plugin = self.build() / 'plugin'
        binary = plugin / 'runtime/bugboard-mcp.exe'
        before = binary.read_bytes()
        binary.write_bytes(before + b'tampered')
        with self.assertRaisesRegex(ValueError, 'mismatch'):
            builder.verify_bundle(plugin)
        binary.write_bytes(before)
        (plugin / 'secret.env').write_text('synthetic')
        with self.assertRaisesRegex(ValueError, 'mismatch'):
            builder.verify_bundle(plugin)

    def test_deterministic_zip_exact_layout_and_existing_refusal(self):
        first = self.build('first')
        second = self.build('second')
        first_zip, second_zip = self.root / 'first.zip', self.root / 'second.zip'
        builder.deterministic_zip(first, first_zip)
        builder.deterministic_zip(second, second_zip)
        self.assertEqual(first_zip.read_bytes(), second_zip.read_bytes())
        with zipfile.ZipFile(first_zip) as archive:
            names = archive.namelist()
            self.assertIn('.agents/plugins/marketplace.json', names)
            self.assertIn('plugin/bundle-manifest.json', names)
            self.assertTrue(all(name.startswith('plugin/') or name == '.agents/plugins/marketplace.json' for name in names))
            self.assertTrue(all(info.date_time == (1980, 1, 1, 0, 0, 0) for info in archive.infolist()))
        with self.assertRaisesRegex(ValueError, 'already exists'):
            self.build('first')
        with self.assertRaisesRegex(ValueError, 'already exists'):
            builder.deterministic_zip(second, first_zip)

    def test_committed_symlink_rejected_without_reading_target(self):
        oid = subprocess.run(['git', '-C', str(self.skills), 'hash-object', '-w', '--stdin'],
                             input=b'../../private.env', capture_output=True, check=True).stdout.decode().strip()
        self.git('update-index', '--add', '--cacheinfo', f'120000,{oid},{builder.SKILL_PREFIX}link')
        self.git('commit', '-qm', 'symlink fixture')
        with self.assertRaisesRegex(ValueError, 'symlink'):
            self.build()

    def test_manifest_traversal_and_unreviewed_tracked_skill_rejected(self):
        plugin = self.build() / 'plugin'
        manifest = json.loads((plugin / builder.MANIFEST).read_text())
        manifest['files']['../private.env'] = '0' * 64
        (plugin / builder.MANIFEST).write_text(json.dumps(manifest))
        with self.assertRaisesRegex(ValueError, 'Unsafe'):
            builder.verify_bundle(plugin)
        self.git('add', builder.SKILL_PREFIX + 'private-cookie.env')
        self.git('commit', '-qm', 'unreviewed fixture')
        with self.assertRaisesRegex(ValueError, 'Unreviewed'):
            self.build('unreviewed')


if __name__ == '__main__':
    unittest.main()
