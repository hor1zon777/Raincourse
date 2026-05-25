//! 自定义 cookie jar：基于 `cookie_store::CookieStore`，实现 `reqwest::cookie::CookieStore`
//! 接口，**额外暴露遍历能力**用于完整 dump。
//!
//! ## 为什么不用 `reqwest::cookie::Jar`？
//!
//! `reqwest::cookie::Jar` 只能通过 `cookies(url)` 按 url 查询，要求调用方提前
//! 知道所有相关域名。雨课堂涉及多域（`yuketang.cn`、`xuetangx.com`、`changjie.*`、
//! `mooc.*` 等），无法穷举。漏掉的域 cookie 会在持久化时丢失，导致"切换账户后
//! 再次登录提示会话已过期"——磁盘上的 session 缺关键 cookie。
//!
//! 本 jar 用 `iter_unexpired()` 遍历**全部** cookie，按 host 分组导出。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use cookie_store::{CookieStore as RawStore, RawCookie};
use parking_lot::RwLock;
use reqwest::cookie::CookieStore;
use reqwest::header::HeaderValue;

/// 对 reqwest 暴露的 cookie store；对外提供完整 dump/load 能力。
#[derive(Default)]
pub struct DomainAwareJar {
    inner: RwLock<RawStore>,
    /// 记录所有曾经被 `set_cookies` 的 request URL host。
    ///
    /// **存在意义**：cookie_store 内部用 `CookieDomain::HostOnly(host)` 表示
    /// host-only cookie（Set-Cookie 不含 `Domain` 属性），但这个枚举字段对外
    /// 是 `pub(crate)`，外部拿不到。`cookie.domain()` 仅返回原始 Domain 属性
    /// 值，对 host-only cookie 返回 `None`，无法回答"这个 cookie 属于哪个 host"。
    ///
    /// 这里维护一个影子集合：每次 set_cookies 时记录 url.host，dump 时
    /// 用 `cookie.matches(url)` 反查 host-only cookie 的归属。
    hosts_seen: RwLock<HashSet<String>>,
}

impl DomainAwareJar {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// 把 jar 中**所有未过期** cookie 按 host 分组导出。
    ///
    /// - 显式 `Domain=...` 的 cookie：key = `Domain` 属性去掉前置点
    /// - host-only cookie（无 Domain 属性）：key = 它在 set_cookies 时所属的 url.host
    ///
    /// 同 host 下多 cookie 合并到同一个 inner map。
    pub fn dump_all(&self) -> HashMap<String, HashMap<String, String>> {
        let store = self.inner.read();
        let hosts: Vec<String> = self.hosts_seen.read().iter().cloned().collect();
        let mut result: HashMap<String, HashMap<String, String>> = HashMap::new();

        for cookie in store.iter_unexpired() {
            let name = cookie.name().to_string();
            let value = cookie.value().to_string();

            // 1. 优先用显式 Domain 属性
            if let Some(d) = cookie.domain() {
                let domain = d.trim_start_matches('.').to_string();
                if !domain.is_empty() {
                    result.entry(domain).or_default().insert(name, value);
                    continue;
                }
            }

            // 2. host-only cookie：用 cookie.matches() 反查 hosts_seen
            //    cookie_store 的 matches 综合 domain + path + secure 判断；
            //    host-only cookie 只会 match 它当初被 set 的那个 host。
            let mut matched = false;
            for host in &hosts {
                let url_str = format!("https://{}/", host);
                let url = match url::Url::parse(&url_str) {
                    Ok(u) => u,
                    Err(_) => continue,
                };
                if cookie.matches(&url) {
                    result
                        .entry(host.clone())
                        .or_default()
                        .insert(name.clone(), value.clone());
                    matched = true;
                    break; // host-only 唯一归属
                }
            }
            if !matched {
                log::debug!(
                    "dump_all 跳过无法归属的 host-only cookie: {}={}",
                    name,
                    value
                );
            }
        }
        result
    }

