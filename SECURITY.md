# Security Policy

## Supported versions

`ratada` is pre-1.0 and released from a single line. Security fixes land on the
latest published `0.x` version; there is no back-porting to older releases.
Depend on an up-to-date version and read [`CHANGELOG.md`](CHANGELOG.md) before
upgrading, since a `0.x` minor bump may carry breaking changes.

| Version        | Supported          |
| -------------- | ------------------ |
| latest `0.x`   | :white_check_mark: |
| older releases | :x:                |

## Reporting a vulnerability

Please report suspected vulnerabilities privately, not through a public issue.
Use GitHub's private vulnerability reporting: open the repository's **Security**
tab and choose **Report a vulnerability**. Include the affected version, a
description, and – where possible – a minimal reproduction.

You can expect an acknowledgement within a few days. Once a fix is ready it will
be published as a new release and the advisory disclosed.

## Scope

`ratada` is a terminal UI toolkit with no network layer and no persistence, so
its attack surface is narrow. The security-sensitive seams, and how they are
hardened, are:

- **External process invocation** (`clipboard`, `editor`, `opener`): commands
  are built with an explicit argument list (`Command::arg`/`args`), never
  `sh -c` with an interpolated string, so a value cannot inject a shell command.
- **Path confinement** (`path_picker`): an optional root is enforced with
  `canonicalize()` + `starts_with()` to keep navigation from escaping it.
- **Terminal geometry**: `u16`/`usize` conversions are bounded by the screen
  size.

Reports about these areas – or any way to make the library panic on untrusted
input – are especially welcome.
