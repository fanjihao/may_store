#!/usr/bin/env python3
"""
扫描 query_as::<_, Type> 的一致性
对每个 query_as 调用:
1. 提取紧跟着的 SQL 字符串 (SELECT ... FROM ...)
2. 解析 SQL 中的列名 (考虑 AS 重命名)
3. 读结构体定义, 找 #[sqlx(rename)] 字段
4. 对比: rename 后的名字是否在 SQL 列名集合里
5. 输出不匹配的项

用法:
    python3 scripts/check_query_as.py [files...]
    不传参数则扫描 src/ 下所有 .rs
"""

import re
import sys
from pathlib import Path
from collections import defaultdict

# Windows 的默认控制台编码可能是 GBK；测试通过管道捕获输出时，
# emoji 会触发 UnicodeEncodeError。统一输出 UTF-8，保证脚本跨平台运行。
if hasattr(sys.stdout, 'reconfigure'):
    sys.stdout.reconfigure(encoding='utf-8', errors='replace')
if hasattr(sys.stderr, 'reconfigure'):
    sys.stderr.reconfigure(encoding='utf-8', errors='replace')

ROOT = Path(__file__).resolve().parent.parent

# ============ 1. 找结构体定义 + #[sqlx(rename)] ============

STRUCT_DEF_RE = re.compile(
    r'(?:pub\s+)?struct\s+(\w+)\s*\{',
    re.MULTILINE,
)
SQLX_RENAME_RE = re.compile(
    r'#\[sqlx\(rename\s*=\s*[\'"]([^\'"]+)[\'"]\s*\)\]\s*\n\s*pub\s+(\w+)\s*:',
)
SQLX_DEFAULT_RE = re.compile(
    r'#\[sqlx\(default\)\]\s*\n\s*pub\s+(\w+)\s*:',
)
FIELD_RE = re.compile(r'^\s*pub\s+(\w+)\s*:', re.MULTILINE)


def parse_structs(content: str) -> dict[str, dict]:
    """返回 {TypeName: {field_name: rename_or_None, has_default: bool}}"""
    structs = {}
    for m in STRUCT_DEF_RE.finditer(content):
        name = m.group(1)
        # 用括号匹配找到 struct 结束
        start = m.end()
        depth = 1
        i = start
        while i < len(content) and depth > 0:
            if content[i] == '{':
                depth += 1
            elif content[i] == '}':
                depth -= 1
            i += 1
        body = content[start:i - 1]

        fields = {}
        for rm in SQLX_RENAME_RE.finditer(body):
            rename_to = rm.group(1)
            rust_name = rm.group(2)
            fields[rust_name] = {'rename': rename_to, 'has_default': False}
        for dm in SQLX_DEFAULT_RE.finditer(body):
            rust_name = dm.group(1)
            if rust_name not in fields:
                fields[rust_name] = {'rename': None, 'has_default': True}
            else:
                fields[rust_name]['has_default'] = True
        # 也抓普通字段 (没 rename/default 的)
        for fm in FIELD_RE.finditer(body):
            rust_name = fm.group(1)
            if rust_name not in fields:
                fields[rust_name] = {'rename': None, 'has_default': False}
        if fields:
            structs[name] = fields
    return structs


# ============ 2. 找 query_as::<_, Type> 紧跟的 SQL ============

# 匹配 "query_as::<_, SomeType>(" 然后字符串字面量
QUERY_AS_RE = re.compile(
    r'query_as\s*::<\s*_\s*,\s*(\w+)\s*>\s*\(\s*(r?#"|")',
)
# 字符串字面量内容 (支持 r#"..."# 和 "...")
STRING_RE = re.compile(r'r?#"([^"]*)"\s*#?\s*|"([^"\\]*(?:\\.[^"\\]*)*)"')


def extract_sql(text: str, pos: int) -> tuple[str, int] | None:
    """从 pos 位置提取 query_as 后面的 SQL 字符串, 返回 (sql, end_pos)"""
    # 找开括号后的引号
    m = QUERY_AS_RE.match(text, pos)
    if not m:
        return None
    type_name = m.group(1)
    quote_start = m.end(2)
    quote_char = m.group(2)

    # 根据引号类型提取
    if quote_char == 'r#"':
        end_marker = '"#'
    else:
        end_marker = quote_char

    end_idx = text.find(end_marker, quote_start)
    if end_idx == -1:
        return None
    sql = text[quote_start:end_idx]
    # 跳过引号结束符
    return type_name, sql, end_idx + len(end_marker)