    /// 把按 host 分组的 cookies 加载到 jar 中。
    ///
    /// **关键策略**（防止 H3 风险——子域 cookie 被错误升级为根域）：
    /// - **Apex 域**（如 `yuketang.cn`，段数 ≤ 2）：加 `Domain=.yuketang.cn`，
    ///   让所有 `*.yuketang.cn` 子域共享（保持原 domain cookie 语义）
    /// - **子域**（如 `www.yuketang.cn`，段数 ≥ 3）：**不加 Domain 属性**，
    ///   加载为 host-only —— 只在该精确 host 有效，不会污染同根域其它子域
    /// - **IP/localhost** 等不含点的 host：host-only
    pub fn load_all(&self, cookies_by_domain: &HashMap<String, HashMap<String, String>>) {
        let mut store = self.inner.write();
        let mut hosts_seen = self.hosts_seen.write();

        for (host, cookies) in cookies_by_domain {
            let host_norm = host.trim_start_matches('.');
            if host_norm.is_empty() {
                continue;
            }
            let url_str = format!("https://{}/", host_norm);
            let url: url::Url = match url_str.parse() {
                Ok(u) => u,
                Err(e) => {
                    log::warn!("跳过非法 host '{}': {}", host, e);
                    continue;
                }
            };

            // 记录到 hosts_seen，未来 dump_all 才能反查 host-only cookie
            hosts_seen.insert(host_norm.to_string());

            // 决定加载形态
            let domain_attr = derive_domain_attr_for_load(host_norm);

            let raws: Vec<RawCookie<'static>> = cookies
                .iter()
                .filter_map(|(name, value)| {
                    let cookie_str = match &domain_attr {
                        Some(d) => format!("{}={}; Domain={}; Path=/", name, value, d),
                        None => format!("{}={}; Path=/", name, value), // host-only
                    };
                    match RawCookie::parse(cookie_str) {
                        Ok(c) => Some(c.into_owned()),
                        Err(e) => {
                            log::warn!("解析 cookie '{}' 失败: {}", name, e);
                            None
                        }
                    }
                })
                .collect();

            store.store_response_cookies(raws.into_iter(), &url);
        }
    }

    /// 当前 jar 中未过期 cookie 数量（用于日志/调试）。
    pub fn len(&self) -> usize {
        self.inner.read().iter_unexpired().count()
    }
}

impl CookieStore for DomainAwareJar {
    fn set_cookies(
        &self,
        cookie_headers: &mut dyn Iterator<Item = &HeaderValue>,
        url: &url::Url,
    ) {
        // 维护影子集合用于 dump_all 的 host-only 反查
        if let Some(host) = url.host_str() {
            self.hosts_seen.write().insert(host.to_string());
        }

        let cookies: Vec<RawCookie<'static>> = cookie_headers
            .filter_map(|val| std::str::from_utf8(val.as_bytes()).ok())
            .filter_map(|s| RawCookie::parse(s.to_string()).ok())
            .collect();
        if cookies.is_empty() {
            return;
        }
        self.inner
            .write()
            .store_response_cookies(cookies.into_iter(), url);
    }

    fn cookies(&self, url: &url::Url) -> Option<HeaderValue> {
        let store = self.inner.read();
        let s = store
            .get_request_values(url)
            .map(|(name, value)| format!("{}={}", name, value))
            .collect::<Vec<_>>()
            .join("; ");
        if s.is_empty() {
            return None;
        }
        HeaderValue::from_str(&s).ok()
    }
}

