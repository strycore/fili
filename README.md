fili (FIle Library Indexer)
===========================

Fili is a small tool used to build a database of files and query it. The
database is SQLite3 and is located under ~/.fili.db

Usage:
------

Add to database every file under [path]

    fili index [path]

Remove from database every file located under [path]

    fili unindex [path]

Search for file names in database containing [expr]

    fili search [expr]

Search for file duplicates (using md5 hash)

    fili dupes

Delete duplicates from disk, keeping only the first file found

    fili delete-dupes


Todo:
-----

A lot. This is an early working prototype.
