use crate::api::TorboxClient;
use crate::api::types::{Torrent, WebDownload, UsenetDownload};
use crate::automation::types::*;
use chrono::DateTime;
use leptos::logging::log;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct AutomationEngine;

impl AutomationEngine {
    pub fn new() -> Self { Self }

    pub async fn execute_rule(&self, rule: &AutomationRule, api_key: &str) -> Result<ExecutionResult, String> {
        let client = TorboxClient::new(api_key.to_string());
        match rule.download_category() {
            DownloadCategory::Torrent => self.execute_torrent_rule(rule, &client).await,
            DownloadCategory::WebDownload => self.execute_webdl_rule(rule, &client).await,
            DownloadCategory::Usenet => self.execute_usenet_rule(rule, &client).await,
        }
    }

    async fn execute_torrent_rule(&self, rule: &AutomationRule, client: &TorboxClient) -> Result<ExecutionResult, String> {
        log!("Fetching torrent list for rule: {}", rule.name);
        let items = self.fetch_with_retry(|| client.get_torrent_list(None, Some(true), None, None), &rule.name, "torrents").await?;
        let matching: Vec<(i32, String)> = items.iter()
            .filter(|t| self.eval_torrent_conditions(rule, t))
            .map(|t| (t.id, t.name.clone()))
            .collect();
        log!("Rule '{}' matched {} torrents", rule.name, matching.len());
        self.process(rule, matching, |id| {
            let c = client.clone(); let a = rule.action_config.action_type.clone();
            async move { exec_torrent_action(&a, &c, id).await }
        }).await
    }

    async fn execute_webdl_rule(&self, rule: &AutomationRule, client: &TorboxClient) -> Result<ExecutionResult, String> {
        log!("Fetching web download list for rule: {}", rule.name);
        let items = self.fetch_with_retry(|| client.get_web_download_list(None, Some(true), None, None), &rule.name, "web downloads").await?;
        let matching: Vec<(i32, String)> = items.iter()
            .filter(|w| self.eval_webdl_conditions(rule, w))
            .map(|w| (w.id, w.name.clone()))
            .collect();
        log!("Rule '{}' matched {} web downloads", rule.name, matching.len());
        self.process(rule, matching, |id| {
            let c = client.clone(); let a = rule.action_config.action_type.clone();
            async move { exec_webdl_action(&a, &c, id).await }
        }).await
    }

    async fn execute_usenet_rule(&self, rule: &AutomationRule, client: &TorboxClient) -> Result<ExecutionResult, String> {
        log!("Fetching usenet list for rule: {}", rule.name);
        let items = self.fetch_with_retry(|| client.get_usenet_download_list(None, Some(true), None, None), &rule.name, "usenet").await?;
        let matching: Vec<(i32, String)> = items.iter()
            .filter(|u| self.eval_usenet_conditions(rule, u))
            .map(|u| (u.id, u.name.clone()))
            .collect();
        log!("Rule '{}' matched {} usenet downloads", rule.name, matching.len());
        self.process(rule, matching, |id| {
            let c = client.clone(); let a = rule.action_config.action_type.clone();
            async move { exec_usenet_action(&a, &c, id).await }
        }).await
    }

    async fn fetch_with_retry<T, F, Fut>(&self, f: F, rule_name: &str, kind: &str) -> Result<Vec<T>, String>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<crate::api::types::ApiResponse<Vec<T>>, crate::api::ApiError>>,
    {
        for attempt in 1u32..=3 {
            match f().await {
                Ok(r) => return r.data.ok_or_else(|| format!("No {} data returned", kind)),
                Err(e) => {
                    let s = e.to_string();
                    let transient = s.contains("530") || s.contains("504") || s.contains("502") || s.contains("503") || s.contains("Network error");
                    if transient && attempt < 3 {
                        log!("Transient error for rule '{}', retry {}/3: {}", rule_name, attempt, s);
                        tokio::time::sleep(tokio::time::Duration::from_secs(attempt as u64)).await;
                    } else {
                        return Err(format!("Failed to fetch {}: {}", kind, s));
                    }
                }
            }
        }
        Err(format!("Failed to fetch {} after 3 attempts", kind))
    }

