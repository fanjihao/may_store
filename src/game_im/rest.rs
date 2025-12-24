use rand::Rng;
use reqwest::Url;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::errors::CustomError;
use crate::game_im::sign::generate_user_sig;
use crate::models::game_im::ImConfig;

#[derive(Debug, Clone)]
pub struct ImRestClient {
    cfg: ImConfig,
    http: reqwest::Client,
}

impl ImRestClient {
    pub fn new(cfg: ImConfig) -> Self {
        Self {
            cfg,
            http: reqwest::Client::new(),
        }
    }

    fn admin_identifier(&self) -> String {
        self.cfg.admin_identifier()
    }

    fn admin_usersig(&self) -> Result<String, CustomError> {
        let now = chrono::Utc::now().timestamp();
        generate_user_sig(
            &self.admin_identifier(),
            self.cfg.sdk_app_id,
            &self.cfg.secret_key,
            self.cfg.expire_seconds,
            now,
        )
    }

    fn build_url(&self, path: &str) -> Result<Url, CustomError> {
        let mut url = Url::parse(&format!("https://console.tim.qq.com/v4/{path}"))
            .map_err(|e| CustomError::InternalServerError(e.to_string()))?;
        let random: u32 = rand::thread_rng().gen();
        url.query_pairs_mut()
            .append_pair("sdkappid", &self.cfg.sdk_app_id.to_string())
            .append_pair("identifier", &self.admin_identifier())
            .append_pair("usersig", &self.admin_usersig()?)
            .append_pair("random", &random.to_string())
            .append_pair("contenttype", "json");
        Ok(url)
    }

