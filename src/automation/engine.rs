use crate::api::TorboxClient;
use crate::api::types::{Torrent, WebDownload, UsenetDownload};
use crate::automation::types::*;
use chrono::DateTime;
use leptos::logging::log;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct AutomationEngine;

impl AutomationEngine {
    pub fn new() -> Self {
        Self
    }

    pub async fn execute_rule(
        &self,
        rule: &AutomationRule,
        api_key: &str,
    ) -> Result<ExecutionResult, String> {
        let client = TorboxClient::new(api_key.to_string());

        match rule.download_category {
            DownloadCategory::Torrent => self.execute_torrent_rule(rule, &client).await,
            DownloadCategory::WebDownload => self.execute_webdl_rule(rule, &client).await,
            DownloadCategory::Usenet => self.execute_usenet_rule(rule, &client).await,
        }
    }

    // ── Torrent ──────────────────────────────────────────────────────────────

    async fn execute_torrent_rule(
        &self,
        rule: &AutomationRule,
        client: &TorboxClient,
    ) -> Result<ExecutionResult, String> {
        log!("Fetching torrent list for rule: {}", rule.name);
        let items = self.fetch_torrents_with_retry(client, &rule.name, 3).await?;
        log!("Fetched {} torrents for rule: {}", items.len(), rule.name);

        let matching: Vec<&Torrent> = items.iter()
            .filter(|t| self.evaluate_torrent_conditions(rule, t))
            .collect();

        self.process_items(
            rule,
            matching.len() as i32,
            matching.into_iter().map(|t| (t.id, t.name.clone())).collect(),
            |id| {
                let client = client.clone();
                let action = rule.action_config.action_type.clone();
                async move { execute_torrent_action(&action, &client, id).await }
            },
        ).await
    }

    async fn fetch_torrents_with_retry(
        &self,
        client: &TorboxClient,
        rule_name: &str,
        max_retries: u32,
    ) -> Result<Vec<Torrent>, String> {
        let mut last_error = None;
        for attempt in 1..=max_retries {
            match client.get_torrent_list(None, Some(true), None, None).await {
                Ok(response) => {
                    if let Some(data) = response.data {
                        return Ok(data);
                    } else {
                        return Err("No torrent data returned".to_string());
                    }
                }
                Err(e) => {
                    let error_str = e.to_string();
                    let is_transient = error_str.contains("530")
                        || error_str.contains("504")
                        || error_str.contains("502")
                        || error_str.contains("503")
                        || error_str.contains("Network error");
                    last_error = Some(error_str.clone());
                    if is_transient && attempt < max_retries {
                        tokio::time::sleep(tokio::time::Duration::from_secs(attempt as u64)).await;
                    } else if !is_transient {
                        return Err(format!("Failed to fetch torrents: {}", error_str));
                    }
                }
            }
        }
        Err(format!("Failed to fetch torrents after {} attempts: {}", max_retries,
            last_error.unwrap_or_else(|| "Unknown error".to_string())))
    }

    // ── Web Downloads ─────────────────────────────────────────────────────────

    async fn execute_webdl_rule(
        &self,
        rule: &AutomationRule,
        client: &TorboxClient,
    ) -> Result<ExecutionResult, String> {
        log!("Fetching web download list for rule: {}", rule.name);
        let items = self.fetch_webdl_with_retry(client, &rule.name, 3).await?;
        log!("Fetched {} web downloads for rule: {}", items.len(), rule.name);

        let matching: Vec<&WebDownload> = items.iter()
            .filter(|w| self.evaluate_webdl_conditions(rule, w))
            .collect();

        self.process_items(
            rule,
            matching.len() as i32,
            matching.into_iter().map(|w| (w.id, w.name.clone())).collect(),
            |id| {
                let client = client.clone();
                let action = rule.action_config.action_type.clone();
                async move { execute_webdl_action(&action, &client, id).await }
            },
        ).await
    }

    async fn fetch_webdl_with_retry(
        &self,
        client: &TorboxClient,
        rule_name: &str,
        max_retries: u32,
    ) -> Result<Vec<WebDownload>, String> {
        let mut last_error = None;
        for attempt in 1..=max_retries {
            match client.get_web_download_list(None, Some(true), None, None).await {
                Ok(response) => {
                    if let Some(data) = response.data {
                        return Ok(data);
                    } else {
                        return Err("No web download data returned".to_string());
                    }
                }
                Err(e) => {
                    let error_str = e.to_string();
                    let is_transient = error_str.contains("530")
                        || error_str.contains("504")
                        || error_str.contains("502")
                        || error_str.contains("503")
                        || error_str.contains("Network error");
                    last_error = Some(error_str.clone());
                    if is_transient && attempt < max_retries {
                        tokio::time::sleep(tokio::time::Duration::from_secs(attempt as u64)).await;
                    } else if !is_transient {
                        return Err(format!("Failed to fetch web downloads: {}", error_str));
                    }
                }
            }
        }
        Err(format!("Failed to fetch web downloads after {} attempts: {}", max_retries,
            last_error.unwrap_or_else(|| "Unknown error".to_string())))
    }