    async fn process<F, Fut>(&self, rule: &AutomationRule, items: Vec<(i32, String)>, action_fn: F) -> Result<ExecutionResult, String>
    where
        F: Fn(i32) -> Fut,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        let total = items.len() as i32;
        if items.is_empty() {
            return Ok(ExecutionResult { items_processed: 0, total_items: 0, success: true, error_message: None, processed_items: Some(vec![]), partial: false });
        }
        let action_name = match rule.action_config.action_type {
            ActionType::StopSeeding => "Stop Seeding", ActionType::Delete => "Delete",
            ActionType::Stop => "Stop", ActionType::Resume => "Resume",
            ActionType::Restart => "Restart", ActionType::Reannounce => "Reannounce",
            ActionType::ForceStart => "Force Start",
        }.to_string();

        let mut error_count = 0;
        let mut errors: Vec<String> = vec![];
        let mut processed: Vec<ProcessedItem> = vec![];
        let timeout = tokio::time::Duration::from_secs(10);

        for (id, name) in &items {
            let result = match tokio::time::timeout(timeout, action_fn(*id)).await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) if e.contains("429") || e.contains("Rate limit") => {
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    action_fn(*id).await
                }
                Ok(Err(e)) => Err(e),
                Err(_) => Err("Action timed out after 10 seconds".to_string()),
            };
            let ok = result.is_ok();
            let err = result.err();
            if err.is_some() { error_count += 1; if errors.len() < 10 { errors.push(err.clone().unwrap()); } }
            processed.push(ProcessedItem { id: *id, name: name.clone(), action: action_name.clone(), success: ok, error: err });
        }

        let done = processed.len() as i32;
        let partial = done < total;
        Ok(ExecutionResult {
            items_processed: done, total_items: total,
            success: error_count == 0 && !partial,
            error_message: if error_count > 0 || partial {
                let mut parts = vec![];
                if partial { parts.push(format!("Only processed {}/{} items", done, total)); }
                if error_count > 0 { parts.push(format!("{} of {} actions failed. {}", error_count, done, errors.join("; "))); }
                Some(parts.join(". "))
            } else { None },
            processed_items: Some(processed),
            partial,
        })
    }

    fn eval_torrent_conditions(&self, rule: &AutomationRule, item: &Torrent) -> bool {
        rule.conditions.iter().all(|c| eval_torrent(c, item))
    }
    fn eval_webdl_conditions(&self, rule: &AutomationRule, item: &WebDownload) -> bool {
        rule.conditions.iter().all(|c| eval_webdl(c, item))
    }
    fn eval_usenet_conditions(&self, rule: &AutomationRule, item: &UsenetDownload) -> bool {
        rule.conditions.iter().all(|c| eval_usenet(c, item))
    }
}

// ── Action executors ──────────────────────────────────────────────────────────

async fn exec_torrent_action(action: &ActionType, client: &TorboxClient, id: i32) -> Result<(), String> {
    let op = match action {
        ActionType::StopSeeding => "stop_seeding", ActionType::Delete => "delete",
        ActionType::Stop => "stop", ActionType::Resume => "resume",
        ActionType::Restart => "restart", ActionType::Reannounce => "reannounce",
        ActionType::ForceStart => "start",
    };
    client.control_torrent(op.to_string(), id, false).await.map(|_| ()).map_err(|e| format!("{}", e))
}

async fn exec_webdl_action(action: &ActionType, client: &TorboxClient, id: i32) -> Result<(), String> {
    let op = match action {
        ActionType::Delete => "delete", ActionType::Stop => "stop", ActionType::Resume => "resume",
        _ => return Err(format!("Action {:?} not supported for web downloads", action)),
    };
    client.control_web_download(op.to_string(), id, false).await.map(|_| ()).map_err(|e| format!("{}", e))
}

async fn exec_usenet_action(action: &ActionType, client: &TorboxClient, id: i32) -> Result<(), String> {
    let op = match action {
        ActionType::Delete => "delete", ActionType::Stop => "stop", ActionType::Resume => "resume",
        _ => return Err(format!("Action {:?} not supported for usenet downloads", action)),
    };
    client.control_usenet_download(op.to_string(), id, false).await.map(|_| ()).map_err(|e| format!("{}", e))
}

// ── Condition evaluators ──────────────────────────────────────────────────────

fn now_secs() -> i64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64 }
fn boolmatch(b: bool, v: f64) -> bool { ((if b { 1.0_f64 } else { 0.0 }) - v).abs() < 0.001 }
fn cmp(lhs: f64, op: &Operator, rhs: f64) -> bool {
    match op { Operator::GreaterThan => lhs > rhs, Operator::LessThan => lhs < rhs,
               Operator::GreaterThanOrEqual => lhs >= rhs, Operator::LessThanOrEqual => lhs <= rhs,
               Operator::Equal => (lhs - rhs).abs() < 0.001 }
}

