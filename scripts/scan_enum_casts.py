#!/usr/bin/env python3
"""
扫描全仓 .rs 文件中所有 SQL, 找出可能漏写 ::xxx_enum 强转的地方.

只盯真正会炸的两类:
  HIGH   INSERT/UPDATE 的值位 (VALUES/SET) 有 $N bind 或 'X' 字面量, 写入 enum 列但没 ::enum_name 强转
  MEDIUM WHERE 条件 col = $N 或 col = 'X' 或 col IN (...), 右边没 ::enum_name 强转 (PG 不会隐式转 text 到 enum)

SELECT 里读 enum 列 (`SELECT col`) 不报, 因为读出来是 enum, Rust 端 sqlx 会自动用 FromRow 解码成 derive(FromRow) 的 String 字段,
加 ::text 也是为了转字符串, 不是这个 bug 的场景.

用法:
  python3 scripts/scan_enum_casts.py [scan_dir]
"""
import re
import sys
from pathlib import Path
from collections import defaultdict

ROOT = Path(__file__).resolve().parent.parent
V3_SQL = ROOT / "src" / "v3.sql"
DEFAULT_SCAN_DIR = ROOT / "src"


# ---------- 1. 解析 v3.sql: 提取 enum 类型 + 列归属 ----------

def parse_v3_sql(path: Path):
    """
    返回:
      enum_types: set[str]
      enum_columns: dict[enum_name, set[(table, col)]]
      col_to_enum: dict[col_name, set[enum_name]]  反向索引, 用于按列名快速找 enum
    """
    enum_types = set()
    enum_columns = defaultdict(set)
    col_to_enum = defaultdict(set)

    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()

    current_table = None
    in_create_type = False
    i = 0
    while i < len(lines):
        line = lines[i].strip()

        m = re.match(r"CREATE TABLE\s+(\w+)\s*\(", line)
        if m:
            current_table = m.group(1)
            i += 1
            continue

        m = re.match(r"CREATE TYPE\s+(\w+)\s+AS\s+ENUM\s*\(", line)
        if m:
            enum_types.add(m.group(1))
            in_create_type = True
            i += 1
            continue
        if in_create_type:
            if line.startswith(")"):
                in_create_type = False
            i += 1
            continue

        if current_table and not in_create_type and not line.startswith("--"):
            tokens = line.replace(",", " ").split()
            if not tokens or tokens[0].upper() in (
                "PRIMARY", "FOREIGN", "UNIQUE", "CHECK", "CONSTRAINT", "INDEX", "ALTER", "DROP"
            ):
                i += 1
                continue
            col_name = tokens[0]
            skip_next = {"NOT", "NULL", "DEFAULT", "REFERENCES", "COLLATE", "PRIMARY", "KEY"}
            for tok in tokens[1:]:
                if tok in skip_next:
                    continue
                if tok.startswith("'"):
                    continue
                clean = tok.rstrip(",").rstrip(";")
                if clean in enum_types:
                    enum_columns[clean].add((current_table, col_name))
                    col_to_enum[col_name].add(clean)
                    break
                if clean.endswith("[]"):
                    break
        i += 1
    return enum_types, enum_columns, col_to_enum


# ---------- 2. 提取 .rs 文件中的 SQL 字符串 ----------

SQL_STRING_RE = re.compile(r'"((?:[^"\\]|\\.)*)"', re.DOTALL)
SQL_HINT_RE = re.compile(r"\b(SELECT|INSERT|UPDATE|DELETE|WITH)\b", re.IGNORECASE)


def extract_sql_strings(rs_path: Path):
    """返回 [(line_no, sql_text)]"""
    text = rs_path.read_text(encoding="utf-8")
    out = []
    for m in SQL_STRING_RE.finditer(text):
        s = m.group(1)
        if SQL_HINT_RE.search(s) and len(s) > 10:
            line_no = text[: m.start()].count("\n") + 1
            out.append((line_no, s))
    return out


# ---------- 3. 风险评估 ----------

