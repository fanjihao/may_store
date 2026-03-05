use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use ntex::web::types::{Json, Path, State};
use ntex::web::{HttpResponse, Responder};
use rand::seq::SliceRandom;

use crate::{
    config::AppState,
    errors::CustomError,
    game_im::models::{
        ImRoomListOut, ImRoomOut, ImStartGameOut, ImUserSigOut, ImVoteIn, ImVoteOut, Role,
        RoomPhase, RoomRuntime,
    },
    game_im::service::{generate_user_sig, runtime, ImRestClient},
    middlewares::auth::UserToken,
};

#[utoipa::path(
	get,
	path = "/im/usersig",
	tag = "游戏",
	summary = "获取当前登录用户的腾讯云 IM UserSig",
	responses(
		(status = 200, body = ImUserSigOut),
		(status = 400, body = CustomError),
		(status = 401, body = CustomError)
	),
	security(("cookie_auth" = []))
)]
pub async fn get_user_sig(
    token: UserToken,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let Some(cfg) = state.im_config.clone() else {
        return Err(CustomError::BadRequest(
            "IM 未配置：请设置 TENCENT_IM_SDK_APP_ID / TENCENT_IM_SECRET_KEY".into(),
        ));
    };

    let now = Utc::now().timestamp();
    let identifier = token.user_id.to_string();
    let user_sig = generate_user_sig(
        &identifier,
        cfg.sdk_app_id,
        &cfg.secret_key,
        cfg.expire_seconds,
        now,
    )?;
    let expire_at = now + cfg.expire_seconds as i64;

    Ok(HttpResponse::Ok().json(&ImUserSigOut {
        sdk_app_id: cfg.sdk_app_id,
        identifier,
        user_id: token.user_id,
        user_sig,
        expire_at,
    }))
}

#[utoipa::path(
    get,
    path = "/game/rooms",
    tag = "游戏",
    summary = "小游戏大厅：获取房间列表（IM 群组列表）",
    responses((status = 200, body = ImRoomListOut), (status = 400, body = CustomError), (status = 401, body = CustomError)),
    security(("cookie_auth" = []))
)]
pub async fn list_rooms(
    _token: UserToken,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let Some(cfg) = state.im_config.as_ref().map(|c| (**c).clone()) else {
        return Err(CustomError::BadRequest(
            "IM 未配置：请设置 TENCENT_IM_SDK_APP_ID / TENCENT_IM_SECRET_KEY".into(),
        ));
    };
    let client = ImRestClient::new(cfg);
    let groups = client.list_groups().await?;
    let rooms = groups
        .into_iter()
        .map(|(group_id, name, member_num, owner_identifier)| ImRoomOut {
            group_id,
            name,
            member_num,
            owner_identifier,
        })
        .collect();
    Ok(HttpResponse::Ok().json(&ImRoomListOut { rooms }))
}

#[utoipa::path(
    post,
    path = "/game/rooms/{group_id}/start",
    tag = "游戏",
    summary = "房主开始游戏：分配身份并通过 C2C 私聊下发",
    params(("group_id" = String, Path, description = "IM 群组 ID")),
    responses((status = 200, body = ImStartGameOut), (status = 400, body = CustomError), (status = 401, body = CustomError)),
    security(("cookie_auth" = []))
)]
pub async fn start_game(
    _token: UserToken,
    group_id: Path<String>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let Some(cfg) = state.im_config.as_ref().map(|c| (**c).clone()) else {
        return Err(CustomError::BadRequest(
            "IM 未配置：请设置 TENCENT_IM_SDK_APP_ID / TENCENT_IM_SECRET_KEY".into(),
        ));
    };
    let group_id = group_id.into_inner();
    let client = ImRestClient::new(cfg);

    // Fetch current member list from IM, authoritative.
    let members = client.get_group_member_accounts(&group_id).await?;
    if members.len() < 2 {
        return Err(CustomError::BadRequest("房间人数不足".into()));
    }

    // Ready check (backend authoritative)
    let rt = runtime(&state);
    {
        let mut rooms = rt.rooms.lock().await;
        let room = rooms
            .entry(group_id.clone())
            .or_insert_with(RoomRuntime::new);
        if room.phase != RoomPhase::Lobby {
            return Err(CustomError::BadRequest("游戏已开始".into()));
        }

        room.phase = RoomPhase::Started;
        room.started_at = Utc::now().timestamp();
        room.alive = members.iter().cloned().collect();
        room.votes.clear();
        room.roles.clear();

        // Role assignment: 2 wolves, rest villager (simple MVP)
        let mut shuffled = members.clone();
        shuffled.shuffle(&mut rand::thread_rng());
        let wolf_count = if shuffled.len() >= 6 { 2 } else { 1 };
        for (idx, id) in shuffled.iter().enumerate() {
            let role = if idx < wolf_count {
                Role::Wolf
            } else {
                Role::Villager
            };
            room.roles.insert(id.clone(), role);
        }
    }

    // Send each player their role via C2C
    let rt = runtime(&state);
    let (started_at, roles) = {
        let rooms = rt.rooms.lock().await;
        let room = rooms
            .get(&group_id)
            .ok_or_else(|| CustomError::InternalServerError("room missing".into()))?;
        (room.started_at, room.roles.clone())
    };

    let wolves: Vec<String> = roles
        .iter()
        .filter_map(|(id, r)| {
            if *r == Role::Wolf {
                Some(id.clone())
            } else {
                None
            }
        })
        .collect();

    for (to, role) in roles.iter() {
        let role_str = match role {
            Role::Wolf => "WOLF",
            Role::Villager => "VILLAGER",
        };
        let payload = serde_json::json!({
            "type":"ROLE",
            "payload":{
                "role": role_str,
                "wolves": wolves,
                "groupId": group_id,
                "startedAt": started_at
            }
        });
        let _ = client.send_c2c_custom(to, payload).await;
    }

    // Broadcast game started
    let _ = client
        .send_group_custom(
            &group_id,
            serde_json::json!({"type":"GAME_STARTED","payload":{"groupId":group_id,"startedAt":started_at}}),
        )
        .await;

    Ok(HttpResponse::Ok().json(&ImStartGameOut {
        group_id,
        started_at,
        total_players: members.len() as u32,
    }))
}

