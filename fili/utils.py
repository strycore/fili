import os
import hashlib
import binascii
import datetime


def iso_to_datetime(isoformat):
    isoformat = isoformat.split('.')[0]
    return datetime.datetime.strptime(isoformat, "%Y-%m-%dT%H:%M:%S")


def get_file_info(path):
    statinfo = os.stat(path)
    return {
        'path': path,
        'size': statinfo.st_size,
        'accessed': datetime.datetime.fromtimestamp(statinfo.st_atime),
        'modified': datetime.datetime.fromtimestamp(statinfo.st_mtime),
    }


def relativize_path(absolute_path, root_dir):
    if root_dir[-1] != '/':
        root_dir += '/'
    try:
        _, relative = absolute_path.split(root_dir)
    except ValueError:
        return absolute_path
    return relative


def get_fastsum(filename, length=8):
    """Generates a very basic file identifier in O(1) time."""
    size = os.path.getsize(filename)
    if size == 0:
        return None
    partsize = float(size) / float(length)
    fh = open(filename, 'rb')
    output = ""
    for i in range(length):
        offset = int(i * partsize)
        fh.seek(offset)
        output += binascii.hexlify(fh.read(1)).decode()
    fh.close()
    return output


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


def calculate_sha1(filename):
    sha1 = hashlib.sha1()
    try:
        with open(filename, 'rb') as f:
            for chunk in iter(lambda: f.read(8192), b''):
                sha1.update(chunk)
    except IOError:
        print("Error reading %s" % filename)
        return False
    return sha1.hexdigest()


def iter_dir(path):
    for root, dirs, files in os.walk(path):
        for filename in files:
            yield os.path.join(root, filename)
