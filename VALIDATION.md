# Validation of Python 0.2

Verified locally on Windows with Python 3.11.9 on 2026-09-16.

- Full suite: **111 passed**. Includes the published BIP-352 receiving and address vectors, API coverage and pagination, client response validation, atomic database writes, restart recovery, and chain reorganization handling.
- Bitcoin Core 28.1 regtest integration: indexed real blocks, resumed after restart, replaced a chain branch, received a ZMQ notification, mined a labeled Silent Payment, and detected it through the Python client. The node used a temporary, isolated data directory.
- Ruff lint and formatting checks passed. JavaScript syntax checking passed.
- Built a Python wheel and confirmed that dashboard assets were included.
- Browser checks passed at desktop and 390-pixel mobile widths: live status and blocks, request submission, input validation, responsive layout, labels, and no console errors from the new dashboard.
- `git diff --check` passed.

The Windows ZMQ integration test emits a runtime warning when Tornado supplies an additional selector thread for the default Proactor event loop. The notification test passes; Tornado is included as a Windows dependency.

## Limits of this validation

Docker and a live PostgreSQL instance were unavailable, so the Compose deployment and PostgreSQL backend have not been exercised locally. CI is configured for Windows and Linux with Python 3.11–3.13, but those remote jobs have not been run as part of this work. Performance at mainnet scale and an independent cryptographic/security audit remain outside this validation.

The Impeccable static detector could not resolve the root-relative stylesheet URL from the filesystem and reported a typography warning. Browser inspection confirmed that the stylesheet loads and that headings and body text use different sizes. The Understand graph generator was unavailable; source review was performed manually.

See [README.md](README.md) for setup and [MIGRATION.md](MIGRATION.md) for the required fresh index when moving from Rust.
