import os
import json
import platform
import datetime
from fili.models import File, Scan
from fili.utils import (iter_dupes, iter_dir, calculate_sha1,
                        get_file_info, iso_to_datetime, get_fastsum)


def index_list():
    scans = Scan.select()
    if scans.count() == 0:
        print("No indexes")
        return
    for scan in scans:
        print('{:32}'.format(scan.name))


def index_file(path, fastsum, sha1, fileinfo, scan):
    print("Indexing %s" % fileinfo['path'])
    indexed_file = File(
        path=fileinfo['path'],
        size=fileinfo['size'],
        fastsum=fastsum,
        sha1=sha1,
        accessed=fileinfo['accessed'],
        modified=fileinfo['modified'],
        scan=scan
    )
    indexed_file.save()


def index_create(path, name=None, fast=False):
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
        existing = Scan.get(Scan.name == name)
    except Scan.DoesNotExist:
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
        fastsum = get_fastsum(filepath)
        if fast:
            filehash = "null"
        else:
            filehash = calculate_sha1(filepath)
        fileinfo = get_file_info(filepath)
        index_file(filepath, fastsum, filehash, fileinfo, scan)


def index_export(name, outfile_path):
    index = Scan.select().where(Scan.name == name).get()
    index_data = index.as_json()
    with open(outfile_path, 'w') as outfile:
        outfile.write(json.dumps(index_data, indent=2))


def index_import(infile_path):
    with open(infile_path, 'r') as infile:
        contents = infile.read()
    index_data = json.reads(contents)
    imported_scan = Scan(
        name=index_data['name'],
        machine=index_data['machine_name'],
        root=index_data['root_directory'],
        created_at=iso_to_datetime(index_data['created_at'])
    )
    imported_scan.save()
    for file_data in index_data['files']:
        imported_file = File(
            path=file_data['path'],
            size=file_data['size'],
            sha1=file_data['sha1'],
            fastsum=file_data['fastsum'],
            accessed=iso_to_datetime(file_data['accessed']),
            modified=iso_to_datetime(file_data['modified']),
            scan=imported_scan
        )
        imported_file.save()


def index_delete(name):
    try:
        scan_instance = Scan.get(Scan.name == name)
    except Scan.DoesNotExist:
        print("No scan named {}".format(name))
        return
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
