use std::sync::Arc;
use ntex::web::{
    types::{Json, Path, Query, State}, HttpResponse, Responder
};
use crate::{
    errors::CustomError, models::{
        invitation::{BindStruct, Invitation},
        users::{UserInfo, UserToken}
    }, AppState
};

#[utoipa::path(
    get,
    path = "/invitation",
    params(
        ("user_id" = Option<i32>, Query, description = "用户Id"),
    ),
    tag = "用户",
    responses(
        (status = 201, body = Vec<Invitation>),
        (status = 400, body = CustomError, example = json!(CustomError::BadRequest("参数错误".to_string())))
    ),
    security(
        ("cookie_auth" = [])
    )
)]
pub async fn get_invitation(
    user_token: UserToken,
    data: Query<UserInfo>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;
    println!("user_token: {:?}", user_token);


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
        WHERE s.bind_id = $1 OR s.user_id = $1
        ORDER BY bind_date DESC",
        data.user_id
    )
    .fetch_all(db_pool)
    .await?;

    Ok(Json(ship))
}


#[utoipa::path(
    post,
    path = "/invitation",
    request_body = Invitation,
    tag = "用户",
    responses(
        (status = 201, body = String),
        (status = 400, body = CustomError, example = json!(CustomError::BadRequest("参数错误".to_string())))
    ),
    security(
        ("cookie_auth" = [])
    )
)]
pub async fn new_invitation(
    _: UserToken,
    data: Json<Invitation>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    let date = chrono::Utc::now();

    let record = sqlx::query!(
        "SELECT COUNT(*) FROM user_ships WHERE ship_status = 1 AND (bind_id = $1 OR user_id = $1)",
        data.bind_id
    ).fetch_one(db_pool).await?;

    let count = match record.count {
        Some(count) => {
            count
        },
        None => 0
    };
    if count > 0_i64 {
        Err(CustomError::BadRequest("该用户已存在绑定关系".to_string()))
    } else {
        sqlx::query!(
            "INSERT INTO user_ships (user_id, bind_id, bind_date) VALUES ($1, $2, $3)",
            data.user_id,
            data.bind_id,
            date.date_naive()
        )
        .execute(db_pool)
        .await?;
    
        let _record = sqlx::query!(
            "SELECT * FROM users WHERE user_id = $1",
            data.bind_id
        ).fetch_one(db_pool).await?;
    
        // let _ = send_template(Json(TemplateMessage {
        //     template_id: "-rnlOjKqvvuhIjKysIrTlzW0x-M_iryCNjQvLT58VuQ".to_string(),
        //     push_id: record.push_id.expect("no push id"),
        //     date: Some("2024年8月16日".to_string()),
        //     city: Some("成都市".to_string()),
        //     weather: Some("多云".to_string()),
        //     low: Some("23°".to_string()),
        //     high: Some("33°".to_string()),
        //     love_days: Some("899天".to_string()),
        //     birthdays: Some("()".to_string()),
        // })).await;
        Ok(HttpResponse::Created().body("发送成功"))
    }
}

#[utoipa::path(
    put,
    path = "/invitation/{id}",
    params(
        ("id" = i32, Path, description = "邀请ID"),
    ),
    request_body = BindStruct,
    tag = "用户",
    responses(
        (status = 201, body = String),
        (status = 400, body = CustomError, example = json!(CustomError::BadRequest("参数错误".to_string())))
    ),
    security(
        ("cookie_auth" = [])
    )
)]
pub async fn confirm_invitation(
    _: UserToken,
    id: Path<(i32,)>,
    data: Json<BindStruct>,
    state: State<Arc<AppState>>
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;
    let mut transaction = db_pool.begin().await?;

    let date = chrono::Utc::now();
    sqlx::query!(
        "UPDATE user_ships SET ship_status = 1, update_date = $2 WHERE ship_id = $1", 
        id.0,
        date
    ).execute(&mut *transaction)
    .await?;

    sqlx::query!(
        "UPDATE users SET associate_id = $1 WHERE user_id = $2", 
        data.bind_id,
        data.user_id
    ).execute(&mut *transaction)
    .await?;

    sqlx::query!(
        "UPDATE users SET associate_id = $1 WHERE user_id = $2", 
        data.user_id,
        data.bind_id
    ).execute(&mut *transaction)
    .await?;

    transaction.commit().await?;
    Ok(HttpResponse::Created().body("绑定成功"))
}

#[utoipa::path(
    delete,
    path = "/invitation/{id}",
    params(
        ("id" = i32, Path, description = "邀请ID"),
    ),
    tag = "用户",
    responses(
        (status = 201, body = String),
        (status = 400, body = CustomError, example = json!(CustomError::BadRequest("参数错误".to_string())))
    ),
    security(
        ("cookie_auth" = [])
    )
)]
pub async fn cancel_invitation(
    _: UserToken,
    id: Path<(i32,)>,
    state: State<Arc<AppState>>
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    sqlx::query!(
        "DELETE FROM user_ships WHERE ship_id = $1", 
        id.0
    ).execute(db_pool)
    .await?;

    Ok(HttpResponse::Created().body("删除成功"))
}