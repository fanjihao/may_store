use std::env;

pub fn init_logger() {
    // 设置默认日志级别
    // DEBUG 期间: 默认开启应用层 info, 方便看 [join_group] 等诊断日志
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "ntex=info,may_store=info");
    }
    env_logger::init();
}