# 找所有 ::xxx 强转
CAST_RE = re.compile(r"::\s*(\w+)", re.IGNORECASE)
# 找所有 $N 参数
BIND_RE = re.compile(r"\$(\d+)")


def find_casts(sql: str, enum_types: set):
    """SQL 里出现的 ::xxx, 其中哪些 enum 被覆盖了"""
    found = set()
    for m in CAST_RE.finditer(sql):
        if m.group(1) in enum_types:
            found.add(m.group(1))
    return found


def extract_insert_columns(sql: str):
    """
    解析 INSERT INTO tbl (col1, col2, ...) 的列名列表.
    返回 [col_name] 顺序.
    """
    m = re.search(r"INSERT\s+INTO\s+\w+\s*\(([^)]+)\)", sql, re.IGNORECASE)
    if not m:
        return []
    cols = [c.strip() for c in m.group(1).split(",") if c.strip()]
    return cols


def extract_update_set(sql: str):
    """
    解析 UPDATE tbl SET col1 = expr, col2 = expr, ...
    返回 [(col_name, rhs_expr)] 顺序.
    """
    m = re.search(r"UPDATE\s+\w+\s+SET\s+(.*?)(?:\s+WHERE\b|$)", sql, re.IGNORECASE | re.DOTALL)
    if not m:
        return []
    out = []
    body = m.group(1)
    # 简单按逗号 split, 但 CASE WHEN 里可能含逗号, 简化处理
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
    for p in parts:
        if "=" not in p:
            continue
        k, v = p.split("=", 1)
        out.append((k.strip(), v.strip()))
    return out


def extract_where_clauses(sql: str):
    """
    找 WHERE col = ... / WHERE col IN (...) / WHERE col <> ...
    返回 [(col, op, rhs), ...]
    """
    out = []
    m = re.search(r"\bWHERE\b(.*?)(?:\bORDER\b|\bGROUP\b|\bLIMIT\b|\bRETURNING\b|$)",
                  sql, re.IGNORECASE | re.DOTALL)
    if not m:
        return out
    body = m.group(1)

    # col = 'X'  /  col = $N  /  col = expr
    for m2 in re.finditer(r"\b(\w+)\s*(=|<>|!=)\s*('[^']*'|\$\d+|\([^)]+\))", body):
        out.append((m2.group(1), m2.group(2), m2.group(3)))
    # col IN (x, y, ...)
    for m2 in re.finditer(r"\b(\w+)\s+IN\s*\(([^)]+)\)", body, re.IGNORECASE):
        out.append((m2.group(1), "IN", m2.group(2)))
    # 同样扫 ON/USING/HAVING
    for kw in ("ON", "USING", "HAVING"):
        for m2 in re.finditer(rf"\b{kw}\b(.*?)(?:\bWHERE\b|\bORDER\b|\bGROUP\b|\bLIMIT\b|$)",
                              sql, re.IGNORECASE | re.DOTALL):
            seg = m2.group(1)
            for m3 in re.finditer(r"\b(\w+)\s*(=|<>|!=)\s*('[^']*'|\$\d+)", seg):
                out.append((m3.group(1), m3.group(2), m3.group(3)))
    return out


def has_text_to_enum(rhs: str, enum_type: str) -> bool:
    """rhs 中是否包含 ::enum_type 强转"""
    return f"::{enum_type}" in rhs