    // ── Usenet ────────────────────────────────────────────────────────────────

    async fn execute_usenet_rule(
        &self,
        rule: &AutomationRule,
        client: &TorboxClient,
    ) -> Result<ExecutionResult, String> {
        log!("Fetching usenet download list for rule: {}", rule.name);
        let items = self.fetch_usenet_with_retry(client, &rule.name, 3).await?;
        log!("Fetched {} usenet downloads for rule: {}", items.len(), rule.name);

        let matching: Vec<&UsenetDownload> = items.iter()
            .filter(|u| self.evaluate_usenet_conditions(rule, u))
            .collect();

        self.process_items(
            rule,
            matching.len() as i32,
            matching.into_iter().map(|u| (u.id, u.name.clone())).collect(),
            |id| {
                let client = client.clone();
                let action = rule.action_config.action_type.clone();
                async move { execute_usenet_action(&action, &client, id).await }
            },
        ).await
    }

    async fn fetch_usenet_with_retry(
        &self,
        client: &TorboxClient,
        rule_name: &str,
        max_retries: u32,
    ) -> Result<Vec<UsenetDownload>, String> {
        let mut last_error = None;
        for attempt in 1..=max_retries {
            match client.get_usenet_download_list(None, Some(true), None, None).await {
                Ok(response) => {
                    if let Some(data) = response.data {
                        return Ok(data);
                    } else {
                        return Err("No usenet data returned".to_string());
                    }
                }
                Err(e) => {
                    let error_str = e.to_string();
                    let is_transient = error_str.contains("530")
                        || error_str.contains("504")
                        || error_str.contains("502")
                        || error_str.contains("503")
                        || error_str.contains("Network error");
                    last_error = Some(error_str.clone());
                    if is_transient && attempt < max_retries {
                        tokio::time::sleep(tokio::time::Duration::from_secs(attempt as u64)).await;
                    } else if !is_transient {
                        return Err(format!("Failed to fetch usenet downloads: {}", error_str));
                    }
                }
            }
        }
        Err(format!("Failed to fetch usenet downloads after {} attempts: {}", max_retries,
            last_error.unwrap_or_else(|| "Unknown error".to_string())))
    }

    // ── Shared processor ─────────────────────────────────────────────────────

    async fn process_items<F, Fut>(
        &self,
        rule: &AutomationRule,
        total_items: i32,
        items: Vec<(i32, String)>,
        action_fn: F,
    ) -> Result<ExecutionResult, String>
    where
        F: Fn(i32) -> Fut,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        if items.is_empty() {
            return Ok(ExecutionResult {
                items_processed: 0,
                total_items: 0,
                success: true,
                error_message: None,
                processed_items: Some(Vec::new()),
                partial: false,
            });
        }

        let action_name = match rule.action_config.action_type {
            ActionType::StopSeeding => "Stop Seeding",
            ActionType::Delete => "Delete",
            ActionType::Stop => "Stop",
            ActionType::Resume => "Resume",
            ActionType::Restart => "Restart",
            ActionType::Reannounce => "Reannounce",
            ActionType::ForceStart => "Force Start",
        }.to_string();

        let mut error_count = 0;
        let mut errors: Vec<String> = Vec::new();
        let mut processed_items: Vec<ProcessedItem> = Vec::new();
        let per_item_timeout = tokio::time::Duration::from_secs(10);

