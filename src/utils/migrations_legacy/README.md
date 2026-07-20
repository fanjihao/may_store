# Legacy SQL Migrations (Archived)

⚠️ **These files are historical reference only. Do not apply them to a database.**

The clean-install end state is [`src/v3.sql`](../../v3.sql). Production upgrades use the forward-only migrations in [`migrations/`](../../../migrations/); every new migration must also be reflected in `v3.sql`.

## Why these are archived

Both `001_v1_260423.sql` and `002_phase1_foundation.sql` are **strictly redundant** with `v3.sql`. `v3.sql` is a superset of every object they create. The binary does not load any file from this directory at startup; schema bootstrap is a manual `psql -f src/v3.sql` step.

These two historical files predate the current baseline and are not compatible with the versioned migration chain. Keeping them beside live code made them look executable, which contributed to past schema drift.

## What they contain

- `001_v1_260423.sql` — Original baseline migration that added footprint/achievement/diamond tables and their seed data. Purely additive (`IF NOT EXISTS`). 170 lines.
- `002_phase1_foundation.sql` — FSD v2 transaction tables (`user_group_points`, `love_point_transactions`, etc.) and column additions to `orders` / `wishes`. 358 lines. **Non-idempotent** — running it against a fresh `v3.sql` database will fail with `42P07 duplicate object` errors on the FSD v2 tables.

## If you need to reconstruct historical state

`git log --follow <file>` will reveal when each file was last touched. The two files together represent the evolution from a pre-FSD-v2 single-pair model (`couple_relations`, `invitations`) to the current FSD-v2 group model (`association_groups`, `association_group_members`). The `v3.sql` consolidation drops the legacy single-pair artifacts in favor of the group model.
