from unittest import TestCase
from fili import utils


class DateTest(TestCase):
    def test_iso_to_datetime(self):
        dt_input = "2015-05-31T18:06:54"
        dt = utils.iso_to_datetime(dt_input)
        self.assertEqual(dt.year, 2015)


class PathTest(TestCase):
    def test_relativize_path(self):
        absolute_path = "/path/to/file/here"
        root_dir = "/path/to"
        self.assertEqual(utils.relativize_path(absolute_path, root_dir),
                         'file/here')

        # Also works with trailing slash
        root_dir = "/path/to/"
        self.assertEqual(utils.relativize_path(absolute_path, root_dir),
                         'file/here')

        # Different root_dir returns the original absolute path
        root_dir = "/something/totally/different"
        self.assertEqual(utils.relativize_path(absolute_path, root_dir),
                         absolute_path)
