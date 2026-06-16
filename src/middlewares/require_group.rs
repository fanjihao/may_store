//! "用户必须已加入组"提取器
//!
//! `RequireGroup` —— handler 在签名里加 `_: RequireGroup`(或带名字的形参)即可声明
//!   - "本接口要求调用者必须已经加入了某个组"
//!   - 用户未登录(扩展里没有 `UserPublic`)→ 返回 AUTH_INVALID_TOKEN
//!   - 用户已登录但没加入组 → 返回 USER_NOT_IN_GROUP(403)
//!   - 用户已在组 → handler 拿到 `RequireGroup { user_id, group_id }`
//!
//! 用途:覆盖所有"挂组"的接口,做统一的"未加组"拦截,由前端弹出"您还未加入组"。
//!
//! 例外:做客链路(`/api/kitchens/invitations/*`)不要使用本提取器 —— 客人本身就不是组成员。

use std::future::Future;

use ntex::{
    http::Payload,
    web::{ErrorRenderer, FromRequest, HttpRequest},
};

use crate::domain::user::UserPublic;
use crate::errors::CustomError;

/// 已加入组的用户上下文。`group_id` 一定不是 None。
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
            let group_id = user_public.group_id.ok_or_else(|| {
                CustomError::user_not_in_group("您还未加入组")
            })?;
            Ok(RequireGroup {
                user_id: user_public.user_id,
                group_id,
            })
        }
    }
}