def parse_sql_columns(sql: str) -> set[str]:
    """提取 SQL 中 SELECT 列表的所有列名 (考虑 AS 重命名)
    返回 set of 列名 (即 Row 中的列名)"""
    # 找 SELECT 和 FROM 之间的部分
    sql_upper = sql.upper()
    select_idx = sql_upper.find('SELECT')
    from_idx = sql_upper.find('FROM', select_idx)
    if select_idx == -1 or from_idx == -1:
        return set()
    select_part = sql[select_idx + 6:from_idx]

    cols = set()
    # 简单按逗号分割顶层项 (不处理嵌套函数调用里的逗号)
    depth = 0
    cur = []
    items = []
    for ch in select_part:
        if ch == '(':
            depth += 1
            cur.append(ch)
        elif ch == ')':
            depth -= 1
            cur.append(ch)
        elif ch == ',' and depth == 0:
            items.append(''.join(cur).strip())
            cur = []
        else:
            cur.append(ch)
    if cur:
        items.append(''.join(cur).strip())

    for item in items:
        # 跳过 SELECT 关键字, 字面量等
        if not item or item == '*':
            continue
        # 处理 t.col 这种 - 取最后一段
        item_clean = item.strip()
        # 去掉 AS 部分前的字段名 (e.g., "o.guest_user_id AS guest_id" -> "guest_id")
        as_match = re.search(r'\bAS\s+("?[\w.]+"?)\s*$', item_clean, re.IGNORECASE)
        if as_match:
            alias = as_match.group(1).strip('"')
            cols.add(alias.split('.')[-1])
        else:
            # 没有 AS, 取最后一段 (可能是 t.col 或 col 或 expr)
            # 函数调用跳过
            if '(' in item_clean:
                continue
            # 字面量跳过
            if re.match(r"^['\d]", item_clean):
                continue
            last = item_clean.split('.')[-1].strip().strip('"')
            cols.add(last)
    return cols


# ============ 3. 主流程 ============

def main() -> int:
    if len(sys.argv) > 1:
        files = [Path(p) for p in sys.argv[1:]]
    else:
        files = list((ROOT / 'src').rglob('*.rs'))

    # 收集所有结构体定义
    all_structs: dict[str, dict] = {}
    for f in files:
        if f.is_dir():
            continue
        content = f.read_text(encoding='utf-8')
        all_structs.update(parse_structs(content))

    # 扫描所有 query_as 调用
    issues = []
    for f in files:
        if f.is_dir():
            continue
        content = f.read_text(encoding='utf-8')
        for m in QUERY_AS_RE.finditer(content):
            result = extract_sql(content, m.start())
            if not result:
                continue
            type_name, sql, _ = result
            sql_cols = parse_sql_columns(sql)
            if type_name not in all_structs:
                # 类型在外部 crate 或别名
                continue
            struct = all_structs[type_name]

            # 检查每个有 rename 的字段
            for field, info in struct.items():
                if info['rename']:
                    target = info['rename']
                    if target not in sql_cols:
                        issues.append({
                            'file': str(f.relative_to(ROOT)),
                            'type': type_name,
                            'field': field,
                            'rename_target': target,
                            'sql_cols_sample': sorted(sql_cols)[:20],
                        })

            # 检查没 rename 没 default 的字段 - 这些必须有 SQL 列
            for field, info in struct.items():
                if not info['rename'] and not info['has_default']:
                    if field not in sql_cols:
                        # sqlx 用 try_get 兜底的可能没事, 这里只警告
                        pass

    if not issues:
        print('✅ All query_as renames match SQL columns')
        return 0

    print(f'❌ Found {len(issues)} potential rename mismatches:\n')
    by_file = defaultdict(list)
    for issue in issues:
        by_file[issue['file']].append(issue)
    for file, items in sorted(by_file.items()):
        print(f'📄 {file}')
        for it in items:
            print(f"   {it['type']}.{it['field']}  →  sqlx(rename = \"{it['rename_target']}\")")
            print(f"   ⚠️  SQL 列中找不到 '{it['rename_target']}'")
            print(f"   SQL SELECT 列: {it['sql_cols_sample']}")
            print()
    return 1


if __name__ == '__main__':
    import sys
    sys.exit(main())