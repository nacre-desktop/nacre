"""Compatibility import for the standalone Orient engine module."""

import sys
from orient import engine as implementation

sys.modules[__name__] = implementation