fn eval_torrent(c: &Condition, t: &Torrent) -> bool {
    let now = now_secs();
    let val: Option<f64> = match c.r#type {
        ConditionType::SeedingTime => {
            if !t.active || !t.download_finished { return false; }
            t.cached_at.as_ref().and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .or_else(|| DateTime::parse_from_rfc3339(&t.updated_at).ok())
                .map(|d| (now - d.timestamp()) as f64 / 3600.0)
        }
        ConditionType::SeedingRatio => { if !t.active { return false; } Some(t.ratio as f64) }
        ConditionType::StalledTime => {
            let sl = t.download_state.to_lowercase();
            if sl.contains("stalled") {
                DateTime::parse_from_rfc3339(&t.created_at).ok().map(|d| (now - d.timestamp()) as f64 / 3600.0)
            } else {
                let is_dl = sl == "downloading" || sl == "active" || sl.contains("downloading");
                let is_chk = sl == "checking";
                if !is_dl && !is_chk { return false; }
                let stalled = if is_chk { DateTime::parse_from_rfc3339(&t.updated_at).ok().map(|d| now - d.timestamp() > 21600).unwrap_or(false) }
                              else { t.download_speed < 1024 && t.upload_speed == 0 && (t.seeds == 0 && t.peers == 0 || !t.active) };
                if !stalled { return false; }
                DateTime::parse_from_rfc3339(&t.updated_at).ok().map(|d| (now - d.timestamp()) as f64 / 3600.0)
            }
        }
        ConditionType::Seeds => Some(t.seeds as f64),
        ConditionType::Peers => Some(t.peers as f64),
        ConditionType::TotalUploaded => Some(t.total_uploaded as f64 / (1024.0*1024.0*1024.0)),
        ConditionType::LongTermSeeding => return boolmatch(t.long_term_seeding, c.value),
        ConditionType::SeedTorrent => return boolmatch(t.seed_torrent, c.value),
        ConditionType::HasMagnet => return boolmatch(t.magnet.is_some(), c.value),
        ConditionType::AllowZipped => return boolmatch(t.allow_zipped, c.value),
        ConditionType::TorrentFile => return boolmatch(t.torrent_file, c.value),
        ConditionType::Age => DateTime::parse_from_rfc3339(&t.created_at).ok().map(|d| (now - d.timestamp()) as f64 / 3600.0),
        ConditionType::DownloadSpeed => Some(t.download_speed as f64),
        ConditionType::UploadSpeed => Some(t.upload_speed as f64),
        ConditionType::FileSize => Some(t.size as f64 / (1024.0*1024.0*1024.0)),
        ConditionType::Progress => Some(t.progress as f64),
        ConditionType::TotalDownloaded => Some(t.total_downloaded as f64 / (1024.0*1024.0*1024.0)),
        ConditionType::DownloadState => return match c.value as i32 {
            0 => t.download_state == "downloading", 1 => t.download_state == "uploading" || t.download_state == "uploading (no peers)",
            2 => t.download_state == "stopped seeding" || t.download_state == "stopped", 3 => t.download_state == "cached", _ => false,
        },
        ConditionType::Inactive => {
            let sl = t.download_state.to_lowercase();
            let inactive = sl == "reported missing" || sl == "missingfiles" || sl == "failed" || sl == "error" || sl.starts_with("failed")
                || t.expires_at.as_ref().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|d| d.timestamp() < now).unwrap_or(false)
                || sl == "expired" || (!t.active && !sl.contains("cached") && !sl.contains("completed") && !sl.contains("uploading") && !sl.contains("seeding") && !sl.contains("stalled"))
                || sl == "stopped seeding" || sl == "stopped";
            Some(if inactive { 1.0 } else { 0.0 })
        }
        ConditionType::DownloadFinished => return boolmatch(t.download_finished, c.value),
        ConditionType::Cached => return boolmatch(t.cached, c.value),
        ConditionType::Private => return boolmatch(t.private, c.value),
        ConditionType::ETA => Some(t.eta as f64 / 3600.0),
        ConditionType::Availability => Some(t.availability as f64),
        ConditionType::ExpiresAt => t.expires_at.as_ref().and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|d| ((d.timestamp() - now) as f64 / 3600.0).max(0.0)),
        ConditionType::DownloadPresent => return boolmatch(t.download_present, c.value),
    };
    cmp(val.unwrap_or(0.0), &c.operator, c.value)
}