/// 决定 load_all 时为每个 host 构造的 cookie 形态。
///
/// - 返回 `Some(".yuketang.cn")` → 加载为 domain cookie（跨子域共享）
/// - 返回 `None` → 加载为 host-only（仅该精确 host 有效）
///
/// **规则**：仅当 host 是 apex（点数 ≤ 1）且非 IP/localhost 时升级为 domain cookie；
/// 否则保持 host-only 不污染同根域其它子域。
fn derive_domain_attr_for_load(host: &str) -> Option<String> {
    // IP 地址 / localhost / 单段名：host-only
    if host.parse::<std::net::IpAddr>().is_ok() || !host.contains('.') {
        return None;
    }
    let dot_count = host.matches('.').count();
    if dot_count <= 1 {
        // apex（foo.cn / foo.com）→ 用 Domain=.foo.cn 让子域共享
        Some(format!(".{}", host))
    } else {
        // 子域（www.foo.cn / changjie.foo.cn）→ host-only，不加 Domain 属性
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_jar_dump_is_empty() {
        let jar = DomainAwareJar::default();
        assert!(jar.dump_all().is_empty());
    }

    #[test]
    fn load_then_dump_apex_keeps_domain() {
        let jar = DomainAwareJar::default();
        let mut input = HashMap::new();
        let mut yk = HashMap::new();
        yk.insert("sessionid".to_string(), "abc".to_string());
        yk.insert("csrftoken".to_string(), "xyz".to_string());
        input.insert("yuketang.cn".to_string(), yk);

        jar.load_all(&input);
        let dumped = jar.dump_all();

        // apex 加载为 domain cookie，cookie.domain() == ".yuketang.cn"
        // dump_all 去掉前置点 → key 仍是 "yuketang.cn"
        assert_eq!(
            dumped.get("yuketang.cn").and_then(|m| m.get("sessionid")),
            Some(&"abc".to_string()),
            "apex domain cookie should roundtrip, dumped={:?}",
            dumped
        );
    }

    #[test]
    fn load_then_dump_subdomain_is_host_only() {
        let jar = DomainAwareJar::default();
        let mut input = HashMap::new();
        let mut sub = HashMap::new();
        sub.insert("session_sub".to_string(), "sub_value".to_string());
        input.insert("changjie.yuketang.cn".to_string(), sub);

        jar.load_all(&input);
        let dumped = jar.dump_all();

        // 子域加载为 host-only：dump 通过 hosts_seen 反查到 "changjie.yuketang.cn"
        assert_eq!(
            dumped
                .get("changjie.yuketang.cn")
                .and_then(|m| m.get("session_sub")),
            Some(&"sub_value".to_string()),
            "subdomain host-only cookie should roundtrip, dumped={:?}",
            dumped
        );
        // 不应被升级为 .yuketang.cn 域
        assert!(
            dumped.get("yuketang.cn").is_none(),
            "subdomain cookie leaked to apex, dumped={:?}",
            dumped
        );
    }

    #[test]
    fn multi_domain_roundtrip_preserves_each_origin() {
        let jar = DomainAwareJar::default();
        let mut input = HashMap::new();
        let mut yk = HashMap::new();
        yk.insert("sessionid".to_string(), "yk_ses".to_string());
        input.insert("yuketang.cn".to_string(), yk);

        let mut ex = HashMap::new();
        ex.insert("sessionid".to_string(), "exam_ses".to_string());
        input.insert("examination.xuetangx.com".to_string(), ex);

        jar.load_all(&input);
        let dumped = jar.dump_all();

        // 不同根域的同名 cookie 各归各
        assert_eq!(
            dumped.get("yuketang.cn").and_then(|m| m.get("sessionid")),
            Some(&"yk_ses".to_string())
        );
        assert_eq!(
            dumped
                .get("examination.xuetangx.com")
                .and_then(|m| m.get("sessionid")),
            Some(&"exam_ses".to_string())
        );
    }

    #[test]
    fn set_cookies_host_only_can_be_dumped() {
        // H2 修复验证：服务端 Set-Cookie 不带 Domain 属性时，依然能 dump 出来
        let jar = DomainAwareJar::default();
        let url: url::Url = "https://www.yuketang.cn/".parse().unwrap();
        let headers = vec![
            HeaderValue::from_static("hostonly_ses=ho_val; Path=/"),
        ];
        let mut iter = headers.iter();
        CookieStore::set_cookies(&jar, &mut iter, &url);

        let dumped = jar.dump_all();
        assert_eq!(
            dumped
                .get("www.yuketang.cn")
                .and_then(|m| m.get("hostonly_ses")),
            Some(&"ho_val".to_string()),
            "host-only cookie should still appear in dump, dumped={:?}",
            dumped
        );
    }

    #[test]
    fn set_cookies_with_domain_attr_dumped_by_apex() {
        // 显式 Domain=.yuketang.cn → dump key = "yuketang.cn"
        let jar = DomainAwareJar::default();
        let url: url::Url = "https://www.yuketang.cn/".parse().unwrap();
        let headers = vec![HeaderValue::from_static(
            "sessionid=server_set; Domain=.yuketang.cn; Path=/",
        )];
        let mut iter = headers.iter();
        CookieStore::set_cookies(&jar, &mut iter, &url);

        let dumped = jar.dump_all();
        assert_eq!(
            dumped.get("yuketang.cn").and_then(|m| m.get("sessionid")),
            Some(&"server_set".to_string()),
            "domain cookie should be keyed by apex, dumped={:?}",
            dumped
        );
    }

    #[test]
    fn cookies_for_url_returns_combined_string() {
        let jar = DomainAwareJar::default();
        let url: url::Url = "https://www.yuketang.cn/".parse().unwrap();
        let mut iter = vec![HeaderValue::from_static(
            "sessionid=foo; Domain=.yuketang.cn; Path=/",
        )];
        let mut it = iter.iter_mut().map(|h| &*h);
        CookieStore::set_cookies(&jar, &mut it, &url);

        let cookie_header = CookieStore::cookies(&jar, &url);
        assert!(cookie_header.is_some());
        let s = cookie_header.unwrap();
        assert!(
            s.to_str().unwrap().contains("sessionid=foo"),
            "got: {:?}",
            s
        );
    }

    #[test]
    fn derive_attr_apex_vs_subdomain() {
        assert_eq!(
            derive_domain_attr_for_load("yuketang.cn"),
            Some(".yuketang.cn".to_string())
        );
        assert_eq!(
            derive_domain_attr_for_load("xuetangx.com"),
            Some(".xuetangx.com".to_string())
        );
        // 子域 → host-only
        assert_eq!(derive_domain_attr_for_load("www.yuketang.cn"), None);
        assert_eq!(derive_domain_attr_for_load("changjie.yuketang.cn"), None);
        assert_eq!(derive_domain_attr_for_load("examination.xuetangx.com"), None);
        // IP / localhost → host-only
        assert_eq!(derive_domain_attr_for_load("localhost"), None);
        assert_eq!(derive_domain_attr_for_load("127.0.0.1"), None);
    }

    #[test]
    fn subdomain_cookie_does_not_leak_to_sibling() {
        // 关键 H3 验证：changjie.yuketang.cn 的 cookie 不应被 www.yuketang.cn 收到
        let jar = DomainAwareJar::default();
        let mut input = HashMap::new();
        let mut m = HashMap::new();
        m.insert("private_token".to_string(), "secret".to_string());
        input.insert("changjie.yuketang.cn".to_string(), m);
        jar.load_all(&input);

        // 用 reqwest 的 cookies(url) 模拟请求 www.yuketang.cn 时会带哪些 cookie
        let www_url: url::Url = "https://www.yuketang.cn/api".parse().unwrap();
        let header = CookieStore::cookies(&jar, &www_url);
        if let Some(h) = header {
            let s = h.to_str().unwrap();
            assert!(
                !s.contains("private_token"),
                "host-only cookie leaked to sibling subdomain! got: {}",
                s
            );
        }

        // 但 changjie.yuketang.cn 自身应该能收到
        let cj_url: url::Url = "https://changjie.yuketang.cn/api".parse().unwrap();
        let header2 = CookieStore::cookies(&jar, &cj_url).expect("cookie 应该可用");
        assert!(header2.to_str().unwrap().contains("private_token"));
    }
}