        for (idx, (id, name)) in items.iter().enumerate() {
            if idx > 0 && idx % 10 == 0 {
                log!("Processed {}/{} items for rule '{}'", idx, items.len(), rule.name);
            }

            let result = match tokio::time::timeout(per_item_timeout, action_fn(*id)).await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) => {
                    if e.contains("429") || e.contains("Rate limit") {
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                        action_fn(*id).await
                    } else {
                        Err(e)
                    }
                }
                Err(_) => Err("Action timed out after 10 seconds".to_string()),
            };

            let success = result.is_ok();
            let error = result.as_ref().err().map(|e| e.to_string());

            processed_items.push(ProcessedItem {
                id: *id,
                name: name.clone(),
                action: action_name.clone(),
                success,
                error: error.clone(),
            });

            if let Err(e) = result {
                error_count += 1;
                if errors.len() < 10 {
                    errors.push(e);
                }
            }
        }

        let items_processed = processed_items.len() as i32;
        let partial = items_processed < total_items;

        Ok(ExecutionResult {
            items_processed,
            total_items,
            success: error_count == 0 && !partial,
            error_message: if error_count > 0 || partial {
                let mut parts = Vec::new();
                if partial {
                    parts.push(format!("Only processed {}/{} items", items_processed, total_items));
                }
                if error_count > 0 {
                    parts.push(format!("{} of {} actions failed. {}",
                        error_count, items_processed, errors.join("; ")));
                }
                Some(parts.join(". "))
            } else {
                None
            },
            processed_items: Some(processed_items),
            partial,
        })
    }

    // ── Condition evaluators ──────────────────────────────────────────────────

    fn evaluate_torrent_conditions(&self, rule: &AutomationRule, item: &Torrent) -> bool {
        rule.conditions.iter().all(|c| self.evaluate_torrent_condition(c, item))
    }

    fn evaluate_torrent_condition(&self, condition: &Condition, item: &Torrent) -> bool {
        let now = now_secs();

        let val: Option<f64> = match condition.r#type {
            ConditionType::SeedingTime => {
                if !item.active || !item.download_finished { return false; }
                let ts = item.cached_at.as_ref()
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .or_else(|| DateTime::parse_from_rfc3339(&item.updated_at).ok());
                ts.map(|t| (now - t.timestamp()) as f64 / 3600.0)
            }
            ConditionType::SeedingRatio => {
                if !item.active { return false; }
                Some(item.ratio as f64)
            }
            ConditionType::StalledTime => {
                let sl = item.download_state.to_lowercase();
                if sl.contains("stalled") {
                    DateTime::parse_from_rfc3339(&item.created_at).ok()
                        .map(|t| (now - t.timestamp()) as f64 / 3600.0)
                } else {
                    let is_dl = sl == "downloading" || sl == "active" || sl.contains("downloading");
                    let is_checking = sl == "checking";
                    if !is_dl && !is_checking { return false; }
                    let stalled = if is_checking {
                        DateTime::parse_from_rfc3339(&item.updated_at).ok()
                            .map(|t| now - t.timestamp() > 21600).unwrap_or(false)
                    } else {
                        item.download_speed < 1024 && item.upload_speed == 0
                            && (item.seeds == 0 && item.peers == 0 || !item.active)
                    };
                    if !stalled { return false; }
                    DateTime::parse_from_rfc3339(&item.updated_at).ok()
                        .map(|t| (now - t.timestamp()) as f64 / 3600.0)
                }
            }
            ConditionType::Seeds => Some(item.seeds as f64),
            ConditionType::Peers => Some(item.peers as f64),
            ConditionType::TotalUploaded => Some(item.total_uploaded as f64 / (1024.0 * 1024.0 * 1024.0)),
            ConditionType::LongTermSeeding => return bool_match(item.long_term_seeding, condition.value),
            ConditionType::SeedTorrent => return bool_match(item.seed_torrent, condition.value),
            ConditionType::HasMagnet => return bool_match(item.magnet.is_some(), condition.value),
            ConditionType::AllowZipped => return bool_match(item.allow_zipped, condition.value),
            ConditionType::TorrentFile => return bool_match(item.torrent_file, condition.value),
            // Shared fields
            ConditionType::Age => DateTime::parse_from_rfc3339(&item.created_at).ok()
                .map(|t| (now - t.timestamp()) as f64 / 3600.0),
            ConditionType::DownloadSpeed => Some(item.download_speed as f64),
            ConditionType::UploadSpeed => Some(item.upload_speed as f64),
            ConditionType::FileSize => Some(item.size as f64 / (1024.0 * 1024.0 * 1024.0)),
            ConditionType::Progress => Some(item.progress as f64),
            ConditionType::TotalDownloaded => Some(item.total_downloaded as f64 / (1024.0 * 1024.0 * 1024.0)),
            ConditionType::DownloadState => return match condition.value as i32 {
                0 => item.download_state == "downloading",
                1 => item.download_state == "uploading" || item.download_state == "uploading (no peers)",
                2 => item.download_state == "stopped seeding" || item.download_state == "stopped",
                3 => item.download_state == "cached",
                _ => false,
            },
            ConditionType::Inactive => Some(if is_torrent_inactive(item, now) { 1.0 } else { 0.0 }),
            ConditionType::DownloadFinished => return bool_match(item.download_finished, condition.value),
            ConditionType::Cached => return bool_match(item.cached, condition.value),
            ConditionType::Private => return bool_match(item.private, condition.value),
            ConditionType::ETA => Some(item.eta as f64 / 3600.0),
            ConditionType::Availability => Some(item.availability as f64),
            ConditionType::ExpiresAt => item.expires_at.as_ref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|t| ((t.timestamp() - now) as f64 / 3600.0).max(0.0)),
            ConditionType::DownloadPresent => return bool_match(item.download_present, condition.value),
        };

        compare(val.unwrap_or(0.0), &condition.operator, condition.value)
    }

    fn evaluate_webdl_conditions(&self, rule: &AutomationRule, item: &WebDownload) -> bool {
        rule.conditions.iter().all(|c| self.evaluate_webdl_condition(c, item))
    }

    fn evaluate_webdl_condition(&self, condition: &Condition, item: &WebDownload) -> bool {
        let now = now_secs();

        let val: Option<f64> = match condition.r#type {
            // Torrent-only conditions — skip for web downloads
            ConditionType::SeedingTime | ConditionType::SeedingRatio
            | ConditionType::StalledTime | ConditionType::Seeds | ConditionType::Peers
            | ConditionType::TotalUploaded | ConditionType::LongTermSeeding
            | ConditionType::SeedTorrent | ConditionType::HasMagnet
            | ConditionType::AllowZipped | ConditionType::TorrentFile
            | ConditionType::Cached | ConditionType::Private => return false,
            // Shared
            ConditionType::Age => DateTime::parse_from_rfc3339(&item.created_at).ok()
                .map(|t| (now - t.timestamp()) as f64 / 3600.0),
            ConditionType::DownloadSpeed => Some(item.download_speed as f64),
            ConditionType::UploadSpeed => Some(item.upload_speed as f64),
            ConditionType::FileSize => Some(item.size as f64 / (1024.0 * 1024.0 * 1024.0)),
            ConditionType::Progress => Some(item.progress as f64),
            ConditionType::TotalDownloaded => None,
            ConditionType::DownloadState => return match condition.value as i32 {
                0 => item.download_state == "downloading",
                2 => item.download_state == "stopped",
                3 => item.download_state == "cached",
                _ => false,
            },
            ConditionType::Inactive => {
                let sl = item.download_state.to_lowercase();
                let inactive = sl == "failed" || sl.starts_with("failed") || sl == "error"
                    || sl == "expired" || (!item.active && !item.download_finished
                    && !sl.contains("cached") && !sl.contains("downloading"));
                Some(if inactive { 1.0 } else { 0.0 })
            }
            ConditionType::DownloadFinished => return bool_match(item.download_finished, condition.value),
            ConditionType::ETA => Some(item.eta as f64 / 3600.0),
            ConditionType::Availability => Some(item.availability as f64),
            ConditionType::ExpiresAt => item.expires_at.as_ref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|t| ((t.timestamp() - now) as f64 / 3600.0).max(0.0)),
            ConditionType::DownloadPresent => return bool_match(item.download_present, condition.value),
        };

        compare(val.unwrap_or(0.0), &condition.operator, condition.value)
    }

    fn evaluate_usenet_conditions(&self, rule: &AutomationRule, item: &UsenetDownload) -> bool {
        rule.conditions.iter().all(|c| self.evaluate_usenet_condition(c, item))
    }

    fn evaluate_usenet_condition(&self, condition: &Condition, item: &UsenetDownload) -> bool {
        let now = now_secs();

        let val: Option<f64> = match condition.r#type {
            // Torrent-only conditions — skip for usenet
            ConditionType::SeedingTime | ConditionType::SeedingRatio
            | ConditionType::StalledTime | ConditionType::Seeds | ConditionType::Peers
            | ConditionType::TotalUploaded | ConditionType::LongTermSeeding
            | ConditionType::SeedTorrent | ConditionType::HasMagnet
            | ConditionType::AllowZipped | ConditionType::TorrentFile
            | ConditionType::Private | ConditionType::UploadSpeed => return false,
            // Shared
            ConditionType::Age => DateTime::parse_from_rfc3339(&item.created_at).ok()
                .map(|t| (now - t.timestamp()) as f64 / 3600.0),
            ConditionType::DownloadSpeed => Some(item.download_speed as f64),
            ConditionType::FileSize => Some(item.size as f64 / (1024.0 * 1024.0 * 1024.0)),
            ConditionType::Progress => Some(item.progress as f64),
            ConditionType::TotalDownloaded => None,
            ConditionType::DownloadState => return match condition.value as i32 {
                0 => item.download_state == "downloading",
                2 => item.download_state == "stopped",
                3 => item.download_state == "cached",
                _ => false,
            },
            ConditionType::Inactive => {
                let sl = item.download_state.to_lowercase();
                let inactive = sl == "failed" || sl.starts_with("failed") || sl == "error"
                    || sl == "expired" || (!item.active && !item.download_finished
                    && !sl.contains("cached") && !sl.contains("downloading"));
                Some(if inactive { 1.0 } else { 0.0 })
            }
            ConditionType::DownloadFinished => return bool_match(item.download_finished, condition.value),
            ConditionType::Cached => return bool_match(item.cached, condition.value),
            ConditionType::ETA => Some(item.eta as f64 / 3600.0),
            ConditionType::Availability => None,
            ConditionType::ExpiresAt => item.expires_at.as_ref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|t| ((t.timestamp() - now) as f64 / 3600.0).max(0.0)),
            ConditionType::DownloadPresent => return bool_match(item.download_present, condition.value),
            ConditionType::UploadSpeed => None,
        };

        compare(val.unwrap_or(0.0), &condition.operator, condition.value)
    }
}

