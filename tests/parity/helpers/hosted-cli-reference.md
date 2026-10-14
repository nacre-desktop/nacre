# Hosted CLI reference boundaries

The 39 `cli-boundary-*` cases were frozen from current Python sources before
retiring dashboard-data/media, launcher-preferences and notification-history.
Each input records the original source SHA-256. Help, option ordering, exit codes,
stdout, file bytes/modes and effects remain exact; extra arguments are ignored
only where Python ignored them. The capture driver refuses existing destinations.

Launcher usage recording uses only the two timestamp rules already present in
the original `case-record.json`. The original raw pre-port observation is retained
as `launcher-preferences/reference-cli-boundary-08-raw.json`; no native output was
used to update its expected data. All other captured fields remain unchanged.

Intended difference: five enumerated unhandled errors retain exact exception
class/message, stdout, exit and files while omitting retired Python source frames.
The explicit contracts also check source hashes, input arguments and source-frame
identity; negative controls reject wider scope or any other observable change.

The two nmcli escape-error catalog IDs remain unchanged as compatibility entries.
The translation checker permits only those exact texts when their Python
transport is absent and its native replacement exists, and still requires
translator context. Changed text, live reuse and other unused IDs fail checks.
