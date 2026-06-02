use std::collections::HashMap;
use std::sync::Arc;

use reqwest::cookie::CookieStore;
use reqwest::header::{HeaderMap, HeaderValue, COOKIE, REFERER, USER_AGENT};
use reqwest::Client;
use serde_json::Value;

use crate::api::jar::DomainAwareJar;
use crate::error::AppError;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36 Edg/125.0.0.0";

const BASE_URL: &str = "https://www.yuketang.cn";
const EXAM_BASE_URL: &str = "https://examination.xuetangx.com";
const MOOC_BASE_URL: &str = "https://wyuyjs.yuketang.cn";

#[derive(Clone)]
pub struct RainClient {
    pub client: Client,
    pub jar: Arc<DomainAwareJar>,
}

impl RainClient {
    pub fn new() -> Self {
        let jar = DomainAwareJar::new();
        let client = Client::builder()
            .cookie_provider(jar.clone())
            .user_agent(UA)
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .expect("无法创建 HTTP 客户端");

        Self { client, jar }
    }

    /// 初始化访问，获取初始 cookie
    pub async fn init(&self) -> Result<(), AppError> {
        let url = format!("{}/web", BASE_URL);
        self.client.get(&url).send().await?.error_for_status()?;
        log::info!("网站初始化访问成功");
        Ok(())
    }

    /// 获取 csrftoken
    pub fn get_csrftoken(&self) -> Option<String> {
        let url = BASE_URL.parse::<url::Url>().ok()?;
        let cookies = self.jar.cookies(&url)?;
        let cookie_str: &str = cookies.to_str().ok()?;
        for part in cookie_str.split(';') {
            let trimmed: &str = part.trim();
            if let Some(val) = trimmed.strip_prefix("csrftoken=") {
                return Some(val.to_string());
            }
        }
        None
    }

    /// 获取指定 cookie 值
    pub fn get_cookie_value(&self, name: &str) -> Option<String> {
        let url = BASE_URL.parse::<url::Url>().ok()?;
        let cookies = self.jar.cookies(&url)?;
        let cookie_str: &str = cookies.to_str().ok()?;
        let prefix = format!("{}=", name);
        for part in cookie_str.split(';') {
            let trimmed: &str = part.trim();
            if let Some(val) = trimmed.strip_prefix(&prefix) {
                return Some(val.to_string());
            }
        }
        None
    }

    /// 获取全部 cookie 字符串
    pub fn get_all_cookies(&self) -> String {
        let url = match BASE_URL.parse::<url::Url>() {
            Ok(u) => u,
            Err(_) => return String::new(),
        };
        self.jar
            .cookies(&url)
            .and_then(|c| c.to_str().ok().map(|s| s.to_string()))
            .unwrap_or_default()
    }

    /// 按域分组导出 jar 中**当前有效**的 cookie。
    ///
    /// 直接遍历 `DomainAwareJar` 的内部 store，确保拿到**所有**域、所有 cookie，
    /// 不再依赖预设的域名列表 → 即使雨课堂在 `changjie.yuketang.cn` 等
    /// 我们没列出的子域设了 cookie，也能完整持久化。
    pub fn dump_cookies_by_domain(&self) -> HashMap<String, HashMap<String, String>> {
        self.jar.dump_all()
    }

    /// 把按域分组的 cookies 加载回 jar。
    ///
    /// 与 `dump_cookies_by_domain` 互为反操作；每个 cookie 按其原域名挂载，
    /// 不会出现"把 yuketang.cn 的 sessionid 同时塞到 xuetangx.com"那种污染。
    pub fn load_cookies_by_domain(
        &self,
        cookies_by_domain: &HashMap<String, HashMap<String, String>>,
    ) {
        self.jar.load_all(cookies_by_domain);
    }

