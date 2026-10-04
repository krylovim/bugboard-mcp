#!/usr/bin/env python3
"""Build a local Windows Bugboard package from an explicit file allowlist.

Added by krylovim, 2026. Apache-2.0 + Commons Clause; see repository LICENSE.
The skill is read from Git objects at one committed revision, never its worktree.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import subprocess
import tempfile
import zipfile

REPO = Path(__file__).resolve().parents[1]
TEMPLATES = (
    '.codex-plugin/plugin.json', '.mcp.json', 'README.md',
    'auth/package.json', 'auth/package-lock.json',
    'scripts/common.ps1', 'scripts/install.ps1', 'scripts/launch.ps1',
    'scripts/login.ps1', 'scripts/rollback.ps1',
)
SKILL_PREFIX = 'skills/bugboard-diagnostics/'
SKILL_FILES = frozenset(('SKILL.md', 'agents/openai.yaml', 'references/binding.md',
    'references/search-routing.md', 'references/validation.md', 'scripts/binding.py'))
MANIFEST = 'bundle-manifest.json'


def digest(data):
    return hashlib.sha256(data).hexdigest()


def safe_relative(value):
    path = PurePosixPath(value)
    if (not value or path.is_absolute() or '\\' in value or ':' in value
            or any(part in ('', '.', '..') for part in value.split('/'))):
        raise ValueError('Unsafe package path')
    return path


def reject_link(path):
    info = path.lstat()
    if stat.S_ISLNK(info.st_mode) or getattr(info, 'st_file_attributes', 0) & 0x400:
        raise ValueError('Symlinks and reparse points are not package inputs')


def source_bytes(path):
    path = path.absolute()
    for ancestor in [path, *path.parents]:
        reject_link(ancestor)
    if not path.is_file():
        raise ValueError('Package input is not a regular file')
    return path.read_bytes()


def write_file(root, relative, data):
    target = root.joinpath(*safe_relative(relative).parts)
    target.parent.mkdir(parents=True, exist_ok=True)
    with target.open('xb') as output:
        output.write(data)


def write_json(root, relative, value):
    write_file(root, relative, (json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + '\n').encode('utf-8'))


def git(repo, *args):
    result = subprocess.run(['git', '-C', str(repo), *args], capture_output=True)
    if result.returncode:
        raise ValueError('Cannot read committed skill snapshot; check Git repository access')
    return result.stdout


def skill_snapshot(repo):
    commit = git(repo, 'rev-parse', '--verify', 'HEAD^{commit}').decode('ascii').strip()
    entries = git(repo, 'ls-tree', '-r', '-z', commit, '--', SKILL_PREFIX)
    result = {}
    for entry in entries.split(b'\0'):
        if not entry:
            continue
        metadata, raw_name = entry.split(b'\t', 1)
        mode, kind, oid = metadata.decode('ascii').split()
        name = raw_name.decode('utf-8')
        if not name.startswith(SKILL_PREFIX):
            raise ValueError('Unexpected skill path')
        relative = name[len(SKILL_PREFIX):]
        parts = safe_relative(relative).parts
        if '__pycache__' in parts or relative.endswith(('.pyc', '.pyo')):
            continue
        if mode not in ('100644', '100755') or kind != 'blob':
            raise ValueError('Skill snapshot contains a symlink or non-file entry')
        if relative == 'scripts/test_binding.py':
            continue
        if relative not in SKILL_FILES and relative not in ('LICENSE', 'NOTICE'):
            raise ValueError('Unreviewed file in committed skill snapshot')
        result[relative] = git(repo, 'cat-file', 'blob', oid)
    if 'SKILL.md' not in result:
        raise ValueError('Committed Bugboard skill is missing')
    return commit, result


def inventory(root):
    reject_link(root)
    result = {}
    seen = set()
    for directory, dirs, files in os.walk(root, followlinks=False):
        for name in [*dirs, *files]:
            path = Path(directory) / name
            reject_link(path)
        for name in files:
            path = Path(directory) / name
            relative = path.relative_to(root).as_posix()
            safe_relative(relative)
            if relative.casefold() in seen:
                raise ValueError('Case-colliding package paths')
            seen.add(relative.casefold())
            result[relative] = digest(path.read_bytes())
    return dict(sorted(result.items()))


def install_dependencies(auth_root):
    npm = shutil.which('npm.cmd' if os.name == 'nt' else 'npm')
    if not npm:
        raise ValueError('npm is required to build the optional browser helper')
    env = dict(os.environ, PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD='1')
    result = subprocess.run([npm, 'ci', '--ignore-scripts', '--no-audit', '--no-fund'],
                            cwd=auth_root, env=env, capture_output=True)
    if result.returncode:
        raise ValueError('Pinned npm dependency installation failed')
    # These packages are the whole reviewed dependency tree. Do not package
    # random additions from a developer node_modules directory.
    modules = auth_root / 'node_modules'
    expected = {'.bin', '.package-lock.json', 'playwright', 'playwright-core', 'fsevents'}
    if not modules.is_dir() or any(p.name not in expected for p in modules.iterdir()):
        raise ValueError('Unexpected npm dependency tree')
    for package in ('playwright', 'playwright-core'):
        value = json.loads((modules / package / 'package.json').read_text('utf-8'))
        if value.get('version') != '1.62.1':
            raise ValueError('Unexpected Playwright version')


def verify_bundle(plugin):
    manifest = json.loads((plugin / MANIFEST).read_text('utf-8'))
    expected = manifest.get('files')
    if not isinstance(expected, dict) or not expected:
        raise ValueError('Invalid file manifest')
    for relative, checksum in expected.items():
        safe_relative(relative)
        if relative == MANIFEST or not re.fullmatch('[0-9a-f]{64}', checksum):
            raise ValueError('Invalid manifest entry')
    actual = inventory(plugin)
    actual.pop(MANIFEST, None)
    if actual != expected:
        raise ValueError('Package inventory or hash mismatch')
    return manifest


def deterministic_zip(root, output):
    if output.exists():
        raise ValueError('ZIP output already exists')
    files = inventory(root)
    with zipfile.ZipFile(output, 'x', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for relative in files:
            entry = zipfile.ZipInfo(relative, date_time=(1980, 1, 1, 0, 0, 0))
            entry.compress_type = zipfile.ZIP_DEFLATED
            entry.create_system = 3
            entry.external_attr = 0o100644 << 16
            archive.writestr(entry, root.joinpath(*safe_relative(relative).parts).read_bytes(), compresslevel=9)


def build(binary, binary_commit, skill_repo, output, version='0.2.0', repo=REPO,
          dependency_installer=install_dependencies):
    if not re.fullmatch('[0-9a-fA-F]{40}', binary_commit):
        raise ValueError('--binary-commit must be a full Git commit hash')
    if not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9.-]+)?', version):
        raise ValueError('Invalid package version')
    output = output.absolute()
    if output.exists() or output.is_symlink():
        raise ValueError('Package output already exists')
    binary_bytes = source_bytes(binary)
    if not binary_bytes.startswith(b'MZ'):
        raise ValueError('Expected a Windows PE executable')
    skill_commit, skill_files = skill_snapshot(skill_repo)
    output.parent.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix='.bugboard-package-', dir=output.parent)).resolve()
    try:
        plugin = stage / 'plugin'
        plugin.mkdir()
        for relative in TEMPLATES:
            data = source_bytes(repo / 'plugin' / relative)
            if relative == '.codex-plugin/plugin.json':
                value = json.loads(data)
                value['version'] = version
                data = (json.dumps(value, ensure_ascii=False, indent=2) + '\n').encode('utf-8')
            write_file(plugin, relative, data)
        write_file(plugin, 'runtime/bugboard-mcp.exe', binary_bytes)
        write_file(plugin, 'auth/browser-login.cjs', source_bytes(repo / 'scripts/browser-login.cjs'))
        write_file(plugin, 'scripts/configure-plugin.py', source_bytes(repo / 'scripts/configure-plugin.py'))
        write_file(plugin, 'scripts/register-plugin.py', source_bytes(repo / 'scripts/register-plugin.py'))
        write_file(plugin, 'LICENSE', source_bytes(repo / 'LICENSE'))
        if (repo / 'NOTICE').is_file():
            write_file(plugin, 'NOTICE', source_bytes(repo / 'NOTICE'))
        for relative, data in sorted(skill_files.items()):
            write_file(plugin, 'skills/bugboard-diagnostics/' + relative, data)
        write_json(plugin, 'skill-provenance.json', {
            'repository': 'https://gitlab.itprogress.ru/krylovim/skills',
            'commit': skill_commit, 'source_path': SKILL_PREFIX.rstrip('/'),
            'snapshot': 'committed Git blobs; untracked and working-tree changes excluded',
            'distribution': 'Local use by repository owner only. No independent public redistribution grant identified.',
            'files': {name: digest(data) for name, data in sorted(skill_files.items())},
        })
        write_file(plugin, 'ATTRIBUTION.md', (
            '# Attribution and local distribution\n\n'
            'Bugboard MCP: bapho-bush upstream and krylovim fork contributors.\n'
            'Apache-2.0 with Commons Clause 1.0; preserve the complete LICENSE.\n'
            'Plugin packaging and modifications by krylovim, 2026.\n\n'
            'The Bugboard diagnostics skill is a committed snapshot from the owner\'s practical-skills repository.\n'
            'See skill-provenance.json. No independent public redistribution license was identified;\n'
            'this local-use bundle is not authorized for public redistribution by this build script.\n\n'
            'Playwright and playwright-core retain their upstream LICENSE/NOTICE files under auth/node_modules.\n'
        ).encode('utf-8'))
        dependency_installer(plugin / 'auth')
        write_json(stage, '.agents/plugins/marketplace.json', {
            'name': 'bugboard-local', 'interface': {'displayName': 'Bugboard local'},
            'plugins': [{'name': 'bugboard', 'source': {'source': 'local', 'path': './plugin'},
                         'category': 'Productivity',
                         'policy': {'installation': 'AVAILABLE', 'authentication': 'ON_INSTALL'}}],
        })
        write_json(plugin, MANIFEST, {
            'version': version, 'runtime_commit': binary_commit.lower(), 'skill_commit': skill_commit,
            'files': inventory(plugin),
        })
        verify_bundle(plugin)
        if output.exists():
            raise ValueError('Package output already exists')
        stage.rename(output)
        return output
    finally:
        if stage.exists():
            # Delete only the fresh temporary directory allocated above; never
            # a caller-provided output or an unverified computed directory.
            if stage.parent != output.parent.resolve() or not stage.name.startswith('.bugboard-package-'):
                raise ValueError('Unsafe temporary cleanup path')
            reject_link(stage)
            shutil.rmtree(stage)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--binary-commit', required=True, help='Commit from which the supplied binary was built')
    parser.add_argument('--skill-repo', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--version', default='0.2.0')
    parser.add_argument('--zip', type=Path, help='Optional deterministic ZIP; must not already exist')
    args = parser.parse_args()
    if args.zip and args.zip.exists():
        parser.error('ZIP output already exists')
    output = build(args.binary, args.binary_commit, args.skill_repo, args.output, args.version)
    if args.zip:
        deterministic_zip(output, args.zip)
    print(json.dumps({'package': str(output), 'version': args.version,
                      'files': len(verify_bundle(output / 'plugin')['files']),
                      'zip': str(args.zip) if args.zip else None}))


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        raise SystemExit(f'Package build failed: {error}')
