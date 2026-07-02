#!/usr/bin/env python3
"""
清理 v3.sql: 删 17 张死表 + 3 个只被死表用的 enum

策略:
- 对每个死表名, 匹配从 "CREATE TABLE x (" 开始的整段, 包括随后的 COMMENT ON TABLE x 和 CREATE INDEX idx_xxx ON x(...)
- 段结束标记: 下一个 CREATE TABLE / CREATE TYPE / 段头 (-- ================) / 文件末尾
- 同样处理 enum
"""
import re
from pathlib import Path

DEAD_TABLES = {
    "achievement_definitions", "association_group_requests", "cart_items",
    "carts", "diamond_flow", "group_achievements", "group_diamond_flow",
    "group_footprint_capacity", "group_invites", "lottery_draw_results",
    "lottery_draws", "message_categories", "sign_records", "sweet_talks",
    "user_diamond", "user_message_state", "user_record",
}
DEAD_ENUMS = {"cart_status_enum", "group_invite_status_enum", "lottery_success_enum"}

path = Path("src/v3.sql")
text = path.read_text(encoding="utf-8")

# 删除 DROP TABLE 行
for t in DEAD_TABLES:
    text = re.sub(rf"^DROP TABLE IF EXISTS {t} CASCADE;\n", "", text, flags=re.MULTILINE)

# 删除 DROP TYPE 行
for e in DEAD_ENUMS:
    text = re.sub(rf"^DROP TYPE IF EXISTS {e} CASCADE;\n", "", text, flags=re.MULTILINE)

# 删除 CREATE TABLE 块 (含后面的 COMMENT/INDEX, 直到下一个 CREATE TABLE/TYPE/段头)
# 段头模式: ^-- =+ 或 ^CREATE TABLE / ^CREATE TYPE
boundary = r"(?=^-- =+|^CREATE TABLE|^CREATE TYPE|\Z)"

for t in DEAD_TABLES:
    # 从 CREATE TABLE t ( 开始, 找 ) 闭合后所有后续行直到 boundary
    # 简化: 用多行 DOTALL 匹配
    pattern = (
        rf"^CREATE TABLE {t} \([\s\S]*?\);\n"        # CREATE TABLE + 内容 + );
        rf"(?:COMMENT ON TABLE {t}[\s\S]*?\n)*"       # 0+ 行 COMMENT ON TABLE t
        rf"(?:CREATE INDEX idx_\w+ ON {t}[\s\S]*?\n)*"  # 0+ 行 CREATE INDEX ON t
        rf"(?:\n)?"                                    # 可选空行
    )
    text = re.sub(pattern, "", text, flags=re.MULTILINE)

# 删除 CREATE TYPE 块
for e in DEAD_ENUMS:
    pattern = (
        rf"^CREATE TYPE {e} AS ENUM \([\s\S]*?\);\n"
    )
    text = re.sub(pattern, "", text, flags=re.MULTILINE)

# 清理可能留下的多余空行 (3+ 连续空行变 2)
text = re.sub(r"\n\n\n+", "\n\n", text)

path.write_text(text, encoding="utf-8")

# 验证
import subprocess
print("--- 清理结果 ---")
for t in DEAD_TABLES:
    out = subprocess.run(["grep", "-c", f"\\b{t}\\b", "src/v3.sql"], capture_output=True, text=True)
    cnt = out.stdout.strip()
    status = "✓" if cnt == "0" else f"✗ 剩 {cnt}"
    print(f"  {t}: {status}")
for e in DEAD_ENUMS:
    out = subprocess.run(["grep", "-c", f"\\b{e}\\b", "src/v3.sql"], capture_output=True, text=True)
    cnt = out.stdout.strip()
    status = "✓" if cnt == "0" else f"✗ 剩 {cnt}"
    print(f"  {e}: {status}")

print()
print(f"剩余 CREATE TABLE 数: {len(re.findall(r'^CREATE TABLE', text, re.MULTILINE))}")
print(f"剩余 CREATE TYPE 数:  {len(re.findall(r'^CREATE TYPE', text, re.MULTILINE))}")
print(f"文件总行数: {text.count(chr(10)) + 1}")
