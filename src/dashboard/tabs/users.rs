use leptos::prelude::*;
use leptos::task::spawn_local;
#[cfg(feature = "hydrate")]
use web_sys;
#[cfg(feature = "hydrate")]
use js_sys;
#[cfg(feature = "hydrate")]
use wasm_bindgen::JsCast;
use crate::dashboard::DashboardContext;
use crate::api::types::{Torrent, WebDownload, UsenetDownload};

fn format_bytes(bytes: i64) -> String {
    if bytes >= 1_099_511_627_776 {
        format!("{:.2} TB", bytes as f64 / 1_099_511_627_776.0)
    } else if bytes >= 1_073_741_824 {
        format!("{:.2} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.2} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} bytes", bytes)
    }
}

#[component]
pub fn UsersTab() -> impl IntoView {
    let context = use_context::<DashboardContext>()
        .expect("DashboardContext should be provided");

    let user_data = context.user_data;
    let user_loading = context.user_loading;

    // Monthly usage state
    let monthly_downloaded = RwSignal::new(0i64);
    let monthly_usage_loading = RwSignal::new(false);
    let monthly_usage_error = RwSignal::new(Option::<String>::None);
    let cycle_start_date = RwSignal::new(String::new());

    let format_date = move |date_str: String| -> String {
        if date_str.is_empty() {
            return "N/A".to_string();
        }
        #[cfg(feature = "hydrate")]
        {
            if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(&date_str) {
                let utc_timestamp = parsed.timestamp_millis();
                if let Some(window) = web_sys::window() {
                    let js_date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(utc_timestamp as f64));
                    let month = js_date.get_month() as u32;
                    let day = js_date.get_date() as u32;
                    let year = js_date.get_full_year() as u32;
                    let hours = js_date.get_hours() as u32;
                    let minutes = js_date.get_minutes() as u32;
                    let am_pm = if hours < 12 { "AM" } else { "PM" };
                    let display_hours = if hours == 0 { 12 } else if hours > 12 { hours - 12 } else { hours };
                    let month_names = ["January","February","March","April","May","June","July","August","September","October","November","December"];
                    format!("{} {}, {} at {}:{:02} {}", month_names[month as usize], day, year, display_hours, minutes, am_pm)
                } else {
                    parsed.format("%B %d, %Y at %I:%M %p").to_string()
                }
            } else {
                date_str
            }
        }
        #[cfg(not(feature = "hydrate"))]
        {
            if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(&date_str) {
                parsed.format("%B %d, %Y at %I:%M %p").to_string()
            } else {
                date_str
            }
        }
    };

    // Fetch monthly usage by summing sizes of all transfers added since billing cycle start
    let fetch_monthly_usage = move || {
        if monthly_usage_loading.get() { return; }
        monthly_usage_loading.set(true);
        monthly_usage_error.set(None);

        let user = user_data.get();
        if user.is_none() {
            monthly_usage_loading.set(false);
            return;
        }
        let premium_expires = user.unwrap().premium_expires_at.clone();

        spawn_local(async move {
            #[cfg(feature = "hydrate")]
            {
                if let Some(window) = web_sys::window() {
                    if let Ok(Some(storage)) = window.local_storage() {
                        if let Ok(Some(api_key)) = storage.get_item("api_key") {
                            if api_key.is_empty() {
                                monthly_usage_loading.set(false);
                                return;
                            }

                            // Compute billing cycle start: premium_expires_at - 30 days
                            let cycle_start_ts = if let Ok(expires) = chrono::DateTime::parse_from_rfc3339(&premium_expires) {
                                let start = expires - chrono::Duration::days(30);
                                cycle_start_date.set(start.format("%B %d, %Y").to_string());
                                start.timestamp()
                            } else {
                                // fallback: first day of current month
                                let now = js_sys::Date::new_0();
                                let year = now.get_full_year() as i32;
                                let month = now.get_month() as u32;
                                let start_str = format!("{}-{:02}-01T00:00:00Z", year, month + 1);
                                if let Ok(start) = chrono::DateTime::parse_from_rfc3339(&start_str) {
                                    cycle_start_date.set(start.format("%B %d, %Y").to_string());
                                    start.timestamp()
                                } else {
                                    monthly_usage_loading.set(false);
                                    return;
                                }
                            };

                            use crate::api::TorboxClient;
                            let client = TorboxClient::new(api_key);

                            let mut total_bytes: i64 = 0;

                            // Fetch torrents
                            if let Ok(resp) = client.get_torrent_list(None, Some(true), None, None).await {
                                if let Some(torrents) = resp.data {
                                    for t in &torrents {
                                        if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&t.created_at) {
                                            if created.timestamp() >= cycle_start_ts && t.size > 0 {
                                                total_bytes += t.size;
                                            }
                                        }
                                    }
                                }
                            }

                            // Fetch web downloads
                            if let Ok(resp) = client.get_web_download_list(None, Some(true), None, None).await {
                                if let Some(webdls) = resp.data {
                                    for w in &webdls {
                                        if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&w.created_at) {
                                            if created.timestamp() >= cycle_start_ts && w.size > 0 {
                                                total_bytes += w.size;
                                            }
                                        }
                                    }
                                }
                            }

                            // Fetch usenet downloads
                            if let Ok(resp) = client.get_usenet_download_list(None, Some(true), None, None).await {
                                if let Some(usnets) = resp.data {
                                    for u in &usnets {
                                        if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&u.created_at) {
                                            if created.timestamp() >= cycle_start_ts && u.size > 0 {
                                                total_bytes += u.size;
                                            }
                                        }
                                    }
                                }
                            }

                            monthly_downloaded.set(total_bytes);
                            monthly_usage_loading.set(false);
                        }
                    }
                }
            }
            #[cfg(not(feature = "hydrate"))]
            {
                monthly_usage_loading.set(false);
            }
        });
    };

    // Get plan threshold
    let plan_threshold_bytes = move || -> i64 {
        user_data.get().map(|u| match u.plan {
            0 => 5_497_558_138_880i64,   // 5 TB
            1 => 10_995_116_277_760i64,  // 10 TB
            2 => 32_985_348_833_280i64,  // 30 TB
            3 => 21_990_232_555_520i64,  // 20 TB
            _ => 10_995_116_277_760i64,
        }).unwrap_or(10_995_116_277_760i64)
    };

    let plan_threshold_label = move || -> &'static str {
        user_data.get().map(|u| match u.plan {
            0 => "5 TB",
            1 => "10 TB",
            2 => "30 TB",
            3 => "20 TB",
            _ => "10 TB",
        }).unwrap_or("10 TB")
    };

    let usage_percent = move || -> f64 {
        let threshold = plan_threshold_bytes();
        if threshold == 0 { return 0.0; }
        (monthly_downloaded.get() as f64 / threshold as f64 * 100.0).min(100.0)
    };

    let usage_bar_color = move || -> &'static str {
        let pct = usage_percent();
        if pct >= 90.0 { "var(--color-error, #ef4444)" }
        else if pct >= 70.0 { "#f59e0b" }
        else { "var(--accent-primary)" }
    };

    view! {
        <div class="flex flex-col items-center w-full mt-10 sm:mt-12">
            <Show when=move || user_loading.get()>
                <div class="flex items-center justify-center py-12">
                    <div class="flex items-center space-x-2" style="color: var(--text-secondary);">
                        <div class="w-4 h-4 border-2 border-t-transparent rounded-full animate-spin" style="border-color: var(--text-secondary);"></div>
                        <span>"Loading user data..."</span>
                    </div>
                </div>
            </Show>

            <Show when=move || !user_loading.get() && user_data.get().is_some()>
                <div class="w-full mx-auto">
                    <div class="grid grid-cols-1 md:grid-cols-2 gap-4 sm:gap-6">

                        // Basic Information
                        <div class="rounded-xl p-4 sm:p-6 border" style="background-color: var(--bg-card); border-color: var(--border-secondary);">
                            <h3 class="text-lg sm:text-xl font-semibold mb-3 sm:mb-4" style="color: var(--text-primary);">"Basic Information"</h3>
                            <div class="space-y-2 sm:space-y-2">
                                <div class="flex flex-col sm:flex-row sm:justify-between gap-1 sm:gap-0">
                                    <span class="text-sm sm:text-base" style="color: var(--text-secondary);">"User ID:"</span>
                                    <span class="text-sm sm:text-base break-all sm:break-normal" style="color: var(--text-primary);">{move || user_data.get().map(|u| u.id.to_string()).unwrap_or_default()}</span>
                                </div>
                                <div class="flex flex-col sm:flex-row sm:justify-between gap-1 sm:gap-0">
                                    <span class="text-sm sm:text-base" style="color: var(--text-secondary);">"Email:"</span>
                                    <span class="text-sm sm:text-base break-all sm:break-normal" style="color: var(--text-primary);">{move || user_data.get().map(|u| u.email.clone()).unwrap_or_default()}</span>
                                </div>
                                <div class="flex flex-col sm:flex-row sm:justify-between gap-1 sm:gap-0">
                                    <span class="text-sm sm:text-base" style="color: var(--text-secondary);">"Plan:"</span>
                                    <span class="text-sm sm:text-base" style="color: var(--text-primary);">{move || {
                                        user_data.get().map(|u| match u.plan {
                                            0 => "Free".to_string(),
                                            1 => "Essential".to_string(),
                                            2 => "Pro".to_string(),
                                            3 => "Standard".to_string(),
                                            _ => format!("Plan {}", u.plan),
                                        }).unwrap_or_default()
                                    }}</span>
                                </div>
                                <div class="flex flex-col sm:flex-row sm:justify-between gap-2 sm:gap-0 sm:items-center">
                                    <span class="text-sm sm:text-base" style="color: var(--text-secondary);">"Referral Code:"</span>
                                    <div class="flex items-center space-x-2">
                                        <span class="text-sm sm:text-base break-all sm:break-normal" style="color: var(--text-primary);">{move || user_data.get().map(|u| u.user_referral.clone()).unwrap_or_default()}</span>
                                        <button
                                            class="transition-colors p-1.5 sm:p-1 rounded-lg hover:bg-opacity-10 hover:bg-white"
                                            style="color: var(--text-secondary);"
                                            on:click=move |_| {
                                                #[cfg(target_arch = "wasm32")]
                                                {
                                                    if let Some(window) = web_sys::window() {
                                                        if let Some(referral_code) = user_data.get().map(|u| u.user_referral.clone()) {
                                                            let clipboard = window.navigator().clipboard();
                                                            let _ = clipboard.write_text(&referral_code);
                                                        }
                                                    }
                                                }
                                            }
                                            title="Copy referral code"
                                        >
                                            <svg class="w-4 h-4 sm:w-4 sm:h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"></path>
                                            </svg>
                                        </button>
                                    </div>
                                </div>
                            </div>
                        </div>

                        // Account Statistics
                        <div class="rounded-xl p-4 sm:p-6 border" style="background-color: var(--bg-card); border-color: var(--border-secondary);">
                            <h3 class="text-lg sm:text-xl font-semibold mb-3 sm:mb-4" style="color: var(--text-primary);">"Account Statistics"</h3>
                            <div class="space-y-2 sm:space-y-2">
                                <div class="flex flex-col sm:flex-row sm:justify-between gap-1 sm:gap-0">
                                    <span class="text-sm sm:text-base" style="color: var(--text-secondary);">"Total Downloads:"</span>
                                    <span class="text-sm sm:text-base" style="color: var(--text-primary);">{move || user_data.get().map(|u| u.total_downloaded.to_string()).unwrap_or_default()}</span>
                                </div>
                                <div class="flex flex-col sm:flex-row sm:justify-between gap-1 sm:gap-0">
                                    <span class="text-sm sm:text-base" style="color: var(--text-secondary);">"Torrents Downloaded:"</span>
                                    <span class="text-sm sm:text-base" style="color: var(--text-primary);">{move || user_data.get().map(|u| u.torrents_downloaded.to_string()).unwrap_or_default()}</span>
                                </div>
                                <div class="flex flex-col sm:flex-row sm:justify-between gap-1 sm:gap-0">
                                    <span class="text-sm sm:text-base" style="color: var(--text-secondary);">"Web Downloads:"</span>
                                    <span class="text-sm sm:text-base" style="color: var(--text-primary);">{move || user_data.get().map(|u| u.web_downloads_downloaded.to_string()).unwrap_or_default()}</span>
                                </div>
                                <div class="flex flex-col sm:flex-row sm:justify-between gap-1 sm:gap-0">
                                    <span class="text-sm sm:text-base" style="color: var(--text-secondary);">"Usenet Downloads:"</span>
                                    <span class="text-sm sm:text-base" style="color: var(--text-primary);">{move || user_data.get().map(|u| u.usenet_downloads_downloaded.to_string()).unwrap_or_default()}</span>
                                </div>
                                <div class="flex flex-col sm:flex-row sm:justify-between gap-1 sm:gap-0">
                                    <span class="text-sm sm:text-base" style="color: var(--text-secondary);">"Referrals:"</span>
                                    <span class="text-sm sm:text-base" style="color: var(--text-primary);">{move || user_data.get().map(|u| u.purchases_referred.to_string()).unwrap_or_default()}</span>
                                </div>
                            </div>
                        </div>

                        // All-Time Data Usage
                        <div class="rounded-xl p-4 sm:p-6 border" style="background-color: var(--bg-card); border-color: var(--border-secondary);">
                            <h3 class="text-lg sm:text-xl font-semibold mb-3 sm:mb-4" style="color: var(--text-primary);">"All-Time Data Usage"</h3>
                            <div class="space-y-2 sm:space-y-2">
                                <div class="flex flex-col sm:flex-row sm:justify-between gap-1 sm:gap-0">
                                    <span class="text-sm sm:text-base" style="color: var(--text-secondary);">"Downloaded:"</span>
                                    <span class="text-sm sm:text-base break-all sm:break-normal" style="color: var(--text-primary);">{move || {
                                        user_data.get().map(|u| format_bytes(u.total_bytes_downloaded)).unwrap_or_default()
                                    }}</span>
                                </div>
                                <div class="flex flex-col sm:flex-row sm:justify-between gap-1 sm:gap-0">
                                    <span class="text-sm sm:text-base" style="color: var(--text-secondary);">"Uploaded:"</span>
                                    <span class="text-sm sm:text-base break-all sm:break-normal" style="color: var(--text-primary);">{move || {
                                        user_data.get().map(|u| format_bytes(u.total_bytes_uploaded)).unwrap_or_default()
                                    }}</span>
                                </div>
                                <div class="flex flex-col sm:flex-row sm:justify-between gap-1 sm:gap-0">
                                    <span class="text-sm sm:text-base" style="color: var(--text-secondary);">"Upload/Download Ratio:"</span>
                                    <span class="text-sm sm:text-base" style="color: var(--text-primary);">{move || {
                                        user_data.get().map(|u| {
                                            let d = u.total_bytes_downloaded as f64;
                                            let up = u.total_bytes_uploaded as f64;
                                            if d == 0.0 { "N/A".to_string() } else { format!("{:.3}", up / d) }
                                        }).unwrap_or_default()
                                    }}</span>
                                </div>
                            </div>
                        </div>

                        // Monthly (Billing Cycle) Usage
                        <div class="rounded-xl p-4 sm:p-6 border" style="background-color: var(--bg-card); border-color: var(--border-secondary);">
                            <div class="flex items-center justify-between mb-3 sm:mb-4">
                                <h3 class="text-lg sm:text-xl font-semibold" style="color: var(--text-primary);">"Billing Cycle Usage"</h3>
                                <button
                                    class="px-3 py-1.5 text-xs font-medium rounded-lg transition-colors disabled:opacity-50"
                                    style="background-color: var(--accent-primary); color: white;"
                                    on:click=move |_| fetch_monthly_usage()
                                    disabled=move || monthly_usage_loading.get()
                                >
                                    {move || if monthly_usage_loading.get() { "Loading..." } else { "Calculate" }}
                                </button>
                            </div>

                            <Show when=move || monthly_usage_error.get().is_some()>
                                <div class="text-xs mb-3 p-2 rounded" style="background-color: rgba(239,68,68,0.1); color: #ef4444;">
                                    {move || monthly_usage_error.get().unwrap_or_default()}
                                </div>
                            </Show>

                            <div class="space-y-3">
                                <Show when=move || !cycle_start_date.get().is_empty()>
                                    <div class="flex flex-col sm:flex-row sm:justify-between gap-1 sm:gap-0">
                                        <span class="text-sm sm:text-base" style="color: var(--text-secondary);">"Cycle Start:"</span>
                                        <span class="text-sm sm:text-base" style="color: var(--text-primary);">{move || cycle_start_date.get()}</span>
                                    </div>
                                </Show>
                                <div class="flex flex-col sm:flex-row sm:justify-between gap-1 sm:gap-0">
                                    <span class="text-sm sm:text-base" style="color: var(--text-secondary);">"Used this cycle:"</span>
                                    <span class="text-sm sm:text-base font-semibold" style="color: var(--text-primary);">
                                        {move || if monthly_downloaded.get() == 0 && !monthly_usage_loading.get() && cycle_start_date.get().is_empty() {
                                            "Click Calculate →".to_string()
                                        } else {
                                            format_bytes(monthly_downloaded.get())
                                        }}
                                    </span>
                                </div>
                                <div class="flex flex-col sm:flex-row sm:justify-between gap-1 sm:gap-0">
                                    <span class="text-sm sm:text-base" style="color: var(--text-secondary);">"Threshold (minimum):"</span>
                                    <span class="text-sm sm:text-base" style="color: var(--text-primary);">{move || plan_threshold_label()}</span>
                                </div>

                                // Progress bar
                                <Show when=move || monthly_downloaded.get() > 0>
                                    <div>
                                        <div class="flex justify-between text-xs mb-1" style="color: var(--text-secondary);">
                                            <span>{move || format!("{:.1}% of minimum threshold used", usage_percent())}</span>
                                            <span style=move || format!("color: {};", usage_bar_color())>
                                                {move || {
                                                    let pct = usage_percent();
                                                    if pct >= 90.0 { "⚠ Near limit" }
                                                    else if pct >= 70.0 { "↑ Moderate" }
                                                    else { "✓ Safe" }
                                                }}
                                            </span>
                                        </div>
                                        <div class="w-full rounded-full h-2.5" style="background-color: var(--bg-tertiary);">
                                            <div
                                                class="h-2.5 rounded-full transition-all"
                                                style=move || format!("width: {}%; background-color: {};", usage_percent(), usage_bar_color())
                                            ></div>
                                        </div>
                                    </div>
                                </Show>

                                <p class="text-xs pt-1" style="color: var(--text-secondary); line-height: 1.5;">
                                    "Estimated from transfer sizes added since your billing cycle started. Cached downloads don't count toward TorBox's abuse threshold."
                                </p>
                            </div>
                        </div>

                        // Account Details
                        <div class="rounded-xl p-4 sm:p-6 border" style="background-color: var(--bg-card); border-color: var(--border-secondary);">
                            <h3 class="text-lg sm:text-xl font-semibold mb-3 sm:mb-4" style="color: var(--text-primary);">"Account Details"</h3>
                            <div class="space-y-2 sm:space-y-2">
                                <div class="flex flex-col sm:flex-row sm:justify-between gap-1 sm:gap-0">
                                    <span class="text-sm sm:text-base" style="color: var(--text-secondary);">"Created:"</span>
                                    <span class="text-sm sm:text-base break-all sm:break-normal" style="color: var(--text-primary);">{move || user_data.get().map(|u| format_date(u.created_at.clone())).unwrap_or_default()}</span>
                                </div>
                                <div class="flex flex-col sm:flex-row sm:justify-between gap-1 sm:gap-0">
                                    <span class="text-sm sm:text-base" style="color: var(--text-secondary);">"Premium Expires:"</span>
                                    <span class="text-sm sm:text-base break-all sm:break-normal" style="color: var(--text-primary);">{move || user_data.get().map(|u| format_date(u.premium_expires_at.clone())).unwrap_or_default()}</span>
                                </div>
                                <div class="flex flex-col sm:flex-row sm:justify-between gap-1 sm:gap-0">
                                    <span class="text-sm sm:text-base" style="color: var(--text-secondary);">"Vendor:"</span>
                                    <span class="text-sm sm:text-base" style="color: var(--text-primary);">{move || user_data.get().map(|u| if u.is_vendor { "Yes" } else { "No" }).unwrap_or_default()}</span>
                                </div>
                            </div>
                        </div>

                    </div>
                </div>
            </Show>

            <Show when=move || !user_loading.get() && user_data.get().is_none()>
                <div class="text-center py-12">
                    <div style="color: var(--text-secondary);">"Failed to load user data. Please check your API connection."</div>
                </div>
            </Show>

            <div class="w-full mx-auto mt-8 sm:mt-10">
                <div class="grid grid-cols-1 md:grid-cols-2 gap-4 sm:gap-6">
                    // Developer Section
                    <div class="rounded-xl p-4 sm:p-6 border" style="background-color: var(--bg-card); border-color: var(--border-secondary);">
                        <h3 class="text-lg sm:text-xl font-semibold mb-3 sm:mb-4" style="color: var(--text-primary);">"Developer"</h3>
                        <div class="space-y-3 sm:space-y-3">
                            <a
                                href="https://github.com/atharvkharbade/torbox-companion"
                                target="_blank"
                                rel="noopener noreferrer"
                                class="flex items-center space-x-2 text-sm sm:text-base transition-colors hover:opacity-80"
                                style="color: var(--text-primary);"
                            >
                                <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                                    <path fill-rule="evenodd" d="M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.531 1.032 1.531 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0112 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0022 12.017C22 6.484 17.522 2 12 2z" clip-rule="evenodd"/>
                                </svg>
                                <span>"GitHub (Fork)"</span>
                                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"></path>
                                </svg>
                            </a>
                            <div class="flex flex-col space-y-2 pt-2 border-t" style="border-color: var(--border-secondary);">
                                <div class="flex flex-col sm:flex-row sm:justify-between gap-1 sm:gap-0 sm:items-center">
                                    <span class="text-sm sm:text-base" style="color: var(--text-secondary);">"Referral Link:"</span>
                                    <div class="flex items-center space-x-2">
                                        <a
                                            href="https://torbox.app/subscription?referral=09c3f0f3-4e61-4634-a6dc-40af39f8165c"
                                            target="_blank"
                                            rel="noopener noreferrer"
                                            class="text-sm sm:text-base break-all sm:break-normal transition-colors hover:opacity-80"
                                            style="color: var(--accent-primary);"
                                        >
                                            "View Link"
                                        </a>
                                        <button
                                            class="transition-colors p-1.5 sm:p-1 rounded-lg hover:bg-opacity-10 hover:bg-white"
                                            style="color: var(--text-secondary);"
                                            on:click=move |_| {
                                                #[cfg(target_arch = "wasm32")]
                                                {
                                                    if let Some(window) = web_sys::window() {
                                                        let clipboard = window.navigator().clipboard();
                                                        let _ = clipboard.write_text("https://torbox.app/subscription?referral=09c3f0f3-4e61-4634-a6dc-40af39f8165c");
                                                    }
                                                }
                                            }
                                            title="Copy referral link"
                                        >
                                            <svg class="w-4 h-4 sm:w-4 sm:h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"></path>
                                            </svg>
                                        </button>
                                    </div>
                                </div>
                            </div>
                        </div>
                    </div>

                    // Docs & Links Section
                    <div class="rounded-xl p-4 sm:p-6 border" style="background-color: var(--bg-card); border-color: var(--border-secondary);">
                        <h3 class="text-lg sm:text-xl font-semibold mb-3 sm:mb-4" style="color: var(--text-primary);">"Documentation & Links"</h3>
                        <div class="space-y-3 sm:space-y-3">
                            <a
                                href="https://www.postman.com/torbox/torbox/overview"
                                target="_blank"
                                rel="noopener noreferrer"
                                class="flex items-center space-x-2 text-sm sm:text-base transition-colors hover:opacity-80"
                                style="color: var(--text-primary);"
                            >
                                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253"></path>
                                </svg>
                                <span>"API Documentation"</span>
                                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"></path>
                                </svg>
                            </a>
                            <a
                                href="https://torbox.app/settings"
                                target="_blank"
                                rel="noopener noreferrer"
                                class="flex items-center space-x-2 text-sm sm:text-base transition-colors hover:opacity-80"
                                style="color: var(--text-primary);"
                            >
                                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"></path>
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"></path>
                                </svg>
                                <span>"TorBox Settings"</span>
                                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"></path>
                                </svg>
                            </a>
                            <a
                                href="https://help.torbox.app/en/articles/10792754-the-torbox-abuse-system"
                                target="_blank"
                                rel="noopener noreferrer"
                                class="flex items-center space-x-2 text-sm sm:text-base transition-colors hover:opacity-80"
                                style="color: var(--text-primary);"
                            >
                                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"></path>
                                </svg>
                                <span>"TorBox Abuse Policy"</span>
                                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"></path>
                                </svg>
                            </a>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    }
}
