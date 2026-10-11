"""Compatibility import for the standalone Orient extract module."""

import sys
from orient import extract as implementation

sys.modules[__name__] = implementation
