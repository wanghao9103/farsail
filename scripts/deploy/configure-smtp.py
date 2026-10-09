#!/usr/bin/env python3
"""Update private SMTP settings or restore missing defaults; never print credentials."""

import argparse
import fcntl
import getpass
import ipaddress
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
from urllib.parse import quote


class ConfigError(Exception):
    pass


KEY = re.compile(r"^\s*(?:export\s+)?([A-Z][A-Z0-9_]*)\s*[=:]\s*(.*)$")


def quoted(value):
    if not value or any(ord(c) < 32 or ord(c) == 127 for c in value):
        raise ConfigError("Empty values and control characters are not allowed.")
    return '"' + value.replace('\\', '\\\\').replace('"', '\\"').replace('$', '$$') + '"'


def merge(original, changes):
    kept = []
    for line in original.splitlines(keepends=True):
        match = KEY.match(line)
        if not match or match[1] not in changes:
            kept.append(line)
    text = ''.join(kept)
    if text and not text.endswith('\n'):
        text += '\n'
    return text + ''.join(f"{key}={quoted(value)}\n" for key, value in changes.items())


def missing(original, name):
    values = [m[2].strip() for line in original.splitlines() if (m := KEY.match(line)) and m[1] == name]
    return not values or all(value in ('', "''", '""') or value.startswith('#') for value in values)


def defaults(original, model, settings):
    changes = {}
    # Compose config JSON escapes dollars for replay; undo that serialization here.
    def env(service, key):
        value = model.get('services', {}).get(service, {}).get('environment', {}).get(key)
        if not isinstance(value, str) or not value:
            raise ConfigError(f"Cannot restore defaults: missing {service}/{key} in private deployment.")
        return value.replace('$$', '$')

    if missing(original, 'FARSAIL_DATABASE_URL'):
        user, password, database = (env('db', key) for key in ('POSTGRES_USER', 'POSTGRES_PASSWORD', 'POSTGRES_DB'))
        changes['FARSAIL_DATABASE_URL'] = f"postgres://{quote(user, safe='')}:{quote(password, safe='')}@db:5432/{quote(database, safe='')}"
    if missing(original, 'FARSAIL_BIND'):
        changes['FARSAIL_BIND'] = '127.0.0.1:8787'
    if missing(original, 'FARSAIL_RELAY_ACCESS_TOKEN'):
        changes['FARSAIL_RELAY_ACCESS_TOKEN'] = env('relay', 'IROH_RELAY_HTTP_BEARER_TOKEN')
    if missing(original, 'FARSAIL_RELAY_URLS'):
        try:
            address = str(ipaddress.IPv4Address(settings['ip']))
            ports = model['services']['gateway']['ports']
            port = int(next(p['published'] for p in ports if int(p['target']) == 8444))
            if not 1 <= port <= 65535:
                raise ValueError()
        except (KeyError, ValueError, StopIteration, TypeError):
            raise ConfigError('Cannot restore relay URL from existing settings and gateway port.') from None
        changes['FARSAIL_RELAY_URLS'] = f'https://{address}:{port}/'
    return changes


def run(command):
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode:
        # stderr can contain private configuration; do not echo it.
        raise ConfigError('Docker Compose command failed. Inspect coordinator logs locally; redact credentials before sharing.')
    return result.stdout


def replace_file(path, content, owner):
    fd, temp = tempfile.mkstemp(prefix='.smtp-config-', dir=path.parent)
    try:
        os.fchmod(fd, 0o600)
        if os.geteuid() == 0:
            os.fchown(fd, owner.st_uid, owner.st_gid)
        with os.fdopen(fd, 'w', encoding='utf-8', newline='') as output:
            output.write(content)
            output.flush()
            os.fsync(output.fileno())
        os.replace(temp, path)
    finally:
        Path(temp).unlink(missing_ok=True)


