use std::sync::Arc;

use ntex::web::{
    types::{Json, Query, State},
    Responder, HttpResponse
};

use crate::{
    errors::CustomError, models::{
        invitation::Invitation,
        users::{UserInfo, UserToken}, wx_official::TemplateMessage,
    }, wx_official::send_to_user::send_template, AppState
};

pub async fn get_invitation(
    _: UserToken,
    data: Query<UserInfo>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    let ship = sqlx::query_as!(
        Invitation,
        "SELECT
                s.*,
                wu.nick_name as send_name, wu.avatar as send_avatar, wu.role as send_role,
                rece.nick_name as bind_name, rece.avatar as bind_avatar, rece.role as bind_role
            FROM
                user_ships s
                LEFT JOIN users wu ON s.user_id = wu.user_id
                LEFT JOIN users rece ON s.bind_id = rece.user_id
            WHERE (s.ship_status = 0 OR s.ship_status = 1 ) AND (s.bind_id = $1 OR s.user_id = $1)",
        data.user_id
    )
    .fetch_all(db_pool)
    .await?;
    // let records = sqlx::query!("SELECT COUNT(*) FROM msgs m WHERE m.recv_user_id = $1 AND m.msg_type = 8 AND m.msg_status = 2 AND m.is_del = 0", data.user_id).fetch_one(db_pool).await?;

    Ok(Json(ship))
}

pub async fn new_invitation(
    _: UserToken,
    data: Json<Invitation>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    let date = chrono::Utc::now();
    println!("{:?}", data);
    sqlx::query!(
        "INSERT INTO user_ships (user_id, bind_id, bind_date) VALUES ($1, $2, $3)",
        data.user_id,
        data.bind_id,
        date.date_naive()
    )
    .execute(db_pool)
    .await?;

    let record = sqlx::query!(
        "SELECT * FROM users WHERE user_id = $1",
        data.bind_id
    ).fetch_one(db_pool).await?;

    let _ = send_template(Json(TemplateMessage {
        template_id: "q5FAhgoNR7Va3e_F8wq5IEKHaxz-ebnHUTpfaU0JepM".to_string(),
        push_id: record.push_id.expect("no push id"),
        date: Some("()".to_string()),
        city: Some("()".to_string()),
        weather: Some("()".to_string()),
        low: Some("()".to_string()),
        high: Some("()".to_string()),
        love_days: Some("()".to_string()),
        birthdays: Some("()".to_string()),
    })).await;
    Ok(HttpResponse::Created().body("发送成功"))
}
