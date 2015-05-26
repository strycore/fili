import os
import peewee
DBPATH = os.path.join(os.path.expanduser('~'), '.fili.db')
db = peewee.SqliteDatabase(DBPATH)


class Model(peewee.Model):
    class Meta:
        database = db


class Scan(Model):
    machine = peewee.CharField()
    name = peewee.CharField(max_length=64)
    root = peewee.CharField(max_length=4096)
    created_at = peewee.DateTimeField()


class File(Model):
    path = peewee.CharField()
    size = peewee.IntegerField()
    sha1 = peewee.CharField(null=True)
    fastsum = peewee.CharField(max_length=16, null=True)
    accessed = peewee.DateTimeField()
    modified = peewee.DateTimeField()
    scan = peewee.ForeignKeyField(Scan)


def create_tables():
    db.connect()
    db.create_tables([Scan, File], safe=True)
