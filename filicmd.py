#!/usr/bin/python
import os
import sys
import datetime
import hashlib
import platform
import binascii
from fili.models import File, Scan, create_tables
from fili import shell


def get_file_info(path):
    statinfo = os.stat(path)
    return {
        'path': path.decode('utf-8'),
        'size': statinfo.st_size,
        'accessed': statinfo.st_atime,
        'modified': statinfo.st_mtime,
    }


def fastcheck(filename, length=8):
    """Generates a very basic file identifier in O(1) time."""
    size = os.path.getsize(filename)
    if size == 0:
        return None
    partsize = float(size) / float(length)
    fh = open(filename, 'r')
    output = ""
    for i in range(length):
        fh.seek(int(i * partsize))
        output += binascii.hexlify(fh.read(1))
    fh.close()
    return output


def index_file(path, sha1, fileinfo, scan):
    print("Indexing %s" % fileinfo['path'])
    indexed_file = File(
        path=fileinfo['path'],
        size=fileinfo['size'],
        sha1=sha1,
        accessed=fileinfo['accessed'],
        modified=fileinfo['modified'],
        scan=scan
    )
    indexed_file.save()


def index_path(path):
    scan = Scan(
        machine=platform.node(),
        root=path,
        created_at=datetime.datetime.now()
    )
    scan.save()
    for filepath in iter_dir(path):
        filehash = calculate_sha1(filepath)
        fileinfo = get_file_info(filepath)
        index_file(filepath, filehash, fileinfo, scan)


def iter_dupes():
    pass
    # dupes = cursor.execute("""SELECT count(path), hash
    #                            FROM file GROUP BY hash
    #                            HAVING count(path) > 1 AND hash != '0'
    #                            ORDER BY path""")
    # for dupehash in dupes.fetchall():
    #    filehash = dupehash[1]
    #    dupe_files = cursor.execute("""SELECT * FROM file WHERE hash=?""",
    #                                (filehash, ))
    #    yield dupe_files.fetchall()


def delete_dupes():
    for dupe_group in iter_dupes():
        for index, dupefile in enumerate(dupe_group):
            filepath = dupefile[0].encode('utf-8')
            if index == 0:
                print "keeping " + filepath
            else:
                print "deleting " + filepath
                try:
                    os.remove(filepath)
                except OSError:
                    print "Can't remove file %s" % filepath
                unindex(filepath, strict=True)


def print_dupes():
    for dupe_group in iter_dupes():
        for dupefile in dupe_group:
            print dupefile[0].encode('utf-8')


def search_file(query):
    results = File.select().where(File.path.contains(query))
    for result in results:
        print result.path.encode('utf-8')


def unindex(path_query, strict=False):
    glob = '' if strict else '%'
    path = path_query.decode('utf-8') + glob
    Scan.delete().where(Scan.path == path)


def calculate_sha1(filename):
    sha1 = hashlib.sha1()
    try:
        with open(filename, 'rb') as f:
            for chunk in iter(lambda: f.read(8192), b''):
                sha1.update(chunk)
    except IOError:
        print "Error reading %s" % filename
        return False
    return sha1.hexdigest()


def iter_dir(path):
    for root, dirs, files in os.walk(path):
        for filename in files:
            yield os.path.join(root, filename)


if __name__ == "__main__":
    create_tables()
    args = shell.dispatch_arguments(sys.argv[1:])
    if args.command == 'index':
        index_path(args.path)
    if args.command == "dupes":
        if args.dupes_command == 'list':
            print_dupes()
        elif args.dupes_command == 'delete':
            delete_dupes()
    if args.command == 'unindex':
        unindex(args.path)
    if args.command == 'search':
        search_file(args.query)
