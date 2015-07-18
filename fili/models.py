import os
from . import peewee
DBPATH = os.path.join(os.path.expanduser('~'), '.fili.db')
db = peewee.SqliteDatabase(DBPATH)


class Model(peewee.Model):
    class Meta:
        database = db


class Scan(Model):
    machine = peewee.CharField()
    name = peewee.CharField(max_length=64, unique=True)
    root = peewee.CharField(max_length=4096)
    created_at = peewee.DateTimeField()

    def as_json(self):
        return {
            'name': self.name,
            'machine_name': self.machine,
            'root_directory': self.root,
            'created_at': self.created_at.isoformat(),
            'files': [file_instance.as_json() for file_instance in self.files]
        }

    def __str__(self):
        return self.name


class File(Model):
    path = peewee.CharField()
    size = peewee.IntegerField()
    sha1 = peewee.CharField(null=True)
    fastsum = peewee.CharField(max_length=16, null=True)
    accessed = peewee.DateTimeField()
    modified = peewee.DateTimeField()
    scan = peewee.ForeignKeyField(Scan, related_name='files')

    def as_json(self):
        return {
            'path': self.path,
            'size': self.size,
            'sha1': self.sha1,
            'fastsum': self.fastsum,
            'accessed': self.accessed.isoformat(),
            'modified': self.modified.isoformat()
        }


def create_tables():
    db.connect()
    db.create_tables([Scan, File], safe=True)
