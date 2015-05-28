import argparse


def add_index_parser(subparsers):
    index_parser = subparsers.add_parser(
        'index',
        help="Index the contents of a directory"
    )
    index_subparsers = index_parser.add_subparsers(
        title="Index commands",
        dest="index_command",
    )

    index_subparsers.add_parser('list', help="List all indexed directories")

    create_parser = index_subparsers.add_parser('create')
    create_parser.add_argument('path', help="Directory to index")
    create_parser.add_argument(
        '-f', '--fast',
        action="store_true",
        help="Provides a faster and less reliable than sha1"
    )
    create_parser.add_argument(
        '-n', '--name', dest='name', help="Name of the index"
    )

    export_parser = index_subparsers.add_parser('export',
                                                help="Export index to file")
    export_parser.add_argument('name', help="name of the index")
    export_parser.add_argument('outfile', help="path for exported data")

    import_parser = index_subparsers.add_parser(
        'import',
        help="Import json index to database"
    )
    import_parser.add_argument('infile', help="path for imported data")

    delete_parser = index_subparsers.add_parser('delete',
                                                help="Delete an index")
    delete_parser.add_argument('name', help="Name of the index to delete")

    update_parser = index_subparsers.add_parser('update',
                                                help="Rescan an index")
    update_parser.add_argument('name', help="Name of the index to rescan")


def add_dupes_parser(subparsers):
    dupes_parser = subparsers.add_parser(
        'dupes',
        help="Find and delete duplicates"
    )
    dupes_subparsers = dupes_parser.add_subparsers(
        title="Commands",
        dest="dupes_command",
    )
    dupes_subparsers.add_parser('list')
    dupes_subparsers.add_parser('delete')


def dispatch_arguments(args):
    parser = argparse.ArgumentParser(
        prog="fili",
        description="File Library Indexer",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )
    subparsers = parser.add_subparsers(title="Commands", dest="command",
                                       help=False)
    add_index_parser(subparsers)
    add_dupes_parser(subparsers)

    search_parser = subparsers.add_parser('search',
                                          help="Search for an indexed file")
    search_parser.add_argument('query', help="part of filename to search for")
    return parser.parse_args(args)