def configure(root, state, smtp=None, apply=False):
    root, state = Path(root).resolve(), Path(state).resolve()
    path = state / 'coordinator.env'
    if path.is_symlink() or not path.is_file() or not (state / 'compose.env').is_file() or not (root / 'deploy/production/compose.yaml').is_file():
        raise ConfigError('Existing deployment not found, or coordinator.env is a symlink; nothing changed.')
    command = ['docker', 'compose', '--env-file', str(state / 'compose.env'), '-f', str(root / 'deploy/production/compose.yaml')]
    settings = json.loads((state / 'settings.json').read_text()) if (state / 'settings.json').is_file() else {}
    if settings.get('mode') == 'test':
        command += ['-f', str(root / 'deploy/production/test.override.yaml')]
    with open(str(state) + '.operation.lock', 'a') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise ConfigError('Another deployment operation is active; nothing changed.') from None
        original = path.read_text()
        model = json.loads(run(command + ['config', '--format', 'json']))
        changes = defaults(original, model, settings)
        if smtp:
            changes.update(smtp)
        if not changes and not apply:
            return None
        candidate = merge(original, changes)
        backup_parent = state / 'backups'
        backup_parent.mkdir(mode=0o700, exist_ok=True)
        backup = Path(tempfile.mkdtemp(prefix='smtp-', dir=backup_parent)) / 'coordinator.env'
        shutil.copyfile(path, backup)
        backup.chmod(0o600)
        owner = path.stat()
        attempted = False
        recreate = command + ['up', '-d', '--pull', 'never', '--no-deps', '--force-recreate', '--wait', '--wait-timeout', '180', 'coordinator']
        try:
            replace_file(path, candidate, owner)
            run(command + ['config', '--quiet'])
            if apply:
                attempted = True
                run(recreate)
        except (ConfigError, OSError, KeyboardInterrupt):
            replace_file(path, original, owner)
            if attempted:
                try:
                    run(recreate)
                except ConfigError:
                    raise ConfigError(f'Original configuration restored; service recovery failed. Private backup: {backup}') from None
            raise ConfigError(f'Update failed; original configuration restored. Private backup: {backup}') from None
        return backup


def prompt_smtp():
    if not sys.stdin.isatty():
        raise ConfigError('Use an interactive terminal for SMTP credentials, or --repair-only to restore missing defaults.')
    host = input('SMTP host [smtp.126.com]: ').strip() or 'smtp.126.com'
    tls = input('TLS mode [implicit; or starttls]: ').strip() or 'implicit'
    port = input(f"SMTP port [{'587' if tls == 'starttls' else '465'}]: ").strip() or ('587' if tls == 'starttls' else '465')
    sender = input('Sender email: ').strip()
    username = input('SMTP username [sender email]: ').strip() or sender
    password = getpass.getpass('SMTP authorization code (hidden): ')
    if not re.fullmatch(r'[A-Za-z0-9](?:[A-Za-z0-9.-]*[A-Za-z0-9])?', host) or tls not in ('implicit', 'starttls') or not port.isdigit() or not 1 <= int(port) <= 65535 or not re.fullmatch(r'[^\s<>@]+@[^\s<>@]+\.[^\s<>@]+', sender):
        raise ConfigError('Invalid SMTP host, TLS mode, port or sender email; nothing changed.')
    values = {'FARSAIL_MAIL_MODE': 'smtp-tls', 'FARSAIL_SMTP_HOST': host, 'FARSAIL_SMTP_PORT': str(int(port)), 'FARSAIL_SMTP_TLS': tls, 'FARSAIL_SMTP_USER': username, 'FARSAIL_SMTP_PASSWORD': password, 'FARSAIL_MAIL_FROM': f'FarSail <{sender}>'}
    for value in values.values():
        quoted(value)
    return values


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--deploy-root', default='/home/data/farsail')
    parser.add_argument('--apply', action='store_true', help='Recreate only coordinator; restore original configuration if startup fails.')
    parser.add_argument('--repair-only', action='store_true', help='Restore missing database/bind/relay parameters without changing SMTP.')
    args = parser.parse_args()
    root = Path(args.deploy_root).resolve()
    state = Path(os.environ.get('FARSAIL_STATE_DIR', str(root / '.local/production')))
    os.umask(0o077)
    try:
        backup = configure(root, state, None if args.repair_only else prompt_smtp(), args.apply)
    except (ConfigError, OSError, ValueError, EOFError, KeyboardInterrupt) as error:
        # Generic OS/parse errors may contain private data; print only controlled messages.
        print(str(error) if isinstance(error, ConfigError) else 'Configuration update interrupted or private files invalid; inspect locally.', file=sys.stderr)
        return 1
    if backup:
        print(f'Configuration updated. Private backup: {backup}')
        print('Coordinator healthy.' if args.apply else 'Container unchanged. Recreate coordinator to apply settings.')
    else:
        print('No missing base parameters; configuration and container unchanged.')
    print('SMTP delivery still requires a real mailbox acceptance check.')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