    /// 构建带 CSRF 的通用请求头
    pub fn common_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(UA));
        headers.insert(REFERER, HeaderValue::from_static("https://www.yuketang.cn/"));
        if let Some(csrf) = self.get_csrftoken() {
            if let Ok(v) = HeaderValue::from_str(&csrf) {
                headers.insert("X-Csrftoken", v);
            } else {
                log::warn!("csrftoken 含非法字符，已跳过 X-Csrftoken header");
            }
        }
        headers
    }

    /// 构建课堂级请求头
    pub fn classroom_headers(&self, class_id: &str) -> HeaderMap {
        let mut headers = self.common_headers();
        headers.insert("Xtbz", HeaderValue::from_static("ykt"));
        if let Ok(v) = HeaderValue::from_str(class_id) {
            headers.insert("Classroom-Id", v);
        } else {
            log::warn!("class_id 含非法字符: {:?}", class_id);
        }
        headers.insert("X-Client", HeaderValue::from_static("web"));
        headers.insert("xt-agent", HeaderValue::from_static("web"));

        // 构建完整 cookie，包含 csrftoken + sessionid + classroomId
        let mut cookie_parts: Vec<String> = Vec::new();
        if let Some(csrf) = self.get_cookie_value("csrftoken") {
            cookie_parts.push(format!("csrftoken={}", csrf));
        }
        if let Some(sid) = self.get_cookie_value("sessionid") {
            cookie_parts.push(format!("sessionid={}", sid));
        }
        cookie_parts.push(format!("classroom_id={}", class_id));
        cookie_parts.push(format!("classroomId={}", class_id));

        if !cookie_parts.is_empty() {
            let cookie_val = cookie_parts.join(";");
            if let Ok(v) = HeaderValue::from_str(&cookie_val) {
                headers.insert(COOKIE, v);
            } else {
                log::warn!("cookie 含非法字符，已跳过 Cookie header");
            }
        }
        headers
    }

    /// 构建考试平台请求头
    pub fn exam_headers(&self, exam_id: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(UA));
        headers.insert("Accept", HeaderValue::from_static("application/json, text/plain, */*"));
        headers.insert("Content-Type", HeaderValue::from_static("application/json"));
        headers.insert("X-Client", HeaderValue::from_static("web"));
        headers.insert("Xtbz", HeaderValue::from_static("cloud"));
        headers.insert("Origin", HeaderValue::from_static(EXAM_BASE_URL));
        let referer = format!("{}/exam/{}?isFrom=2", EXAM_BASE_URL, exam_id);
        if let Ok(v) = HeaderValue::from_str(&referer) {
            headers.insert(REFERER, v);
        } else {
            log::warn!("referer 含非法字符: {:?}", referer);
        }
        headers
    }

    // ========== 认证 API ==========

    pub async fn post_web_login(&self, user_id: i64, auth: &str) -> Result<(), AppError> {
        let url = format!("{}/pc/web_login", BASE_URL);
        let body = serde_json::json!({"UserID": user_id, "Auth": auth});
        self.client
            .post(&url)
            .headers(self.common_headers())
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn get_user_info(&self) -> Result<Value, AppError> {
        let url = format!("{}/v2/api/web/userinfo", BASE_URL);
        let resp = self
            .client
            .get(&url)
            .headers(self.common_headers())
            .send()
            .await?
            .json::<Value>()
            .await?;
        Ok(resp)
    }

    // ========== 课程 API ==========

    pub async fn get_course_list(&self) -> Result<Value, AppError> {
        let url = format!("{}/v2/api/web/courses/list?identity=2", BASE_URL);
        let resp = self
            .client
            .get(&url)
            .headers(self.common_headers())
            .send()
            .await?
            .json::<Value>()
            .await?;
        Ok(resp)
    }

    pub async fn get_works(&self, course_id: &str) -> Result<Value, AppError> {
        let url = format!(
            "{}/v2/api/web/logs/learn/{}?actype=5&page=0&offset=20&sort=-1",
            BASE_URL, course_id
        );
        let resp = self
            .client
            .get(&url)
            .headers(self.common_headers())
            .send()
            .await?
            .json::<Value>()
            .await?;
        Ok(resp)
    }

    pub async fn get_ppts(&self, class_id: &str) -> Result<Value, AppError> {
        let url = format!(
            "{}/v2/api/web/logs/learn/{}?actype=15&page=0&offset=200&sort=-1",
            BASE_URL, class_id
        );
        let resp = self
            .client
            .get(&url)
            .headers(self.common_headers())
            .send()
            .await?
            .json::<Value>()
            .await?;
        Ok(resp)
    }

    // ========== 考试/答案 API ==========

    /// 初始化考试页面（获取重定向后的 cookie）
    pub async fn init_exam(&self, course_id: &str, work_id: &str) -> Result<(), AppError> {
        let url = format!(
            "{}/v2/web/trans/{}/{}?status=1",
            BASE_URL, course_id, work_id
        );
        // 雨课堂会做 302 重定向（携带 cookie），4xx/5xx 视为失败
        self.client
            .get(&url)
            .headers(self.common_headers())
            .send()
            .await?
            .error_for_status()?;
        log::info!("init_exam 完成: course={}, work={}", course_id, work_id);
        Ok(())
    }

    /// 初始化考试页面（type=20 的作业用 status=4）
    pub async fn init_exam_2(&self, course_id: &str, work_id: &str) -> Result<(), AppError> {
        let url = format!(
            "{}/v2/web/trans/{}/{}?status=4",
            BASE_URL, course_id, work_id
        );
        self.client
            .get(&url)
            .headers(self.common_headers())
            .send()
            .await?
            .error_for_status()?;
        log::info!("init_exam_2 完成: course={}, work={}", course_id, work_id);
        Ok(())
    }

    pub async fn get_token_work(
        &self,
        course_id: &str,
        work_id: &str,
    ) -> Result<Value, AppError> {
        let url = format!("{}/v/exam/gen_token", BASE_URL);
        let mut headers = self.common_headers();
        let referer = format!(
            "{}/v2/web/trans/{}/{}?status=1",
            BASE_URL, course_id, work_id
        );
        headers.insert(REFERER, HeaderValue::from_str(&referer).unwrap());
        headers.insert("Content-Type", HeaderValue::from_static("application/json"));

        let body_str = serde_json::json!({
            "exam_id": work_id,
            "classroom_id": course_id
        })
        .to_string();

        let resp = self
            .client
            .post(&url)
            .headers(headers)
            .body(body_str)
            .send()
            .await?
            .json::<Value>()
            .await?;
        Ok(resp)
    }

    pub async fn get_token_work_2(
        &self,
        course_id: &str,
        work_id: &str,
    ) -> Result<Value, AppError> {
        let url = format!("{}/v/exam/gen_token", BASE_URL);
        let mut headers = self.classroom_headers(course_id);
        let referer = format!(
            "{}/v2/web/trans/{}/{}?status=4",
            BASE_URL, course_id, work_id
        );
        headers.insert(REFERER, HeaderValue::from_str(&referer).unwrap());
        headers.insert("Content-Type", HeaderValue::from_static("application/json"));

        // 确保 csrftoken 也在 cookie 中
        if let Some(csrf) = self.get_csrftoken() {
            if let Some(existing_cookie) = headers.get(COOKIE).cloned() {
                let cookie_str = existing_cookie.to_str().unwrap_or("");
                if !cookie_str.contains("csrftoken=") {
                    let new_cookie = format!("{};csrftoken={}", cookie_str, csrf);
                    headers.insert(COOKIE, HeaderValue::from_str(&new_cookie).unwrap());
                }
            }
        }

        let body_str = serde_json::json!({
            "exam_id": work_id,
            "classroom_id": course_id
        })
        .to_string();

        let resp = self
            .client
            .post(&url)
            .headers(headers)
            .body(body_str)
            .send()
            .await?
            .json::<Value>()
            .await?;
        Ok(resp)
    }

    pub async fn get_exam_login(
        &self,
        work_id: &str,
        user_id: &str,
        token: &str,
    ) -> Result<(), AppError> {
        let url = format!("{}/login", EXAM_BASE_URL);
        let next_url = format!("{}/exam/{}?isFrom=2", EXAM_BASE_URL, work_id);
        self.client
            .get(&url)
            .query(&[
                ("exam_id", work_id),
                ("user_id", user_id),
                ("crypt", token),
                ("next", &next_url),
                ("language", "zh"),
            ])
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn get_all_answer(&self, exam_id: &str) -> Result<Option<Value>, AppError> {
        let url = format!(
            "{}/exam_room/problem_results?exam_id={}",
            EXAM_BASE_URL, exam_id
        );
        let resp = self
            .client
            .get(&url)
            .headers(self.exam_headers(exam_id))
            .send()
            .await?;
        match resp.json::<Value>().await {
            Ok(val) => Ok(Some(val)),
            Err(_) => Ok(None),
        }
    }

    pub async fn get_all_question(&self, exam_id: &str) -> Result<Value, AppError> {
        let url = format!(
            "{}/exam_room/show_paper?exam_id={}",
            EXAM_BASE_URL, exam_id
        );
        let resp = self
            .client
            .get(&url)
            .headers(self.exam_headers(exam_id))
            .send()
            .await?
            .json::<Value>()
            .await?;
        Ok(resp)
    }

    pub async fn get_exam_cover(
        &self,
        classroom_id: &str,
        exam_id: &str,
    ) -> Result<Value, AppError> {
        let url = format!(
            "{}/v/exam/cover?exam_id={}&classroom_id={}",
            BASE_URL, exam_id, classroom_id
        );
        let resp = self
            .client
            .get(&url)
            .headers(self.common_headers())
            .send()
            .await?
            .json::<Value>()
            .await?;
        Ok(resp)
    }

    pub async fn get_pub_new_prob(
        &self,
        classroom_id: &str,
        work_id: &str,
    ) -> Result<Value, AppError> {
        let url = format!("{}/mooc-api/v1/lms/learn/course/pub_new_pro", BASE_URL);
        let mut headers = self.classroom_headers(classroom_id);
        headers.insert("Origin", HeaderValue::from_static("https://examination.xuetangx.com"));
        let body = serde_json::json!({"cid": classroom_id, "new_id": [work_id]});
        let resp = self
            .client
            .post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await?
            .json::<Value>()
            .await?;
        Ok(resp)
    }

    // ========== 章节/刷课 API ==========

    pub async fn get_course_sign(&self, class_id: &str) -> Result<Value, AppError> {
        let url = format!(
            "{}/v2/api/web/classrooms/{}?role=5",
            BASE_URL, class_id
        );
        let resp = self
            .client
            .get(&url)
            .headers(self.classroom_headers(class_id))
            .send()
            .await?
            .json::<Value>()
            .await?;
        Ok(resp)
    }

    pub async fn get_all_chapter(
        &self,
        class_id: &str,
        course_sign: &str,
    ) -> Result<Value, AppError> {
        let uv_id = self.get_cookie_value("uv_id").unwrap_or_default();
        let url = format!(
            "{}/mooc-api/v1/lms/learn/course/chapter?cid={}&sign={}&term=latest&uv_id={}",
            BASE_URL, class_id, course_sign, uv_id
        );
        let resp = self
            .client
            .get(&url)
            .headers(self.classroom_headers(class_id))
            .send()
            .await?
            .json::<Value>()
            .await?;
        Ok(resp)
    }

    /// 获取课程学习进度。
    ///
    /// 与 `get_all_chapter` 同参数（cid + sign + term + uv_id + classroom_headers），
    /// 仅接口 path 为 `course/schedule`。返回 `data.leaf_schedules{leaf_id: 完成度}`
    /// （1=完成、0=未完成、测验为浮点完成度）与 `data.total_schedule`（整体完成度 0~1）。
    pub async fn get_course_schedule(
        &self,
        class_id: &str,
        course_sign: &str,
    ) -> Result<Value, AppError> {
        let uv_id = self.get_cookie_value("uv_id").unwrap_or_default();
        let url = format!(
            "{}/mooc-api/v1/lms/learn/course/schedule?cid={}&sign={}&term=latest&uv_id={}",
            BASE_URL, class_id, course_sign, uv_id
        );
        let resp = self
            .client
            .get(&url)
            .headers(self.classroom_headers(class_id))
            .send()
            .await?
            .json::<Value>()
            .await?;
        Ok(resp)
    }

    pub async fn get_leaf_info(
        &self,
        leaf_id: &str,
        class_id: &str,
        course_sign: &str,
    ) -> Result<Value, AppError> {
        let uv_id = self.get_cookie_value("uv_id").unwrap_or_default();
        let url = format!(
            "{}/mooc-api/v1/lms/learn/leaf_info/{}/{}/?sign={}&term=latest&uv_id={}",
            MOOC_BASE_URL, class_id, leaf_id, course_sign, uv_id
        );
        let resp = self
            .client
            .get(&url)
            .headers(self.classroom_headers(class_id))
            .send()
            .await?
            .json::<Value>()
            .await?;
        Ok(resp)
    }

    pub async fn get_status(&self, leaf_id: &str, class_id: &str) -> Result<Value, AppError> {
        let uv_id = self.get_cookie_value("uv_id").unwrap_or_default();
        let url = format!(
            "{}/v/discussion/v2/student/comment/status/?leaf_id={}&classroom_id={}&term=latest&uv_id={}",
            MOOC_BASE_URL, leaf_id, class_id, uv_id
        );
        let resp = self
            .client
            .get(&url)
            .headers(self.classroom_headers(class_id))
            .send()
            .await?
            .json::<Value>()
            .await?;
        Ok(resp)
    }

    pub async fn read_announcement(
        &self,
        leaf_id: &str,
        class_id: &str,
        sku_id: &str,
    ) -> Result<Value, AppError> {
        let uv_id = self.get_cookie_value("uv_id").unwrap_or_default();
        let url = format!(
            "{}/mooc-api/v1/lms/learn/user_article_finish/{}/?cid={}&sid={}&term=latest&uv_id={}",
            MOOC_BASE_URL, leaf_id, class_id, sku_id, uv_id
        );
        let resp = self
            .client
            .get(&url)
            .headers(self.classroom_headers(class_id))
            .send()
            .await?
            .json::<Value>()
            .await?;
        Ok(resp)
    }

    /// 获取章节测验/练习（leaf_type=6）的题目与答案列表。
    ///
    /// 与 examination 平台的作业/考试不同，章节测验走 MOOC 平台：
    /// 先由 `get_leaf_info` 拿到 `leaf_type_id` 与 `sku_id`，再请求本接口。
    pub async fn get_exercise_list(
        &self,
        class_id: &str,
        leaf_type_id: &str,
        sku_id: &str,
    ) -> Result<Value, AppError> {
        let uv_id = self.get_cookie_value("uv_id").unwrap_or_default();
        let url = format!(
            "{}/mooc-api/v1/lms/exercise/get_exercise_list/{}/{}/?term=latest&uv_id={}",
            MOOC_BASE_URL, leaf_type_id, sku_id, uv_id
        );
        let resp = self
            .client
            .get(&url)
            .headers(self.classroom_headers(class_id))
            .send()
            .await?
            .json::<Value>()
            .await?;
        Ok(resp)
    }

    /// 提交章节测验/练习（leaf_type=6）单题答案。
    ///
    /// URL / body / headers 已按真实抓包（HAR）校准：
    /// `POST {BASE_URL}/mooc-api/v1/lms/exercise/problem_apply/`（host=www，无 query），
    /// body `{classroom_id, problem_id, answer}`（前两者为数字，answer 为字符串数组
    /// 如 `["true"]` / `["A","B"]`），复用 `classroom_headers`（含 X-Csrftoken +
    /// Classroom-Id + Xtbz），补 Content-Type。
    /// `class_id` 实为 classroom_id（前端路由 id 即 classroom_id，见 Dashboard 跳转）。
    pub async fn post_exercise_answer(
        &self,
        class_id: &str,
        problem_id: &str,
        answer: &Value,
    ) -> Result<Value, AppError> {
        let url = format!("{}/mooc-api/v1/lms/exercise/problem_apply/", BASE_URL);
        let mut headers = self.classroom_headers(class_id);
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json;charset=UTF-8"),
        );
        // id 类字段优先以数字提交（真实 problem_id 在响应中为数字），纯数字串转 Number，
        // 否则退回字符串。待联调：若服务端要求字符串可在此调整。
        let id_value = |s: &str| -> Value {
            s.parse::<i64>().map(Value::from).unwrap_or_else(|_| Value::String(s.to_string()))
        };
        let body = serde_json::json!({
            "classroom_id": id_value(class_id),
            "problem_id": id_value(problem_id),
            "answer": answer,
        });
        let resp = self
            .client
            .post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await?
            .json::<Value>()
            .await?;
        Ok(resp)
    }

    // ========== 视频 API ==========

    pub async fn get_video_progress(
        &self,
        video_id: &str,
        cid: &str,
        class_id: &str,
        user_id: &str,
    ) -> Result<Option<Value>, AppError> {
        let uni_id = self.get_cookie_value("university_id").unwrap_or_default();
        let url = format!(
            "{}/video-log/get_video_watch_progress/?cid={}&user_id={}&classroom_id={}&video_type=video&vtype=rate&video_id={}&snapshot=1&term=latest&uv_id={}",
            MOOC_BASE_URL, cid, user_id, class_id, video_id, uni_id
        );
        // 网络错误向上抛，让调用方决定重试还是放弃；
        // 仅响应体不是合法 JSON（业务暂时无进度）时返回 None。
        let resp = self
            .client
            .get(&url)
            .headers(self.classroom_headers(class_id))
            .send()
            .await?;
        match resp.json::<Value>().await {
            Ok(val) => Ok(Some(val)),
            Err(e) => {
                log::debug!("get_video_progress JSON 解析失败（可能视频暂无进度记录）: {}", e);
                Ok(None)
            }
        }
    }

    pub async fn send_heartbeat(&self, heart_data: Vec<Value>) -> Result<Option<String>, AppError> {
        let url = format!("{}/video-log/heartbeat/", BASE_URL);
        let body = serde_json::json!({"heart_data": heart_data});
        // 网络错误向上抛，调用方可据此判定网络异常并暂停心跳。
        let resp = self
            .client
            .post(&url)
            .headers(self.common_headers())
            .json(&body)
            .send()
            .await?;
        let text = resp.text().await.unwrap_or_default();
        Ok(Some(text))
    }

    // ========== PPT API ==========

    pub async fn get_ppt_questions_answer(
        &self,
        class_id: &str,
        ppt_id: &str,
    ) -> Result<Value, AppError> {
        let url = format!(
            "{}/v2/api/web/cards/detlist/{}?classroom_id={}",
            BASE_URL, ppt_id, class_id
        );
        let resp = self
            .client
            .get(&url)
            .headers(self.common_headers())
            .send()
            .await?
            .json::<Value>()
            .await?;
        Ok(resp)
    }

    /// 获取 WebSocket 登录用的 header 列表
    pub fn get_ws_login_headers(&self) -> Vec<String> {
        let mut result = vec![format!("User-Agent: {}", UA)];
        let cookies = self.get_all_cookies();
        if !cookies.is_empty() {
            result.push(format!("Cookie: {}", cookies));
        }
        result
    }

    /// 获取 PPT WebSocket 用的 header 列表
    pub fn get_ws_ppt_headers(&self, class_id: &str) -> Vec<String> {
        let mut result = vec![format!("User-Agent: {}", UA)];
        if let Some(csrf) = self.get_csrftoken() {
            result.push(format!("X-Csrftoken: {}", csrf));
        }
        let cookies = self.get_all_cookies();
        let extra = format!("; classroomId=;{}", class_id);
        result.push(format!("Cookie: {}{}", cookies, extra));
        result
    }
}