#[utoipa::path(
    post,
    path = "/game/rooms/{group_id}/vote",
    tag = "游戏",
    summary = "投票：HTTP 上报投票，后端记录并通过群消息广播进度/结果",
    params(("group_id" = String, Path, description = "IM 群组 ID")),
    request_body = ImVoteIn,
    responses((status = 200, body = ImVoteOut), (status = 400, body = CustomError), (status = 401, body = CustomError)),
    security(("cookie_auth" = []))
)]
pub async fn vote(
    token: UserToken,
    group_id: Path<String>,
    body: Json<ImVoteIn>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let Some(cfg) = state.im_config.as_ref().map(|c| (**c).clone()) else {
        return Err(CustomError::BadRequest(
            "IM 未配置：请设置 TENCENT_IM_SDK_APP_ID / TENCENT_IM_SECRET_KEY".into(),
        ));
    };
    let group_id = group_id.into_inner();
    let voter = token.user_id.to_string();
    let rt = runtime(&state);

    let (voted_count, total_alive, finished, eliminated) = {
        let mut rooms = rt.rooms.lock().await;
        let room = rooms
            .entry(group_id.clone())
            .or_insert_with(RoomRuntime::new);
        if room.phase == RoomPhase::Lobby {
            return Err(CustomError::BadRequest("游戏未开始".into()));
        }
        if !room.alive.contains(&voter) {
            return Err(CustomError::BadRequest("仅存活玩家可投票".into()));
        }
        if room.phase == RoomPhase::Started {
            room.phase = RoomPhase::Voting;
        }
        room.votes
            .insert(voter.clone(), body.target_identifier.clone());
        let total_alive = room.alive.len() as u32;
        let voted_count = room.votes.len() as u32;
        let finished = voted_count >= total_alive;

        let eliminated = if finished {
            // tally
            let mut tally: HashMap<String, u32> = HashMap::new();
            for (_from, to) in room.votes.iter() {
                if let Some(t) = to {
                    if room.alive.contains(t) {
                        *tally.entry(t.clone()).or_insert(0) += 1;
                    }
                }
            }
            let mut items: Vec<(String, u32)> = tally.into_iter().collect();
            items.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
            if items.is_empty() {
                None
            } else if items.len() >= 2 && items[0].1 == items[1].1 {
                None
            } else {
                let out = items[0].0.clone();
                room.alive.remove(&out);
                room.votes.clear();
                room.phase = RoomPhase::Started;
                Some(out)
            }
        } else {
            None
        };

        (voted_count, total_alive, finished, eliminated)
    };

    let client = ImRestClient::new(cfg);

    // Broadcast progress
    let _ = client
        .send_group_custom(
            &group_id,
            serde_json::json!({
                "type":"VOTE_PROGRESS",
                "payload":{
                    "votedCount": voted_count,
                    "totalAlive": total_alive,
                    "finished": finished
                }
            }),
        )
        .await;

    if finished {
        let _ = client
            .send_group_custom(
                &group_id,
                serde_json::json!({
                    "type":"VOTE_RESULT",
                    "payload":{
                        "eliminated": eliminated,
                        "groupId": group_id
                    }
                }),
            )
            .await;
    }

    Ok(HttpResponse::Ok().json(&ImVoteOut {
        group_id,
        voter_identifier: voter,
        voted_count,
        total_alive,
        finished,
    }))
}
