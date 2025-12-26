use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use ntex::service::{fn_factory_with_config, fn_service};
use ntex::util::ByteString;
use ntex::web::{self, HttpRequest, HttpResponse};
use ntex::web::types::State;
use ntex::web::ws::{Frame, Message, WsSink};
use rand::seq::SliceRandom;
use rand::Rng;
use rand::thread_rng;
use serde_json::json;
use tokio::sync::Mutex;

use crate::{AppState, errors::CustomError};
use crate::models::game_ws::{
    BroadcastIn, GameOut, JoinRoomIn, LeaveRoomIn, Role, RoleRevealIn, RoleRevealOut, RoomMemberOut,
    RoomStateOut, RoomStatus, SetReadyIn, StartGameIn, WsInboundEnvelope, WsOutboundEnvelope,
};

#[derive(Debug, Clone)]
pub struct GameHub {
    rooms: Arc<Mutex<HashMap<String, Room>>>,
}

impl GameHub {
    pub fn new() -> Self {
        Self {
            rooms: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn generate_room_code(&self) -> Result<String, CustomError> {
        const MAX_TRIES: usize = 64;
        for _ in 0..MAX_TRIES {
            let code = random_room_code();
            let rooms = self.rooms.lock().await;
            if !rooms.contains_key(&code) {
                return Ok(code);
            }
        }
        Err(CustomError::InternalServerError(
            "无法生成唯一房间码，请重试".into(),
        ))
    }

    async fn join_room(&self, input: JoinRoomIn, sender: ClientSender) -> Result<RoomStateOut, CustomError> {
        validate_room_code(&input.room_code)?;
        let mut rooms = self.rooms.lock().await;

        if !input.create_if_not_exists && !rooms.contains_key(&input.room_code) {
            return Err(CustomError::BadRequest("房间不存在".into()));
        }

        let room = rooms.entry(input.room_code.clone()).or_insert_with(|| Room {
            room_code: input.room_code.clone(),
            host_user_id: input.user_id,
            status: RoomStatus::Lobby,
            members: HashMap::new(),
            senders: HashMap::new(),
            game: None,
            broadcast_rate_limit: HashMap::new(),
        });

        if room.room_code != input.room_code {
            // should never happen
            return Err(CustomError::InternalServerError("room code mismatch".into()));
        }

        if room.members.is_empty() {
            room.host_user_id = input.user_id;
        }

        if !room.members.contains_key(&input.user_id) {
            if room.game.is_some() && room.status == RoomStatus::Started {
                // currently no spectator UI; still allow join per spec; treat as member
            }
        }

        let now = Utc::now().timestamp_millis();
        let entry = room.members.entry(input.user_id).or_insert_with(|| RoomMember {
            user_id: input.user_id,
            nick_name: input.nick_name.clone(),
            avatar: input.avatar.clone(),
            ready: false,
            joined_at: now,
        });
        // idempotent update nick/avatar; keep ready
        entry.nick_name = input.nick_name;
        entry.avatar = input.avatar;

        room.senders.insert(input.user_id, sender);

        Ok(room.to_out())
    }

    async fn leave_room(&self, room_code: &str, user_id: i64) -> Option<RoomStateOut> {
        let mut rooms = self.rooms.lock().await;
        let room = rooms.get_mut(room_code)?;

        room.members.remove(&user_id);
        room.senders.remove(&user_id);
        room.broadcast_rate_limit.remove(&user_id);

        if room.members.is_empty() {
            rooms.remove(room_code);
            return None;
        }

        if room.host_user_id == user_id {
            if let Some((&new_host, _)) = room.members.iter().next() {
                room.host_user_id = new_host;
            }
        }

        Some(room.to_out())
    }

    async fn set_ready(&self, room_code: &str, user_id: i64, ready: bool) -> Result<RoomStateOut, CustomError> {
        let mut rooms = self.rooms.lock().await;
        let room = rooms.get_mut(room_code).ok_or_else(|| CustomError::BadRequest("房间不存在".into()))?;

        if room.status != RoomStatus::Lobby {
            return Err(CustomError::BadRequest("仅在未开局阶段可准备".into()));
        }

        let m = room.members.get_mut(&user_id).ok_or_else(|| CustomError::BadRequest("用户不在房间中".into()))?;
        m.ready = ready;

        Ok(room.to_out())
    }

    async fn start_game(&self, room_code: &str, user_id: i64, game_key: String) -> Result<(RoomStateOut, Vec<(i64, RoleRevealOut)>), CustomError> {
        let mut rooms = self.rooms.lock().await;
        let room = rooms.get_mut(room_code).ok_or_else(|| CustomError::BadRequest("房间不存在".into()))?;

        if room.host_user_id != user_id {
            return Err(CustomError::BadRequest("仅房主可开始游戏".into()));
        }
        if room.status != RoomStatus::Lobby {
            return Err(CustomError::BadRequest("当前不是未开局阶段".into()));
        }
        // if room.members.len() < 4 {
        //     return Err(CustomError::BadRequest("开局人数不足（至少 4 人）".into()));
        // }
        if room.members.values().any(|m| !m.ready) {
            return Err(CustomError::BadRequest("需要全员准备".into()));
        }

        let started_at = Utc::now().timestamp_millis();
        room.status = RoomStatus::Started;
        room.game = Some(GameState { key: game_key.clone(), started_at });

        let members: Vec<i64> = room.members.keys().copied().collect();
        let roles = deal_witness_werewolf_roles(&members);

        let total_players = members.len() as i64;
        let wolf_user_ids: Vec<i64> = roles.iter().filter_map(|(uid, role)| {
            if *role == Role::Wolf { Some(*uid) } else { None }
        }).collect();
        let wolf_count = wolf_user_ids.len() as i64;

        let mut reveals: Vec<(i64, RoleRevealOut)> = Vec::with_capacity(members.len());
        for (to_user_id, role) in roles {
            let wolf_ids_for_user = match role {
                Role::Wolf => wolf_user_ids.iter().copied().filter(|id| *id != to_user_id).collect(),
                Role::Witness => wolf_user_ids.clone(),
                Role::Villager => Vec::new(),
            };

            reveals.push((
                to_user_id,
                RoleRevealOut {
                    to_user_id,
                    role,
                    wolf_user_ids: wolf_ids_for_user,
                    wolf_count,
                    total_players,
                },
            ));
        }

        Ok((room.to_out(), reveals))
    }

    async fn broadcast(&self, room_code: &str, from_user_id: i64, text: String) -> Result<Vec<ClientSender>, CustomError> {
        let mut rooms = self.rooms.lock().await;
        let room = rooms.get_mut(room_code).ok_or_else(|| CustomError::BadRequest("房间不存在".into()))?;
        if !room.members.contains_key(&from_user_id) {
            return Err(CustomError::BadRequest("用户不在房间中".into()));
        }

        // Simple rate limit: 1 message per 300ms
        let now = Utc::now().timestamp_millis();
        let last = room.broadcast_rate_limit.get(&from_user_id).copied().unwrap_or(0);
        if now.saturating_sub(last) < 300 {
            return Err(CustomError::BadRequest("发送太频繁".into()));
        }
        room.broadcast_rate_limit.insert(from_user_id, now);

        let payload = json!({
            "text": text,
            "at": now,
            "fromUserId": from_user_id,
        });
        let msg = WsInboundEnvelope { r#type: "broadcast".into(), payload };
        let txt = serde_json::to_string(&msg).unwrap_or_else(|_| "{\"type\":\"error\",\"payload\":{\"message\":\"serialize error\"}}".into());

        let senders: Vec<ClientSender> = room.senders.values().cloned().collect();
        drop(rooms);

        for s in &senders {
            let _ = s.send(txt.clone());
        }

        Ok(senders)
    }

    async fn forward_to_user(&self, room_code: &str, to_user_id: i64, msg: &str) -> Result<(), CustomError> {
        let rooms = self.rooms.lock().await;
        let room = rooms.get(room_code).ok_or_else(|| CustomError::BadRequest("房间不存在".into()))?;
        let sender = room.senders.get(&to_user_id).ok_or_else(|| CustomError::BadRequest("目标用户不在房间".into()))?;
        let _ = sender.send(msg.to_string());
        Ok(())
    }

    async fn broadcast_room_state(&self, room_code: &str) -> Result<(), CustomError> {
        let (senders, state_txt) = {
            let rooms = self.rooms.lock().await;
            let room = rooms.get(room_code).ok_or_else(|| CustomError::BadRequest("房间不存在".into()))?;
            let out = room.to_out();
            let env = WsInboundEnvelope {
                r#type: "room_state".into(),
                payload: serde_json::to_value(out).unwrap_or(serde_json::Value::Null),
            };
            let txt = serde_json::to_string(&env).unwrap_or_else(|_| "{\"type\":\"error\",\"payload\":{\"message\":\"serialize error\"}}".into());
            (room.senders.values().cloned().collect::<Vec<_>>(), txt)
        };

        for s in &senders {
            let _ = s.send(state_txt.clone());
        }
        Ok(())
    }
}

#[derive(Debug)]
struct Room {
    room_code: String,
    host_user_id: i64,
    status: RoomStatus,
    members: HashMap<i64, RoomMember>,
    senders: HashMap<i64, ClientSender>,
    game: Option<GameState>,
    broadcast_rate_limit: HashMap<i64, i64>,
}

impl Room {
    fn to_out(&self) -> RoomStateOut {
        let mut members: Vec<RoomMemberOut> = self
            .members
            .values()
            .map(|m| RoomMemberOut {
                user_id: m.user_id,
                nick_name: m.nick_name.clone(),
                avatar: m.avatar.clone(),
                ready: m.ready,
                is_host: m.user_id == self.host_user_id,
                joined_at: m.joined_at,
            })
            .collect();
        members.sort_by_key(|m| m.joined_at);

        RoomStateOut {
            room_code: self.room_code.clone(),
            host_user_id: self.host_user_id,
            status: self.status,
            members,
            game: self
                .game
                .clone()
                .map(|g| GameOut { key: g.key, started_at: g.started_at }),
        }
    }
}

#[derive(Debug, Clone)]
struct RoomMember {
    user_id: i64,
    nick_name: String,
    avatar: Option<String>,
    ready: bool,
    joined_at: i64,
}

#[derive(Debug, Clone)]
struct GameState {
    key: String,
    started_at: i64,
}

type ClientSender = tokio::sync::mpsc::UnboundedSender<String>;

type ClientReceiver = tokio::sync::mpsc::UnboundedReceiver<String>;

#[derive(Clone)]
struct ConnContext {
    hub: Arc<GameHub>,
}

#[derive(Debug, Default)]
struct ConnState {
    room_code: Option<String>,
    user_id: Option<i64>,
}

pub async fn ws_game(req: HttpRequest, state: State<Arc<AppState>>) -> Result<HttpResponse, CustomError> {
    {
        let peer = req.peer_addr().map(|a| a.to_string()).unwrap_or_else(|| "<unknown>".into());
        let origin = req
            .headers()
            .get("origin")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("<none>");
        let user_agent = req
            .headers()
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("<none>");
        log::info!("[game-ws] upgrade request peer={} origin={} ua={}", peer, origin, user_agent);
    }

    let hub = state.game_hub.clone();

    let factory = fn_factory_with_config(move |sink: WsSink| {
        let hub = hub.clone();

        async move {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();

            // outbound forwarding task
            spawn_outbound_forwarder(sink.clone(), rx);

            let conn_state = std::rc::Rc::new(std::cell::RefCell::new(ConnState::default()));

            // cleanup on disconnect
            {
                let hub = hub.clone();
                let conn_state = conn_state.clone();
                let on_disc = sink.on_disconnect();
                ntex::rt::spawn(async move {
                    let _ = on_disc.await;
                    let (room, uid) = {
                        let st = conn_state.borrow();
                        (st.room_code.clone(), st.user_id)
                    };
                    log::info!("[game-ws] disconnected room={:?} user_id={:?}", room, uid);
                    if let (Some(room_code), Some(user_id)) = (room, uid) {
                        if let Some(_new_state) = hub.leave_room(&room_code, user_id).await {
                            let _ = hub.broadcast_room_state(&room_code).await;
                        }
                    }
                });
            }

            let context = ConnContext { hub: hub.clone() };

            Ok::<_, CustomError>(fn_service(move |frame: Frame| {
                let context = context.clone();
                let tx = tx.clone();
                let conn_state = conn_state.clone();
                async move {
                    match handle_frame(frame, context, tx, conn_state).await {
                        Ok(Some(msg)) => Result::<Option<Message>, CustomError>::Ok(Some(msg)),
                        Ok(None) => Result::<Option<Message>, CustomError>::Ok(None),
                        Err(e) => {
                            let env = WsInboundEnvelope {
                                r#type: "error".into(),
                                payload: json!({ "message": e.to_string() }),
                            };
                            let txt = serde_json::to_string(&env).unwrap_or_else(|_| "{\"type\":\"error\",\"payload\":{\"message\":\"error\"}}".into());
                            Result::<Option<Message>, CustomError>::Ok(Some(Message::Text(ByteString::from(txt))))
                        }
                    }
                }
            }))
        }
    });

    Ok(web::ws::start::<_, _, CustomError>(req, factory).await?)
}

pub async fn get_room_code(state: State<Arc<AppState>>) -> Result<HttpResponse, CustomError> {
    let code = state.game_hub.generate_room_code().await?;
    let payload = json!({ "roomCode": code });
    Ok(HttpResponse::Ok().json(&payload))
}

fn spawn_outbound_forwarder(sink: WsSink, mut rx: ClientReceiver) {
    ntex::rt::spawn(async move {
        while let Some(txt) = rx.recv().await {
            let _ = sink.send(Message::Text(ByteString::from(txt))).await;
        }
        let _ = sink.send(Message::Close(None)).await;
    });
}

async fn handle_frame(
    frame: Frame,
    context: ConnContext,
    sender: ClientSender,
    conn_state: std::rc::Rc<std::cell::RefCell<ConnState>>,
) -> Result<Option<Message>, CustomError> {
    let text = match frame {
        Frame::Text(bytes) => String::from_utf8(bytes.to_vec()).map_err(|_| CustomError::BadRequest("Invalid UTF-8".into()))?,
        Frame::Binary(bytes) => String::from_utf8(bytes.to_vec()).map_err(|_| CustomError::BadRequest("Invalid UTF-8".into()))?,
        Frame::Ping(bytes) => return Ok(Some(Message::Pong(bytes))),
        Frame::Pong(_) => return Ok(None),
        Frame::Close(_) => return Ok(Some(Message::Close(None))),
        Frame::Continuation(_) => return Ok(None),
    };

    let env: WsOutboundEnvelope = serde_json::from_str(&text)
        .map_err(|_| CustomError::BadRequest("Invalid message JSON".into()))?;

    match env.r#type.as_str() {
        "join_room" => {
            let input: JoinRoomIn = serde_json::from_value(env.payload)
                .map_err(|_| CustomError::BadRequest("join_room payload error".into()))?;

            // If already in a room and joining another: leave old room first
            let old = {
                let st = conn_state.borrow();
                (st.room_code.clone(), st.user_id)
            };
            if let (Some(old_room), Some(old_uid)) = old {
                if old_room != input.room_code {
                    let _ = context.hub.leave_room(&old_room, old_uid).await;
                    let _ = context.hub.broadcast_room_state(&old_room).await;
                }
            }

            let state = context.hub.join_room(input.clone(), sender.clone()).await?;

            {
                let mut st = conn_state.borrow_mut();
                st.room_code = Some(input.room_code.clone());
                st.user_id = Some(input.user_id);
            }

            // broadcast room state
            context.hub.broadcast_room_state(&input.room_code).await?;

            // also respond immediately with room_state to the joining client (idempotent)
            let txt = serde_json::to_string(&WsInboundEnvelope {
                r#type: "room_state".into(),
                payload: serde_json::to_value(state).unwrap_or(serde_json::Value::Null),
            }).unwrap_or_else(|_| "{\"type\":\"error\",\"payload\":{\"message\":\"serialize error\"}}".into());

            Ok(Some(Message::Text(ByteString::from(txt))))
        }
        "leave_room" => {
            let input: LeaveRoomIn = serde_json::from_value(env.payload)
                .map_err(|_| CustomError::BadRequest("leave_room payload error".into()))?;

            let _ = context.hub.leave_room(&input.room_code, input.user_id).await;
            context.hub.broadcast_room_state(&input.room_code).await.ok();

            {
                let mut st = conn_state.borrow_mut();
                if st.room_code.as_deref() == Some(&input.room_code) && st.user_id == Some(input.user_id) {
                    st.room_code = None;
                    st.user_id = None;
                }
            }

            Ok(None)
        }
        "set_ready" => {
            let input: SetReadyIn = serde_json::from_value(env.payload)
                .map_err(|_| CustomError::BadRequest("set_ready payload error".into()))?;

            let _state = context.hub.set_ready(&input.room_code, input.user_id, input.ready).await?;
            context.hub.broadcast_room_state(&input.room_code).await?;
            Ok(None)
        }
        "start_game" => {
            let input: StartGameIn = serde_json::from_value(env.payload)
                .map_err(|_| CustomError::BadRequest("start_game payload error".into()))?;

            let (state, reveals) = context
                .hub
                .start_game(&input.room_code, input.user_id, input.game_key)
                .await?;

            // broadcast room_state
            let txt = serde_json::to_string(&WsInboundEnvelope {
                r#type: "room_state".into(),
                payload: serde_json::to_value(state).unwrap_or(serde_json::Value::Null),
            }).unwrap_or_else(|_| "{\"type\":\"error\",\"payload\":{\"message\":\"serialize error\"}}".into());

            // push to all via hub
            {
                let rooms = context.hub.rooms.lock().await;
                if let Some(room) = rooms.get(&input.room_code) {
                    for s in room.senders.values() {
                        let _ = s.send(txt.clone());
                    }
                }
            }

            // send role_reveal to each user
            for (to_user_id, reveal) in reveals {
                let env = WsInboundEnvelope {
                    r#type: "role_reveal".into(),
                    payload: serde_json::to_value(reveal).unwrap_or(serde_json::Value::Null),
                };
                let msg = serde_json::to_string(&env).unwrap_or_else(|_| "{\"type\":\"error\",\"payload\":{\"message\":\"serialize error\"}}".into());
                let _ = context.hub.forward_to_user(&input.room_code, to_user_id, &msg).await;
            }

            Ok(None)
        }
        "broadcast" => {
            let input: BroadcastIn = serde_json::from_value(env.payload)
                .map_err(|_| CustomError::BadRequest("broadcast payload error".into()))?;

            let _ = context.hub.broadcast(&input.room_code, input.user_id, input.text).await?;
            Ok(None)
        }
        "role_reveal" => {
            let input: RoleRevealIn = serde_json::from_value(env.payload)
                .map_err(|_| CustomError::BadRequest("role_reveal payload error".into()))?;

            // validate host and started state
            {
                let rooms = context.hub.rooms.lock().await;
                let room = rooms
                    .get(&input.room_code)
                    .ok_or_else(|| CustomError::BadRequest("房间不存在".into()))?;
                if room.host_user_id != input.user_id {
                    return Err(CustomError::BadRequest("仅房主可发牌".into()));
                }
                if room.status != RoomStatus::Started {
                    return Err(CustomError::BadRequest("仅开局后可发牌".into()));
                }
            }

            let env = WsInboundEnvelope {
                r#type: "role_reveal".into(),
                payload: serde_json::to_value(RoleRevealOut {
                    to_user_id: input.to_user_id,
                    role: input.role,
                    wolf_user_ids: input.wolf_user_ids,
                    wolf_count: input.wolf_count,
                    total_players: input.total_players,
                }).unwrap_or(serde_json::Value::Null),
            };
            let msg = serde_json::to_string(&env).unwrap_or_else(|_| "{\"type\":\"error\",\"payload\":{\"message\":\"serialize error\"}}".into());
            context
                .hub
                .forward_to_user(&input.room_code, input.to_user_id, &msg)
                .await?;
            Ok(None)
        }
        other => {
            let env = WsInboundEnvelope { r#type: "error".into(), payload: json!({"message": format!("Unknown type: {other}")}) };
            let txt = serde_json::to_string(&env).unwrap_or_else(|_| "{\"type\":\"error\",\"payload\":{\"message\":\"unknown\"}}".into());
            Ok(Some(Message::Text(ByteString::from(txt))))
        }
    }
}

fn validate_room_code(code: &str) -> Result<(), CustomError> {
    if code.len() != 6 {
        return Err(CustomError::BadRequest("roomCode 格式错误".into()));
    }
    if !code.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
        return Err(CustomError::BadRequest("roomCode 格式错误".into()));
    }
    Ok(())
}

fn random_room_code() -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = thread_rng();
    (0..6)
        .map(|_| {
            let idx = rng.gen_range(0..CHARS.len());
            CHARS[idx] as char
        })
        .collect()
}

fn wolf_count(total: usize) -> usize {
    match total {
        0 => 0,
        1..=5 => 1,
        6..=8 => 2,
        9..=11 => 3,
        12..=15 => 4,
        n => std::cmp::max(1, n / 3),
    }
}

fn deal_witness_werewolf_roles(user_ids: &[i64]) -> Vec<(i64, Role)> {
    let total = user_ids.len();
    let wolves = wolf_count(total);
    let has_witness = total >= 5;

    let mut ids = user_ids.to_vec();
    ids.shuffle(&mut thread_rng());

    let mut out: Vec<(i64, Role)> = Vec::with_capacity(total);

    for (idx, uid) in ids.iter().copied().enumerate() {
        let role = if idx < wolves {
            Role::Wolf
        } else if has_witness && idx == wolves {
            Role::Witness
        } else {
            Role::Villager
        };
        out.push((uid, role));
    }

    out
}
