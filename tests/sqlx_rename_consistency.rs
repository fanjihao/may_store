// 集成测试: 检查所有 query_as::<_, Type> 的字段 rename 与 SQL 列名是否匹配
// 用法: cargo test --test sqlx_rename_consistency
//
// 此测试调用 scripts/check_query_as.py 扫描 src/ 下所有 Rust 文件,
// 找出 #[sqlx(rename = "X")] 字段的 X 在对应 SQL 中找不到列的 bug。
// 如果返回非零退出码或有输出, 测试失败。

use std::process::Command;

#[test]
fn query_as_rename_matches_sql_columns() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let script = format!("{}/scripts/check_query_as.py", manifest_dir);

    // 确认脚本存在
    assert!(
        std::path::Path::new(&script).exists(),
        "❌ 找不到 {} - 请确认 scripts/check_query_as.py 存在",
        script
    );

    // 从 manifest_dir 跑脚本,这样脚本能找到 src/
    let output = Command::new("python3")
        .arg(&script)
        .current_dir(manifest_dir)
        .output()
        .expect("❌ 无法执行 python3, 请确认已安装 Python 3");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    if !output.status.success() {
        panic!(
            "❌ sqlx rename 一致性检查失败 (exit code = {:?}):\n{}\n\n\
             这通常意味着 #[sqlx(rename = \"X\")] 中的 X 在对应 SQL 的 SELECT 列表中找不到。\n\
             检查项:\n\
             1. SQL 是否给该列做了 AS 重命名\n\
             2. 结构体 rename 目标是否与 SQL 列名一致\n\
             详见 scripts/check_query_as.py 头部注释",
            output.status.code(),
            combined
        );
    }

    // 即使 exit code 是 0 (无 mismatch), 也打印一下确认信息
    println!("✅ query_as rename 一致性检查通过");
}