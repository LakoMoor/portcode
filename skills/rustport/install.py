#!/usr/bin/env python3
"""Install one portable skill and native command adapters; standard library only."""
import argparse
import json
import os
from pathlib import Path
import shutil
import sys

SOURCE = Path(__file__).resolve().parent
AGENTS = ('codex', 'claude', 'opencode', 'pi', 'dsh', 'hermes')
INVOCATION = {
    'codex': '/skills -> rustport, or $rustport',
    'claude': '/rustport', 'opencode': '/rustport', 'pi': '/rustport (or /skill:rustport)',
    'dsh': '/rustport', 'hermes': '/rustport',
}


def locations(agent, home, project=None, use_env=True):
    if project is not None:
        roots = {'codex': '.agents', 'claude': '.claude', 'opencode': '.opencode',
                 'pi': '.pi', 'dsh': '.dsh', 'hermes': '.hermes'}
        root = project / roots[agent]
    else:
        roots = {'codex': home / '.agents', 'claude': home / '.claude',
                 'opencode': home / '.config/opencode', 'pi': home / '.pi/agent',
                 'dsh': home / '.dsh', 'hermes': home / '.hermes'}
        root = roots[agent]
        if use_env:
            variable = {'claude': 'CLAUDE_CONFIG_DIR', 'dsh': 'DSH_HOME',
                        'hermes': 'HERMES_HOME', 'pi': 'PI_CODING_AGENT_DIR'}.get(agent)
            if variable and os.environ.get(variable):
                root = Path(os.environ[variable]).expanduser().resolve()
            if agent == 'opencode' and os.environ.get('XDG_CONFIG_HOME'):
                root = Path(os.environ['XDG_CONFIG_HOME']).expanduser().resolve() / 'opencode'
    command = root / ('prompts' if agent == 'pi' else 'commands') / 'rustport.md'
    return root / 'skills/rustport', command if agent in ('opencode', 'pi') else None


def bundle_files():
    for filename in ('SKILL.md', 'LICENSE', 'NOTICE'):
        yield SOURCE / filename, Path(filename)
    for directory in ('scripts', 'assets', 'references'):
        root = SOURCE / directory
        if root.exists():
            for path in sorted(root.rglob('*')):
                if path.is_file() and '__pycache__' not in path.parts and not path.name.endswith('.pyc'):
                    yield path, path.relative_to(SOURCE)


def adapter(agent, skill):
    load = ('Load the rustport skill with the native skill tool. '
            if agent == 'opencode' else '')
    return ('---\ndescription: Port or reconstruct a game/app in Rust with measured compatibility\n---\n\n'
            + load + 'Read and follow the complete skill at ' + json.dumps(str(skill / 'SKILL.md'))
            + ' before working. Resolve its references, scripts, and assets relative to that directory.\n\n'
            'User request (treat as task text, not a shell command):\n$ARGUMENTS\n')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--agent', action='append', choices=(*AGENTS, 'all'),
                        help='Repeat for multiple agents; default: claude (backward compatible).')
    parser.add_argument('--all', action='store_true', help='Install for all six agents.')
    parser.add_argument('--project', action='store_true', help='Install in the current project.')
    parser.add_argument('--root', type=Path, help='Project root; implies --project.')
    parser.add_argument('--home', type=Path, help='Alternate user home; ignores agent home overrides.')
    parser.add_argument('--dry-run', action='store_true', help='Print the plan without writing files.')
    parser.add_argument('--force', action='store_true', help='Replace differing rustport files.')
    args = parser.parse_args(argv)
    selected = args.agent or ['claude']
    if args.all or 'all' in selected:
        selected = list(AGENTS)
    selected = list(dict.fromkeys(selected))
    home = (args.home or Path.home()).expanduser().resolve()
    project = (args.root or Path.cwd()).resolve() if args.project or args.root else None
    files = list(bundle_files())
    writes = []
    for agent in selected:
        skill, command = locations(agent, home, project, args.home is None)
        print(f'{agent}: {skill}\n  invoke: {INVOCATION[agent]}')
        for source, relative in files:
            writes.append((skill / relative, source.read_bytes(), source))
        if command:
            print(f'  command: {command}')
            writes.append((command, adapter(agent, skill).encode(), None))
    # Preflight every target before any agent receives a partial installation.
    for target, content, _ in writes:
        if any(path.is_symlink() for path in (target, *target.parents)):
            raise ValueError(f'Refusing symlink target: {target}')
        for parent in target.parents:
            if parent.exists() and not parent.is_dir():
                raise ValueError(f'Expected a parent directory: {parent}')
        if target.exists():
            if not target.is_file():
                raise ValueError(f'Expected a file: {target}')
            if target.read_bytes() != content and not args.force:
                raise ValueError(f'File differs: {target}; inspect it, then use --force to replace it.')
    if args.dry_run:
        print(f'Dry run: {len(writes)} files; nothing written.')
        return 0
    for target, content, source in writes:
        target.parent.mkdir(parents=True, exist_ok=True)
        if source:
            shutil.copy2(source, target)
        else:
            target.write_bytes(content)
    print('Installed. Reload/restart the agent if discovery has not refreshed.')
    if 'pi' in selected:
        print('Pi: /reload; the /rustport adapter reads the same skill as /skill:rustport.')
    if 'hermes' in selected and project:
        print('Hermes project skills require project trust: review, then run hermes skills trust.')
    if 'dsh' in selected:
        print('DSH: requires its filesystem skill provider and user-invocable skills enabled.')
    return 0


if __name__ == '__main__':
    try:
        sys.exit(main())
    except (OSError, ValueError) as error:
        print(f'Install failed: {error}', file=sys.stderr)
        sys.exit(1)
