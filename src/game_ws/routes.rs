use std::sync::Arc;

use ntex::service::{fn_factory_with_config, fn_service};
use ntex::util::ByteString;
use ntex::web::types::State;
use ntex::web::ws::{Frame, Message, WsSink};
use ntex::web::{self, HttpRequest, HttpResponse};
use serde_json::json;

use crate::{config::AppState, errors::CustomError};
use crate::game_ws::models::WsInboundEnvelope;
use crate::game_ws::service::{handle_frame, spawn_outbound_forwarder, ConnContext, ConnState};

pub async fn ws_game(
    req: HttpRequest,
    state: State<Arc<AppState>>,
) -> Result<HttpResponse, CustomError> {
    {
        let peer = req
            .peer_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|| "<unknown>".into());
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
        log::info!(
            "[game-ws] upgrade request peer={} origin={} ua={}",
            peer,
            origin,
            user_agent
        );
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
                            let txt = serde_json::to_string(&env).unwrap_or_else(|_| {
                                "{\"type\":\"error\",\"payload\":{\"message\":\"error\"}}".into()
                            });
                            Result::<Option<Message>, CustomError>::Ok(Some(Message::Text(
                                ByteString::from(txt),
                            )))
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
