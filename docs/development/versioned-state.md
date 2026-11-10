# Versioned state

Nacre-owned JSON documents use the registry at [`nacre-state-files.json`](../../nacre/shell-tools/nacre-state-files.json). Rust and Python readers validate registered schemas, migrate only on save, and create a pre-migration backup before atomic replacement. Failed writes leave the original intact. Provider-owned credentials and databases remain with their provider and are never copied into Nacre backups.

The registry records each document’s owner, schema, and path. Add a new migration only with a schema version and fixtures for the old and new forms. Bead views access Nacre state through daemon contracts; they do not become settings writers.