    async fn post_json<TReq: Serialize, TResp: DeserializeOwned>(
        &self,
        path: &str,
        body: &TReq,
    ) -> Result<TResp, CustomError> {
        let url = self.build_url(path)?;
        let resp = self.http.post(url).json(body).send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(CustomError::BadRequest(format!("IM http error: {status} {text}")));
        }
        let parsed: TResp = serde_json::from_str(&text)?;
        Ok(parsed)
    }

    pub async fn create_group(
        &self,
        owner_identifier: &str,
        name: &str,
    ) -> Result<String, CustomError> {
        #[derive(Debug, Serialize)]
        struct Req<'a> {
            #[serde(rename = "Type")]
            r#type: &'a str,
            #[serde(rename = "Name")]
            name: &'a str,
            #[serde(rename = "Owner_Account")]
            owner_account: &'a str,
            #[serde(rename = "MaxMemberCount")]
            max_member_count: u32,
        }
        #[derive(Debug, Deserialize)]
        struct Resp {
            #[serde(rename = "ActionStatus")]
            action_status: String,
            #[serde(rename = "ErrorCode")]
            error_code: i32,
            #[serde(rename = "ErrorInfo")]
            error_info: String,
            #[serde(rename = "GroupId")]
            group_id: Option<String>,
        }
        let out: Resp = self
            .post_json(
                "group_open_http_svc/create_group",
                &Req {
                    r#type: "Public",
                    name,
                    owner_account: owner_identifier,
                    max_member_count: 200,
                },
            )
            .await?;
        if out.action_status != "OK" || out.error_code != 0 {
            return Err(CustomError::BadRequest(format!("IM create_group failed: {}", out.error_info)));
        }
        out.group_id
            .ok_or_else(|| CustomError::InternalServerError("IM create_group missing GroupId".into()))
    }

    pub async fn account_import(&self, identifier: &str) -> Result<(), CustomError> {
        #[derive(Debug, Serialize)]
        struct Req<'a> {
            #[serde(rename = "UserID")]
            user_id: &'a str,
        }
        #[derive(Debug, Deserialize)]
        struct Resp {
            #[serde(rename = "ActionStatus")]
            action_status: String,
            #[serde(rename = "ErrorCode")]
            error_code: i32,
            #[serde(rename = "ErrorInfo")]
            error_info: String,
        }
        let out: Resp = self
            .post_json("im_open_login_svc/account_import", &Req { user_id: identifier })
            .await?;
        if out.action_status != "OK" || out.error_code != 0 {
            return Err(CustomError::BadRequest(format!("IM account_import failed: {}", out.error_info)));
        }
        Ok(())
    }

    pub async fn add_group_member(&self, group_id: &str, identifier: &str) -> Result<(), CustomError> {
        #[derive(Debug, Serialize)]
        struct Member<'a> {
            #[serde(rename = "Member_Account")]
            member_account: &'a str,
        }
        #[derive(Debug, Serialize)]
        struct Req<'a> {
            #[serde(rename = "GroupId")]
            group_id: &'a str,
            #[serde(rename = "MemberList")]
            member_list: Vec<Member<'a>>,
        }
        #[derive(Debug, Deserialize)]
        struct Resp {
            #[serde(rename = "ActionStatus")]
            action_status: String,
            #[serde(rename = "ErrorCode")]
            error_code: i32,
            #[serde(rename = "ErrorInfo")]
            error_info: String,
        }
        let out: Resp = self
            .post_json(
                "group_open_http_svc/add_group_member",
                &Req {
                    group_id,
                    member_list: vec![Member { member_account: identifier }],
                },
            )
            .await?;
        if out.action_status != "OK" || out.error_code != 0 {
            return Err(CustomError::BadRequest(format!("IM add_group_member failed: {}", out.error_info)));
        }
        Ok(())
    }

    pub async fn get_group_owner_account(&self, group_id: &str) -> Result<Option<String>, CustomError> {
        #[derive(Debug, Serialize)]
        struct InfoReq {
            #[serde(rename = "GroupIdList")]
            group_id_list: Vec<String>,
            #[serde(rename = "ResponseFilter")]
            response_filter: ResponseFilter,
        }
        #[derive(Debug, Serialize)]
        struct ResponseFilter {
            #[serde(rename = "GroupBaseInfoFilter")]
            group_base_info_filter: Vec<String>,
        }
        #[derive(Debug, Deserialize)]
        struct GroupBaseInfo {
            #[serde(rename = "GroupId")]
            group_id: String,
            #[serde(rename = "Owner_Account")]
            owner_account: Option<String>,
        }
        #[derive(Debug, Deserialize)]
        struct InfoResp {
            #[serde(rename = "ActionStatus")]
            action_status: String,
            #[serde(rename = "ErrorCode")]
            error_code: i32,
            #[serde(rename = "ErrorInfo")]
            error_info: String,
            #[serde(rename = "GroupInfo")]
            group_info: Option<Vec<GroupBaseInfo>>,
        }

        let out: InfoResp = self
            .post_json(
                "group_open_http_svc/get_group_info",
                &InfoReq {
                    group_id_list: vec![group_id.to_string()],
                    response_filter: ResponseFilter {
                        group_base_info_filter: vec!["Owner_Account".to_string()],
                    },
                },
            )
            .await?;
        if out.action_status != "OK" || out.error_code != 0 {
            return Err(CustomError::BadRequest(format!(
                "IM get_group_info failed: {}",
                out.error_info
            )));
        }
        let owner = out
            .group_info
            .unwrap_or_default()
            .into_iter()
            .find(|g| g.group_id == group_id)
            .and_then(|g| g.owner_account);
        Ok(owner)
    }

    pub async fn destroy_group(&self, group_id: &str) -> Result<(), CustomError> {
        #[derive(Debug, Serialize)]
        struct Req<'a> {
            #[serde(rename = "GroupId")]
            group_id: &'a str,
        }
        #[derive(Debug, Deserialize)]
        struct Resp {
            #[serde(rename = "ActionStatus")]
            action_status: String,
            #[serde(rename = "ErrorCode")]
            error_code: i32,
            #[serde(rename = "ErrorInfo")]
            error_info: String,
        }
        let out: Resp = self
            .post_json("group_open_http_svc/destroy_group", &Req { group_id })
            .await?;
        if out.action_status != "OK" || out.error_code != 0 {
            return Err(CustomError::BadRequest(format!(
                "IM destroy_group failed: {}",
                out.error_info
            )));
        }
        Ok(())
    }

    pub async fn get_group_member_accounts(&self, group_id: &str) -> Result<Vec<String>, CustomError> {
        #[derive(Debug, Serialize)]
        struct Req<'a> {
            #[serde(rename = "GroupId")]
            group_id: &'a str,
            #[serde(rename = "Limit")]
            limit: u32,
            #[serde(rename = "Offset")]
            offset: u32,
        }
        #[derive(Debug, Deserialize)]
        struct Member {
            #[serde(rename = "Member_Account")]
            member_account: String,
        }
        #[derive(Debug, Deserialize)]
        struct Resp {
            #[serde(rename = "ActionStatus")]
            action_status: String,
            #[serde(rename = "ErrorCode")]
            error_code: i32,
            #[serde(rename = "ErrorInfo")]
            error_info: String,
            #[serde(rename = "MemberList")]
            member_list: Option<Vec<Member>>,
        }
        let out: Resp = self
            .post_json(
                "group_open_http_svc/get_group_member_info",
                &Req {
                    group_id,
                    limit: 200,
                    offset: 0,
                },
            )
            .await?;
        if out.action_status != "OK" || out.error_code != 0 {
            return Err(CustomError::BadRequest(format!("IM get_group_member_info failed: {}", out.error_info)));
        }
        Ok(out
            .member_list
            .unwrap_or_default()
            .into_iter()
            .map(|m| m.member_account)
            .collect())
    }

    pub async fn send_group_custom(&self, group_id: &str, custom: serde_json::Value) -> Result<(), CustomError> {
        #[derive(Debug, Serialize)]
        struct Elem<'a> {
            #[serde(rename = "MsgType")]
            msg_type: &'a str,
            #[serde(rename = "MsgContent")]
            msg_content: serde_json::Value,
        }
        #[derive(Debug, Serialize)]
        struct Req<'a> {
            #[serde(rename = "GroupId")]
            group_id: &'a str,
            #[serde(rename = "From_Account")]
            from_account: &'a str,
            #[serde(rename = "MsgBody")]
            msg_body: Vec<Elem<'a>>,
        }
        #[derive(Debug, Deserialize)]
        struct Resp {
            #[serde(rename = "ActionStatus")]
            action_status: String,
            #[serde(rename = "ErrorCode")]
            error_code: i32,
            #[serde(rename = "ErrorInfo")]
            error_info: String,
        }
        let from = self.admin_identifier();
        let out: Resp = self
            .post_json(
                "group_open_http_svc/send_group_msg",
                &Req {
                    group_id,
                    from_account: &from,
                    msg_body: vec![Elem {
                        msg_type: "TIMCustomElem",
                        msg_content: json!({"Data": custom.to_string()}),
                    }],
                },
            )
            .await?;
        if out.action_status != "OK" || out.error_code != 0 {
            return Err(CustomError::BadRequest(format!("IM send_group_msg failed: {}", out.error_info)));
        }
        Ok(())
    }

    pub async fn send_c2c_custom(&self, to_identifier: &str, custom: serde_json::Value) -> Result<(), CustomError> {
        #[derive(Debug, Serialize)]
        struct Elem<'a> {
            #[serde(rename = "MsgType")]
            msg_type: &'a str,
            #[serde(rename = "MsgContent")]
            msg_content: serde_json::Value,
        }
        #[derive(Debug, Serialize)]
        struct Req<'a> {
            #[serde(rename = "SyncOtherMachine")]
            sync_other_machine: u8,
            #[serde(rename = "From_Account")]
            from_account: &'a str,
            #[serde(rename = "To_Account")]
            to_account: &'a str,
            #[serde(rename = "MsgBody")]
            msg_body: Vec<Elem<'a>>,
        }
        #[derive(Debug, Deserialize)]
        struct Resp {
            #[serde(rename = "ActionStatus")]
            action_status: String,
            #[serde(rename = "ErrorCode")]
            error_code: i32,
            #[serde(rename = "ErrorInfo")]
            error_info: String,
        }
        let from = self.admin_identifier();
        let out: Resp = self
            .post_json(
                "openim/sendmsg",
                &Req {
                    sync_other_machine: 1,
                    from_account: &from,
                    to_account: to_identifier,
                    msg_body: vec![Elem {
                        msg_type: "TIMCustomElem",
                        msg_content: json!({"Data": custom.to_string()}),
                    }],
                },
            )
            .await?;
        if out.action_status != "OK" || out.error_code != 0 {
            return Err(CustomError::BadRequest(format!("IM sendmsg failed: {}", out.error_info)));
        }
        Ok(())
    }

    pub async fn list_groups(&self) -> Result<Vec<(String, String, Option<u32>, Option<String>)>, CustomError> {
        // Step 1: get group ids
        #[derive(Debug, Serialize)]
        struct ListReq {
            #[serde(rename = "Limit")]
            limit: u32,
            #[serde(rename = "Offset")]
            offset: u32,
        }
        #[derive(Debug, Deserialize)]
        struct GroupIdItem {
            #[serde(rename = "GroupId")]
            group_id: String,
        }
        #[derive(Debug, Deserialize)]
        struct ListResp {
            #[serde(rename = "ActionStatus")]
            action_status: String,
            #[serde(rename = "ErrorCode")]
            error_code: i32,
            #[serde(rename = "ErrorInfo")]
            error_info: String,
            #[serde(rename = "GroupIdList")]
            group_id_list: Option<Vec<GroupIdItem>>,
        }

        let list_out: ListResp = self
            .post_json(
                "group_open_http_svc/get_appid_group_list",
                &ListReq { limit: 100, offset: 0 },
            )
            .await?;

        println!("IM list_groups response: {:?}", list_out);
        if list_out.action_status != "OK" || list_out.error_code != 0 {
            return Err(CustomError::BadRequest(format!(
                "IM get_appid_group_list failed: {}",
                list_out.error_info
            )));
        }
        let group_ids: Vec<String> = list_out
            .group_id_list
            .unwrap_or_default()
            .into_iter()
            .map(|x| x.group_id)
            .collect();

        if group_ids.is_empty() {
            return Ok(vec![]);
        }

        // Step 2: fetch group details in batch
        #[derive(Debug, Serialize)]
        struct InfoReq {
            #[serde(rename = "GroupIdList")]
            group_id_list: Vec<String>,
            #[serde(rename = "ResponseFilter")]
            response_filter: ResponseFilter,
        }
        #[derive(Debug, Serialize)]
        struct ResponseFilter {
            #[serde(rename = "GroupBaseInfoFilter")]
            group_base_info_filter: Vec<String>,
        }
        #[derive(Debug, Deserialize)]
        struct GroupBaseInfo {
            #[serde(rename = "GroupId")]
            group_id: String,
            #[serde(rename = "Name")]
            name: Option<String>,
            #[serde(rename = "MemberNum")]
            member_num: Option<u32>,
            #[serde(rename = "Owner_Account")]
            owner_account: Option<String>,
        }
        #[derive(Debug, Deserialize)]
        struct InfoResp {
            #[serde(rename = "ActionStatus")]
            action_status: String,
            #[serde(rename = "ErrorCode")]
            error_code: i32,
            #[serde(rename = "ErrorInfo")]
            error_info: String,
            #[serde(rename = "GroupInfo")]
            group_info: Option<Vec<GroupBaseInfo>>,
        }

        let info_out: InfoResp = self
            .post_json(
                "group_open_http_svc/get_group_info",
                &InfoReq {
                    group_id_list: group_ids,
                    response_filter: ResponseFilter {
                        group_base_info_filter: vec![
                            "Name".to_string(),
                            "MemberNum".to_string(),
                            "Owner_Account".to_string(),
                        ],
                    },
                },
            )
            .await?;
        if info_out.action_status != "OK" || info_out.error_code != 0 {
            return Err(CustomError::BadRequest(format!(
                "IM get_group_info failed: {}",
                info_out.error_info
            )));
        }

        Ok(info_out
            .group_info
            .unwrap_or_default()
            .into_iter()
            .map(|g| {
                let name = g.name.unwrap_or_else(|| g.group_id.clone());
                (g.group_id, name, g.member_num, g.owner_account)
            })
            .collect())
    }
}
