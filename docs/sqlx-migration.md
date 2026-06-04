# sqlx Compile-Time SQL Validation

This project uses [`sqlx`](https://github.com/launchbadge/sqlx) version 0.8 with the `query!` and `query_as!` compile-time-checked macros. These macros validate SQL strings against the live database at compile time, catching column-name, type, and nullability mismatches before the code ever runs.

## Quick start

### 1. One-time dev-machine setup

Install `sqlx-cli` (the offline cache tool):

```bash
cargo install sqlx-cli --version 0.8.0 --no-default-features --features postgres,rustls
```

The project already has `sqlx = "0.8.0"` with the `macros` feature enabled, so no Cargo.toml change is required for the macro to work.

### 2. Build (two modes)

**Online mode (default — recommended for local dev):**
```bash
cargo build
```
Requires a running Postgres reachable at the URL in `.env`'s `DATABASE_URL`. The macro opens a short-lived connection to validate each query's column types and nullability.

**Offline mode (hermetic, no DB needed):**
```bash
SQLX_OFFLINE=true cargo build
```
Reads prepared query metadata from `.sqlx/` (committed in this repo) instead of connecting to a DB. Use this in CI and on machines that don't have a local Postgres.

### 3. When you change a query

If you edit a `query!` / `query_as!` invocation, **regenerate the offline cache**:

```bash
cargo sqlx prepare
```

This rewrites the JSON files in `.sqlx/`. Commit the updated cache alongside your code change.

To verify the cache matches the live DB (CI gate):
```bash
cargo sqlx prepare --check
```
This exits non-zero if the cache is stale. Run it in CI before `cargo build` to catch drift.

## Conventions in this codebase

When using `query_as!` against an entity with custom Postgres enums (e.g. `user_role_enum`, `gender_enum`, `login_method_enum`):

```sql
u.role::text AS "role!: UserRole"
```

The `::text` cast is needed because the macro does not understand custom Postgres enum types directly. The `!` forces non-null (since the original column is `NOT NULL`). For nullable columns that map to `Option<T>` fields, use `?` instead. For `Option<T>` fields where the column is `NOT NULL` (rare; usually a struct smell), use `!: Option<T>` to opt into the wrapper.

Subqueries that return a possibly-empty result need a `!:` or `?:` override so the macro knows the type and nullability:
```sql
(SELECT agm.group_id FROM ... LIMIT 1) AS "group_id!: Option<i64>"
```

## Pilot

`src/middlewares/auth.rs` is the first file converted to the compile-time macro. Every authenticated request flows through this query, so any regression surfaces immediately. It also exercises the subquery override (`group_id`) and the enum `::text` cast pattern.

## Out of scope (deliberately)

- **Full conversion of all 343 runtime `sqlx::query()` / `sqlx::query_as()` call sites.** Many use `format!` to interpolate SQL fragments (SQL-injection smell) or `QueryBuilder` for dynamic WHERE clauses — those need a separate refactor, not just a macro conversion.
- **The 16 `format!`-built SQL fragments in route handlers** (notifications, sign_in, wishes, achievement, footprints, economy) interpolate user-controlled values without binding. These are SQL-injection risks and deserve a dedicated security-focused PR.
- **The 5 `QueryBuilder` chains** in `order_service.rs`, `food_service.rs`, `footprint_service.rs` cannot be converted to compile-time macros at all; they need a different pattern.
