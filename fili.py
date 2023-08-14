#!/usr/bin/env python3
"""Plopé"""
import binascii
import hashlib
import json
import os
import sqlite3

import click

DBPATH = os.path.join(os.path.expanduser('~'), '.fili.db')
VIRTUAL_FS = [
    "/dev",
    "/sys",
    "/proc",
    "/run/user",
    "/run/udev",
    "/run/systemd",
    "/var/lib/flatpak",
]

# Common cache paths that can be ignored
CACHE_PATHS = {
    ".cache/mesa_shader_cache": "mesa",
    ".cache/thumbnails": "generic-image-thumbnails",
    ".cache/opera/Cache": "opera",
    ".cache/opera/Code Cache": "opera",
    ".config/opera/Service Worker/CacheStorage": "opera",
    ".cache/opera-developer/Cache": "opera-developer",
    ".cache/mozilla": "firefox",
    ".cache/pip": "pip",
    ".cache/pypoetry": "poetry",
    ".cache/shotwell/thumbs": "shotwell",
    ".cache/typescript": "typescript",
    ".config/Code/CachedData": "code",
    ".config/discord/Cache": "discord",
    ".config/discord/Code Cache": "discord",
    ".local/share/lutris/runners/wine": "lutris",
    ".node-gyp": "nodejs",
}

class Cursor():
    """Manage a SQLite connection"""
    def __init__(self):
        self.db_conn = None

    def __enter__(self):
        self.db_conn = sqlite3.connect(DBPATH)
        cursor = self.db_conn.cursor()
        return cursor

    def __exit__(self, _type, _value, _traceback):
        self.db_conn.commit()
        self.db_conn.close()


def create_db():
    """Create the SQLite database and table"""
    db_conn = sqlite3.connect(DBPATH)
    cursor = db_conn.cursor()
    cursor.execute("""CREATE TABLE files
                   (path text,
                    size integer,
                    hash text,
                    hostname text,
                    accessed integer,
                    modified integer)""")
    db_conn.commit()
    db_conn.close()


def get_file_info(path):
    """Return the info for a file"""
    statinfo = os.stat(path)
    return {
        'path': path,
        'size': statinfo.st_size,
        'accessed': statinfo.st_atime,
        'modified': statinfo.st_mtime,
    }


@click.group()
def main():
    """Example script."""


def fastcheck(filename, length=8):
    """Generates a very basic file identifier in O(1) time."""
    size = os.path.getsize(filename)
    if size == 0:
        return None
    partsize = float(size) / float(length)
    handler = open(filename, 'r', encoding="utf-8")
    output = ""
    for i in range(length):
        handler.seek(int(i * partsize))
        output += binascii.hexlify(handler.read(1))
    handler.close()
    return output

@main.command("list")
@click.argument("path")
@click.option("-r", "--recurse", is_flag=True)
def list_path(path, recurse=True):
    """List files in a folder"""
    cache_paths = [
            os.path.join(os.path.expanduser("~"), cache_path)
            for cache_path in CACHE_PATHS
    ]
    for filepath in iter_dir(path, recurse=recurse):
        skip_item = False
        for root_path in VIRTUAL_FS:
            if filepath.startswith(root_path):
                skip_item = True
        for root_path in cache_paths:
            if filepath.startswith(root_path):
                skip_item = True
        if filepath.endswith("dev/core"):
            skip_item = True
        if skip_item:
            continue
        try:
            fileinfo = get_file_info(filepath)
        except (FileNotFoundError, PermissionError):
            continue
        click.echo(
            f"{fileinfo['path']} || {fileinfo['size']} || "
            f"{fileinfo['accessed']} || {fileinfo['modified']}"
        )


