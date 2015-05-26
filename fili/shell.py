import argparse


def dispatch_arguments(args):
    parser = argparse.ArgumentParser(
        prog="fili",
        description="File Library Indexer",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )
    subparsers = parser.add_subparsers(title="Commands", dest="command",
                                       help=False)
    index_parser = subparsers.add_parser(
        'index',
        help="Index the contents of a directory"
    )
    index_parser.add_argument('path', help="Directory to index")
    index_parser.add_argument(
        '-f', '--fast',
        action="store_true",
        help="Provides a faster and less reliable than sha1")

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

    unindex_parser = subparsers.add_parser('unindex',
                                           help="Remove path from the index")
    unindex_parser.add_argument('path', help="Path to unindex")

    search_parser = subparsers.add_parser('search',
                                          help="Search for an indexed file")
    search_parser.add_argument('query', help="part of filename to search for")
    return parser.parse_args(args)
