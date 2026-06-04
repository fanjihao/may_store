# Legacy SQL Migrations (Archived)

⚠️ **These files are historical reference only. Do not apply them to a database.**

The canonical, single-source-of-truth schema is [`src/v3.sql`](../../v3.sql). It is self-contained: every enum, table, index, foreign key, and seed row needed to bootstrap a working database is defined there.

## Why these are archived

Both `001_v1_260423.sql` and `002_phase1_foundation.sql` are **strictly redundant** with `v3.sql`. `v3.sql` is a superset of every object they create. The binary does not load any file from this directory at startup; schema bootstrap is a manual `psql -f src/v3.sql` step.

Keeping these files in a `migrations/` subfolder next to live code made them look authoritative, which contributed to past bugs where the code and the live schema drifted (see git history for the `is_active` / `user_status_enum` mismatch).

## What they contain

- `001_v1_260423.sql` — Original baseline migration that added footprint/achievement/diamond tables and their seed data. Purely additive (`IF NOT EXISTS`). 170 lines.
- `002_phase1_foundation.sql` — FSD v2 transaction tables (`user_group_points`, `love_point_transactions`, etc.) and column additions to `orders` / `wishes`. 358 lines. **Non-idempotent** — running it against a fresh `v3.sql` database will fail with `42P07 duplicate object` errors on the FSD v2 tables.

## If you need to reconstruct historical state

`git log --follow <file>` will reveal when each file was last touched. The two files together represent the evolution from a pre-FSD-v2 single-pair model (`couple_relations`, `invitations`) to the current FSD-v2 group model (`association_groups`, `association_group_members`). The `v3.sql` consolidation drops the legacy single-pair artifacts in favor of the group model.
