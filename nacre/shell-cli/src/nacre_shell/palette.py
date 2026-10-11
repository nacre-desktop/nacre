"""Compatibility import for the standalone Orient palette module."""

import sys
from orient import palette as implementation

sys.modules[__name__] = implementation
