// OpenAPI 文档生成
// 从外部文件加载 OpenAPI 3.0 JSON 文档

/// 生成 OpenAPI 3.0 JSON 文档
pub fn openapi_json() -> String {
    // 从外部 JSON 文件加载 OpenAPI 文档
    // 避免在 Rust 代码中处理 $ref 等特殊字符
    include_str!("openapi.json").to_string()
}
