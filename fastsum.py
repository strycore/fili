import os
import binascii


def fastcheck(filename, length=8):
    """Generates a very basic file identifier in O(1) time."""
    size = os.path.getsize(filename)
    if size == 0:
        return None
    partsize = float(size) / float(length)
    fh = open(filename, 'r')
    output = ""
    for i in range(length):
        fh.seek(int(i * partsize))
        output += binascii.hexlify(fh.read(1))
    fh.close()
    return output
