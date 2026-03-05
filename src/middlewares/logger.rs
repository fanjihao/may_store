use std::env;

pub fn init_logger() {
    // 设置默认日志级别
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "ntex=info");
    }
    env_logger::init();
}
