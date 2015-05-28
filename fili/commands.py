import os
import json
import platform
import datetime
from fili.models import File, Scan
from fili.utils import iter_dupes, iter_dir, calculate_sha1, get_file_info


def index_list():
    scans = Scan.select()
    if scans.count() == 0:
        print("No scans")
        return
    for scan in scans:
        print('{:32}'.format(scan.name))


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


def index_create(path, name=None):
    created_at = datetime.datetime.now()

    if path.endswith('/'):
        path = path[:-1]

    if not name:
        directory = os.path.split(os.path.abspath(path))[1]
        if not directory:
            directory = 'root'
        creation_date = created_at.strftime('%Y%m%d%H%M')
        name = '-'.join((directory, creation_date))
    try:
        existing = Scan.select().where(name == name)
    except Scan.DoesNotExists:
        existing = None
    if existing:
        print(
            'A scan named {} already exists, please choose a different name'
            'or update the existing one.'.format(name)
        )
        return

    scan = Scan(
        name=name,
        machine=platform.node(),
        root=path,
        created_at=datetime.datetime.now()
    )
    scan.save()
    for filepath in iter_dir(path):
        filehash = calculate_sha1(filepath)
        fileinfo = get_file_info(filepath)
        index_file(filepath, filehash, fileinfo, scan)


def index_export(name, path):
    index = Scan.select().where(Scan.name == name).get()
    index_data = index.as_json()
    print(index.files)
    with open(path, 'w') as outfile:
        outfile.write(json.dumps(index_data, indent=2))


def index_delete(name):
    scan_instance = Scan.select().where(Scan.name == name).get()
    print("deleting index {}".format(scan_instance.name))
    scan_instance.delete_instance(recursive=True)


def delete_dupes():
    for dupe_group in iter_dupes():
        for index, dupefile in enumerate(dupe_group):
            filepath = dupefile[0].encode('utf-8')
            if index == 0:
                print("keeping " + filepath)
            else:
                print("deleting " + filepath)
                try:
                    os.remove(filepath)
                except OSError:
                    print("Can't remove file %s" % filepath)
                # TODO: remove file from index


def print_dupes():
    for dupe_group in iter_dupes():
        for dupefile in dupe_group:
            print(dupefile[0].encode('utf-8'))


def search_file(query):
    results = File.select().where(File.path.contains(query))
    for result in results:
        print(result.path.encode('utf-8'))
