#!/usr/bin/env python3
"""
一次性扫描所有 .rs, 给未强转的 enum 字面量自动补 ::xxx_enum 强转.

启发式:
- 仅在 SQL 字符串里改
- 已知 (table, col) -> enum 的映射
- 如果 'LITERAL' 紧跟的字面量没 ::xxx 强转, 自动补
- 排除已经 ::text / ::jsonb 之类显式转的
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
V3_SQL = ROOT / "src" / "v3.sql"
DEFAULT_SCAN_DIR = ROOT / "src"


def parse_v3_sql(path):
    enum_types = set()
    enum_columns = {}  # (table, col) -> enum_type
    lines = path.read_text(encoding="utf-8").splitlines()
    current_table = None
    in_ct = False
    for i, line in enumerate(lines):
        s = line.strip()
        m = re.match(r"CREATE TABLE\s+(\w+)\s*\(", s)
        if m:
            current_table = m.group(1)
            continue
        m = re.match(r"CREATE TYPE\s+(\w+)\s+AS\s+ENUM\s*\(", s)
        if m:
            enum_types.add(m.group(1))
            in_ct = True
            continue
        if in_ct:
            if s.startswith(")"):
                in_ct = False
            continue
        if not current_table or in_ct or s.startswith("--"):
            continue
        tokens = s.replace(",", " ").split()
        if not tokens or tokens[0].upper() in ("PRIMARY", "FOREIGN", "UNIQUE", "CHECK", "CONSTRAINT", "INDEX", "ALTER", "DROP"):
            continue
        col = tokens[0]
        skip = {"NOT", "NULL", "DEFAULT", "REFERENCES", "COLLATE", "PRIMARY", "KEY"}
        for tok in tokens[1:]:
            if tok in skip:
                continue
            if tok.startswith("'"):
                continue
            clean = tok.rstrip(",;").rstrip()
            if clean in enum_types:
                enum_columns[(current_table, col)] = clean
                break
            if clean.endswith("[]"):
                break
    return enum_columns


# enum 类型 -> 典型列名 (用于按列名找 enum, 因为 SQL 里通常不写表名)
def build_col_to_enum(enum_columns):
    col_to_enum = {}
    for (t, c), et in enum_columns.items():
        col_to_enum.setdefault(c, set()).add(et)
    return col_to_enum


# 已知 (col, enum_value) -> enum_type 的字面量映射
# 用于按字面量精确判断 (比 col_to_enum 准, 解决 status/type 通用列名歧义)
KNOWN_LITERALS = {
    # member_status
    ("member_status", "ACTIVE"): "group_member_status_enum",
    ("member_status", "LEFT"):   "group_member_status_enum",
    # role_in_group
    ("role_in_group", "RECEIVING"): "group_member_role_enum",
    ("role_in_group", "ORDERING"):  "group_member_role_enum",
    # mark_type
    ("mark_type", "FAVORITE"):   "mark_type_enum",
    ("mark_type", "DISLIKE"):    "mark_type_enum",
    # user_status
    ("status", "ACTIVE"):     "user_status_enum",  # users.status
    ("status", "DELETED"):    "user_status_enum",
    ("status", "BANNED"):     "user_status_enum",
    # user_role
    ("role", "ORDERING"):     "user_role_enum",
    ("role", "RECEIVING"):    "user_role_enum",
    ("role", "ADMIN"):        "user_role_enum",  # also admin_role_enum, ambiguous
    # admin_role
    ("role", "SUPER_ADMIN"):  "admin_role_enum",
    # wish_status
    ("status", "DRAFT"):      "wish_status_enum",
    ("status", "NEGOTIATING"):"wish_status_enum",
    ("status", "CLAIMED"):    "wish_status_enum",
    ("status", "FINISHED"):   "wish_status_enum",
    ("status", "EXPIRED"):    "wish_status_enum",
    ("status", "CLOSED"):     "wish_status_enum",
    # order_status
    ("status", "CREATED"):            "order_status_enum",
    ("status", "ACCEPTED"):           "order_status_enum",
    ("status", "PRODUCTION_COMPLETED"): "order_status_enum",
    ("status", "CONFIRMED_COMPLETED"):  "order_status_enum",
    ("status", "CONFIRMED_INCOMPLETE"): "order_status_enum",
    ("status", "REJECTED"):           "order_status_enum",
    ("status", "CANCELLED"):          "order_status_enum",
    ("status", "TIMEOUT"):            "order_status_enum",
    ("status", "PENDING_ACCEPT"):     "order_status_enum",
    ("status", "IN_PROGRESS"):        "order_status_enum",
    ("status", "BREEDER_FINISHED"):   "order_status_enum",
    ("status", "COMPLETED"):          "order_status_enum",  # legacy
    # love_point_tx_type
    ("type", "EARN"):     "love_point_tx_type_enum",
    ("type", "FREEZE"):   "love_point_tx_type_enum",
    ("type", "UNFREEZE"): "love_point_tx_type_enum",
    ("type", "DEDUCT"):   "love_point_tx_type_enum",
    ("type", "ADJUST"):   "love_point_tx_type_enum",
    # diamond_tx_type (既有 EARN/ADJUST 又可能有 CONSUME 等)
    ("type", "CONSUME"):  "diamond_tx_type_enum",
    # group_exp_tx_type
    # food_status
    ("food_status", "NORMAL"):  "food_status_enum",
    ("food_status", "DISABLED"): "food_status_enum",
    # apply_status
    ("apply_status", "PENDING"): "apply_status_enum",
    ("apply_status", "APPROVED"): "apply_status_enum",
    ("apply_status", "REJECTED"): "apply_status_enum",
    ("from_status", "PENDING"):  "apply_status_enum",  # food_audit_logs.from_status
    ("from_status", "APPROVED"): "apply_status_enum",
    ("from_status", "REJECTED"): "apply_status_enum",
    ("to_status", "PENDING"):    "apply_status_enum",  # food_audit_logs.to_status
    ("to_status", "APPROVED"):   "apply_status_enum",
    ("to_status", "REJECTED"):   "apply_status_enum",
    # food_status
    ("food_status", "NORMAL"):   "food_status_enum",
    ("food_status", "DISABLED"): "food_status_enum",
    ("food_status", "DELETED"):  "food_status_enum",
    # wish_negotiation_action
    ("action", "QUOTE"):        "wish_negotiation_action_enum",
    ("action", "COUNTER"):      "wish_negotiation_action_enum",
    ("action", "SET_DEADLINE"): "wish_negotiation_action_enum",
    ("action", "ACCEPT"):       "wish_negotiation_action_enum",
    ("action", "REJECT"):       "wish_negotiation_action_enum",
    ("action", "CLOSE"):        "wish_negotiation_action_enum",
    # wish_quality_status
    ("quality_review_status", "PENDING"):  "wish_quality_status_enum",
    ("quality_review_status", "REVIEWED"): "wish_quality_status_enum",
    # event_status
    ("status", "PENDING"):    "event_status_enum",
    ("status", "PROCESSING"): "event_status_enum",
    ("status", "DONE"):       "event_status_enum",
    ("status", "FAILED"):     "event_status_enum",
    # group_type
    ("group_type", "DEFAULT"):   "group_type_enum",
    ("group_type", "CUSTOM"):    "group_type_enum",
    # point_grant_status
    ("point_grant_status", "NONE"):          "point_grant_status_enum",
    ("point_grant_status", "PENDING"):       "point_grant_status_enum",
    ("point_grant_status", "PENDING_REVIEW"): "point_grant_status_enum",
    ("point_grant_status", "GRANTED"):       "point_grant_status_enum",
    ("point_grant_status", "REVOKED"):       "point_grant_status_enum",
    # exp_grant_status
    ("exp_grant_status", "NONE"):    "exp_grant_status_enum",
    ("exp_grant_status", "PENDING"): "exp_grant_status_enum",
    ("exp_grant_status", "GRANTED"): "exp_grant_status_enum",
    # risk_status
    ("risk_status", "PASS"):     "risk_status_enum",
    ("risk_status", "SUSPECT"):  "risk_status_enum",
    ("risk_status", "REJECTED"): "risk_status_enum",
    # order_type
    ("type", "NORMAL"):   "order_type_enum",
    ("type", "GUEST"):    "order_type_enum",
    # submit_role
    ("submit_role", "BREEDER"):   "submit_role_enum",
    ("submit_role", "ORDERING"):  "submit_role_enum",
    # cart_status
    ("status", "ACTIVE_CART"): "cart_status_enum",
    # message_status
    ("status", "UNREAD"):   "message_status_enum",
    ("status", "READ"):     "message_status_enum",
    # feedback_status
    ("status", "OPEN"):     "feedback_status_enum",
    ("status", "CLOSED"):   "feedback_status_enum",
    # notification_type
    ("type", "ORDER_NOTIFY"):      "notification_type_enum",
    ("type", "WISH_NOTIFY"):       "notification_type_enum",
    ("type", "SIGN_NOTIFY"):       "notification_type_enum",
    # audit_action (admin audit_logs)
    ("action", "CREATE"):     "audit_action_enum",
    ("action", "UPDATE"):     "audit_action_enum",
    ("action", "DELETE"):     "audit_action_enum",
    ("action", "LOGIN"):      "audit_action_enum",
    # upload_business_ref
    ("biz_type", "FOOD"):     "upload_business_ref_enum",
    ("biz_type", "AVATAR"):   "upload_business_ref_enum",
    ("biz_type", "FEEDBACK"): "upload_business_ref_enum",
    # mark_type 扩展
    ("mark_type", "LIKE"):    "mark_type_enum",
    ("mark_type", "WANT"):    "mark_type_enum",
    # lottery_success
    ("result", "WIN"):     "lottery_success_enum",
    ("result", "LOSE"):    "lottery_success_enum",
    # admin_role
    ("role", "SUPER_ADMIN"): "admin_role_enum",
    # user_status
    ("status", "BANNED"):     "user_status_enum",
    # category (multi-type, ambiguous, skip)
}


# 哪些字面量属于 INSERT 场景的 '$N' bind, 需要单独 patch
# 因为 INSERT VALUES ($1, $2, ...) 中没有字面量, 需要看列名
# 规则: 如果 INSERT 的第 N 列是 enum, 就 patch 那个 $N
# (由 patch_insert_bind 处理, 不用在 KNOWN_LITERALS 里)
INSERT_BIND_COL_ENUMS = {
    # (table_name, col_name) -> enum_type
    # 从 v3.sql 自动构建, 这里只是样例
}


SQL_STRING_RE = re.compile(r'"((?:[^"\\]|\\.)*)"', re.DOTALL)

# (col, lit) 已知对应 enum 的字面量, 用于 SET col = 'lit' 形式
# 已有 KNOWN_LITERALS, 现在再加 INSERT/UPDATE 的 $N bind patch


def patch_insert_select(sql: str, enum_columns: dict):
    """
    处理 INSERT INTO tbl (cols...) SELECT ... 形式.
    SELECT 后面的字段按列顺序对应, 给字面量和 $N 补强转.
    """
    changes = 0
    # 匹配 INSERT INTO tbl (cols) SELECT expr1, expr2, ...
    m = re.search(
        r"INSERT\s+INTO\s+(\w+)\s*\(([^)]+)\)\s*SELECT\s+(.*?)(?:\bFROM\b|$)",
        sql, re.IGNORECASE | re.DOTALL,
    )
    if not m:
        return sql, 0
    tbl = m.group(1)
    cols_str = m.group(2)
    select_part = m.group(3)

    cols = [c.strip() for c in cols_str.split(",") if c.strip()]
    # 把 select_part 按顶层逗号切 (注意 FROM 关键字已被前面 ?:\bFROM\b 排除, 但子查询里也可能有 FROM)
    # 简化: 处理顶层 SELECT expr1, expr2, expr3 FROM x
    # 顶层切到 FROM 为止
    # 但我们只关心前 N 个字面量/参数, 不必深拆
    # 简化处理: 用一个简易 split, 不支持嵌套
    # 实际我们看到的多是扁平  SELECT $1, 'EARN', $2, ... 形式

    # 拆 SELECT 后的字段
    depth = 0
    cur = ""
    parts = []
    for ch in select_part:
        if ch == "(":
            depth += 1
            cur += ch
        elif ch == ")":
            depth -= 1
            cur += ch
        elif ch == "," and depth == 0:
            parts.append(cur.strip())
            cur = ""
        else:
            cur += ch
    if cur.strip():
        parts.append(cur.strip())

    new_parts = list(parts)
    for i, col in enumerate(cols, start=1):
        if i > len(new_parts):
            break
        et = enum_columns.get((tbl, col))
        if not et:
            continue
        p = new_parts[i - 1].strip()
        # 字面量
        lm = re.match(r"^'([^']*)'$", p)
        if lm:
            lit = lm.group(1)
            if f"::{et}" not in p:
                new_parts[i - 1] = f"'{lit}'::{et}"
                changes += 1
                continue
        # $N bind
        bm = re.match(r"^\$(\d+)$", p)
        if bm:
            n = bm.group(1)
            if f"::{et}" not in p:
                new_parts[i - 1] = f"${n}::{et}"
                changes += 1

    if changes:
        new_select = ", ".join(new_parts)
        # 替换原 SELECT ... 段
        # 找完整 SELECT 段
        old_select_match = re.search(
            r"SELECT\s+(.*?)(?:\bFROM\b)",
            sql, re.IGNORECASE | re.DOTALL,
        )
        if old_select_match:
            old_select = old_select_match.group(1)
            sql = sql.replace(f"SELECT {old_select}", f"SELECT {new_select}", 1)
    return sql, changes


def patch_bind_params(sql: str, enum_columns: dict):
    """
    处理 INSERT/UPDATE 中 $N 绑到 enum 列的强转.

    1) INSERT INTO tbl (col1, col2, ...) VALUES ($1, $2, ...):
       如果第 N 列是 enum, 把 VALUES 中的 $N 改成 $N::enum
    2) UPDATE tbl SET col1 = $1, col2 = $2:
       如果 col 是 enum, 把 $N 改成 $N::enum
    """
    changes = 0

    # --- INSERT 形式 ---
    m = re.search(r"INSERT\s+INTO\s+(\w+)\s*\(([^)]+)\)", sql, re.IGNORECASE)
    if m:
        tbl = m.group(1)
        cols = [c.strip() for c in m.group(2).split(",") if c.strip()]
        # 找 VALUES ( ... ) 块
        vm = re.search(r"VALUES\s*\(([^)]+)\)", sql, re.IGNORECASE)
        if vm:
            values = [v.strip() for v in vm.group(1).split(",")]
            # 一一对应 patch
            for i, col in enumerate(cols, start=1):
                if i > len(values):
                    break
                et = enum_columns.get((tbl, col))
                if not et:
                    continue
                v = values[i - 1]
                # 如果 v 是 $N
                bm = re.match(r"^\$(\d+)$", v)
                if bm:
                    n = bm.group(1)
                    new_v = f"${n}::{et}"
                    if new_v != v:
                        # 在原 sql 中替换
                        # 简化: 找 VALUES (..., $N, ...) 中第 i 个 $N
                        # 因为可能有多组 VALUES (multi-row insert), 暂时只处理单组
                        old_full = f"({m.group(2)})"
                        new_full_sql = sql  # we'll do the replacement below
                        # 找 VALUES (... ,$N, ...) 中的第 i 个 $N
                        # 用 finditer 找所有 "$N" token in VALUES 段
                        vals_match = re.search(r"VALUES\s*\(([^)]+)\)", sql, re.IGNORECASE)
                        if vals_match:
                            body = vals_match.group(1)
                            tokens = [t.strip() for t in body.split(",")]
                            if i <= len(tokens) and tokens[i-1] == f"${n}":
                                # 替换
                                new_tokens = list(tokens)
                                new_tokens[i-1] = new_v
                                new_body = ", ".join(new_tokens)
                                sql = sql.replace(f"VALUES ({body})", f"VALUES ({new_body})")
                                changes += 1
                # 如果 v 是 'lit' 字面量 (例如 'NEGOTIATING')
                elif re.match(r"^'[^']*'$", v):
                    # 字面量没强转, 补上
                    lit = v.strip("'")
                    if (col, lit) in KNOWN_LITERALS:
                        et2 = KNOWN_LITERALS[(col, lit)]
                        new_v = f"{v}::{et2}"
                        if new_v != v:
                            vals_match = re.search(r"VALUES\s*\(([^)]+)\)", sql, re.IGNORECASE)
                            if vals_match:
                                body = vals_match.group(1)
                                tokens = [t.strip() for t in body.split(",")]
                                if i <= len(tokens) and tokens[i-1] == v:
                                    new_tokens = list(tokens)
                                    new_tokens[i-1] = new_v
                                    new_body = ", ".join(new_tokens)
                                    sql = sql.replace(f"VALUES ({body})", f"VALUES ({new_body})")
                                    changes += 1

    # --- UPDATE SET 形式 ---
    # 找 "SET col1 = expr1, col2 = expr2, ..."
    m = re.search(r"UPDATE\s+(\w+)\s+SET\s+(.*?)(?:\s+WHERE\b|$)", sql, re.IGNORECASE | re.DOTALL)
    if m:
        tbl = m.group(1)
        body = m.group(2)
        # 拆 token, 简单 split by comma (可能 CASE WHEN 里有逗号, 简化: 假设没有)
        depth = 0
        cur = ""
        parts = []
        for ch in body:
            if ch == "(":
                depth += 1
                cur += ch
            elif ch == ")":
                depth -= 1
                cur += ch
            elif ch == "," and depth == 0:
                parts.append(cur.strip())
                cur = ""
            else:
                cur += ch
        if cur.strip():
            parts.append(cur.strip())

        for part in parts:
            if "=" not in part:
                continue
            col, rhs = part.split("=", 1)
            col = col.strip()
            rhs = rhs.strip()
            et = enum_columns.get((tbl, col))
            if not et:
                continue
            # rhs 是否含 $N 或 字面量
            bm = re.match(r"^\$(\d+)$", rhs)
            if bm:
                n = bm.group(1)
                new_rhs = f"${n}::{et}"
                if f"::{et}" not in rhs:
                    sql = sql.replace(f"SET {col} = {rhs}", f"SET {col} = {new_rhs}")
                    changes += 1
            elif re.match(r"^'[^']*'$", rhs):
                lit = rhs.strip("'")
                # 字面量强转: 用 KNOWN_LITERALS 找 enum (但要排除错误匹配)
                # 如果 enum_columns[(tbl, col)] 已经能确定, 直接用
                new_rhs = f"{rhs}::{et}"
                if f"::{et}" not in rhs:
                    sql = sql.replace(f"SET {col} = {rhs}", f"SET {col} = {new_rhs}")
                    changes += 1
    return sql, changes


def patch_sql(sql: str, enum_columns: dict):
    """
    在 SQL 字符串里:
    1) 给未强转的 enum 字面量自动加 ::xxx_enum
    2) 给 INSERT/UPDATE 中 $N bind 到 enum 列的加 ::xxx_enum
    返回 (新SQL, 修改数).
    """
    changes = 0
    # Step 1: 字面量 (col = 'lit', IN, SET)
    sql, c = patch_sql_literals(sql)
    changes += c
    # Step 2: INSERT INTO ... SELECT
    sql, c = patch_insert_select(sql, enum_columns)
    changes += c
    # Step 3: INSERT/UPDATE 的 $N bind
    sql, c = patch_bind_params(sql, enum_columns)
    changes += c
    return sql, changes


def patch_sql_literals(sql: str):
    """
    旧 patch_sql 的逻辑拆出来.
    """
    changes = 0
    for (col, lit), et in KNOWN_LITERALS.items():
        # 1) col = 'lit'
        pat1 = re.compile(rf"\b{re.escape(col)}\s*=\s*'{re.escape(lit)}'")
        def repl(m, et=et):
            nonlocal changes
            start, end = m.span()
            after = sql[end:end+30]
            if f"::{et}" in after:
                return m.group(0)
            changes += 1
            return m.group(0) + f"::{et}"
        sql = pat1.sub(repl, sql)

        # 2) IN (x, y)
        def fix_in_block(m, col=col, lit=lit, et=et):
            nonlocal changes
            body = m.group(2)
            items = re.findall(r"'([^']*)'", body)
            if lit not in items:
                return m.group(0)
            if f"'{lit}'::{et}" in body:
                return m.group(0)
            new_body = re.sub(rf"'{re.escape(lit)}'(?!::)", f"'{lit}'::{et}", body)
            if new_body != body:
                changes += 1
            return m.group(1) + new_body + m.group(3)

        pat2 = re.compile(rf"(\b{re.escape(col)}\s+IN\s*\()([^)]+)(\))", re.IGNORECASE)
        sql = pat2.sub(fix_in_block, sql)

        # 3) SET col = 'lit'
        pat3 = re.compile(rf"(SET\s+{re.escape(col)}\s*=\s*)'{re.escape(lit)}'")
        def repl_set(m, et=et):
            nonlocal changes
            start, end = m.span()
            after = sql[end:end+30]
            if f"::{et}" in after:
                return m.group(0)
            changes += 1
            return m.group(0) + f"::{et}"
        sql = pat3.sub(repl_set, sql)

    return sql, changes


def patch_sql_old(sql: str):  # 已废弃, 保留作参考, 主函数不调用
    """兼容旧调用, 走 patch_sql_literals"""
    return patch_sql_literals(sql)


def main():
    scan_dir = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_SCAN_DIR
    enum_columns = parse_v3_sql(V3_SQL)
    print(f"[fix] v3.sql: {len(enum_columns)} 个 (table,col) -> enum 映射")

    total_files = 0
    total_changes = 0
    for rs_path in sorted(scan_dir.rglob("*.rs")):
        if not rs_path.is_file():
            continue
        text = rs_path.read_text(encoding="utf-8")
        file_changes = 0
        out = []
        last = 0
        for m in SQL_STRING_RE.finditer(text):
            out.append(text[last:m.start()])
            sql = m.group(1)
            # 启发: 长度>10 且含 SQL 关键字
            if len(sql) > 10 and re.search(r"\b(SELECT|INSERT|UPDATE|DELETE|WITH)\b", sql, re.IGNORECASE):
                new_sql, c = patch_sql(sql, enum_columns)
                file_changes += c
                sql = new_sql
            out.append('"' + sql + '"')
            last = m.end()
        out.append(text[last:])

        if file_changes:
            new_text = "".join(out)
            rs_path.write_text(new_text, encoding="utf-8")
            rel = rs_path.relative_to(ROOT)
            print(f"  ✓ {rel}: {file_changes} 处强转")
            total_files += 1
            total_changes += file_changes

    print(f"\n[fix] 完成: 改了 {total_files} 个文件, 共 {total_changes} 处强转")


if __name__ == "__main__":
    main()
