import os
import sqlite3

DBPATH = os.path.join(os.path.expanduser('~'), '.fili.db')


class cursor():
    def __enter__(self):
        self.db_conn = sqlite3.connect(DBPATH)
        cursor = self.db_conn.cursor()
        return cursor

    def __exit__(self, type, value, traceback):
        self.db_conn.commit()
        self.db_conn.close()


def create():
    if os.path.exists(DBPATH):
        return
    db_conn = sqlite3.connect(DBPATH)
    cursor = db_conn.cursor()
    cursor.execute("""CREATE TABLE index(
        id INTEGER PRIMARY KEY,
        machine TEXT,
        path TEXT,
        timestamp INTEGER
    )""")
    cursor.execute("""CREATE TABLE file(
        id INTERGER PRIMARY KEY,
        path TEXT,
        size INTEGER,
        hash TEXT,
        accessed INTEGER,
        modified INTEGER,
        FOREIGN KEY(index_id) REFERENCES index(id)
    )""")
    db_conn.commit()
    db_conn.close()
