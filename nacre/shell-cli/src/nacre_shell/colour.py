"""Compatibility import for the standalone Orient colour module."""

import sys
from orient import colour as implementation

sys.modules[__name__] = implementation
