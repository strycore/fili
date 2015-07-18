import argparse


def add_commands(index_subparsers):
    list_parser = index_subparsers.add_parser(
        'list', help="List all indexed directories"
    )
    list_parser.add_argument('-s', '--short', action="store_true",
                             help="Only print the name of indexes")

    create_parser = index_subparsers.add_parser(
        'create', help="Indexes the files in a directory"
    )
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

    diff_parser = index_subparsers.add_parser('diff',
                                              help="Compare 2 indexes")
    diff_parser.add_argument('reference', help="The index serving as reference")
    diff_parser.add_argument('other', help="The index being compared to")
    diff_parser.add_argument(
        '--copy-diff', '-c', dest='copy_dest',
        help="Copy non matching files from reference to the given directory."
    )


def dispatch_arguments(args):
    parser = argparse.ArgumentParser(
        prog="fili",
        description="File Library Indexer",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )
    subparsers = parser.add_subparsers(title="Commands", dest="command",
                                       help=False)
    add_commands(subparsers)
    return parser.parse_args(args)
