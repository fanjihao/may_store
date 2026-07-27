use std::fs;
use std::path::{Path, PathBuf};

fn rust_string_literals(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut strings = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i..].starts_with(b"//") {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }

        if bytes[i..].starts_with(b"/*") {
            i += 2;
            let mut depth = 1;
            while i < bytes.len() && depth > 0 {
                if bytes[i..].starts_with(b"/*") {
                    depth += 1;
                    i += 2;
                } else if bytes[i..].starts_with(b"*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            continue;
        }

        let raw_prefix = if bytes[i] == b'r' {
            Some(i + 1)
        } else if bytes[i..].starts_with(b"br") {
            Some(i + 2)
        } else {
            None
        };

        if let Some(mut quote) = raw_prefix {
            let mut hashes = 0;
            while quote < bytes.len() && bytes[quote] == b'#' {
                hashes += 1;
                quote += 1;
            }
            if quote < bytes.len() && bytes[quote] == b'"' {
                let content_start = quote + 1;
                let mut end = content_start;
                while end < bytes.len() {
                    if bytes[end] == b'"'
                        && end + 1 + hashes <= bytes.len()
                        && bytes[end + 1..end + 1 + hashes]
                            .iter()
                            .all(|byte| *byte == b'#')
                    {
                        strings.push(source[content_start..end].to_string());
                        i = end + 1 + hashes;
                        break;
                    }
                    end += 1;
                }
                if end >= bytes.len() {
                    break;
                }
                continue;
            }
        }

        let quote = if bytes[i] == b'"' {
            Some(i)
        } else if bytes[i..].starts_with(b"b\"") {
            Some(i + 1)
        } else {
            None
        };

        if let Some(quote) = quote {
            let content_start = quote + 1;
            let mut end = content_start;
            while end < bytes.len() {
                if bytes[end] == b'\\' {
                    end += 2;
                } else if bytes[end] == b'"' {
                    strings.push(source[content_start..end].to_string());
                    i = end + 1;
                    break;
                } else {
                    end += 1;
                }
            }
            if end >= bytes.len() {
                break;
            }
            continue;
        }

        i += 1;
    }

    strings
}

fn strip_sql_comments(sql: &str) -> String {
    let bytes = sql.as_bytes();
    let mut clean = String::with_capacity(sql.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i..].starts_with(b"--") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if bytes[i..].starts_with(b"/*") {
            i += 2;
            while i < bytes.len() && !bytes[i..].starts_with(b"*/") {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
        } else {
            clean.push(bytes[i] as char);
            i += 1;
        }
    }

    clean
}

fn normalized_sql(sql: &str) -> Option<String> {
    let clean = strip_sql_comments(sql);
    let normalized = clean
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    let is_sql = ["select ", "insert ", "update ", "delete ", "with "]
        .iter()
        .any(|keyword| normalized.contains(keyword));
    is_sql.then_some(normalized)
}

fn contains_identifier(haystack: &str, needle: &str) -> bool {
    haystack.match_indices(needle).any(|(position, _)| {
        let before = haystack[..position].chars().next_back();
        let after = haystack[position + needle.len()..].chars().next();
        let is_identifier = |character: char| character.is_ascii_alphanumeric() || character == '_';
        before.map_or(true, |character| !is_identifier(character))
            && after.map_or(true, |character| !is_identifier(character))
    })
}

fn contains_bare_identifier(haystack: &str, needle: &str) -> bool {
    haystack.match_indices(needle).any(|(position, _)| {
        let before = haystack[..position].chars().next_back();
        let after = haystack[position + needle.len()..].chars().next();
        let is_identifier = |character: char| character.is_ascii_alphanumeric() || character == '_';
        before.map_or(true, |character| {
            !is_identifier(character) && character != '.'
        }) && after.map_or(true, |character| {
            !is_identifier(character) && character != '.'
        })
    })
}

fn select_projection(sql: &str) -> Option<&str> {
    let select = sql.find("select ")? + "select ".len();
    let from = sql[select..].find(" from ")? + select;
    Some(&sql[select..from])
}

fn reject_identifier(sql: &str, forbidden: &str, issues: &mut Vec<String>) {
    if contains_identifier(sql, forbidden) {
        issues.push(format!("废弃 schema 标识 `{forbidden}`"));
    }
}

fn achievement_category_rhs_has_cast(rhs: &str) -> bool {
    let cast = "::achievement_category_enum";
    if let Some(parameter) = rhs.strip_prefix('$') {
        let digits = parameter
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .count();
        return digits > 0 && parameter[digits..].starts_with(cast);
    }

    if let Some(literal) = rhs.strip_prefix('\'') {
        return literal
            .find('\'')
            .map_or(false, |end| literal[end + 1..].starts_with(cast));
    }

    true
}