def analyze_sql(sql: str, col_to_enum: dict, enum_types: set):
    """
    返回 [(risk, col, enum_type, hint, snippet)] 列表.
    """
    findings = []
    sql_oneline = " ".join(sql.split())
    cast_enums = find_casts(sql, enum_types)

    # === 1) INSERT 风险 ===
    if re.search(r"\bINSERT\s+INTO\b", sql, re.IGNORECASE):
        cols = extract_insert_columns(sql)
        for i, col in enumerate(cols, start=1):
            if col not in col_to_enum:
                continue
            et = next(iter(col_to_enum[col]))
            if et in cast_enums:
                continue  # 整 SQL 已强转, 通常已覆盖所有 $N
            # 看 $i 是否被强转
            rhs_pat = re.search(rf"\$\b{i}\b", sql)
            if not rhs_pat:
                continue
            # 找 $i 周围的 30 字符看是否有 ::et
            pos = rhs_pat.start()
            nearby = sql[max(0, pos - 5) : pos + 50]
            if f"::{et}" in nearby:
                continue
            findings.append(("HIGH", col, et,
                              f"INSERT 第 {i} 列 ({col}/{et}) bind $${i} 无 ::{et} 强转",
                              sql_oneline[:200]))

    # === 2) UPDATE SET 风险 ===
    if re.search(r"\bUPDATE\s+\w+\s+SET\b", sql, re.IGNORECASE):
        for col, rhs in extract_update_set(sql):
            if col not in col_to_enum:
                continue
            et = next(iter(col_to_enum[col]))
            if et in cast_enums:
                continue
            if has_text_to_enum(rhs, et):
                continue
            # rhs 是否含 'X' 或 $N
            if re.search(r"'[^']*'|\$\d+", rhs):
                findings.append(("HIGH", col, et,
                                f"UPDATE SET {col} = ... (类型 {et}) 值无 ::{et} 强转",
                                sql_oneline[:200]))

    # === 3) WHERE 风险 ===
    for col, op, rhs in extract_where_clauses(sql):
        if col not in col_to_enum:
            continue
        et = next(iter(col_to_enum[col]))
        if et in cast_enums:
            continue
        if has_text_to_enum(rhs, et):
            continue
        # rhs 是字面量或 $N
        if re.match(r"'[^']*'|\$\d+", rhs.strip()):
            findings.append(("MEDIUM", col, et,
                            f"WHERE {col} {op} {rhs[:30]} (类型 {et}) 比较无 ::{et} 强转",
                            sql_oneline[:200]))
    return findings


# ---------- 4. 主流程 ----------

def main():
    scan_dir = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_SCAN_DIR
    enum_types, enum_columns, col_to_enum = parse_v3_sql(V3_SQL)
    print(f"[scan] v3.sql 解析: {len(enum_types)} 个 enum, {sum(len(v) for v in enum_columns.values())} 处列定义")
    print(f"[scan] 扫描目录: {scan_dir.relative_to(ROOT)}")
    print()

    all_findings = []
    rs_files = [p for p in sorted(scan_dir.rglob("*.rs")) if p.is_file()]
    for rs_path in rs_files:
        for line_no, sql in extract_sql_strings(rs_path):
            for risk, col, et, hint, snippet in analyze_sql(sql, col_to_enum, enum_types):
                all_findings.append((risk, rs_path, line_no, col, et, hint, snippet))

    # 按风险+文件聚合展示
    by_risk = defaultdict(list)
    for f in all_findings:
        by_risk[f[0]].append(f[1:])

    print("=" * 80)
    print(f"扫描结果: 共 {len(all_findings)} 条")
    print(f"  HIGH   (INSERT/UPDATE 值位):  {len(by_risk['HIGH'])}")
    print(f"  MEDIUM (WHERE 比较):           {len(by_risk['MEDIUM'])}")
    print("=" * 80)

    for risk in ("HIGH", "MEDIUM"):
        items = by_risk[risk]
        if not items:
            continue
        print(f"\n--- [{risk}] 共 {len(items)} 条 ---\n")
        # 按文件聚合, 同一文件多条只展开一次
        by_file = defaultdict(list)
        for rs_path, line_no, col, et, hint, snippet in items:
            by_file[rs_path].append((line_no, col, et, hint, snippet))
        for rs_path, lst in sorted(by_file.items(), key=lambda x: str(x[0])):
            rel = str(rs_path.relative_to(ROOT))
            print(f"  📄 {rel}")
            for line_no, col, et, hint, snippet in sorted(lst, key=lambda x: x[0]):
                print(f"    L{line_no}  {col}/{et}  →  {hint}")
                print(f"           {snippet[:140]}{'...' if len(snippet) > 140 else ''}")
            print()


if __name__ == "__main__":
    main()
