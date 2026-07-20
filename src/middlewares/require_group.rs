//! “用户已加入任意组”提取器
//!
//! `RequireGroup` —— handler 在签名里加 `_: RequireGroup`(或带名字的形参)即可声明
//!   - “本接口要求调用者必须已经加入了某个组”
//!   - 用户未登录(扩展里没有 `UserPublic`)→ 返回 AUTH_INVALID_TOKEN
//!   - 用户已登录但没加入组 → 返回 USER_NOT_IN_GROUP(403)
//!   - 用户已在组 → handler 拿到 `RequireGroup { user_id, group_id }`
//!
//! 注意：`group_id` 来自登录态缓存，只代表缓存选中的某个组。`RequireGroup`
//! **不能**证明调用者属于 URL、请求体或业务对象指定的目标组，也不能证明缓存命中的
//! 成员关系和组当前仍为 ACTIVE。目标组读写必须额外调用
//! `target_group::require_active_target_group_member`，以数据库状态为准。
//!
//! 用途：只做统一的“尚未加入任何组”前置拦截，由前端弹出“您还未加入组”。
//!
//! 例外:做客链路(`/api/kitchens/invitations/*`)不要使用本提取器 —— 客人本身就不是组成员。

use std::future::Future;

use ntex::{
    http::Payload,
    web::{ErrorRenderer, FromRequest, HttpRequest},
};

use crate::domain::user::UserPublic;
use crate::errors::CustomError;

/// 已加入任意组的缓存上下文。`group_id` 一定不是 None，但不可用于目标组授权。
#[derive(Debug, Clone)]
pub struct RequireGroup {
    pub user_id: i64,
    pub group_id: i64,
}

impl<E: ErrorRenderer> FromRequest<E> for RequireGroup {
    type Error = CustomError;

    fn from_request(
        req: &HttpRequest,
        _: &mut Payload,
    ) -> impl Future<Output = Result<Self, Self::Error>> {
        let user_public = req
            .extensions()
            .get::<UserPublic>()
            .cloned()
            .ok_or_else(|| CustomError::auth_invalid_token("缺少用户身份信息"));

        async move {
            let user_public = user_public?;
            let group_id = user_public
                .group_id
                .ok_or_else(|| CustomError::user_not_in_group("您还未加入组"))?;
            Ok(RequireGroup {
                user_id: user_public.user_id,
                group_id,
            })
        }
    }
}