fn sql_issues(sql: &str) -> Vec<String> {
    let Some(sql) = normalized_sql(sql) else {
        return Vec::new();
    };
    let mut issues = Vec::new();

    if contains_identifier(&sql, "record_group") {
        reject_identifier(&sql, "max_capacity", &mut issues);
        reject_identifier(&sql, "current_count", &mut issues);
    }

    if contains_identifier(&sql, "achievements") {
        for forbidden in [
            "a.id",
            "ua.id",
            "a.is_active",
            "display_order",
            "requirement_value",
        ] {
            reject_identifier(&sql, forbidden, &mut issues);
        }
    }

    if contains_identifier(&sql, "foods") {
        for forbidden in ["foods.tags", "f.tags", "f.name"] {
            reject_identifier(&sql, forbidden, &mut issues);
        }
        if select_projection(&sql).map_or(false, |projection| {
            contains_bare_identifier(projection, "tags")
        }) {
            issues.push("废弃 schema 标识 `foods.tags`".to_string());
        }
    }

    if contains_identifier(&sql, "tags") {
        reject_identifier(&sql, "tags.id", &mut issues);
        if contains_bare_identifier(&sql, "id") {
            issues.push("废弃 schema 标识 `tags.id`".to_string());
        }
    }

    if contains_identifier(&sql, "notifications") {
        reject_identifier(&sql, "notifications.id", &mut issues);
    }

    if contains_identifier(&sql, "achievements") {
        if let Some(projection) = select_projection(&sql) {
            let bad_category_read = projection.split(',').any(|item| {
                let expression = item.split(" as ").next().unwrap_or(item).trim();
                let reads_category = contains_identifier(expression, "a.category")
                    || contains_bare_identifier(expression, "category");
                let compact_expression: String =
                    expression.chars().filter(|c| !c.is_whitespace()).collect();
                reads_category && !compact_expression.contains("::text")
            });
            if bad_category_read {
                issues.push("achievement category 读取必须使用 `a.category::text`".to_string());
            }
        }

        let compact_sql: String = sql.chars().filter(|c| !c.is_whitespace()).collect();
        for (position, _) in compact_sql.match_indices("a.category=") {
            let rhs = &compact_sql[position + "a.category=".len()..];
            if !achievement_category_rhs_has_cast(rhs) {
                issues.push(
                    "achievement category 比较必须显式转换为 achievement_category_enum".to_string(),
                );
            }
        }
    }

    issues
}

fn rust_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];

    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path).expect("must read source directory") {
            let path = entry.expect("must read directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .map_or(false, |extension| extension == "rs")
            {
                files.push(path);
            }
        }
    }

    files
}