// ── Action executors ──────────────────────────────────────────────────────────

async fn execute_torrent_action(action: &ActionType, client: &TorboxClient, id: i32) -> Result<(), String> {
    let op = match action {
        ActionType::StopSeeding => "stop_seeding",
        ActionType::Delete => "delete",
        ActionType::Stop => "stop",
        ActionType::Resume => "resume",
        ActionType::Restart => "restart",
        ActionType::Reannounce => "reannounce",
        ActionType::ForceStart => "start",
    };
    client.control_torrent(op.to_string(), id, false).await
        .map_err(|e| format!("Torrent action '{}' failed: {}", op, e))?;
    Ok(())
}

async fn execute_webdl_action(action: &ActionType, client: &TorboxClient, id: i32) -> Result<(), String> {
    let op = match action {
        ActionType::Delete => "delete",
        ActionType::Stop => "stop",
        ActionType::Resume => "resume",
        ActionType::StopSeeding | ActionType::Restart
        | ActionType::Reannounce | ActionType::ForceStart => {
            return Err(format!("Action '{:?}' is not supported for web downloads", action));
        }
    };
    client.control_web_download(op.to_string(), id, false).await
        .map_err(|e| format!("Web download action '{}' failed: {}", op, e))?;
    Ok(())
}

async fn execute_usenet_action(action: &ActionType, client: &TorboxClient, id: i32) -> Result<(), String> {
    let op = match action {
        ActionType::Delete => "delete",
        ActionType::Stop => "stop",
        ActionType::Resume => "resume",
        ActionType::StopSeeding | ActionType::Restart
        | ActionType::Reannounce | ActionType::ForceStart => {
            return Err(format!("Action '{:?}' is not supported for usenet downloads", action));
        }
    };
    client.control_usenet_download(op.to_string(), id, false).await
        .map_err(|e| format!("Usenet action '{}' failed: {}", op, e))?;
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

fn bool_match(b: bool, value: f64) -> bool {
    (if b { 1.0_f64 } else { 0.0_f64 } - value).abs() < 0.001
}

fn compare(lhs: f64, op: &Operator, rhs: f64) -> bool {
    match op {
        Operator::GreaterThan => lhs > rhs,
        Operator::LessThan => lhs < rhs,
        Operator::GreaterThanOrEqual => lhs >= rhs,
        Operator::LessThanOrEqual => lhs <= rhs,
        Operator::Equal => (lhs - rhs).abs() < 0.001,
    }
}

fn is_torrent_inactive(item: &crate::api::types::Torrent, now: i64) -> bool {
    let sl = item.download_state.to_lowercase();
    if matches!(sl.as_str(), "reported missing" | "missingfiles" | "failed" | "error") || sl.starts_with("failed") {
        return true;
    }
    if item.download_finished { return false; }
    let is_expired = item.expires_at.as_ref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.timestamp() < now)
        .unwrap_or(false);
    if is_expired || sl == "expired" { return true; }
    if !item.active && !sl.contains("cached") && !sl.contains("completed")
        && !sl.contains("uploading") && !sl.contains("seeding")
        && !sl.contains("stalled") { return true; }
    matches!(sl.as_str(), "stopped seeding" | "stopped" | "error" | "failed")
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

impl Default for AutomationEngine {
    fn default() -> Self {
        Self::new()
    }
}
