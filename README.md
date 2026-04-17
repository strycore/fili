fili (FIle Library Indexer)
===========================

Fili is a personal file intelligence system. It scans your filesystem, indexes
collections, and lets you query, classify, and protect them. The database is
SQLite3 stored under the XDG data directory.

Usage:
------

Initialize the database:

    fili init

Scan a path (defaults to `~`):

    fili scan [path]

Show an overview of indexed collections:

    fili status

Search for files or collections:

    fili find [query]
    fili find [query] --collections

List known paths (optionally only unclassified ones):

    fili paths [--unknown]

Classify a path:

    fili classify [path] -t [type]

Show collections that aren't backed up:

    fili unprotected

Show duplicate collections:

    fili duplicates [--same-device]

Export the index to JSON:

    fili export [output]

Show statistics:

    fili stats

Set a privacy level for a path (`public`, `personal`, `confidential`):

    fili privacy [path] [level] [--marker]

Start a local web UI + REST API for browsing the index (default
`http://127.0.0.1:7777`):

    fili serve [--addr 127.0.0.1] [--port 7777]


TODO:
-----

Features from the original Python prototype that haven't been ported yet:

- `recent` — show the most recently accessed files
- `unindex` — remove entries matching a path prefix from the database
