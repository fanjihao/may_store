// 领域层 - 签到值对象
// 包含签到规则和奖励计算逻辑

/// 计算签到奖励钻石
/// 根据配置的奖励数组循环
/// 如果没有提供配置，则使用默认的 [5, 6, 7, 8, 9, 10, 20]
#[allow(dead_code)]
pub fn calculate_sign_diamonds(consecutive_days: i32, rewards: &[i32]) -> i32 {
    if rewards.is_empty() {
        return 0;
    }
    let len = rewards.len() as i32;
    let index = ((consecutive_days - 1) % len) as usize;
    rewards[index]
}
