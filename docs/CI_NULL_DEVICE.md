# Disposable Windows CI NUL preparation

This is an explicit owner-approved CI host condition, not an Argentum capability,
an application installation step or a general Windows support qualification.
The helper is not linked into either executable and never starts with the CLI.

Hosted failures establish that LPAC children cannot reopen NUL while inherited
stdin works. The approved candidate adds one non-inheriting read/write ACE for
`S-1-15-2-2` on the exact kernel object `\Device\Null`. The mask is `0x0012019f`
(`FILE_GENERIC_READ | FILE_GENERIC_WRITE`), without execute, delete, WRITE_DAC or
WRITE_OWNER access for that trustee. No token capability, file/directory,
registry, network, owner, group, full SACL or integrity-label change is requested.
This is a host-wide NUL grant during the scope, not process-local access.

`-CiNullAccess` is off by default. It is selected only by the native hosted
workflow. The full driver scopes it to the `argentum_execution` and
`tool_catalog_http_cli` test executables after compilation; the focused driver
scopes it to the actual execution diagnostic. All original assertions and
required targets remain. A global device mutex rejects overlapping helpers.

The native handle is opened only after GitHub-hosted Windows and existing admin
checks, and its bounded queried kernel name must be exactly `\Device\Null`.
Environment values are accidental-use guards, not authentication against a
compromised OS user. The code does not elevate or request security privileges.
It requests GENERIC_READ, READ_CONTROL and WRITE_DAC on its private non-inherited
handle, never WRITE_OWNER or ACCESS_SYSTEM_SECURITY. No arbitrary target path
is accepted. Missing authority fails rather than widening access.

Before mutation, the helper captures bounded owner/group/DACL/mandatory-label
state. It rejects null or conflicting DACL entries, preserves every existing
ACE and appends only the exact new tuple. Existing sufficient access is a no-op.
It rechecks the original state before writing and checks complete sampled
descriptor identity after a DACL-only write. Full SACL preservation follows
from never requesting its write; audit entries are not disclosed in receipts.

The scope restores the original DACL in `finally`, including when the body or
apply result fails, and verifies the original sampled descriptor hash. An
unexpected concurrent change is not overwritten. Restore, capture and close
failures fail CI rather than claiming qualification. Receipts contain hashes,
the fixed SID/mask and explicit applied/restored state, not reusable authority.
Hard process termination may prevent `finally`; the receipt then lacks verified
restoration and the disposable job VM is discarded. There is no persistent
host-preparation guarantee or local-machine fallback.

Synthetic state controls cover tuple/preservation, no-op, body exceptions,
uncertain writes, drift, failed restoration, wrong devices, null/conflicting
DACLs, integrity changes and local/self-hosted/other-OS rejection. They open no
native NUL handle. Actual hosted LPAC compiler/test and existing private-file,
registry, network, cancellation and source-preservation tests are still required.
If another permission or a label change is needed, stop for a separate decision.

The relevant primary sources are
[Microsoft host preparation](https://github.com/microsoft/mxc/blob/c6f301d53a1430c4c921a05c57af838f7392348f/docs/backends/process-container/host-prep.md),
[security information flags](https://learn.microsoft.com/en-us/windows/win32/secauthz/security-information)
and [hosted job isolation](https://docs.github.com/en/actions/concepts/runners/github-hosted-runners).
Microsoft's full descriptor replacement is not used: our approval does not
permit its owner/group/label changes or system-drive preparation.
