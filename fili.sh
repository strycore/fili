#!/bin/bash

ROOTDIR="$( cd "$( echo "${BASH_SOURCE[0]%/*}" )" && pwd )"
export PYTHONPATH="${ROOTDIR}"
export PYTHON3PATH="${ROOTDIR}"

$ROOTDIR/bin/fili $*