fn eval_webdl(c: &Condition, w: &WebDownload) -> bool {
    let now = now_secs();
    let val: Option<f64> = match c.r#type {
        ConditionType::SeedingTime | ConditionType::SeedingRatio | ConditionType::StalledTime
        | ConditionType::Seeds | ConditionType::Peers | ConditionType::TotalUploaded
        | ConditionType::LongTermSeeding | ConditionType::SeedTorrent | ConditionType::HasMagnet
        | ConditionType::AllowZipped | ConditionType::TorrentFile | ConditionType::Cached
        | ConditionType::Private => return false,
        ConditionType::Age => DateTime::parse_from_rfc3339(&w.created_at).ok().map(|d| (now - d.timestamp()) as f64 / 3600.0),
        ConditionType::DownloadSpeed => Some(w.download_speed as f64),
        ConditionType::UploadSpeed => Some(w.upload_speed as f64),
        ConditionType::FileSize => Some(w.size as f64 / (1024.0*1024.0*1024.0)),
        ConditionType::Progress => Some(w.progress as f64),
        ConditionType::TotalDownloaded => None,
        ConditionType::DownloadState => return match c.value as i32 { 0 => w.download_state == "downloading", 2 => w.download_state == "stopped", 3 => w.download_state == "cached", _ => false },
        ConditionType::Inactive => { let sl = w.download_state.to_lowercase(); Some(if sl == "failed" || sl.starts_with("failed") || sl == "error" || sl == "expired" || (!w.active && !w.download_finished && !sl.contains("cached") && !sl.contains("downloading")) { 1.0 } else { 0.0 }) }
        ConditionType::DownloadFinished => return boolmatch(w.download_finished, c.value),
        ConditionType::ETA => Some(w.eta as f64 / 3600.0),
        ConditionType::Availability => Some(w.availability as f64),
        ConditionType::ExpiresAt => w.expires_at.as_ref().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|d| ((d.timestamp()-now) as f64/3600.0).max(0.0)),
        ConditionType::DownloadPresent => return boolmatch(w.download_present, c.value),
    };
    cmp(val.unwrap_or(0.0), &c.operator, c.value)
}

fn eval_usenet(c: &Condition, u: &UsenetDownload) -> bool {
    let now = now_secs();
    let val: Option<f64> = match c.r#type {
        ConditionType::SeedingTime | ConditionType::SeedingRatio | ConditionType::StalledTime
        | ConditionType::Seeds | ConditionType::Peers | ConditionType::TotalUploaded
        | ConditionType::LongTermSeeding | ConditionType::SeedTorrent | ConditionType::HasMagnet
        | ConditionType::AllowZipped | ConditionType::TorrentFile | ConditionType::Private
        | ConditionType::UploadSpeed => return false,
        ConditionType::Age => DateTime::parse_from_rfc3339(&u.created_at).ok().map(|d| (now - d.timestamp()) as f64 / 3600.0),
        ConditionType::DownloadSpeed => Some(u.download_speed as f64),
        ConditionType::FileSize => Some(u.size as f64 / (1024.0*1024.0*1024.0)),
        ConditionType::Progress => Some(u.progress as f64),
        ConditionType::TotalDownloaded => None,
        ConditionType::DownloadState => return match c.value as i32 { 0 => u.download_state == "downloading", 2 => u.download_state == "stopped", 3 => u.download_state == "cached", _ => false },
        ConditionType::Inactive => { let sl = u.download_state.to_lowercase(); Some(if sl == "failed" || sl.starts_with("failed") || sl == "error" || sl == "expired" || (!u.active && !u.download_finished && !sl.contains("cached") && !sl.contains("downloading")) { 1.0 } else { 0.0 }) }
        ConditionType::DownloadFinished => return boolmatch(u.download_finished, c.value),
        ConditionType::Cached => return boolmatch(u.cached, c.value),
        ConditionType::ETA => Some(u.eta as f64 / 3600.0),
        ConditionType::Availability => None,
        ConditionType::ExpiresAt => u.expires_at.as_ref().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|d| ((d.timestamp()-now) as f64/3600.0).max(0.0)),
        ConditionType::DownloadPresent => return boolmatch(u.download_present, c.value),
    };
    cmp(val.unwrap_or(0.0), &c.operator, c.value)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionResult {
    pub items_processed: i32,
    pub total_items: i32,
    pub success: bool,
    pub error_message: Option<String>,
    pub processed_items: Option<Vec<ProcessedItem>>,
    pub partial: bool,
}

impl Default for AutomationEngine { fn default() -> Self { Self::new() } }