#[test]
fn live_rust_sql_does_not_use_retired_schema_identifiers() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut failures = Vec::new();

    for path in rust_files(&root) {
        let source = fs::read_to_string(&path).expect("Rust source must be UTF-8");
        for (index, literal) in rust_string_literals(&source).into_iter().enumerate() {
            for issue in sql_issues(&literal) {
                failures.push(format!(
                    "{} (SQL string #{index}): {issue}",
                    path.strip_prefix(&root)
                        .expect("source path must be below src")
                        .display()
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "发现运行时 SQL 与 v3 schema 不一致:\n{}",
        failures.join("\n")
    );
}

#[test]
fn scanner_ignores_rust_and_sql_comments() {
    let source = r##"
        // sqlx::query("SELECT f.name, max_capacity FROM foods f");
        /*
          sqlx::query(r#"SELECT a.is_active, requirement_value FROM achievements a"#);
        */
        let _ = sqlx::query(
            r#"SELECT f.food_name
               FROM foods f
               -- f.name and foods.tags are retired
               /* display_order and current_count are retired */"#,
        );
    "##;

    let issues: Vec<_> = rust_string_literals(source)
        .iter()
        .flat_map(|literal| sql_issues(literal))
        .collect();
    assert!(issues.is_empty(), "注释不应触发回归检查: {issues:?}");
}

#[test]
fn scanner_flags_known_schema_regressions() {
    for sql in [
        "SELECT max_capacity FROM record_group",
        "SELECT current_count FROM record_group",
        "SELECT a.id, a.is_active FROM achievements a",
        "SELECT a.code FROM achievements a ORDER BY display_order",
        "SELECT requirement_value FROM achievements",
        "SELECT foods.tags FROM foods",
        "SELECT f.name FROM foods f",
        "SELECT id FROM tags",
        "SELECT notifications.id FROM notifications",
        "SELECT a.category FROM achievements a",
        "SELECT a.code FROM achievements a WHERE a.category=$1",
    ] {
        assert!(!sql_issues(sql).is_empty(), "应识别已知 schema 回归: {sql}");
    }

    assert!(
        sql_issues(
            "SELECT a.category::text AS category FROM achievements a \
             WHERE a.category=$1::achievement_category_enum"
        )
        .is_empty(),
        "正确的 achievement enum 读写不应误报"
    );
}

#[test]
fn wish_cost_constraints_match_canonical_and_forward_schemas() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let normalize = |path: &Path| {
        fs::read_to_string(path)
            .expect("schema SQL must be readable")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase()
    };
    let canonical = normalize(&root.join("src/v3.sql"));
    let migration = normalize(&root.join("migrations/202607200002_wish_cost_constraints.sql"));

    for (table, constraint, column) in [
        ("wishes", "wishes_wish_cost_range_check", "wish_cost"),
        ("wishes", "wishes_initial_cost_range_check", "initial_cost"),
        ("wishes", "wishes_final_cost_range_check", "final_cost"),
        ("wishes", "wishes_claim_cost_range_check", "claim_cost"),
        (
            "wish_negotiations",
            "wish_negotiations_cost_range_check",
            "cost",
        ),
    ] {
        let definition = format!("constraint {constraint} check ({column} between 1 and 1000000)");
        assert!(
            canonical.contains(&definition),
            "{table}.{column} must have canonical constraint {constraint}"
        );
        assert!(
            migration.contains(&format!("add {definition} not valid")),
            "{constraint} must be installed as NOT VALID"
        );
        assert!(
            migration.contains(&format!("validate constraint {constraint}")),
            "{constraint} must be validated"
        );
    }
}

#[test]
fn wish_dual_feedback_contract_matches_canonical_and_forward_schemas() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let normalize = |path: &Path| {
        fs::read_to_string(path)
            .expect("schema SQL must be readable")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase()
    };
    let canonical = normalize(&root.join("src/v3.sql"));
    let migration = normalize(&root.join("migrations/202607230001_wish_dual_feedback.sql"));

    for column in [
        "points_frozen_at timestamptz",
        "creator_checkin_due_at timestamptz",
        "auto_completed_at timestamptz",
        "role_snapshot varchar(32)",
    ] {
        assert!(
            canonical.contains(column),
            "canonical schema missing {column}"
        );
        assert!(
            migration.contains(column),
            "forward migration missing {column}"
        );
    }
    for index in [
        "uniq_wish_feedback_user on wish_feedbacks(wish_id, user_id)",
        "uniq_active_claim_per_fulfiller on wishes(group_id, claimed_by)",
        "uniq_wish_point_settlement on love_point_transactions(biz_id)",
    ] {
        assert!(
            canonical.contains(index),
            "canonical schema missing {index}"
        );
        assert!(
            migration.contains(index),
            "forward migration missing {index}"
        );
    }
    assert!(
        migration.contains("drop constraint if exists wish_feedbacks_wish_id_key"),
        "forward migration must remove the historical one-feedback constraint"
    );
    assert!(
        migration.contains("'wish_finish_' || o.wish_id"),
        "forward migration must settle historical FINISHED freezes idempotently"
    );
    assert!(
        migration.contains("set claimed_by = fulfiller_id, selected_by = fulfiller_id"),
        "forward migration must normalize legacy claim ownership"
    );
}

#[test]
fn love_point_nonnegative_constraints_match_canonical_and_forward_schemas() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let normalize = |path: &Path| {
        fs::read_to_string(path)
            .expect("schema SQL must be readable")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase()
    };
    let canonical = normalize(&root.join("src/v3.sql"));
    let migration = normalize(&root.join("migrations/202607270001_nonnegative_love_points.sql"));

    for constraint in [
        "users_love_point_nonnegative_check",
        "ugp_available_love_point_nonnegative_check",
        "ugp_frozen_love_point_nonnegative_check",
        "ugp_love_point_nonnegative_check",
    ] {
        assert!(
            canonical.contains(constraint),
            "canonical schema missing {constraint}"
        );
        assert!(
            migration.contains(constraint),
            "forward migration missing {constraint}"
        );
    }
}

#[test]
fn admin_management_indexes_match_canonical_and_forward_schemas() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let normalize = |path: &Path| {
        fs::read_to_string(path)
            .expect("schema SQL must be readable")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase()
    };
    let canonical = normalize(&root.join("src/v3.sql"));
    let migration = normalize(&root.join("migrations/202607270002_admin_management_indexes.sql"));

    for index in [
        "idx_orders_pending_grant_review",
        "idx_orders_risk_created",
        "idx_wishes_pending_quality",
        "idx_audit_created_id",
        "idx_audit_target_created",
    ] {
        assert!(
            canonical.contains(index),
            "canonical schema missing {index}"
        );
        assert!(
            migration.contains(index),
            "forward migration missing {index}"
        );
    }
}

#[test]
fn order_reward_backfill_uses_only_reward_transactions() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let migration = fs::read_to_string(
        root.join("migrations/202607270003_backfill_order_reward_snapshots.sql"),
    )
    .expect("order reward backfill must be readable")
    .to_ascii_uppercase();

    assert!(migration.contains("'ORDER_REWARD_REVIEW'"));
    assert!(migration.contains("TYPE = 'EARN'"));
    assert!(!migration.contains("'ORDER_RATING'"));
}

#[test]
fn group_member_count_is_derived_and_kept_in_sync() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let canonical = fs::read_to_string(root.join("src/v3.sql"))
        .expect("canonical schema must be readable")
        .to_ascii_lowercase();
    let migration =
        fs::read_to_string(root.join("migrations/202607270004_sync_group_member_count.sql"))
            .expect("member count migration must be readable")
            .to_ascii_lowercase();

    for sql in [&canonical, &migration] {
        assert!(sql.contains("sync_group_member_count"));
        assert!(sql.contains("trg_sync_group_member_count"));
        assert!(sql.contains("member_status = 'active'::group_member_status_enum"));
    }
}