@main.command("index")
@click.argument("path")
@click.option("-i", "--ignore-cache", is_flag=True)
@click.option("-h", "--with-hash", is_flag=True)
def index_path(path, ignore_cache=True, with_hash=False):
    """List path and index metadata to database"""
    if ignore_cache:
        cache_paths = [
            os.path.join(os.path.expanduser("~"), cache_path)
            for cache_path in CACHE_PATHS
        ]
    else:
        cache_paths = []
    stats = {
        "indexed": 0,
        "skipped": 0,
        "failed": 0,
    }
    hostname = os.uname().nodename
    with Cursor() as cursor:
        res = cursor.execute("DELETE FROM files WHERE path LIKE ?", (path + "%", ))
        click.echo(f"Deleted {res.rowcount} entries")
        for filepath in iter_dir(path):
            skip_item = False
            for root_path in VIRTUAL_FS:
                if filepath.startswith(root_path):
                    skip_item = True
            for root_path in cache_paths:
                if filepath.startswith(root_path):
                    skip_item = True
            if skip_item:
                stats["skipped"] += 1
                continue
            if with_hash:
                filehash = calculate_md5(filepath)
            else:
                filehash = ""
            try:
                fileinfo = get_file_info(filepath)
            except (FileNotFoundError, PermissionError):
                click.echo(f"Failed to read file info in {filepath}")
                stats["failed"] += 1
                continue
            try:
                cursor.execute(
                    "INSERT INTO files VALUES (?, ?, ?, ?, ?, ?)", (
                        fileinfo['path'],
                        fileinfo['size'],
                        filehash,
                        hostname,
                        fileinfo['accessed'],
                        fileinfo['modified']
                    )
                )
            except UnicodeEncodeError:
                click.echo(f"Failed to save {fileinfo}")
                stats["failed"] += 1
                continue
            stats["indexed"] += 1
            if stats["indexed"] % 1000 == 0:
                click.echo(f"Indexed {stats['indexed']} files. (Current file: {filepath})")
        click.echo(f"Indexed {stats['indexed']} files")
        click.echo(f"Skipped {stats['skipped']} files")
        click.echo(f"Failed {stats['failed']} files")


def iter_dupes():
    """Find duplicate files"""
    with Cursor() as cursor:
        dupes = cursor.execute("""SELECT count(path), hash
                                  FROM files GROUP BY hash
                                  HAVING count(path) > 1 AND hash != '0'
                                  ORDER BY path""")
        for dupehash in dupes.fetchall():
            filehash = dupehash[1]
            dupe_files = cursor.execute("""SELECT * FROM files WHERE hash=?""",
                                        (filehash, ))
            yield dupe_files.fetchall()

@main.command("recent")
def get_recent():
    """Show the 100 most recently accessed files"""
    with Cursor() as cursor:
        recent_files = cursor.execute(
            """SELECT * FROM files
            WHERE path LIKE '/home%' AND path NOT LIKE '%/.%'
            ORDER BY accessed DESC
            LIMIT 100"""
        )
        for recent in recent_files.fetchall():
            print(recent)


@main.command("dupes")
def list_dupes():
    """Print list of duplicate files"""
    for dupe_group in iter_dupes():
        for dupefile in dupe_group:
            click.echo(dupefile[0].encode('utf-8'))


@main.command("search")
@click.argument("query")
def search_file(query):
    """Search for files matching query"""
    with Cursor() as cursor:
        result = cursor.execute(f"SELECT * FROM files WHERE path LIKE '%{query}%'")
        for fileentry in result.fetchall():
            click.echo(fileentry[0].encode('utf-8'))


@main.command()
@click.argument("path-query")
@click.option("-s", "--strict", is_flag=True)
def unindex(path_query, strict=False):
    """Remove files matching path-query from database"""
    glob = '' if strict else '%'
    with Cursor() as cursor:
        res = cursor.execute("DELETE FROM files WHERE path LIKE ?", (path_query + glob, ))
        click.echo(f"{res.rowcount} files unindexed")


def calculate_md5(filename):
    """Return the MD5 of a file"""
    md5 = hashlib.md5()
    try:
        with open(filename, 'rb') as _file:
            for chunk in iter(lambda: _file.read(8192), b''):
                md5.update(chunk)
    except IOError:
        click.echo(f"Error reading {filename}")
        return False
    return md5.hexdigest()


def iter_dir(path, recurse=True):
    """List all files in a path, by default recursively"""
    if recurse:
        for root, dirs, files in os.walk(path):
            for dirname in dirs:
                yield os.path.join(root, dirname)
            for filename in files:
                yield os.path.join(root, filename)
    else:
        for filename in os.listdir(path):
            yield os.path.join(path, filename)

@main.command()
@click.option("-e", "--export-path")
def export_files(export_path="results.json"):
    """Export all indexed files to a JSON file"""
    all_data = []
    with Cursor() as cursor:
        recent_files = cursor.execute(
            """SELECT * FROM files"""
        )
        for recent in recent_files.fetchall():
            all_data.append({
                "path": str(recent[0]),
                "size": recent[1],
                "hash": recent[2],
                "host": recent[3],
                "atime": recent[4],
                "mtime": recent[5]
            })
    with open(export_path, "w", encoding="utf-8") as result_file:
        json.dump(all_data, result_file, indent=2)


@main.command("stats")
def print_stats():
    """Display some stats"""
    with Cursor() as cursor:
        count = cursor.execute("""SELECT COUNT(*) FROM files""")
        for cnt in count.fetchall():
            files_indexed = cnt[0]
    click.echo(f"{files_indexed} files indexed")


if not os.path.exists(DBPATH):
    create_db()
